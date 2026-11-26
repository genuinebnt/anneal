//! RECIPE 300 (web): the anatomy of an HTTP/1.1 exchange: raw bytes on the wire, then the same endpoints with reqwest
//! LEVEL: intermediate
//! Use in: web client/server debugging, API clients.   Run: cargo run --release --example http_client_server_loopback
//!
//! WHAT: a real axum server on 127.0.0.1:0 and two kinds of client. First a raw `TcpStream` writes a hand-typed
//! request and prints the exact response bytes (status line, headers, blank line, body) for both a
//! `Content-Length` body and a `Transfer-Encoding: chunked` body (we decode the chunk framing ourselves).
//! Then reqwest exercises the same server: query strings, JSON and form bodies, custom headers, status classes
//! and `error_for_status`, redirects (301/302/303 turn POST into GET, 307/308 keep method and body), timeouts,
//! streaming, HEAD, OPTIONS/Allow, 404/405/415/400/422.
//! WHY: almost every "HTTP bug" is a misunderstanding of what is actually sent. Printing a `curl -v` style
//! transcript (volatile `date` masked) makes requests and responses visible and testable.
//! HOW: `Server::start` = `axum::serve(listener, app).with_graceful_shutdown(..)`; `try_send` prints the request
//! (`> ...`), executes it, prints the response (`< ...`). 400 = unparseable syntax, 422 = syntactically valid
//! JSON of the wrong shape, 415 = wrong/missing Content-Type, 405 must carry `Allow`.
//! COMMON USES: debugging API clients with curl-style traces; writing contract tests against a real socket;
//! understanding proxies/CDNs (framing: Content-Length vs chunked); choosing correct status codes in REST APIs;
//! redirect handling in HTTP clients (why a POST silently became a GET after a 302).
//! Read: https://developer.mozilla.org/en-US/docs/Web/HTTP , https://datatracker.ietf.org/doc/html/rfc9110 ,
//! https://datatracker.ietf.org/doc/html/rfc9112
#![allow(dead_code)]
use axum::{
    Form, Json, Router,
    body::Body,
    extract::{Path, Query},
    http::{Method, header},
    response::IntoResponse,
    routing::{any, get, post},
};
use futures_util::stream;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, convert::Infallible};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

// ---------------------------------------------------------------- loopback server + transcript helpers
use axum::http::{HeaderMap, StatusCode};
use bytes::Bytes;
use reqwest::{Client, RequestBuilder};
use std::{net::SocketAddr, time::Duration};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};

/// A real axum server on 127.0.0.1:<ephemeral port> with graceful shutdown.
struct Server {
    addr: SocketAddr,
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

impl Server {
    async fn start(app: axum::Router) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await
                .unwrap();
        });
        Server {
            addr,
            stop: Some(tx),
            task,
        }
    }
    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }
    async fn shutdown(mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
        tokio::time::timeout(Duration::from_secs(5), self.task)
            .await
            .expect("graceful shutdown timed out")
            .unwrap();
    }
}

/// One completed exchange (status, headers, full body).
struct Exch {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl Exch {
    /// All values of a header joined with ", " ("" when absent).
    fn h(&self, name: &str) -> String {
        self.headers
            .get_all(name)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect::<Vec<_>>()
            .join(", ")
    }
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// Volatile or secret values are masked so the transcript is deterministic and safe to share.
fn mask(name: &str, value: &str) -> String {
    match name {
        "date" => "<masked>".into(),
        "authorization" | "proxy-authorization" => {
            let scheme = value.split(' ').next().unwrap_or("");
            format!("{scheme} <redacted>")
        }
        "cookie" | "set-cookie" | "x-csrf-token" => redact_tokens(value),
        _ => value.replace(
            &format!(
                "127.0.0.1:{}",
                PORT.load(std::sync::atomic::Ordering::Relaxed)
            ),
            "127.0.0.1:PORT",
        ),
    }
}

/// Random-looking runs of 24+ token characters (session ids, CSRF tokens, signatures) become `<TOKEN>`
/// so transcripts stay deterministic even though the ids are random.
fn redact_tokens(v: &str) -> String {
    let mut out = String::new();
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        out.push_str(&if run.len() >= 24 {
            "<TOKEN>".to_string()
        } else {
            run.clone()
        });
        run.clear();
    };
    for ch in v.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            run.push(ch);
        } else {
            flush(&mut run, &mut out);
            out.push(ch);
        }
    }
    flush(&mut run, &mut out);
    out
}

/// When set, secrets in response bodies (random tokens, JWTs) are replaced by `<TOKEN>` in transcripts.
static REDACT_BODY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static PORT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);

fn preview(b: &[u8]) -> String {
    match std::str::from_utf8(b) {
        Ok(s) => {
            let one = s.replace('\r', "\\r").replace('\n', "\\n");
            if one.chars().count() > 110 {
                format!(
                    "{}... ({} bytes)",
                    one.chars().take(110).collect::<String>(),
                    b.len()
                )
            } else {
                one
            }
        }
        Err(_) => format!(
            "<{} binary bytes, starts {:02x?}>",
            b.len(),
            &b[..b.len().min(4)]
        ),
    }
}

/// Send a request, print a `curl -v` style transcript, and hand the whole exchange back.
async fn try_send(c: &Client, rb: RequestBuilder) -> Result<Exch, reqwest::Error> {
    try_send_req(c, rb.build()?).await
}

async fn try_send_req(c: &Client, req: reqwest::Request) -> Result<Exch, reqwest::Error> {
    PORT.store(
        req.url().port().unwrap_or(0),
        std::sync::atomic::Ordering::Relaxed,
    );
    let pq = match req.url().query() {
        Some(q) => format!("{}?{}", req.url().path(), q),
        None => req.url().path().to_string(),
    };
    println!("> {} {} HTTP/1.1", req.method(), pq);
    println!("> host: 127.0.0.1:PORT");
    for (k, v) in req.headers() {
        println!(
            "> {}: {}",
            k,
            mask(k.as_str(), v.to_str().unwrap_or("<bin>"))
        );
    }
    if let Some(b) = req.body().and_then(|b| b.as_bytes()) {
        println!(">");
        let p = preview(b);
        println!(
            "> {}",
            if REDACT_BODY.load(std::sync::atomic::Ordering::Relaxed) {
                redact_tokens(&p)
            } else {
                p
            }
        );
    }
    let resp = c.execute(req).await?;
    let status = resp.status();
    let headers = resp.headers().clone();
    println!("< {:?} {}", resp.version(), status);
    let mut names: Vec<_> = headers.iter().collect();
    names.sort_by_key(|(k, _)| k.as_str().to_string());
    for (k, v) in names {
        println!(
            "< {}: {}",
            k,
            mask(k.as_str(), v.to_str().unwrap_or("<bin>"))
        );
    }
    let body = resp.bytes().await?;
    if !body.is_empty() {
        println!("<");
        let p = preview(&body);
        println!(
            "< {}",
            if REDACT_BODY.load(std::sync::atomic::Ordering::Relaxed) {
                redact_tokens(&p)
            } else {
                p
            }
        );
    }
    println!();
    Ok(Exch {
        status,
        headers,
        body,
    })
}

async fn send(c: &Client, rb: RequestBuilder) -> Exch {
    try_send(c, rb).await.expect("request failed")
}

fn client() -> Client {
    Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

fn title(s: &str) {
    println!("==== {s}");
}

// ---------------------------------------------------------------- the server under test
#[derive(Deserialize)]
struct NewItem {
    name: String,
    qty: u32,
}

#[derive(Deserialize)]
struct Login {
    user: String,
    note: String,
}

async fn hello() -> &'static str {
    "hello"
}

/// Body from a stream => no known length => `Transfer-Encoding: chunked`.
async fn chunked() -> impl IntoResponse {
    let parts = ["alpha-", "beta-", "gamma"].map(|c| Ok::<_, Infallible>(bytes::Bytes::from(c)));
    Body::from_stream(stream::iter(parts))
}

/// Slow stream: each chunk arrives separately on the client.
async fn slow_stream() -> impl IntoResponse {
    let s = stream::unfold(0u8, |i| async move {
        if i == 4 {
            return None;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        Some((
            Ok::<_, Infallible>(bytes::Bytes::from(format!("part{i};"))),
            i + 1,
        ))
    });
    Body::from_stream(s)
}

async fn search(Query(q): Query<BTreeMap<String, String>>) -> Json<Value> {
    Json(json!({ "query": q }))
}

async fn create_item(Json(i): Json<NewItem>) -> impl IntoResponse {
    (
        StatusCode::CREATED,
        [(header::LOCATION, "/items/1")],
        Json(json!({ "id": 1, "name": i.name, "qty": i.qty })),
    )
}

async fn items_options() -> impl IntoResponse {
    (
        StatusCode::NO_CONTENT,
        [(header::ALLOW, "GET, HEAD, POST, OPTIONS")],
    )
}

async fn form(Form(l): Form<Login>) -> String {
    format!("user={} note={}", l.user, l.note)
}

async fn echo_headers(h: HeaderMap) -> impl IntoResponse {
    let get = |n: &str| {
        h.get(n)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("-")
            .to_string()
    };
    (
        [("x-served-by", "loopback"), ("cache-control", "no-store")],
        Json(
            json!({ "x-trace-id": get("x-trace-id"), "accept": get("accept"), "user-agent": get("user-agent") }),
        ),
    )
}

async fn status(Path(code): Path<u16>) -> impl IntoResponse {
    (
        StatusCode::from_u16(code).unwrap(),
        format!("status {code}"),
    )
}

async fn redirect(Path(code): Path<u16>) -> impl IntoResponse {
    (
        StatusCode::from_u16(code).unwrap(),
        [(header::LOCATION, "/landing")],
    )
}

async fn landing(method: Method, body: String) -> String {
    format!("landed via {method} body={body:?}")
}

async fn slow() -> &'static str {
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    "finally"
}

fn app() -> Router {
    Router::new()
        .route("/hello", get(hello))
        .route("/chunked", get(chunked))
        .route("/stream", get(slow_stream))
        .route("/search", get(search))
        .route(
            "/items",
            get(|| async { Json(json!([])) })
                .post(create_item)
                .options(items_options),
        )
        .route("/form", post(form))
        .route("/headers", get(echo_headers))
        .route("/status/{code}", get(status))
        .route("/redir/{code}", any(redirect))
        .route("/landing", any(landing))
        .route("/slow", get(slow))
}

// ---------------------------------------------------------------- raw TCP client
fn show_wire(prefix: &str, bytes: &[u8]) {
    let s = String::from_utf8_lossy(bytes);
    for line in s.split_inclusive('\n') {
        let line = if line.to_ascii_lowercase().starts_with("date:") {
            "date: <masked>\r\n"
        } else {
            line
        };
        println!(
            "{prefix} {}",
            line.replace('\r', "\\r").replace('\n', "\\n")
        );
    }
}

async fn raw_exchange(addr: std::net::SocketAddr, request: &str) -> Vec<u8> {
    let mut s = TcpStream::connect(addr).await.unwrap();
    show_wire("wire>", request.as_bytes());
    s.write_all(request.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    // `Connection: close` makes the server close after the response, so read_to_end terminates.
    tokio::time::timeout(std::time::Duration::from_secs(5), s.read_to_end(&mut buf))
        .await
        .unwrap()
        .unwrap();
    show_wire("wire<", &buf);
    println!();
    buf
}

fn split_head_body(raw: &[u8]) -> (String, Vec<u8>) {
    let pos = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("blank line separating head from body");
    (
        String::from_utf8(raw[..pos].to_vec()).unwrap(),
        raw[pos + 4..].to_vec(),
    )
}

/// Decode `Transfer-Encoding: chunked`: `<hex size>\r\n<data>\r\n ... 0\r\n\r\n`. Returns (chunk sizes, body).
fn decode_chunked(mut b: &[u8]) -> (Vec<usize>, Vec<u8>) {
    let (mut sizes, mut out) = (vec![], vec![]);
    loop {
        let eol = b.windows(2).position(|w| w == b"\r\n").unwrap();
        let n = usize::from_str_radix(std::str::from_utf8(&b[..eol]).unwrap().trim(), 16).unwrap();
        b = &b[eol + 2..];
        if n == 0 {
            break;
        }
        sizes.push(n);
        out.extend_from_slice(&b[..n]);
        assert_eq!(&b[n..n + 2], b"\r\n", "each chunk ends with CRLF");
        b = &b[n + 2..];
    }
    (sizes, out)
}

async fn raw_section(srv: &Server) {
    title("RAW TCP: what is really on the wire (Content-Length)");
    let raw = raw_exchange(
        srv.addr,
        "GET /hello HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    let (head, body) = split_head_body(&raw);
    let lower = head.to_ascii_lowercase();
    assert!(
        head.starts_with("HTTP/1.1 200 OK\r\n"),
        "status line = version SP code SP reason"
    );
    assert!(lower.contains("content-length: 5"));
    assert!(!lower.contains("transfer-encoding"));
    assert_eq!(body, b"hello");

    title("RAW TCP: chunked framing");
    let raw = raw_exchange(
        srv.addr,
        "GET /chunked HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    let (head, body) = split_head_body(&raw);
    assert!(
        head.to_ascii_lowercase()
            .contains("transfer-encoding: chunked")
    );
    assert!(
        !head.to_ascii_lowercase().contains("content-length"),
        "chunked and Content-Length are mutually exclusive"
    );
    let (sizes, data) = decode_chunked(&body);
    println!(
        "decoded chunk sizes {sizes:?} => {:?}\n",
        String::from_utf8_lossy(&data)
    );
    assert_eq!(sizes, vec![6, 5, 5]);
    assert_eq!(data, b"alpha-beta-gamma");

    title(
        "RAW TCP: a POST with a hand-written body (Content-Length tells the server where the body ends)",
    );
    let body = r#"{"name":"bolt","qty":3}"#;
    let req = format!(
        "POST /items HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let raw = raw_exchange(srv.addr, &req).await;
    let (head, rbody) = split_head_body(&raw);
    assert!(head.starts_with("HTTP/1.1 201 Created"));
    assert!(head.to_ascii_lowercase().contains("location: /items/1"));
    assert_eq!(serde_json::from_slice::<Value>(&rbody).unwrap()["qty"], 3);

    title("RAW TCP: keep-alive: two requests on ONE connection");
    let mut s = TcpStream::connect(srv.addr).await.unwrap();
    for i in 0..2 {
        s.write_all(b"GET /hello HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        let mut buf = vec![0u8; 4096];
        let mut got = 0;
        loop {
            let n =
                tokio::time::timeout(std::time::Duration::from_secs(5), s.read(&mut buf[got..]))
                    .await
                    .unwrap()
                    .unwrap();
            got += n;
            if buf[..got].ends_with(b"hello") || n == 0 {
                break;
            }
        }
        println!(
            "response {i} on same socket: {} bytes, ends with {:?}",
            got,
            String::from_utf8_lossy(&buf[got - 5..got])
        );
        assert!(buf[..got].starts_with(b"HTTP/1.1 200 OK"));
    }
    println!();
}

// ---------------------------------------------------------------- reqwest client
#[tokio::main]
async fn main() {
    let srv = Server::start(app()).await;
    raw_section(&srv).await;

    let c = client();

    title("GET with query parameters (percent/plus encoding is the client's job)");
    let r = send(
        &c,
        c.get(srv.url("/search"))
            .query(&[("q", "rust http"), ("limit", "5"), ("tag", "a&b")]),
    )
    .await;
    assert_eq!(r.status, 200);
    let v: Value = serde_json::from_slice(&r.body).unwrap();
    assert_eq!(v["query"]["q"], "rust http");
    assert_eq!(
        v["query"]["tag"], "a&b",
        "server decodes what the client encoded"
    );
    assert!(r.h("content-type").starts_with("application/json"));

    title("POST JSON: Content-Type declares the body, Accept the desired response");
    let r = send(
        &c,
        c.post(srv.url("/items"))
            .header("accept", "application/json")
            .json(&json!({"name": "bolt", "qty": 3})),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED);
    assert_eq!(r.h("location"), "/items/1");

    title("Bad JSON bodies: 400 (syntax) vs 422 (shape) vs 415 (content type)");
    let r = send(
        &c,
        c.post(srv.url("/items"))
            .header("content-type", "application/json")
            .body(r#"{"name": "bolt","#),
    )
    .await;
    assert_eq!(
        r.status,
        StatusCode::BAD_REQUEST,
        "malformed JSON is a syntax error"
    );
    let r = send(&c, c.post(srv.url("/items")).json(&json!({"name": "bolt"}))).await;
    assert_eq!(
        r.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "valid JSON, missing field qty"
    );
    let r = send(
        &c,
        c.post(srv.url("/items"))
            .header("content-type", "text/plain")
            .body(r#"{"name":"b","qty":1}"#),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);

    title("POST application/x-www-form-urlencoded");
    let r = send(
        &c,
        c.post(srv.url("/form"))
            .form(&[("user", "ann lee"), ("note", "a&b=c")]),
    )
    .await;
    assert_eq!(r.status, 200);
    assert_eq!(r.text(), "user=ann lee note=a&b=c");

    title("Custom request headers and response headers");
    let r = send(
        &c,
        c.get(srv.url("/headers"))
            .header("x-trace-id", "t-42")
            .header("accept", "application/json")
            .header("user-agent", "cookbook/1.0"),
    )
    .await;
    let v: Value = serde_json::from_slice(&r.body).unwrap();
    assert_eq!(v["x-trace-id"], "t-42");
    assert_eq!(v["user-agent"], "cookbook/1.0");
    assert_eq!(r.h("x-served-by"), "loopback");
    assert_eq!(r.h("cache-control"), "no-store");

    title("Status classes: reqwest does NOT fail on 4xx/5xx by itself; error_for_status() opts in");
    for (code, class) in [
        (200u16, "2xx success"),
        (204, "2xx success"),
        (404, "4xx client error"),
        (500, "5xx server error"),
        (503, "5xx server error"),
    ] {
        let resp = c
            .get(srv.url(&format!("/status/{code}")))
            .send()
            .await
            .unwrap();
        let st = resp.status();
        let outcome = match resp.error_for_status() {
            Ok(_) => "Ok(response)".to_string(),
            Err(e) => format!(
                "Err(is_status={}, status={:?})",
                e.is_status(),
                e.status().map(|s| s.as_u16())
            ),
        };
        println!(
            "GET /status/{code:<3} -> {class:<17} client_error={} server_error={}  error_for_status => {outcome}",
            st.is_client_error(),
            st.is_server_error()
        );
        assert_eq!(
            st.as_u16(),
            code,
            "the send() itself succeeded for every status"
        );
        assert_eq!(outcome.starts_with("Err"), code >= 400);
    }
    println!();

    title("Redirects: 301/302/303 rewrite POST to GET and drop the body; 307/308 keep both");
    let follow = client();
    let nofollow = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let r = send(
        &nofollow,
        nofollow.post(srv.url("/redir/307")).body("payload"),
    )
    .await;
    assert_eq!(r.status, 307);
    assert_eq!(r.h("location"), "/landing");
    for (code, expect) in [
        (301, "landed via GET body=\"\""),
        (302, "landed via GET body=\"\""),
        (303, "landed via GET body=\"\""),
        (307, "landed via POST body=\"payload\""),
        (308, "landed via POST body=\"payload\""),
    ] {
        let resp = follow
            .post(srv.url(&format!("/redir/{code}")))
            .body("payload")
            .send()
            .await
            .unwrap();
        let (url_path, st) = (resp.url().path().to_string(), resp.status());
        let text = resp.text().await.unwrap();
        println!("POST /redir/{code} -> followed to {url_path} ({st}): {text}");
        assert_eq!((url_path.as_str(), text.as_str()), ("/landing", expect));
    }
    println!();
    let r = send(&nofollow, nofollow.get(srv.url("/redir/301"))).await;
    assert_eq!(r.status, StatusCode::MOVED_PERMANENTLY);
    let limited = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::limited(0))
        .build()
        .unwrap();
    let err = limited.get(srv.url("/redir/302")).send().await.unwrap_err();
    println!(
        "redirect limit 0 => error: is_redirect={}\n",
        err.is_redirect()
    );
    assert!(err.is_redirect());

    title("Timeouts: a per-request timeout fails fast instead of hanging");
    let started = std::time::Instant::now();
    let e = try_send(
        &c,
        c.get(srv.url("/slow"))
            .timeout(std::time::Duration::from_millis(150)),
    )
    .await
    .err()
    .expect("must time out");
    println!(
        "request aborted client-side: is_timeout={}\n",
        e.is_timeout()
    );
    assert!(e.is_timeout());
    assert!(started.elapsed() < std::time::Duration::from_millis(1200));

    title("Streaming a response body chunk by chunk");
    let mut resp = c.get(srv.url("/stream")).send().await.unwrap();
    assert!(
        resp.headers().get("content-length").is_none(),
        "streamed: length unknown up front"
    );
    let mut chunks = vec![];
    while let Some(ch) = resp.chunk().await.unwrap() {
        println!("  got chunk {:?}", String::from_utf8_lossy(&ch));
        chunks.push(ch);
    }
    println!();
    assert!(
        chunks.len() >= 2,
        "arrived in several pieces, not one buffered blob"
    );
    assert_eq!(chunks.concat(), b"part0;part1;part2;part3;");

    title("HEAD: headers identical to GET (including Content-Length), no body");
    let g = send(&c, c.get(srv.url("/hello"))).await;
    let h = send(&c, c.head(srv.url("/hello"))).await;
    assert_eq!(h.status, 200);
    assert_eq!(h.h("content-length"), g.h("content-length"));
    assert!(h.body.is_empty());

    title("OPTIONS and 405 Method Not Allowed (must list Allow)");
    let o = send(&c, c.request(Method::OPTIONS, srv.url("/items"))).await;
    assert_eq!(o.status, StatusCode::NO_CONTENT);
    assert!(o.h("allow").contains("POST") && o.h("allow").contains("OPTIONS"));
    let d = send(&c, c.delete(srv.url("/hello"))).await;
    assert_eq!(d.status, StatusCode::METHOD_NOT_ALLOWED);
    assert!(
        d.h("allow").contains("GET"),
        "405 responses carry Allow (RFC 9110 15.5.6)"
    );

    title("404 Not Found");
    let n = send(&c, c.get(srv.url("/no/such/thing"))).await;
    assert_eq!(n.status, StatusCode::NOT_FOUND);

    drop(c);
    drop(follow);
    srv.shutdown().await;
    println!("all HTTP anatomy assertions passed");
}
