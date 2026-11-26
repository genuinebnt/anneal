//! RECIPE 310 (web): form posts and file uploads: urlencoded + multipart/form-data parsed from scratch
//! LEVEL: intermediate
//! Use in: web stage (HTML forms, file upload endpoints, signed/untrusted input handling).   Run: cargo run --release --example multipart_forms_uploads
//!
//! What: a real axum server with hand-written parsers, driven by a reqwest client that builds the multipart bytes
//! itself (reqwest's own `multipart` feature is not enabled in this crate, and neither is axum's `Multipart`, which needs
//! the `multer` crate: with `axum = { features = ["multipart"] }` the equivalent is
//! `while let Some(field) = multipart.next_field().await? { field.name(); field.file_name(); field.chunk().await? }`).
//!  * `application/x-www-form-urlencoded`: split on `&`/`=`, `+` => space, percent-decoding with strict validation (bad `%zz`,
//!    truncated `%`, invalid UTF-8 => 400), repeated keys kept in order; cross-checked against axum's own `Form` extractor.
//!  * `multipart/form-data` (RFC 7578 / RFC 2046): boundary from the Content-Type (quoted or not, <= 70 chars), a STREAMING
//!    state machine (preamble -> part headers -> data -> delimiter) that never holds more than one network chunk plus
//!    `len(delimiter)` bytes, even when a delimiter is split across chunks (asserted by feeding 1 byte at a time).
//!  * Per-part headers: Content-Disposition (`name`, `filename`, escaped quotes), Content-Type.
//!  * Uploads stream into a temp file with a SIZE LIMIT (413, partial file deleted); a 6 MB upload is accepted with a
//!    buffer of a few tens of KB (`max_buffered` is asserted and printed).
//!  * Security: filenames are untrusted (`../../etc/passwd` => 400, we store under a server-generated name); the
//!    Content-Type the client claims is NOT trusted: we sniff magic bytes and refuse mismatches (e.g. HTML sent as `image/png`).
//! COMMON USES: avatar/document/video uploads, CSV import, HTML form handling, API gateways that must bound memory, antivirus/scan queues.
//! Read: https://datatracker.ietf.org/doc/html/rfc7578 , https://developer.mozilla.org/en-US/docs/Web/HTTP ,
//! https://datatracker.ietf.org/doc/html/rfc9110
#![allow(dead_code)]
use axum::{
    Form, Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Query, State},
    http::HeaderMap,
    response::Response,
    routing::post,
};
use futures_util::StreamExt;
use reqwest::{Client, RequestBuilder};
use serde_json::{Map, Value, json};
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering::SeqCst},
    },
    time::Duration,
};
use tokio::{io::AsyncWriteExt, net::TcpListener, sync::oneshot, task::JoinHandle};

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

fn reply(status: u16, v: Value) -> Response {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(v.to_string()))
        .unwrap()
}
fn fail(status: u16, msg: &str) -> Response {
    reply(status, json!({"error": msg}))
}

// ================================================================ urlencoded
fn pct_decode(s: &str, plus: bool) -> Result<String, String> {
    let b = s.as_bytes();
    let (mut out, mut i) = (Vec::with_capacity(b.len()), 0);
    while i < b.len() {
        match b[i] {
            b'+' if plus => {
                out.push(b' ');
                i += 1;
            }
            b'%' => {
                let h = b
                    .get(i + 1..i + 3)
                    .and_then(|h| std::str::from_utf8(h).ok())
                    .and_then(|h| u8::from_str_radix(h, 16).ok());
                out.push(h.ok_or(format!("bad percent-escape at byte {i}"))?);
                i += 3;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| "decoded bytes are not valid UTF-8".to_string())
}
fn pct_encode_form(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else if b == b' ' {
                "+".into()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn parse_urlencoded(body: &str) -> Result<Vec<(String, String)>, String> {
    body.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            Ok((pct_decode(k, true)?, pct_decode(v, true)?))
        })
        .collect()
}
fn group(pairs: Vec<(String, String)>) -> Value {
    let mut m: Map<String, Value> = Map::new();
    for (k, v) in pairs {
        m.entry(k)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .unwrap()
            .push(json!(v)); // repeated keys => ordered list
    }
    Value::Object(m)
}
async fn form_own(body: Bytes) -> Response {
    match std::str::from_utf8(&body)
        .map_err(|e| e.to_string())
        .and_then(parse_urlencoded)
    {
        Ok(p) => reply(200, group(p)),
        Err(e) => fail(400, &e),
    }
}
async fn form_axum(Form(p): Form<Vec<(String, String)>>) -> Response {
    reply(200, group(p))
}

// ================================================================ multipart: parameters, headers, boundary
/// Parse `token; k=v; k2="quoted \" value"`. Returns (first token, params with lowercase keys).
fn parse_params(s: &str) -> (String, Vec<(String, String)>) {
    let (mut parts, mut cur, mut inq, mut esc) = (vec![], String::new(), false, false);
    for ch in s.chars() {
        if esc {
            cur.push(ch);
            esc = false;
        } else if inq && ch == '\\' {
            esc = true;
        } else if ch == '"' {
            inq = !inq;
        } else if ch == ';' && !inq {
            parts.push(std::mem::take(&mut cur));
        } else {
            cur.push(ch);
        }
    }
    parts.push(cur);
    let first = parts.remove(0).trim().to_string();
    (
        first,
        parts
            .iter()
            .filter_map(|p| p.split_once('='))
            .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
            .collect(),
    )
}
fn boundary_of(content_type: &str) -> Result<String, String> {
    let (mt, ps) = parse_params(content_type);
    if !mt.eq_ignore_ascii_case("multipart/form-data") {
        return Err("expected multipart/form-data".into());
    }
    let b = ps
        .into_iter()
        .find(|(k, _)| k == "boundary")
        .map(|(_, v)| v)
        .ok_or("missing boundary parameter")?;
    if b.is_empty() || b.len() > 70 {
        return Err("boundary must be 1..=70 characters".into());
    }
    Ok(b)
}

#[derive(Debug, Clone, PartialEq)]
struct PartInfo {
    name: String,
    filename: Option<String>,
    ctype: Option<String>,
    extra: Vec<(String, String)>,
}
#[derive(Debug, PartialEq)]
enum Ev {
    Start(PartInfo),
    Data(Vec<u8>),
    End,
}
#[derive(PartialEq, Clone, Copy, Debug)]
enum St {
    Preamble,
    AfterDelim,
    Headers,
    Data,
    Done,
}

/// Incremental parser: `feed` any chunking of the body, get events. Buffer stays ~ one chunk + delimiter.
struct Mp {
    first: Vec<u8>,
    delim: Vec<u8>,
    buf: Vec<u8>,
    st: St,
    max_buf: usize,
}
const MAX_HEADERS: usize = 8 * 1024;
fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

impl Mp {
    fn new(boundary: &str) -> Mp {
        Mp {
            first: format!("--{boundary}").into_bytes(),
            delim: format!("\r\n--{boundary}").into_bytes(),
            buf: vec![],
            st: St::Preamble,
            max_buf: 0,
        }
    }
    fn feed(&mut self, chunk: &[u8]) -> Result<Vec<Ev>, String> {
        self.buf.extend_from_slice(chunk);
        self.max_buf = self.max_buf.max(self.buf.len());
        let mut ev = vec![];
        loop {
            match self.st {
                St::Preamble => match find(&self.buf, &self.first) {
                    Some(p) => {
                        self.buf.drain(..p + self.first.len());
                        self.st = St::AfterDelim;
                    }
                    None => {
                        let keep = self.first.len() - 1;
                        if self.buf.len() > keep {
                            let n = self.buf.len() - keep;
                            self.buf.drain(..n);
                        } // ignore the preamble
                        return Ok(ev);
                    }
                },
                St::AfterDelim => {
                    if self.buf.len() < 2 {
                        return Ok(ev);
                    }
                    if self.buf.starts_with(b"--") {
                        self.st = St::Done;
                    } else if self.buf.starts_with(b"\r\n") {
                        self.buf.drain(..2);
                        self.st = St::Headers;
                    } else {
                        return Err("garbage after boundary".into());
                    }
                }
                St::Headers => match find(&self.buf, b"\r\n\r\n") {
                    Some(p) => {
                        let head = String::from_utf8(self.buf[..p].to_vec())
                            .map_err(|_| "part headers are not UTF-8")?;
                        self.buf.drain(..p + 4);
                        ev.push(Ev::Start(parse_part_headers(&head)?));
                        self.st = St::Data;
                    }
                    None => {
                        if self.buf.len() > MAX_HEADERS {
                            return Err("part headers too large".into());
                        }
                        return Ok(ev);
                    }
                },
                St::Data => match find(&self.buf, &self.delim) {
                    Some(p) => {
                        if p > 0 {
                            ev.push(Ev::Data(self.buf[..p].to_vec()));
                        }
                        self.buf.drain(..p + self.delim.len());
                        ev.push(Ev::End);
                        self.st = St::AfterDelim;
                    }
                    None => {
                        let keep = self.delim.len() - 1; // a delimiter may be split across chunks: hold back its possible prefix
                        if self.buf.len() > keep {
                            let n = self.buf.len() - keep;
                            ev.push(Ev::Data(self.buf[..n].to_vec()));
                            self.buf.drain(..n);
                        }
                        return Ok(ev);
                    }
                },
                St::Done => {
                    self.buf.clear();
                    return Ok(ev);
                }
            }
        }
    }
    fn finish(&self) -> Result<(), String> {
        if self.st == St::Done {
            Ok(())
        } else {
            Err(format!(
                "body ended inside the multipart structure (state {:?})",
                self.st
            ))
        }
    }
}
fn parse_part_headers(head: &str) -> Result<PartInfo, String> {
    let (mut name, mut filename, mut ctype, mut extra) = (None, None, None, vec![]);
    for line in head.split("\r\n") {
        let (k, v) = line.split_once(':').ok_or("malformed part header line")?;
        match k.trim().to_ascii_lowercase().as_str() {
            "content-disposition" => {
                let (disp, ps) = parse_params(v);
                if !disp.eq_ignore_ascii_case("form-data") {
                    return Err("Content-Disposition must be form-data".into());
                }
                for (pk, pv) in ps {
                    match pk.as_str() {
                        "name" => name = Some(pv),
                        "filename" => filename = Some(pv),
                        _ => {}
                    }
                }
            }
            "content-type" => ctype = Some(v.trim().to_ascii_lowercase()),
            other => extra.push((other.to_string(), v.trim().to_string())),
        }
    }
    Ok(PartInfo {
        name: name.ok_or("part without a name")?,
        filename,
        ctype,
        extra,
    })
}

// ================================================================ security helpers
/// Filenames from clients are attacker-controlled. Reject anything path-like; whitelist what remains.
fn clean_filename(raw: &str) -> Result<String, String> {
    if raw.contains('/') || raw.contains('\\') || raw.contains('\0') || raw.contains("..") {
        return Err(format!("unsafe filename {raw:?}"));
    }
    let s: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let s = s.trim_start_matches('.').to_string(); // no hidden files
    if s.is_empty() {
        Err("empty filename".into())
    } else {
        Ok(s.chars().take(100).collect())
    }
}
/// Magic-number sniffing of the first bytes.
fn sniff(b: &[u8]) -> &'static str {
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if b.starts_with(b"GIF8") {
        "image/gif"
    } else if b.starts_with(b"%PDF-") {
        "application/pdf"
    } else if b.starts_with(b"PK\x03\x04") {
        "application/zip"
    } else if !b.is_empty()
        && b.iter()
            .all(|c| matches!(c, b'\t' | b'\n' | b'\r' | 0x20..=0x7e))
    {
        let l = String::from_utf8_lossy(b).to_ascii_lowercase();
        if l.starts_with('<') {
            "text/html"
        } else {
            "text/plain"
        }
    } else {
        "application/octet-stream"
    }
}
const ALLOWED: [&str; 4] = ["image/png", "image/jpeg", "application/pdf", "text/plain"];
fn fnv(mut h: u64, d: &[u8]) -> u64 {
    for b in d {
        h = (h ^ *b as u64).wrapping_mul(0x100000001b3);
    }
    h
}
const FNV0: u64 = 0xcbf29ce484222325;

// ================================================================ upload handler (streams to disk)
struct Up {
    dir: PathBuf,
    limit: usize,
    n: AtomicU32,
}
struct Cur {
    info: PartInfo,
    text: Vec<u8>,
    file: Option<(tokio::fs::File, PathBuf, usize, Vec<u8>, u64)>,
}

async fn upload(
    State(up): State<Arc<Up>>,
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let mut made = vec![];
    let res = upload_inner(
        &up,
        q.get("trust").map(String::as_str) == Some("client"),
        &headers,
        body,
        &mut made,
    )
    .await;
    match res {
        Ok(v) => reply(200, v),
        Err((s, m)) => {
            for p in made {
                let _ = tokio::fs::remove_file(p).await;
            }
            fail(s, &m)
        } // never leave partial files
    }
}
async fn upload_inner(
    up: &Up,
    trust_client: bool,
    headers: &HeaderMap,
    body: Body,
    made: &mut Vec<PathBuf>,
) -> Result<Value, (u16, String)> {
    let ct = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let boundary = boundary_of(ct).map_err(|e| (if ct.is_empty() { 415 } else { 400 }, e))?;
    let (mut mp, mut stream) = (Mp::new(&boundary), body.into_data_stream());
    let (mut fields, mut files, mut cur) = (Map::new(), vec![], None::<Cur>);
    let (mut max_chunk, mut total, mut too_big) = (0usize, 0usize, false);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| (400, format!("body error: {e}")))?;
        max_chunk = max_chunk.max(chunk.len());
        total += chunk.len();
        if too_big {
            if total > up.limit * 8 {
                break;
            }
            continue;
        } // drain (discard) so the client can read our 413, but only up to a cap
        for ev in mp.feed(&chunk).map_err(|e| (400, e))? {
            match ev {
                Ev::Start(info) => {
                    let file = if let Some(raw) = &info.filename {
                        clean_filename(raw).map_err(|e| (400, e))?;
                        let path = up
                            .dir
                            .join(format!("upload-{}.bin", up.n.fetch_add(1, SeqCst) + 1)); // OUR name, never theirs
                        made.push(path.clone());
                        Some((
                            tokio::fs::File::create(&path)
                                .await
                                .map_err(|e| (500, e.to_string()))?,
                            path,
                            0usize,
                            vec![],
                            FNV0,
                        ))
                    } else {
                        None
                    };
                    cur = Some(Cur {
                        info,
                        text: vec![],
                        file,
                    });
                }
                Ev::Data(d) => {
                    let c = cur.as_mut().unwrap();
                    match &mut c.file {
                        Some((f, _, size, first, h)) => {
                            *size += d.len();
                            if *size > up.limit {
                                too_big = true;
                                break;
                            }
                            if first.len() < 16 {
                                first.extend_from_slice(&d[..d.len().min(16 - first.len())]);
                            }
                            *h = fnv(*h, &d);
                            f.write_all(&d).await.map_err(|e| (500, e.to_string()))?;
                        }
                        None => {
                            c.text.extend_from_slice(&d);
                            if c.text.len() > 64 * 1024 {
                                return Err((413, "text field too large".into()));
                            }
                        }
                    }
                }
                Ev::End => {
                    let c = cur.take().unwrap();
                    match c.file {
                        Some((mut f, path, size, first, h)) => {
                            f.flush().await.map_err(|e| (500, e.to_string()))?;
                            let claimed = c
                                .info
                                .ctype
                                .clone()
                                .unwrap_or("application/octet-stream".into());
                            let real = sniff(&first);
                            if !trust_client && (real != claimed || !ALLOWED.contains(&real)) {
                                return Err((
                                    415,
                                    format!(
                                        "file content looks like {real} but was declared as {claimed} (allowed: {ALLOWED:?})"
                                    ),
                                ));
                            }
                            files.push(json!({"field": c.info.name, "filename": clean_filename(c.info.filename.as_deref().unwrap()).unwrap(), "stored_as": path.file_name().unwrap().to_string_lossy(),
                                "bytes": size, "claimed_type": claimed, "sniffed_type": real, "fnv": format!("{h:016x}")}));
                        }
                        None => {
                            fields.insert(
                                c.info.name,
                                json!(
                                    String::from_utf8(c.text).map_err(|_| (
                                        400,
                                        "text field is not UTF-8".to_string()
                                    ))?
                                ),
                            );
                        }
                    }
                }
            }
        }
    }
    if too_big {
        return Err((413, format!("file exceeds the {} byte limit", up.limit)));
    }
    mp.finish().map_err(|e| (400, e))?;
    Ok(
        json!({"fields": fields, "files": files, "max_buffered": mp.max_buf, "max_network_chunk": max_chunk, "total_bytes": total}),
    )
}

// ================================================================ client side: build multipart by hand
struct PartSpec<'a> {
    name: &'a str,
    filename: Option<&'a str>,
    ctype: Option<&'a str>,
    data: &'a [u8],
}
fn multipart_body(boundary: &str, parts: &[PartSpec]) -> Vec<u8> {
    let mut out = vec![];
    for p in parts {
        out.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{}\"",
                p.name.replace('"', "\\\"")
            )
            .as_bytes(),
        );
        if let Some(f) = p.filename {
            out.extend_from_slice(format!("; filename=\"{}\"", f.replace('"', "\\\"")).as_bytes());
        }
        out.extend_from_slice(b"\r\n");
        if let Some(c) = p.ctype {
            out.extend_from_slice(format!("Content-Type: {c}\r\n").as_bytes());
        }
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(p.data);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    out
}
fn show_raw(title: &str, b: &[u8]) {
    println!("\n   raw bytes on the wire ({title}), CR LF made visible:");
    for (i, l) in String::from_utf8_lossy(b).split("\r\n").enumerate() {
        if i >= 14 {
            println!("   | ...");
            break;
        }
        let l = l.replace('\n', "\\n");
        println!(
            "   | {}\\r\\n",
            if l.chars().count() > 80 {
                format!("{}...", l.chars().take(80).collect::<String>())
            } else {
                l
            }
        );
    }
}
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0";
async fn post_mp(
    c: &Client,
    url: String,
    boundary: &str,
    parts: &[PartSpec<'_>],
    note: &str,
) -> Resp {
    call(
        c,
        c.post(url)
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(multipart_body(boundary, parts)),
        note,
    )
    .await
}

#[tokio::main]
async fn main() {
    let dir = std::env::temp_dir().join(format!("cookbook-uploads-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let small = Arc::new(Up {
        dir: dir.clone(),
        limit: 1_000_000,
        n: AtomicU32::new(0),
    });
    let big = Arc::new(Up {
        dir: dir.clone(),
        limit: 8_000_000,
        n: AtomicU32::new(100),
    });
    let app = Router::new()
        .route("/form", post(form_own).layer(DefaultBodyLimit::max(4096)))
        .route("/form-axum", post(form_axum))
        .route("/upload", post(upload))
        .with_state(small.clone())
        .merge(
            Router::new()
                .route("/upload-big", post(upload))
                .with_state(big),
        );
    let srv = Srv::start(app).await;
    let c = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap();
    let url = |p: &str| srv.url(p);
    let files_in_dir = || {
        let mut v: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        v.sort();
        v
    };

    println!("==== 1. application/x-www-form-urlencoded ====");
    let body = "name=Ada+Lovelace&note=100%25+sure&tag=a&tag=b&city=caf%C3%A9&empty=&flag";
    let r = call(
        &c,
        c.post(url("/form"))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(body),
        "own parser: + => space, %XX decoded, repeated keys grouped",
    )
    .await;
    let j = r.json();
    assert_eq!(
        (
            j["name"][0].as_str(),
            j["note"][0].as_str(),
            j["city"][0].as_str()
        ),
        (Some("Ada Lovelace"), Some("100% sure"), Some("café"))
    );
    assert_eq!(j["tag"], json!(["a", "b"]));
    assert_eq!(
        (j["empty"][0].as_str(), j["flag"][0].as_str()),
        (Some(""), Some(""))
    );
    let r2 = call(
        &c,
        c.post(url("/form-axum"))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(body),
        "axum's Form extractor gives the same answer",
    )
    .await;
    assert_eq!(r2.json(), j);
    for (bad, why) in [
        ("a=%zz", "bad hex"),
        ("a=%", "truncated escape"),
        ("a=%C3%28", "invalid UTF-8"),
    ] {
        let r = call(
            &c,
            c.post(url("/form"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(bad),
            &format!("malformed input ({why}) => 400"),
        )
        .await;
        assert_eq!(r.status, 400);
    }
    assert_eq!(
        parse_urlencoded(&format!("k={}", pct_encode_form("a b&c=d/é"))).unwrap(),
        vec![("k".to_string(), "a b&c=d/é".to_string())],
        "encode/decode roundtrip"
    );
    let r = call(
        &c,
        c.post(url("/form"))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(format!("x={}", "a".repeat(10_000))),
        "body limit on the form route (4 KiB) => 413",
    )
    .await;
    assert_eq!(r.status, 413);

    println!("\n==== 2. multipart/form-data: the bytes, the headers, the parse ====");
    let bnd = "----cookbookBoundary7MA4YWxk";
    let parts = [
        PartSpec {
            name: "title",
            filename: None,
            ctype: None,
            data: "Holiday \"2024\"".as_bytes(),
        },
        PartSpec {
            name: "note",
            filename: None,
            ctype: None,
            data: b"line1\r\nline2",
        },
        PartSpec {
            name: "doc",
            filename: Some("my notes (1).txt"),
            ctype: Some("text/plain"),
            data: b"hello from a file\n",
        },
    ];
    show_raw("request body", &multipart_body(bnd, &parts));
    let r = post_mp(
        &c,
        url("/upload"),
        bnd,
        &parts,
        "upload: 2 text fields + 1 file",
    )
    .await;
    let j = r.json();
    assert_eq!(r.status, 200);
    assert_eq!(
        (j["fields"]["title"].as_str(), j["fields"]["note"].as_str()),
        (Some("Holiday \"2024\""), Some("line1\r\nline2"))
    );
    assert_eq!(
        (
            j["files"][0]["filename"].as_str(),
            j["files"][0]["bytes"].as_u64(),
            j["files"][0]["sniffed_type"].as_str()
        ),
        (Some("my_notes__1_.txt"), Some(18), Some("text/plain"))
    );
    let stored = j["files"][0]["stored_as"].as_str().unwrap().to_string();
    assert_eq!(
        std::fs::read(dir.join(&stored)).unwrap(),
        b"hello from a file\n"
    );
    println!(
        "   file stored as `{stored}` (server-chosen), client name kept only as sanitized metadata"
    );

    println!("\n==== 3. the parser is chunking-invariant (delimiter split across chunks) ====");
    let tricky: Vec<u8> =
        b"binary \r\n--Xy not-a-delimiter \r\n--Xyz also not \x00\xff--XyZ end".to_vec();
    let body = multipart_body(
        "XyZ",
        &[
            PartSpec {
                name: "a",
                filename: Some("a.bin"),
                ctype: Some("application/octet-stream"),
                data: &tricky,
            },
            PartSpec {
                name: "b",
                filename: None,
                ctype: None,
                data: b"",
            },
        ],
    );
    let collect = |size: usize| {
        let mut mp = Mp::new("XyZ");
        let (mut evs, mut data) = (vec![], vec![]);
        for ch in body.chunks(size) {
            for e in mp.feed(ch).unwrap() {
                match e {
                    Ev::Data(d) => data.extend(d),
                    other => evs.push(other),
                }
            }
        }
        mp.finish().unwrap();
        (evs, data, mp.max_buf)
    };
    let (ev_all, data_all, _) = collect(body.len());
    assert_eq!(data_all, tricky, "data containing near-delimiters survives");
    for size in [1, 2, 3, 5, 7, 13, 64] {
        let (e, d, max_buf) = collect(size);
        assert_eq!((&e, &d), (&ev_all, &data_all), "chunk size {size}");
        assert!(
            max_buf <= size + 160,
            "buffer bounded by chunk + delimiter/part headers (got {max_buf} for chunk {size})"
        );
    }
    println!(
        "   identical events for chunk sizes 1,2,3,5,7,13,64 and whole-body; buffer <= chunk + (delimiter or one part-header block)"
    );
    assert_eq!(
        ev_all[0],
        Ev::Start(PartInfo {
            name: "a".into(),
            filename: Some("a.bin".into()),
            ctype: Some("application/octet-stream".into()),
            extra: vec![]
        })
    );
    // boundary and header edge cases
    assert_eq!(
        boundary_of("multipart/form-data; boundary=\"a b;c\"").unwrap(),
        "a b;c"
    );
    assert_eq!(
        boundary_of("Multipart/Form-Data; charset=utf-8; BOUNDARY=xyz").unwrap(),
        "xyz"
    );
    assert!(
        boundary_of("multipart/form-data").is_err()
            && boundary_of("text/plain; boundary=x").is_err()
    );
    assert!(boundary_of(&format!("multipart/form-data; boundary={}", "x".repeat(71))).is_err());
    let h = parse_part_headers("content-disposition: form-data; name=\"we\\\"ird\"; filename=\"a;b.txt\"\r\nCONTENT-TYPE: Text/Plain\r\nContent-Transfer-Encoding: binary").unwrap();
    assert_eq!(
        (
            h.name.as_str(),
            h.filename.as_deref(),
            h.ctype.as_deref(),
            h.extra.len()
        ),
        ("we\"ird", Some("a;b.txt"), Some("text/plain"), 1)
    );
    let mut mp = Mp::new("b");
    assert!(
        mp.feed(b"--bXXX").is_err(),
        "garbage after the boundary line"
    );
    let mut mp = Mp::new("b");
    mp.feed(b"preamble text is ignored\r\n--b\r\nContent-Disposition: form-data; name=\"x\"\r\n\r\nval\r\n--b--\r\nepilogue ignored").unwrap();
    mp.finish().unwrap();

    println!("\n==== 4. streaming a 6 MB upload to disk with bounded memory ====");
    const N_CHUNKS: usize = 94;
    let gen_chunk = |i: usize| -> Bytes {
        Bytes::from(
            (0..65_536usize)
                .map(|j| ((i * 7 + j * 13) % 251) as u8)
                .collect::<Vec<u8>>(),
        )
    };
    let (mut want, mut want_len) = (FNV0, 0usize);
    for i in 0..N_CHUNKS {
        let ch = gen_chunk(i);
        want = fnv(want, &ch);
        want_len += ch.len();
    }
    let head = Bytes::from(format!(
        "--B\r\nContent-Disposition: form-data; name=\"movie\"; filename=\"big.png\"\r\nContent-Type: image/png\r\n\r\n"
    ));
    let tail = Bytes::from_static(b"\r\n--B--\r\n");
    // the file is PNG-prefixed so the sniffing policy accepts it as image/png; the rest is binary
    let first = Bytes::from([PNG, &gen_chunk(0)[PNG.len()..]].concat());
    let mut hasher_first = fnv(FNV0, &first);
    let chunks: Vec<Bytes> = std::iter::once(head)
        .chain(std::iter::once(first))
        .chain((1..N_CHUNKS).map(&gen_chunk))
        .chain(std::iter::once(tail))
        .collect();
    for i in 1..N_CHUNKS {
        hasher_first = fnv(hasher_first, &gen_chunk(i));
    }
    let stream = futures_util::stream::iter(chunks.into_iter().map(Ok::<_, std::io::Error>));
    let r = call(
        &c,
        c.post(url("/upload-big"))
            .header("content-type", "multipart/form-data; boundary=B")
            .body(reqwest::Body::wrap_stream(stream)),
        "6 MB body sent as a stream (chunked transfer-encoding)",
    )
    .await;
    let j = r.json();
    assert_eq!(r.status, 200, "{}", r.body);
    let f = &j["files"][0];
    assert_eq!(f["bytes"].as_u64(), Some(want_len as u64));
    assert_eq!(
        f["fnv"].as_str(),
        Some(format!("{hasher_first:016x}").as_str()),
        "bytes on disk match what was sent"
    );
    let on_disk = std::fs::metadata(dir.join(f["stored_as"].as_str().unwrap()))
        .unwrap()
        .len();
    assert_eq!(on_disk as usize, want_len);
    let (mb, mc) = (
        j["max_buffered"].as_u64().unwrap(),
        j["max_network_chunk"].as_u64().unwrap(),
    );
    println!(
        "   upload {} bytes; largest network chunk {mc}; parser buffered at most {mb} bytes ({:.2}% of the upload)",
        want_len,
        100.0 * mb as f64 / want_len as f64
    );
    assert!(
        mb <= mc + 128 && mb < 1_000_000,
        "memory is bounded by one chunk, not by the file size"
    );

    println!("\n==== 5. size limit => 413, partial file removed ====");
    let before = files_in_dir();
    let huge = vec![b'a'; 1_500_000];
    let r = post_mp(
        &c,
        url("/upload"),
        "L",
        &[PartSpec {
            name: "f",
            filename: Some("big.txt"),
            ctype: Some("text/plain"),
            data: &huge,
        }],
        "1.5 MB file against a 1 MB limit",
    )
    .await;
    assert_eq!(r.status, 413);
    assert_eq!(
        files_in_dir(),
        before,
        "the partially written temp file was deleted"
    );

    println!("\n==== 6. hostile filenames ====");
    let before = files_in_dir();
    for evil in [
        "../../etc/passwd",
        "..\\..\\windows\\system32\\cmd.exe",
        "/etc/passwd",
        "a/b.txt",
        "nul\0byte.txt",
        "..",
        "...hidden/../x",
    ] {
        let r = post_mp(
            &c,
            url("/upload"),
            "E",
            &[PartSpec {
                name: "f",
                filename: Some(evil),
                ctype: Some("text/plain"),
                data: b"pwn",
            }],
            &format!("filename {:?}", evil),
        )
        .await;
        assert_eq!(r.status, 400, "{evil:?} must be rejected");
    }
    assert_eq!(
        files_in_dir(),
        before,
        "nothing was written anywhere, nothing escaped {}",
        "the upload directory"
    );
    assert_eq!(
        clean_filename("résumé final.pdf").unwrap(),
        "r_sum__final.pdf"
    );
    assert_eq!(clean_filename(".bashrc").unwrap(), "bashrc");
    assert!(clean_filename("").is_err());

    println!("\n==== 7. sniffing vs trusting the client's Content-Type ====");
    let html = b"<html><script>alert(document.cookie)</script></html>";
    let r = post_mp(
        &c,
        url("/upload"),
        "S",
        &[PartSpec {
            name: "avatar",
            filename: Some("me.png"),
            ctype: Some("image/png"),
            data: html,
        }],
        "HTML disguised as image/png",
    )
    .await;
    assert_eq!(r.status, 415);
    assert!(r.json()["error"].as_str().unwrap().contains("text/html"));
    let r = post_mp(
        &c,
        url("/upload"),
        "S",
        &[PartSpec {
            name: "avatar",
            filename: Some("me.png"),
            ctype: Some("image/png"),
            data: PNG,
        }],
        "a real PNG header",
    )
    .await;
    assert_eq!(
        (r.status, r.json()["files"][0]["sniffed_type"].as_str()),
        (200, Some("image/png"))
    );
    let r = post_mp(
        &c,
        url("/upload"),
        "S",
        &[PartSpec {
            name: "doc",
            filename: Some("run.exe"),
            ctype: Some("text/plain"),
            data: b"MZ\x90\x00\x03\x00",
        }],
        "binary claiming to be text/plain",
    )
    .await;
    assert_eq!(r.status, 415);
    let r = post_mp(
        &c,
        url("/upload?trust=client"),
        "S",
        &[PartSpec {
            name: "avatar",
            filename: Some("me.png"),
            ctype: Some("image/png"),
            data: html,
        }],
        "the NAIVE server (trusts the declared type) accepts the very same payload",
    )
    .await;
    assert_eq!(
        (r.status, r.json()["files"][0]["sniffed_type"].as_str()),
        (200, Some("text/html"))
    );
    println!(
        "   => serve user uploads from another origin with `Content-Disposition: attachment` and `X-Content-Type-Options: nosniff`."
    );

    println!("\n==== 8. malformed multipart ====");
    let r = call(
        &c,
        c.post(url("/upload"))
            .header("content-type", "multipart/form-data")
            .body("x"),
        "no boundary parameter",
    )
    .await;
    assert_eq!(r.status, 400);
    let r = call(
        &c,
        c.post(url("/upload"))
            .header("content-type", "application/json")
            .body("{}"),
        "wrong content type",
    )
    .await;
    assert_eq!(r.status, 400);
    let r = call(&c, c.post(url("/upload")).header("content-type", "multipart/form-data; boundary=Q").body("--Q\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\nvalue but the body just stops"), "truncated body: no closing boundary").await;
    assert_eq!(r.status, 400);
    let r = call(
        &c,
        c.post(url("/upload"))
            .header("content-type", "multipart/form-data; boundary=Q")
            .body(format!("--Q\r\nX-Pad: {}", "a".repeat(9000))),
        "part headers that never end",
    )
    .await;
    assert_eq!(r.status, 400);
    assert!(r.json()["error"].as_str().unwrap().contains("too large"));

    srv.stop().await;
    std::fs::remove_dir_all(&dir).unwrap();
    println!("\nOK: all form / multipart / upload assertions passed");
}
