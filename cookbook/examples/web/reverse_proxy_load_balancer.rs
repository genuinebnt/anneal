//! RECIPE 311 (web): a reverse proxy / load balancer in front of three backends (axum + reqwest)
//! LEVEL: advanced
//! Use in: gateway stage (edge proxy, API gateway, sidecar).   Run: cargo run --release --example reverse_proxy_load_balancer
//!
//! What: a real proxy (`proxy.test`) forwarding to three real backends (`b1.test`, `b2.test`, `b3.test`), with transcripts of BOTH legs.
//!  * Forwarding: method, path, query, headers and body (large bodies are STREAMED, small ones are buffered so they can be retried).
//!  * HOP-BY-HOP headers (Connection and every header it names, Keep-Alive, TE, Trailer, Transfer-Encoding, Upgrade, Proxy-*)
//!    are meaningful for one TCP hop only and are stripped in both directions.
//!  * `X-Forwarded-For/Proto/Host` and `Forwarded` (RFC 7239). A client can SEND these headers, so an edge proxy must overwrite
//!    them (or only extend them when the peer is a trusted proxy) and a backend must read the chain from the RIGHT, skipping trusted hops:
//!    taking the leftmost value lets anyone spoof their IP (rate limits, allow-lists, audit logs).
//!  * Balancing strategies behind a `Strategy` trait: round robin, least connections, smooth weighted round robin (nginx style), and
//!    consistent hashing with virtual nodes (sticky sessions; when a backend dies only ITS keys move). Exact distributions are asserted.
//!  * Health: active checks (probe `/healthz`, thresholds in both directions), passive ejection (consecutive failures, injectable clock,
//!    timed recovery), retries on another backend for idempotent requests (and for connect errors, where nothing was sent), timeouts
//!    mapped to 504, unreachable upstream to 502, no healthy upstream to 503 + Retry-After.
//!  * `X-Request-Id`: accepted if well-formed, otherwise generated; propagated to the backend and echoed to the client.
//! COMMON USES: nginx/HAProxy/Envoy-style edge proxies, API gateways, canary routing, per-tenant sticky routing, service-mesh sidecars.
//! Read: https://datatracker.ietf.org/doc/html/rfc7239 , https://datatracker.ietf.org/doc/html/rfc9110 ,
//! https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Forwarded , https://datatracker.ietf.org/doc/html/rfc7230
#![allow(dead_code)]
use axum::{
    Router,
    body::Body,
    extract::{ConnectInfo, State},
    http::{HeaderMap, HeaderName, HeaderValue, Request},
    response::Response,
    routing::get,
};
use futures_util::StreamExt;
use reqwest::{Client, RequestBuilder};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering::SeqCst},
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

// ================================================================ backends
struct Ctl {
    name: String,
    up: AtomicBool,
    delay_ms: AtomicU64,
    fail500: AtomicBool,
    fail503: AtomicBool,
    hits: AtomicU32,
    inflight: AtomicU32,
}
impl Ctl {
    fn new(n: &str) -> Arc<Ctl> {
        Arc::new(Ctl {
            name: n.into(),
            up: AtomicBool::new(true),
            delay_ms: AtomicU64::new(0),
            fail500: AtomicBool::new(false),
            fail503: AtomicBool::new(false),
            hits: AtomicU32::new(0),
            inflight: AtomicU32::new(0),
        })
    }
}

fn json_resp(status: u16, hs: &[(&str, String)], v: Value) -> Response {
    let mut b = Response::builder()
        .status(status)
        .header("content-type", "application/json");
    for (k, v) in hs {
        b = b.header(*k, v);
    }
    b.body(Body::from(v.to_string())).unwrap()
}
async fn healthz(State(c): State<Arc<Ctl>>) -> Response {
    json_resp(
        if c.up.load(SeqCst) { 200 } else { 503 },
        &[],
        json!({"backend": c.name}),
    )
}
/// Echo what the backend RECEIVED: this is "leg 2" of the transcript.
async fn echo(State(c): State<Arc<Ctl>>, req: Request<Body>) -> Response {
    c.hits.fetch_add(1, SeqCst);
    c.inflight.fetch_add(1, SeqCst);
    let d = c.delay_ms.load(SeqCst);
    if d > 0 {
        tokio::time::sleep(Duration::from_millis(d)).await;
    }
    let (parts, body) = req.into_parts();
    let mut n = 0usize;
    let mut s = body.into_data_stream();
    while let Some(ch) = s.next().await {
        n += ch.map(|b| b.len()).unwrap_or(0);
    }
    c.inflight.fetch_sub(1, SeqCst);
    let mut hs = serde_json::Map::new();
    for (k, v) in &parts.headers {
        hs.insert(k.as_str().to_string(), json!(v.to_str().unwrap_or("?")));
    }
    let st = if c.fail503.load(SeqCst) {
        503
    } else if c.fail500.load(SeqCst) {
        500
    } else {
        200
    };
    // keep-alive/connection-style headers set by the backend: the proxy must not forward them
    json_resp(
        st,
        &[
            ("x-backend", c.name.clone()),
            ("keep-alive", "timeout=5".into()),
            ("proxy-authenticate", "Basic realm=internal".into()),
            ("x-app", "v1".into()),
        ],
        json!({"backend": c.name, "method": parts.method.as_str(), "uri": parts.uri.to_string(), "headers": hs, "body_bytes": n}),
    )
}
async fn stream_route(State(c): State<Arc<Ctl>>) -> Response {
    c.hits.fetch_add(1, SeqCst);
    let s = futures_util::stream::iter(0..5).then(|i| async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        Ok::<_, std::io::Error>(format!("chunk{i};"))
    });
    Response::builder()
        .status(200)
        .body(Body::from_stream(s))
        .unwrap()
}
async fn start_backend(name: &str) -> (Srv, Arc<Ctl>) {
    let ctl = Ctl::new(name);
    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/stream", get(stream_route))
        .fallback(echo)
        .with_state(ctl.clone());
    (Srv::start_named(&format!("{name}.test"), app).await, ctl)
}

// ================================================================ strategies
#[derive(Clone, Debug)]
struct View {
    idx: usize,
    weight: u32,
    active: u32,
    ok: bool,
}
trait Strategy: Send + Sync {
    fn name(&self) -> &'static str;
    /// `key` is a stickiness key (user id / client ip); `ok` marks eligible backends (healthy, not excluded).
    fn pick(&self, key: &str, v: &[View]) -> Option<usize>;
}
struct RoundRobin(AtomicUsize);
impl Strategy for RoundRobin {
    fn name(&self) -> &'static str {
        "round-robin"
    }
    fn pick(&self, _: &str, v: &[View]) -> Option<usize> {
        let s = self.0.load(SeqCst);
        let j = (0..v.len()).map(|i| (s + i) % v.len()).find(|j| v[*j].ok)?;
        self.0.store(j + 1, SeqCst); // continue AFTER the one we picked, so skipping a dead node does not double up its neighbour
        Some(j)
    }
}
struct LeastConn;
impl Strategy for LeastConn {
    fn name(&self) -> &'static str {
        "least-connections"
    }
    fn pick(&self, _: &str, v: &[View]) -> Option<usize> {
        v.iter()
            .filter(|b| b.ok)
            .min_by_key(|b| (b.active, b.idx))
            .map(|b| b.idx)
    }
}
/// nginx "smooth weighted round robin": no long bursts on the heaviest backend.
struct Weighted(Mutex<Vec<i64>>);
impl Strategy for Weighted {
    fn name(&self) -> &'static str {
        "weighted (smooth)"
    }
    fn pick(&self, _: &str, v: &[View]) -> Option<usize> {
        let mut cur = self.0.lock().unwrap();
        cur.resize(v.len(), 0);
        let total: i64 = v.iter().filter(|b| b.ok).map(|b| b.weight as i64).sum();
        for b in v.iter().filter(|b| b.ok) {
            cur[b.idx] += b.weight as i64;
        }
        let best = v
            .iter()
            .filter(|b| b.ok)
            .max_by_key(|b| (cur[b.idx], std::cmp::Reverse(b.idx)))?
            .idx;
        cur[best] -= total;
        Some(best)
    }
}
fn hash64(s: &str) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in s.bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100000001b3);
    }
    h ^= h >> 30;
    h = h.wrapping_mul(0xbf58476d1ce4e5b9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94d049bb133111eb);
    h ^ (h >> 31) // splitmix finalizer: FNV alone clusters similar keys
}
struct ConsistentHash {
    ring: Vec<(u64, usize)>,
}
impl ConsistentHash {
    fn new(names: &[&str], vnodes: usize) -> Self {
        let mut ring: Vec<_> = names
            .iter()
            .enumerate()
            .flat_map(|(i, n)| (0..vnodes).map(move |v| (hash64(&format!("{n}#{v}")), i)))
            .collect();
        ring.sort();
        ConsistentHash { ring }
    }
}
impl Strategy for ConsistentHash {
    fn name(&self) -> &'static str {
        "consistent-hash"
    }
    fn pick(&self, key: &str, v: &[View]) -> Option<usize> {
        let start = self.ring.partition_point(|p| p.0 < hash64(key));
        (0..self.ring.len())
            .map(|i| self.ring[(start + i) % self.ring.len()].1)
            .find(|j| v[*j].ok) // walk clockwise past dead nodes
    }
}

// ================================================================ the proxy
struct B {
    name: String,
    base: String,
    weight: u32,
    active: AtomicU32,
    healthy: AtomicBool,
    fails: AtomicU32,
    ejected_until: AtomicU64,
    ok_streak: AtomicU32,
    bad_streak: AtomicU32,
    tries: AtomicU32,
}
struct Guard(Arc<B>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, SeqCst);
    }
}
#[derive(Clone)]
enum Fwd {
    Overwrite,
    TrustedProxies(Vec<IpAddr>),
}
struct Px {
    c: Client,
    bs: Vec<Arc<B>>,
    strat: Mutex<Arc<dyn Strategy>>,
    clock: Arc<AtomicU64>, // ms, injectable
    fwd: Mutex<Fwd>,
    timeout_ms: AtomicU64,
    rid: AtomicU32,
    eject_ms: u64,
}
const HOP: [&str; 9] = [
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "proxy-connection",
];
fn strip_hop(h: &HeaderMap) -> HeaderMap {
    let named: Vec<String> = h
        .get_all("connection")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(|t| t.trim().to_ascii_lowercase())
        .collect();
    let mut out = HeaderMap::new();
    for (k, v) in h {
        if !HOP.contains(&k.as_str()) && !named.iter().any(|n| n == k.as_str()) {
            out.append(k.clone(), v.clone());
        }
    }
    out
}
fn is_idempotent(m: &str) -> bool {
    matches!(m, "GET" | "HEAD" | "PUT" | "DELETE" | "OPTIONS")
}
fn valid_rid(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}
/// What a backend should do: walk the chain from the right, skipping hops we operate; the first untrusted address is the client.
fn real_client_ip(xff: &str, trusted: &[IpAddr]) -> Option<IpAddr> {
    xff.split(',')
        .rev()
        .filter_map(|s| s.trim().parse::<IpAddr>().ok())
        .find(|ip| !trusted.contains(ip))
}

impl Px {
    fn now(&self) -> u64 {
        self.clock.load(SeqCst)
    }
    fn views(&self, excluded: &[usize]) -> Vec<View> {
        self.bs
            .iter()
            .enumerate()
            .map(|(i, b)| View {
                idx: i,
                weight: b.weight,
                active: b.active.load(SeqCst),
                ok: b.healthy.load(SeqCst)
                    && self.now() >= b.ejected_until.load(SeqCst)
                    && !excluded.contains(&i),
            })
            .collect()
    }
    fn record(&self, i: usize, ok: bool) {
        let b = &self.bs[i];
        if ok {
            b.fails.store(0, SeqCst);
        } else if b.fails.fetch_add(1, SeqCst) + 1 >= 3 {
            // passive ejection
            b.ejected_until.store(self.now() + self.eject_ms, SeqCst);
            b.fails.store(0, SeqCst);
        }
    }
    /// Active health check round (a real proxy runs this on a timer). 2 failures => down, 2 successes => up.
    async fn health_round(&self) {
        for b in &self.bs {
            let ok = self
                .c
                .get(format!("{}/healthz", b.base))
                .timeout(Duration::from_millis(300))
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false);
            if ok {
                b.bad_streak.store(0, SeqCst);
                if b.ok_streak.fetch_add(1, SeqCst) + 1 >= 2 {
                    b.healthy.store(true, SeqCst);
                }
            } else {
                b.ok_streak.store(0, SeqCst);
                if b.bad_streak.fetch_add(1, SeqCst) + 1 >= 2 {
                    b.healthy.store(false, SeqCst);
                }
            }
        }
    }
}

fn plain(status: u16, msg: &str, extra: &[(&str, String)]) -> Response {
    let mut b = Response::builder()
        .status(status)
        .header("content-type", "application/json");
    for (k, v) in extra {
        b = b.header(*k, v);
    }
    b.body(Body::from(json!({"error": msg}).to_string()))
        .unwrap()
}

async fn proxy(
    State(px): State<Arc<Px>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    req: Request<Body>,
) -> Response {
    let (parts, body) = req.into_parts();
    let method = parts.method.as_str().to_string();
    let pq = parts
        .uri
        .path_and_query()
        .map(|p| p.as_str())
        .unwrap_or("/")
        .to_string();
    let rid = match parts
        .headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
    {
        Some(v) if valid_rid(v) => v.to_string(),
        _ => format!("req-{:04}", px.rid.fetch_add(1, SeqCst) + 1), // missing or malformed (log injection!) => ours
    };
    // ---- request headers: drop hop-by-hop and everything the client may have forged
    let mut h = strip_hop(&parts.headers);
    let orig_host = h
        .remove("host")
        .and_then(|v| v.to_str().ok().map(str::to_string))
        .unwrap_or_default();
    let (old_xff, old_proto) = (h.remove("x-forwarded-for"), h.remove("x-forwarded-proto"));
    for n in ["x-forwarded-host", "forwarded", "x-request-id", "via"] {
        h.remove(n);
    }
    let (xff, proto) = match &*px.fwd.lock().unwrap() {
        Fwd::Overwrite => (peer.ip().to_string(), "http".to_string()),
        Fwd::TrustedProxies(t) if t.contains(&peer.ip()) => (
            match old_xff.and_then(|v| v.to_str().ok().map(str::to_string)) {
                Some(o) => format!("{o}, {}", peer.ip()),
                None => peer.ip().to_string(),
            },
            old_proto
                .and_then(|v| v.to_str().ok().map(str::to_string))
                .filter(|p| p == "https" || p == "http")
                .unwrap_or("http".into()),
        ),
        Fwd::TrustedProxies(_) => (peer.ip().to_string(), "http".to_string()),
    };
    let ins = |h: &mut HeaderMap, k: &'static str, v: &str| {
        if let Ok(v) = HeaderValue::from_str(v) {
            h.insert(HeaderName::from_static(k), v);
        }
    };
    ins(&mut h, "x-forwarded-for", &xff);
    ins(&mut h, "x-forwarded-proto", &proto);
    ins(&mut h, "x-forwarded-host", &orig_host);
    ins(
        &mut h,
        "forwarded",
        &format!("for={};proto={};host=\"{}\"", peer.ip(), proto, orig_host),
    );
    ins(&mut h, "x-request-id", &rid);
    ins(&mut h, "via", "1.1 forge-proxy");
    // ---- body: small + length known => buffer (so it can be replayed on retry); otherwise stream, single attempt
    let cl = parts
        .headers
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok());
    let no_body = cl.is_none() && !parts.headers.contains_key("transfer-encoding");
    let (buffered, mut streaming) = if no_body {
        (Some(bytes::Bytes::new()), None)
    } else if cl.is_some_and(|n| n <= 64 * 1024) {
        match axum::body::to_bytes(body, 64 * 1024).await {
            Ok(b) => (Some(b), None),
            Err(_) => return plain(400, "bad body", &[]),
        }
    } else {
        (None, Some(body))
    };
    let sticky = parts
        .headers
        .get("x-user-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .unwrap_or(peer.ip().to_string());
    let (mut excluded, mut tries) = (vec![], 0);
    let mut last: Option<(u16, &str)> = None;
    loop {
        let strat = px.strat.lock().unwrap().clone();
        let Some(i) = strat.pick(&sticky, &px.views(&excluded)) else {
            return match last {
                Some((s, m)) => plain(s, m, &[("x-request-id", rid)]),
                None => plain(
                    503,
                    "no healthy upstream",
                    &[("retry-after", "5".into()), ("x-request-id", rid)],
                ),
            };
        };
        tries += 1;
        let b = px.bs[i].clone();
        b.tries.fetch_add(1, SeqCst);
        let guard = Guard(b.clone());
        b.active.fetch_add(1, SeqCst);
        let mut rb =
            px.c.request(parts.method.clone(), format!("{}{pq}", b.base))
                .headers(h.clone())
                .timeout(Duration::from_millis(px.timeout_ms.load(SeqCst)));
        rb = match (&buffered, streaming.take()) {
            (Some(bytes), _) => rb.body(bytes.clone()),
            (None, Some(s)) => rb.body(reqwest::Body::wrap_stream(s.into_data_stream())),
            (None, None) => unreachable!(),
        };
        let replayable = buffered.is_some();
        match rb.send().await {
            Ok(resp) => {
                let st = resp.status().as_u16();
                if matches!(st, 502 | 503 | 504) {
                    px.record(i, false);
                    if replayable && is_idempotent(&method) && tries < 3 {
                        excluded.push(i);
                        last = Some((st, "upstream unavailable"));
                        continue;
                    }
                } else {
                    px.record(i, true);
                }
                let mut out = Response::builder().status(st);
                for (k, v) in strip_hop(resp.headers()).iter() {
                    out = out.header(k, v);
                }
                let stream = resp.bytes_stream().map(move |c| {
                    let _g = &guard;
                    c.map_err(std::io::Error::other)
                }); // guard lives as long as the body
                return out
                    .header("via", "1.1 forge-proxy")
                    .header("x-request-id", &rid)
                    .header("x-served-by", &b.name)
                    .body(Body::from_stream(stream))
                    .unwrap();
            }
            Err(e) => {
                px.record(i, false);
                let (status, msg) = if e.is_timeout() {
                    (504, "upstream timed out")
                } else {
                    (502, "upstream unreachable")
                };
                // connect errors mean NOTHING was sent: safe to retry any method. Timeouts/resets: only idempotent ones.
                if replayable && tries < 3 && (e.is_connect() || is_idempotent(&method)) {
                    excluded.push(i);
                    last = Some((status, msg));
                    continue;
                }
                return plain(status, msg, &[("x-request-id", rid)]);
            }
        }
    }
}

async fn start_proxy(px: Arc<Px>) -> Srv {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    let (tx, rx) = oneshot::channel::<()>();
    let app = Router::new()
        .fallback(proxy)
        .with_state(px)
        .into_make_service_with_connect_info::<SocketAddr>();
    let h = tokio::spawn(async move {
        axum::serve(l, app)
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await
            .unwrap();
    });
    NAMES
        .lock()
        .unwrap()
        .push((addr.to_string(), "proxy.test".into()));
    Srv {
        addr,
        tx: Some(tx),
        h,
    }
}
fn new_px(
    c: &Client,
    backends: &[(&str, String, u32)],
    strat: Arc<dyn Strategy>,
    clock: Arc<AtomicU64>,
) -> Arc<Px> {
    let bs = backends
        .iter()
        .map(|(n, base, w)| {
            Arc::new(B {
                name: n.to_string(),
                base: base.clone(),
                weight: *w,
                active: AtomicU32::new(0),
                healthy: AtomicBool::new(true),
                fails: AtomicU32::new(0),
                ejected_until: AtomicU64::new(0),
                ok_streak: AtomicU32::new(0),
                bad_streak: AtomicU32::new(0),
                tries: AtomicU32::new(0),
            })
        })
        .collect();
    Arc::new(Px {
        c: c.clone(),
        bs,
        strat: Mutex::new(strat),
        clock,
        fwd: Mutex::new(Fwd::Overwrite),
        timeout_ms: AtomicU64::new(2000),
        rid: AtomicU32::new(0),
        eject_ms: 30_000,
    })
}

fn served_by(r: &Resp) -> String {
    r.h("x-served-by")
}
fn show_leg2(r: &Resp) {
    let j = r.json();
    println!(
        "   leg 2, what {} actually received:\n   > {} {} HTTP/1.1",
        j["backend"].as_str().unwrap_or("?"),
        j["method"].as_str().unwrap_or("?"),
        j["uri"].as_str().unwrap_or("?")
    );
    let mut hs: Vec<_> = j["headers"]
        .as_object()
        .map(|o| o.iter().collect())
        .unwrap_or_default();
    hs.sort_by_key(|(k, _)| k.to_string());
    for (k, v) in hs {
        if k != "host" {
            println!("   > {k}: {}", v.as_str().unwrap_or(""));
        }
    }
}
fn reset_hits(c: &[&Arc<Ctl>]) {
    for x in c {
        x.hits.store(0, SeqCst);
    }
}

#[tokio::main]
async fn main() {
    let c = Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_millis(500))
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let (s1, c1) = start_backend("b1").await;
    let (s2, c2) = start_backend("b2").await;
    let (s3, c3) = start_backend("b3").await;
    let specs = [
        ("b1", s1.url(""), 5u32),
        ("b2", s2.url(""), 1),
        ("b3", s3.url(""), 1),
    ];
    let clock = Arc::new(AtomicU64::new(0));
    let px = new_px(
        &c,
        &specs,
        Arc::new(RoundRobin(AtomicUsize::new(0))),
        clock.clone(),
    );
    let proxy_srv = start_proxy(px.clone()).await;
    let p = |path: &str| proxy_srv.url(path);

    println!("==== 1. one request through the proxy, both legs ====");
    let r = call(
        &c,
        c.get(p("/api/items?limit=2&sort=name"))
            .header("host", "shop.example.com")
            .header("accept", "application/json")
            .header("x-forwarded-for", "6.6.6.6"),
        "leg 1: client -> proxy (client forges X-Forwarded-For)",
    )
    .await;
    assert_eq!(
        (r.status, r.json()["uri"].as_str(), served_by(&r).as_str()),
        (200, Some("/api/items?limit=2&sort=name"), "b1")
    );
    show_leg2(&r);
    let h = &r.json()["headers"];
    assert_eq!(
        h["x-forwarded-for"], "127.0.0.1",
        "forged value discarded, replaced by the socket peer"
    );
    assert_eq!(
        (
            h["x-forwarded-host"].as_str(),
            h["x-forwarded-proto"].as_str(),
            h["via"].as_str()
        ),
        (
            Some("shop.example.com"),
            Some("http"),
            Some("1.1 forge-proxy")
        )
    );
    assert_eq!(
        h["forwarded"],
        "for=127.0.0.1;proto=http;host=\"shop.example.com\""
    );
    assert_eq!(r.h("x-request-id"), "req-0001");
    assert_eq!(h["x-request-id"], "req-0001", "same id on both legs");

    println!("\n==== 2. hop-by-hop headers are stripped (both directions) ====");
    let r = call(
        &c,
        c.get(p("/hop"))
            .header("connection", "keep-alive, x-secret-hop")
            .header("x-secret-hop", "leak")
            .header("keep-alive", "timeout=99")
            .header("te", "trailers")
            .header("upgrade", "websocket")
            .header("proxy-authorization", "Basic c2VjcmV0")
            .header("x-end-to-end", "kept"),
        "request carrying hop-by-hop headers",
    )
    .await;
    let h = &r.json()["headers"];
    for gone in [
        "x-secret-hop",
        "keep-alive",
        "te",
        "upgrade",
        "proxy-authorization",
    ] {
        assert!(h.get(gone).is_none(), "{gone} must not reach the backend");
    }
    assert_eq!(h["x-end-to-end"], "kept");
    assert!(
        r.headers.get("keep-alive").is_none() && r.headers.get("proxy-authenticate").is_none(),
        "response hop-by-hop headers removed"
    );
    assert_eq!(r.h("x-app"), "v1");
    println!(
        "   stripped: connection-named x-secret-hop, keep-alive, te, upgrade, proxy-authorization; kept: x-end-to-end (and x-app in the response)"
    );

    println!("\n==== 3. X-Forwarded-* trust: never believe the client ====");
    let r = c
        .get(p("/who"))
        .header("x-forwarded-for", "6.6.6.6")
        .header("x-forwarded-proto", "https")
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        (
            r["headers"]["x-forwarded-for"].as_str(),
            r["headers"]["x-forwarded-proto"].as_str()
        ),
        (Some("127.0.0.1"), Some("http")),
        "edge proxy: OVERWRITE"
    );
    *px.fwd.lock().unwrap() = Fwd::TrustedProxies(vec!["127.0.0.1".parse().unwrap()]);
    let r = call(
        &c,
        c.get(p("/who"))
            .header("x-forwarded-for", "6.6.6.6, 203.0.113.9")
            .header("x-forwarded-proto", "https"),
        "peer is a TRUSTED proxy: chain is extended, not replaced",
    )
    .await;
    let xff = r.json()["headers"]["x-forwarded-for"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(xff, "6.6.6.6, 203.0.113.9, 127.0.0.1");
    assert_eq!(r.json()["headers"]["x-forwarded-proto"], "https");
    let trusted: Vec<IpAddr> = vec!["127.0.0.1".parse().unwrap(), "203.0.113.9".parse().unwrap()];
    assert_eq!(
        real_client_ip(&xff, &trusted),
        Some("6.6.6.6".parse().unwrap()),
        "203.0.113.9 is our CDN, so the first untrusted hop to its left is the client..."
    );
    assert_eq!(
        real_client_ip(&xff, &["127.0.0.1".parse().unwrap()]),
        Some("203.0.113.9".parse().unwrap()),
        "...while only trusting the nearest proxy stops at 203.0.113.9"
    );
    assert_ne!(
        xff.split(',').next().unwrap(),
        "203.0.113.9",
        "the LEFTMOST entry is attacker-controlled: never use it"
    );
    println!(
        "   backend rule: walk X-Forwarded-For from the right, skipping addresses of proxies you operate."
    );
    *px.fwd.lock().unwrap() = Fwd::Overwrite;

    println!("\n==== 4. X-Request-Id ====");
    let r = call(
        &c,
        c.get(p("/rid")).header("x-request-id", "trace-abc.123"),
        "well-formed id is propagated",
    )
    .await;
    assert_eq!(
        (
            r.h("x-request-id").as_str(),
            r.json()["headers"]["x-request-id"].as_str()
        ),
        ("trace-abc.123", Some("trace-abc.123"))
    );
    let r = c
        .get(p("/rid"))
        .header("x-request-id", "bad id with spaces")
        .send()
        .await
        .unwrap();
    assert!(
        r.headers()["x-request-id"]
            .to_str()
            .unwrap()
            .starts_with("req-"),
        "malformed id replaced"
    );
    let r = c
        .get(p("/rid"))
        .header("x-request-id", "x".repeat(200))
        .send()
        .await
        .unwrap();
    assert!(
        r.headers()["x-request-id"]
            .to_str()
            .unwrap()
            .starts_with("req-"),
        "oversized id replaced"
    );

    println!("\n==== 5. streaming: bodies are forwarded, not collected ====");
    let big = vec![7u8; 2_000_000];
    let r = call(
        &c,
        c.post(p("/upload"))
            .header("content-type", "application/octet-stream")
            .body(reqwest::Body::wrap_stream(futures_util::stream::iter(
                (0..20).map(|_| Ok::<_, std::io::Error>(bytes::Bytes::from(vec![7u8; 100_000]))),
            ))),
        "2 MB chunked upload (no Content-Length => streamed, single attempt)",
    )
    .await;
    assert_eq!(r.json()["body_bytes"], 2_000_000);
    let _ = big;
    let r = call(
        &c,
        c.post(p("/small")).body("hello"),
        "small body: buffered (so a retry could replay it)",
    )
    .await;
    assert_eq!(
        (r.json()["body_bytes"].as_u64(), r.json()["method"].as_str()),
        (Some(5), Some("POST"))
    );
    let st = std::time::Instant::now();
    let r = c.get(p("/stream")).send().await.unwrap();
    let mut s = r.bytes_stream();
    let first = s.next().await.unwrap().unwrap();
    let t_first = st.elapsed();
    let mut all = first.to_vec();
    while let Some(ch) = s.next().await {
        all.extend_from_slice(&ch.unwrap());
    }
    assert_eq!(
        String::from_utf8(all).unwrap(),
        "chunk0;chunk1;chunk2;chunk3;chunk4;"
    );
    assert!(
        t_first < st.elapsed(),
        "first chunk arrived before the stream finished (response streamed through)"
    );

    println!("\n==== 6. strategies ====");
    reset_hits(&[&c1, &c2, &c3]);
    let seq: Vec<String> = {
        let mut v = vec![];
        for _ in 0..30 {
            v.push(served_by(&Resp {
                status: 0,
                headers: c.get(p("/rr")).send().await.unwrap().headers().clone(),
                body: String::new(),
            }));
        }
        v
    };
    println!("   round-robin first 6: {:?}", &seq[..6]);
    assert_eq!(
        (
            c1.hits.load(SeqCst),
            c2.hits.load(SeqCst),
            c3.hits.load(SeqCst)
        ),
        (10, 10, 10)
    );
    assert!(
        seq.chunks(3).all(|w| {
            let mut w = w.to_vec();
            w.sort();
            w == ["b1", "b2", "b3"]
        }),
        "strict rotation"
    );
    // weighted 5:1:1
    *px.strat.lock().unwrap() = Arc::new(Weighted(Mutex::new(vec![])));
    reset_hits(&[&c1, &c2, &c3]);
    let mut seq = vec![];
    for _ in 0..70 {
        seq.push(served_by(&Resp {
            status: 0,
            headers: c.get(p("/w")).send().await.unwrap().headers().clone(),
            body: String::new(),
        }));
    }
    println!("   weighted 5:1:1 first 14: {:?}", &seq[..14]);
    assert_eq!(
        (
            c1.hits.load(SeqCst),
            c2.hits.load(SeqCst),
            c3.hits.load(SeqCst)
        ),
        (50, 10, 10)
    );
    let max_run = seq.chunk_by(|a, b| a == b).map(|r| r.len()).max().unwrap();
    assert!(
        max_run <= 4,
        "smooth: bursts are broken up by the light backends (max run {max_run}; naive weighted RR would give 5)"
    );
    // least connections, end to end: b1 is busy with a slow request
    *px.strat.lock().unwrap() = Arc::new(LeastConn);
    reset_hits(&[&c1, &c2, &c3]);
    c1.delay_ms.store(500, SeqCst);
    let slow = {
        let (c, u) = (c.clone(), p("/slow"));
        tokio::spawn(async move { c.get(u).send().await.unwrap().status().as_u16() })
    };
    while c1.inflight.load(SeqCst) == 0 {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    for _ in 0..4 {
        assert_eq!(
            served_by(&Resp {
                status: 0,
                headers: c.get(p("/fast")).send().await.unwrap().headers().clone(),
                body: String::new()
            }),
            "b2"
        );
    }
    assert_eq!(
        (
            c1.hits.load(SeqCst),
            c2.hits.load(SeqCst),
            c3.hits.load(SeqCst)
        ),
        (1, 4, 0),
        "traffic flows around the busy backend"
    );
    assert_eq!(slow.await.unwrap(), 200);
    c1.delay_ms.store(0, SeqCst);
    println!(
        "   least-connections: b1 busy => the next 4 requests went to b2 (hits b1/b2/b3 = 1/4/0)"
    );
    // consistent hashing
    let ch = Arc::new(ConsistentHash::new(&["b1", "b2", "b3"], 100));
    *px.strat.lock().unwrap() = ch.clone();
    reset_hits(&[&c1, &c2, &c3]);
    let mut owners = HashMap::new();
    for _ in 0..10 {
        *owners
            .entry(
                c.get(p("/me"))
                    .header("x-user-id", "alice")
                    .send()
                    .await
                    .unwrap()
                    .headers()["x-served-by"]
                    .to_str()
                    .unwrap()
                    .to_string(),
            )
            .or_insert(0) += 1;
    }
    assert_eq!(
        owners.len(),
        1,
        "same user, same backend, every time: {owners:?}"
    );
    let all_ok = |dead: Option<usize>| -> Vec<View> {
        (0..3)
            .map(|i| View {
                idx: i,
                weight: 1,
                active: 0,
                ok: Some(i) != dead,
            })
            .collect()
    };
    let keys: Vec<String> = (0..300).map(|i| format!("user-{i}")).collect();
    let before: Vec<usize> = keys
        .iter()
        .map(|k| ch.pick(k, &all_ok(None)).unwrap())
        .collect();
    let mut dist = [0usize; 3];
    for b in &before {
        dist[*b] += 1;
    }
    println!("   300 users over 3 backends: {dist:?}");
    assert!(
        dist.iter().all(|n| (60..=140).contains(n)),
        "balanced within 20-47%: {dist:?}"
    );
    let after: Vec<usize> = keys
        .iter()
        .map(|k| ch.pick(k, &all_ok(Some(1))).unwrap())
        .collect();
    let moved = before.iter().zip(&after).filter(|(a, b)| a != b).count();
    assert!(
        before.iter().zip(&after).all(|(a, b)| a == b || *a == 1),
        "only the dead backend's users move"
    );
    assert_eq!(moved, dist[1]);
    assert!(after.iter().all(|a| *a != 1));
    println!(
        "   b2 removed: exactly its {moved} users moved; the other {} stay sticky (naive `hash % n` would reshuffle ~2/3)",
        300 - moved
    );
    let modn_moved = keys
        .iter()
        .filter(|k| (hash64(k) % 3) != (hash64(k) % 2))
        .count();
    assert!(
        modn_moved > moved,
        "modulo hashing moves more keys ({modn_moved})"
    );

    println!("\n==== 7. active health checks ====");
    *px.strat.lock().unwrap() = Arc::new(RoundRobin(AtomicUsize::new(0)));
    c2.up.store(false, SeqCst);
    px.health_round().await;
    assert!(
        px.bs[1].healthy.load(SeqCst),
        "one failed probe is not enough (threshold 2)"
    );
    px.health_round().await;
    assert!(!px.bs[1].healthy.load(SeqCst));
    reset_hits(&[&c1, &c2, &c3]);
    for _ in 0..12 {
        c.get(p("/hc")).send().await.unwrap();
    }
    assert_eq!(
        (
            c1.hits.load(SeqCst),
            c2.hits.load(SeqCst),
            c3.hits.load(SeqCst)
        ),
        (6, 0, 6),
        "b2 out of rotation"
    );
    c2.up.store(true, SeqCst);
    px.health_round().await;
    assert!(
        !px.bs[1].healthy.load(SeqCst),
        "recovery also needs 2 good probes"
    );
    px.health_round().await;
    assert!(px.bs[1].healthy.load(SeqCst));
    println!(
        "   b2: healthy -> down after 2 bad probes -> healthy after 2 good probes; traffic 6/0/6 while down"
    );

    println!("\n==== 8. retries, ejection, timeouts, error mapping ====");
    *px.strat.lock().unwrap() = Arc::new(LeastConn); // deterministic: ties go to b1
    // (a) upstream 503 on an idempotent GET is retried on another backend; the client never sees it
    c1.fail503.store(true, SeqCst);
    reset_hits(&[&c1, &c2, &c3]);
    let r = call(
        &c,
        c.get(p("/flaky")),
        "GET: b1 answers 503 => proxy retries on b2",
    )
    .await;
    assert_eq!(
        (
            r.status,
            served_by(&r).as_str(),
            c1.hits.load(SeqCst),
            c2.hits.load(SeqCst)
        ),
        (200, "b2", 1, 1)
    );
    // (b) the same 503 on a POST is passed through: the request may have been processed
    reset_hits(&[&c1, &c2, &c3]);
    let r = call(
        &c,
        c.post(p("/orders")).body("{\"sku\":1}"),
        "POST: b1 answers 503 => NOT retried",
    )
    .await;
    assert_eq!(
        (r.status, c1.hits.load(SeqCst), c2.hits.load(SeqCst)),
        (503, 1, 0)
    );
    c1.fail503.store(false, SeqCst);
    // 500 (an application bug) is never retried, even for GET
    c1.fail500.store(true, SeqCst);
    reset_hits(&[&c1, &c2, &c3]);
    let r = c.get(p("/bug")).send().await.unwrap();
    assert_eq!(
        (
            r.status().as_u16(),
            c1.hits.load(SeqCst),
            c2.hits.load(SeqCst)
        ),
        (500, 1, 0)
    );
    c1.fail500.store(false, SeqCst);
    // (c) b1 is slow: GET times out on b1, retried on b2; POST gets 504 and is not retried
    px.timeout_ms.store(300, SeqCst);
    c1.delay_ms.store(900, SeqCst);
    reset_hits(&[&c1, &c2, &c3]);
    let r = call(
        &c,
        c.get(p("/slowget")),
        "GET with a 300ms upstream timeout; b1 takes 900ms",
    )
    .await;
    assert_eq!((r.status, served_by(&r).as_str()), (200, "b2"));
    let r = call(
        &c,
        c.post(p("/slowpost")).body("x"),
        "POST to the slow backend => 504, no retry",
    )
    .await;
    assert_eq!(
        (r.status, r.json()["error"].as_str()),
        (504, Some("upstream timed out"))
    );
    c1.delay_ms.store(0, SeqCst);
    px.timeout_ms.store(2000, SeqCst);
    // (d) b3 crashes: connect errors are retried for ANY method (nothing was sent); 3 failures eject it
    px.bs[0].healthy.store(false, SeqCst); // steer: b1 out, so b2/b3 alternate by least-conn tie -> b2 first
    s2.stop().await; // b2 dies; its listener is gone => connection refused
    reset_hits(&[&c1, &c2, &c3]);
    let t0 = px.bs[1].tries.load(SeqCst);
    for i in 0..4 {
        let r = if i % 2 == 0 {
            c.get(p("/after-crash")).send().await.unwrap()
        } else {
            c.post(p("/after-crash")).body("{}").send().await.unwrap()
        };
        assert_eq!(
            (
                r.status().as_u16(),
                r.headers()["x-served-by"].to_str().unwrap().to_string()
            ),
            (200, "b3".to_string()),
            "request {i} survived the crash (retry on b3)"
        );
    }
    let tried = px.bs[1].tries.load(SeqCst) - t0;
    println!(
        "   b2 was attempted {tried} times, then ejected for {}s; GET and POST both succeeded via b3",
        px.eject_ms / 1000
    );
    assert_eq!(
        tried, 3,
        "3 consecutive failures => ejected (passive health), requests 4+ never touch it"
    );
    assert!(px.now() < px.bs[1].ejected_until.load(SeqCst));
    clock.fetch_add(30_001, SeqCst);
    let before = px.bs[1].tries.load(SeqCst);
    let r = c.get(p("/probe")).send().await.unwrap();
    assert_eq!(r.status().as_u16(), 200);
    assert_eq!(
        px.bs[1].tries.load(SeqCst),
        before + 1,
        "after the ejection time the proxy tries b2 again (and, still dead, fails over and re-ejects after 3 more failures)"
    );
    // (e) nothing healthy at all => 503 + Retry-After; unreachable single upstream => 502
    for b in &px.bs {
        b.healthy.store(false, SeqCst);
    }
    let r = call(&c, c.get(p("/anything")), "no healthy upstream").await;
    assert_eq!((r.status, r.h("retry-after").as_str()), (503, "5"));
    let dead = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead_url = format!("http://{}", dead.local_addr().unwrap());
    drop(dead);
    let px2 = new_px(
        &c,
        &[("ghost", dead_url, 1)],
        Arc::new(RoundRobin(AtomicUsize::new(0))),
        clock.clone(),
    );
    let proxy2 = start_proxy(px2).await;
    let r = call(
        &c,
        c.post(proxy2.url("/x")).body("{}"),
        "single upstream, connection refused",
    )
    .await;
    assert_eq!(
        (r.status, r.json()["error"].as_str()),
        (502, Some("upstream unreachable"))
    );
    proxy2.stop().await;

    proxy_srv.stop().await;
    s1.stop().await;
    s3.stop().await;
    println!("\nOK: all reverse proxy / load balancer assertions passed");
}
