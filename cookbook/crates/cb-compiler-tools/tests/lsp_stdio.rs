//! A REAL stdio test: the built `pebble-tools lsp` binary is spawned as a child process and
//! spoken to with `Content-Length` framing, exactly as an editor would.
//!
//! Hardening: every read has a timeout and the child is killed when a test fails or hangs
//! (`Drop`), writes happen on a separate thread so a stuck server can never block the test,
//! and the exit code is observed with a bounded wait.

mod common;
use common::Rng;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_pebble-tools");
const T: Duration = Duration::from_secs(30);
const URI: &str = "file:///stdio/t.peb";

const DOC: &str = "/// Adds two numbers.\nfn add(a: int, b: int) -> int { a + b }\nfn main() {\n    let r: int = add(1, 2);\n    let bad: int = \"x\";\n    println(to_str(r));\n}\n";

/// Frame a JSON message.
fn frame(v: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(v).unwrap();
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend(body);
    out
}

struct Lsp {
    child: Arc<Mutex<Child>>,
    tx: Option<Sender<Vec<u8>>>,
    rx: Receiver<Result<Value, String>>,
    notes: Vec<Value>,
    next_id: i64,
}

impl Lsp {
    fn start() -> Lsp {
        let mut child = Command::new(BIN)
            .arg("lsp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn pebble-tools");
        let mut stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, wr) = mpsc::channel::<Vec<u8>>();
        // Writer thread: the test never blocks on a full pipe.
        std::thread::spawn(move || {
            for chunk in wr {
                if stdin.write_all(&chunk).and_then(|_| stdin.flush()).is_err() {
                    return;
                }
            }
        });
        let (otx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut r = BufReader::new(stdout);
            loop {
                let mut len = None;
                loop {
                    let mut line = String::new();
                    match r.read_line(&mut line) {
                        Ok(0) | Err(_) => {
                            let _ = otx.send(Err("eof".into()));
                            return;
                        }
                        Ok(_) => {}
                    }
                    let line = line.trim_end();
                    if line.is_empty() {
                        break;
                    }
                    if let Some(v) = line.strip_prefix("Content-Length:") {
                        len = v.trim().parse::<usize>().ok();
                    }
                }
                let Some(len) = len else {
                    let _ = otx.send(Err("frame without Content-Length from server".into()));
                    return;
                };
                let mut body = vec![0u8; len];
                if r.read_exact(&mut body).is_err() {
                    let _ = otx.send(Err("truncated frame".into()));
                    return;
                }
                let _ = otx.send(serde_json::from_slice(&body).map_err(|e| e.to_string()));
            }
        });
        Lsp {
            child: Arc::new(Mutex::new(child)),
            tx: Some(tx),
            rx,
            notes: vec![],
            next_id: 1,
        }
    }

    fn send_bytes(&self, b: Vec<u8>) {
        self.tx.as_ref().unwrap().send(b).unwrap();
    }

    fn send(&self, v: &Value) {
        self.send_bytes(frame(v));
    }

    fn notify(&self, method: &str, params: Value) {
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": params}));
    }

    fn request_msg(&mut self, method: &str, params: Value) -> (i64, Value) {
        let id = self.next_id;
        self.next_id += 1;
        (
            id,
            json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
        )
    }

    /// Wait for the next message (any), failing (and killing the child) after `T`.
    fn next(&mut self) -> Value {
        match self.rx.recv_timeout(T) {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => panic!("server output ended: {e}"),
            Err(_) => panic!("timed out waiting for the server (killed in Drop)"),
        }
    }

    /// Wait for the response to `id`, collecting notifications on the way.
    fn wait_response(&mut self, id: i64) -> Value {
        loop {
            let m = self.next();
            if m.get("id") == Some(&json!(id)) && m.get("method").is_none() {
                return m;
            }
            self.notes.push(m);
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let (id, msg) = self.request_msg(method, params);
        self.send(&msg);
        self.wait_response(id)
    }

    fn wait_note(&mut self, method: &str, uri: &str) -> Value {
        if let Some(i) = self
            .notes
            .iter()
            .position(|m| m["method"] == method && m["params"]["uri"] == uri)
        {
            return self.notes.remove(i);
        }
        loop {
            let m = self.next();
            if m["method"] == method && m["params"]["uri"] == uri {
                return m;
            }
            self.notes.push(m);
        }
    }

    fn initialize(&mut self) -> Value {
        let r = self.request(
            "initialize",
            json!({"processId": null, "rootUri": null, "capabilities": {}}),
        );
        self.notify("initialized", json!({}));
        r
    }

    fn open(&self, uri: &str, text: &str) {
        self.notify("textDocument/didOpen", json!({"textDocument": {"uri": uri, "languageId": "pebble", "version": 1, "text": text}}));
    }

    /// Wait for the process to exit; its code.
    fn wait_exit(&mut self) -> Option<i32> {
        let end = Instant::now() + T;
        while Instant::now() < end {
            if let Some(st) = self.child.lock().unwrap().try_wait().unwrap() {
                return st.code();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the server did not exit within {T:?}");
    }

    fn close_stdin(&mut self) {
        self.tx = None;
    }
}

impl Drop for Lsp {
    fn drop(&mut self) {
        let mut c = self.child.lock().unwrap();
        let _ = c.kill();
        let _ = c.wait();
    }
}

fn position_of(text: &str, needle: &str, nth: usize) -> Value {
    let off = text
        .match_indices(needle)
        .nth(nth)
        .unwrap_or_else(|| panic!("{needle} not found"))
        .0;
    let (l, c) = common::lsp_position(text, off);
    json!({"line": l, "character": c})
}

/// One full scripted session; returns the interesting results (compared across framings).
fn session(l: &mut Lsp) -> Value {
    let init = l.initialize();
    assert!(
        init["result"]["capabilities"]["hoverProvider"]
            .as_bool()
            .unwrap(),
        "{init}"
    );
    l.open(URI, DOC);
    let diag = l.wait_note("textDocument/publishDiagnostics", URI);
    let ds = diag["params"]["diagnostics"].as_array().unwrap();
    assert!(
        ds.iter()
            .any(|d| d["severity"] == 1 && d["code"] == "E0301"),
        "expected a type error diagnostic: {diag}"
    );
    let td = json!({"uri": URI});
    let hover = l.request(
        "textDocument/hover",
        json!({"textDocument": td, "position": position_of(DOC, "add(1", 0)}),
    );
    assert!(
        hover["result"]["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("fn add(a: int, b: int) -> int"),
        "{hover}"
    );
    let comp = l.request(
        "textDocument/completion",
        json!({"textDocument": td, "position": position_of(DOC, "println", 0)}),
    );
    let labels: Vec<&str> = comp["result"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["label"].as_str().unwrap())
        .collect();
    assert!(
        labels.contains(&"add") && labels.contains(&"r"),
        "{labels:?}"
    );
    let rename = l.request(
        "textDocument/rename",
        json!({"textDocument": td, "position": position_of(DOC, "add(1", 0), "newName": "plus"}),
    );
    let edits = &rename["result"]["changes"][URI];
    assert_eq!(edits.as_array().unwrap().len(), 2, "{rename}");
    let renamed = common::apply_lsp_edits(DOC, edits);
    assert!(renamed.contains("fn plus(") && renamed.contains("plus(1, 2)"));
    // Formatting of an unformatted document.
    let ugly = "fn main(){let x=1;   println(to_str(x));}\n";
    let u2 = "file:///stdio/ugly.peb";
    l.open(u2, ugly);
    let fmt = l.request(
        "textDocument/formatting",
        json!({"textDocument": {"uri": u2}, "options": {"tabSize": 4, "insertSpaces": true}}),
    );
    let formatted = common::apply_lsp_edits(ugly, &fmt["result"]);
    assert_eq!(formatted, pebble_tools::fmt::format(ugly).unwrap());
    let symbols = l.request("textDocument/documentSymbol", json!({"textDocument": td}));
    assert_eq!(symbols["result"].as_array().unwrap().len(), 2);
    let sh = l.request("shutdown", Value::Null);
    assert_eq!(sh["result"], Value::Null);
    l.notify("exit", Value::Null);
    json!({"diag_count": ds.len(), "hover": hover["result"], "labels": labels, "rename": edits, "formatted": formatted})
}

#[test]
fn full_session_over_real_stdio_exits_zero() {
    let mut l = Lsp::start();
    session(&mut l);
    assert_eq!(l.wait_exit(), Some(0), "exit after shutdown must be 0");
}

#[test]
fn exit_without_shutdown_is_code_one_and_eof_does_not_hang() {
    let mut l = Lsp::start();
    l.initialize();
    l.notify("exit", Value::Null);
    assert_eq!(l.wait_exit(), Some(1));
    let mut l = Lsp::start();
    l.initialize();
    l.close_stdin(); // the editor died: stdin closes without `exit`
    assert_eq!(l.wait_exit(), Some(1));
}

/// Collect the exact bytes of a whole session so it can be replayed with any chunking.
fn session_bytes() -> Vec<u8> {
    let td = json!({"uri": URI});
    let msgs = vec![
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"capabilities": {}}}),
        json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}),
        json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {"textDocument": {"uri": URI, "languageId": "pebble", "version": 1, "text": DOC}}}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "textDocument/hover", "params": {"textDocument": td, "position": position_of(DOC, "add(1", 0)}}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "textDocument/documentSymbol", "params": {"textDocument": td}}),
        json!({"jsonrpc": "2.0", "id": 4, "method": "shutdown"}),
        json!({"jsonrpc": "2.0", "method": "exit"}),
    ];
    msgs.iter().flat_map(frame).collect()
}

/// Run `bytes` through the server as `chunks` and return (responses by id, exit code).
fn replay(chunks: Vec<Vec<u8>>) -> (Vec<Value>, Option<i32>) {
    let mut l = Lsp::start();
    for c in chunks {
        l.send_bytes(c);
    }
    let mut responses: Vec<Value> = Vec::new();
    let mut diag = false;
    while responses.len() < 4 || !diag {
        let m = l.next();
        if m["method"] == "textDocument/publishDiagnostics" {
            diag = true;
        } else if m.get("id").is_some() {
            responses.push(m);
        }
    }
    responses.sort_by_key(|m| m["id"].as_i64());
    (responses, l.wait_exit())
}

#[test]
fn partial_reads_at_arbitrary_byte_boundaries_and_coalesced_writes() {
    let bytes = session_bytes();
    let (baseline, code) = replay(vec![bytes.clone()]);
    assert_eq!(code, Some(0));
    assert_eq!(baseline.len(), 4);
    // Byte by byte: every possible split point (inside headers, bodies, and between frames).
    let (r, code) = replay(bytes.iter().map(|b| vec![*b]).collect());
    assert_eq!(
        (r, code),
        (baseline.clone(), Some(0)),
        "byte-at-a-time framing"
    );
    // Random chunk sizes, several seeds (chunks of 1..40 bytes, sometimes huge and covering several frames).
    for seed in 0..12u64 {
        let mut rng = Rng::new(seed + 1);
        let mut chunks = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            let n = if rng.chance(10) {
                1 + rng.below(bytes.len())
            } else {
                1 + rng.below(40)
            };
            let end = (at + n).min(bytes.len());
            chunks.push(bytes[at..end].to_vec());
            at = end;
        }
        let (r, code) = replay(chunks);
        assert_eq!((r, code), (baseline.clone(), Some(0)), "seed {seed}");
    }
    // Split exactly inside the header name, and inside the `\r\n\r\n` separator.
    let first = frame(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}));
    for cut in [
        3,
        15,
        17,
        first.iter().position(|b| *b == b'{').unwrap() - 2,
        first.iter().position(|b| *b == b'{').unwrap() - 1,
    ] {
        let mut l = Lsp::start();
        l.send_bytes(first[..cut].to_vec());
        std::thread::sleep(Duration::from_millis(20)); // let the server block mid-header
        l.send_bytes(first[cut..].to_vec());
        let r = l.wait_response(1);
        assert!(r["result"]["capabilities"].is_object(), "cut at {cut}: {r}");
    }
}

#[test]
fn two_messages_in_one_write_both_answered_in_order() {
    let mut l = Lsp::start();
    l.initialize();
    l.open(URI, DOC);
    let (a, ma) = l.request_msg(
        "textDocument/hover",
        json!({"textDocument": {"uri": URI}, "position": position_of(DOC, "add(1", 0)}),
    );
    let (b, mb) = l.request_msg(
        "textDocument/documentSymbol",
        json!({"textDocument": {"uri": URI}}),
    );
    let mut both = frame(&ma);
    both.extend(frame(&mb));
    l.send_bytes(both);
    let ra = l.wait_response(a);
    let rb = l.wait_response(b);
    assert!(ra["result"]["contents"].is_object());
    assert!(rb["result"].is_array());
    // A request and the notification that follows it, in one write.
    let (c, mc) = l.request_msg("shutdown", Value::Null);
    let mut tail = frame(&mc);
    tail.extend(frame(&json!({"jsonrpc": "2.0", "method": "exit"})));
    l.send_bytes(tail);
    assert_eq!(l.wait_response(c)["result"], Value::Null);
    assert_eq!(l.wait_exit(), Some(0));
}

#[test]
fn huge_payloads_in_both_directions() {
    let mut l = Lsp::start();
    l.initialize();
    // 6 MB of comment in one didOpen (the frame is far larger than any pipe buffer).
    let mut text = String::from("fn main() {}\n// ");
    text.push_str(&"x".repeat(6_000_000));
    text.push('\n');
    l.open(URI, &text);
    let r = l.request(
        "textDocument/documentSymbol",
        json!({"textDocument": {"uri": URI}}),
    );
    assert_eq!(r["result"].as_array().unwrap().len(), 1);
    // A big RESPONSE: semantic tokens for 20k lines of code.
    let mut big = String::new();
    for i in 0..20_000 {
        big.push_str(&format!("fn f{i}(a: int) -> int {{ a + {i} }}\n"));
    }
    let u2 = "file:///stdio/big.peb";
    l.open(u2, &big);
    let r = l.request(
        "textDocument/semanticTokens/full",
        json!({"textDocument": {"uri": u2}}),
    );
    assert!(
        r["result"]["data"].as_array().unwrap().len() >= 20_000 * 5 * 8,
        "big response"
    );
    // Many requests pipelined without waiting for answers.
    let ids: Vec<i64> = (0..200)
        .map(|_| {
            let (id, m) = l.request_msg(
                "textDocument/documentSymbol",
                json!({"textDocument": {"uri": URI}}),
            );
            l.send(&m);
            id
        })
        .collect();
    for id in ids {
        assert!(l.wait_response(id)["result"].is_array());
    }
    l.request("shutdown", Value::Null);
    l.notify("exit", Value::Null);
    assert_eq!(l.wait_exit(), Some(0));
}

#[test]
fn malformed_json_body_is_an_error_response_and_the_session_continues() {
    let mut l = Lsp::start();
    l.initialize();
    l.send_bytes(b"Content-Length: 9\r\n\r\n{ not js}".to_vec());
    let m = l.next();
    assert_eq!(m["error"]["code"], -32700, "{m}");
    assert_eq!(m["id"], Value::Null);
    l.send_bytes(b"Content-Length: 2\r\n\r\n42".to_vec()); // valid JSON, not a request object
    let m = l.next();
    assert_eq!(m["error"]["code"], -32600, "{m}");
    // Extra headers and a lower-case name are accepted.
    let body =
        serde_json::to_vec(&json!({"jsonrpc": "2.0", "id": 77, "method": "shutdown"})).unwrap();
    let mut msg = format!(
        "content-length: {}\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n",
        body.len()
    )
    .into_bytes();
    msg.extend(body);
    l.send_bytes(msg);
    assert_eq!(l.wait_response(77)["result"], Value::Null);
    l.notify("exit", Value::Null);
    assert_eq!(l.wait_exit(), Some(0));
}

#[test]
fn malformed_headers_end_the_session_without_hanging() {
    let cases: [&[u8]; 5] = [
        b"Content-Length: abc\r\n\r\n{}",
        b"Content-Length: -5\r\n\r\n{}",
        b"X-Other: 1\r\n\r\n{}",
        b"Content-Length: 99999999999999999999\r\n\r\n",
        b"Content-Length: 10\r\n\r\n{}", // promises 10 bytes, delivers 2, then EOF
    ];
    for (i, case) in cases.iter().enumerate() {
        let mut l = Lsp::start();
        l.initialize();
        l.send_bytes(case.to_vec());
        l.close_stdin();
        let code = l.wait_exit();
        assert_eq!(
            code,
            Some(1),
            "case {i} ({:?}) must end the server with exit code 1",
            String::from_utf8_lossy(case)
        );
    }
    // Garbage before any header.
    let mut l = Lsp::start();
    l.send_bytes(b"\x00\x01\x02 garbage without newline".to_vec());
    l.close_stdin();
    assert_eq!(l.wait_exit(), Some(1));
}
