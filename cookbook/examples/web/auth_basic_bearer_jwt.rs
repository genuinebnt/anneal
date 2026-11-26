//! RECIPE 303 (web): authentication on the wire: Basic, API keys, Bearer tokens and a from-scratch HS256 JWT
//! LEVEL: advanced
//! Use in: API security, auth middleware.   Run: cargo run --release --example auth_basic_bearer_jwt
//!
//! WHAT: four schemes behind one axum server: HTTP Basic (parse `Authorization: Basic`, 401 +
//! `WWW-Authenticate` challenge), API keys (`X-API-Key`, stored hashed), opaque Bearer tokens (RFC 6750) and a
//! JWT (RFC 7519, HS256) implemented from scratch: base64url without padding, `header.payload.signature`,
//! HMAC-SHA256, `exp/nbf/iat/iss/aud/sub` validation with an injectable clock and leeway, `kid` key rotation,
//! all behind an axum `FromRequestParts` extractor. The classic attacks are mounted and REJECTED: `alg:none`,
//! algorithm confusion, tampered payload, expired / not-yet-valid tokens, wrong audience/issuer, unknown `kid`,
//! plus the timing leak of a naive signature compare versus a constant-time one.
//! WHY: authentication bugs are catastrophic and almost always come from trusting the token to describe how it
//! should be verified. Rules used here: the VERIFIER fixes the algorithm and the key set; the signature is
//! checked BEFORE any claim is believed; claims are checked against the server's expectations.
//! HOW: 401 = "I do not know who you are" (always with `WWW-Authenticate`); 403 = "I know you, you may not".
//! The access log records `auth=present|absent`, never the header value (asserted). Refresh flow: short-lived
//! access JWT + single-use rotating refresh token stored hashed; reusing a consumed refresh token signals theft
//! and revokes the whole token family. RS256 (asymmetric) needs big-integer RSA and is not implemented: with it,
//! pin the allowed `alg` per key so a public key can never be reused as an HMAC secret (shown in the confusion
//! demo with a deliberately vulnerable verifier).
//! COMMON USES: protecting REST APIs and microservices (service-to-service JWTs), mobile/SPA access+refresh
//! tokens, webhook/API-key partners, simple internal tools behind Basic over TLS, key rotation without downtime.
//! Read: https://datatracker.ietf.org/doc/html/rfc7519 , https://datatracker.ietf.org/doc/html/rfc6750 ,
//! https://datatracker.ietf.org/doc/html/rfc9110 , https://developer.mozilla.org/en-US/docs/Web/HTTP/Authentication
#![allow(dead_code)]
use axum::{
    Json, Router,
    extract::{FromRequestParts, Request, State},
    http::{header, request::Parts},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
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

fn b64_std_encode(d: &[u8]) -> String {
    let mut s: String = b64u_encode(d)
        .chars()
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            c => c,
        })
        .collect();
    while s.len() % 4 != 0 {
        s.push('=');
    }
    s
}

fn b64_std_decode(s: &str) -> Option<Vec<u8>> {
    b64u_decode(
        &s.trim_end_matches('=')
            .chars()
            .map(|c| match c {
                '+' => '-',
                '/' => '_',
                c => c,
            })
            .collect::<String>(),
    )
}

// ---------------------------------------------------------------- JWT (HS256)
#[derive(Debug, PartialEq, Clone)]
enum JwtError {
    Malformed,
    UnsupportedAlg(String),
    UnknownKid,
    BadSignature,
    Expired,
    NotYetValid,
    WrongIssuer,
    WrongAudience,
    MissingClaim(&'static str),
}

struct Keyring {
    keys: HashMap<String, Vec<u8>>,
    current: String,
}

struct Validation {
    iss: String,
    aud: String,
    leeway: u64,
}

fn jwt_encode(kid: &str, key: &[u8], claims: &Value) -> String {
    let header = json!({ "alg": "HS256", "typ": "JWT", "kid": kid });
    let signing_input = format!(
        "{}.{}",
        b64u_encode(header.to_string().as_bytes()),
        b64u_encode(claims.to_string().as_bytes())
    );
    format!(
        "{signing_input}.{}",
        b64u_encode(&hmac_sha256(key, signing_input.as_bytes()))
    )
}

/// Decode without verifying (debugging ONLY: never make decisions from this).
fn jwt_peek(token: &str) -> Option<(Value, Value)> {
    let mut it = token.split('.');
    let (h, p) = (it.next()?, it.next()?);
    Some((
        serde_json::from_slice(&b64u_decode(h)?).ok()?,
        serde_json::from_slice(&b64u_decode(p)?).ok()?,
    ))
}

/// Verify in the safe order: shape -> algorithm allow-list -> key by kid -> signature -> claims.
fn jwt_decode(token: &str, ring: &Keyring, v: &Validation, now: u64) -> Result<Value, JwtError> {
    if token.len() > 4096 {
        return Err(JwtError::Malformed);
    }
    let parts: Vec<&str> = token.split('.').collect();
    let [h, p, sig] = parts[..] else {
        return Err(JwtError::Malformed);
    };
    let header: Value = b64u_decode(h)
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or(JwtError::Malformed)?;
    // The verifier decides the algorithm. `none`, `RS256`, `hs256` (case games) ... all rejected here.
    match header["alg"].as_str() {
        Some("HS256") => {}
        other => {
            return Err(JwtError::UnsupportedAlg(
                other.unwrap_or("<missing>").to_string(),
            ));
        }
    }
    // `kid` is attacker-controlled input: only ever a lookup key into our own map, never a path or query.
    let key = header["kid"]
        .as_str()
        .and_then(|k| ring.keys.get(k))
        .ok_or(JwtError::UnknownKid)?;
    let given = b64u_decode(sig).ok_or(JwtError::Malformed)?;
    if !ct_eq(&given, &hmac_sha256(key, format!("{h}.{p}").as_bytes())) {
        return Err(JwtError::BadSignature);
    }
    // Only now is the payload trustworthy as "issued by us"; still validate it.
    let c: Value = b64u_decode(p)
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or(JwtError::Malformed)?;
    let num = |n: &'static str| c[n].as_u64().ok_or(JwtError::MissingClaim(n));
    let (exp, nbf, iat) = (num("exp")?, num("nbf")?, num("iat")?);
    if now > exp + v.leeway {
        return Err(JwtError::Expired);
    }
    if now + v.leeway < nbf || now + v.leeway < iat {
        return Err(JwtError::NotYetValid);
    }
    if c["iss"].as_str() != Some(v.iss.as_str()) {
        return Err(JwtError::WrongIssuer);
    }
    let aud_ok = match &c["aud"] {
        Value::String(a) => *a == v.aud,
        Value::Array(a) => a.iter().any(|x| x.as_str() == Some(v.aud.as_str())),
        _ => false,
    };
    if !aud_ok {
        return Err(JwtError::WrongAudience);
    }
    c["sub"].as_str().ok_or(JwtError::MissingClaim("sub"))?;
    Ok(c)
}

/// A deliberately BROKEN verifier: trusts the token's own `alg` and reuses "the key" for any algorithm.
fn vulnerable_decode(token: &str, public_key: &[u8]) -> Option<Value> {
    let parts: Vec<&str> = token.split('.').collect();
    let header: Value = serde_json::from_slice(&b64u_decode(parts.first()?)?).ok()?;
    let ok = match header["alg"].as_str()? {
        "none" => true,
        "HS256" => ct_eq(
            &b64u_decode(parts.get(2)?)?,
            &hmac_sha256(public_key, format!("{}.{}", parts[0], parts[1]).as_bytes()),
        ),
        _ => false,
    };
    if ok {
        serde_json::from_slice(&b64u_decode(parts.get(1)?)?).ok()
    } else {
        None
    }
}

/// What a naive `==` leaks: how many leading bytes matched before the early exit.
fn naive_compare_steps(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count() + 1
}

// ---------------------------------------------------------------- the service
struct App {
    now: Arc<AtomicU64>,
    ring: Mutex<Keyring>,
    val: Validation,
    users: HashMap<&'static str, (&'static str, String)>, // user -> (salt, sha256(salt:pw) hex)
    api_keys: HashMap<String, (&'static str, Vec<&'static str>)>, // sha256(key) -> (owner, scopes)
    opaque: HashMap<String, &'static str>,                // sha256(token) -> user
    refresh: Mutex<HashMap<String, (String, u32, bool)>>, // sha256(rt) -> (user, family, consumed)
    revoked_families: Mutex<Vec<u32>>,
    log: Mutex<Vec<String>>,
}

const T0: u64 = 1_700_000_000;
const API_KEY: &str = "ck_test_0123456789abcdef0123456789abcdef";
const OPAQUE: &str = "pat_9f8e7d6c5b4a39281706f5e4d3c2b1a0";

fn pw_hash(salt: &str, pw: &str) -> String {
    hex(&sha256(format!("{salt}:{pw}").as_bytes())) // real systems: argon2id/scrypt, not one SHA-256
}

impl App {
    fn now(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }
    fn issue(&self, user: &str, scope: &str, aud: &str, ttl: u64) -> String {
        let r = self.ring.lock().unwrap();
        let claims = json!({ "iss": self.val.iss, "sub": user, "aud": aud, "iat": self.now(), "nbf": self.now(), "exp": self.now() + ttl, "scope": scope });
        jwt_encode(&r.current, &r.keys[&r.current], &claims)
    }
}

type St = State<Arc<App>>;

fn challenge(scheme: &str, extra: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, format!("{scheme} {extra}"))],
        "unauthorized",
    )
        .into_response()
}

fn auth_header(parts_headers: &axum::http::HeaderMap) -> Option<(String, String)> {
    let v = parts_headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, cred) = v.split_once(' ')?;
    Some((scheme.to_ascii_lowercase(), cred.trim().to_string())) // scheme names are case-insensitive
}

/// Validated JWT claims, as an extractor: handlers that take `Claims` cannot run unauthenticated.
struct Claims(Value);

impl FromRequestParts<Arc<App>> for Claims {
    type Rejection = Response;
    async fn from_request_parts(parts: &mut Parts, app: &Arc<App>) -> Result<Self, Response> {
        let Some((_, token)) = auth_header(&parts.headers).filter(|(s, _)| s == "bearer") else {
            return Err(challenge("Bearer", "realm=\"api\"")); // no credentials: bare challenge, no error code
        };
        let res = jwt_decode(&token, &app.ring.lock().unwrap(), &app.val, app.now());
        match res {
            Ok(c) => Ok(Claims(c)),
            Err(e) => {
                app.log.lock().unwrap().push(format!("jwt rejected: {e:?}")); // precise reason: server side only
                let desc = if e == JwtError::Expired {
                    "token expired"
                } else {
                    "invalid token"
                };
                Err(challenge(
                    "Bearer",
                    &format!(
                        "realm=\"api\", error=\"invalid_token\", error_description=\"{desc}\""
                    ),
                ))
            }
        }
    }
}

async fn basic(State(app): St, h: axum::http::HeaderMap) -> Response {
    let creds = auth_header(&h)
        .filter(|(s, _)| s == "basic")
        .and_then(|(_, c)| b64_std_decode(&c))
        .and_then(|b| String::from_utf8(b).ok())
        .and_then(|s| {
            s.split_once(':')
                .map(|(u, p)| (u.to_string(), p.to_string()))
        }); // first colon splits; password may contain ':'
    let Some((user, pw)) = creds else {
        return challenge("Basic", "realm=\"cookbook\", charset=\"UTF-8\"");
    };
    // Always hash, even for unknown users, so response time does not reveal which usernames exist.
    let (salt, want) = app
        .users
        .get(user.as_str())
        .map(|(s, w)| (*s, w.as_str()))
        .unwrap_or(("x", "00"));
    if ct_eq(pw_hash(salt, &pw).as_bytes(), want.as_bytes()) {
        format!("hello {user}").into_response()
    } else {
        challenge("Basic", "realm=\"cookbook\", charset=\"UTF-8\"")
    }
}

async fn api_key(State(app): St, h: axum::http::HeaderMap, method: axum::http::Method) -> Response {
    let Some(k) = h.get("x-api-key").and_then(|v| v.to_str().ok()) else {
        return challenge("ApiKey", "header=\"X-API-Key\"");
    };
    let Some((owner, scopes)) = app.api_keys.get(&hex(&sha256(k.as_bytes()))) else {
        return challenge("ApiKey", "header=\"X-API-Key\"");
    };
    let need = if method == axum::http::Method::GET {
        "read"
    } else {
        "write"
    };
    if !scopes.contains(&need) {
        return (
            StatusCode::FORBIDDEN,
            format!("key of {owner} lacks scope {need}"),
        )
            .into_response(); // known caller, not allowed
    }
    format!("{owner} may {need}").into_response()
}

async fn opaque(State(app): St, h: axum::http::HeaderMap) -> Response {
    match auth_header(&h).filter(|(s, _)| s == "bearer") {
        None => challenge("Bearer", "realm=\"pat\""),
        Some((_, t)) => match app.opaque.get(&hex(&sha256(t.as_bytes()))) {
            Some(u) => format!("token belongs to {u}").into_response(),
            None => challenge("Bearer", "realm=\"pat\", error=\"invalid_token\""),
        },
    }
}

async fn jwt_me(Claims(c): Claims) -> Json<Value> {
    Json(json!({ "sub": c["sub"], "scope": c["scope"] }))
}

async fn jwt_admin(Claims(c): Claims) -> Response {
    if c["scope"]
        .as_str()
        .is_some_and(|s| s.split(' ').any(|x| x == "admin"))
    {
        "welcome, admin".into_response()
    } else {
        (
            StatusCode::FORBIDDEN,
            [(
                header::WWW_AUTHENTICATE,
                "Bearer error=\"insufficient_scope\", scope=\"admin\"",
            )],
            "forbidden",
        )
            .into_response()
    }
}

fn new_refresh(app: &App, user: &str, family: u32) -> String {
    let rt = hex(&random_bytes(24));
    app.refresh.lock().unwrap().insert(
        hex(&sha256(rt.as_bytes())),
        (user.to_string(), family, false),
    );
    rt
}

fn token_response(app: &App, user: &str, scope: &str, family: u32) -> Response {
    let body = json!({ "access_token": app.issue(user, scope, "cookbook-api", 900), "token_type": "Bearer", "expires_in": 900, "refresh_token": new_refresh(app, user, family) });
    ([(header::CACHE_CONTROL, "no-store")], Json(body)).into_response()
}

async fn token(State(app): St, Json(b): Json<Value>) -> Response {
    let (u, p) = (
        b["user"].as_str().unwrap_or(""),
        b["pw"].as_str().unwrap_or(""),
    );
    let (salt, want) = app
        .users
        .get(u)
        .map(|(s, w)| (*s, w.as_str()))
        .unwrap_or(("x", "00"));
    if !ct_eq(pw_hash(salt, p).as_bytes(), want.as_bytes()) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "invalid_grant"})),
        )
            .into_response();
    }
    let family = app.refresh.lock().unwrap().len() as u32 + 1;
    token_response(
        &app,
        u,
        if u == "root" { "read admin" } else { "read" },
        family,
    )
}

async fn refresh(State(app): St, Json(b): Json<Value>) -> Response {
    let key = hex(&sha256(
        b["refresh_token"].as_str().unwrap_or("").as_bytes(),
    ));
    let found = app
        .refresh
        .lock()
        .unwrap()
        .get_mut(&key)
        .map(|e| (e.0.clone(), e.1, std::mem::replace(&mut e.2, true)));
    let bad = || {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "invalid_grant"})),
        )
            .into_response()
    };
    match found {
        None => bad(),
        Some((_, family, _)) if app.revoked_families.lock().unwrap().contains(&family) => bad(),
        Some((_, family, true)) => {
            app.revoked_families.lock().unwrap().push(family); // reuse of a consumed token: assume theft, kill the family
            app.log.lock().unwrap().push(format!(
                "refresh token reuse detected: family {family} revoked"
            ));
            bad()
        }
        Some((user, family, false)) => token_response(&app, &user, "read", family),
    }
}

/// Access log: method, path, status and whether credentials were present, NEVER their value.
async fn access_log(State(app): St, req: Request, next: Next) -> Response {
    let line = format!(
        "{} {} auth={}",
        req.method(),
        req.uri().path(),
        if req.headers().contains_key(header::AUTHORIZATION) {
            "present"
        } else {
            "absent"
        }
    );
    let resp = next.run(req).await;
    app.log
        .lock()
        .unwrap()
        .push(format!("{line} -> {}", resp.status().as_u16()));
    resp
}

fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/basic", get(basic))
        .route("/apikey", get(api_key).post(api_key))
        .route("/opaque", get(opaque))
        .route("/jwt/me", get(jwt_me))
        .route("/jwt/admin", get(jwt_admin))
        .route("/auth/token", post(token))
        .route("/auth/refresh", post(refresh))
        .layer(middleware::from_fn_with_state(app.clone(), access_log))
        .with_state(app)
}

// ---------------------------------------------------------------- pure JWT attack tests
fn forge(header: Value, payload: &Value, sig: &str) -> String {
    format!(
        "{}.{}.{sig}",
        b64u_encode(header.to_string().as_bytes()),
        b64u_encode(payload.to_string().as_bytes())
    )
}

fn jwt_unit_tests() {
    title("JWT: validation rules and attacks (pure, injectable clock)");
    let mut ring = Keyring {
        keys: HashMap::new(),
        current: "k1".into(),
    };
    ring.keys
        .insert("k1".into(), b"first-signing-key-32-bytes-long!".to_vec());
    let val = Validation {
        iss: "https://auth.cookbook.test".into(),
        aud: "cookbook-api".into(),
        leeway: 30,
    };
    let claims = json!({ "iss": val.iss, "sub": "ann", "aud": "cookbook-api", "iat": T0, "nbf": T0, "exp": T0 + 900, "scope": "read" });
    let good = jwt_encode("k1", &ring.keys["k1"], &claims);
    let (h, p) = jwt_peek(&good).unwrap();
    println!(
        "token   : {}\nheader  : {h}\npayload : {p}",
        good.replacen(good.split('.').nth(2).unwrap(), "<sig>", 1)
    );
    assert!(
        good.split('.').all(|s| !s.contains('=')),
        "base64url without padding"
    );
    // RFC 7515 appendix A.1 style sanity: HMAC test vector for the primitive itself
    assert_eq!(
        hex(&hmac_sha256(
            b"key",
            b"The quick brown fox jumps over the lazy dog"
        )),
        "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
    );

    let dec = |t: &str, now: u64| jwt_decode(t, &ring, &val, now);
    let show = |what: &str, r: &Result<Value, JwtError>| {
        println!(
            "{what:<44} => {}",
            match r {
                Ok(_) => "ACCEPTED".to_string(),
                Err(e) => format!("rejected {e:?}"),
            }
        )
    };
    let r = dec(&good, T0 + 10);
    show("valid token", &r);
    assert_eq!(r.unwrap()["sub"], "ann");

    let none_tok = forge(
        json!({"alg": "none", "typ": "JWT"}),
        &json!({ "iss": val.iss, "sub": "root", "aud": "cookbook-api", "iat": T0, "nbf": T0, "exp": T0 + 900, "scope": "read admin" }),
        "",
    );
    let r = dec(&none_tok, T0 + 10);
    show("alg:none, empty signature", &r);
    assert_eq!(r, Err(JwtError::UnsupportedAlg("none".into())));
    for variant in ["None", "NONE", "nOnE", "hs256", "RS256", "ES256"] {
        let t = forge(json!({"alg": variant}), &claims, "AAAA");
        assert!(
            matches!(dec(&t, T0), Err(JwtError::UnsupportedAlg(_))),
            "{variant}"
        );
    }
    assert!(
        matches!(
            dec(&forge(json!({"typ": "JWT"}), &claims, "AAAA"), T0),
            Err(JwtError::UnsupportedAlg(_))
        ),
        "missing alg"
    );

    // algorithm confusion: attacker knows only the PUBLIC key and signs HS256 with it as the secret
    let public_key = b"-----BEGIN PUBLIC KEY----- (pretend RSA public key, which is public) -----END PUBLIC KEY-----";
    let evil_claims = json!({ "iss": val.iss, "sub": "root", "aud": "cookbook-api", "iat": T0, "nbf": T0, "exp": T0 + 900, "scope": "read admin" });
    let confusion = jwt_encode("k1", public_key, &evil_claims);
    assert!(
        vulnerable_decode(&confusion, public_key).is_some(),
        "the broken verifier is fooled by HS256-with-public-key"
    );
    assert!(
        vulnerable_decode(&none_tok, public_key).is_some(),
        "...and by alg none"
    );
    let r = dec(&confusion, T0 + 10);
    show("algorithm confusion (HMAC keyed with pubkey)", &r);
    assert_eq!(
        r,
        Err(JwtError::BadSignature),
        "our verifier only ever uses its own secret for HS256"
    );

    // tampered payload keeps the old signature
    let parts: Vec<&str> = good.split('.').collect();
    let evil_payload = b64u_encode(evil_claims.to_string().as_bytes());
    let tampered = format!("{}.{evil_payload}.{}", parts[0], parts[2]);
    let r = dec(&tampered, T0 + 10);
    show("tampered payload (sub=root, scope+admin)", &r);
    assert_eq!(r, Err(JwtError::BadSignature));

    // time rules with leeway 30s
    let r = dec(&good, T0 + 900 + 30);
    show("expired by exactly leeway (still ok)", &r);
    assert!(r.is_ok());
    let r = dec(&good, T0 + 900 + 31);
    show("expired beyond leeway", &r);
    assert_eq!(r, Err(JwtError::Expired));
    let future = jwt_encode(
        "k1",
        &ring.keys["k1"],
        &json!({ "iss": val.iss, "sub": "ann", "aud": "cookbook-api", "iat": T0, "nbf": T0 + 600, "exp": T0 + 1200 }),
    );
    assert_eq!(dec(&future, T0 + 100), Err(JwtError::NotYetValid));
    assert!(
        dec(&future, T0 + 575).is_ok(),
        "leeway tolerates clock skew on nbf"
    );
    show("nbf in the future", &dec(&future, T0 + 100));

    // audience / issuer / missing claims
    let mut c2 = claims.clone();
    c2["aud"] = json!("other-api");
    assert_eq!(
        dec(&jwt_encode("k1", &ring.keys["k1"], &c2), T0),
        Err(JwtError::WrongAudience)
    );
    show(
        "aud = other-api",
        &dec(&jwt_encode("k1", &ring.keys["k1"], &c2), T0),
    );
    c2["aud"] = json!(["x", "cookbook-api"]);
    assert!(
        dec(&jwt_encode("k1", &ring.keys["k1"], &c2), T0).is_ok(),
        "aud may be an array containing us"
    );
    c2["iss"] = json!("https://evil.test");
    assert_eq!(
        dec(&jwt_encode("k1", &ring.keys["k1"], &c2), T0),
        Err(JwtError::WrongIssuer)
    );
    let mut c3 = claims.clone();
    c3.as_object_mut().unwrap().remove("exp");
    assert_eq!(
        dec(&jwt_encode("k1", &ring.keys["k1"], &c3), T0),
        Err(JwtError::MissingClaim("exp")),
        "tokens without exp are refused"
    );
    for bad in ["", "a.b", "a.b.c.d", "!!!.???.***", &"x".repeat(5000)] {
        assert_eq!(dec(bad, T0), Err(JwtError::Malformed), "{bad:.20}");
    }

    // kid rotation
    ring.keys
        .insert("k2".into(), b"second-signing-key-32-bytes-lng".to_vec());
    ring.current = "k2".into();
    let new_tok = jwt_encode(&ring.current, &ring.keys["k2"], &claims);
    assert!(
        jwt_decode(&good, &ring, &val, T0).is_ok(),
        "overlap: old tokens (kid k1) still verify"
    );
    assert!(
        jwt_decode(&new_tok, &ring, &val, T0).is_ok(),
        "new tokens (kid k2) verify"
    );
    ring.keys.remove("k1");
    assert_eq!(
        jwt_decode(&good, &ring, &val, T0),
        Err(JwtError::UnknownKid),
        "k1 retired"
    );
    assert!(jwt_decode(&new_tok, &ring, &val, T0).is_ok());
    let evil_kid = forge(
        json!({"alg": "HS256", "kid": "../../etc/passwd"}),
        &claims,
        "AAAA",
    );
    assert_eq!(
        jwt_decode(&evil_kid, &ring, &val, T0),
        Err(JwtError::UnknownKid)
    );
    println!("kid rotation: k1+k2 overlap, then k1 retired => old tokens UnknownKid");

    // timing
    let sig = hmac_sha256(b"k", b"m");
    let mut guess = [0u8; 32];
    let mut steps = vec![];
    for correct_prefix in [0usize, 8, 16, 31] {
        guess[..correct_prefix].copy_from_slice(&sig[..correct_prefix]);
        guess[correct_prefix..].fill(sig[correct_prefix].wrapping_add(1));
        steps.push(naive_compare_steps(&sig, &guess));
        assert!(!ct_eq(&sig, &guess));
    }
    println!(
        "naive == inspects {steps:?} bytes as the guessed prefix grows 0,8,16,31 -> a timing oracle; ct_eq always scans all 32\n"
    );
    assert_eq!(steps, vec![1, 9, 17, 32]);
}

// ---------------------------------------------------------------- HTTP demo
#[tokio::main]
async fn main() {
    jwt_unit_tests();
    REDACT_BODY.store(true, Ordering::Relaxed);

    let clock = Arc::new(AtomicU64::new(T0));
    let mut ring = Keyring {
        keys: HashMap::new(),
        current: "k2".into(),
    };
    ring.keys
        .insert("k1".into(), b"first-signing-key-32-bytes-long!".to_vec());
    ring.keys
        .insert("k2".into(), b"second-signing-key-32-bytes-lng".to_vec());
    let app = Arc::new(App {
        now: clock.clone(),
        ring: Mutex::new(ring),
        val: Validation {
            iss: "https://auth.cookbook.test".into(),
            aud: "cookbook-api".into(),
            leeway: 30,
        },
        users: HashMap::from([
            ("ann", ("s1", pw_hash("s1", "s3cret"))),
            ("root", ("s2", pw_hash("s2", "r00t"))),
        ]),
        api_keys: HashMap::from([(
            hex(&sha256(API_KEY.as_bytes())),
            ("billing-service", vec!["read"]),
        )]),
        opaque: HashMap::from([(hex(&sha256(OPAQUE.as_bytes())), "ann")]),
        refresh: Mutex::new(HashMap::new()),
        revoked_families: Mutex::new(vec![]),
        log: Mutex::new(vec![]),
    });
    let srv = Server::start(router(app.clone())).await;
    let c = client();
    let www = |e: &Exch| e.h("www-authenticate");

    title("HTTP Basic: challenge, credentials, failures");
    let r = send(&c, c.get(srv.url("/basic"))).await;
    assert_eq!(r.status.as_u16(), 401);
    assert!(
        www(&r).starts_with("Basic realm="),
        "401 MUST carry WWW-Authenticate"
    );
    let r = send(
        &c,
        c.get(srv.url("/basic")).basic_auth("ann", Some("s3cret")),
    )
    .await;
    assert_eq!(
        (r.status.as_u16(), r.text()),
        (200, "hello ann".to_string())
    );
    println!(
        "(on the wire the header is: Authorization: Basic {} == base64(\"ann:s3cret\"): encoding, NOT encryption; use TLS)\n",
        b64_std_encode(b"ann:s3cret")
    );
    assert_eq!(b64_std_encode(b"ann:s3cret"), "YW5uOnMzY3JldA==");
    assert_eq!(
        send(
            &c,
            c.get(srv.url("/basic")).basic_auth("ann", Some("wrong"))
        )
        .await
        .status
        .as_u16(),
        401
    );
    assert_eq!(
        send(&c, c.get(srv.url("/basic")).basic_auth("nobody", Some("x")))
            .await
            .status
            .as_u16(),
        401
    );
    assert_eq!(
        c.get(srv.url("/basic"))
            .header("authorization", "Basic !!!not-base64")
            .send()
            .await
            .unwrap()
            .status(),
        401,
        "garbage is 401, not 500"
    );
    assert_eq!(
        c.get(srv.url("/basic"))
            .header("authorization", "basic YW5uOnMzY3JldA==")
            .send()
            .await
            .unwrap()
            .status(),
        200,
        "scheme is case-insensitive"
    );

    title("API key: 401 vs 403");
    let r = send(&c, c.get(srv.url("/apikey"))).await;
    assert_eq!(r.status.as_u16(), 401);
    let r = send(
        &c,
        c.get(srv.url("/apikey"))
            .header("x-api-key", "ck_test_wrong"),
    )
    .await;
    assert_eq!(r.status.as_u16(), 401);
    let r = send(&c, c.get(srv.url("/apikey")).header("x-api-key", API_KEY)).await;
    assert_eq!(r.status.as_u16(), 200);
    let r = send(&c, c.post(srv.url("/apikey")).header("x-api-key", API_KEY)).await;
    assert_eq!(
        r.status.as_u16(),
        403,
        "authenticated, but a read-only key may not write; no new challenge"
    );
    assert!(r.h("www-authenticate").is_empty());

    title("opaque Bearer token (RFC 6750)");
    assert_eq!(
        send(&c, c.get(srv.url("/opaque")))
            .await
            .h("www-authenticate"),
        "Bearer realm=\"pat\""
    );
    let r = send(&c, c.get(srv.url("/opaque")).bearer_auth("pat_wrong")).await;
    assert!(www(&r).contains("invalid_token"));
    assert_eq!(
        send(&c, c.get(srv.url("/opaque")).bearer_auth(OPAQUE))
            .await
            .text(),
        "token belongs to ann"
    );

    title("JWT behind the Claims extractor");
    let me = |c: &Client, t: &str| c.get(srv.url("/jwt/me")).bearer_auth(t);
    let r = send(&c, c.get(srv.url("/jwt/me"))).await;
    assert_eq!(
        (r.status.as_u16(), www(&r)),
        (401, "Bearer realm=\"api\"".to_string()),
        "no credentials: bare challenge"
    );
    let good = app.issue("ann", "read", "cookbook-api", 900);
    let r = send(&c, me(&c, &good)).await;
    assert_eq!(r.status.as_u16(), 200);
    assert_eq!(
        serde_json::from_slice::<Value>(&r.body).unwrap()["sub"],
        "ann"
    );
    let claims_of = |sub: &str, scope: &str, aud: &str| json!({ "iss": "https://auth.cookbook.test", "sub": sub, "aud": aud, "iat": T0, "nbf": T0, "exp": T0 + 900, "scope": scope });
    let k2 = b"second-signing-key-32-bytes-lng";
    let attacks: Vec<(&str, String, &str)> = vec![
        (
            "alg:none",
            forge(
                json!({"alg": "none"}),
                &claims_of("root", "read admin", "cookbook-api"),
                "",
            ),
            "invalid token",
        ),
        (
            "algorithm confusion",
            jwt_encode(
                "k2",
                b"-----BEGIN PUBLIC KEY-----",
                &claims_of("root", "read admin", "cookbook-api"),
            ),
            "invalid token",
        ),
        (
            "tampered payload",
            {
                let p: Vec<&str> = good.split('.').collect();
                format!(
                    "{}.{}.{}",
                    p[0],
                    b64u_encode(
                        claims_of("root", "read admin", "cookbook-api")
                            .to_string()
                            .as_bytes()
                    ),
                    p[2]
                )
            },
            "invalid token",
        ),
        (
            "wrong audience",
            jwt_encode("k2", k2, &claims_of("ann", "read", "other-api")),
            "invalid token",
        ),
        (
            "unknown kid",
            jwt_encode("k9", k2, &claims_of("ann", "read", "cookbook-api")),
            "invalid token",
        ),
    ];
    for (name, tok, desc) in attacks {
        println!("-- attack: {name}");
        let r = send(&c, me(&c, &tok)).await;
        assert_eq!(r.status.as_u16(), 401, "{name}");
        assert!(
            www(&r).contains("error=\"invalid_token\"") && www(&r).contains(desc),
            "{name}: {}",
            www(&r)
        );
    }
    println!("-- expired token");
    clock.fetch_add(901 + 30, Ordering::SeqCst);
    let r = send(&c, me(&c, &good)).await;
    assert_eq!(r.status.as_u16(), 401);
    assert!(www(&r).contains("token expired"));
    clock.store(T0, Ordering::SeqCst);

    title("403 vs 401: valid token, insufficient scope");
    let r = send(&c, c.get(srv.url("/jwt/admin")).bearer_auth(&good)).await;
    assert_eq!(r.status.as_u16(), 403);
    assert!(www(&r).contains("insufficient_scope"));
    let admin = app.issue("root", "read admin", "cookbook-api", 900);
    assert_eq!(
        send(&c, c.get(srv.url("/jwt/admin")).bearer_auth(&admin))
            .await
            .text(),
        "welcome, admin"
    );

    title("refresh-token flow: login -> refresh -> rotation -> reuse detection");
    let r = send(
        &c,
        c.post(srv.url("/auth/token"))
            .json(&json!({"user": "ann", "pw": "s3cret"})),
    )
    .await;
    assert_eq!(
        r.h("cache-control"),
        "no-store",
        "token responses must not be cached"
    );
    let t1: Value = serde_json::from_slice(&r.body).unwrap();
    let rt1 = t1["refresh_token"].as_str().unwrap().to_string();
    assert_eq!(
        send(
            &c,
            c.post(srv.url("/auth/token"))
                .json(&json!({"user": "ann", "pw": "nope"}))
        )
        .await
        .status
        .as_u16(),
        401
    );
    clock.fetch_add(800, Ordering::SeqCst);
    let r = send(
        &c,
        c.post(srv.url("/auth/refresh"))
            .json(&json!({"refresh_token": rt1})),
    )
    .await;
    assert_eq!(r.status.as_u16(), 200);
    let t2: Value = serde_json::from_slice(&r.body).unwrap();
    let rt2 = t2["refresh_token"].as_str().unwrap().to_string();
    assert_ne!(rt1, rt2, "refresh tokens are single-use and rotate");
    assert_eq!(
        send(&c, me(&c, t2["access_token"].as_str().unwrap()))
            .await
            .status
            .as_u16(),
        200
    );
    let r = send(
        &c,
        c.post(srv.url("/auth/refresh"))
            .json(&json!({"refresh_token": rt1})),
    )
    .await;
    assert_eq!(
        r.status.as_u16(),
        401,
        "consumed token replayed: theft suspected"
    );
    let r = send(
        &c,
        c.post(srv.url("/auth/refresh"))
            .json(&json!({"refresh_token": rt2})),
    )
    .await;
    assert_eq!(
        r.status.as_u16(),
        401,
        "...so even the legitimate successor is revoked, forcing re-login"
    );

    title("access log never contains credentials");
    let log = app.log.lock().unwrap().clone();
    for l in log
        .iter()
        .filter(|l| l.contains("/jwt/me") || l.contains("/basic"))
        .take(6)
    {
        println!("  {l}");
    }
    let secrets = [
        good.as_str(),
        admin.as_str(),
        "s3cret",
        API_KEY,
        OPAQUE,
        "YW5uOnMzY3JldA",
        "Bearer ",
        "Basic ",
    ];
    for l in &log {
        assert!(
            secrets.iter().all(|s| !l.contains(s)),
            "secret leaked into log line: {l}"
        );
    }
    assert!(
        log.iter().any(|l| l.contains("auth=present"))
            && log.iter().any(|l| l.contains("auth=absent"))
    );
    assert!(log.iter().any(|l| l.contains("reuse detected")));

    drop(c);
    srv.shutdown().await;
    println!("\nall authentication assertions passed");
}
