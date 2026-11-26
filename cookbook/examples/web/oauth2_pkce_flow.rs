//! RECIPE 307 (web): OAuth 2.0 authorization code flow with PKCE, three servers, full redirect chain
//! LEVEL: advanced
//! Use in: auth stage (login with an identity provider, API access tokens).   Run: cargo run --release --example oauth2_pkce_flow
//!
//! What: three real axum servers on loopback, named in the transcripts `app.test` (client app / BFF),
//! `as.test` (authorization server) and `rs.test` (resource server), plus a hand-driven "browser" that follows
//! redirects one hop at a time and prints each hop.
//!  * Authorization request: `response_type=code&client_id&redirect_uri&scope&state&code_challenge&code_challenge_method=S256`.
//!    The AS validates `client_id` and the EXACT `redirect_uri` BEFORE it ever redirects (never redirect to an unregistered URI).
//!  * Login + consent, then a 302 back to the app with `code` and `state`. The app checks `state` against its
//!    cookie-bound session (CSRF / login-CSRF defence).
//!  * Token endpoint: code is single use and short lived (60s virtual clock), `redirect_uri` must equal the one used
//!    at /authorize, and `code_verifier` must hash (SHA-256, base64url) to the stored `code_challenge`: so a stolen
//!    code alone is useless. Replaying a code revokes the tokens it produced.
//!  * Access token (10 min) + refresh token with ROTATION: every refresh returns a new refresh token; presenting an
//!    old one is treated as theft and revokes the whole token FAMILY.
//!  * Resource server validates tokens by introspection (RFC 7662; a JWT would be validated locally with a signature
//!    instead), enforces scopes: 401 + `WWW-Authenticate: Bearer`, 403 `insufficient_scope`.
//!  * Attacks asserted to fail: code interception without the verifier, state mismatch, code replay, redirect_uri
//!    tampering, expired code, PKCE downgrade.
//! Tokens are opaque and the PRNG is a fixed-seed xorshift only so that output is reproducible: use an OS CSPRNG in real code.
//! COMMON USES: "Sign in with ..." buttons, mobile/SPA apps (public clients: PKCE replaces the client secret),
//! API access delegation (GitHub/Google/Slack apps), BFF pattern where tokens never reach the browser.
//! Read: https://datatracker.ietf.org/doc/html/rfc6749 , https://datatracker.ietf.org/doc/html/rfc7636 ,
//! https://datatracker.ietf.org/doc/html/rfc6750 , https://datatracker.ietf.org/doc/html/rfc7662 ,
//! https://datatracker.ietf.org/doc/html/rfc9700
#![allow(dead_code)]
use axum::{
    Form, Router,
    body::{Body, Bytes},
    extract::{Query, State},
    http::HeaderMap,
    response::Response,
    routing::{get, post},
};
use reqwest::{Client, RequestBuilder};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering::SeqCst},
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

const CODE_TTL: u64 = 60;
const AT_TTL: u64 = 600;
const CLIENT_ID: &str = "forge-notes-app";

fn pkce_challenge(verifier: &str) -> String {
    b64e(&sha256(verifier.as_bytes()))
}
fn pct(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn rand_b64(rng: &mut u64, n: usize) -> String {
    let bytes: Vec<u8> = (0..n)
        .map(|_| {
            *rng ^= *rng << 13;
            *rng ^= *rng >> 7;
            *rng ^= *rng << 17;
            (*rng >> 24) as u8
        })
        .collect();
    b64e(&bytes)
}
fn json_resp(status: u16, headers: &[(&str, String)], body: Value) -> Response {
    let mut b = Response::builder()
        .status(status)
        .header("content-type", "application/json");
    for (k, v) in headers {
        b = b.header(*k, v);
    }
    b.body(Body::from(body.to_string())).unwrap()
}
fn redirect(to: &str, extra: &[(&str, String)]) -> Response {
    let mut b = Response::builder()
        .status(302)
        .header("location", to)
        .header("cache-control", "no-store");
    for (k, v) in extra {
        b = b.header(*k, v);
    }
    b.body(Body::empty()).unwrap()
}
fn oerr(status: u16, code: &str, desc: &str) -> Response {
    json_resp(
        status,
        &[
            ("cache-control", "no-store".into()),
            ("pragma", "no-cache".into()),
        ],
        json!({"error": code, "error_description": desc}),
    )
}

// ================================================================ authorization server
#[derive(Clone)]
struct AuthReq {
    client_id: String,
    redirect_uri: String,
    scope: String,
    state: String,
    challenge: String,
}
#[derive(Clone)]
struct CodeRec {
    req: AuthReq,
    user: String,
    issued: u64,
    used: bool,
    family: Option<u64>,
}
#[derive(Clone)]
struct AccessRec {
    user: String,
    client: String,
    scope: String,
    exp: u64,
    family: u64,
}
#[derive(Clone)]
struct RefreshRec {
    user: String,
    client: String,
    scope: String,
    family: u64,
    used: bool,
}
struct ClientCfg {
    redirect_uris: Vec<String>,
    scopes: Vec<String>,
}
#[derive(Default)]
struct AsState {
    n: u64,
    rng: u64,
    clients: HashMap<String, ClientCfg>,
    pending: HashMap<String, AuthReq>,
    codes: HashMap<String, CodeRec>,
    access: HashMap<String, AccessRec>,
    refresh: HashMap<String, RefreshRec>,
    revoked: HashMap<u64, bool>,
}
struct As {
    clock: Arc<AtomicU64>,
    st: Mutex<AsState>,
}

fn err_redirect(req_redirect: &str, state: &str, code: &str, desc: &str) -> Response {
    redirect(
        &format!(
            "{req_redirect}?error={code}&error_description={}&state={}",
            pct(desc),
            pct(state)
        ),
        &[],
    )
}

async fn authorize(State(a): State<Arc<As>>, Query(q): Query<HashMap<String, String>>) -> Response {
    let g = |k: &str| q.get(k).cloned().unwrap_or_default();
    let mut s = a.st.lock().unwrap();
    // 1. these two errors must NOT redirect: the redirect_uri is not trusted yet
    let Some(cl) = s.clients.get(&g("client_id")) else {
        return oerr(
            400,
            "invalid_request",
            "unknown client_id (not redirecting)",
        );
    };
    if !cl.redirect_uris.contains(&g("redirect_uri")) {
        return oerr(
            400,
            "invalid_request",
            "redirect_uri is not registered for this client (not redirecting)",
        );
    }
    // 2. from here on errors are reported to the (now trusted) redirect_uri
    let (ru, st) = (g("redirect_uri"), g("state"));
    if g("response_type") != "code" {
        return err_redirect(
            &ru,
            &st,
            "unsupported_response_type",
            "only response_type=code",
        );
    }
    let requested: Vec<String> = g("scope")
        .split(' ')
        .filter(|x| !x.is_empty())
        .map(str::to_string)
        .collect();
    if requested.is_empty() || !requested.iter().all(|r| cl.scopes.contains(r)) {
        return err_redirect(
            &ru,
            &st,
            "invalid_scope",
            "scope not allowed for this client",
        );
    }
    if g("code_challenge").is_empty() || g("code_challenge_method") != "S256" {
        return err_redirect(
            &ru,
            &st,
            "invalid_request",
            "PKCE with code_challenge_method=S256 is required",
        );
    }
    s.n += 1;
    let id = format!("req-{}", s.n);
    s.pending.insert(
        id.clone(),
        AuthReq {
            client_id: g("client_id"),
            redirect_uri: ru,
            scope: requested.join(" "),
            state: st,
            challenge: g("code_challenge"),
        },
    );
    redirect(&format!("/consent?req={id}"), &[])
}

async fn consent_page(
    State(a): State<Arc<As>>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let s = a.st.lock().unwrap();
    match q.get("req").and_then(|r| s.pending.get(r)) {
        Some(p) => json_resp(
            200,
            &[],
            json!({"page": "login+consent", "client": p.client_id, "wants_scopes": p.scope, "form": ["req", "username", "password", "decision=allow|deny"]}),
        ),
        None => oerr(
            400,
            "invalid_request",
            "unknown or expired authorization request",
        ),
    }
}

async fn consent_submit(
    State(a): State<Arc<As>>,
    Form(f): Form<HashMap<String, String>>,
) -> Response {
    let g = |k: &str| f.get(k).cloned().unwrap_or_default();
    let now = a.clock.load(SeqCst);
    let mut s = a.st.lock().unwrap();
    let Some(p) = s.pending.get(&g("req")).cloned() else {
        return oerr(400, "invalid_request", "unknown authorization request");
    };
    if g("username") != "alice" || g("password") != "wonderland" {
        return oerr(401, "access_denied", "bad credentials");
    }
    s.pending.remove(&g("req"));
    if g("decision") != "allow" {
        return err_redirect(
            &p.redirect_uri,
            &p.state,
            "access_denied",
            "the user denied the request",
        );
    }
    let mut rng = s.rng ^ s.n.wrapping_mul(0x9E3779B97F4A7C15) | 1;
    let code = rand_b64(&mut rng, 24);
    s.rng = rng;
    s.codes.insert(
        code.clone(),
        CodeRec {
            req: p.clone(),
            user: "alice".into(),
            issued: now,
            used: false,
            family: None,
        },
    );
    redirect(
        &format!(
            "{}?code={}&state={}",
            p.redirect_uri,
            pct(&code),
            pct(&p.state)
        ),
        &[],
    )
}

fn issue(
    s: &mut AsState,
    user: &str,
    client: &str,
    scope: &str,
    family: Option<u64>,
    now: u64,
) -> (Value, u64) {
    s.n += 1;
    let fam = family.unwrap_or(s.n);
    let mut rng = s.rng ^ s.n.wrapping_mul(0xD1B54A32D192ED03) | 1;
    let (at, rt) = (
        format!("at_{}", rand_b64(&mut rng, 18)),
        format!("rt_{}", rand_b64(&mut rng, 18)),
    );
    s.rng = rng;
    s.access.insert(
        at.clone(),
        AccessRec {
            user: user.into(),
            client: client.into(),
            scope: scope.into(),
            exp: now + AT_TTL,
            family: fam,
        },
    );
    s.refresh.insert(
        rt.clone(),
        RefreshRec {
            user: user.into(),
            client: client.into(),
            scope: scope.into(),
            family: fam,
            used: false,
        },
    );
    (
        json!({"access_token": at, "token_type": "Bearer", "expires_in": AT_TTL, "refresh_token": rt, "scope": scope}),
        fam,
    )
}

async fn token(State(a): State<Arc<As>>, Form(f): Form<HashMap<String, String>>) -> Response {
    let g = |k: &str| f.get(k).cloned().unwrap_or_default();
    let now = a.clock.load(SeqCst);
    let mut s = a.st.lock().unwrap();
    if !s.clients.contains_key(&g("client_id")) {
        return oerr(401, "invalid_client", "unknown client");
    }
    let nostore = [
        ("cache-control", "no-store".to_string()),
        ("pragma", "no-cache".to_string()),
    ];
    match g("grant_type").as_str() {
        "authorization_code" => {
            let code = g("code");
            let Some(c) = s.codes.get(&code).cloned() else {
                return oerr(400, "invalid_grant", "unknown authorization code");
            };
            if c.req.client_id != g("client_id") {
                return oerr(400, "invalid_grant", "code was issued to another client");
            }
            if c.used {
                if let Some(fam) = c.family {
                    s.revoked.insert(fam, true);
                }
                return oerr(
                    400,
                    "invalid_grant",
                    "authorization code already used: tokens issued from it are revoked",
                );
            }
            if now.saturating_sub(c.issued) > CODE_TTL {
                return oerr(400, "invalid_grant", "authorization code expired");
            }
            if g("redirect_uri") != c.req.redirect_uri {
                return oerr(
                    400,
                    "invalid_grant",
                    "redirect_uri does not match the authorization request",
                );
            }
            let v = g("code_verifier");
            if !(43..=128).contains(&v.len())
                || !ct_eq(pkce_challenge(&v).as_bytes(), c.req.challenge.as_bytes())
            {
                return oerr(
                    400,
                    "invalid_grant",
                    "PKCE verification failed (code_verifier does not match code_challenge)",
                );
            }
            s.codes.get_mut(&code).unwrap().used = true;
            let (body, fam) = issue(&mut s, &c.user, &c.req.client_id, &c.req.scope, None, now);
            s.codes.get_mut(&code).unwrap().family = Some(fam);
            json_resp(200, &nostore, body)
        }
        "refresh_token" => {
            let rt = g("refresh_token");
            let Some(r) = s.refresh.get(&rt).cloned() else {
                return oerr(400, "invalid_grant", "unknown refresh token");
            };
            if r.client != g("client_id") {
                return oerr(
                    400,
                    "invalid_grant",
                    "refresh token belongs to another client",
                );
            }
            if s.revoked.get(&r.family) == Some(&true) {
                return oerr(400, "invalid_grant", "token family has been revoked");
            }
            if r.used {
                s.revoked.insert(r.family, true); // reuse of a rotated token => assume theft, kill the family
                return oerr(
                    400,
                    "invalid_grant",
                    "refresh token reuse detected: token family revoked",
                );
            }
            s.refresh.get_mut(&rt).unwrap().used = true;
            let (body, _) = issue(&mut s, &r.user, &r.client, &r.scope, Some(r.family), now);
            json_resp(200, &nostore, body)
        }
        _ => oerr(
            400,
            "unsupported_grant_type",
            "use authorization_code or refresh_token",
        ),
    }
}

async fn introspect(
    State(a): State<Arc<As>>,
    headers: HeaderMap,
    Form(f): Form<HashMap<String, String>>,
) -> Response {
    if headers.get("x-rs-secret").and_then(|v| v.to_str().ok()) != Some("rs-secret") {
        return oerr(401, "invalid_client", "resource server must authenticate");
    }
    let now = a.clock.load(SeqCst);
    let s = a.st.lock().unwrap();
    match f.get("token").and_then(|t| s.access.get(t)) {
        Some(r) if now < r.exp && s.revoked.get(&r.family) != Some(&true) => json_resp(
            200,
            &[],
            json!({"active": true, "scope": r.scope, "sub": r.user, "client_id": r.client, "exp": r.exp}),
        ),
        _ => json_resp(200, &[], json!({"active": false})),
    }
}

// ================================================================ resource server
struct Rs {
    c: Client,
    as_url: String,
    notes: Mutex<Vec<String>>,
    introspections: AtomicU64,
}

fn www_auth(extra: &str) -> [(&'static str, String); 2] {
    [
        ("www-authenticate", format!("Bearer realm=\"notes\"{extra}")),
        ("cache-control", "no-store".into()),
    ]
}
async fn rs_guard(rs: &Rs, headers: &HeaderMap, need: &str) -> Result<Value, Response> {
    let Some(tok) = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    else {
        return Err(json_resp(
            401,
            &[("www-authenticate", "Bearer realm=\"notes\"".into())],
            json!({"error": "missing bearer token"}),
        ));
    };
    rs.introspections.fetch_add(1, SeqCst);
    let info: Value =
        rs.c.post(format!("{}/introspect", rs.as_url))
            .header("x-rs-secret", "rs-secret")
            .form(&[("token", tok)])
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
    if info["active"] != true {
        return Err(json_resp(
            401,
            &www_auth(", error=\"invalid_token\", error_description=\"token expired or revoked\""),
            json!({"error": "invalid_token"}),
        ));
    }
    if !info["scope"]
        .as_str()
        .unwrap_or("")
        .split(' ')
        .any(|s| s == need)
    {
        return Err(json_resp(
            403,
            &www_auth(&format!(", error=\"insufficient_scope\", scope=\"{need}\"")),
            json!({"error": "insufficient_scope", "required": need}),
        ));
    }
    Ok(info)
}
async fn notes_get(State(rs): State<Arc<Rs>>, headers: HeaderMap) -> Response {
    match rs_guard(&rs, &headers, "notes:read").await {
        Ok(i) => json_resp(
            200,
            &[],
            json!({"user": i["sub"], "notes": *rs.notes.lock().unwrap()}),
        ),
        Err(r) => r,
    }
}
async fn notes_post(State(rs): State<Arc<Rs>>, headers: HeaderMap, body: Bytes) -> Response {
    match rs_guard(&rs, &headers, "notes:write").await {
        Ok(_) => {
            rs.notes
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(&body).to_string());
            json_resp(201, &[], json!({"created": true}))
        }
        Err(r) => r,
    }
}

// ================================================================ client app (a BFF: tokens stay server-side)
#[derive(Default, Clone)]
struct Sess {
    state: String,
    verifier: String,
    redirect_uri: String,
    tokens: Option<Value>,
}
struct Ca {
    c: Client,
    as_url: String,
    rs_url: String,
    me: RwLock<String>,
    sessions: Mutex<HashMap<String, Sess>>,
    rng: Mutex<u64>,
    n: AtomicU64,
}
fn cookie_sid(h: &HeaderMap) -> Option<String> {
    h.get("cookie").and_then(|v| v.to_str().ok()).and_then(|c| {
        c.split(';')
            .find_map(|p| p.trim().strip_prefix("sid=").map(str::to_string))
    })
}

async fn ca_start(State(ca): State<Arc<Ca>>, Query(q): Query<HashMap<String, String>>) -> Response {
    let (verifier, state) = {
        let mut r = ca.rng.lock().unwrap();
        (rand_b64(&mut r, 32), rand_b64(&mut r, 16))
    };
    let sid = format!("sid{}", ca.n.fetch_add(1, SeqCst) + 1);
    let me = ca.me.read().unwrap().clone();
    let redirect_uri = q
        .get("redirect_uri")
        .cloned()
        .unwrap_or(format!("{me}/callback"));
    let scope = q.get("scope").cloned().unwrap_or("notes:read".into());
    ca.sessions.lock().unwrap().insert(
        sid.clone(),
        Sess {
            state: state.clone(),
            verifier: verifier.clone(),
            redirect_uri: redirect_uri.clone(),
            tokens: None,
        },
    );
    let url = format!(
        "{}/authorize?response_type=code&client_id={CLIENT_ID}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
        ca.as_url,
        pct(&redirect_uri),
        pct(&scope),
        pct(&state),
        pct(&pkce_challenge(&verifier))
    );
    redirect(
        &url,
        &[(
            "set-cookie",
            format!("sid={sid}; HttpOnly; SameSite=Lax; Path=/"),
        )],
    )
}

async fn ca_callback(
    State(ca): State<Arc<Ca>>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let Some(sid) = cookie_sid(&headers) else {
        return json_resp(400, &[], json!({"error": "no session cookie"}));
    };
    let Some(sess) = ca.sessions.lock().unwrap().get(&sid).cloned() else {
        return json_resp(400, &[], json!({"error": "unknown session"}));
    };
    let got = q.get("state").cloned().unwrap_or_default();
    if !ct_eq(got.as_bytes(), sess.state.as_bytes()) {
        return json_resp(
            400,
            &[],
            json!({"error": "state mismatch: possible CSRF, authorization response rejected"}),
        );
    }
    if let Some(e) = q.get("error") {
        return json_resp(
            403,
            &[],
            json!({"error": e, "description": q.get("error_description")}),
        );
    }
    let Some(code) = q.get("code") else {
        return json_resp(400, &[], json!({"error": "no code"}));
    };
    let r = call(&ca.c, ca.c.post(format!("{}/token", ca.as_url)).form(&[("grant_type", "authorization_code"), ("code", code.as_str()), ("redirect_uri", sess.redirect_uri.as_str()), ("client_id", CLIENT_ID), ("code_verifier", sess.verifier.as_str())]),
        "   app -> as: back-channel token exchange (PKCE verifier sent here, never through the browser)").await;
    if r.status != 200 {
        return json_resp(
            502,
            &[],
            json!({"error": "token exchange failed", "as_said": r.json()}),
        );
    }
    let t = r.json();
    let scope = t["scope"].clone();
    let mut m = ca.sessions.lock().unwrap();
    let s = m.get_mut(&sid).unwrap();
    s.tokens = Some(t);
    s.verifier.clear(); // one use
    json_resp(
        200,
        &[],
        json!({"logged_in": true, "user": "alice", "scope": scope}),
    )
}

async fn ca_api(ca: &Ca, headers: &HeaderMap, method: &str, body: String) -> Response {
    let Some(sid) = cookie_sid(headers) else {
        return json_resp(401, &[], json!({"error": "not logged in"}));
    };
    for attempt in 0..2 {
        let Some(tokens) = ca
            .sessions
            .lock()
            .unwrap()
            .get(&sid)
            .and_then(|s| s.tokens.clone())
        else {
            return json_resp(401, &[], json!({"error": "not logged in"}));
        };
        let url = format!("{}/api/notes", ca.rs_url);
        let rb = if method == "GET" {
            ca.c.get(url)
        } else {
            ca.c.post(url).body(body.clone())
        };
        let r = call(
            &ca.c,
            rb.bearer_auth(tokens["access_token"].as_str().unwrap()),
            &format!("   app -> rs: {method} /api/notes with the access token"),
        )
        .await;
        if r.status == 401 && attempt == 0 && r.h("www-authenticate").contains("invalid_token") {
            let rt = tokens["refresh_token"].as_str().unwrap().to_string();
            let rr = call(
                &ca.c,
                ca.c.post(format!("{}/token", ca.as_url)).form(&[
                    ("grant_type", "refresh_token"),
                    ("refresh_token", rt.as_str()),
                    ("client_id", CLIENT_ID),
                ]),
                "   app -> as: access token rejected, refreshing (rotation)",
            )
            .await;
            if rr.status != 200 {
                if let Some(s) = ca.sessions.lock().unwrap().get_mut(&sid) {
                    s.tokens = None;
                }
                return json_resp(
                    401,
                    &[],
                    json!({"error": "reauthentication_required", "as_said": rr.json()["error"]}),
                );
            }
            ca.sessions.lock().unwrap().get_mut(&sid).unwrap().tokens = Some(rr.json());
            continue;
        }
        let mut b = Response::builder()
            .status(r.status)
            .header("content-type", "application/json");
        if !r.h("www-authenticate").is_empty() {
            b = b.header("www-authenticate", r.h("www-authenticate"));
        }
        return b.body(Body::from(r.body)).unwrap();
    }
    json_resp(502, &[], json!({"error": "unreachable"}))
}
async fn ca_notes_get(State(ca): State<Arc<Ca>>, headers: HeaderMap) -> Response {
    ca_api(&ca, &headers, "GET", String::new()).await
}
async fn ca_notes_post(State(ca): State<Arc<Ca>>, headers: HeaderMap, body: Bytes) -> Response {
    ca_api(
        &ca,
        &headers,
        "POST",
        String::from_utf8_lossy(&body).to_string(),
    )
    .await
}

// ================================================================ the "browser"
struct Env {
    c: Client,
    as_s: Srv,
    rs_s: Srv,
    ca_s: Srv,
    asv: Arc<As>,
    rs: Arc<Rs>,
    ca: Arc<Ca>,
    clock: Arc<AtomicU64>,
}

/// Follow redirects one hop at a time (printing each). Stops at a non-redirect or when the next hop starts with `stop_at`.
async fn browse(
    env: &Env,
    mut url: String,
    cookie: &mut Option<String>,
    stop_at: &str,
) -> (Resp, String) {
    for hop in 1..=10 {
        let mut rb = env.c.get(&url);
        if url.starts_with(&env.ca_s.url(""))
            && let Some(ck) = cookie.as_ref()
        {
            rb = rb.header("cookie", format!("sid={ck}"));
        }
        let r = call(&env.c, rb, &format!("browser hop {hop}")).await;
        if let Some(sc) = r.headers.get("set-cookie").and_then(|v| v.to_str().ok()) {
            *cookie = sc
                .strip_prefix("sid=")
                .and_then(|s| s.split(';').next())
                .map(str::to_string);
        }
        if !(300..400).contains(&r.status) {
            return (r, url);
        }
        let loc = r.h("location");
        let next = reqwest::Url::parse(&url)
            .unwrap()
            .join(&loc)
            .unwrap()
            .to_string();
        if !stop_at.is_empty() && next.contains(stop_at) {
            return (r, url);
        }
        url = next;
    }
    panic!("redirect loop");
}
async fn submit_consent(env: &Env, req_id: &str, decision: &str, note: &str) -> Resp {
    call(
        &env.c,
        env.c.post(env.as_s.url("/consent")).form(&[
            ("req", req_id),
            ("username", "alice"),
            ("password", "wonderland"),
            ("decision", decision),
        ]),
        note,
    )
    .await
}
fn req_id(url: &str) -> String {
    url.split("req=").nth(1).unwrap().to_string()
}
fn qp(url: &str, k: &str) -> Option<String> {
    reqwest::Url::parse(url)
        .unwrap()
        .query_pairs()
        .find(|(a, _)| a == k)
        .map(|(_, v)| v.into_owned())
}

/// Full login through the client app: /start -> as /authorize -> /consent -> callback. Returns (callback response, cookie).
async fn app_login(env: &Env, scope: &str) -> (Resp, String) {
    let mut cookie = None;
    let start = format!("{}?scope={}", env.ca_s.url("/start"), pct(scope));
    let (consent, last) = browse(env, start, &mut cookie, "").await;
    assert_eq!(consent.status, 200, "consent page");
    let cb = submit_consent(
        env,
        &req_id(&last),
        "allow",
        "user logs in and clicks Allow",
    )
    .await;
    assert_eq!(cb.status, 302);
    let (done, _) = browse(env, cb.h("location"), &mut cookie, "").await;
    (done, cookie.unwrap())
}

/// Drive the AS directly as a PKCE client (no app): returns the callback Location.
async fn direct_authorize(
    env: &Env,
    verifier: &str,
    state: &str,
    scope: &str,
    redirect_uri: &str,
) -> String {
    quiet(true);
    let mut ck = None;
    let url = format!(
        "{}/authorize?response_type=code&client_id={CLIENT_ID}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
        env.as_s.url(""),
        pct(redirect_uri),
        pct(scope),
        pct(state),
        pct(&pkce_challenge(verifier))
    );
    let (_, last) = browse(env, url, &mut ck, "").await;
    let r = submit_consent(env, &req_id(&last), "allow", "").await;
    quiet(false);
    assert_eq!(r.status, 302);
    r.h("location")
}
async fn exchange(env: &Env, code: &str, redirect_uri: &str, verifier: &str, note: &str) -> Resp {
    call(
        &env.c,
        env.c.post(env.as_s.url("/token")).form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", CLIENT_ID),
            ("code_verifier", verifier),
        ]),
        note,
    )
    .await
}

#[tokio::main]
async fn main() {
    // RFC 7636 appendix B test vector
    assert_eq!(
        pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
    let clock = Arc::new(AtomicU64::new(1_000));
    let c = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let asv = Arc::new(As {
        clock: clock.clone(),
        st: Mutex::new(AsState {
            rng: 0x2545F4914F6CDD1D,
            ..Default::default()
        }),
    });
    let as_s = Srv::start_named(
        "as.test",
        Router::new()
            .route("/authorize", get(authorize))
            .route("/consent", get(consent_page).post(consent_submit))
            .route("/token", post(token))
            .route("/introspect", post(introspect))
            .with_state(asv.clone()),
    )
    .await;
    let rs = Arc::new(Rs {
        c: c.clone(),
        as_url: as_s.url(""),
        notes: Mutex::new(vec!["buy milk".into()]),
        introspections: AtomicU64::new(0),
    });
    let rs_s = Srv::start_named(
        "rs.test",
        Router::new()
            .route("/api/notes", get(notes_get).post(notes_post))
            .with_state(rs.clone()),
    )
    .await;
    let ca = Arc::new(Ca {
        c: c.clone(),
        as_url: as_s.url(""),
        rs_url: rs_s.url(""),
        me: RwLock::new(String::new()),
        sessions: Mutex::new(HashMap::new()),
        rng: Mutex::new(0x1234_5678_9ABC_DEF1),
        n: AtomicU64::new(0),
    });
    let ca_s = Srv::start_named(
        "app.test",
        Router::new()
            .route("/start", get(ca_start))
            .route("/callback", get(ca_callback))
            .route("/notes", get(ca_notes_get).post(ca_notes_post))
            .with_state(ca.clone()),
    )
    .await;
    *ca.me.write().unwrap() = ca_s.url("");
    let cb = ca_s.url("/callback");
    let cb2 = ca_s.url("/callback-alt");
    asv.st.lock().unwrap().clients.insert(
        CLIENT_ID.into(),
        ClientCfg {
            redirect_uris: vec![cb.clone(), cb2.clone()],
            scopes: vec!["notes:read".into(), "notes:write".into()],
        },
    );
    let env = Env {
        c,
        as_s,
        rs_s,
        ca_s,
        asv,
        rs,
        ca,
        clock,
    };

    println!("==== 1. the happy path: full redirect chain ====");
    let (done, cookie) = app_login(&env, "notes:read").await;
    assert_eq!(
        (
            done.status,
            done.json()["logged_in"].clone(),
            done.json()["scope"].clone()
        ),
        (200, json!(true), json!("notes:read"))
    );
    let toks = env.ca.sessions.lock().unwrap()[&cookie]
        .tokens
        .clone()
        .unwrap();
    let (at1, rt1) = (
        toks["access_token"].as_str().unwrap().to_string(),
        toks["refresh_token"].as_str().unwrap().to_string(),
    );
    assert!(at1.starts_with("at_") && rt1.starts_with("rt_") && toks["expires_in"] == 600);
    let r = call(
        &env.c,
        env.c
            .get(env.ca_s.url("/notes"))
            .header("cookie", format!("sid={cookie}")),
        "browser asks the app for notes (app calls the API with the token)",
    )
    .await;
    assert_eq!(
        (r.status, r.json()["notes"].clone()),
        (200, json!(["buy milk"]))
    );

    println!("\n==== 2. scope enforcement at the resource server ====");
    let r = call(
        &env.c,
        env.c
            .post(env.ca_s.url("/notes"))
            .header("cookie", format!("sid={cookie}"))
            .body("pay rent"),
        "write attempt with a read-only token",
    )
    .await;
    assert_eq!(r.status, 403);
    assert_eq!(
        r.h("www-authenticate"),
        "Bearer realm=\"notes\", error=\"insufficient_scope\", scope=\"notes:write\""
    );
    let r = call(
        &env.c,
        env.c.get(env.rs_s.url("/api/notes")),
        "no token at all",
    )
    .await;
    assert_eq!(
        (r.status, r.h("www-authenticate").as_str()),
        (401, "Bearer realm=\"notes\"")
    );
    let r = call(
        &env.c,
        env.c
            .get(env.rs_s.url("/api/notes"))
            .bearer_auth("at_garbage"),
        "garbage token",
    )
    .await;
    assert_eq!(r.status, 401);
    assert!(r.h("www-authenticate").contains("error=\"invalid_token\""));
    println!("\n--- a second login that consents to notes:write ---");
    quiet(true);
    let (_, ck2) = app_login(&env, "notes:read notes:write").await;
    quiet(false);
    let r = call(
        &env.c,
        env.c
            .post(env.ca_s.url("/notes"))
            .header("cookie", format!("sid={ck2}"))
            .body("pay rent"),
        "write with a notes:write token",
    )
    .await;
    assert_eq!(r.status, 201);
    assert_eq!(env.rs.notes.lock().unwrap().len(), 2);

    println!("\n==== 3. access token expiry, refresh token ROTATION ====");
    env.clock.fetch_add(AT_TTL + 1, SeqCst);
    let r = call(
        &env.c,
        env.c
            .get(env.ca_s.url("/notes"))
            .header("cookie", format!("sid={cookie}")),
        "10+ minutes later: the app transparently refreshes",
    )
    .await;
    assert_eq!(r.status, 200);
    let toks2 = env.ca.sessions.lock().unwrap()[&cookie]
        .tokens
        .clone()
        .unwrap();
    let (at2, rt2) = (
        toks2["access_token"].as_str().unwrap().to_string(),
        toks2["refresh_token"].as_str().unwrap().to_string(),
    );
    assert!(at2 != at1 && rt2 != rt1, "both tokens rotate");

    println!("\n==== 4. refresh token REUSE detection: the whole family is revoked ====");
    let refresh = |rt: String| {
        let env = &env;
        async move {
            call(
                &env.c,
                env.c.post(env.as_s.url("/token")).form(&[
                    ("grant_type", "refresh_token"),
                    ("refresh_token", rt.as_str()),
                    ("client_id", CLIENT_ID),
                ]),
                "attacker replays the OLD refresh token rt1",
            )
            .await
        }
    };
    let r = refresh(rt1.clone()).await;
    assert_eq!(
        (r.status, r.json()["error"].as_str()),
        (400, Some("invalid_grant"))
    );
    assert!(
        r.json()["error_description"]
            .as_str()
            .unwrap()
            .contains("reuse detected")
    );
    let r = call(
        &env.c,
        env.c
            .get(env.ca_s.url("/notes"))
            .header("cookie", format!("sid={cookie}")),
        "the legitimate session is now dead too (access token revoked, rt2 revoked)",
    )
    .await;
    assert_eq!(
        (r.status, r.json()["error"].as_str()),
        (401, Some("reauthentication_required"))
    );
    // the other login (another family) is unaffected
    let r = call(
        &env.c,
        env.c
            .get(env.ca_s.url("/notes"))
            .header("cookie", format!("sid={ck2}")),
        "a different token family is unaffected",
    )
    .await;
    assert_eq!(r.status, 200);
    let _ = (at2, rt2);

    println!("\n==== 5. attack: authorization code interception (no verifier) ====");
    let verifier = "V".repeat(43);
    let loc = direct_authorize(&env, &verifier, "st-int", "notes:read", &cb).await;
    println!(
        "   (the victim's redirect, intercepted by malware / a malicious app registered for the scheme):\n   {}",
        mask(&loc)
    );
    let stolen = qp(&loc, "code").unwrap();
    assert_eq!(qp(&loc, "state").as_deref(), Some("st-int"));
    let r = exchange(
        &env,
        &stolen,
        &cb,
        &"A".repeat(43),
        "attacker exchanges the stolen code with his own verifier",
    )
    .await;
    assert_eq!(
        (r.status, r.json()["error"].as_str()),
        (400, Some("invalid_grant"))
    );
    assert!(
        r.json()["error_description"]
            .as_str()
            .unwrap()
            .contains("PKCE")
    );
    let r = exchange(&env, &stolen, &cb, "", "attacker without any verifier").await;
    assert_eq!(r.status, 400);
    let r = exchange(
        &env,
        &stolen,
        &cb,
        &verifier,
        "the legitimate client (knows the verifier) still succeeds",
    )
    .await;
    assert_eq!(r.status, 200);
    let victim_at = r.json()["access_token"].as_str().unwrap().to_string();

    println!("\n==== 6. attack: authorization code REPLAY ====");
    let r = exchange(
        &env,
        &stolen,
        &cb,
        &verifier,
        "replaying the same code (even with the right verifier)",
    )
    .await;
    assert_eq!(
        (r.status, r.json()["error"].as_str()),
        (400, Some("invalid_grant"))
    );
    let r = call(
        &env.c,
        env.c
            .get(env.rs_s.url("/api/notes"))
            .bearer_auth(&victim_at),
        "tokens minted from a replayed code are revoked",
    )
    .await;
    assert_eq!(r.status, 401);

    println!("\n==== 7. attack: redirect_uri tampering ====");
    let evil = "http://evil.example/steal";
    let url = format!(
        "{}/authorize?response_type=code&client_id={CLIENT_ID}&redirect_uri={}&scope=notes:read&state=x&code_challenge={}&code_challenge_method=S256",
        env.as_s.url(""),
        pct(evil),
        pct(&pkce_challenge(&verifier))
    );
    let r = call(
        &env.c,
        env.c.get(url),
        "authorize with an unregistered redirect_uri: shown an error, NOT redirected",
    )
    .await;
    assert_eq!((r.status, r.h("location").as_str()), (400, ""));
    let loc = direct_authorize(&env, &verifier, "st-ru", "notes:read", &cb).await;
    let r = exchange(
        &env,
        &qp(&loc, "code").unwrap(),
        &cb2,
        &verifier,
        "code issued for /callback, redeemed with another (registered!) redirect_uri",
    )
    .await;
    assert_eq!(
        (r.status, r.json()["error"].as_str()),
        (400, Some("invalid_grant"))
    );
    assert!(
        r.json()["error_description"]
            .as_str()
            .unwrap()
            .contains("redirect_uri")
    );
    let loc = direct_authorize(&env, &verifier, "st-ru2", "notes:read", &cb).await;
    let r = exchange(
        &env,
        &qp(&loc, "code").unwrap(),
        &format!("{cb}/"),
        &verifier,
        "trailing slash: matching is EXACT string comparison",
    )
    .await;
    assert_eq!(r.status, 400);

    println!("\n==== 8. attack: state mismatch (login CSRF) ====");
    // the attacker completes a flow with HIS account and hands the callback URL to the victim
    let mut attacker_ck = None;
    let (_, last_a) = browse(
        &env,
        format!("{}?scope=notes%3Aread", env.ca_s.url("/start")),
        &mut attacker_ck,
        "",
    )
    .await;
    quiet(true);
    let atk = submit_consent(&env, &req_id(&last_a), "allow", "").await;
    quiet(false);
    let attacker_callback = atk.h("location");
    let mut victim_ck = None;
    let (_, _) = browse(
        &env,
        format!("{}?scope=notes%3Aread", env.ca_s.url("/start")),
        &mut victim_ck,
        "/consent",
    )
    .await;
    let r = call(
        &env.c,
        env.c
            .get(&attacker_callback)
            .header("cookie", format!("sid={}", victim_ck.unwrap())),
        "victim's browser (own session cookie) is made to load the ATTACKER's callback URL",
    )
    .await;
    assert_eq!(r.status, 400);
    assert!(
        r.json()["error"]
            .as_str()
            .unwrap()
            .contains("state mismatch")
    );
    let r = call(
        &env.c,
        env.c.get(&attacker_callback),
        "same, but with no session cookie at all",
    )
    .await;
    assert_eq!(r.status, 400);

    println!("\n==== 9. other protocol checks ====");
    let url = format!(
        "{}/authorize?response_type=code&client_id={CLIENT_ID}&redirect_uri={}&scope=notes:read&state=s9",
        env.as_s.url(""),
        pct(&cb)
    );
    let r = call(
        &env.c,
        env.c.get(url),
        "PKCE downgrade (no code_challenge): error is delivered to the redirect_uri",
    )
    .await;
    assert_eq!(r.status, 302);
    assert!(
        r.h("location").contains("error=invalid_request") && r.h("location").contains("state=s9")
    );
    let loc = direct_authorize(&env, &verifier, "st-exp", "notes:read", &cb).await;
    env.clock.fetch_add(CODE_TTL + 1, SeqCst);
    let r = exchange(
        &env,
        &qp(&loc, "code").unwrap(),
        &cb,
        &verifier,
        "code redeemed after its 60s lifetime",
    )
    .await;
    assert!(
        r.json()["error_description"]
            .as_str()
            .unwrap()
            .contains("expired")
    );
    let url = format!(
        "{}/authorize?response_type=code&client_id={CLIENT_ID}&redirect_uri={}&scope=admin&state=s9&code_challenge={}&code_challenge_method=S256",
        env.as_s.url(""),
        pct(&cb),
        pct(&pkce_challenge(&verifier))
    );
    let r = call(&env.c, env.c.get(url), "scope the client may not request").await;
    assert!(r.h("location").contains("error=invalid_scope"));
    let mut ck = None;
    let (consent, last) = browse(
        &env,
        format!("{}?scope=notes%3Aread", env.ca_s.url("/start")),
        &mut ck,
        "",
    )
    .await;
    assert_eq!(consent.status, 200);
    let deny = submit_consent(&env, &req_id(&last), "deny", "user clicks Deny").await;
    let (r, _) = browse(&env, deny.h("location"), &mut ck, "").await;
    assert_eq!(
        (r.status, r.json()["error"].as_str()),
        (403, Some("access_denied"))
    );

    println!(
        "\nintrospection calls made by the resource server: {}",
        env.rs.introspections.load(SeqCst)
    );
    env.ca_s.stop().await;
    env.rs_s.stop().await;
    env.as_s.stop().await;
    println!("OK: all OAuth 2.0 + PKCE assertions passed");
}
