//! RECIPE 301 (web): HTTP caching and conditional requests: ETag, Last-Modified, Cache-Control, Vary, Range, If-Match
//! LEVEL: advanced
//! Use in: REST/CDN-facing APIs, download clients, optimistic concurrency.   Run: cargo run --release --example http_caching_conditional
//!
//! WHAT: an axum server that implements the validators and preconditions of RFC 9110/9111 and a client that
//! exploits them. Strong `ETag` = quoted hash of the body; `If-None-Match` hit => `304 Not Modified` with no
//! body; `Last-Modified`/`If-Modified-Since` as the weaker date validator. A tiny client-side cache (injectable
//! clock) implements FRESHNESS (`max-age`) and REVALIDATION (`no-cache`, `must-revalidate`) and we assert the
//! exact number of network hits per scenario. `Vary` + `Accept`/`Accept-Encoding` negotiation (gzip via
//! tower-http), `Range` requests (206, 416, multi-range described), a resumable download that reconnects with
//! `Range: bytes=N-` + `If-Range` after the server kills the connection, and `If-Match` (412 on lost update).
//! WHY: the cheapest request is the one never sent, the second cheapest is a 304. Preconditions also give
//! optimistic concurrency without locks.
//! HOW: `no-cache` means "store it but ask before reuse"; `no-store` means "never keep it"; `immutable` says
//! a client need not revalidate while the response is fresh; `private` forbids shared caches (CDNs); `public` allows them. A response that
//! varies by a request header MUST say so in `Vary`, otherwise a shared cache serves the wrong variant.
//! Multi-range (`bytes=0-9,20-29`) would be answered as `multipart/byteranges`; servers may ignore it and send
//! 200 (we do), which is always legal. Dates are compared as parsed epoch seconds, never as strings.
//! COMMON USES: CDN/browser caching of APIs and assets (fingerprinted files with `immutable`), conditional
//! polling of feeds/config endpoints to save bandwidth, resumable large downloads (package managers, model
//! weights, video seeking), optimistic locking on PUT (`If-Match`), create-only PUT (`If-None-Match: *`).
//! Read: https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching , https://datatracker.ietf.org/doc/html/rfc9111 ,
//! https://datatracker.ietf.org/doc/html/rfc9110 , https://developer.mozilla.org/en-US/docs/Web/HTTP
#![allow(dead_code)]
use axum::{
    Router,
    body::Body,
    extract::{Path, State},
    http::header,
    response::{IntoResponse, Response},
    routing::{get, put},
};
use futures_util::stream;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tower_http::compression::CompressionLayer;

// ---------------------------------------------------------------- loopback server + transcript helpers
use axum::http::{HeaderMap, Method, StatusCode};
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

// ---------------------------------------------------------------- SHA-256 and hex (from scratch, only to make ETags)
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut st: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..64 {
            w[i] = if i < 16 {
                u32::from_be_bytes(block[4 * i..4 * i + 4].try_into().unwrap())
            } else {
                let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
                let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
                w[i - 16]
                    .wrapping_add(s0)
                    .wrapping_add(w[i - 7])
                    .wrapping_add(s1)
            };
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = st;
        for i in 0..64 {
            let t1 = h
                .wrapping_add(e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25))
                .wrapping_add((e & f) ^ (!e & g))
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let t2 = (a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22))
                .wrapping_add((a & b) ^ (a & c) ^ (b & c));
            (h, g, f, e, d, c, b, a) = (g, f, e, d.wrapping_add(t1), c, b, a, t1.wrapping_add(t2));
        }
        for (s, v) in st.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *s = s.wrapping_add(v);
        }
    }
    let mut out = [0u8; 32];
    for (i, s) in st.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&s.to_be_bytes());
    }
    out
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// ---------------------------------------------------------------- validators
/// Strong ETag: quoted, hex of the first 8 bytes of SHA-256(body). Same bytes => same tag.
fn etag_of(body: &[u8]) -> String {
    format!("\"{}\"", hex(&sha256(body)[..8]))
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
}

/// Parse IMF-fixdate ("Sun, 06 Nov 1994 08:49:37 GMT") into epoch seconds.
fn parse_http_date(s: &str) -> Option<i64> {
    let p: Vec<&str> = s.split_whitespace().collect();
    if p.len() != 6 || p[5] != "GMT" {
        return None;
    }
    let mon = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|m| *m == p[2])? as i64
        + 1;
    // fixed widths and ranges: the client controls this text, and a huge year would overflow the arithmetic
    let field = |s: &str, width: usize, max: i64| -> Option<i64> {
        let digits = s.len() == width && s.bytes().all(|b| b.is_ascii_digit());
        s.parse().ok().filter(|&n| digits && n <= max)
    };
    let (day, year) = (field(p[1], 2, 31)?, field(p[3], 4, 9999)?);
    let mut time = p[4].split(':');
    let hour = field(time.next()?, 2, 23)?;
    let minute = field(time.next()?, 2, 59)?;
    let second = field(time.next()?, 2, 60)?;
    if day == 0 || time.next().is_some() {
        return None;
    }
    Some(days_from_civil(year, mon, day) * 86400 + hour * 3600 + minute * 60 + second)
}

/// Weak comparison (RFC 9110 8.8.3.2) for If-None-Match: ignore the `W/` prefix; `*` matches anything.
fn inm_matches(header_value: &str, etag: &str) -> bool {
    header_value
        .split(',')
        .map(str::trim)
        .any(|t| t == "*" || t.strip_prefix("W/").unwrap_or(t) == etag)
}

/// Strong comparison for If-Match: weak tags never match.
fn im_matches(header_value: &str, etag: &str) -> bool {
    header_value
        .split(',')
        .map(str::trim)
        .any(|t| t == "*" || (t == etag && !t.starts_with("W/")))
}

/// Should we answer 304? If-None-Match wins over If-Modified-Since when both are present.
fn not_modified(h: &HeaderMap, etag: &str, last_modified: Option<&str>) -> bool {
    if let Some(v) = h.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) {
        return inm_matches(v, etag);
    }
    match (
        h.get(header::IF_MODIFIED_SINCE)
            .and_then(|v| v.to_str().ok()),
        last_modified,
    ) {
        (Some(ims), Some(lm)) => {
            matches!((parse_http_date(ims), parse_http_date(lm)), (Some(a), Some(b)) if a >= b)
        }
        _ => false,
    }
}

// ---------------------------------------------------------------- server
const DOC: &str = "The quick brown fox jumps over the lazy dog. Caching makes the web fast.";
const LAST_MODIFIED: &str = "Sun, 06 Nov 1994 08:49:37 GMT";
const BLOB_LEN: usize = 10_000;
const FLAKY_CHUNK: usize = 3_500;
const MAX_NOTES: usize = 100; // axum limits one body (2 MB); nothing else limits how many we keep

fn blob() -> Vec<u8> {
    (0..BLOB_LEN)
        .map(|i| ((i * 7 + i / 256) % 251) as u8)
        .collect()
}

#[derive(Default)]
struct S {
    hits: Mutex<BTreeMap<String, u32>>,
    notes: Mutex<HashMap<String, String>>,
}

impl S {
    fn hit(&self, k: &str) {
        *self.hits.lock().unwrap().entry(k.to_string()).or_default() += 1;
    }
    fn hits(&self, k: &str) -> u32 {
        *self.hits.lock().unwrap().get(k).unwrap_or(&0)
    }
}

type St = State<Arc<S>>;

async fn doc(State(s): St, h: HeaderMap) -> Response {
    s.hit("doc");
    let etag = etag_of(DOC.as_bytes());
    let common = [
        (header::ETAG, etag.clone()),
        (header::LAST_MODIFIED, LAST_MODIFIED.to_string()),
        (header::CACHE_CONTROL, "public, max-age=60".to_string()),
    ];
    if not_modified(&h, &etag, Some(LAST_MODIFIED)) {
        return (StatusCode::NOT_MODIFIED, common).into_response();
    }
    (common, DOC).into_response()
}

async fn cc(State(s): St, Path(kind): Path<String>, h: HeaderMap) -> Response {
    s.hit(&kind);
    let directive = match kind.as_str() {
        "public" => "public, max-age=3600",
        "private" => "private, max-age=60",
        "max-age" => "max-age=60",
        "no-cache" => "no-cache",
        "no-store" => "no-store",
        "immutable" => "public, max-age=31536000, immutable",
        "must-revalidate" => "max-age=30, must-revalidate",
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    let body = format!("resource {kind} v1");
    let etag = etag_of(body.as_bytes());
    let hdrs = [
        (header::ETAG, etag.clone()),
        (header::CACHE_CONTROL, directive.to_string()),
    ];
    if kind != "no-store" && not_modified(&h, &etag, None) {
        return (StatusCode::NOT_MODIFIED, hdrs).into_response();
    }
    (hdrs, body).into_response()
}

/// Content negotiation: the representation depends on `Accept`, so the response says `Vary: Accept`.
async fn negotiate(h: HeaderMap) -> Response {
    let accept = h
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("*/*");
    let (ct, body) = if accept.contains("application/json") {
        (
            "application/json",
            format!(
                "{{\"items\":[{}]}}",
                vec!["\"negotiation-is-fun\""; 80].join(",")
            ),
        )
    } else if accept.contains("text/plain") || accept.contains("*/*") {
        ("text/plain", "negotiation is fun\n".repeat(120))
    } else {
        return (
            StatusCode::NOT_ACCEPTABLE,
            "supported: application/json, text/plain",
        )
            .into_response();
    };
    ([(header::CONTENT_TYPE, ct), (header::VARY, "Accept")], body).into_response()
}

enum Rng {
    Full,
    Part(usize, usize),
    Unsatisfiable,
}

/// Parse a single `bytes=` range. Multi-range or malformed => ignore the header (legal: answer 200).
fn parse_range(v: &str, total: usize) -> Rng {
    let Some((a, b)) = v
        .trim()
        .strip_prefix("bytes=")
        .and_then(|spec| spec.split_once('-'))
    else {
        return Rng::Full;
    };
    // Digits only (`str::parse` also accepts "+5"). A number too big for `usize` is still a very large number.
    // A comma (multi-range) fails this test, so we answer such a request with the whole body.
    let number = |s: &str| {
        let digits = !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit());
        digits.then(|| s.parse::<usize>().unwrap_or(usize::MAX))
    };
    match (number(a), number(b)) {
        // the last n bytes; an empty body has none, so `total - 1` below cannot underflow
        (None, Some(n)) if a.is_empty() => {
            if n == 0 || total == 0 {
                Rng::Unsatisfiable
            } else {
                Rng::Part(total.saturating_sub(n), total - 1)
            }
        }
        (Some(a), Some(b)) if b < a => Rng::Full,
        (Some(a), last) if last.is_some() || b.is_empty() => {
            if a >= total {
                Rng::Unsatisfiable
            } else {
                Rng::Part(a, last.unwrap_or(usize::MAX).min(total - 1))
            }
        }
        _ => Rng::Full,
    }
}

fn serve_blob(method: &Method, h: &HeaderMap, flaky: bool) -> Response {
    let data = blob();
    let etag = etag_of(&data);
    // "Do you still need it at all?" comes before "which part?"
    if not_modified(h, &etag, None) {
        return (StatusCode::NOT_MODIFIED, [(header::ETAG, etag)]).into_response();
    }
    // If-Range: only honour Range when the validator still matches, else send the whole new thing.
    let range_ok = h
        .get(header::IF_RANGE)
        .and_then(|v| v.to_str().ok())
        .is_none_or(|v| v == etag);
    let rng = match h.get(header::RANGE).and_then(|v| v.to_str().ok()) {
        // Range means something only for GET (axum also sends HEAD to a GET route)
        Some(r) if range_ok && method == Method::GET => parse_range(r, data.len()),
        _ => Rng::Full,
    };
    let base = [
        (header::ACCEPT_RANGES, "bytes".to_string()),
        (header::ETAG, etag),
        (
            header::HeaderName::from_static("x-sha256"),
            hex(&sha256(&data)),
        ),
    ];
    let (status, start, end) = match rng {
        Rng::Unsatisfiable => {
            return (
                StatusCode::RANGE_NOT_SATISFIABLE,
                base,
                [(header::CONTENT_RANGE, format!("bytes */{}", data.len()))],
            )
                .into_response();
        }
        Rng::Part(a, b) => (StatusCode::PARTIAL_CONTENT, a, b),
        Rng::Full => (StatusCode::OK, 0, data.len() - 1),
    };
    let slice = data[start..=end].to_vec();
    let cr = (status == StatusCode::PARTIAL_CONTENT).then(|| {
        (
            header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{}", data.len()),
        )
    });
    if flaky && slice.len() > FLAKY_CHUNK {
        // Promise the full length, deliver a prefix, then break the connection mid-body.
        let first = bytes::Bytes::copy_from_slice(&slice[..FLAKY_CHUNK]);
        // The pause lets the headers + first bytes reach the client before the break.
        let broken = async {
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            Err::<bytes::Bytes, _>(std::io::Error::other("connection reset"))
        };
        let s = futures_util::StreamExt::chain(stream::iter(vec![Ok(first)]), stream::once(broken));
        let mut resp = (
            status,
            base,
            [(header::CONTENT_LENGTH, slice.len().to_string())],
            Body::from_stream(s),
        )
            .into_response();
        if let Some((k, v)) = cr {
            resp.headers_mut().insert(k, v.parse().unwrap());
        }
        return resp;
    }
    let mut resp = (status, base, slice).into_response();
    if let Some((k, v)) = cr {
        resp.headers_mut().insert(k, v.parse().unwrap());
    }
    resp
}

async fn blob_h(method: Method, h: HeaderMap) -> Response {
    serve_blob(&method, &h, false)
}
async fn flaky_h(State(s): St, method: Method, h: HeaderMap) -> Response {
    s.hit("flaky");
    serve_blob(&method, &h, true)
}

/// PUT with preconditions: missing If-Match => 428; mismatch => 412 (lost-update protection).
async fn put_note(State(s): St, Path(name): Path<String>, h: HeaderMap, body: String) -> Response {
    let mut notes = s.notes.lock().unwrap();
    let current = notes.get(&name).map(|b| etag_of(b.as_bytes()));
    let im = h.get(header::IF_MATCH).and_then(|v| v.to_str().ok());
    let inm = h.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok());
    let created = current.is_none();
    let failed = |why: &'static str| (StatusCode::PRECONDITION_FAILED, why).into_response();
    // 1. If-Match must name the version that is stored now
    match (im, &current) {
        (Some(_), None) => return failed("no such resource"),
        (Some(v), Some(cur)) if !im_matches(v, cur) => {
            return failed("resource changed since you read it");
        }
        _ => {}
    }
    // 2. If-None-Match must NOT match it (`*` matches any stored version)
    if let (Some(v), Some(cur)) = (inm, &current)
        && inm_matches(v, cur)
    {
        return failed("a matching version exists");
    }
    // 3. our own rule, checked whatever else was sent: replacing a note needs If-Match
    if im.is_none() && !created {
        return (StatusCode::PRECONDITION_REQUIRED, "send If-Match").into_response();
    }
    if created && notes.len() >= MAX_NOTES {
        return (StatusCode::INSUFFICIENT_STORAGE, "too many notes").into_response();
    }
    let etag = etag_of(body.as_bytes());
    notes.insert(name, body);
    (
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        [(header::ETAG, etag)],
    )
        .into_response()
}

async fn get_note(State(s): St, Path(name): Path<String>) -> Response {
    match s.notes.lock().unwrap().get(&name) {
        Some(b) => ([(header::ETAG, etag_of(b.as_bytes()))], b.clone()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn app(s: Arc<S>) -> Router {
    let plain = Router::new()
        .route("/doc", get(doc))
        .route("/cc/{kind}", get(cc))
        .route("/blob", get(blob_h))
        .route("/flaky", get(flaky_h))
        .route("/notes/{name}", put(put_note).get(get_note))
        .with_state(s);
    // Only this route is compressed, so Range/ETag behaviour of the others stays byte-exact.
    let negotiated = Router::new()
        .route("/negotiate", get(negotiate))
        .layer(CompressionLayer::new());
    plain.merge(negotiated)
}

// ---------------------------------------------------------------- client-side cache
struct Entry {
    etag: Option<String>,
    body: bytes::Bytes,
    stored_at: u64,
    max_age: u64,
    no_cache: bool,
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Src {
    Fresh,
    Revalidated,
    Network,
}

#[derive(Default, Debug, PartialEq)]
struct Cc {
    max_age: Option<u64>,
    no_cache: bool,
    no_store: bool,
    private: bool,
    public: bool,
    immutable: bool,
    must_revalidate: bool,
}

fn parse_cc(v: &str) -> Cc {
    let mut cc = Cc::default();
    for d in v.split(',').map(|d| d.trim().to_ascii_lowercase()) {
        match d.split_once('=').unwrap_or((d.as_str(), "")) {
            ("max-age", n) => cc.max_age = n.parse().ok(),
            ("no-cache", _) => cc.no_cache = true,
            ("no-store", _) => cc.no_store = true,
            ("private", _) => cc.private = true,
            ("public", _) => cc.public = true,
            ("immutable", _) => cc.immutable = true,
            ("must-revalidate", _) => cc.must_revalidate = true,
            _ => {}
        }
    }
    cc
}

/// A private (browser-like) cache with an injectable clock (seconds).
struct HttpCache {
    c: reqwest::Client,
    now: Arc<AtomicU64>,
    map: HashMap<String, Entry>,
}

impl HttpCache {
    async fn get(&mut self, url: &str) -> (bytes::Bytes, Src) {
        let now = self.now.load(Ordering::SeqCst);
        if let Some(e) = self.map.get(url) {
            if !e.no_cache && now.saturating_sub(e.stored_at) < e.max_age {
                return (e.body.clone(), Src::Fresh); // zero network traffic
            }
        }
        let mut rb = self.c.get(url);
        if let Some(t) = self.map.get(url).and_then(|e| e.etag.clone()) {
            rb = rb.header("if-none-match", t);
        }
        let resp = rb.send().await.unwrap();
        let cc = parse_cc(
            resp.headers()
                .get("cache-control")
                .and_then(|v| v.to_str().ok())
                .unwrap_or(""),
        );
        let etag = resp
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(String::from);
        if resp.status() == 304 {
            let e = self.map.get_mut(url).expect("304 without a stored entry");
            e.stored_at = now;
            e.max_age = cc.max_age.unwrap_or(e.max_age);
            e.no_cache = cc.no_cache;
            return (e.body.clone(), Src::Revalidated);
        }
        let ok = resp.status() == 200;
        let body = resp.bytes().await.unwrap();
        // this small cache keeps only 200 responses: an error page must not become "the resource"
        if cc.no_store || !ok {
            self.map.remove(url);
        } else {
            let max_age = cc.max_age.unwrap_or(0);
            self.map.insert(
                url.to_string(),
                Entry {
                    etag,
                    body: body.clone(),
                    stored_at: now,
                    max_age,
                    no_cache: cc.no_cache,
                },
            );
        }
        (body, Src::Network)
    }
}

// ---------------------------------------------------------------- main
fn plain_client() -> reqwest::Client {
    // gzip(false): reqwest would otherwise send Accept-Encoding and transparently decode, hiding the wire format.
    reqwest::Client::builder()
        .no_proxy()
        .gzip(false)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap()
}

#[tokio::main]
async fn main() {
    let state = Arc::new(S::default());
    let srv = Server::start(app(state.clone())).await;
    let c = plain_client();

    // SHA-256 of "abc", the published test value: the ETags are only as good as the hash
    assert_eq!(
        hex(&sha256(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    // client-controlled text: bad dates are refused, not computed with
    for bad in [
        "Sun, 06 Nov 9223372036854775807 08:49:37 GMT",
        "Sun, 06 Nov 1994 9223372036854775807:49:37 GMT",
        "Sun, 00 Nov 1994 08:49:37 GMT",
        "Sun, 06 Nov 1994 08:49:37:00 GMT",
    ] {
        assert_eq!(parse_http_date(bad), None, "{bad}");
    }
    assert!(
        !inm_matches("W/W/\"x\"", "\"x\""),
        "one weak prefix, not many"
    );
    // ranges at the edges: an empty body, a sign, numbers too big to fit
    let range = |v: String, total| match parse_range(&v, total) {
        Rng::Full => (200, 0, 0),
        Rng::Part(a, b) => (206, a, b),
        Rng::Unsatisfiable => (416, 0, 0),
    };
    assert_eq!(
        range("bytes=-5".into(), 0),
        (416, 0, 0),
        "empty body, no underflow"
    );
    assert_eq!(range("bytes=0-".into(), 0), (416, 0, 0));
    assert_eq!(
        range("bytes=+5-9".into(), 100),
        (200, 0, 0),
        "a sign is not a digit"
    );
    assert_eq!(range("bytes= 5 - 9".into(), 100), (200, 0, 0));
    assert_eq!(
        range("bytes=20000-abc".into(), 100),
        (200, 0, 0),
        "malformed is ignored"
    );
    let huge = "99999999999999999999999";
    assert_eq!(range(format!("bytes=5-{huge}"), 100), (206, 5, 99));
    assert_eq!(range(format!("bytes={huge}-"), 100), (416, 0, 0));
    assert_eq!(range(format!("bytes=-{huge}"), 100), (206, 0, 99));

    title("1. strong ETag, If-None-Match -> 304 (no body), Last-Modified / If-Modified-Since");
    let etag = etag_of(DOC.as_bytes());
    assert_eq!(etag, etag_of(DOC.as_bytes()), "deterministic");
    assert_ne!(
        etag,
        etag_of(b"another body"),
        "content change => tag change"
    );
    let r = send(&c, c.get(srv.url("/doc"))).await;
    assert_eq!((r.status.as_u16(), r.h("etag")), (200, etag.clone()));
    let r = send(&c, c.get(srv.url("/doc")).header("if-none-match", &etag)).await;
    assert_eq!(r.status.as_u16(), 304);
    assert!(r.body.is_empty(), "304 has no body");
    assert_eq!(r.h("etag"), etag, "304 repeats the validators");
    let r = send(
        &c,
        c.get(srv.url("/doc")).header("if-none-match", "\"nope\""),
    )
    .await;
    assert_eq!(r.status.as_u16(), 200);
    let r = send(
        &c,
        c.get(srv.url("/doc"))
            .header("if-modified-since", "Mon, 07 Nov 1994 00:00:00 GMT"),
    )
    .await;
    assert_eq!(
        r.status.as_u16(),
        304,
        "resource not modified since a later date"
    );
    let r = send(
        &c,
        c.get(srv.url("/doc"))
            .header("if-modified-since", "Sat, 05 Nov 1994 00:00:00 GMT"),
    )
    .await;
    assert_eq!(
        r.status.as_u16(),
        200,
        "resource modified after the given date"
    );
    let r = send(
        &c,
        c.get(srv.url("/doc"))
            .header("if-none-match", "\"nope\"")
            .header("if-modified-since", "Mon, 07 Nov 1994 00:00:00 GMT"),
    )
    .await;
    assert_eq!(
        r.status.as_u16(),
        200,
        "If-None-Match takes precedence over If-Modified-Since"
    );
    assert_eq!(
        parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT"),
        Some(784111777)
    );

    title("2. Cache-Control directives parsed");
    assert_eq!(parse_cc("public, max-age=3600").max_age, Some(3600));
    assert!(parse_cc("no-cache").no_cache && !parse_cc("no-cache").no_store);
    assert!(parse_cc("no-store").no_store);
    assert!(parse_cc("public, max-age=31536000, immutable").immutable);
    assert!(parse_cc("private, max-age=60").private && !parse_cc("private, max-age=60").public);
    assert!(parse_cc("max-age=30, must-revalidate").must_revalidate);
    println!(
        "no-cache: store, but revalidate every time | no-store: never keep | immutable: never revalidate\n"
    );

    title("3. client-side cache: freshness + revalidation with an injectable clock");
    let now = Arc::new(AtomicU64::new(1_000));
    let mut cache = HttpCache {
        c: plain_client(),
        now: now.clone(),
        map: HashMap::new(),
    };
    let step = |label: &str, src: Src, expect: Src, hits: u32, actual_hits: u32| {
        println!("{label:<52} -> {src:?} (server hits so far: {actual_hits})");
        assert_eq!(src, expect, "{label}");
        assert_eq!(actual_hits, hits, "{label}: network hits");
    };
    let u = srv.url("/cc/max-age");
    let (b, s) = cache.get(&u).await;
    assert_eq!(b, "resource max-age v1");
    step(
        "max-age=60 first GET (cold)",
        s,
        Src::Network,
        1,
        state.hits("max-age"),
    );
    let (_, s) = cache.get(&u).await;
    step(
        "again immediately (fresh, no network)",
        s,
        Src::Fresh,
        1,
        state.hits("max-age"),
    );
    now.fetch_add(59, Ordering::SeqCst);
    let (_, s) = cache.get(&u).await;
    step("+59s still fresh", s, Src::Fresh, 1, state.hits("max-age"));
    now.fetch_add(2, Ordering::SeqCst);
    let (b, s) = cache.get(&u).await;
    assert_eq!(b, "resource max-age v1", "304 served the cached body");
    step(
        "+61s stale: conditional GET -> 304",
        s,
        Src::Revalidated,
        2,
        state.hits("max-age"),
    );
    let (_, s) = cache.get(&u).await;
    step(
        "fresh again after revalidation",
        s,
        Src::Fresh,
        2,
        state.hits("max-age"),
    );

    let u = srv.url("/cc/no-cache");
    let (_, s1) = cache.get(&u).await;
    let (_, s2) = cache.get(&u).await;
    let (_, s3) = cache.get(&u).await;
    assert_eq!(
        (s1, s2, s3),
        (Src::Network, Src::Revalidated, Src::Revalidated)
    );
    step(
        "no-cache x3: 1 full + 2 conditional",
        s3,
        Src::Revalidated,
        3,
        state.hits("no-cache"),
    );

    let u = srv.url("/cc/no-store");
    let (_, s1) = cache.get(&u).await;
    let (_, s2) = cache.get(&u).await;
    assert_eq!((s1, s2), (Src::Network, Src::Network));
    step(
        "no-store x2: both full downloads, nothing stored",
        s2,
        Src::Network,
        2,
        state.hits("no-store"),
    );
    assert!(!cache.map.contains_key(&u));

    let u = srv.url("/cc/immutable");
    cache.get(&u).await;
    now.fetch_add(86_400 * 30, Ordering::SeqCst);
    let (_, s) = cache.get(&u).await;
    step(
        "immutable, 30 days later: still fresh",
        s,
        Src::Fresh,
        1,
        state.hits("immutable"),
    );
    let u = srv.url("/cc/unknown");
    cache.get(&u).await;
    assert!(!cache.map.contains_key(&u), "a 404 is not stored");
    println!();

    title("4. Vary + content negotiation + gzip (tower-http CompressionLayer)");
    let j = send(
        &c,
        c.get(srv.url("/negotiate"))
            .header("accept", "application/json"),
    )
    .await;
    let t = send(
        &c,
        c.get(srv.url("/negotiate")).header("accept", "text/plain"),
    )
    .await;
    assert_eq!(j.h("content-type"), "application/json");
    assert_eq!(t.h("content-type"), "text/plain");
    assert!(
        j.h("vary").to_ascii_lowercase().contains("accept"),
        "same URL, different bodies => Vary: Accept"
    );
    assert_ne!(
        j.body, t.body,
        "a cache keyed by URL alone would serve the wrong variant"
    );
    let n = send(
        &c,
        c.get(srv.url("/negotiate")).header("accept", "image/png"),
    )
    .await;
    assert_eq!(n.status.as_u16(), 406);
    let gz = send(
        &c,
        c.get(srv.url("/negotiate"))
            .header("accept", "text/plain")
            .header("accept-encoding", "gzip"),
    )
    .await;
    assert_eq!(gz.h("content-encoding"), "gzip");
    assert!(gz.body.starts_with(&[0x1f, 0x8b]), "gzip magic number");
    assert!(
        gz.body.len() < t.body.len() / 5,
        "highly repetitive text compresses well"
    );
    assert!(
        gz.h("vary")
            .to_ascii_lowercase()
            .contains("accept-encoding"),
        "encoding choice is a Vary dimension"
    );
    let auto = reqwest::Client::builder()
        .no_proxy()
        .gzip(true)
        .build()
        .unwrap(); // sends Accept-Encoding, decodes
    let decoded = auto
        .get(srv.url("/negotiate"))
        .header("accept", "text/plain")
        .send()
        .await
        .unwrap();
    assert!(
        decoded.headers().get("content-encoding").is_none(),
        "reqwest strips the header after decoding"
    );
    assert_eq!(
        decoded.bytes().await.unwrap(),
        t.body,
        "decoded gzip body equals the identity body"
    );
    println!(
        "gzip: {} bytes on the wire vs {} identity; decoded body identical\n",
        gz.body.len(),
        t.body.len()
    );

    title("5. Range requests: 206, 416, multi-range, If-Range");
    let full = blob();
    let r = send(&c, c.get(srv.url("/blob")).header("range", "bytes=100-199")).await;
    assert_eq!(
        (r.status.as_u16(), r.h("content-range")),
        (206, "bytes 100-199/10000".to_string())
    );
    assert_eq!(r.body, full[100..200]);
    let r = send(&c, c.get(srv.url("/blob")).header("range", "bytes=-50")).await;
    assert_eq!(
        (r.status.as_u16(), r.h("content-range")),
        (206, "bytes 9950-9999/10000".to_string())
    );
    let r = send(&c, c.get(srv.url("/blob")).header("range", "bytes=9990-")).await;
    assert_eq!(
        (r.h("content-range"), r.body.len()),
        ("bytes 9990-9999/10000".to_string(), 10)
    );
    let r = send(&c, c.get(srv.url("/blob")).header("range", "bytes=20000-")).await;
    assert_eq!(
        (r.status.as_u16(), r.h("content-range")),
        (416, "bytes */10000".to_string())
    );
    let r = send(
        &c,
        c.get(srv.url("/blob")).header("range", "bytes=0-9,20-29"),
    )
    .await;
    assert_eq!(
        r.status.as_u16(),
        200,
        "multi-range ignored => full representation (RFC allows)"
    );
    assert_eq!(r.body.len(), BLOB_LEN);
    let r = send(
        &c,
        c.get(srv.url("/blob"))
            .header("range", "bytes=0-9")
            .header("if-range", "\"stale-etag\""),
    )
    .await;
    assert_eq!(
        r.status.as_u16(),
        200,
        "If-Range validator mismatch => whole new representation, not a corrupt splice"
    );
    let r = send(&c, c.get(srv.url("/blob"))).await;
    assert_eq!(r.h("accept-ranges"), "bytes");
    let r = send(&c, c.head(srv.url("/blob")).header("range", "bytes=0-9")).await;
    assert_eq!(r.status.as_u16(), 200, "Range is ignored for HEAD");
    let r = send(
        &c,
        c.get(srv.url("/blob"))
            .header("if-none-match", etag_of(&full))
            .header("range", "bytes=0-9"),
    )
    .await;
    assert_eq!(r.status.as_u16(), 304, "unchanged: no part to send");

    title(
        "6. resumable download: connection dies mid-body, client reconnects with Range: bytes=N-",
    );
    let (mut buf, mut attempts) = (Vec::<u8>::new(), 0);
    let (mut etag, mut want_sum, mut total) = (String::new(), String::new(), 0usize);
    while attempts < 10 && (buf.is_empty() || buf.len() < total) {
        attempts += 1;
        let mut rb = c.get(srv.url("/flaky"));
        if !buf.is_empty() {
            rb = rb
                .header("range", format!("bytes={}-", buf.len()))
                .header("if-range", &etag);
        }
        let mut resp = rb.send().await.unwrap();
        if buf.is_empty() {
            assert_eq!(resp.status().as_u16(), 200);
            total = resp.content_length().unwrap() as usize;
            etag = resp.headers()["etag"].to_str().unwrap().to_string();
            want_sum = resp.headers()["x-sha256"].to_str().unwrap().to_string();
        } else {
            assert_eq!(resp.status().as_u16(), 206);
            let cr = resp.headers()["content-range"]
                .to_str()
                .unwrap()
                .to_string();
            assert!(
                cr.starts_with(&format!("bytes {}-", buf.len())),
                "server resumed at our offset: {cr}"
            );
        }
        let before = buf.len();
        loop {
            match resp.chunk().await {
                Ok(Some(ch)) => buf.extend_from_slice(&ch),
                Ok(None) => break,
                Err(_) => break, // connection broken mid-body: keep what we have
            }
        }
        println!(
            "attempt {attempts}: {} -> have {} / {total} bytes",
            if before == 0 {
                "GET /flaky".to_string()
            } else {
                format!("GET /flaky Range: bytes={before}-")
            },
            buf.len()
        );
    }
    println!();
    assert_eq!(
        attempts, 3,
        "10000 bytes at 3500 per connection = 3 connections"
    );
    assert_eq!(buf.len(), BLOB_LEN);
    assert_eq!(
        hex(&sha256(&buf)),
        want_sum,
        "final checksum verifies the stitched download"
    );
    assert_eq!(buf, full);
    assert_eq!(state.hits("flaky"), 3);

    title("7. If-Match / If-None-Match preconditions: 412 on lost update");
    let r = send(
        &c,
        c.put(srv.url("/notes/a"))
            .header("if-none-match", "*")
            .body("v1"),
    )
    .await;
    assert_eq!(r.status.as_u16(), 201, "create-only PUT");
    let e1 = r.h("etag").to_string();
    let r = send(
        &c,
        c.put(srv.url("/notes/a"))
            .header("if-none-match", "*")
            .body("again"),
    )
    .await;
    assert_eq!(r.status.as_u16(), 412, "already exists");
    let r = send(&c, c.put(srv.url("/notes/a")).body("blind overwrite")).await;
    assert_eq!(r.status.as_u16(), 428, "server demands a precondition");
    let r = send(
        &c,
        c.put(srv.url("/notes/a"))
            .header("if-match", &e1)
            .body("alice's edit"),
    )
    .await;
    assert_eq!(r.status.as_u16(), 200);
    let e2 = r.h("etag").to_string();
    assert_ne!(e1, e2);
    // If-None-Match must not be a way round the If-Match rule
    for (tag, want) in [("\"something-else\"", 428), (e2.as_str(), 412)] {
        let rb = c.put(srv.url("/notes/a")).header("if-none-match", tag);
        let r = send(&c, rb.body("sneaky overwrite")).await;
        assert_eq!(r.status.as_u16(), want, "If-None-Match: {tag}");
    }
    let r = send(
        &c,
        c.put(srv.url("/notes/a"))
            .header("if-match", &e1)
            .body("bob's edit (based on stale v1)"),
    )
    .await;
    assert_eq!(
        r.status.as_u16(),
        412,
        "bob's write would have clobbered alice's"
    );
    let r = c.get(srv.url("/notes/a")).send().await.unwrap();
    assert_eq!(r.text().await.unwrap(), "alice's edit");
    let r = send(
        &c,
        c.put(srv.url("/notes/a"))
            .header("if-match", &e2)
            .body("bob re-reads and retries"),
    )
    .await;
    assert_eq!(r.status.as_u16(), 200);

    // the number of stored notes is bounded ("a" is already one of them)
    for i in 1..=MAX_NOTES {
        let url = srv.url(&format!("/notes/extra-{i}"));
        let status = c.put(url).body("x").send().await.unwrap().status();
        let want = if i < MAX_NOTES { 201 } else { 507 };
        assert_eq!(status.as_u16(), want, "note number {}", i + 1);
    }

    drop(cache);
    drop(c);
    drop(auto);
    srv.shutdown().await;
    println!("all caching / conditional-request assertions passed");
}
