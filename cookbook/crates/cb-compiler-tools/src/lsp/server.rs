//! The protocol state machine: lifecycle, document store, dispatch, diagnostics.
//!
//! DESIGN
//! * [`Server::process`] takes a batch of already-received messages and returns every
//!   message to send. Nothing in it blocks or touches I/O, so tests drive it directly.
//! * DEBOUNCE: edits only mark a document dirty; diagnostics are computed once per
//!   batch, for the newest text ("latest version wins"). [`serve`] builds batches by
//!   draining whatever the reader thread has queued (plus an optional quiet period),
//!   so ten keystrokes arriving together cost one analysis, not ten.
//! * CANCELLATION: a `$/cancelRequest` that arrives in the same batch as its request
//!   makes the request answer with -32800 instead of running.
//! * ROBUSTNESS: every message is handled under `catch_unwind`; a panic becomes an
//!   InternalError response and bumps [`Server::internal_errors`] (tests assert 0).

use super::snapshot::{Snapshot, apply_change};
use super::workspace::{self, Workspace};
use super::{completion, features, nav, semtok};
use crate::analysis::diag::{Diagnostic, Severity};
use crate::fmt;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::io::{BufRead, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;
pub const SERVER_NOT_INITIALIZED: i64 = -32002;
pub const REQUEST_CANCELLED: i64 = -32800;
pub const REQUEST_FAILED: i64 = -32803;

#[derive(Debug)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

pub type RpcResult = Result<Value, RpcError>;

pub fn rpc_err(code: i64, message: impl Into<String>) -> RpcError {
    RpcError {
        code,
        message: message.into(),
    }
}

/// Which `textDocument/didChange` form the server advertises (it accepts both always).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncKind {
    Full,
    Incremental,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    Uninitialized,
    Running,
    ShutDown,
}

/// Analysis cache of one document: the snapshot of the newest (version, edit generation).
///
/// Keyed by BOTH the client's version and a server-side generation counter that moves on
/// every text change, so a client that reuses a version number for different text can
/// never be served a stale analysis, and repeated requests at one version cost one analysis.
struct Document {
    text: String,
    version: i64,
    generation: u64,
    cache: Option<((i64, u64), Rc<Snapshot>)>,
}

/// Semantic-token results remembered per document for `full/delta` (bounded).
const MAX_TOKEN_RESULTS: usize = 8;

pub struct Server {
    state: State,
    sync: SyncKind,
    docs: HashMap<String, Document>,
    dirty: BTreeSet<String>,
    exit_code: Option<i32>,
    /// Handlers that panicked (must stay 0).
    pub internal_errors: usize,
    /// How many analyses were run (for the "latest version wins" tests).
    pub analyses: usize,
    pub settings: Value,
    pub workspace: Workspace,
    token_results: HashMap<String, VecDeque<(String, Vec<u32>)>>,
    next_result_id: u64,
    next_request_id: u64,
    client_watches_files: bool,
}

impl Default for Server {
    fn default() -> Self {
        Server::new(SyncKind::Incremental)
    }
}

fn ok(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn notification(method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "method": method, "params": params})
}

impl Server {
    pub fn new(sync: SyncKind) -> Server {
        Server {
            state: State::Uninitialized,
            sync,
            docs: HashMap::new(),
            dirty: BTreeSet::new(),
            exit_code: None,
            internal_errors: 0,
            analyses: 0,
            settings: Value::Null,
            workspace: Workspace::default(),
            token_results: HashMap::new(),
            next_result_id: 0,
            next_request_id: 0,
            client_watches_files: false,
        }
    }

    /// `Some(code)` once an `exit` notification was processed: 0 after `shutdown`, else 1.
    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    pub fn document_text(&self, uri: &str) -> Option<&str> {
        self.docs.get(uri).map(|d| d.text.as_str())
    }

    pub fn document_version(&self, uri: &str) -> Option<i64> {
        self.docs.get(uri).map(|d| d.version)
    }

    /// Handle one message.
    pub fn handle(&mut self, msg: Value) -> Vec<Value> {
        self.process(vec![msg])
    }

    /// Handle raw frame bytes: malformed JSON answers with -32700 and id `null`.
    pub fn handle_bytes(&mut self, body: &[u8]) -> Vec<Value> {
        match serde_json::from_slice::<Value>(body) {
            Ok(v) => self.handle(v),
            Err(e) => vec![error(
                &Value::Null,
                PARSE_ERROR,
                &format!("parse error: {e}"),
            )],
        }
    }

    /// Handle a batch; diagnostics for documents edited anywhere in the batch are
    /// published once, at the end, for their latest text.
    pub fn process(&mut self, msgs: Vec<Value>) -> Vec<Value> {
        let cancelled: HashSet<String> = msgs
            .iter()
            .filter(|m| m.get("method").and_then(Value::as_str) == Some("$/cancelRequest"))
            .filter_map(|m| {
                m.get("params")
                    .and_then(|p| p.get("id"))
                    .map(Value::to_string)
            })
            .collect();
        let mut out = Vec::new();
        for msg in msgs {
            if self.exit_code.is_some() {
                break;
            }
            let r = catch_unwind(AssertUnwindSafe(|| self.dispatch(&msg, &cancelled)));
            match r {
                Ok(mut v) => out.append(&mut v),
                Err(_) => {
                    self.internal_errors += 1;
                    if let Some(id) = msg.get("id").filter(|_| msg.get("method").is_some()) {
                        out.push(error(id, INTERNAL_ERROR, "internal error"));
                    }
                }
            }
        }
        let r = catch_unwind(AssertUnwindSafe(|| self.flush_diagnostics()));
        match r {
            Ok(mut v) => out.append(&mut v),
            Err(_) => self.internal_errors += 1,
        }
        out
    }

    fn dispatch(&mut self, msg: &Value, cancelled: &HashSet<String>) -> Vec<Value> {
        let Some(obj) = msg.as_object() else {
            return vec![error(
                &Value::Null,
                INVALID_REQUEST,
                "request must be a JSON object",
            )];
        };
        let id = obj.get("id");
        let Some(method) = obj.get("method").and_then(Value::as_str) else {
            // A response from the client (to a request we never send) or garbage.
            return if id.is_some() && (obj.contains_key("result") || obj.contains_key("error")) {
                vec![]
            } else {
                vec![error(
                    id.unwrap_or(&Value::Null),
                    INVALID_REQUEST,
                    "missing `method`",
                )]
            };
        };
        let params = obj.get("params").unwrap_or(&Value::Null);
        match id {
            Some(id) => {
                if cancelled.contains(&id.to_string()) {
                    return vec![error(id, REQUEST_CANCELLED, "request cancelled")];
                }
                vec![match self.request(method, params) {
                    Ok(v) => ok(id, v),
                    Err(e) => error(id, e.code, &e.message),
                }]
            }
            None => self.notify(method, params),
        }
    }

    // ------------------------------------------------------------ lifecycle

    fn request(&mut self, method: &str, p: &Value) -> RpcResult {
        match (self.state, method) {
            (State::Uninitialized, "initialize") => {
                self.state = State::Running;
                self.workspace.folders = workspace::folders_from_initialize(p);
                self.client_watches_files = p
                    .pointer("/capabilities/workspace/didChangeWatchedFiles/dynamicRegistration")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                return Ok(self.initialize_result());
            }
            (State::Uninitialized, _) => {
                return Err(rpc_err(SERVER_NOT_INITIALIZED, "server not initialized"));
            }
            (State::Running, "initialize") => {
                return Err(rpc_err(INVALID_REQUEST, "already initialized"));
            }
            (State::ShutDown, _) => return Err(rpc_err(INVALID_REQUEST, "server is shut down")),
            (State::Running, "shutdown") => {
                self.state = State::ShutDown;
                return Ok(Value::Null);
            }
            _ => {}
        }
        match method {
            "textDocument/formatting"
            | "textDocument/rangeFormatting"
            | "textDocument/onTypeFormatting" => self.formatting(method, p),
            "workspace/symbol" => {
                Ok(self.workspace_symbols(p.get("query").and_then(Value::as_str).unwrap_or("")))
            }
            "textDocument/semanticTokens/full"
            | "textDocument/semanticTokens/full/delta"
            | "textDocument/semanticTokens/range" => self.semantic_tokens(method, p),
            "completionItem/resolve" => {
                Ok(completion::resolve(p, &mut |uri| self.snapshot_of(uri)))
            }
            _ => {
                let uri = p
                    .get("textDocument")
                    .or_else(|| p.get("item"))
                    .and_then(|t| t.get("uri"))
                    .and_then(Value::as_str);
                let known = matches!(
                    method,
                    "textDocument/hover"
                        | "textDocument/definition"
                        | "textDocument/references"
                        | "textDocument/prepareRename"
                        | "textDocument/rename"
                        | "textDocument/documentSymbol"
                        | "textDocument/completion"
                        | "textDocument/signatureHelp"
                        | "textDocument/codeAction"
                        | "textDocument/documentHighlight"
                        | "textDocument/selectionRange"
                        | "textDocument/typeDefinition"
                        | "textDocument/prepareCallHierarchy"
                        | "callHierarchy/incomingCalls"
                        | "callHierarchy/outgoingCalls"
                        | "textDocument/inlayHint"
                        | "textDocument/foldingRange"
                );
                if !known {
                    return Err(rpc_err(
                        METHOD_NOT_FOUND,
                        format!("method not found: {method}"),
                    ));
                }
                let uri = uri.ok_or_else(|| rpc_err(INVALID_PARAMS, "missing textDocument.uri"))?;
                let snap = self
                    .snapshot_of(uri)
                    .ok_or_else(|| rpc_err(REQUEST_FAILED, format!("document not open: {uri}")))?;
                Self::feature(method, &snap, p)
            }
        }
    }

    fn feature(method: &str, s: &Snapshot, p: &Value) -> RpcResult {
        Ok(match method {
            "textDocument/hover" => features::hover(s, p),
            "textDocument/definition" => features::definition(s, p),
            "textDocument/references" => features::references(s, p),
            "textDocument/prepareRename" => return features::prepare_rename(s, p),
            "textDocument/documentHighlight" => nav::document_highlight(s, p),
            "textDocument/selectionRange" => nav::selection_ranges(s, p),
            "textDocument/typeDefinition" => nav::type_definition(s, p),
            "textDocument/prepareCallHierarchy" => nav::prepare_call_hierarchy(s, p),
            "callHierarchy/incomingCalls" => return nav::incoming_calls(s, p),
            "callHierarchy/outgoingCalls" => return nav::outgoing_calls(s, p),
            "textDocument/rename" => return features::rename(s, p),
            "textDocument/documentSymbol" => features::document_symbols(s),
            "textDocument/completion" => completion::complete(s, p),
            "textDocument/signatureHelp" => features::signature_help(s, p),
            "textDocument/codeAction" => features::code_actions(s, p),
            "textDocument/inlayHint" => features::inlay_hints(s, p),
            "textDocument/foldingRange" => features::folding_ranges(s),
            _ => {
                return Err(rpc_err(
                    METHOD_NOT_FOUND,
                    format!("method not found: {method}"),
                ));
            }
        })
    }

    fn initialize_result(&self) -> Value {
        json!({
            "capabilities": {
                "positionEncoding": "utf-16",
                "textDocumentSync": {"openClose": true, "change": if self.sync == SyncKind::Full { 1 } else { 2 }, "save": {"includeText": false}},
                "hoverProvider": true,
                "definitionProvider": true,
                "referencesProvider": true,
                "renameProvider": {"prepareProvider": true},
                "documentSymbolProvider": true,
                "completionProvider": {"triggerCharacters": [".", ":"], "resolveProvider": true},
                "signatureHelpProvider": {"triggerCharacters": ["(", ","]},
                "semanticTokensProvider": {"legend": semtok::legend(), "full": {"delta": true}, "range": true},
                "documentFormattingProvider": true,
                "documentRangeFormattingProvider": true,
                "documentOnTypeFormattingProvider": {"firstTriggerCharacter": "}", "moreTriggerCharacter": [";"]},
                "workspaceSymbolProvider": true,
                "documentHighlightProvider": true,
                "selectionRangeProvider": true,
                "typeDefinitionProvider": true,
                "callHierarchyProvider": true,
                "codeActionProvider": {"codeActionKinds": ["quickfix"]},
                "inlayHintProvider": true,
                "foldingRangeProvider": true,
                "workspace": {"workspaceFolders": {"supported": true, "changeNotifications": true}}
            },
            "serverInfo": {"name": "pebble-tools", "version": env!("CARGO_PKG_VERSION")}
        })
    }

    fn notify(&mut self, method: &str, p: &Value) -> Vec<Value> {
        if method == "exit" {
            self.exit_code = Some(if self.state == State::ShutDown { 0 } else { 1 });
            return vec![];
        }
        if self.state != State::Running {
            return vec![]; // notifications before `initialize` (or after `shutdown`) are dropped
        }
        let td = p.get("textDocument");
        let uri = td.and_then(|t| t.get("uri")).and_then(Value::as_str);
        match (method, uri) {
            ("textDocument/didOpen", Some(uri)) => {
                let text = td
                    .and_then(|t| t.get("text"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let version = td
                    .and_then(|t| t.get("version"))
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                self.docs.insert(
                    uri.to_string(),
                    Document {
                        text: text.to_string(),
                        version,
                        generation: 0,
                        cache: None,
                    },
                );
                self.dirty.insert(uri.to_string());
            }
            ("textDocument/didChange", Some(uri)) => self.did_change(uri, p),
            ("textDocument/didClose", Some(uri)) => {
                self.docs.remove(uri);
                self.token_results.remove(uri);
                self.dirty.insert(uri.to_string());
            }
            ("workspace/didChangeConfiguration", _) => {
                // Tolerated, whatever shape the client sends; kept for future options.
                if let Some(s) = p.get("settings") {
                    self.settings = s.clone();
                }
            }
            ("initialized", _) => {
                self.workspace.rescan();
                if self.client_watches_files {
                    self.next_request_id += 1;
                    return vec![json!({
                        "jsonrpc": "2.0", "id": format!("pebble-register-{}", self.next_request_id), "method": "client/registerCapability",
                        "params": {"registrations": [{"id": "pebble-watch-peb", "method": "workspace/didChangeWatchedFiles",
                            "registerOptions": {"watchers": [{"globPattern": "**/*.peb"}]}}]}
                    })];
                }
            }
            ("workspace/didChangeWatchedFiles", _) => {
                for c in p
                    .get("changes")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if let (Some(u), Some(t)) = (
                        c.get("uri").and_then(Value::as_str),
                        c.get("type").and_then(Value::as_i64),
                    ) {
                        self.workspace.file_event(u, t);
                    }
                }
            }
            ("workspace/didChangeWorkspaceFolders", _) => {
                let uris = |k: &str| -> Vec<std::path::PathBuf> {
                    p.pointer(&format!("/event/{k}"))
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|f| {
                            f.get("uri")
                                .and_then(Value::as_str)
                                .and_then(workspace::uri_to_path)
                        })
                        .collect()
                };
                for f in uris("removed") {
                    self.workspace.remove_folder(&f);
                }
                for f in uris("added") {
                    self.workspace.add_folder(f);
                }
            }
            _ => {} // didSave, $/setTrace, $/cancelRequest (handled up front), unknown: ignore
        }
        vec![]
    }

    fn did_change(&mut self, uri: &str, p: &Value) {
        let version = p
            .get("textDocument")
            .and_then(|t| t.get("version"))
            .and_then(Value::as_i64);
        let Some(doc) = self.docs.get_mut(uri) else {
            return;
        };
        if let Some(v) = version {
            if v < doc.version {
                return; // stale notification that overtook a newer one
            }
            doc.version = v;
        }
        for change in p
            .get("contentChanges")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            apply_change(&mut doc.text, change);
        }
        doc.generation += 1;
        doc.cache = None;
        self.dirty.insert(uri.to_string());
    }

    // ------------------------------------------------------------ analysis cache

    /// The analysed current text of `uri` (computed at most once per edit).
    fn snapshot_of(&mut self, uri: &str) -> Option<Rc<Snapshot>> {
        let doc = self.docs.get_mut(uri)?;
        let key = (doc.version, doc.generation);
        if doc.cache.as_ref().is_none_or(|(k, _)| *k != key) {
            self.analyses += 1;
            doc.cache = Some((key, Rc::new(Snapshot::new(uri, doc.version, &doc.text))));
        }
        doc.cache.as_ref().map(|(_, s)| s.clone())
    }

    fn flush_diagnostics(&mut self) -> Vec<Value> {
        let dirty = std::mem::take(&mut self.dirty);
        let mut out = Vec::new();
        for uri in dirty {
            match self.snapshot_of(&uri) {
                Some(s) => out.push(notification(
                    "textDocument/publishDiagnostics",
                    json!({"uri": uri, "version": s.version, "diagnostics": diagnostics_json(&s)}),
                )),
                None => out.push(notification(
                    "textDocument/publishDiagnostics",
                    json!({"uri": uri, "diagnostics": []}),
                )),
            }
        }
        out
    }

    fn formatting(&mut self, method: &str, p: &Value) -> RpcResult {
        let uri = p
            .get("textDocument")
            .and_then(|t| t.get("uri"))
            .and_then(Value::as_str)
            .ok_or_else(|| rpc_err(INVALID_PARAMS, "missing textDocument.uri"))?;
        let s = self
            .snapshot_of(uri)
            .ok_or_else(|| rpc_err(REQUEST_FAILED, "document not open"))?;
        let edits_of = |reps: Vec<fmt::Replacement>| -> Value {
            Value::Array(
                reps.into_iter()
                    .map(|r| {
                        s.text_edit(crate::analysis::source::Span::new(r.start, r.end), &r.text)
                    })
                    .collect(),
            )
        };
        match method {
            "textDocument/formatting" => match fmt::format(&s.text) {
                Ok(formatted) => {
                    let Some((a, b, new)) = fmt::minimal_edit(&s.text, &formatted) else {
                        return Ok(json!([]));
                    };
                    Ok(json!([s.text_edit(
                        crate::analysis::source::Span::new(a, b),
                        &new
                    )]))
                }
                // The file does not parse: format the error-free items, keep the rest verbatim.
                Err(fmt::FormatError::Syntax(_)) => Ok(edits_of(fmt::format_items(&s.text, None))),
                Err(e @ fmt::FormatError::Internal(_)) => {
                    Err(rpc_err(REQUEST_FAILED, e.to_string()))
                }
            },
            "textDocument/rangeFormatting" => {
                let want = s.span_of_range(&p["range"]);
                Ok(edits_of(fmt::format_range(
                    &s.text,
                    want.start as usize,
                    want.end as usize,
                )))
            }
            _ => {
                let ch = p
                    .get("ch")
                    .and_then(Value::as_str)
                    .and_then(|c| c.chars().next())
                    .unwrap_or(' ');
                let off = s.offset(&p["position"]);
                Ok(edits_of(fmt::format_on_type(&s.text, off, ch)))
            }
        }
    }

    fn workspace_symbols(&mut self, query: &str) -> Value {
        let mut sets: Vec<Rc<Vec<workspace::WsSymbol>>> = Vec::new();
        let open: Vec<String> = self.docs.keys().cloned().collect();
        for uri in &open {
            if let Some(s) = self.snapshot_of(uri) {
                sets.push(Rc::new(workspace::symbols_of(&s)));
            }
        }
        let docs = &self.docs;
        sets.extend(self.workspace.closed_symbols(&|u| docs.contains_key(u)));
        workspace::query(query, sets)
    }

    fn remember_tokens(&mut self, uri: &str, data: &[u32]) -> String {
        self.next_result_id += 1;
        let id = self.next_result_id.to_string();
        let q = self.token_results.entry(uri.to_string()).or_default();
        q.push_back((id.clone(), data.to_vec()));
        while q.len() > MAX_TOKEN_RESULTS {
            q.pop_front();
        }
        id
    }

    fn semantic_tokens(&mut self, method: &str, p: &Value) -> RpcResult {
        let uri = p
            .get("textDocument")
            .and_then(|t| t.get("uri"))
            .and_then(Value::as_str)
            .ok_or_else(|| rpc_err(INVALID_PARAMS, "missing textDocument.uri"))?;
        let s = self
            .snapshot_of(uri)
            .ok_or_else(|| rpc_err(REQUEST_FAILED, format!("document not open: {uri}")))?;
        if method.ends_with("/range") {
            let at = |v: &Value| {
                let lc = |k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0) as usize;
                (lc("line"), lc("character"))
            };
            return Ok(
                json!({"data": semtok::range_data(&s, at(&p["range"]["start"]), at(&p["range"]["end"]))}),
            );
        }
        let data = semtok::full_data(&s);
        let previous = p
            .get("previousResultId")
            .and_then(Value::as_str)
            .and_then(|prev| {
                self.token_results
                    .get(uri)?
                    .iter()
                    .find(|(id, _)| id == prev)
                    .map(|(_, d)| d.clone())
            });
        let id = self.remember_tokens(uri, &data);
        if method.ends_with("/delta")
            && let Some(old) = previous
        {
            let edits = match semtok::diff(&old, &data) {
                Some((start, delete, new)) => {
                    json!([{"start": start, "deleteCount": delete, "data": new}])
                }
                None => json!([]),
            };
            return Ok(json!({"resultId": id, "edits": edits}));
        }
        Ok(json!({"resultId": id, "data": data}))
    }
}

pub fn diagnostics_json(s: &Snapshot) -> Value {
    Value::Array(
        s.an.diagnostics
            .iter()
            .map(|d| diagnostic_json(s, d))
            .collect(),
    )
}

pub fn diagnostic_json(s: &Snapshot, d: &Diagnostic) -> Value {
    let severity = match d.severity {
        Severity::Error => 1,
        Severity::Warning => 2,
        Severity::Note => 3,
        Severity::Help => 4,
    };
    let span = d.primary_span().unwrap_or_default();
    let mut message = d.message.clone();
    for n in &d.notes {
        message.push_str(&format!("\nnote: {n}"));
    }
    for h in &d.help {
        message.push_str(&format!("\nhelp: {h}"));
    }
    let related: Vec<Value> = d
        .labels
        .iter()
        .filter(|l| !l.primary && !l.text.is_empty())
        .map(|l| json!({"location": s.location(l.span), "message": l.text}))
        .collect();
    let mut tags = vec![];
    if matches!(
        d.code,
        "W0501" | "W0502" | "W0503" | "W0504" | "W0507" | "W0508"
    ) {
        tags.push(1); // Unnecessary: editors render the range faded
    }
    let mut v = json!({"range": s.range(span), "severity": severity, "code": d.code, "source": "pebble", "message": message});
    if !related.is_empty() {
        v["relatedInformation"] = Value::Array(related);
    }
    if !tags.is_empty() {
        v["tags"] = json!(tags);
    }
    v
}

// ------------------------------------------------------------------ stdio driver

/// Run the server over `input`/`output` until `exit` or end of input; returns the exit code.
///
/// A reader thread parses frames and queues them; the main loop takes everything
/// queued as one batch (after waiting `debounce` for stragglers when the batch is
/// only edits), which is how rapid edits coalesce.
pub fn serve<R: BufRead + Send + 'static, W: Write>(
    server: &mut Server,
    input: R,
    output: &mut W,
    debounce: Duration,
) -> i32 {
    let (tx, rx) = mpsc::channel::<Result<Vec<u8>, std::io::Error>>();
    std::thread::spawn(move || {
        let mut input = input;
        loop {
            match super::transport::read_message(&mut input) {
                Ok(Some(body)) => {
                    if tx.send(Ok(body)).is_err() {
                        return;
                    }
                }
                Ok(None) => return,
                Err(e) => {
                    let _ = tx.send(Err(e));
                    return;
                }
            }
        }
    });
    loop {
        let first = match rx.recv() {
            Ok(Ok(b)) => b,
            // End of input or a framing error without `exit`: behave like `exit` (code 1).
            _ => return server.exit_code().unwrap_or(1),
        };
        let mut bodies = vec![first];
        loop {
            while let Ok(Ok(b)) = rx.try_recv() {
                bodies.push(b);
            }
            if debounce.is_zero() || !only_edits(&bodies) {
                break;
            }
            match rx.recv_timeout(debounce) {
                Ok(Ok(b)) => bodies.push(b),
                _ => break,
            }
        }
        let mut msgs = Vec::new();
        let mut out = Vec::new();
        for b in &bodies {
            match serde_json::from_slice::<Value>(b) {
                Ok(v) => msgs.push(v),
                Err(e) => out.push(error(
                    &Value::Null,
                    PARSE_ERROR,
                    &format!("parse error: {e}"),
                )),
            }
        }
        out.extend(server.process(msgs));
        for m in &out {
            if super::transport::write_message(output, m).is_err() {
                return server.exit_code().unwrap_or(1);
            }
        }
        if let Some(code) = server.exit_code() {
            return code;
        }
    }
}

fn only_edits(bodies: &[Vec<u8>]) -> bool {
    bodies.iter().all(|b| {
        let s = String::from_utf8_lossy(b);
        s.contains("\"textDocument/didChange\"") || s.contains("\"textDocument/didOpen\"")
    })
}
