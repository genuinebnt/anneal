//! RECIPE 302 (web): cookies, server-side sessions, signed cookies and CSRF protection, by hand
//! LEVEL: advanced
//! Use in: login flows, web apps, any stateful HTTP service.   Run: cargo run --release --example cookies_sessions_csrf
//!
//! WHAT: (1) `Set-Cookie` serialization and parsing (Path, Domain, Max-Age, Expires, Secure, HttpOnly,
//! SameSite) and a client-side cookie JAR that applies RFC 6265 domain/path/expiry/Secure/SameSite rules
//! (we assert which cookies go to which request). (2) Server-side SESSIONS: random 256-bit id from
//! /dev/urandom, store, ROTATION on login (session-fixation defence), idle and absolute timeouts driven by an
//! injectable clock. (3) SIGNED cookies: `value.base64url(HMAC-SHA256(secret, value))`, tamper detection with a
//! constant-time compare. (4) CSRF: signed double-submit token (cookie + `X-CSRF-Token` header, bound to the
//! session by HMAC) and what SameSite=Lax/Strict do. (5) A login -> authenticated call -> logout transcript
//! through reqwest (which has NO cookie feature here) with the jar handled manually.
//! WHY: cookies are ambient authority: the browser attaches them to every matching request, including ones a
//! hostile page triggers. Sessions and CSRF defences exist because of that single fact.
//! HOW: the session cookie is an opaque random id (state lives on the server, revocable); `HttpOnly` hides it
//! from scripts; no `Max-Age` makes it a browser-session cookie while the SERVER enforces idle/absolute limits;
//! `Secure` keeps it off plain HTTP (our jar refuses to send it over http://). SameSite: Strict = same-site
//! requests only; Lax (browser default) = also top-level cross-site GET navigations; None = always (needs
//! Secure). SameSite is defence in depth, the CSRF token is the primary control for state-changing requests.
//! Ids and tokens are redacted in the transcripts (`<TOKEN>`) because they are random.
//! COMMON USES: login/logout for server-rendered apps and SPAs with a same-site API; "remember me" vs session
//! cookies; signed preference/flash cookies without server storage; CSRF defence for cookie-authenticated
//! APIs; building HTTP clients/test harnesses that need cookie persistence (crawlers, API test suites).
//! Read: https://datatracker.ietf.org/doc/html/rfc6265 , https://developer.mozilla.org/en-US/docs/Web/HTTP/Cookies ,
//! https://developer.mozilla.org/en-US/docs/Web/HTTP
#![allow(dead_code)]
use axum::{
    Form, Router,
    extract::State,
    http::header,
    response::{AppendHeaders, IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
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

// ---------------------------------------------------------------- SHA-256, HMAC, base64url, hex (from scratch)
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

/// Constant-time equality: no early exit, so timing does not reveal the first differing byte.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// base64url WITHOUT padding (RFC 4648 section 5), as used by JWT and signed cookies.
fn b64u_encode(data: &[u8]) -> String {
    let mut out = String::new();
    for ch in data.chunks(3) {
        let n = (ch[0] as u32) << 16
            | (*ch.get(1).unwrap_or(&0) as u32) << 8
            | *ch.get(2).unwrap_or(&0) as u32;
        for i in 0..=ch.len() {
            out.push(B64[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

fn b64u_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes() {
        let v = B64.iter().position(|&x| x == c)? as u32;
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Random bytes from the OS (/dev/urandom); never use a seeded PRNG for session ids or tokens.
fn random_bytes(n: usize) -> Vec<u8> {
    use std::io::Read;
    let mut v = vec![0u8; n];
    std::fs::File::open("/dev/urandom")
        .unwrap()
        .read_exact(&mut v)
        .unwrap();
    v
}

// ---------------------------------------------------------------- dates
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + (m <= 2) as i64, m, d)
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// Epoch seconds -> "Sun, 06 Nov 1994 08:49:37 GMT".
fn fmt_http_date(t: i64) -> String {
    let (days, secs) = (t.div_euclid(86400), t.rem_euclid(86400));
    let (y, m, d) = civil_from_days(days);
    format!(
        "{}, {d:02} {} {y} {:02}:{:02}:{:02} GMT",
        DAYS[(days + 4).rem_euclid(7) as usize],
        MONTHS[m as usize - 1],
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

fn parse_http_date(s: &str) -> Option<i64> {
    let p: Vec<&str> = s.split_whitespace().collect();
    if p.len() != 6 || p[5] != "GMT" {
        return None;
    }
    let mon = MONTHS.iter().position(|m| *m == p[2])? as i64 + 1;
    let hms: Vec<i64> = p[4].split(':').filter_map(|x| x.parse().ok()).collect();
    (hms.len() == 3).then(|| {
        days_from_civil(
            p[3].parse().ok().unwrap_or(0),
            mon,
            p[1].parse().unwrap_or(1),
        ) * 86400
            + hms[0] * 3600
            + hms[1] * 60
            + hms[2]
    })
}

// ---------------------------------------------------------------- Set-Cookie
#[derive(Clone, Copy, PartialEq, Debug)]
enum SameSite {
    Strict,
    Lax,
    None,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Cookie {
    name: String,
    value: String,
    domain: Option<String>,
    path: Option<String>,
    max_age: Option<i64>,
    expires: Option<i64>,
    secure: bool,
    http_only: bool,
    same_site: Option<SameSite>,
}

impl Cookie {
    fn new(name: &str, value: &str) -> Cookie {
        Cookie {
            name: name.into(),
            value: value.into(),
            ..Default::default()
        }
    }
    fn serialize(&self) -> String {
        let mut s = format!("{}={}", self.name, self.value);
        if let Some(d) = &self.domain {
            s += &format!("; Domain={d}");
        }
        if let Some(p) = &self.path {
            s += &format!("; Path={p}");
        }
        if let Some(m) = self.max_age {
            s += &format!("; Max-Age={m}");
        }
        if let Some(e) = self.expires {
            s += &format!("; Expires={}", fmt_http_date(e));
        }
        if self.secure {
            s += "; Secure";
        }
        if self.http_only {
            s += "; HttpOnly";
        }
        if let Some(ss) = self.same_site {
            s += &format!("; SameSite={ss:?}");
        }
        s
    }
}

fn parse_set_cookie(s: &str) -> Option<Cookie> {
    let mut parts = s.split(';');
    let (name, value) = parts.next()?.split_once('=')?;
    if name.trim().is_empty() {
        return None;
    }
    let mut c = Cookie::new(name.trim(), value.trim());
    for attr in parts {
        let (k, v) = attr
            .split_once('=')
            .map(|(k, v)| (k.trim(), v.trim()))
            .unwrap_or((attr.trim(), ""));
        match k.to_ascii_lowercase().as_str() {
            "domain" => c.domain = Some(v.trim_start_matches('.').to_ascii_lowercase()),
            "path" => c.path = Some(v.to_string()),
            "max-age" => c.max_age = v.parse().ok(),
            "expires" => c.expires = parse_http_date(v),
            "secure" => c.secure = true,
            "httponly" => c.http_only = true,
            "samesite" => {
                c.same_site = match v.to_ascii_lowercase().as_str() {
                    "strict" => Some(SameSite::Strict),
                    "lax" => Some(SameSite::Lax),
                    "none" => Some(SameSite::None),
                    _ => None,
                }
            }
            _ => {} // unknown attributes are ignored, as RFC 6265 requires
        }
    }
    Some(c)
}

/// Parse a request `Cookie: a=1; b=2` header into pairs.
fn parse_cookie_header(h: &str) -> Vec<(String, String)> {
    h.split(';')
        .filter_map(|p| p.trim().split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn cookies_of(h: &HeaderMap) -> HashMap<String, String> {
    h.get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(parse_cookie_header)
        .collect()
}

// ---------------------------------------------------------------- the jar
/// What kind of request is the browser making? (for SameSite)
#[derive(Clone, Copy, PartialEq, Debug)]
enum Ctx {
    SameSite,
    CrossSiteNavGet,
    CrossSiteNavPost,
    CrossSiteSubrequest,
}

fn samesite_allows(ss: Option<SameSite>, ctx: Ctx) -> bool {
    match (ss.unwrap_or(SameSite::Lax), ctx) {
        (_, Ctx::SameSite) | (SameSite::None, _) | (SameSite::Lax, Ctx::CrossSiteNavGet) => true,
        _ => false,
    }
}

fn domain_match(host: &str, d: &str) -> bool {
    host == d || host.ends_with(&format!(".{d}"))
}

fn path_match(req: &str, cookie: &str) -> bool {
    req == cookie
        || (req.starts_with(cookie)
            && (cookie.ends_with('/') || req[cookie.len()..].starts_with('/')))
}

/// RFC 6265 5.1.4: directory of the request path.
fn default_path(req: &str) -> String {
    match req.rfind('/') {
        Some(0) | None => "/".into(),
        Some(i) => req[..i].into(),
    }
}

struct Stored {
    c: Cookie,
    domain: String,
    host_only: bool,
    path: String,
    expires_at: Option<i64>,
    seq: u64,
}

struct Jar {
    items: Vec<Stored>,
    now: Arc<AtomicU64>,
    seq: u64,
}

impl Jar {
    fn new(now: Arc<AtomicU64>) -> Jar {
        Jar {
            items: vec![],
            now,
            seq: 0,
        }
    }
    fn now(&self) -> i64 {
        self.now.load(Ordering::SeqCst) as i64
    }
    /// Apply a Set-Cookie received from `host` for a request to `req_path`. Returns what happened.
    fn store(
        &mut self,
        host: &str,
        req_path: &str,
        https: bool,
        set_cookie: &str,
    ) -> Result<&'static str, &'static str> {
        let c = parse_set_cookie(set_cookie).ok_or("unparseable")?;
        let (domain, host_only) = match &c.domain {
            Some(d) if !d.contains('.') => {
                return Err("rejected: domain looks like a public suffix");
            }
            Some(d) if !domain_match(host, d) => {
                return Err("rejected: Domain does not match the request host");
            }
            Some(d) => (d.clone(), false),
            None => (host.to_string(), true),
        };
        if c.secure && !https {
            return Err("rejected: Secure cookie from an insecure origin");
        }
        if c.same_site == Some(SameSite::None) && !c.secure {
            return Err("rejected: SameSite=None requires Secure");
        }
        let path = c
            .path
            .clone()
            .filter(|p| p.starts_with('/'))
            .unwrap_or_else(|| default_path(req_path));
        let now = self.now();
        // Max-Age beats Expires (RFC 6265 5.3 step 3)
        let expires_at = match (c.max_age, c.expires) {
            (Some(m), _) => Some(now + m.max(-1)),
            (None, Some(e)) => Some(e),
            _ => None,
        };
        self.items
            .retain(|s| !(s.c.name == c.name && s.domain == domain && s.path == path));
        if expires_at.is_some_and(|e| e <= now) {
            return Ok("deleted (already expired)");
        }
        self.seq += 1;
        self.items.push(Stored {
            c,
            domain,
            host_only,
            path,
            expires_at,
            seq: self.seq,
        });
        Ok("stored")
    }
    /// The `Cookie` header value to send, if any.
    fn header(&self, host: &str, path: &str, https: bool, ctx: Ctx) -> Option<String> {
        let now = self.now();
        let mut hits: Vec<&Stored> = self
            .items
            .iter()
            .filter(|s| s.expires_at.is_none_or(|e| e > now))
            .filter(|s| {
                if s.host_only {
                    host == s.domain
                } else {
                    domain_match(host, &s.domain)
                }
            })
            .filter(|s| path_match(path, &s.path))
            .filter(|s| !s.c.secure || https)
            .filter(|s| samesite_allows(s.c.same_site, ctx))
            .collect();
        hits.sort_by(|a, b| b.path.len().cmp(&a.path.len()).then(a.seq.cmp(&b.seq))); // longer paths first
        (!hits.is_empty()).then(|| {
            hits.iter()
                .map(|s| format!("{}={}", s.c.name, s.c.value))
                .collect::<Vec<_>>()
                .join("; ")
        })
    }
    fn get(&self, name: &str) -> Option<String> {
        self.items
            .iter()
            .find(|s| s.c.name == name)
            .map(|s| s.c.value.clone())
    }
}

// ---------------------------------------------------------------- signed cookies
fn sign_value(secret: &[u8], v: &str) -> String {
    format!("{v}.{}", b64u_encode(&hmac_sha256(secret, v.as_bytes())))
}

fn verify_value<'a>(secret: &[u8], signed: &'a str) -> Option<&'a str> {
    let (v, sig) = signed.rsplit_once('.')?;
    let got = b64u_decode(sig)?;
    ct_eq(&got, &hmac_sha256(secret, v.as_bytes())).then_some(v)
}

fn csrf_token(secret: &[u8], sid: &str) -> String {
    b64u_encode(&hmac_sha256(secret, format!("csrf:{sid}").as_bytes()))
}

// ---------------------------------------------------------------- sessions
struct Session {
    user: Option<String>,
    created: u64,
    last_seen: u64,
}

#[derive(Debug, PartialEq)]
enum Why {
    Missing,
    IdleExpired,
    AbsoluteExpired,
}

struct Sessions {
    map: HashMap<String, Session>,
    idle: u64,
    absolute: u64,
}

impl Sessions {
    fn create(&mut self, user: Option<String>, now: u64) -> String {
        let id = hex(&random_bytes(32));
        self.map.insert(
            id.clone(),
            Session {
                user,
                created: now,
                last_seen: now,
            },
        );
        id
    }
    /// Look up and touch a session, enforcing both timeouts.
    fn get(&mut self, id: &str, now: u64) -> Result<&mut Session, Why> {
        let s = self.map.get(id).ok_or(Why::Missing)?;
        let why = if now - s.last_seen > self.idle {
            Some(Why::IdleExpired)
        } else if now - s.created > self.absolute {
            Some(Why::AbsoluteExpired)
        } else {
            None
        };
        if let Some(w) = why {
            self.map.remove(id);
            return Err(w);
        }
        let s = self.map.get_mut(id).unwrap();
        s.last_seen = now;
        Ok(s)
    }
    /// Privilege change (login): issue a NEW id and destroy the old one, so a planted id is useless.
    fn rotate(&mut self, old: Option<&str>, user: &str, now: u64) -> String {
        if let Some(o) = old {
            self.map.remove(o);
        }
        self.create(Some(user.to_string()), now)
    }
}

// ---------------------------------------------------------------- the web app
struct App {
    secret: Vec<u8>,
    sessions: Mutex<Sessions>,
    now: Arc<AtomicU64>,
}

type St = State<Arc<App>>;

fn set_cookie_hdr(c: &Cookie) -> (header::HeaderName, String) {
    (header::SET_COOKIE, c.serialize())
}

fn session_cookie(id: &str) -> Cookie {
    Cookie {
        path: Some("/".into()),
        http_only: true,
        same_site: Some(SameSite::Lax),
        ..Cookie::new("sid", id)
    }
}

fn csrf_cookie(app: &App, sid: &str) -> Cookie {
    // Readable by page scripts on purpose (no HttpOnly): JS copies it into the X-CSRF-Token header.
    Cookie {
        path: Some("/".into()),
        same_site: Some(SameSite::Lax),
        ..Cookie::new("csrf", &csrf_token(&app.secret, sid))
    }
}

fn deny(code: StatusCode, state: &'static str, msg: &'static str) -> Response {
    (code, [("x-session-state", state)], msg).into_response()
}

/// Resolve the logged-in (sid, user) or produce the 401 response.
fn authenticate(app: &App, h: &HeaderMap) -> Result<(String, String), Response> {
    let jar = cookies_of(h);
    let sid = jar
        .get("sid")
        .ok_or_else(|| deny(StatusCode::UNAUTHORIZED, "no-cookie", "login required"))?;
    let now = app.now.load(Ordering::SeqCst);
    let mut ss = app.sessions.lock().unwrap();
    match ss.get(sid, now) {
        Ok(Session { user: Some(u), .. }) => Ok((sid.clone(), u.clone())),
        Ok(_) => Err(deny(
            StatusCode::UNAUTHORIZED,
            "anonymous",
            "login required",
        )),
        Err(Why::Missing) => Err(deny(
            StatusCode::UNAUTHORIZED,
            "unknown-session",
            "login required",
        )),
        Err(Why::IdleExpired) => Err(deny(
            StatusCode::UNAUTHORIZED,
            "idle-expired",
            "session expired",
        )),
        Err(Why::AbsoluteExpired) => Err(deny(
            StatusCode::UNAUTHORIZED,
            "absolute-expired",
            "session expired",
        )),
    }
}

/// Double-submit: header token == cookie token AND the token is the HMAC bound to this session.
fn csrf_ok(app: &App, h: &HeaderMap, sid: &str) -> bool {
    let (Some(hdr), Some(cookie)) = (
        h.get("x-csrf-token").and_then(|v| v.to_str().ok()),
        cookies_of(h).get("csrf").cloned(),
    ) else {
        return false;
    };
    ct_eq(hdr.as_bytes(), cookie.as_bytes())
        && ct_eq(hdr.as_bytes(), csrf_token(&app.secret, sid).as_bytes())
}

async fn index(State(app): St, h: HeaderMap) -> Response {
    let now = app.now.load(Ordering::SeqCst);
    let mut ss = app.sessions.lock().unwrap();
    let have = cookies_of(&h)
        .get("sid")
        .is_some_and(|s| ss.get(s, now).is_ok());
    let prefs = Cookie {
        path: Some("/".into()),
        max_age: Some(86_400 * 365),
        ..Cookie::new("prefs", &sign_value(&app.secret, "theme:dark"))
    };
    if have {
        return ([set_cookie_hdr(&prefs)], "welcome back").into_response();
    }
    let sid = ss.create(None, now); // anonymous session (e.g. a shopping cart)
    (
        AppendHeaders([
            set_cookie_hdr(&session_cookie(&sid)),
            set_cookie_hdr(&prefs),
        ]),
        "welcome, anonymous",
    )
        .into_response()
}

#[derive(Deserialize)]
struct Login {
    user: String,
    pw: String,
}

async fn login(State(app): St, h: HeaderMap, Form(f): Form<Login>) -> Response {
    if !(f.user == "ann" && f.pw == "s3cret") {
        return deny(
            StatusCode::UNAUTHORIZED,
            "bad-credentials",
            "invalid credentials",
        ); // no rotation, no new cookie
    }
    let now = app.now.load(Ordering::SeqCst);
    let old = cookies_of(&h).get("sid").cloned();
    let sid = app
        .sessions
        .lock()
        .unwrap()
        .rotate(old.as_deref(), &f.user, now);
    (
        AppendHeaders([
            set_cookie_hdr(&session_cookie(&sid)),
            set_cookie_hdr(&csrf_cookie(&app, &sid)),
        ]),
        "logged in",
    )
        .into_response()
}

async fn me(State(app): St, h: HeaderMap) -> Response {
    match authenticate(&app, &h) {
        Ok((_, user)) => format!("hello {user}").into_response(),
        Err(r) => r,
    }
}

async fn transfer(State(app): St, h: HeaderMap) -> Response {
    let (sid, user) = match authenticate(&app, &h) {
        Ok(x) => x,
        Err(r) => return r,
    };
    if !csrf_ok(&app, &h, &sid) {
        return (StatusCode::FORBIDDEN, "CSRF token missing or invalid").into_response();
    }
    format!("{user} transferred 100").into_response()
}

async fn logout(State(app): St, h: HeaderMap) -> Response {
    let (sid, _) = match authenticate(&app, &h) {
        Ok(x) => x,
        Err(r) => return r,
    };
    if !csrf_ok(&app, &h, &sid) {
        return (StatusCode::FORBIDDEN, "CSRF token missing or invalid").into_response();
    }
    app.sessions.lock().unwrap().map.remove(&sid);
    let gone = |n: &str| Cookie {
        path: Some("/".into()),
        max_age: Some(0),
        expires: Some(0),
        ..Cookie::new(n, "")
    };
    (
        AppendHeaders([set_cookie_hdr(&gone("sid")), set_cookie_hdr(&gone("csrf"))]),
        "logged out",
    )
        .into_response()
}

async fn prefs(State(app): St, h: HeaderMap) -> Response {
    match cookies_of(&h)
        .get("prefs")
        .and_then(|p| verify_value(&app.secret, p).map(String::from))
    {
        Some(v) => format!("prefs ok: {v}").into_response(),
        None => (StatusCode::BAD_REQUEST, "prefs cookie missing or tampered").into_response(),
    }
}

fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/login", post(login))
        .route("/me", get(me))
        .route("/transfer", post(transfer))
        .route("/logout", post(logout))
        .route("/prefs", get(prefs))
        .with_state(app)
}

// ---------------------------------------------------------------- client flow with a manual jar
struct Browser {
    c: Client,
    jar: Jar,
    srv_url: String,
}

impl Browser {
    /// Attach cookies from the jar, send, then feed every Set-Cookie back into the jar.
    async fn call(&mut self, rb: RequestBuilder, ctx: Ctx) -> Exch {
        let mut req = rb.build().unwrap();
        let path = req.url().path().to_string();
        if let Some(h) = self.jar.header("127.0.0.1", &path, false, ctx) {
            req.headers_mut().insert(header::COOKIE, h.parse().unwrap());
        }
        let ex = try_send_req(&self.c, req).await.unwrap();
        for sc in ex.headers.get_all(header::SET_COOKIE) {
            let r = self
                .jar
                .store("127.0.0.1", &path, false, sc.to_str().unwrap());
            println!("  jar: {}", r.unwrap_or_else(|e| e));
        }
        ex
    }
    fn url(&self, p: &str) -> String {
        format!("{}{p}", self.srv_url)
    }
}

fn jar_rules_demo() {
    title("cookie jar rules (pure, simulated hosts)");
    let now = Arc::new(AtomicU64::new(1_700_000_000));
    let mut jar = Jar::new(now.clone());
    let sets = [
        ("api.example.com", "/v1/items", "a=1"),
        (
            "api.example.com",
            "/v1/items",
            "b=2; Domain=example.com; Path=/",
        ),
        ("api.example.com", "/v1/items", "c=3; Domain=other.com"),
        ("api.example.com", "/", "d=4; Path=/admin; HttpOnly"),
        ("api.example.com", "/", "e=5; Secure"),
        ("api.example.com", "/", "f=6; Max-Age=60; Path=/"),
        (
            "api.example.com",
            "/",
            "g=7; Expires=Sun, 06 Nov 1994 08:49:37 GMT",
        ),
        ("api.example.com", "/", "h=8; Domain=com"),
        ("api.example.com", "/", "i=9; SameSite=None"),
    ];
    for (host, path, sc) in sets {
        println!(
            "{host}{path:<10} Set-Cookie: {sc:<45} => {}",
            jar.store(host, path, false, sc).unwrap_or_else(|e| e)
        );
    }
    let secure = jar.store(
        "api.example.com",
        "/",
        true,
        "s=10; Secure; Path=/; SameSite=None",
    );
    println!("(https) Set-Cookie: s=10; Secure; SameSite=None => {secure:?}");
    let at = |host, path, https| jar.header(host, path, https, Ctx::SameSite);
    let rows = [
        ("api.example.com", "/v1/items", false, "a=1; b=2; f=6"),
        ("api.example.com", "/v1/items/9", false, "a=1; b=2; f=6"), // a: default-path /v1 matches
        ("api.example.com", "/v2", false, "b=2; f=6"),
        ("api.example.com", "/admin/users", false, "d=4; b=2; f=6"), // longer path first
        ("www.example.com", "/", false, "b=2"), // only the Domain cookie crosses hosts
        ("example.com", "/", false, "b=2"),
        ("evil.com", "/", false, ""),
        ("badexample.com", "/", false, ""), // suffix match needs a dot boundary
        ("api.example.com", "/administrator", false, "b=2; f=6"), // /admin must not match /administrator
        ("api.example.com", "/", true, "b=2; f=6; s=10"),
    ];
    for (host, path, https, want) in rows {
        let got = at(host, path, https).unwrap_or_default();
        println!(
            "  GET {:<34} Cookie: {got}",
            format!("{}://{host}{path}", if https { "https" } else { "http" })
        );
        assert_eq!(got, want, "{host}{path}");
    }
    now.fetch_add(61, Ordering::SeqCst);
    assert_eq!(
        jar.header("api.example.com", "/v2", false, Ctx::SameSite)
            .unwrap(),
        "b=2",
        "Max-Age=60 cookie expired after 61s"
    );
    jar.store(
        "api.example.com",
        "/",
        false,
        "b=; Domain=example.com; Path=/; Max-Age=0",
    )
    .unwrap();
    assert_eq!(
        jar.header("api.example.com", "/v2", false, Ctx::SameSite),
        None,
        "Max-Age=0 deletes"
    );

    // round trip
    let c = Cookie {
        domain: Some("example.com".into()),
        path: Some("/".into()),
        max_age: Some(3600),
        expires: Some(784111777),
        secure: true,
        http_only: true,
        same_site: Some(SameSite::Strict),
        ..Cookie::new("k", "v")
    };
    let s = c.serialize();
    println!("\nserialized: {s}");
    assert_eq!(
        s,
        "k=v; Domain=example.com; Path=/; Max-Age=3600; Expires=Sun, 06 Nov 1994 08:49:37 GMT; Secure; HttpOnly; SameSite=Strict"
    );
    assert_eq!(parse_set_cookie(&s).unwrap(), c);
    assert_eq!(
        fmt_http_date(parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT").unwrap()),
        "Sun, 06 Nov 1994 08:49:37 GMT"
    );
    println!();

    title("SameSite: which cookies does the browser attach? (y = sent)");
    let cols = [
        Ctx::SameSite,
        Ctx::CrossSiteNavGet,
        Ctx::CrossSiteNavPost,
        Ctx::CrossSiteSubrequest,
    ];
    println!(
        "{:<14}{:>12}{:>12}{:>12}{:>12}",
        "cookie", "same-site", "xsite-GET", "xsite-POST", "xsite-fetch"
    );
    for (label, ss, expect) in [
        (
            "Strict",
            Some(SameSite::Strict),
            [true, false, false, false],
        ),
        ("Lax", Some(SameSite::Lax), [true, true, false, false]),
        ("(unset=Lax)", None, [true, true, false, false]),
        (
            "None;Secure",
            Some(SameSite::None),
            [true, true, true, true],
        ),
    ] {
        let cells: Vec<bool> = cols.iter().map(|c| samesite_allows(ss, *c)).collect();
        println!(
            "{label:<14}{:>12}{:>12}{:>12}{:>12}",
            yn(cells[0]),
            yn(cells[1]),
            yn(cells[2]),
            yn(cells[3])
        );
        assert_eq!(cells, expect);
    }
    println!(
        "=> Lax blocks the classic cross-site auto-submitted POST form; it does NOT stop a same-site attacker (XSS, sibling subdomain), so keep the token.\n"
    );
}

fn yn(b: bool) -> &'static str {
    if b { "y" } else { "-" }
}

fn session_and_signing_demo() {
    title("sessions (injectable clock) and signed cookies (pure)");
    let mut ss = Sessions {
        map: HashMap::new(),
        idle: 1800,
        absolute: 3600,
    };
    let a = ss.create(None, 0);
    assert_eq!(a.len(), 64, "256 random bits as hex");
    assert_ne!(a, ss.create(None, 0), "ids never repeat");
    let b = ss.rotate(Some(&a), "ann", 10);
    assert_ne!(a, b);
    assert_eq!(
        ss.get(&a, 10).err(),
        Some(Why::Missing),
        "fixation: the pre-login id is dead after rotation"
    );
    assert!(ss.get(&b, 1000).is_ok());
    assert!(
        ss.get(&b, 2800).is_ok(),
        "activity at 1000 resets the idle timer"
    );
    assert_eq!(ss.get(&b, 4700).err(), Some(Why::IdleExpired));
    let c = ss.rotate(None, "ann", 5000);
    for t in [5000 + 1500, 5000 + 3000] {
        assert!(ss.get(&c, t).is_ok());
    }
    assert_eq!(
        ss.get(&c, 5000 + 3700).err(),
        Some(Why::AbsoluteExpired),
        "kept alive by activity, still dies at the absolute limit"
    );
    println!(
        "idle (30 min) and absolute (60 min) timeouts enforced; rotation kills the pre-login id"
    );

    let key = b"demo-signing-key";
    let signed = sign_value(key, "theme:dark");
    println!("signed cookie value: {signed}");
    assert_eq!(verify_value(key, &signed), Some("theme:dark"));
    assert_eq!(
        verify_value(key, &signed.replace("dark", "lite")),
        None,
        "tampered value"
    );
    let mut flipped = signed.clone().into_bytes();
    let last = flipped.len() - 1;
    flipped[last] = if flipped[last] == b'A' { b'B' } else { b'A' };
    assert_eq!(
        verify_value(key, std::str::from_utf8(&flipped).unwrap()),
        None,
        "tampered signature"
    );
    assert_eq!(verify_value(b"other-key", &signed), None, "wrong key");
    assert_eq!(verify_value(key, "no-signature-at-all"), None);
    assert!(ct_eq(b"abc", b"abc") && !ct_eq(b"abc", b"abd") && !ct_eq(b"abc", b"ab"));
    println!();
}

#[tokio::main]
async fn main() {
    jar_rules_demo();
    session_and_signing_demo();

    let clock = Arc::new(AtomicU64::new(1_000_000));
    let app = Arc::new(App {
        secret: b"cookbook-demo-secret-key-32bytes!".to_vec(),
        sessions: Mutex::new(Sessions {
            map: HashMap::new(),
            idle: 1800,
            absolute: 3600,
        }),
        now: clock.clone(),
    });
    let srv = Server::start(router(app.clone())).await;
    let mut b = Browser {
        c: client(),
        jar: Jar::new(clock.clone()),
        srv_url: srv.url(""),
    };
    let login_form = |b: &Browser, pw: &str| {
        b.c.post(b.url("/login"))
            .form(&[("user", "ann"), ("pw", pw)])
    };

    title("flow 1: first visit sets an anonymous session + signed prefs cookie");
    let r = b.call(b.c.get(b.url("/")), Ctx::SameSite).await;
    assert_eq!(r.headers.get_all("set-cookie").iter().count(), 2);
    assert!(r.h("set-cookie").contains("HttpOnly") && r.h("set-cookie").contains("SameSite=Lax"));
    let anon = b.jar.get("sid").unwrap();
    let r = b.call(b.c.get(b.url("/prefs")), Ctx::SameSite).await;
    assert_eq!(r.text(), "prefs ok: theme:dark");
    let r =
        b.c.get(b.url("/prefs"))
            .header("cookie", "prefs=theme:lite.AAAA")
            .send()
            .await
            .unwrap();
    assert_eq!(r.status(), 400, "forged prefs cookie is rejected");
    let r = b.call(b.c.get(b.url("/me")), Ctx::SameSite).await;
    assert_eq!(
        (r.status.as_u16(), r.h("x-session-state")),
        (401, "anonymous".to_string())
    );

    title("flow 2: login rotates the session id (fixation defence)");
    let r = b.call(login_form(&b, "wrong"), Ctx::SameSite).await;
    assert_eq!(r.status.as_u16(), 401);
    assert_eq!(
        b.jar.get("sid").unwrap(),
        anon,
        "failed login must not change the session"
    );
    let r = b.call(login_form(&b, "s3cret"), Ctx::SameSite).await;
    assert_eq!(r.status.as_u16(), 200);
    let authed = b.jar.get("sid").unwrap();
    assert_ne!(authed, anon, "id rotated on privilege change");
    let attacker =
        b.c.get(b.url("/me"))
            .header("cookie", format!("sid={anon}"))
            .send()
            .await
            .unwrap();
    assert_eq!(
        attacker.status(),
        401,
        "an attacker who planted the old id gains nothing"
    );

    title("flow 3: authenticated request, CSRF-protected request");
    let r = b.call(b.c.get(b.url("/me")), Ctx::SameSite).await;
    assert_eq!(r.text(), "hello ann");
    let token = b.jar.get("csrf").unwrap();
    let r = b.call(b.c.post(b.url("/transfer")), Ctx::SameSite).await;
    assert_eq!(
        r.status.as_u16(),
        403,
        "cookie alone is not enough: no X-CSRF-Token header"
    );
    let r = b
        .call(
            b.c.post(b.url("/transfer"))
                .header("x-csrf-token", "forged-token-value"),
            Ctx::SameSite,
        )
        .await;
    assert_eq!(
        r.status.as_u16(),
        403,
        "attacker cannot read the token, so cannot echo it"
    );
    let r = b
        .call(
            b.c.post(b.url("/transfer")).header("x-csrf-token", &token),
            Ctx::SameSite,
        )
        .await;
    assert_eq!(r.text(), "ann transferred 100");
    // forged cross-site POST: browser applies SameSite=Lax and withholds the cookies entirely
    assert_eq!(
        b.jar
            .header("127.0.0.1", "/transfer", false, Ctx::CrossSiteNavPost),
        None
    );
    let r = b
        .call(b.c.post(b.url("/transfer")), Ctx::CrossSiteNavPost)
        .await;
    assert_eq!(
        r.status.as_u16(),
        401,
        "no cookie sent cross-site: unauthenticated"
    );

    title("flow 4: idle and absolute timeouts (server-side, injectable clock)");
    clock.fetch_add(1801, Ordering::SeqCst);
    let r = b.call(b.c.get(b.url("/me")), Ctx::SameSite).await;
    assert_eq!(
        (r.status.as_u16(), r.h("x-session-state")),
        (401, "idle-expired".to_string())
    );
    b.call(login_form(&b, "s3cret"), Ctx::SameSite).await;
    for _ in 0..2 {
        clock.fetch_add(1500, Ordering::SeqCst);
        assert_eq!(
            b.call(b.c.get(b.url("/me")), Ctx::SameSite)
                .await
                .status
                .as_u16(),
            200
        );
    }
    clock.fetch_add(700, Ordering::SeqCst);
    let r = b.call(b.c.get(b.url("/me")), Ctx::SameSite).await;
    assert_eq!(
        (r.status.as_u16(), r.h("x-session-state")),
        (401, "absolute-expired".to_string())
    );

    title("flow 5: logout destroys the session and clears the cookies");
    b.call(login_form(&b, "s3cret"), Ctx::SameSite).await;
    let live = b.jar.get("sid").unwrap();
    let token = b.jar.get("csrf").unwrap();
    let r = b
        .call(
            b.c.post(b.url("/logout")).header("x-csrf-token", &token),
            Ctx::SameSite,
        )
        .await;
    assert!(r.h("set-cookie").contains("Max-Age=0"));
    assert!(
        b.jar.get("sid").is_none() && b.jar.get("csrf").is_none(),
        "jar dropped both cookies"
    );
    assert_eq!(
        b.call(b.c.get(b.url("/me")), Ctx::SameSite)
            .await
            .status
            .as_u16(),
        401
    );
    let replay =
        b.c.get(b.url("/me"))
            .header("cookie", format!("sid={live}"))
            .send()
            .await
            .unwrap();
    assert_eq!(
        replay.status(),
        401,
        "a stolen copy of the old cookie is dead server-side too"
    );

    drop(b);
    srv.shutdown().await;
    println!("all cookie / session / CSRF assertions passed");
}
