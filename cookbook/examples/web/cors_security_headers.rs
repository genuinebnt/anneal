//! RECIPE 304 (web): CORS from first principles, security headers, body limits, timeouts and Retry-After
//! LEVEL: intermediate
//! Use in: browser-facing APIs, hardening a service.   Run: cargo run --release --example cors_security_headers
//!
//! WHAT: (1) SIMPLE vs PREFLIGHT cross-origin requests, a hand-written CORS middleware (origin allow-list,
//! credentials with a SPECIFIC origin never `*`, `Vary: Origin`, `Access-Control-Max-Age`) and tower-http's
//! `CorsLayer` configured identically, compared header by header on 10 scenarios against two real servers.
//! (2) A small model of what the BROWSER enforces (it reads the response and decides whether JS may see it)
//! versus what the server does (it executes the request regardless; curl/reqwest are never blocked).
//! (3) Security headers (HSTS, nosniff, CSP, Referrer-Policy, X-Frame-Options) via `SetResponseHeaderLayer`.
//! (4) Request body limits (413 via `RequestBodyLimitLayer`), request timeouts (408 vs 504 via `TimeoutLayer`),
//! and `Retry-After` on 429/503 with a client that honours both its delta-seconds and HTTP-date forms.
//! WHY: CORS is the most misunderstood browser feature: it is NOT access control. It relaxes the same-origin
//! policy for reading responses; a "simple" cross-origin POST still reaches your handler (so CSRF defences
//! are separate), and only non-simple requests are gated by an OPTIONS preflight the server answers.
//! HOW: a request is non-simple if its method is not GET/HEAD/POST, or it has a non-safelisted header
//! (`Authorization`, `X-Api-Key`, ...), or a Content-Type other than form/multipart/text-plain. The preflight
//! carries `Access-Control-Request-Method/Headers`; the answer lists what is allowed and for how long to cache
//! it. Responses that vary by Origin MUST send `Vary: Origin` or a shared cache serves one origin's headers
//! to another. `Access-Control-Allow-Credentials: true` needs an exact origin (tower-http panics on `*`).
//! COMMON USES: SPA front end on another origin calling an API; public read-only APIs (`*` without
//! credentials); locking APIs to known origins; defence-in-depth headers on every response; protecting against
//! oversized or slow requests; polite client back-off with `Retry-After` on rate limits and maintenance windows.
//! Read: https://developer.mozilla.org/en-US/docs/Web/HTTP/CORS , https://developer.mozilla.org/en-US/docs/Web/HTTP ,
//! https://datatracker.ietf.org/doc/html/rfc9110
#![allow(dead_code)]
use axum::{
    Json, Router,
    extract::{Request, State},
    http::{HeaderName, HeaderValue, Method, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::json;
use std::{
    collections::BTreeSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
};
use tower_http::{
    cors::{AllowOrigin, Any, CorsLayer},
    limit::RequestBodyLimitLayer,
    set_header::SetResponseHeaderLayer,
    timeout::TimeoutLayer,
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

const APP_ORIGIN: &str = "https://app.example.com";
const ADMIN_ORIGIN: &str = "https://admin.example.com";
const EVIL: &str = "https://evil.example";
const T0: u64 = 1_700_000_000;

// ---------------------------------------------------------------- shared inner API
struct App {
    writes: AtomicU32,
    now: Arc<AtomicU64>,
    window: Mutex<(u64, u32)>, // (window start, requests in window)
}

type St = State<Arc<App>>;

async fn data_get() -> impl IntoResponse {
    (
        [("x-request-id", "req-1")],
        Json(json!({ "items": [1, 2, 3] })),
    )
}

async fn data_write(State(app): St, body: String) -> impl IntoResponse {
    app.writes.fetch_add(1, Ordering::SeqCst);
    (StatusCode::CREATED, format!("stored {} bytes", body.len()))
}

fn api(app: Arc<App>) -> Router {
    Router::new()
        .route("/api/data", get(data_get).post(data_write).put(data_write))
        .with_state(app)
}

fn new_app() -> Arc<App> {
    Arc::new(App {
        writes: AtomicU32::new(0),
        now: Arc::new(AtomicU64::new(T0)),
        window: Mutex::new((T0, 0)),
    })
}

// ---------------------------------------------------------------- hand-written CORS
struct CorsCfg {
    origins: Vec<&'static str>,
    methods: Vec<&'static str>,
    headers: Vec<&'static str>,
    credentials: bool,
    max_age: u64,
    expose: Vec<&'static str>,
}

fn cfg() -> CorsCfg {
    CorsCfg {
        origins: vec![APP_ORIGIN, ADMIN_ORIGIN],
        methods: vec!["GET", "POST", "PUT"],
        headers: vec!["content-type", "x-api-key"],
        credentials: true,
        max_age: 600,
        expose: vec!["x-request-id"],
    }
}

fn hv(s: &str) -> HeaderValue {
    HeaderValue::from_str(s).unwrap()
}

async fn cors_mw(State(cfg): State<Arc<CorsCfg>>, req: Request, next: Next) -> Response {
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let preflight = req.method() == Method::OPTIONS
        && req
            .headers()
            .contains_key(header::ACCESS_CONTROL_REQUEST_METHOD);
    // Exact string match against the allow-list; "null" and look-alike origins never match.
    let allowed = origin.filter(|o| cfg.origins.contains(&o.as_str()));
    let mut resp = if preflight {
        StatusCode::OK.into_response()
    } else {
        next.run(req).await
    };
    let h = resp.headers_mut();
    if let Some(o) = &allowed {
        h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, hv(o)); // echo the one origin, never `*` with credentials
    }
    // Like tower-http, the remaining headers are sent regardless; without a matching ACAO the browser ignores them.
    if cfg.credentials {
        h.insert(header::ACCESS_CONTROL_ALLOW_CREDENTIALS, hv("true"));
    }
    if !preflight && !cfg.expose.is_empty() {
        h.insert(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            hv(&cfg.expose.join(",")),
        );
    }
    if preflight {
        h.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            hv(&cfg.methods.join(",")),
        );
        h.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            hv(&cfg.headers.join(",")),
        );
        h.insert(header::ACCESS_CONTROL_MAX_AGE, hv(&cfg.max_age.to_string()));
    }
    // The response depends on Origin (and, for preflights, on the requested method/headers): caches must key on them.
    for v in [
        "origin",
        "access-control-request-method",
        "access-control-request-headers",
    ] {
        h.append(header::VARY, hv(v));
    }
    resp
}

fn tower_cors() -> CorsLayer {
    let c = cfg();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(c.origins.iter().map(|o| hv(o))))
        .allow_methods(
            c.methods
                .iter()
                .map(|m| Method::from_bytes(m.as_bytes()).unwrap())
                .collect::<Vec<_>>(),
        )
        .allow_headers(
            c.headers
                .iter()
                .map(|h| HeaderName::from_static(h))
                .collect::<Vec<_>>(),
        )
        .allow_credentials(c.credentials)
        .max_age(Duration::from_secs(c.max_age))
        .expose_headers(
            c.expose
                .iter()
                .map(|h| HeaderName::from_static(h))
                .collect::<Vec<_>>(),
        )
}

// ---------------------------------------------------------------- what the browser enforces
const SAFELISTED_HEADERS: [&str; 4] = [
    "accept",
    "accept-language",
    "content-language",
    "content-type",
];

/// Does `fetch()` need a preflight? (simplified from the Fetch standard)
fn needs_preflight(method: &str, headers: &[(&str, &str)]) -> bool {
    !["GET", "HEAD", "POST"].contains(&method)
        || headers.iter().any(|(k, v)| {
            let k = k.to_ascii_lowercase();
            !SAFELISTED_HEADERS.contains(&k.as_str())
                || (k == "content-type"
                    && ![
                        "application/x-www-form-urlencoded",
                        "multipart/form-data",
                        "text/plain",
                    ]
                    .contains(&v.split(';').next().unwrap().trim()))
        })
}

/// The check applied to a (final or preflight) response before script may see it.
fn browser_response_check(
    origin: &str,
    with_credentials: bool,
    acao: &str,
    acac: &str,
) -> Result<(), String> {
    if acao.is_empty() {
        return Err("no Access-Control-Allow-Origin header".into());
    }
    if acao == "*" && with_credentials {
        return Err("wildcard origin is not allowed when credentials are included".into());
    }
    if acao != "*" && acao != origin {
        return Err(format!(
            "Access-Control-Allow-Origin {acao:?} does not match {origin:?}"
        ));
    }
    if with_credentials && acac != "true" {
        return Err(
            "credentials included but Access-Control-Allow-Credentials is not 'true'".into(),
        );
    }
    Ok(())
}

fn browser_preflight_check(
    origin: &str,
    method: &str,
    req_headers: &[&str],
    creds: bool,
    r: &Exch,
) -> Result<(), String> {
    if !r.status.is_success() {
        return Err(format!("preflight status {}", r.status.as_u16()));
    }
    browser_response_check(
        origin,
        creds,
        &r.h("access-control-allow-origin"),
        &r.h("access-control-allow-credentials"),
    )?;
    let split = |h: String| {
        h.split(',')
            .map(|s| s.trim().to_string())
            .collect::<Vec<_>>()
    };
    let (methods, headers) = (
        split(r.h("access-control-allow-methods")),
        split(r.h("access-control-allow-headers")),
    );
    if !["GET", "HEAD", "POST"].contains(&method) && !methods.iter().any(|m| m == method) {
        return Err(format!(
            "method {method} not in Access-Control-Allow-Methods"
        ));
    }
    for h in req_headers
        .iter()
        .filter(|h| !SAFELISTED_HEADERS.contains(h))
    {
        if !headers.iter().any(|a| a.eq_ignore_ascii_case(h)) {
            return Err(format!(
                "request header {h} not in Access-Control-Allow-Headers"
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- comparing the two implementations
/// The CORS-relevant projection of a response: status + normalised (name -> sorted token set).
fn projection(e: &Exch) -> (u16, Vec<(String, BTreeSet<String>)>) {
    let mut out = vec![];
    for name in e
        .headers
        .keys()
        .map(|k| k.as_str().to_string())
        .collect::<BTreeSet<_>>()
    {
        if name.starts_with("access-control-") || name == "vary" {
            let toks = e
                .h(&name)
                .split(',')
                .map(|t| t.trim().to_ascii_lowercase())
                .filter(|t| !t.is_empty())
                .collect();
            out.push((name, toks));
        }
    }
    (e.status.as_u16(), out)
}

type Req<'a> = (&'a str, &'a str, &'a [(&'a str, &'a str)]); // method, path, headers

fn build(c: &Client, base: &Server, (m, p, hs): Req) -> RequestBuilder {
    let mut rb = c.request(Method::from_bytes(m.as_bytes()).unwrap(), base.url(p));
    for (k, v) in hs {
        rb = rb.header(*k, *v);
    }
    rb
}

// ---------------------------------------------------------------- hardening (security headers, limits, timeouts, retry)
async fn ok() -> &'static str {
    "ok"
}
async fn echo(body: String) -> String {
    format!("got {} bytes", body.len())
}
async fn slow() -> &'static str {
    tokio::time::sleep(Duration::from_millis(800)).await;
    "too late"
}

/// Fixed window: 3 requests per 10s; the 4th gets 429 + Retry-After (delta-seconds until the window resets).
async fn limited(State(app): St) -> Response {
    let now = app.now.load(Ordering::SeqCst);
    let mut w = app.window.lock().unwrap();
    if now - w.0 >= 10 {
        *w = (now, 0);
    }
    w.1 += 1;
    if w.1 > 3 {
        let wait = 10 - (now - w.0);
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, wait.to_string())],
            "rate limit exceeded",
        )
            .into_response();
    }
    "ok".into_response()
}

/// Maintenance window: 503 with an absolute HTTP-date.
async fn maintenance(State(app): St) -> Response {
    let back = app.now.load(Ordering::SeqCst) + 120;
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [(header::RETRY_AFTER, fmt_http_date(back as i64))],
        "down for maintenance",
    )
        .into_response()
}

fn hardened(app: Arc<App>) -> Router {
    let set = |name: &'static str, value: &'static str| {
        SetResponseHeaderLayer::overriding(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        )
    };
    Router::new()
        .route("/ok", get(ok))
        .route("/echo", post(echo))
        .route("/limited", get(limited))
        .route("/maintenance", get(maintenance))
        .route(
            "/slow-408",
            get(slow).layer(TimeoutLayer::with_status_code(
                StatusCode::REQUEST_TIMEOUT,
                Duration::from_millis(100),
            )),
        )
        .route(
            "/slow-504",
            get(slow).layer(TimeoutLayer::with_status_code(
                StatusCode::GATEWAY_TIMEOUT,
                Duration::from_millis(100),
            )),
        )
        .with_state(app)
        .layer(RequestBodyLimitLayer::new(1024))
        // outermost: applied to every response, including 404/413/408 produced by inner layers
        .layer(set(
            "strict-transport-security",
            "max-age=63072000; includeSubDomains",
        ))
        .layer(set("x-content-type-options", "nosniff"))
        .layer(set(
            "content-security-policy",
            "default-src 'self'; frame-ancestors 'none'",
        ))
        .layer(set("referrer-policy", "strict-origin-when-cross-origin"))
        .layer(set("x-frame-options", "DENY"))
}

/// Retry-After is either delta-seconds or an HTTP-date (RFC 9110 10.2.3).
fn parse_retry_after(v: &str, now: u64) -> Option<u64> {
    if let Ok(secs) = v.trim().parse::<u64>() {
        return Some(secs);
    }
    parse_http_date(v).map(|t| (t as u64).saturating_sub(now))
}

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
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        yoe + era * 400 + (m <= 2) as i64,
        m,
        doy - (153 * mp + 2) / 5 + 1,
    )
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn fmt_http_date(t: i64) -> String {
    let (days, secs) = (t.div_euclid(86400), t.rem_euclid(86400));
    let (y, m, d) = civil_from_days(days);
    let wd = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][(days + 4).rem_euclid(7) as usize];
    format!(
        "{wd}, {d:02} {} {y} {:02}:{:02}:{:02} GMT",
        MONTHS[m as usize - 1],
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

fn parse_http_date(s: &str) -> Option<i64> {
    let p: Vec<&str> = s.split_whitespace().collect();
    let mon = MONTHS.iter().position(|m| p.get(2) == Some(m))? as i64 + 1;
    let hms: Vec<i64> = p
        .get(4)?
        .split(':')
        .filter_map(|x| x.parse().ok())
        .collect();
    Some(
        days_from_civil(p.get(3)?.parse().ok()?, mon, p.get(1)?.parse().ok()?) * 86400
            + hms[0] * 3600
            + hms[1] * 60
            + hms[2],
    )
}

// ---------------------------------------------------------------- main
#[tokio::main]
async fn main() {
    let (hand_app, tower_app) = (new_app(), new_app());
    let hand = Server::start(
        api(hand_app.clone()).layer(middleware::from_fn_with_state(Arc::new(cfg()), cors_mw)),
    )
    .await;
    let tower = Server::start(api(tower_app.clone()).layer(tower_cors())).await;
    let c = client();

    title("simple request vs preflight (transcripts from the hand-written layer)");
    println!(
        "needs preflight? GET plain: {} | POST text/plain: {} | POST json: {} | GET+Authorization: {} | PUT: {}\n",
        needs_preflight("GET", &[]),
        needs_preflight("POST", &[("content-type", "text/plain")]),
        needs_preflight("POST", &[("content-type", "application/json")]),
        needs_preflight("GET", &[("authorization", "x")]),
        needs_preflight("PUT", &[])
    );
    assert!(
        !needs_preflight("GET", &[])
            && !needs_preflight("POST", &[("content-type", "text/plain; charset=utf-8")])
    );
    assert!(
        needs_preflight("POST", &[("content-type", "application/json")])
            && needs_preflight("GET", &[("x-api-key", "k")])
            && needs_preflight("PUT", &[])
            && needs_preflight("DELETE", &[])
    );
    let pf = send(
        &c,
        c.request(Method::OPTIONS, hand.url("/api/data"))
            .header("origin", APP_ORIGIN)
            .header("access-control-request-method", "POST")
            .header("access-control-request-headers", "content-type"),
    )
    .await;
    assert_eq!(pf.status.as_u16(), 200);
    assert_eq!(pf.h("access-control-allow-origin"), APP_ORIGIN);
    assert_eq!(pf.h("access-control-max-age"), "600");
    browser_preflight_check(APP_ORIGIN, "POST", &["content-type"], true, &pf)
        .expect("preflight accepted by the browser model");
    let sim = send(
        &c,
        c.get(hand.url("/api/data")).header("origin", APP_ORIGIN),
    )
    .await;
    assert_eq!(sim.h("access-control-allow-origin"), APP_ORIGIN);
    assert_eq!(sim.h("access-control-allow-credentials"), "true");
    assert_eq!(sim.h("access-control-expose-headers"), "x-request-id");
    assert!(
        sim.h("vary").to_ascii_lowercase().contains("origin"),
        "Vary: Origin"
    );
    assert_ne!(
        sim.h("access-control-allow-origin"),
        "*",
        "credentials => exact origin"
    );

    title("hand-written CORS vs tower-http CorsLayer: header-by-header on 10 scenarios");
    let post_json = [("origin", APP_ORIGIN), ("content-type", "application/json")];
    let scenarios: Vec<(&str, Req)> = vec![
        (
            "1  simple GET, allowed origin",
            ("GET", "/api/data", &[("origin", APP_ORIGIN)]),
        ),
        (
            "2  simple GET, second allowed origin",
            ("GET", "/api/data", &[("origin", ADMIN_ORIGIN)]),
        ),
        (
            "3  simple GET, foreign origin",
            ("GET", "/api/data", &[("origin", EVIL)]),
        ),
        ("4  GET without Origin (curl)", ("GET", "/api/data", &[])),
        (
            "5  preflight POST + content-type",
            (
                "OPTIONS",
                "/api/data",
                &[
                    ("origin", APP_ORIGIN),
                    ("access-control-request-method", "POST"),
                    ("access-control-request-headers", "content-type"),
                ],
            ),
        ),
        (
            "6  preflight DELETE (not allowed)",
            (
                "OPTIONS",
                "/api/data",
                &[
                    ("origin", APP_ORIGIN),
                    ("access-control-request-method", "DELETE"),
                ],
            ),
        ),
        (
            "7  preflight with unlisted header",
            (
                "OPTIONS",
                "/api/data",
                &[
                    ("origin", APP_ORIGIN),
                    ("access-control-request-method", "GET"),
                    ("access-control-request-headers", "x-evil"),
                ],
            ),
        ),
        (
            "8  preflight, foreign origin",
            (
                "OPTIONS",
                "/api/data",
                &[("origin", EVIL), ("access-control-request-method", "POST")],
            ),
        ),
        (
            "9  actual POST after preflight",
            ("POST", "/api/data", &post_json),
        ),
        (
            "10 origin 'null' / scheme mismatch",
            ("GET", "/api/data", &[("origin", "null")]),
        ),
    ];
    println!(
        "{:<40}{:<28}{:<6}{}",
        "scenario", "Access-Control-Allow-Origin", "creds", "identical?"
    );
    for (name, req) in &scenarios {
        let (a, b) = (
            try_quiet(&c, build(&c, &hand, *req)).await,
            try_quiet(&c, build(&c, &tower, *req)).await,
        );
        let (pa, pb) = (projection(&a), projection(&b));
        println!(
            "{name:<40}{:<28}{:<6}{}",
            a.h("access-control-allow-origin"),
            a.h("access-control-allow-credentials"),
            if pa == pb { "yes" } else { "NO" }
        );
        assert_eq!(pa, pb, "scenario {name}:\n hand  = {pa:?}\n tower = {pb:?}");
    }
    let http_origin = send(
        &c,
        c.get(tower.url("/api/data"))
            .header("origin", "http://app.example.com"),
    )
    .await;
    assert!(
        http_origin.h("access-control-allow-origin").is_empty(),
        "scheme is part of the origin"
    );
    println!();
    let t = send(
        &c,
        c.request(Method::OPTIONS, tower.url("/api/data"))
            .header("origin", APP_ORIGIN)
            .header("access-control-request-method", "PUT"),
    )
    .await;
    println!("(above: the tower-http preflight transcript)\n");
    assert_eq!(t.h("access-control-allow-methods"), "GET,POST,PUT");

    title("browser vs server: who enforces what");
    let verdict = |r: Result<(), String>| {
        r.map(|_| "ALLOWED".to_string())
            .unwrap_or_else(|e| format!("BLOCKED ({e})"))
    };
    let ok_resp = send(
        &c,
        c.get(tower.url("/api/data")).header("origin", APP_ORIGIN),
    )
    .await;
    println!(
        "GET from {APP_ORIGIN:<26} -> browser: {}",
        verdict(browser_response_check(
            APP_ORIGIN,
            true,
            &ok_resp.h("access-control-allow-origin"),
            &ok_resp.h("access-control-allow-credentials")
        ))
    );
    let evil_resp = send(&c, c.get(tower.url("/api/data")).header("origin", EVIL)).await;
    let v = browser_response_check(EVIL, false, &evil_resp.h("access-control-allow-origin"), "");
    println!(
        "GET from {EVIL:<26} -> browser: {}   (server still answered {} with a body)",
        verdict(v.clone()),
        evil_resp.status
    );
    assert!(
        v.is_err() && evil_resp.status == 200 && !evil_resp.body.is_empty(),
        "server answered; the browser withholds it from script"
    );
    let del = send(
        &c,
        c.request(Method::OPTIONS, tower.url("/api/data"))
            .header("origin", APP_ORIGIN)
            .header("access-control-request-method", "DELETE"),
    )
    .await;
    let v = browser_preflight_check(APP_ORIGIN, "DELETE", &[], true, &del);
    println!("DELETE preflight -> browser: {}", verdict(v.clone()));
    assert!(v.is_err(), "the actual DELETE would never be sent");
    assert!(
        browser_response_check(APP_ORIGIN, true, "*", "true").is_err(),
        "wildcard + credentials is rejected by browsers"
    );
    assert!(
        browser_response_check(APP_ORIGIN, false, "*", "").is_ok(),
        "wildcard is fine for public, credential-less APIs"
    );
    // The key insight: a "simple" cross-origin POST is EXECUTED even though the origin is not allowed.
    let before = tower_app.writes.load(Ordering::SeqCst);
    let r = send(
        &c,
        c.post(tower.url("/api/data"))
            .header("origin", EVIL)
            .header("content-type", "text/plain")
            .body("transfer=100"),
    )
    .await;
    assert_eq!(r.status.as_u16(), 201);
    assert_eq!(
        tower_app.writes.load(Ordering::SeqCst),
        before + 1,
        "side effect happened: CORS is not CSRF protection"
    );
    println!(
        "simple cross-origin POST from {EVIL}: server executed it (writes {before} -> {}); only reading the reply is blocked. Use CSRF tokens / SameSite.",
        before + 1
    );
    println!("curl/reqwest ignore CORS entirely: they got the body above.\n");

    title("tower-http refuses credentials + wildcard origin");
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let bad = std::panic::catch_unwind(|| {
        let _ = Router::<()>::new()
            .route("/", get(ok))
            .layer(CorsLayer::new().allow_origin(Any).allow_credentials(true));
    });
    std::panic::set_hook(prev);
    println!(
        "CorsLayer::new().allow_origin(Any).allow_credentials(true) panicked: {}\n",
        bad.is_err()
    );
    assert!(bad.is_err());

    let app = new_app();
    let hard = Server::start(hardened(app.clone())).await;

    title("security headers on every response (200, 404, 413)");
    let want = [
        (
            "strict-transport-security",
            "max-age=63072000; includeSubDomains",
        ),
        ("x-content-type-options", "nosniff"),
        (
            "content-security-policy",
            "default-src 'self'; frame-ancestors 'none'",
        ),
        ("referrer-policy", "strict-origin-when-cross-origin"),
        ("x-frame-options", "DENY"),
    ];
    let r = send(&c, c.get(hard.url("/ok"))).await;
    let nf = send(&c, c.get(hard.url("/missing"))).await;
    for e in [&r, &nf] {
        for (k, v) in want {
            assert_eq!(e.h(k), v, "{k} on {}", e.status);
        }
    }
    assert_eq!(nf.status.as_u16(), 404);
    println!(
        "(HSTS is only honoured by browsers over HTTPS; CSP frame-ancestors supersedes X-Frame-Options in modern browsers)\n"
    );

    title("request body limit (1024 bytes): 413 Payload Too Large");
    let r = send(&c, c.post(hard.url("/echo")).body("x".repeat(512))).await;
    assert_eq!(
        (r.status.as_u16(), r.text()),
        (200, "got 512 bytes".to_string())
    );
    let r = send(&c, c.post(hard.url("/echo")).body("x".repeat(4096))).await;
    assert_eq!(
        r.status.as_u16(),
        413,
        "declared Content-Length over the limit: rejected before reading"
    );
    assert_eq!(
        r.h("x-frame-options"),
        "DENY",
        "security headers also decorate the 413"
    );
    let chunks: Vec<Result<Vec<u8>, std::io::Error>> =
        (0..6).map(|_| Ok(vec![b'y'; 512])).collect();
    let r = c
        .post(hard.url("/echo"))
        .body(reqwest::Body::wrap_stream(futures_util::stream::iter(
            chunks,
        )))
        .send()
        .await
        .unwrap();
    println!(
        "streamed 3072-byte chunked upload (no Content-Length) -> {}\n",
        r.status()
    );
    assert_eq!(
        r.status().as_u16(),
        413,
        "no Content-Length: the limit is enforced while reading"
    );

    title("request timeouts: 408 Request Timeout vs 504 Gateway Timeout");
    let started = std::time::Instant::now();
    let r = send(&c, c.get(hard.url("/slow-408"))).await;
    assert_eq!(r.status.as_u16(), 408);
    let r = send(&c, c.get(hard.url("/slow-504"))).await;
    assert_eq!(r.status.as_u16(), 504);
    assert!(
        started.elapsed() < Duration::from_millis(600),
        "handler was cut off at 100ms, not 800ms"
    );
    println!(
        "408 = the server gave up waiting; 504 = a gateway/upstream gave up. Pick the one that tells the client who is at fault.\n"
    );

    title("Retry-After on 429 and 503");
    for i in 1..=3 {
        assert_eq!(
            c.get(hard.url("/limited")).send().await.unwrap().status(),
            200,
            "request {i}"
        );
    }
    app.now.fetch_add(4, Ordering::SeqCst);
    let r = send(&c, c.get(hard.url("/limited"))).await;
    assert_eq!(r.status.as_u16(), 429);
    assert_eq!(r.h("retry-after"), "6", "10s window, 4s elapsed");
    let wait = parse_retry_after(&r.h("retry-after"), T0 + 4).unwrap();
    app.now.fetch_add(wait, Ordering::SeqCst); // a polite client sleeps `wait` seconds (virtual clock here)
    assert_eq!(
        c.get(hard.url("/limited")).send().await.unwrap().status(),
        200,
        "after honouring Retry-After the window reset"
    );
    let m = send(&c, c.get(hard.url("/maintenance"))).await;
    assert_eq!(m.status.as_u16(), 503);
    let date = m.h("retry-after");
    assert!(date.ends_with("GMT"), "HTTP-date form: {date}");
    assert_eq!(
        parse_retry_after(&date, T0 + 10),
        Some(120),
        "HTTP-date (now+120s) converted to a delay of 120s"
    );
    assert_eq!(parse_retry_after("120", T0), Some(120));
    assert_eq!(fmt_http_date(784111777), "Sun, 06 Nov 1994 08:49:37 GMT");

    drop(c);
    hand.shutdown().await;
    tower.shutdown().await;
    hard.shutdown().await;
    println!("all CORS / hardening assertions passed");
}

/// Like `send` but without printing (used for the comparison matrix).
async fn try_quiet(_c: &Client, rb: RequestBuilder) -> Exch {
    let r = rb.send().await.unwrap();
    let (status, headers) = (r.status(), r.headers().clone());
    Exch {
        status,
        headers,
        body: r.bytes().await.unwrap(),
    }
}
