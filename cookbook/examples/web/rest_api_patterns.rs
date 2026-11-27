//! RECIPE 305 (web): REST API design patterns, shown as real HTTP transcripts
//! LEVEL: advanced
//! Use in: REST API stage (pagination, PATCH, ETags, async jobs, problem+json).   Run: cargo run --release --example rest_api_patterns
//!
//! What: one in-memory `/v1/items` API (real axum server on 127.0.0.1:0, reqwest client) demonstrating the patterns
//! a production API needs, each with a `curl -v` style transcript and asserts:
//!  * offset pagination breaks under concurrent writes (duplicate after an insert, skipped row after a delete);
//!    keyset/cursor pagination (opaque base64url cursor + `Link: rel="next"`) does not.
//!  * filtering + sorting through a WHITELIST (unknown field => 400, it never reaches the "query layer").
//!  * PUT (replace, idempotent) vs PATCH; JSON Merge Patch (RFC 7396) and JSON Patch (RFC 6902: add/remove/replace/
//!    move/copy/test) written by hand; a JSON Patch is applied to a COPY and committed only if EVERY op succeeds.
//!  * optimistic concurrency: `ETag` + `If-Match` (412 stale write, 428 header missing), `If-None-Match` => 304.
//!  * idempotency: PUT/DELETE are naturally idempotent, POST gets `Idempotency-Key`; 201 + `Location`;
//!    202 Accepted + a status resource for long-running work.
//!  * errors as `application/problem+json` (RFC 9457) with stable `type` URIs clients can switch on.
//!  * versioning: URL (`/v2/..`) vs media-type header (`Accept: application/vnd.forge.v2+json`), compared.
//!  * HATEOAS-style `_links`, and bulk operations answering with per-item results (207 Multi-Status style).
//!
//! LIMITS: a JSON Patch is limited in operations, growth and nesting depth. Without that, a few `copy` operations double the
//! document until memory runs out, or nest it until the stack overflows and the process aborts. An item has a size limit too,
//! so that many patches cannot grow it without end. `test` compares numbers by how they are written (`1` and `1.0` differ,
//! although RFC 6902 calls them equal). `If-None-Match` matches one exact tag only (no list, `*` or `W/`). `Accept` is matched
//! by substring (no `q` values). Idempotency keys and finished report jobs are kept in memory and are never evicted.
//!
//! Why: these are the decisions that are expensive to change once clients exist.
//! COMMON USES: public/partner APIs, mobile backends, admin consoles with infinite scroll, payment-style APIs
//! (Idempotency-Key + ETag), long-running exports/reports, gateways that enforce one stable error contract.
//! Read: https://datatracker.ietf.org/doc/html/rfc9457 , https://datatracker.ietf.org/doc/html/rfc7396 ,
//! https://datatracker.ietf.org/doc/html/rfc6902 , https://datatracker.ietf.org/doc/html/rfc8288 ,
//! https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/ETag
#![allow(dead_code)]
use axum::{
    Router,
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Response,
    routing::{get, post},
};
use reqwest::{Client, RequestBuilder};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap},
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering::SeqCst},
    },
    time::Duration,
};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};

// ---------- shared helpers: real loopback servers + curl -v style transcripts ----------
// Servers can be given symbolic names so transcripts show `as.test` instead of a random port (deterministic output).
static NAMES: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
static QUIET: AtomicBool = AtomicBool::new(false);
fn quiet(b: bool) {
    QUIET.store(b, SeqCst);
}
fn mask(s: &str) -> String {
    let mut o = s.to_string();
    for (a, n) in NAMES.lock().unwrap().iter() {
        o = o.replace(a.as_str(), n).replace(&a.replace(':', "%3A"), n);
    }
    o
}

struct Resp {
    status: u16,
    headers: HeaderMap,
    body: String,
}
impl Resp {
    fn h(&self, n: &str) -> String {
        self.headers
            .get(n)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    }
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or(Value::Null)
    }
}

fn preview(s: &str) -> String {
    let s = s.replace('\r', "\\r").replace('\n', "\\n");
    if s.chars().count() > 170 {
        format!("{}...", s.chars().take(170).collect::<String>())
    } else {
        s
    }
}

/// Send a request and print a `curl -v` style transcript (Date omitted, Host omitted, ports masked: deterministic).
async fn call(c: &Client, rb: RequestBuilder, note: &str) -> Resp {
    try_call(c, rb, note).await.expect("request failed")
}

/// Like `call`, but a transport error (timeout, reset, truncated body) is returned and printed instead of panicking.
async fn try_call(c: &Client, rb: RequestBuilder, note: &str) -> Result<Resp, reqwest::Error> {
    let req = rb.build().unwrap();
    let show = !QUIET.load(SeqCst);
    let pr = |s: String| {
        if show {
            println!("{s}")
        }
    };
    pr(format!("\n# {note}"));
    let u = req.url();
    pr(format!(
        "> {} {}{} HTTP/1.1",
        req.method(),
        u.path(),
        u.query()
            .map(|q| mask(&format!("?{q}")))
            .unwrap_or_default()
    ));
    for (k, v) in req.headers() {
        pr(format!("> {k}: {}", mask(v.to_str().unwrap_or("<binary>"))));
    }
    if let Some(b) = req.body().and_then(|b| b.as_bytes()) {
        pr(format!(
            ">\n> {}",
            mask(&preview(&String::from_utf8_lossy(b)))
        ));
    }
    let r = match c.execute(req).await {
        Ok(r) => r,
        Err(e) => {
            pr(format!("< (no response: {})", err_kind(&e)));
            return Err(e);
        }
    };
    pr(format!("< HTTP/1.1 {}", r.status()));
    let status = r.status().as_u16();
    let headers = r.headers().clone();
    let mut hs: Vec<_> = headers
        .iter()
        .filter(|(k, _)| k.as_str() != "date")
        .collect();
    hs.sort_by_key(|(k, _)| k.as_str().to_string());
    for (k, v) in hs {
        pr(format!("< {k}: {}", mask(v.to_str().unwrap_or("<binary>"))));
    }
    let body = match r.text().await {
        Ok(b) => b,
        Err(e) => {
            pr(format!("< (body cut off: {})", err_kind(&e)));
            return Err(e);
        }
    };
    if !body.is_empty() {
        pr(format!("<\n< {}", mask(&preview(&body))));
    }
    Ok(Resp {
        status,
        headers,
        body,
    })
}
fn err_kind(e: &reqwest::Error) -> &'static str {
    if e.is_timeout() {
        "timeout"
    } else if e.is_connect() {
        "connect failed"
    } else if e.is_body() || e.is_decode() {
        "incomplete body"
    } else {
        "connection closed/reset"
    }
}

struct Srv {
    addr: SocketAddr,
    tx: Option<oneshot::Sender<()>>,
    h: JoinHandle<()>,
}
impl Srv {
    async fn start_named(name: &str, app: Router) -> Srv {
        let s = Srv::start(app).await;
        NAMES
            .lock()
            .unwrap()
            .push((s.addr.to_string(), name.to_string()));
        s
    }
    async fn start(app: Router) -> Srv {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        let (tx, rx) = oneshot::channel::<()>();
        let h = tokio::spawn(async move {
            axum::serve(l, app)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await
                .unwrap();
        });
        Srv {
            addr,
            tx: Some(tx),
            h,
        }
    }
    fn url(&self, p: &str) -> String {
        format!("http://{}{}", self.addr, p)
    }
    async fn stop(mut self) {
        if let Some(t) = self.tx.take() {
            let _ = t.send(());
        }
        let _ = tokio::time::timeout(Duration::from_secs(3), self.h).await;
    }
}

// ---------------------------------------------------------------- tiny base64url (for opaque cursors)
const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
fn b64e(d: &[u8]) -> String {
    let mut out = String::new();
    for c in d.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..=c.len() {
            out.push(B64[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}
fn b64d(s: &str) -> Option<Vec<u8>> {
    let (mut out, mut buf, mut bits) = (vec![], 0u32, 0);
    for ch in s.bytes() {
        let v = B64.iter().position(|&c| c == ch)? as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

// ---------------------------------------------------------------- RFC 7396 + RFC 6902 by hand
fn merge_patch(target: &mut Value, patch: &Value) {
    if let Value::Object(p) = patch {
        if !target.is_object() {
            *target = json!({});
        }
        let t = target.as_object_mut().unwrap();
        for (k, v) in p {
            if v.is_null() {
                t.remove(k);
            } else {
                merge_patch(t.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
    } else {
        *target = patch.clone();
    }
}

fn unescape(s: &str) -> String {
    s.replace("~1", "/").replace("~0", "~")
}
fn split_ptr(p: &str) -> Result<(String, String), String> {
    if p.is_empty() {
        return Ok((String::new(), String::new()));
    }
    if !p.starts_with('/') {
        return Err(format!("invalid pointer `{p}`"));
    }
    let i = p.rfind('/').unwrap();
    Ok((p[..i].to_string(), unescape(&p[i + 1..])))
}
/// RFC 6901: digits only, without a leading zero (`str::parse` alone would also take `+1` and `01`).
fn array_index(s: &str) -> Result<usize, String> {
    let digits = !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    match s.parse() {
        Ok(i) if digits && (s == "0" || !s.starts_with('0')) => Ok(i),
        _ => Err(format!("bad array index `{s}`")),
    }
}
fn ptr_add(doc: &mut Value, path: &str, v: Value) -> Result<(), String> {
    if path.is_empty() {
        *doc = v;
        return Ok(());
    }
    let (parent, last) = split_ptr(path)?;
    match doc
        .pointer_mut(&parent)
        .ok_or(format!("parent of `{path}` does not exist"))?
    {
        Value::Object(m) => {
            m.insert(last, v);
            Ok(())
        }
        Value::Array(a) => {
            if last == "-" {
                a.push(v);
                return Ok(());
            }
            let i = array_index(&last)?;
            if i > a.len() {
                return Err(format!("index {i} out of range"));
            }
            a.insert(i, v);
            Ok(())
        }
        _ => Err(format!("parent of `{path}` is a scalar")),
    }
}
fn ptr_remove(doc: &mut Value, path: &str) -> Result<Value, String> {
    if path.is_empty() {
        return Err("the whole document cannot be removed".into());
    }
    let (parent, last) = split_ptr(path)?;
    match doc
        .pointer_mut(&parent)
        .ok_or(format!("parent of `{path}` does not exist"))?
    {
        Value::Object(m) => m.remove(&last).ok_or(format!("`{path}` does not exist")),
        Value::Array(a) => {
            let i = array_index(&last)?;
            if i >= a.len() {
                return Err(format!("index {i} out of range"));
            }
            Ok(a.remove(i))
        }
        _ => Err(format!("parent of `{path}` is a scalar")),
    }
}
const MAX_PATCH_OPS: usize = 32;
const MAX_PATCH_GROWTH: usize = 64 * 1024; // bytes of JSON
const MAX_DOC_DEPTH: usize = 32;
const MAX_ITEM_BYTES: usize = 64 * 1024; // of the stored item, however it got there

/// True if arrays and objects are nested at most `left` deep. It recurses no deeper than that, so any input is safe.
fn within_depth(v: &Value, left: usize) -> bool {
    match v {
        Value::Array(a) => left > 0 && a.iter().all(|x| within_depth(x, left - 1)),
        Value::Object(m) => left > 0 && m.values().all(|x| within_depth(x, left - 1)),
        _ => true,
    }
}

/// Apply every operation to a COPY; the caller only commits on Ok (all-or-nothing).
fn json_patch(doc: &Value, ops: &Value) -> Result<Value, (usize, String)> {
    let mut d = doc.clone();
    let ops = ops
        .as_array()
        .ok_or((0, "patch must be an array".to_string()))?;
    if ops.len() > MAX_PATCH_OPS {
        return Err((0, format!("more than {MAX_PATCH_OPS} operations")));
    }
    // checked after every operation: one `copy` can at most double the document before it is caught
    let max_len = doc.to_string().len() + MAX_PATCH_GROWTH;
    for (i, op) in ops.iter().enumerate() {
        let e = |m: String| (i, m);
        let s = |k: &str| op.get(k).and_then(Value::as_str).map(str::to_string);
        let path = s("path").ok_or(e("missing `path`".into()))?;
        match s("op").as_deref() {
            Some("add") => ptr_add(
                &mut d,
                &path,
                op.get("value")
                    .cloned()
                    .ok_or(e("missing `value`".into()))?,
            )
            .map_err(e)?,
            Some("remove") => {
                ptr_remove(&mut d, &path).map_err(e)?;
            }
            Some("replace") => {
                let v = op
                    .get("value")
                    .cloned()
                    .ok_or(e("missing `value`".into()))?;
                *d.pointer_mut(&path)
                    .ok_or(e(format!("`{path}` does not exist")))? = v;
            }
            Some("move") => {
                let from = s("from").ok_or(e("missing `from`".into()))?;
                // after the removal an array index would name the NEXT element, so this must be refused up front
                if path.starts_with(&format!("{from}/")) {
                    return Err(e("cannot move a value into itself".into()));
                }
                let v = ptr_remove(&mut d, &from).map_err(e)?;
                ptr_add(&mut d, &path, v).map_err(e)?;
            }
            Some("copy") => {
                let from = s("from").ok_or(e("missing `from`".into()))?;
                let v = d
                    .pointer(&from)
                    .cloned()
                    .ok_or(e(format!("`{from}` does not exist")))?;
                ptr_add(&mut d, &path, v).map_err(e)?;
            }
            Some("test") => {
                // without this, a missing value and a missing target would compare equal
                let want = op.get("value").ok_or(e("missing `value`".into()))?;
                if d.pointer(&path) != Some(want) {
                    return Err(e(format!("test failed at `{path}`")));
                }
            }
            other => return Err(e(format!("unknown op {other:?}"))),
        }
        // depth first: writing out (like cloning and comparing) recurses once per level. One operation on a document within
        // the limit at most adds a request value (serde_json parses at most 128 levels) or doubles the depth.
        if !within_depth(&d, MAX_DOC_DEPTH) {
            return Err(e("the document is nested too deeply".into()));
        }
        if d.to_string().len() > max_len {
            return Err(e("the document grew too much".into()));
        }
    }
    Ok(d)
}

// ---------------------------------------------------------------- the API
#[derive(Clone)]
struct Item {
    version: u64,
    doc: Value,
}
#[derive(Default)]
struct Store {
    items: BTreeMap<u64, Item>,
    next: u64,
    idem: HashMap<String, (String, u64, Item)>, // key -> (body, id, the item as first answered)
    job_polls: HashMap<u64, u32>,
    next_job: u64,
}
type Db = Arc<Mutex<Store>>;
const CATS: [&str; 3] = ["tools", "toys", "books"];

fn problem(status: u16, slug: &str, title: &str, detail: &str, extra: Value) -> Response {
    let mut b = json!({"type": format!("https://api.example.test/problems/{slug}"), "title": title, "status": status, "detail": detail});
    if let (Some(m), Some(e)) = (b.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            m.insert(k.clone(), v.clone());
        }
    }
    Response::builder()
        .status(status)
        .header("content-type", "application/problem+json")
        .body(Body::from(b.to_string()))
        .unwrap()
}
fn reply(status: u16, headers: &[(&str, String)], body: Value) -> Response {
    let mut b = Response::builder()
        .status(status)
        .header("content-type", "application/json");
    for (k, v) in headers {
        b = b.header(*k, v);
    }
    b.body(Body::from(if body.is_null() {
        String::new()
    } else {
        body.to_string()
    }))
    .unwrap()
}
fn validate(doc: &Value) -> Result<(), Vec<String>> {
    let mut errs = vec![];
    match doc.as_object() {
        None => return Err(vec!["body must be an object".into()]),
        Some(m) => {
            for k in m.keys() {
                if !["name", "price", "category", "tags"].contains(&k.as_str()) {
                    errs.push(format!("unknown field `{k}`"));
                }
            }
        }
    }
    if !doc["name"].as_str().is_some_and(|s| !s.is_empty()) {
        errs.push("`name` must be a non-empty string".into());
    }
    if !doc["price"].as_f64().is_some_and(|p| p >= 0.0) {
        errs.push("`price` must be a number >= 0".into());
    }
    if !doc["category"].as_str().is_some_and(|c| CATS.contains(&c)) {
        errs.push(format!("`category` must be one of {CATS:?}"));
    }
    if !doc["tags"].is_null()
        && !doc["tags"]
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string))
    {
        errs.push("`tags` must be an array of strings".into());
    }
    if doc.to_string().len() > MAX_ITEM_BYTES {
        errs.push(format!("the item is larger than {MAX_ITEM_BYTES} bytes"));
    }
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}
fn invalid(errs: Vec<String>) -> Response {
    problem(
        422,
        "validation-failed",
        "Validation failed",
        "the item is not valid",
        json!({"errors": errs}),
    )
}
fn etag(id: u64, it: &Item) -> String {
    format!("\"{id}-v{}\"", it.version)
}
fn item_json(id: u64, it: &Item) -> Value {
    let mut v = it.doc.clone();
    let m = v.as_object_mut().unwrap();
    m.insert("id".into(), json!(id));
    m.insert(
        "_links".into(),
        json!({"self": {"href": format!("/v1/items/{id}")}, "collection": {"href": "/v1/items"},
        "delete": {"href": format!("/v1/items/{id}"), "method": "DELETE"}}),
    );
    v
}
fn item_reply(status: u16, id: u64, it: &Item, extra: &[(&str, String)]) -> Response {
    let mut h = vec![("etag", etag(id, it))];
    h.extend(extra.iter().map(|(k, v)| (*k, v.clone())));
    reply(status, &h, item_json(id, it))
}
fn not_found(id: u64) -> Response {
    problem(
        404,
        "not-found",
        "Item not found",
        &format!("no item {id}"),
        json!({}),
    )
}
fn check_if_match(h: &HeaderMap, current: &str, required: bool) -> Result<(), Response> {
    match h.get("if-match").and_then(|v| v.to_str().ok()) {
        None if required => Err(problem(
            428,
            "precondition-required",
            "Precondition required",
            "send If-Match with the ETag you last read",
            json!({}),
        )),
        None => Ok(()),
        Some(v) if v.trim() == "*" || v.split(',').any(|t| t.trim() == current) => Ok(()),
        Some(_) => Err(problem(
            412,
            "precondition-failed",
            "Precondition failed",
            "the item changed since you read it; GET it again",
            json!({"current_etag": current}),
        )),
    }
}
fn parse_usize(
    q: &HashMap<String, String>,
    k: &str,
    def: usize,
    (min, max): (usize, usize),
) -> Result<usize, Response> {
    match q.get(k) {
        None => Ok(def),
        Some(s) => s
            .parse::<usize>()
            .ok()
            .filter(|n| (min..=max).contains(n))
            .ok_or_else(|| {
                problem(
                    400,
                    "invalid-parameter",
                    "Invalid parameter",
                    &format!("`{k}` must be an integer in {min}..={max}"),
                    json!({"parameter": k}),
                )
            }),
    }
}

/// Offset pagination + filter + sort (whitelisted).
async fn list_offset(State(db): State<Db>, Query(q): Query<HashMap<String, String>>) -> Response {
    for k in q.keys() {
        if !["limit", "offset", "sort", "category"].contains(&k.as_str()) {
            return problem(
                400,
                "unknown-parameter",
                "Unknown query parameter",
                &format!("`{k}` is not supported"),
                json!({"allowed": ["limit", "offset", "sort", "category"]}),
            );
        }
    }
    let (limit, offset) = match (
        parse_usize(&q, "limit", 5, (1, 50)), // a page of 0 would link to itself
        parse_usize(&q, "offset", 0, (0, 1_000_000)),
    ) {
        (Ok(l), Ok(o)) => (l, o),
        (Err(r), _) | (_, Err(r)) => return r,
    };
    let sort = q.get("sort").map(String::as_str).unwrap_or("id");
    let (field, desc) = (sort.trim_start_matches('-'), sort.starts_with('-'));
    if !["id", "name", "price"].contains(&field) {
        return problem(
            400,
            "invalid-sort",
            "Invalid sort field",
            &format!("cannot sort by `{field}`"),
            json!({"allowed": ["id", "name", "price"]}),
        );
    }
    if let Some(c) = q.get("category")
        && !CATS.contains(&c.as_str())
    {
        return problem(
            400,
            "invalid-filter",
            "Invalid filter",
            &format!("unknown category `{c}`"),
            json!({"allowed": CATS}),
        );
    }
    let g = db.lock().unwrap();
    let mut rows: Vec<(u64, &Item)> = g
        .items
        .iter()
        .filter(|(_, it)| {
            q.get("category")
                .is_none_or(|c| it.doc["category"] == json!(c))
        })
        .map(|(k, v)| (*k, v))
        .collect();
    rows.sort_by(|a, b| match field {
        "name" => a.1.doc["name"].as_str().cmp(&b.1.doc["name"].as_str()),
        "price" => a.1.doc["price"]
            .as_f64()
            .partial_cmp(&b.1.doc["price"].as_f64())
            .unwrap()
            .then(a.0.cmp(&b.0)),
        _ => a.0.cmp(&b.0),
    });
    if desc {
        rows.reverse();
    }
    let total = rows.len();
    let page: Vec<Value> = rows
        .iter()
        .skip(offset)
        .take(limit)
        .map(|(id, it)| item_json(*id, it))
        .collect();
    let rest: String = q
        .iter()
        .filter(|(k, _)| !["limit", "offset"].contains(&k.as_str()))
        .map(|(k, v)| format!("&{k}={v}"))
        .collect();
    let mut links = vec![];
    if offset + limit < total {
        links.push(format!(
            "</v1/items?limit={limit}&offset={}{rest}>; rel=\"next\"",
            offset + limit
        ));
    }
    if offset > 0 {
        links.push(format!(
            "</v1/items?limit={limit}&offset={}{rest}>; rel=\"prev\"",
            offset.saturating_sub(limit)
        ));
    }
    reply(
        200,
        &[
            ("link", links.join(", ")),
            ("x-total-count", total.to_string()),
        ],
        json!({"items": page, "total": total}),
    )
}

/// Keyset (cursor) pagination, newest first. The cursor is opaque to clients.
async fn feed(State(db): State<Db>, Query(q): Query<HashMap<String, String>>) -> Response {
    let limit = match parse_usize(&q, "limit", 3, (1, 50)) {
        Ok(l) => l,
        Err(r) => return r,
    };
    let after = match q.get("cursor") {
        None => u64::MAX,
        Some(c) => match b64d(c)
            .and_then(|b| String::from_utf8(b).ok())
            .and_then(|s| s.strip_prefix("id<").and_then(|n| n.parse().ok()))
        {
            Some(n) => n,
            None => {
                return problem(
                    400,
                    "invalid-cursor",
                    "Invalid cursor",
                    "the cursor is malformed or expired",
                    json!({}),
                );
            }
        },
    };
    let g = db.lock().unwrap();
    let mut rows: Vec<_> = g
        .items
        .iter()
        .rev()
        .filter(|(id, _)| **id < after)
        .take(limit + 1)
        .collect();
    let more = rows.len() > limit;
    rows.truncate(limit);
    let mut h = vec![];
    if more {
        h.push((
            "link",
            format!(
                "</v1/feed?limit={limit}&cursor={}>; rel=\"next\"",
                b64e(format!("id<{}", rows.last().unwrap().0).as_bytes())
            ),
        ));
    }
    reply(
        200,
        &h,
        json!({"items": rows.iter().map(|(id, it)| item_json(**id, it)).collect::<Vec<_>>()}),
    )
}

async fn create(State(db): State<Db>, headers: HeaderMap, body: Bytes) -> Response {
    let text = String::from_utf8_lossy(&body).to_string();
    let Ok(doc) = serde_json::from_str::<Value>(&text) else {
        return problem(
            400,
            "malformed-json",
            "Malformed JSON",
            "request body is not valid JSON",
            json!({}),
        );
    };
    if let Err(e) = validate(&doc) {
        return invalid(e);
    }
    let mut g = db.lock().unwrap();
    let key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    if let Some(k) = &key
        && let Some((orig, id, it)) = g.idem.get(k).cloned()
    {
        if orig != text {
            return problem(
                422,
                "idempotency-key-reuse",
                "Idempotency key reused",
                "this key was used with a different request body",
                json!({}),
            );
        }
        // the stored answer, not the live item: it may have been changed or deleted since
        return item_reply(
            201,
            id,
            &it,
            &[
                ("location", format!("/v1/items/{id}")),
                ("idempotent-replayed", "true".into()),
            ],
        );
    }
    g.next += 1;
    let id = g.next;
    let it = Item { version: 1, doc };
    g.items.insert(id, it.clone());
    if let Some(k) = key {
        g.idem.insert(k, (text, id, it.clone()));
    }
    item_reply(201, id, &it, &[("location", format!("/v1/items/{id}"))])
}

async fn get_one(State(db): State<Db>, Path(id): Path<u64>, headers: HeaderMap) -> Response {
    let g = db.lock().unwrap();
    let Some(it) = g.items.get(&id) else {
        return not_found(id);
    };
    if headers.get("if-none-match").and_then(|v| v.to_str().ok()) == Some(etag(id, it).as_str()) {
        return reply(304, &[("etag", etag(id, it))], Value::Null);
    }
    item_reply(200, id, it, &[])
}

async fn put_one(
    State(db): State<Db>,
    Path(id): Path<u64>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Ok(doc) = serde_json::from_slice::<Value>(&body) else {
        return problem(
            400,
            "malformed-json",
            "Malformed JSON",
            "invalid JSON",
            json!({}),
        );
    };
    let mut g = db.lock().unwrap();
    let Some(it) = g.items.get_mut(&id) else {
        return not_found(id);
    };
    if let Err(r) = check_if_match(&headers, &etag(id, it), true) {
        return r;
    }
    if let Err(e) = validate(&doc) {
        return invalid(e);
    }
    if it.doc != doc {
        it.doc = doc;
        it.version += 1;
    } // same PUT twice => same state, same ETag
    item_reply(200, id, it, &[])
}

async fn patch_one(
    State(db): State<Db>,
    Path(id): Path<u64>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let ct = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let Ok(patch) = serde_json::from_slice::<Value>(&body) else {
        return problem(
            400,
            "malformed-json",
            "Malformed JSON",
            "invalid JSON",
            json!({}),
        );
    };
    let mut g = db.lock().unwrap();
    let Some(it) = g.items.get_mut(&id) else {
        return not_found(id);
    };
    if let Err(r) = check_if_match(&headers, &etag(id, it), false) {
        return r;
    }
    let new_doc = if ct.starts_with("application/merge-patch+json") {
        let mut d = it.doc.clone();
        merge_patch(&mut d, &patch);
        d
    } else if ct.starts_with("application/json-patch+json") {
        match json_patch(&it.doc, &patch) {
            Ok(d) => d,
            Err((i, m)) => {
                return problem(
                    422,
                    "patch-failed",
                    "Patch failed",
                    &m,
                    json!({"operation": i, "applied": 0}),
                );
            }
        }
    } else {
        return problem(
            415,
            "unsupported-media-type",
            "Unsupported media type",
            "use application/merge-patch+json or application/json-patch+json",
            json!({}),
        );
    };
    if let Err(e) = validate(&new_doc) {
        return invalid(e);
    }
    if it.doc != new_doc {
        it.doc = new_doc;
        it.version += 1;
    }
    item_reply(200, id, it, &[])
}

async fn delete_one(State(db): State<Db>, Path(id): Path<u64>) -> Response {
    db.lock().unwrap().items.remove(&id); // 204 whether or not it existed: the END STATE is the same
    reply(204, &[], Value::Null)
}

/// Long-running work: 202 + Location of a status resource that the client polls.
async fn start_report(State(db): State<Db>) -> Response {
    let mut g = db.lock().unwrap();
    g.next_job += 1;
    let id = g.next_job;
    g.job_polls.insert(id, 0);
    reply(
        202,
        &[
            ("location", format!("/v1/jobs/{id}")),
            ("retry-after", "1".into()),
        ],
        json!({"status": "queued", "_links": {"status": {"href": format!("/v1/jobs/{id}")}}}),
    )
}
async fn job_status(State(db): State<Db>, Path(id): Path<u64>) -> Response {
    let mut g = db.lock().unwrap();
    let Some(n) = g.job_polls.get_mut(&id) else {
        return problem(404, "not-found", "Job not found", "no such job", json!({}));
    };
    *n += 1;
    if *n < 3 {
        reply(
            200,
            &[("retry-after", "1".into())],
            json!({"status": "running", "progress": *n * 40}),
        )
    } else {
        reply(
            200,
            &[],
            json!({"status": "succeeded", "_links": {"result": {"href": format!("/v1/jobs/{id}/result")}}}),
        )
    }
}

/// Bulk create: independent per-item results inside one 207; the batch itself is NOT atomic.
async fn bulk(State(db): State<Db>, body: Bytes) -> Response {
    let Ok(v) = serde_json::from_slice::<Value>(&body) else {
        return problem(
            400,
            "malformed-json",
            "Malformed JSON",
            "invalid JSON",
            json!({}),
        );
    };
    let Some(arr) = v["items"].as_array() else {
        return problem(
            400,
            "malformed-json",
            "Malformed",
            "`items` array required",
            json!({}),
        );
    };
    let mut g = db.lock().unwrap();
    let mut results = vec![];
    for (i, doc) in arr.iter().enumerate() {
        match validate(doc) {
            Ok(()) => {
                g.next += 1;
                let id = g.next;
                g.items.insert(id, Item { version: 1, doc: doc.clone() });
                results.push(json!({"index": i, "status": 201, "id": id, "location": format!("/v1/items/{id}")}));
            }
            Err(e) => results.push(json!({"index": i, "status": 422, "problem": {"type": "https://api.example.test/problems/validation-failed", "errors": e}})),
        }
    }
    reply(207, &[], json!({"results": results}))
}

// versioning: v1 is deprecated, v2 splits the name; the header strategy serves the same URL.
async fn user_v1() -> Response {
    reply(
        200,
        &[
            ("deprecation", "true".into()),
            ("sunset", "Wed, 01 Jul 2026 00:00:00 GMT".into()),
            ("link", "</v2/users/me>; rel=\"successor-version\"".into()),
        ],
        json!({"name": "Ada Lovelace"}),
    )
}
async fn user_v2() -> Response {
    reply(
        200,
        &[],
        json!({"first_name": "Ada", "last_name": "Lovelace"}),
    )
}
async fn user_negotiated(headers: HeaderMap) -> Response {
    let accept = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("*/*");
    let (ct, body) = if accept.contains("vnd.forge.v2+json") {
        (
            "application/vnd.forge.v2+json",
            json!({"first_name": "Ada", "last_name": "Lovelace"}),
        )
    } else if accept.contains("vnd.forge.v1+json")
        || accept.contains("*/*")
        || accept.contains("application/json")
    {
        (
            "application/vnd.forge.v1+json",
            json!({"name": "Ada Lovelace"}),
        )
    } else {
        return problem(
            406,
            "not-acceptable",
            "Not acceptable",
            "supported: application/vnd.forge.v1+json, application/vnd.forge.v2+json",
            json!({}),
        );
    };
    Response::builder()
        .status(200)
        .header("content-type", ct)
        .header("vary", "Accept")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn app() -> Router {
    let db: Db = Arc::new(Mutex::new(Store::default()));
    {
        let mut g = db.lock().unwrap();
        for i in 1..=10u64 {
            g.next = i;
            g.items.insert(i, Item { version: 1, doc: json!({"name": format!("item-{i:02}"), "price": (i * 7) % 13 + 1, "category": CATS[(i % 3) as usize], "tags": ["seed"]}) });
        }
    }
    Router::new()
        .route("/v1/items", get(list_offset).post(create))
        .route(
            "/v1/items/{id}",
            get(get_one)
                .put(put_one)
                .patch(patch_one)
                .delete(delete_one),
        )
        .route("/v1/feed", get(feed))
        .route("/v1/reports", post(start_report))
        .route("/v1/jobs/{id}", get(job_status))
        .route("/v1/bulk/items", post(bulk))
        .route("/v1/users/me", get(user_v1))
        .route("/v2/users/me", get(user_v2))
        .route("/users/me", get(user_negotiated))
        .with_state(db)
}

fn ids(r: &Resp) -> Vec<u64> {
    r.json()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_u64().unwrap())
        .collect()
}
fn next_link(r: &Resp) -> Option<String> {
    r.h("link")
        .split(',')
        .find(|p| p.contains("rel=\"next\""))
        .map(|p| {
            p.split('<')
                .nth(1)
                .unwrap()
                .split('>')
                .next()
                .unwrap()
                .to_string()
        })
}
fn new_item(name: &str) -> Value {
    json!({"name": name, "price": 5, "category": "tools", "tags": []})
}

#[tokio::main]
async fn main() {
    let srv = Srv::start(app()).await;
    let c = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let u = |p: &str| srv.url(p);

    println!("==== 1. OFFSET pagination under writes: duplicates and skips ====");
    let p1 = call(
        &c,
        c.get(u("/v1/items?sort=-id&limit=3&offset=0")),
        "offset page 1 (newest first)",
    )
    .await;
    assert_eq!(ids(&p1), vec![10, 9, 8]);
    assert!(p1.h("link").contains("rel=\"next\""));
    let created = call(
        &c,
        c.post(u("/v1/items")).json(&new_item("late-arrival")),
        "someone inserts a new item meanwhile",
    )
    .await;
    assert_eq!(
        (created.status, created.h("location").as_str()),
        (201, "/v1/items/11")
    );
    let p2 = call(
        &c,
        c.get(u("/v1/items?sort=-id&limit=3&offset=3")),
        "offset page 2: item 8 appears AGAIN",
    )
    .await;
    assert_eq!(ids(&p2), vec![8, 7, 6]);
    println!(
        "   => offset paging returned 8 twice: {:?} then {:?}",
        ids(&p1),
        ids(&p2)
    );

    println!("\n==== 2. CURSOR (keyset) pagination: stable, with Link headers ====");
    let c1 = call(&c, c.get(u("/v1/feed?limit=3")), "cursor page 1").await;
    assert_eq!(ids(&c1), vec![11, 10, 9]);
    call(
        &c,
        c.post(u("/v1/items")).json(&new_item("another-late")),
        "insert id 12 between the two page fetches",
    )
    .await;
    let next = next_link(&c1).expect("next link");
    let c2 = call(&c, c.get(u(&next)), "cursor page 2: no duplicate, no skip").await;
    assert_eq!(ids(&c2), vec![8, 7, 6]);
    let mut all = ids(&c1);
    let mut cur = next_link(&c2);
    all.extend(ids(&c2));
    while let Some(l) = cur {
        let r = c.get(u(&l)).send().await.unwrap();
        let link = r
            .headers()
            .get("link")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let j: Value = r.json().await.unwrap();
        all.extend(
            j["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| i["id"].as_u64().unwrap()),
        );
        cur = link.and_then(|l| {
            l.split('<')
                .nth(1)
                .map(|s| s.split('>').next().unwrap().to_string())
        });
    }
    assert_eq!(
        all,
        (1..=11).rev().collect::<Vec<_>>(),
        "walking the cursor chain yields every row exactly once"
    );
    let bad = call(
        &c,
        c.get(u("/v1/feed?cursor=%21%21")),
        "tampered cursor => 400 problem",
    )
    .await;
    assert_eq!(
        (bad.status, bad.h("content-type").as_str()),
        (400, "application/problem+json")
    );
    assert_eq!(
        bad.json()["type"],
        "https://api.example.test/problems/invalid-cursor"
    );

    // offset can also SKIP rows: delete a row of a page already seen
    let o1 = call(
        &c,
        c.get(u("/v1/items?sort=-id&limit=3")),
        "offset page 1 again (12,11,10)",
    )
    .await;
    assert_eq!(ids(&o1), vec![12, 11, 10]);
    call(&c, c.delete(u("/v1/items/11")), "item 11 deleted meanwhile").await;
    let o2 = call(
        &c,
        c.get(u("/v1/items?sort=-id&limit=3&offset=3")),
        "offset page 2: item 9 was SKIPPED",
    )
    .await;
    assert_eq!(ids(&o2), vec![8, 7, 6]);

    println!("\n==== 3. filtering and sorting with a WHITELIST ====");
    let f = call(
        &c,
        c.get(u("/v1/items?category=books&sort=-price&limit=50")),
        "filter + sort",
    )
    .await;
    let prices: Vec<f64> = f.json()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["price"].as_f64().unwrap())
        .collect();
    assert!(prices.windows(2).all(|w| w[0] >= w[1]) && !prices.is_empty());
    let s = call(
        &c,
        c.get(u("/v1/items?sort=password")),
        "sort by a non-whitelisted field",
    )
    .await;
    assert_eq!(
        (s.status, s.json()["type"].as_str().unwrap()),
        (400, "https://api.example.test/problems/invalid-sort")
    );
    let s = call(&c, c.get(u("/v1/items?filter=1%3D1")), "unknown parameter").await;
    assert_eq!(
        s.json()["type"],
        "https://api.example.test/problems/unknown-parameter"
    );

    println!("\n==== 4. ETag + If-Match (optimistic concurrency), 304, PUT idempotency ====");
    let g1 = call(&c, c.get(u("/v1/items/1")), "alice reads item 1").await;
    let tag1 = g1.h("etag");
    assert_eq!(tag1, "\"1-v1\"");
    let nm = call(
        &c,
        c.get(u("/v1/items/1")).header("if-none-match", &tag1),
        "conditional GET",
    )
    .await;
    assert_eq!((nm.status, nm.body.len()), (304, 0));
    let mut doc = g1.json();
    doc.as_object_mut().unwrap().remove("id");
    doc.as_object_mut().unwrap().remove("_links");
    doc["price"] = json!(99);
    let no_hdr = call(
        &c,
        c.put(u("/v1/items/1")).json(&doc),
        "PUT without If-Match => 428",
    )
    .await;
    assert_eq!(no_hdr.status, 428);
    let ok = call(
        &c,
        c.put(u("/v1/items/1")).header("if-match", &tag1).json(&doc),
        "alice PUTs with her ETag",
    )
    .await;
    assert_eq!((ok.status, ok.h("etag").as_str()), (200, "\"1-v2\""));
    let mut doc_bob = doc.clone();
    doc_bob["price"] = json!(1);
    let stale = call(
        &c,
        c.put(u("/v1/items/1"))
            .header("if-match", &tag1)
            .json(&doc_bob),
        "bob PUTs with the STALE ETag => 412",
    )
    .await;
    assert_eq!(stale.status, 412);
    assert_eq!(stale.json()["current_etag"], "\"1-v2\"");
    let again = call(
        &c,
        c.put(u("/v1/items/1"))
            .header("if-match", "\"1-v2\"")
            .json(&doc),
        "same PUT repeated: idempotent, ETag unchanged",
    )
    .await;
    assert_eq!((again.status, again.h("etag").as_str()), (200, "\"1-v2\""));

    println!("\n==== 5. PATCH: JSON Merge Patch (RFC 7396) ====");
    // the RFC 7396 appendix example
    let mut t = json!({"a": "b", "c": {"d": "e", "f": "g"}});
    merge_patch(&mut t, &json!({"a": "z", "c": {"f": null}}));
    assert_eq!(t, json!({"a": "z", "c": {"d": "e"}}));
    let mp = call(
        &c,
        c.patch(u("/v1/items/2"))
            .header("content-type", "application/merge-patch+json")
            .body(r#"{"price": 42, "tags": null}"#),
        "merge patch: set price, delete tags (null = remove)",
    )
    .await;
    assert_eq!(mp.status, 200);
    assert_eq!(mp.json()["price"], 42);
    assert!(mp.json().get("tags").is_none());
    let wrong = call(
        &c,
        c.patch(u("/v1/items/2"))
            .header("content-type", "text/plain")
            .body("{}"),
        "wrong media type => 415",
    )
    .await;
    assert_eq!(wrong.status, 415);
    let inv = call(
        &c,
        c.patch(u("/v1/items/2"))
            .header("content-type", "application/merge-patch+json")
            .body(r#"{"price": -5}"#),
        "merge result is validated => 422",
    )
    .await;
    assert_eq!(inv.status, 422);

    println!("\n==== 6. PATCH: JSON Patch (RFC 6902), atomic ====");
    let ops = r#"[{"op":"test","path":"/name","value":"item-03"},
        {"op":"replace","path":"/price","value":77},
        {"op":"add","path":"/tags/-","value":"hot"},
        {"op":"copy","from":"/name","path":"/tags/0"},
        {"op":"remove","path":"/tags/1"}]"#;
    let jp = call(
        &c,
        c.patch(u("/v1/items/3"))
            .header("content-type", "application/json-patch+json")
            .body(ops),
        "json patch: test, replace, add, copy, remove",
    )
    .await;
    assert_eq!(jp.status, 200);
    assert_eq!(jp.json()["price"], 77);
    assert_eq!(jp.json()["tags"], json!(["item-03", "hot"]));
    let mv = call(
        &c,
        c.patch(u("/v1/items/3"))
            .header("content-type", "application/json-patch+json")
            .body(r#"[{"op":"move","from":"/tags/0","path":"/tags/1"}]"#),
        "json patch: move",
    )
    .await;
    assert_eq!(mv.json()["tags"], json!(["hot", "item-03"]));
    // all-or-nothing: op 2 fails, op 0 and 1 must NOT be visible afterwards
    let bad_ops = r#"[{"op":"replace","path":"/price","value":1},{"op":"add","path":"/tags/-","value":"x"},{"op":"remove","path":"/nope"}]"#;
    let fail = call(
        &c,
        c.patch(u("/v1/items/3"))
            .header("content-type", "application/json-patch+json")
            .body(bad_ops),
        "json patch with a failing 3rd op => 422, nothing applied",
    )
    .await;
    assert_eq!(
        (fail.status, fail.json()["operation"].as_u64()),
        (422, Some(2))
    );
    let after = call(&c, c.get(u("/v1/items/3")), "item 3 unchanged").await;
    assert_eq!(after.json()["price"], 77);
    assert_eq!(after.json()["tags"], json!(["hot", "item-03"]));
    let tf = call(
        &c,
        c.patch(u("/v1/items/3"))
            .header("content-type", "application/json-patch+json")
            .body(r#"[{"op":"test","path":"/price","value":1}]"#),
        "failed `test` op",
    )
    .await;
    assert_eq!(tf.status, 422);
    // pointer escapes from RFC 6901
    let d = json_patch(
        &json!({"a/b": {"m~n": 1}}),
        &json!([{"op":"replace","path":"/a~1b/m~0n","value":2}]),
    )
    .unwrap();
    assert_eq!(d, json!({"a/b": {"m~n": 2}}));

    println!("\n==== 7. POST: 201 + Location, Idempotency-Key ====");
    let k1 = call(
        &c,
        c.post(u("/v1/items"))
            .header("idempotency-key", "k-100")
            .json(&new_item("widget")),
        "first POST with key",
    )
    .await;
    assert_eq!(k1.status, 201);
    let k2 = call(
        &c,
        c.post(u("/v1/items"))
            .header("idempotency-key", "k-100")
            .json(&new_item("widget")),
        "client retry (timeout?) with same key: same item",
    )
    .await;
    assert_eq!((k2.status, k2.h("location")), (201, k1.h("location")));
    assert_eq!(k2.h("idempotent-replayed"), "true");
    let k3 = call(
        &c,
        c.post(u("/v1/items"))
            .header("idempotency-key", "k-100")
            .json(&new_item("DIFFERENT")),
        "same key, different body => 422",
    )
    .await;
    assert_eq!(k3.status, 422);
    let all = c
        .get(u("/v1/items?limit=50&sort=-id"))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        all["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["name"] == "widget")
            .count(),
        1,
        "exactly one widget"
    );
    let m = call(
        &c,
        c.post(u("/v1/items")).body("{nope"),
        "malformed JSON => 400 problem",
    )
    .await;
    assert_eq!(
        m.json()["type"],
        "https://api.example.test/problems/malformed-json"
    );
    let v = call(
        &c,
        c.post(u("/v1/items"))
            .json(&json!({"name": "", "price": "free", "colour": 1})),
        "validation => 422 with error list",
    )
    .await;
    assert_eq!(v.status, 422);
    assert_eq!(v.json()["errors"].as_array().unwrap().len(), 4);

    println!("\n==== 8. DELETE is idempotent ====");
    let d1 = call(&c, c.delete(u("/v1/items/4")), "DELETE").await;
    let d2 = call(&c, c.delete(u("/v1/items/4")), "DELETE again: same outcome").await;
    assert_eq!((d1.status, d2.status), (204, 204));
    assert_eq!(c.get(u("/v1/items/4")).send().await.unwrap().status(), 404);

    println!("\n==== 9. 202 Accepted + status resource ====");
    let job = call(&c, c.post(u("/v1/reports")), "start a long-running report").await;
    assert_eq!(
        (job.status, job.h("location").as_str()),
        (202, "/v1/jobs/1")
    );
    let mut polls = 0;
    loop {
        polls += 1;
        let s = call(&c, c.get(u(&job.h("location"))), &format!("poll #{polls}")).await;
        if s.json()["status"] == "succeeded" {
            assert!(s.json()["_links"]["result"]["href"].is_string());
            break;
        }
        assert!(polls < 10);
    }
    assert_eq!(polls, 3);

    println!("\n==== 10. bulk operations with per-item results (207) ====");
    let b = call(
        &c,
        c.post(u("/v1/bulk/items"))
            .json(&json!({"items": [new_item("b1"), {"name": "bad"}, new_item("b3")]})),
        "bulk create: items 0 and 2 succeed, item 1 fails",
    )
    .await;
    assert_eq!(b.status, 207);
    let st: Vec<u64> = b.json()["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["status"].as_u64().unwrap())
        .collect();
    assert_eq!(st, vec![201, 422, 201]);

    println!("\n==== 11. versioning: URL vs header ====");
    let v1 = call(
        &c,
        c.get(u("/v1/users/me")),
        "URL versioning: /v1 (deprecated)",
    )
    .await;
    assert_eq!(
        (v1.h("deprecation").as_str(), v1.json()["name"].as_str()),
        ("true", Some("Ada Lovelace"))
    );
    let v2 = call(&c, c.get(u("/v2/users/me")), "URL versioning: /v2").await;
    assert_eq!(v2.json()["first_name"], "Ada");
    let h2 = call(
        &c,
        c.get(u("/users/me"))
            .header("accept", "application/vnd.forge.v2+json"),
        "header versioning: same URL, Accept selects v2",
    )
    .await;
    assert_eq!(
        (h2.h("content-type").as_str(), h2.h("vary").as_str()),
        ("application/vnd.forge.v2+json", "Accept")
    );
    let h1 = call(
        &c,
        c.get(u("/users/me")),
        "header versioning: no Accept => default v1",
    )
    .await;
    assert_eq!(h1.json()["name"], "Ada Lovelace");
    let na = call(
        &c,
        c.get(u("/users/me")).header("accept", "application/xml"),
        "unsupported => 406",
    )
    .await;
    assert_eq!(na.status, 406);
    println!(
        "\n   URL: visible, cacheable, easy to route/curl, but the URL of a resource changes."
    );
    println!(
        "   Header: stable URLs and fine-grained evolution, but invisible in logs/browsers; MUST send `Vary: Accept`."
    );

    println!("\n==== 12. HATEOAS-style links ====");
    let one = c
        .get(u("/v1/items/5"))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(one["_links"]["self"]["href"], "/v1/items/5");
    assert_eq!(one["_links"]["delete"]["method"], "DELETE");
    println!("   item 5 links: {}", one["_links"]);

    println!("\n==== 13. hostile input ====");
    for path in ["/v1/feed?limit=0", "/v1/items?limit=0"] {
        let r = call(&c, c.get(u(path)), "a page size of 0 => 400").await;
        assert_eq!(r.status, 400, "{path}");
    }
    let keyed = || {
        c.post(u("/v1/items"))
            .header("idempotency-key", "k-200")
            .json(&new_item("short-lived"))
    };
    let first = call(&c, keyed(), "POST with a new key").await;
    let location = first.h("location");
    call(&c, c.delete(u(&location)), "the item is deleted").await;
    let replay = call(
        &c,
        keyed(),
        "late retry: the first answer, nothing re-created",
    )
    .await;
    assert_eq!((replay.status, &replay.body), (201, &first.body));
    assert_eq!(c.get(u(&location)).send().await.unwrap().status(), 404);

    let base = json!({"l": [{"a": 1}, {"b": 2}], "": 5, "tags": ["x"]});
    for bad in [
        json!([{"op": "test", "path": "/nothing"}]), // no value
        json!([{"op": "move", "from": "/l/0", "path": "/l/0/x"}]), // into itself
        json!([{"op": "remove", "path": ""}]),       // not the member named ""
        json!([{"op": "add", "path": "/l/+1", "value": 0}]),
        json!([{"op": "add", "path": "/l/01", "value": 0}]),
        json!([{"op": "remove", "path": "/l/+1"}]),
        Value::Array(vec![
            json!({"op": "test", "path": "/l/0/a", "value": 1});
            33
        ]),
        Value::Array(vec![
            json!({"op": "copy", "from": "/tags", "path": "/tags/-"});
            20 // about 4 MB if nothing stopped it
        ]),
    ] {
        assert!(json_patch(&base, &bad).is_err(), "accepted: {bad}");
    }
    // `[[]]`, then copy the whole of it into its own innermost array: 15 small operations, 32768 levels
    let mut deep = vec![json!({"op": "add", "path": "/tags", "value": [[]]})];
    let mut levels = 2;
    for _ in 0..14 {
        let into = format!("/tags{}/-", "/0".repeat(levels - 1));
        deep.push(json!({"op": "copy", "from": "/tags", "path": into}));
        levels *= 2;
    }
    quiet(true);
    let r = call(
        &c,
        c.patch(u("/v1/items/5"))
            .header("content-type", "application/json-patch+json")
            .body(Value::Array(deep).to_string()),
        "a patch that nests the document 32768 deep",
    )
    .await;
    assert_eq!(r.status, 422);
    // each patch is small enough by itself; the item still cannot grow without end
    let grow = json!([{"op": "add", "path": "/tags/-", "value": "x".repeat(40_000)}]).to_string();
    for want in [200, 422, 422] {
        let r = call(
            &c,
            c.patch(u("/v1/items/5"))
                .header("content-type", "application/json-patch+json")
                .body(grow.clone()),
            "grow an item patch by patch",
        )
        .await;
        assert_eq!(r.status, want);
    }
    quiet(false);
    let alive = call(&c, c.get(u("/v1/items/1")), "the server still answers").await;
    assert_eq!(alive.status, 200);

    srv.stop().await;
    println!("\nOK: all REST pattern assertions passed");
}
