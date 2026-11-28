//! RECIPE 306 (web): webhooks end to end: HMAC signatures, retries with backoff, replay protection, dead letters
//! LEVEL: advanced
//! Use in: webhook stage (event delivery to customer endpoints, payment/Git-host style callbacks).   Run: cargo run --release --example webhooks_signatures
//!
//! What: a SENDER that signs and delivers events, and a RECEIVER that must not trust the network.
//!  * Signature (Stripe style): header `Webhook-Signature: t=<unix ts>,v1=<hex HMAC-SHA256(secret, "<t>.<raw body>")>`.
//!    Signing the timestamp INTO the MAC is what makes the replay window enforceable.
//!  * Receiver: verify over the RAW BYTES (not parsed-and-reserialized JSON), compare without an early exit, reject
//!    a timestamp outside +/-300s (limits replay), DEDUPLICATE by event id (the sender is at-least-once), answer 2xx
//!    only after the event has been handed on, accept TWO active secrets so a secret can be ROTATED.
//!  * NOT FOR PRODUCTION: SHA-256 and HMAC are hand-written here. `ct_eq` has no early exit in the source, but the
//!    compiler gives no timing guarantee. Use your provider's library, or the `hmac` crate's `verify_slice`.
//!    The queue and the set of seen ids live in memory, so a crash loses both. A real receiver stores them durably
//!    BEFORE it answers 2xx.
//!  * Statuses: 408/425/429/5xx and transport errors => the sender retries, up to its attempt limit. Any other
//!    answer goes to the dead-letter list at once: a redirect, 409 (a known id with another body), and 400, also
//!    the 400 that a genuine delivery gets when the two clocks differ by more than the window.
//!  * Only the FIRST `Webhook-Signature` header is read.
//!  * Sender: exponential backoff with jitter, an injectable clock/sleeper (no real sleeping: virtual time), a max
//!    attempt budget and a DEAD-LETTER queue that can be re-driven. A fresh signature is made for every attempt.
//!  * Attacks shown being rejected: modified body, replayed old timestamp, wrong secret, missing/garbled header, and
//!    the classic bug of verifying a re-serialized JSON value.
//!
//! Retries can deliver an event twice, so the receiver must ignore repeats.
//! COMMON USES: payment providers (Stripe), Git hosts (GitHub X-Hub-Signature-256), chat/CRM/CI integrations,
//! your own platform's outbound event notifications.
//! Read: https://stripe.com/docs/webhooks , https://datatracker.ietf.org/doc/html/rfc9110 ,
//! https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Retry-After
#![allow(dead_code)]
use axum::{
    Router,
    body::{Body, Bytes},
    extract::State,
    http::HeaderMap,
    response::{Redirect, Response},
    routing::post,
};
use reqwest::{Client, RequestBuilder, redirect};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::SeqCst},
    },
    time::Duration,
};
use tokio::{
    net::TcpListener,
    sync::{Semaphore, mpsc, oneshot},
    task::JoinHandle,
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
/// Equality with no early exit on the contents. The length is checked first: the expected length is public.
/// `black_box` is only a hint, so this is NOT a promise of constant time in the compiled program.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let diff = a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y));
    std::hint::black_box(diff) == 0
}

const T0: u64 = 1_700_000_000;
const TOLERANCE: u64 = 300;
const MAX_HEADER: usize = 1024;
const MAX_SIGNATURES: usize = 4;

// ---------------------------------------------------------------- signing / verification
fn sign_header(secrets: &[&str], t: u64, body: &[u8]) -> String {
    let mut signed = format!("{t}.").into_bytes();
    signed.extend_from_slice(body);
    let mut h = format!("t={t}");
    for s in secrets {
        h.push_str(&format!(",v1={}", hex(&hmac_sha256(s.as_bytes(), &signed))));
    }
    h
}

fn verify(
    secrets: &[String],
    header: Option<&str>,
    raw: &[u8],
    now: u64,
) -> Result<(), &'static str> {
    const BAD: &str = "malformed signature header";
    let header = header.ok_or("missing signature header")?;
    if header.len() > MAX_HEADER {
        return Err(BAD);
    }
    let (mut t, mut sigs) = (None, vec![]);
    for part in header.split(',') {
        match part.trim().split_once('=') {
            Some(("t", v)) => {
                if t.replace(v).is_some() {
                    return Err(BAD); // exactly one `t`
                }
            }
            Some(("v1", v)) => sigs.push(v),
            _ => {}
        }
    }
    let t_text = t.ok_or(BAD)?;
    if sigs.is_empty() || sigs.len() > MAX_SIGNATURES {
        return Err(BAD);
    }
    // digits only: `parse` alone would also accept "+17"
    if !t_text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(BAD);
    }
    let t: u64 = t_text.parse().map_err(|_| BAD)?;
    // the MAC covers the timestamp's text as sent, a dot, and the exact bytes of the body
    let mut signed = format!("{t_text}.").into_bytes();
    signed.extend_from_slice(raw);
    let mut ok = false;
    // an empty secret is a configuration mistake: anyone can sign with it
    for s in secrets.iter().filter(|s| !s.is_empty()) {
        let expected = hex(&hmac_sha256(s.as_bytes(), &signed));
        for got in &sigs {
            ok |= ct_eq(expected.as_bytes(), got.as_bytes()); // `|=` does not short-circuit
        }
    }
    if !ok {
        return Err("no matching signature");
    }
    // both directions: too old, and too far in the future
    if now.abs_diff(t) > TOLERANCE {
        return Err("timestamp outside tolerance");
    }
    Ok(())
}

// ---------------------------------------------------------------- receiver
struct Rx {
    secrets: RwLock<Vec<String>>,
    clock: Arc<AtomicU64>,
    seen: Mutex<HashMap<String, Bytes>>, // event id -> the body it was accepted with
    queue: mpsc::UnboundedSender<String>,
    hits: AtomicU32,
    fail_left: AtomicU32,
    fail_status: AtomicU32,
    fail_after_processing: AtomicBool, // simulate "processed it, but the ack was lost"
}

fn text(status: u16, msg: &str) -> Response {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(json!({"status": msg}).to_string()))
        .unwrap()
}

fn accept(rx: &Rx, headers: &HeaderMap, body: &Bytes) -> (u16, &'static str) {
    accept_with(rx, headers, body, |id| rx.queue.send(id).is_ok())
}

/// `hand_on` passes the id to whatever does the work, and says whether it took it. A check supplies its own.
fn accept_with(
    rx: &Rx,
    headers: &HeaderMap,
    body: &Bytes,
    hand_on: impl FnOnce(String) -> bool,
) -> (u16, &'static str) {
    let secrets = rx.secrets.read().unwrap().clone();
    // `get` returns the first header of this name; any later one is ignored
    let sig = headers
        .get("webhook-signature")
        .and_then(|v| v.to_str().ok());
    if let Err(e) = verify(&secrets, sig, body, rx.clock.load(SeqCst)) {
        return (400, e);
    }
    let Ok(v) = serde_json::from_slice::<Value>(body) else {
        return (400, "body is not JSON");
    }; // parse only AFTER verifying
    let Some(id) = v["id"].as_str() else {
        return (400, "missing event id");
    };
    // one lock around "seen?", the hand-off and the mark, so a second copy cannot slip between them
    let mut seen = rx.seen.lock().unwrap();
    match seen.get(id) {
        Some(first) if first == body => return (200, "duplicate ignored"),
        // not a retry of what was accepted: keep the first, and let a person look
        Some(_) => return (409, "known id with another body"),
        None => {}
    }
    if !hand_on(id.to_string()) {
        return (503, "could not hand the event on"); // not marked as seen: the retry must get through
    }
    seen.insert(id.to_string(), body.clone());
    (200, "accepted")
}

async fn hook(State(rx): State<Arc<Rx>>, headers: HeaderMap, body: Bytes) -> Response {
    rx.hits.fetch_add(1, SeqCst);
    let fail = rx
        .fail_left
        .fetch_update(SeqCst, SeqCst, |n| n.checked_sub(1))
        .is_ok();
    let after = rx.fail_after_processing.load(SeqCst);
    if fail && !after {
        return text(rx.fail_status.load(SeqCst) as u16, "simulated outage");
    }
    let (s, m) = accept(&rx, &headers, &body);
    if fail && after {
        return text(500, "ack lost");
    }
    text(s, m)
}

/// THE BUG: verify a re-serialized value instead of the raw bytes. Valid deliveries get rejected.
async fn hook_naive(State(rx): State<Arc<Rx>>, headers: HeaderMap, body: Bytes) -> Response {
    let v: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let reserialized = serde_json::to_string(&v).unwrap();
    let secrets = rx.secrets.read().unwrap().clone();
    let sig = headers
        .get("webhook-signature")
        .and_then(|v| v.to_str().ok());
    match verify(
        &secrets,
        sig,
        reserialized.as_bytes(),
        rx.clock.load(SeqCst),
    ) {
        Ok(()) => text(200, "accepted"),
        Err(e) => text(400, e),
    }
}

// ---------------------------------------------------------------- sender
struct Dead {
    event_id: String,
    attempts: u32,
    reason: String,
    payload: String,
}
struct Sender {
    c: Client,
    url: String,
    secrets: Vec<String>, // sign with ALL of them while rotating (receiver accepts any match)
    clock: Arc<AtomicU64>, // injectable (virtual) clock
    max_attempts: u32,
    base: u64,
    cap: u64,
    rng: u64,
    delays: Vec<u64>,
    dlq: Vec<Dead>,
}
fn retryable(status: u16) -> bool {
    matches!(status, 408 | 425 | 429 | 500..=599)
}

impl Sender {
    fn new(c: &Client, url: String, secrets: &[&str], clock: Arc<AtomicU64>) -> Sender {
        Sender {
            c: c.clone(),
            url,
            secrets: secrets.iter().map(|s| s.to_string()).collect(),
            clock,
            max_attempts: 5,
            base: 30,
            cap: 3600,
            rng: 0x9E3779B97F4A7C15,
            delays: vec![],
            dlq: vec![],
        }
    }
    fn next_rand(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }
    /// Exponential backoff with "equal jitter": delay in [ceiling/2, ceiling].
    fn backoff(&mut self, failed_attempt: u32) -> u64 {
        // a plain shift would lose high bits (or panic) after many attempts
        let doubled = 1u64
            .checked_shl(failed_attempt.saturating_sub(1))
            .and_then(|factor| self.base.checked_mul(factor));
        let ceil = doubled.unwrap_or(self.cap).min(self.cap);
        ceil / 2 + self.next_rand() % (ceil / 2 + 1)
    }
    async fn post_once(&self, payload: &str) -> Result<u16, String> {
        let secrets: Vec<&str> = self.secrets.iter().map(String::as_str).collect();
        let sig = sign_header(&secrets, self.clock.load(SeqCst), payload.as_bytes()); // fresh timestamp per attempt
        let r = self
            .c
            .post(&self.url)
            .header("webhook-signature", sig)
            .header("content-type", "application/json")
            .body(payload.to_string())
            .send()
            .await;
        r.map(|r| r.status().as_u16()).map_err(|e| e.to_string())
    }
    /// Returns the number of attempts used; failed events land in the DLQ.
    async fn deliver(&mut self, event_id: &str, payload: &str) -> u32 {
        let mut attempt = 0;
        loop {
            attempt += 1;
            let res = self.post_once(payload).await;
            match &res {
                Ok(s) if (200..300).contains(s) => {
                    println!("   [sender] {event_id} attempt {attempt}: {s} delivered");
                    return attempt;
                }
                Ok(s) if !retryable(*s) => {
                    println!(
                        "   [sender] {event_id} attempt {attempt}: {s} permanent failure -> dead letter"
                    );
                    self.dlq.push(Dead {
                        event_id: event_id.into(),
                        attempts: attempt,
                        reason: format!("HTTP {s}"),
                        payload: payload.into(),
                    });
                    return attempt;
                }
                other => {
                    if attempt >= self.max_attempts {
                        println!(
                            "   [sender] {event_id} attempt {attempt}: {other:?} out of attempts -> dead letter"
                        );
                        self.dlq.push(Dead {
                            event_id: event_id.into(),
                            attempts: attempt,
                            reason: format!("{other:?}"),
                            payload: payload.into(),
                        });
                        return attempt;
                    }
                    let d = self.backoff(attempt);
                    println!(
                        "   [sender] {event_id} attempt {attempt}: {other:?} -> retry in {d}s (virtual)"
                    );
                    self.delays.push(d);
                    self.clock.fetch_add(d, SeqCst); // the injectable sleeper: advance virtual time, do not sleep
                }
            }
        }
    }
}

async fn wait_for(f: impl Fn() -> bool) {
    for _ in 0..400 {
        if f() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("condition not reached in time");
}

// ---------------------------------------------------------------- checks without a network

struct XorShift(u64);
impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| (self.next() >> 24) as u8).collect()
    }
}

/// Compare with an independent implementation. The constants come from Python's `hmac` and `hashlib`:
///
/// ```text
/// import hashlib, hmac
/// x, M = 0x9E3779B97F4A7C15, (1 << 64) - 1
/// def rand(n):
///     global x
///     out = bytearray()
///     for _ in range(n):
///         x ^= (x << 13) & M; x ^= x >> 7; x ^= (x << 17) & M
///         out.append((x >> 24) & 255)
///     return bytes(out)
/// macs = bytearray()
/// for k in [0, 1, 31, 32, 55, 56, 63, 64, 65, 100, 127, 128, 129, 200]:
///     for m in range(301):
///         key = rand(k); msg = rand(m)
///         macs += hmac.new(key, msg, hashlib.sha256).digest()
/// print(hashlib.sha256(macs).hexdigest())
/// print(hmac.new(b"whsec_test", b'1700000000.{"id":"evt_1"}', hashlib.sha256).hexdigest())
/// ```
fn check_against_python() {
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    let mut macs = Vec::new();
    for k in [0, 1, 31, 32, 55, 56, 63, 64, 65, 100, 127, 128, 129, 200] {
        for m in 0..=300 {
            let key = rng.bytes(k);
            let msg = rng.bytes(m);
            macs.extend_from_slice(&hmac_sha256(&key, &msg));
        }
    }
    assert_eq!(
        hex(&sha256(&macs)),
        "db448c612b22ea1d7d79ab5e98af4e736552bcb5112d87e443e30fd67ee0befb"
    );
    assert_eq!(
        sign_header(&["whsec_test"], T0, br#"{"id":"evt_1"}"#),
        "t=1700000000,v1=c89214b5b5da833daed6f0b8c5bb6bd58cea9022bd80ccc78230f3942d632925"
    );
}

fn check_verifier() {
    // a longer string that continues with 256 zero bytes is not equal
    let long = [&b"a"[..], &[0u8; 256]].concat();
    assert!(!ct_eq(b"a", &long) && !ct_eq(&long, b"a"));

    let secrets = vec!["whsec_test".to_string()];
    let body = br#"{"id":"evt_1"}"#;
    let good = sign_header(&["whsec_test"], T0, body);
    let mac = good.split_once("v1=").unwrap().1.to_string();
    let check = |h: &str, now: u64| verify(&secrets, Some(h), body, now);
    assert_eq!(check(&good, T0), Ok(()));
    // The window includes its edges and has two sides. So a request stamped ahead of this clock, by no more than
    // the tolerance, passes during twice the tolerance: here from T0 - 300 until T0 + 300.
    for now in [T0 - TOLERANCE, T0 + TOLERANCE] {
        assert_eq!(check(&good, now), Ok(()));
    }
    for now in [T0 - TOLERANCE - 1, T0 + TOLERANCE + 1, 0, u64::MAX] {
        assert_eq!(check(&good, now), Err("timestamp outside tolerance"));
    }
    // the same number written another way is not what was signed
    let plus = format!("t=+{T0},v1={mac}");
    assert_eq!(check(&plus, T0), Err("malformed signature header"));
    let zero = format!("t=0{T0},v1={mac}");
    assert_eq!(check(&zero, T0), Err("no matching signature"));
    let twice = format!("t=1,t={T0},v1={mac}");
    assert_eq!(check(&twice, T0), Err("malformed signature header"));
    // a second signature may be the one that matches
    let two = format!("t={T0},v1={},v1={mac}", "0".repeat(64));
    assert_eq!(check(&two, T0), Ok(()));
    let many = format!("t={T0}{}", format!(",v1={mac}").repeat(MAX_SIGNATURES + 1));
    assert_eq!(check(&many, T0), Err("malformed signature header"));
    let huge = format!("{good},x={}", "a".repeat(MAX_HEADER));
    assert_eq!(check(&huge, T0), Err("malformed signature header"));
    // no prefix of the MAC passes, not even the empty one
    for n in 0..mac.len() {
        let short = format!("t={T0},v1={}", &mac[..n]);
        assert_eq!(check(&short, T0), Err("no matching signature"));
    }
    // an empty secret never verifies anything
    let forged = sign_header(&[""], T0, body);
    let empty = vec![String::new()];
    assert!(verify(&empty, Some(&forged), body, T0).is_err());
    assert!(verify(&[], Some(&good), body, T0).is_err());

    // seeded damage: whatever passes must still hold the signed `t` and the true MAC
    let mut rng = XorShift(7);
    let alphabet = [
        't', 'v', '1', '=', ',', ' ', '+', '0', '7', 'a', 'F', '.', '\0', 'é', '€',
    ];
    let mut passed = 0;
    for _ in 0..4_000 {
        let mut chars: Vec<char> = good.chars().collect();
        for _ in 0..1 + rng.next() % 3 {
            let at = (rng.next() % chars.len() as u64) as usize;
            let c = alphabet[(rng.next() % alphabet.len() as u64) as usize];
            match rng.next() % 4 {
                0 => chars[at] = c,
                1 => chars.insert(at, c),
                2 => drop(chars.remove(at)),
                _ => chars.extend([',', c]),
            }
        }
        let h: String = chars.into_iter().collect();
        if check(&h, T0).is_ok() {
            passed += 1;
            let parts: Vec<&str> = h.split(',').map(str::trim).collect();
            assert!(parts.contains(&format!("t={T0}").as_str()), "{h:?}");
            assert!(parts.contains(&format!("v1={mac}").as_str()), "{h:?}");
        }
    }
    assert!(passed > 0, "some damage is harmless, such as an added pair");
}

/// A receiver that cannot hand an event on must not say 2xx, and must accept the retry later.
fn check_hand_off_failure() {
    let (closed, gone) = mpsc::unbounded_channel::<String>();
    drop(gone);
    let mut rx = Rx {
        secrets: RwLock::new(vec!["whsec_test".into()]),
        clock: Arc::new(AtomicU64::new(T0)),
        seen: Mutex::new(HashMap::new()),
        queue: closed,
        hits: AtomicU32::new(0),
        fail_left: AtomicU32::new(0),
        fail_status: AtomicU32::new(503),
        fail_after_processing: AtomicBool::new(false),
    };
    let body = Bytes::from_static(br#"{"id":"evt_1"}"#);
    let mut headers = HeaderMap::new();
    let signature = sign_header(&["whsec_test"], T0, &body);
    headers.insert("webhook-signature", signature.parse().unwrap());
    assert_eq!(accept(&rx, &headers, &body).0, 503);
    let (open, mut queued) = mpsc::unbounded_channel::<String>();
    rx.queue = open;
    assert_eq!(accept(&rx, &headers, &body), (200, "accepted"));
    assert_eq!(queued.try_recv().ok().as_deref(), Some("evt_1"));
    assert_eq!(accept(&rx, &headers, &body), (200, "duplicate ignored"));
    assert!(queued.try_recv().is_err());

    // a validly signed request that reuses the id for another body is not a duplicate
    let changed = Bytes::from_static(br#"{"id":"evt_1","amount":9}"#);
    let mut other = HeaderMap::new();
    let signature = sign_header(&["whsec_test"], T0, &changed);
    other.insert("webhook-signature", signature.parse().unwrap());
    assert_eq!(accept(&rx, &other, &changed).0, 409);
    assert!(queued.try_recv().is_err());

    // only the first signature header counts
    let mut two = HeaderMap::new();
    two.append("webhook-signature", "t=0,v1=00".parse().unwrap());
    two.append("webhook-signature", headers["webhook-signature"].clone());
    assert_eq!(accept(&rx, &two, &body).0, 400);
}

/// The rule: no copy gets a 2xx for an id until a hand-off of that id has succeeded. `accept_with` keeps it
/// by holding one lock from "seen?" through the hand-off to the mark.
///
/// This check catches ONE way to break it: ask, hand off WITHOUT the lock, then mark. The first hand-off pauses
/// until a second copy reaches its own, or until 300 ms pass, so correct code does not fail here. (On a very
/// busy machine the second thread could start too late, and that split would pass.)
///
/// NOT caught, and both wrong:
///  * ask, unlock, then lock again to hand off and mark: both copies can pass "seen?" and both are handed on;
///  * mark first, unlock, hand off, unmark on failure: the second copy gets 200 while the first can still
///    fail, so the sender may stop retrying an event that was never handed on.
fn check_two_copies_at_once() {
    use std::sync::mpsc::channel;
    let (queue, _unused) = mpsc::unbounded_channel::<String>();
    let rx = Rx {
        secrets: RwLock::new(vec!["whsec_test".into()]),
        clock: Arc::new(AtomicU64::new(T0)),
        seen: Mutex::new(HashMap::new()),
        queue,
        hits: AtomicU32::new(0),
        fail_left: AtomicU32::new(0),
        fail_status: AtomicU32::new(503),
        fail_after_processing: AtomicBool::new(false),
    };
    let body = Bytes::from_static(br#"{"id":"evt_1"}"#);
    let mut headers = HeaderMap::new();
    let signature = sign_header(&["whsec_test"], T0, &body);
    headers.insert("webhook-signature", signature.parse().unwrap());
    let (first_inside, wait_for_first) = channel();
    let (second_inside, wait_for_second) = channel();
    let (rx, headers, body) = (&rx, &headers, &body);
    let handed_on = &AtomicU32::new(0);
    std::thread::scope(|s| {
        let first = s.spawn(move || {
            accept_with(rx, headers, body, |_| {
                first_inside.send(()).unwrap();
                let _ = wait_for_second.recv_timeout(Duration::from_millis(300));
                handed_on.fetch_add(1, SeqCst);
                true
            })
        });
        wait_for_first.recv().unwrap();
        let second = s.spawn(move || {
            accept_with(rx, headers, body, |_| {
                let _ = second_inside.send(());
                handed_on.fetch_add(1, SeqCst);
                true
            })
        });
        assert_eq!(first.join().unwrap(), (200, "accepted"));
        assert_eq!(second.join().unwrap(), (200, "duplicate ignored"));
    });
    assert_eq!(handed_on.load(SeqCst), 1, "handed on once");
}

fn check_backoff(c: &Client) {
    let mut s = Sender::new(c, String::new(), &[], Arc::new(AtomicU64::new(T0)));
    for attempt in 1..=200 {
        let delay = s.backoff(attempt);
        let low = if attempt >= 8 { s.cap / 2 } else { 15 };
        assert!((low..=s.cap).contains(&delay), "attempt {attempt}: {delay}");
    }
}

fn event(id: &str, amount: u32) -> String {
    json!({"id": id, "type": "invoice.paid", "amount": amount}).to_string()
}

#[tokio::main]
async fn main() {
    // sanity: HMAC-SHA256 RFC 4231 test case 2
    assert_eq!(
        hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
    assert!(ct_eq(b"abc", b"abc") && !ct_eq(b"abc", b"abd") && !ct_eq(b"abc", b"ab"));
    check_against_python();
    check_verifier();
    check_hand_off_failure();
    check_two_copies_at_once();

    let clock = Arc::new(AtomicU64::new(T0));
    let (qtx, mut qrx) = mpsc::unbounded_channel::<String>();
    let rx = Arc::new(Rx {
        secrets: RwLock::new(vec!["whsec_old_7f3a".into()]),
        clock: clock.clone(),
        seen: Mutex::new(HashMap::new()),
        queue: qtx,
        hits: AtomicU32::new(0),
        fail_left: AtomicU32::new(0),
        fail_status: AtomicU32::new(503),
        fail_after_processing: AtomicBool::new(false),
    });
    // asynchronous processing: the worker is gated so we can PROVE the HTTP ack does not wait for it
    let processed = Arc::new(Mutex::new(Vec::<String>::new()));
    let gate = Arc::new(Semaphore::new(0));
    {
        let (p, g) = (processed.clone(), gate.clone());
        tokio::spawn(async move {
            while let Some(id) = qrx.recv().await {
                g.acquire().await.unwrap().forget(); // pretend this is slow work (DB writes, emails...)
                p.lock().unwrap().push(id);
            }
        });
    }
    let srv = Srv::start(
        Router::new()
            .route("/webhook", post(hook))
            .route("/webhook-naive", post(hook_naive))
            .route("/moved", post(|| async { Redirect::temporary("/webhook") }))
            .with_state(rx.clone()),
    )
    .await;
    let c = Client::builder()
        .no_proxy()
        .redirect(redirect::Policy::none()) // a signed body goes only to the address you chose
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    check_backoff(&c);
    let mut moved = Sender::new(&c, srv.url("/moved"), &["whsec_old_7f3a"], clock.clone());
    moved.deliver("evt_000", &event("evt_000", 0)).await;
    assert_eq!(
        moved.dlq[0].reason, "HTTP 307",
        "redirects are not followed"
    );
    let url = srv.url("/webhook");
    let mut tx = Sender::new(&c, url.clone(), &["whsec_old_7f3a"], clock.clone());

    println!("==== 1. a signed delivery, as the wire sees it ====");
    let body = event("evt_001", 1200);
    let sig = sign_header(&["whsec_old_7f3a"], T0, body.as_bytes());
    let r = call(
        &c,
        c.post(&url)
            .header("webhook-signature", &sig)
            .header("content-type", "application/json")
            .body(body.clone()),
        "sender -> receiver (receiver ACKs before processing)",
    )
    .await;
    assert_eq!(
        (r.status, r.json()["status"].as_str()),
        (200, Some("accepted"))
    );
    assert!(sig.starts_with("t=1700000000,v1=") && sig.len() == "t=1700000000,v1=".len() + 64);
    assert_eq!(
        processed.lock().unwrap().len(),
        0,
        "worker is still blocked: the ack did not wait for processing"
    );
    gate.add_permits(1000);
    wait_for(|| processed.lock().unwrap().len() == 1).await;
    println!(
        "   processed asynchronously after the ack: {:?}",
        processed.lock().unwrap()
    );

    println!("\n==== 2. duplicate delivery is acknowledged but processed once ====");
    let r = call(
        &c,
        c.post(&url)
            .header(
                "webhook-signature",
                sign_header(&["whsec_old_7f3a"], T0, body.as_bytes()),
            )
            .body(body.clone()),
        "same event id again",
    )
    .await;
    assert_eq!(r.json()["status"], "duplicate ignored");
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(processed.lock().unwrap().len(), 1);

    println!("\n==== 3. attacks are rejected (400, nothing processed) ====");
    let before = processed.lock().unwrap().len();
    let good = event("evt_100", 50);
    let good_sig = sign_header(&["whsec_old_7f3a"], T0, good.as_bytes());
    let tampered = event("evt_100", 5_000_000);
    let r = call(
        &c,
        c.post(&url)
            .header("webhook-signature", &good_sig)
            .body(tampered),
        "ATTACK 1: modified body (amount 50 -> 5000000), original signature",
    )
    .await;
    assert_eq!(
        (r.status, r.json()["status"].as_str()),
        (400, Some("no matching signature"))
    );
    let old_t = T0 - 3600;
    let r = call(
        &c,
        c.post(&url)
            .header(
                "webhook-signature",
                sign_header(&["whsec_old_7f3a"], old_t, good.as_bytes()),
            )
            .body(good.clone()),
        "ATTACK 2: replay of a captured request from 1h ago (valid MAC, stale timestamp)",
    )
    .await;
    assert_eq!(
        (r.status, r.json()["status"].as_str()),
        (400, Some("timestamp outside tolerance"))
    );
    let r = call(
        &c,
        c.post(&url)
            .header(
                "webhook-signature",
                sign_header(&["whsec_old_7f3a"], T0 + 3600, good.as_bytes()),
            )
            .body(good.clone()),
        "ATTACK 2b: timestamp far in the future",
    )
    .await;
    assert_eq!(r.status, 400);
    let r = call(
        &c,
        c.post(&url)
            .header(
                "webhook-signature",
                sign_header(&["whsec_attacker_guess"], T0, good.as_bytes()),
            )
            .body(good.clone()),
        "ATTACK 3: signed with the wrong secret",
    )
    .await;
    assert_eq!(r.status, 400);
    let r = call(
        &c,
        c.post(&url).body(good.clone()),
        "ATTACK 4a: no signature header",
    )
    .await;
    assert_eq!(r.json()["status"], "missing signature header");
    let r = call(
        &c,
        c.post(&url)
            .header("webhook-signature", "v1=zz")
            .body(good.clone()),
        "ATTACK 4b: garbled header (no timestamp)",
    )
    .await;
    assert_eq!(r.json()["status"], "malformed signature header");
    // moving the timestamp inside the header does not help: it is part of the MAC input
    let forged_t = good_sig.replace("t=1700000000", "t=1700000100");
    let r = call(
        &c,
        c.post(&url)
            .header("webhook-signature", forged_t)
            .body(good.clone()),
        "ATTACK 5: change t= to dodge the window",
    )
    .await;
    assert_eq!(r.status, 400);
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(
        processed.lock().unwrap().len(),
        before,
        "no attack caused processing"
    );
    // within the window, a replay of a delivered event is harmless because of the dedupe table
    let r = call(
        &c,
        c.post(&url)
            .header(
                "webhook-signature",
                sign_header(&["whsec_old_7f3a"], T0 + 10, body.as_bytes()),
            )
            .body(body.clone()),
        "replay of evt_001 inside the window: dedupe makes it a no-op",
    )
    .await;
    assert_eq!(r.json()["status"], "duplicate ignored");

    println!("\n==== 4. why you verify the RAW bytes ====");
    let spaced = "{\"type\": \"invoice.paid\",  \"id\": \"evt_naive\", \"amount\": 7}"; // legit JSON with odd whitespace/order
    let sig = sign_header(&["whsec_old_7f3a"], T0, spaced.as_bytes());
    let r = call(
        &c,
        c.post(srv.url("/webhook-naive"))
            .header("webhook-signature", &sig)
            .body(spaced.to_string()),
        "NAIVE receiver (parse -> re-serialize -> verify): rejects a GENUINE delivery",
    )
    .await;
    assert_eq!(r.status, 400);
    let r = call(
        &c,
        c.post(&url)
            .header("webhook-signature", &sig)
            .body(spaced.to_string()),
        "raw-bytes receiver: accepts it",
    )
    .await;
    assert_eq!(r.status, 200);
    println!(
        "   (re-serialization also makes `{{\"a\":1,\"a\":2}}`-style duplicate-key tricks invisible to the MAC)"
    );

    println!("\n==== 5. sender retries: backoff + jitter, injected clock ====");
    rx.fail_left.store(2, SeqCst);
    rx.fail_status.store(503, SeqCst);
    let h0 = rx.hits.load(SeqCst);
    let t_before = clock.load(SeqCst);
    let attempts = tx.deliver("evt_200", &event("evt_200", 10)).await;
    assert_eq!(attempts, 3, "2 failures + 1 success");
    assert_eq!(rx.hits.load(SeqCst) - h0, 3);
    assert_eq!(tx.delays.len(), 2);
    assert!(
        (15..=30).contains(&tx.delays[0]) && (30..=60).contains(&tx.delays[1]),
        "delays {:?}",
        tx.delays
    );
    assert_eq!(
        clock.load(SeqCst) - t_before,
        tx.delays.iter().sum::<u64>(),
        "virtual time advanced by exactly the backoff"
    );
    // 408 is retryable too
    rx.fail_left.store(1, SeqCst);
    rx.fail_status.store(408, SeqCst);
    assert_eq!(tx.deliver("evt_201", &event("evt_201", 11)).await, 2);
    // ack lost AFTER processing: the retry is a duplicate and must not be processed twice
    rx.fail_after_processing.store(true, SeqCst);
    rx.fail_left.store(1, SeqCst);
    assert_eq!(tx.deliver("evt_202", &event("evt_202", 12)).await, 2);
    rx.fail_after_processing.store(false, SeqCst);
    wait_for(|| {
        processed
            .lock()
            .unwrap()
            .iter()
            .filter(|i| i.as_str() == "evt_202")
            .count()
            >= 1
    })
    .await;
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(
        processed
            .lock()
            .unwrap()
            .iter()
            .filter(|i| i.as_str() == "evt_202")
            .count(),
        1,
        "effectively-once"
    );
    println!("   evt_202 was delivered twice on the wire but processed once");

    println!("\n==== 6. permanent failure vs exhausted retries -> dead-letter queue ====");
    let mut bad_tx = Sender::new(&c, url.clone(), &["whsec_wrong"], clock.clone());
    let h0 = rx.hits.load(SeqCst);
    assert_eq!(
        bad_tx.deliver("evt_300", &event("evt_300", 1)).await,
        1,
        "400 is permanent: no retry"
    );
    assert_eq!(rx.hits.load(SeqCst) - h0, 1);
    assert_eq!(bad_tx.dlq.len(), 1);
    rx.fail_left.store(1000, SeqCst);
    rx.fail_status.store(500, SeqCst);
    let h0 = rx.hits.load(SeqCst);
    assert_eq!(
        tx.deliver("evt_301", &event("evt_301", 2)).await,
        5,
        "max_attempts = 5"
    );
    assert_eq!(rx.hits.load(SeqCst) - h0, 5);
    assert_eq!(tx.dlq.len(), 1);
    println!(
        "   DLQ: {} (attempts {}, reason {})",
        tx.dlq[0].event_id, tx.dlq[0].attempts, tx.dlq[0].reason
    );
    assert_eq!(tx.delays.len(), 2 + 1 + 1 + 4);
    assert!(tx.delays[tx.delays.len() - 1] <= 240 && tx.delays[tx.delays.len() - 1] >= 120);
    // outage over: re-drive the dead letters
    rx.fail_left.store(0, SeqCst);
    for d in std::mem::take(&mut tx.dlq) {
        println!("   re-driving {} ({})", d.event_id, d.reason);
        assert_eq!(tx.deliver(&d.event_id, &d.payload).await, 1);
    }
    assert!(tx.dlq.is_empty());
    wait_for(|| processed.lock().unwrap().iter().any(|i| i == "evt_301")).await;

    // a sender whose clock is off by more than the window gets 400, and 400 is not retried
    let skewed = Arc::new(AtomicU64::new(clock.load(SeqCst) + TOLERANCE + 1));
    let mut late = Sender::new(&c, url.clone(), &["whsec_old_7f3a"], skewed);
    assert_eq!(late.deliver("evt_302", &event("evt_302", 2)).await, 1);
    assert_eq!(
        late.dlq[0].reason, "HTTP 400",
        "a genuine event, dead at once"
    );

    println!("\n==== 7. secret rotation with two active secrets ====");
    // step 1: receiver learns the new secret (accepts both); senders still use old
    rx.secrets.write().unwrap().push("whsec_new_91bc".into());
    let mut old_tx = Sender::new(&c, url.clone(), &["whsec_old_7f3a"], clock.clone());
    assert_eq!(old_tx.deliver("evt_400", &event("evt_400", 3)).await, 1);
    // step 2: sender switches to the new secret
    let mut new_tx = Sender::new(&c, url.clone(), &["whsec_new_91bc"], clock.clone());
    assert_eq!(new_tx.deliver("evt_401", &event("evt_401", 4)).await, 1);
    // alternative: sender signs with both (two v1= entries): works against a receiver that knows either
    let both = sign_header(
        &["whsec_old_7f3a", "whsec_new_91bc"],
        clock.load(SeqCst),
        b"x",
    );
    assert_eq!(both.matches("v1=").count(), 2);
    println!("   dual-signed header: {}...", &both[..40]);
    // step 3: the old secret is retired
    rx.secrets
        .write()
        .unwrap()
        .retain(|s| s != "whsec_old_7f3a");
    let r = call(
        &c,
        c.post(&url)
            .header(
                "webhook-signature",
                sign_header(
                    &["whsec_old_7f3a"],
                    clock.load(SeqCst),
                    b"{\"id\":\"evt_402\"}",
                ),
            )
            .body("{\"id\":\"evt_402\"}"),
        "after retiring the old secret, the old key is rejected",
    )
    .await;
    assert_eq!(r.status, 400);
    assert_eq!(new_tx.deliver("evt_403", &event("evt_403", 5)).await, 1);

    println!("\n==== 8. a presigned request cannot be retried after the window ====");
    let stale_body = event("evt_500", 9);
    let stale_sig = sign_header(
        &["whsec_new_91bc"],
        clock.load(SeqCst),
        stale_body.as_bytes(),
    );
    clock.fetch_add(TOLERANCE + 1, SeqCst);
    let r = call(
        &c,
        c.post(&url)
            .header("webhook-signature", &stale_sig)
            .body(stale_body.clone()),
        "sender re-sent the OLD signed request 301s later",
    )
    .await;
    assert_eq!(
        (r.status, r.json()["status"].as_str()),
        (400, Some("timestamp outside tolerance"))
    );
    println!("   => that is why the sender re-signs with a fresh timestamp on every attempt.");

    wait_for(|| processed.lock().unwrap().len() >= 9).await;
    let mut p = processed.lock().unwrap().clone();
    p.sort();
    p.dedup();
    assert_eq!(
        p.len(),
        processed.lock().unwrap().len(),
        "no event processed twice"
    );
    println!(
        "\nprocessed exactly once each: {:?}",
        processed.lock().unwrap()
    );
    srv.stop().await;
    println!("OK: all webhook assertions passed");
}
