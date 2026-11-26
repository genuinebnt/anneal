//! RECIPE 308 (web): a resilient HTTP client against a deliberately flaky server
//! LEVEL: advanced
//! Use in: client stage (calling other services, downloading artifacts, surviving partial outages).   Run: cargo run --release --example http_client_resilience
//!
//! What: a scripted FLAKY server (hand-written HTTP/1.1 over TCP so it can really drop connections, truncate bodies
//! and count accepted connections) and a reqwest-based client that survives it:
//!  * timeouts: a connect timeout and a per-attempt total timeout (a slow response is cut and retried).
//!  * RETRY POLICY: retry only IDEMPOTENT methods (GET/HEAD/PUT/DELETE/OPTIONS) or requests carrying an
//!    `Idempotency-Key`; a bare POST that fails after being sent is NOT retried (it may have been processed).
//!    Retry 5xx, 408, 429, transport errors; never 4xx. Exponential backoff with full jitter, and `Retry-After` is honoured.
//!    Delays use a recorded virtual clock (we assert them, then sleep only a few ms) so the recipe is fast and exact.
//!  * CIRCUIT BREAKER (closed -> open -> half-open -> closed) with an injectable clock: while open, calls fail fast
//!    WITHOUT touching the server (asserted by server-side counters).
//!  * CONCURRENCY LIMIT with a Semaphore (server-side in-flight gauge proves the cap).
//!  * HEDGING (tail-latency): if the first try is slow, fire a second one after a delay and take the first answer
//!    (only for idempotent requests, with a budget: it multiplies load).
//!  * CONNECTION POOLING: sequential requests reuse one TCP connection (server counts accepts); pooling disabled => N accepts.
//!  * STREAMING DOWNLOAD with RESUME: `Range: bytes=N-` + `If-Range`, then SHA-256 verification against a checksum header.
//!  * RESPONSE-SIZE LIMIT: reject by `Content-Length`, or abort mid-stream for chunked bodies.
//! COMMON USES: service-to-service calls, SDKs for third-party APIs, artifact/model/file downloaders, CI fetchers, anything
//! behind a load balancer that sometimes returns 502/503.
//! Read: https://datatracker.ietf.org/doc/html/rfc9110 , https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Retry-After ,
//! https://developer.mozilla.org/en-US/docs/Web/HTTP/Guides/Range_requests , https://datatracker.ietf.org/doc/html/rfc7233
#![allow(dead_code)]
use futures_util::StreamExt;
use reqwest::{Client, RequestBuilder, header::HeaderMap};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::SeqCst},
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::Semaphore,
};

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
const IV: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

// ---------- minimal SHA-256 / HMAC-SHA256 / hex / base64url (standalone; see cryptography/ chapter for the theory) ----------
fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h = IV;
    let mut m = data.to_vec();
    m.push(0x80);
    while m.len() % 64 != 56 {
        m.push(0);
    }
    m.extend((data.len() as u64 * 8).to_be_bytes());
    for ch in m.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(ch[4 * i..4 * i + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add((e & f) ^ (!e & g))
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let t2 = s0.wrapping_add((a & b) ^ (a & c) ^ (b & c));
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 32];
    for i in 0..8 {
        out[4 * i..4 * i + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    out
}
fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&sha256(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner: Vec<u8> = k.iter().map(|b| b ^ 0x36).collect();
    inner.extend_from_slice(msg);
    let ih = sha256(&inner);
    let mut outer: Vec<u8> = k.iter().map(|b| b ^ 0x5c).collect();
    outer.extend_from_slice(&ih);
    sha256(&outer)
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
/// Constant-time equality: no early exit, the loop always touches every byte.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    let mut d = (a.len() ^ b.len()) as u8;
    for i in 0..a.len().max(b.len()) {
        d |= a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0);
    }
    d == 0
}
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

// ================================================================ the flaky server
#[derive(Clone, Debug)]
enum Beh {
    Ok,
    Status(u16),
    Rate(u64), // 429 + Retry-After: secs
    Slow(u64), // answer 200 after N ms
    Drop,      // close the connection without a response
    Trunc,     // promise 100 bytes, send 10, close
}
#[derive(Default)]
struct Flaky {
    scripts: Mutex<HashMap<String, Vec<Beh>>>,
    counts: Mutex<HashMap<String, u32>>,
    log: Mutex<Vec<(String, String, Option<String>)>>, // (name, method, idempotency-key)
    accepts: AtomicU32,
    requests: AtomicU32,
    inflight: AtomicU32,
    max_inflight: AtomicU32,
    file_mode: AtomicU32, // 0 normal, 1 first full response is cut at 60_000 bytes, 2 wrong checksum header
    file_cut_done: AtomicBool,
    ranges: Mutex<Vec<String>>,
    big_bytes_sent: AtomicU64,
}
impl Flaky {
    fn script(&self, name: &str, s: Vec<Beh>) {
        self.scripts.lock().unwrap().insert(name.into(), s);
    }
    fn count(&self, name: &str) -> u32 {
        *self.counts.lock().unwrap().get(name).unwrap_or(&0)
    }
}
fn file_bytes() -> Vec<u8> {
    (0..200_000u32)
        .map(|i| (i.wrapping_mul(31).wrapping_add(7) >> 1) as u8)
        .collect()
}

async fn head(
    w: &mut (impl AsyncWriteExt + Unpin),
    status: u16,
    hs: &[(&str, String)],
    len: Option<usize>,
) {
    let reason = match status {
        200 => "OK",
        206 => "Partial Content",
        429 => "Too Many Requests",
        404 => "Not Found",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Status",
    };
    let mut s = format!("HTTP/1.1 {status} {reason}\r\n");
    for (k, v) in hs {
        s.push_str(&format!("{k}: {v}\r\n"));
    }
    match len {
        Some(n) => s.push_str(&format!("content-length: {n}\r\n")),
        None => s.push_str("transfer-encoding: chunked\r\n"),
    }
    s.push_str("\r\n");
    let _ = w.write_all(s.as_bytes()).await;
}
async fn full(
    w: &mut (impl AsyncWriteExt + Unpin),
    status: u16,
    hs: &[(&str, String)],
    body: &[u8],
) {
    head(w, status, hs, Some(body.len())).await;
    let _ = w.write_all(body).await;
}

async fn serve_conn(sock: TcpStream, st: Arc<Flaky>) {
    st.accepts.fetch_add(1, SeqCst);
    let (rd, mut w) = sock.into_split();
    let mut rd = BufReader::new(rd);
    loop {
        let mut line = String::new();
        if rd.read_line(&mut line).await.unwrap_or(0) == 0 {
            return;
        }
        let mut it = line.split_whitespace();
        let (method, path) = (
            it.next().unwrap_or("").to_string(),
            it.next().unwrap_or("").to_string(),
        );
        let (mut clen, mut range, mut idem) = (0usize, None, None);
        loop {
            let mut h = String::new();
            if rd.read_line(&mut h).await.unwrap_or(0) == 0 {
                return;
            }
            let h = h.trim_end().to_string();
            if h.is_empty() {
                break;
            }
            if let Some((k, v)) = h.split_once(':') {
                match k.to_ascii_lowercase().as_str() {
                    "content-length" => clen = v.trim().parse().unwrap_or(0),
                    "range" => range = Some(v.trim().to_string()),
                    "idempotency-key" => idem = Some(v.trim().to_string()),
                    _ => {}
                }
            }
        }
        let mut body = vec![0u8; clen];
        if rd.read_exact(&mut body).await.is_err() {
            return;
        }
        st.requests.fetch_add(1, SeqCst);
        if let Some(name) = path.strip_prefix("/s/") {
            let idx = {
                let mut c = st.counts.lock().unwrap();
                let e = c.entry(name.to_string()).or_insert(0);
                *e += 1;
                *e - 1
            };
            st.log
                .lock()
                .unwrap()
                .push((name.to_string(), method.clone(), idem));
            let beh = st
                .scripts
                .lock()
                .unwrap()
                .get(name)
                .and_then(|s| s.get(idx as usize).cloned())
                .unwrap_or(Beh::Ok);
            let ok = json!({"ok": true, "request_no": idx + 1}).to_string();
            match beh {
                Beh::Ok => {
                    full(
                        &mut w,
                        200,
                        &[("content-type", "application/json".into())],
                        ok.as_bytes(),
                    )
                    .await
                }
                Beh::Status(s) => {
                    full(&mut w, s, &[], format!("{{\"error\":{s}}}").as_bytes()).await
                }
                Beh::Rate(secs) => {
                    full(
                        &mut w,
                        429,
                        &[("retry-after", secs.to_string())],
                        b"{\"error\":\"slow down\"}",
                    )
                    .await
                }
                Beh::Slow(ms) => {
                    tokio::time::sleep(Duration::from_millis(ms)).await;
                    full(&mut w, 200, &[], ok.as_bytes()).await
                }
                Beh::Drop => return,
                Beh::Trunc => {
                    head(&mut w, 200, &[], Some(100)).await;
                    let _ = w.write_all(b"0123456789").await;
                    return;
                }
            }
        } else if path == "/gauge" {
            let n = st.inflight.fetch_add(1, SeqCst) + 1;
            st.max_inflight.fetch_max(n, SeqCst);
            tokio::time::sleep(Duration::from_millis(120)).await;
            st.inflight.fetch_sub(1, SeqCst);
            full(&mut w, 200, &[], b"ok").await;
        } else if path == "/big" {
            let blob = vec![b'x'; 1_000_000];
            head(&mut w, 200, &[], Some(blob.len())).await;
            let _ = w.write_all(&blob).await;
        } else if path == "/chunked" {
            head(&mut w, 200, &[], None).await;
            for _ in 0..60 {
                let chunk = vec![b'y'; 10_000];
                if w.write_all(format!("{:x}\r\n", chunk.len()).as_bytes())
                    .await
                    .is_err()
                    || w.write_all(&chunk).await.is_err()
                    || w.write_all(b"\r\n").await.is_err()
                {
                    return;
                }
                st.big_bytes_sent.fetch_add(10_000, SeqCst);
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            let _ = w.write_all(b"0\r\n\r\n").await;
        } else if path == "/file" {
            let data = file_bytes();
            let sum = hex(&sha256(&data));
            let etag = format!("\"{}\"", &sum[..16]);
            let mut hs = vec![
                ("etag", etag),
                ("accept-ranges", "bytes".to_string()),
                (
                    "x-checksum-sha256",
                    if st.file_mode.load(SeqCst) == 2 {
                        "0".repeat(64)
                    } else {
                        sum
                    },
                ),
            ];
            if let Some(r) = range {
                st.ranges.lock().unwrap().push(r.clone());
                let start: usize = r
                    .trim_start_matches("bytes=")
                    .trim_end_matches('-')
                    .parse()
                    .unwrap_or(0);
                hs.push((
                    "content-range",
                    format!("bytes {start}-{}/{}", data.len() - 1, data.len()),
                ));
                full(&mut w, 206, &hs, &data[start..]).await;
            } else if st.file_mode.load(SeqCst) == 1 && !st.file_cut_done.swap(true, SeqCst) {
                head(&mut w, 200, &hs, Some(data.len())).await;
                let _ = w.write_all(&data[..60_000]).await;
                return; // connection dies mid-body
            } else {
                full(&mut w, 200, &hs, &data).await;
            }
        } else {
            full(&mut w, 404, &[], b"{}").await;
        }
    }
}

async fn start_flaky() -> (String, Arc<Flaky>) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    let st = Arc::new(Flaky::default());
    let s2 = st.clone();
    tokio::spawn(async move {
        loop {
            if let Ok((sock, _)) = l.accept().await {
                tokio::spawn(serve_conn(sock, s2.clone()));
            }
        }
    });
    NAMES
        .lock()
        .unwrap()
        .push((addr.to_string(), "flaky.test".into()));
    (format!("http://{addr}"), st)
}

// ================================================================ the resilient client
struct Robust {
    c: Client,
    max_attempts: u32,
    base_ms: u64,
    cap_ms: u64,
    rng: Mutex<u64>,
    delays: Mutex<Vec<u64>>, // virtual delays chosen before each retry
}
fn idempotent(method: &str, has_key: bool) -> bool {
    matches!(method, "GET" | "HEAD" | "PUT" | "DELETE" | "OPTIONS") || has_key
}
fn retryable_status(s: u16) -> bool {
    matches!(s, 408 | 425 | 429 | 500 | 502 | 503 | 504)
}

impl Robust {
    fn new(per_try: Duration, pooled: bool) -> Robust {
        let mut b = Client::builder()
            .no_proxy()
            .connect_timeout(Duration::from_millis(500))
            .timeout(per_try);
        if !pooled {
            b = b.pool_max_idle_per_host(0);
        }
        Robust {
            c: b.build().unwrap(),
            max_attempts: 4,
            base_ms: 100,
            cap_ms: 5_000,
            rng: Mutex::new(0x853C49E6748FEA9B),
            delays: Mutex::new(vec![]),
        }
    }
    /// Full jitter: uniform in [0, min(cap, base * 2^(n-1))].
    fn backoff(&self, failed: u32) -> u64 {
        let ceil = self.cap_ms.min(self.base_ms << (failed - 1));
        let mut r = self.rng.lock().unwrap();
        *r ^= *r << 13;
        *r ^= *r >> 7;
        *r ^= *r << 17;
        *r % (ceil + 1)
    }
    /// Returns (result, attempts used).
    async fn send(
        &self,
        method: &str,
        url: &str,
        body: Option<&str>,
        key: Option<&str>,
    ) -> (Result<Resp, String>, u32) {
        let can_retry = idempotent(method, key.is_some());
        let mut attempt = 0;
        loop {
            attempt += 1;
            let mut rb = self.c.request(method.parse().unwrap(), url);
            if let Some(k) = key {
                rb = rb.header("idempotency-key", k);
            }
            if let Some(b) = body {
                rb = rb.body(b.to_string());
            }
            let res = try_call(
                &self.c,
                rb,
                &format!("{method} attempt {attempt}/{}", self.max_attempts),
            )
            .await;
            let (why, wait_ms) = match &res {
                Ok(r) if (200..300).contains(&r.status) => {
                    return (res.map_err(|e| e.to_string()), attempt);
                }
                Ok(r) if !retryable_status(r.status) => {
                    return (res.map_err(|e| e.to_string()), attempt);
                }
                Ok(r) => {
                    let ra = r.h("retry-after").parse::<u64>().ok().map(|s| s * 1000);
                    (format!("HTTP {}", r.status), ra)
                }
                Err(e) => (err_kind(e).to_string(), None),
            };
            if !can_retry {
                println!(
                    "   => {why}: {method} without an idempotency key is NOT retried (the server may have acted on it)"
                );
                return (res.map_err(|e| e.to_string()), attempt);
            }
            if attempt >= self.max_attempts {
                println!("   => {why}: out of attempts");
                return (res.map_err(|e| e.to_string()), attempt);
            }
            let delay = self.backoff(attempt).max(wait_ms.unwrap_or(0)); // Retry-After is a floor
            println!(
                "   => {why}: retry in {delay}ms{}",
                if wait_ms.is_some() {
                    " (honouring Retry-After)"
                } else {
                    ""
                }
            );
            self.delays.lock().unwrap().push(delay);
            tokio::time::sleep(Duration::from_millis(delay / 100)).await; // compressed time: we asserted the virtual delay
        }
    }
}

// ---- circuit breaker with an injectable clock
#[derive(Debug, Clone, Copy, PartialEq)]
enum BState {
    Closed(u32),
    Open(u64),
    HalfOpen,
}
struct Breaker {
    st: Mutex<BState>,
    threshold: u32,
    reset_ms: u64,
    clock: Arc<AtomicU64>,
}
impl Breaker {
    fn allow(&self) -> bool {
        let mut s = self.st.lock().unwrap();
        match *s {
            BState::Closed(_) => true,
            BState::Open(until) if self.clock.load(SeqCst) >= until => {
                *s = BState::HalfOpen;
                true
            } // exactly ONE probe
            BState::Open(_) | BState::HalfOpen => false,
        }
    }
    fn record(&self, ok: bool) {
        let mut s = self.st.lock().unwrap();
        let now = self.clock.load(SeqCst);
        *s = match (*s, ok) {
            (_, true) => BState::Closed(0),
            (BState::Closed(f), false) if f + 1 < self.threshold => BState::Closed(f + 1),
            _ => BState::Open(now + self.reset_ms),
        };
    }
    fn state(&self) -> BState {
        *self.st.lock().unwrap()
    }
}

// ---- streaming download with resume + checksum
async fn download(c: &Client, url: &str) -> Result<(Vec<u8>, u32), String> {
    let (mut buf, mut reqs, mut etag, mut want_sum, mut total): (
        Vec<u8>,
        u32,
        Option<String>,
        Option<String>,
        usize,
    ) = (vec![], 0, None, None, 0);
    loop {
        reqs += 1;
        if reqs > 4 {
            return Err("too many resumes".into());
        }
        let mut rb = c.get(url);
        println!("\n# download request {reqs}");
        println!("> GET /file HTTP/1.1");
        if !buf.is_empty() {
            rb = rb
                .header("range", format!("bytes={}-", buf.len()))
                .header("if-range", etag.clone().unwrap());
            println!(
                "> range: bytes={}-\n> if-range: {}",
                buf.len(),
                etag.as_ref().unwrap()
            );
        }
        let r = rb.send().await.map_err(|e| e.to_string())?;
        println!("< HTTP/1.1 {}", r.status());
        for k in [
            "content-length",
            "content-range",
            "etag",
            "accept-ranges",
            "x-checksum-sha256",
        ] {
            if let Some(v) = r.headers().get(k) {
                println!("< {k}: {}", v.to_str().unwrap());
            }
        }
        match r.status().as_u16() {
            200 => {
                buf.clear();
                total = r.content_length().unwrap_or(0) as usize;
                etag = r
                    .headers()
                    .get("etag")
                    .map(|v| v.to_str().unwrap().to_string());
                want_sum = r
                    .headers()
                    .get("x-checksum-sha256")
                    .map(|v| v.to_str().unwrap().to_string());
            }
            206 => {
                let cr = r
                    .headers()
                    .get("content-range")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                let start: usize = cr
                    .trim_start_matches("bytes ")
                    .split('-')
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(usize::MAX);
                if start != buf.len() {
                    return Err(format!("server resumed at {start}, we have {}", buf.len()));
                }
            }
            s => return Err(format!("HTTP {s}")),
        }
        let mut stream = r.bytes_stream();
        let mut failed = false;
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(b) => buf.extend_from_slice(&b),
                Err(e) => {
                    println!(
                        "   ! stream broke after {} bytes: {}",
                        buf.len(),
                        err_kind(&e)
                    );
                    failed = true;
                    break;
                }
            }
        }
        if !failed && buf.len() == total.max(buf.len()) && (total == 0 || buf.len() >= total) {
            let got = hex(&sha256(&buf));
            return if Some(&got) == want_sum.as_ref() {
                println!("   checksum OK ({}...)", &got[..16]);
                Ok((buf, reqs))
            } else {
                Err(format!(
                    "checksum mismatch: got {}..., server said {}...",
                    &got[..8],
                    &want_sum.unwrap()[..8]
                ))
            };
        }
        if etag.is_none() {
            return Err("cannot resume without a validator".into());
        }
        println!("   ... resuming from byte {}", buf.len());
    }
}

// ---- response size limit
async fn get_limited(c: &Client, url: &str, limit: usize) -> (Result<usize, String>, usize) {
    let r = c.get(url).send().await.unwrap();
    if let Some(n) = r.content_length()
        && n as usize > limit
    {
        return (
            Err(format!("declared Content-Length {n} exceeds limit {limit}")),
            0,
        );
    }
    let (mut got, mut s) = (0usize, r.bytes_stream());
    while let Some(ch) = s.next().await {
        got += ch.unwrap().len();
        if got > limit {
            return (
                Err(format!(
                    "body exceeded limit {limit} (aborted after {got} bytes)"
                )),
                got,
            );
        }
    }
    (Ok(got), got)
}

#[tokio::main]
async fn main() {
    let (base, st) = start_flaky().await;
    let u = |p: &str| format!("{base}{p}");

    println!("==== 1. GET: retry 5xx with exponential backoff + jitter ====");
    st.script(
        "flaky500",
        vec![Beh::Status(500), Beh::Status(503), Beh::Ok],
    );
    let rc = Robust::new(Duration::from_millis(400), true);
    let (r, n) = rc.send("GET", &u("/s/flaky500"), None, None).await;
    assert_eq!((r.unwrap().status, n, st.count("flaky500")), (200, 3, 3));
    let d = rc.delays.lock().unwrap().clone();
    assert!(d[0] <= 100 && d[1] <= 200, "full jitter bounds: {d:?}");

    println!("\n==== 2. 429 with Retry-After is honoured ====");
    st.script("limited", vec![Beh::Rate(2), Beh::Ok]);
    let rc = Robust::new(Duration::from_millis(400), true);
    let (r, n) = rc.send("GET", &u("/s/limited"), None, None).await;
    assert_eq!((r.unwrap().status, n), (200, 2));
    assert!(
        rc.delays.lock().unwrap()[0] >= 2000,
        "waited at least Retry-After (virtual ms)"
    );

    println!("\n==== 3. slow response: per-attempt timeout, then retry ====");
    st.script("slow", vec![Beh::Slow(700), Beh::Ok]);
    let rc = Robust::new(Duration::from_millis(250), true);
    let t = Instant::now();
    let (r, n) = rc.send("GET", &u("/s/slow"), None, None).await;
    assert_eq!((r.unwrap().status, n), (200, 2));
    assert!(
        t.elapsed() < Duration::from_millis(650),
        "the slow 700ms response was abandoned at the timeout"
    );

    println!("\n==== 4. dropped connections and truncated bodies ====");
    st.script("dropped", vec![Beh::Drop, Beh::Drop, Beh::Ok]);
    let rc = Robust::new(Duration::from_millis(400), true);
    let (r, n) = rc.send("GET", &u("/s/dropped"), None, None).await;
    assert_eq!((r.unwrap().status, n, st.count("dropped")), (200, 3, 3));
    st.script("trunc", vec![Beh::Trunc, Beh::Ok]);
    let (r, n) = rc.send("GET", &u("/s/trunc"), None, None).await;
    assert_eq!((r.unwrap().status, n), (200, 2));

    println!("\n==== 5. what must NOT be retried ====");
    st.script("pay", vec![Beh::Status(503), Beh::Ok]);
    let rc = Robust::new(Duration::from_millis(400), true);
    let (r, n) = rc
        .send("POST", &u("/s/pay"), Some("{\"amount\":100}"), None)
        .await;
    assert_eq!(
        (r.unwrap().status, n, st.count("pay")),
        (503, 1, 1),
        "bare POST: exactly one attempt"
    );
    st.script("pay_drop", vec![Beh::Drop, Beh::Ok]);
    let (r, n) = rc.send("POST", &u("/s/pay_drop"), Some("{}"), None).await;
    assert!(r.is_err());
    assert_eq!(
        (n, st.count("pay_drop")),
        (1, 1),
        "connection dropped after sending a POST: outcome unknown, do not blindly retry"
    );
    st.script("pay_key", vec![Beh::Drop, Beh::Status(503), Beh::Ok]);
    let (r, n) = rc
        .send("POST", &u("/s/pay_key"), Some("{}"), Some("order-77"))
        .await;
    assert_eq!((r.unwrap().status, n), (200, 3));
    let keys: Vec<_> = st
        .log
        .lock()
        .unwrap()
        .iter()
        .filter(|l| l.0 == "pay_key")
        .map(|l| l.2.clone())
        .collect();
    assert_eq!(
        keys,
        vec![Some("order-77".to_string()); 3],
        "the SAME key on every attempt lets the server dedupe"
    );
    st.script("put", vec![Beh::Status(503), Beh::Ok]);
    let (r, n) = rc.send("PUT", &u("/s/put"), Some("{}"), None).await;
    assert_eq!(
        (r.unwrap().status, n),
        (200, 2),
        "PUT is idempotent by definition"
    );
    let (r, n) = rc.send("GET", &u("/nothing-here"), None, None).await;
    assert_eq!(
        (r.unwrap().status, n),
        (404, 1),
        "4xx is the caller's fault: no retry"
    );
    st.script("always", vec![Beh::Status(500); 10]);
    let (r, n) = rc.send("GET", &u("/s/always"), None, None).await;
    assert_eq!(
        (r.unwrap().status, n, st.count("always")),
        (500, 4, 4),
        "budget: max_attempts"
    );

    println!("\n==== 6. circuit breaker ====");
    st.script(
        "svc",
        vec![
            Beh::Status(500),
            Beh::Status(500),
            Beh::Status(500),
            Beh::Status(500),
            Beh::Ok,
            Beh::Ok,
        ],
    );
    let clock = Arc::new(AtomicU64::new(0));
    let br = Breaker {
        st: Mutex::new(BState::Closed(0)),
        threshold: 3,
        reset_ms: 10_000,
        clock: clock.clone(),
    };
    let c = Client::builder()
        .no_proxy()
        .timeout(Duration::from_millis(400))
        .build()
        .unwrap();
    let guarded = |label: &'static str| {
        let (br, c, url) = (&br, &c, u("/s/svc"));
        async move {
            if !br.allow() {
                println!(
                    "   [{label}] circuit {:?}: failing fast, no request sent",
                    br.state()
                );
                return Err("circuit open");
            }
            let ok = c
                .get(&url)
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false);
            br.record(ok);
            println!(
                "   [{label}] server said {}; circuit now {:?}",
                if ok { "OK" } else { "500" },
                br.state()
            );
            if ok { Ok(()) } else { Err("upstream failed") }
        }
    };
    for i in 1..=3 {
        assert_eq!(
            guarded(["call 1", "call 2", "call 3"][i - 1]).await,
            Err("upstream failed")
        );
    }
    assert_eq!(br.state(), BState::Open(10_000));
    for l in ["call 4", "call 5"] {
        assert_eq!(guarded(l).await, Err("circuit open"));
    }
    assert_eq!(
        st.count("svc"),
        3,
        "open circuit: the failing service was NOT hit again"
    );
    clock.store(10_000, SeqCst);
    assert_eq!(guarded("probe 1").await, Err("upstream failed")); // half-open probe still fails -> re-open
    assert!(matches!(br.state(), BState::Open(20_000)));
    assert_eq!(st.count("svc"), 4);
    assert_eq!(guarded("call 6").await, Err("circuit open"));
    clock.store(20_000, SeqCst);
    assert_eq!(guarded("probe 2").await, Ok(())); // service recovered
    assert_eq!(br.state(), BState::Closed(0));
    assert_eq!(guarded("call 7").await, Ok(()));
    assert_eq!(st.count("svc"), 6);

    println!("\n==== 7. concurrency limit (semaphore) ====");
    let c = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let run = |limit: usize| {
        let (c, base) = (c.clone(), base.clone());
        async move {
            let sem = Arc::new(Semaphore::new(limit));
            let mut hs = vec![];
            for _ in 0..12 {
                let (c, base, sem) = (c.clone(), base.clone(), sem.clone());
                hs.push(tokio::spawn(async move {
                    let _p = sem.acquire().await.unwrap();
                    c.get(format!("{base}/gauge"))
                        .send()
                        .await
                        .unwrap()
                        .text()
                        .await
                        .unwrap()
                }));
            }
            for h in hs {
                h.await.unwrap();
            }
        }
    };
    st.max_inflight.store(0, SeqCst);
    run(3).await;
    println!(
        "   limit 3 -> server saw at most {} concurrent requests",
        st.max_inflight.load(SeqCst)
    );
    assert!(st.max_inflight.load(SeqCst) <= 3);
    st.max_inflight.store(0, SeqCst);
    run(100).await;
    println!(
        "   no limit -> server saw {} concurrent requests",
        st.max_inflight.load(SeqCst)
    );
    assert!(st.max_inflight.load(SeqCst) > 3);

    println!("\n==== 8. hedged request (tail latency) ====");
    st.script("tail", vec![Beh::Slow(800), Beh::Ok]);
    let c = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let t = Instant::now();
    let first = c.get(u("/s/tail")).send();
    tokio::pin!(first);
    let resp = tokio::select! {
        r = &mut first => r.unwrap(),
        _ = tokio::time::sleep(Duration::from_millis(100)) => { // p95 latency exceeded: send ONE hedge, take whichever answers first
            println!("   no answer after 100ms: sending a hedge request");
            let second = c.get(u("/s/tail")).send();
            tokio::select! { r = &mut first => r.unwrap(), r = second => r.unwrap() }
        }
    };
    assert_eq!(resp.status(), 200);
    assert_eq!(st.count("tail"), 2);
    assert!(
        t.elapsed() < Duration::from_millis(600),
        "answered by the hedge long before the 800ms straggler"
    );
    println!(
        "   answered in {}ms instead of 800ms (cost: one extra request; hedge ONLY idempotent calls, cap the hedge rate)",
        if t.elapsed() < Duration::from_millis(600) {
            "<600"
        } else {
            "?"
        }
    );

    println!("\n==== 9. connection pooling (counting accepts on the server) ====");
    let (a0, r0) = (st.accepts.load(SeqCst), st.requests.load(SeqCst));
    let pooled = Robust::new(Duration::from_secs(2), true);
    for _ in 0..8 {
        assert_eq!(
            pooled
                .c
                .get(u("/s/pool"))
                .send()
                .await
                .unwrap()
                .text()
                .await
                .is_ok(),
            true
        );
    }
    let (da, dr) = (st.accepts.load(SeqCst) - a0, st.requests.load(SeqCst) - r0);
    println!("   pooled client: {dr} requests over {da} TCP connection(s)");
    assert_eq!((dr, da), (8, 1));
    let (a0, r0) = (st.accepts.load(SeqCst), st.requests.load(SeqCst));
    let unpooled = Robust::new(Duration::from_secs(2), false);
    for _ in 0..8 {
        unpooled
            .c
            .get(u("/s/pool"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
    }
    let (da, dr) = (st.accepts.load(SeqCst) - a0, st.requests.load(SeqCst) - r0);
    println!("   pool disabled: {dr} requests over {da} TCP connections (a handshake each time)");
    assert_eq!((dr, da), (8, 8));

    println!("\n==== 10. streaming download: resume with Range + checksum ====");
    st.file_mode.store(1, SeqCst);
    let c = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let (data, reqs) = download(&c, &u("/file")).await.unwrap();
    assert_eq!((data.len(), reqs), (200_000, 2));
    assert_eq!(data, file_bytes());
    assert_eq!(
        st.ranges.lock().unwrap().clone(),
        vec!["bytes=60000-".to_string()],
        "the second request resumed exactly where the first died"
    );
    st.file_mode.store(2, SeqCst);
    let e = download(&c, &u("/file")).await.unwrap_err();
    println!("   corrupted transfer detected: {e}");
    assert!(e.contains("checksum mismatch"));

    println!("\n==== 11. response size limit ====");
    let (r, got) = get_limited(&c, &u("/big"), 100_000).await;
    println!("   /big      -> {r:?} (body bytes read: {got})");
    assert!(r.unwrap_err().contains("Content-Length") && got == 0);
    let (r, got) = get_limited(&c, &u("/chunked"), 100_000).await;
    println!("   /chunked  -> {r:?}");
    assert!(r.is_err() && got > 100_000 && got < 100_000 + 100_000);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let sent = st.big_bytes_sent.load(SeqCst);
    println!("   server managed to send only {sent} of 600000 bytes before our abort");
    assert!(sent < 600_000, "aborting the stream stops the transfer");
    let (r, _) = get_limited(&c, &u("/s/pool"), 100_000).await;
    assert!(r.is_ok());
    println!("\nOK: all client resilience assertions passed");
}
