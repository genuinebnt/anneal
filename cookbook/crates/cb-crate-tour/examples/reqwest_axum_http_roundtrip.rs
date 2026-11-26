//! RECIPE 1115 (crates): axum server and reqwest client in one process: extractors, errors, timeouts, redirects, retries, streaming, multipart, cookies
//! LEVEL: intermediate
//! Use in: HTTP APIs and their clients, and especially their integration tests.   Run: cargo run --release -p cb-crate-tour --example reqwest_axum_http_roundtrip
//! COMMON USES: JSON services, webhooks, uploads and downloads, testing a client against a real server on a random port.
//! METHODS: axum::Router::route, Router::with_state, axum::serve, Serve::with_graceful_shutdown, axum::extract::{State, Path, Query, Multipart, ConnectInfo, DefaultBodyLimit}, axum::Json, axum::response::{IntoResponse, Redirect}, axum::http::{StatusCode, HeaderMap, header}, axum::body::{Body, Bytes}, Body::from_stream, reqwest::Client::builder, ClientBuilder::{timeout, redirect, cookie_store, default_headers, user_agent, no_proxy}, RequestBuilder::{query, json, header, bearer_auth, multipart, timeout, send, body}, Response::{status, headers, json, text, bytes, bytes_stream, error_for_status, url, content_length}, reqwest::redirect::Policy::{none, limited}, reqwest::multipart::{Form, Part}, reqwest::Body::wrap_stream, reqwest::Error::{is_timeout, is_connect, is_status, is_redirect, is_decode, status, url, without_url}, RequestBuilder::{form, basic_auth, try_clone}, Response::chunk, axum::Form, axum::extract::{FromRequestParts, rejection::JsonRejection}, axum::middleware::{from_fn, from_fn_with_state, Next}, Router::{layer, route_layer, nest, merge, fallback}, tower::ServiceExt::oneshot, axum::body::to_bytes
//! DOCS: https://docs.rs/axum/0.8/axum/ https://docs.rs/reqwest/0.12/reqwest/
//! CRATES: axum = "0.8" (multipart), reqwest = "0.12" (no default features; json, stream, multipart, cookies; so NO TLS here, plain http on loopback), tokio = "1", futures-util = "0.3", tower = "0.5" (util)
//!
//! CHEAT SHEET
//!   reuse ONE Client                        -> it holds the connection pool; creating one per request wastes handshakes
//!   4xx/5xx are not Err                     -> call error_for_status() or match on status()
//!   always set a timeout                    -> reqwest has no overall timeout by default
//!   path syntax in axum 0.8                 -> "/items/{id}" (older versions used :id)
//!   extractor order                         -> the one that consumes the BODY (Json, Multipart, Bytes) comes last
//!   retry                                   -> only idempotent requests (or with an idempotency key), with a budget and backoff
//!   test server                             -> bind 127.0.0.1:0 and read local_addr()
//!   test routes without any socket          -> router.oneshot(request)   (tower::ServiceExt)
//!   middleware                              -> from_fn ; `layer` wraps only routes added BEFORE it; the last layer runs first
//!   auth for matched routes only            -> route_layer (unknown paths stay 404 instead of becoming 401)
//!   your own error body for bad JSON        -> take Result<Json<T>, JsonRejection>
//!   typed data from headers                 -> impl FromRequestParts
//!   URL may carry a token                   -> error.without_url() before logging
//!
//! Read: https://docs.rs/axum/latest/axum/extract/index.html ; https://docs.rs/axum/latest/axum/error_handling/index.html ; see also examples/web/http_client_resilience.rs
//!
//! TRY NEXT:
//!   - [S] Put Json before State in a handler signature and read the compile error (try #[axum::debug_handler] docs).
//!   - [M] Add exponential backoff with jitter and honour Retry-After.
//!   - [A] PROJECT: add the sqlx repository behind these routes with one in-memory database per test.
#![allow(dead_code)]
use axum::body::{Body, Bytes};
use axum::extract::{ConnectInfo, DefaultBodyLimit, Multipart, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Default)]
struct App {
    items: Arc<Mutex<BTreeMap<u64, Item>>>,
    flaky_calls: Arc<AtomicU32>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Item {
    id: u64,
    name: String,
}
#[derive(Deserialize)]
struct NewItem {
    name: String,
}
#[derive(Deserialize)]
struct Filter {
    #[serde(default)]
    prefix: String,
    limit: Option<usize>,
}

/// One error type for all handlers; its IntoResponse impl is the only place that picks status codes.
#[derive(Debug)]
enum ApiError {
    NotFound,
    Invalid(&'static str),
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::Invalid(c) => (StatusCode::UNPROCESSABLE_ENTITY, c),
        };
        (status, Json(json!({ "error": code }))).into_response()
    }
}

async fn create(
    State(app): State<App>,
    Json(new): Json<NewItem>,
) -> Result<impl IntoResponse, ApiError> {
    if new.name.trim().is_empty() {
        return Err(ApiError::Invalid("empty_name"));
    }
    let mut items = app.items.lock().unwrap(); // std Mutex: no .await while it is held
    let id = items.last_key_value().map_or(1, |(k, _)| k + 1);
    let item = Item { id, name: new.name };
    items.insert(id, item.clone());
    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, format!("/items/{id}"))],
        Json(item),
    ))
}
async fn fetch(State(app): State<App>, Path(id): Path<u64>) -> Result<Json<Item>, ApiError> {
    app.items
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(ApiError::NotFound)
}
async fn list(State(app): State<App>, Query(f): Query<Filter>) -> Json<Vec<Item>> {
    let items = app.items.lock().unwrap();
    Json(
        items
            .values()
            .filter(|i| i.name.starts_with(&f.prefix))
            .take(f.limit.unwrap_or(usize::MAX))
            .cloned()
            .collect(),
    )
}
async fn echo_headers(headers: HeaderMap) -> Json<BTreeMap<String, String>> {
    Json(
        headers
            .iter()
            .filter_map(|(k, v)| Some((k.to_string(), v.to_str().ok()?.to_string())))
            .collect(),
    )
}
async fn slow() -> &'static str {
    tokio::time::sleep(Duration::from_secs(30)).await;
    "too late"
}
/// Fails twice, then succeeds.
async fn flaky(State(app): State<App>) -> Response {
    let n = app.flaky_calls.fetch_add(1, Ordering::SeqCst);
    if n < 2 {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::RETRY_AFTER, "0")],
        )
            .into_response()
    } else {
        "recovered".into_response()
    }
}
async fn download() -> Body {
    let chunks = (0..100).map(|i| Ok::<_, std::io::Error>(Bytes::from(vec![i as u8; 1000])));
    Body::from_stream(futures_util::stream::iter(chunks))
}
async fn upload(mut form: Multipart) -> Result<Json<Value>, ApiError> {
    let mut parts = vec![];
    while let Some(field) = form
        .next_field()
        .await
        .map_err(|_| ApiError::Invalid("bad_multipart"))?
    {
        let name = field.name().unwrap_or("").to_string();
        let file = field.file_name().map(str::to_string);
        let content_type = field.content_type().map(str::to_string);
        let data = field
            .bytes()
            .await
            .map_err(|_| ApiError::Invalid("bad_multipart"))?;
        parts.push(json!({"name": name, "file": file, "type": content_type, "len": data.len()}));
    }
    Ok(Json(json!(parts)))
}
async fn echo_body(body: Bytes) -> Bytes {
    body
}
async fn login() -> impl IntoResponse {
    (
        [(header::SET_COOKIE, "session=abc123; Path=/; HttpOnly")],
        "ok",
    )
}
async fn me(headers: HeaderMap) -> Result<String, StatusCode> {
    let cookie = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    cookie
        .contains("session=abc123")
        .then(|| "ann".to_string())
        .ok_or(StatusCode::UNAUTHORIZED)
}
async fn peer(ConnectInfo(addr): ConnectInfo<SocketAddr>) -> String {
    addr.port().to_string()
}

fn router() -> Router {
    Router::new()
        .route("/items", post(create).get(list))
        .route("/items/{id}", get(fetch))
        .route("/headers", get(echo_headers))
        .route("/slow", get(slow))
        .route("/flaky", get(flaky))
        .route("/old", get(|| async { Redirect::temporary("/new") }))
        .route("/new", get(|| async { "new home" }))
        .route("/loop", get(|| async { Redirect::temporary("/loop") }))
        .route("/download", get(download))
        .route("/upload", post(upload))
        .route("/echo", post(echo_body).layer(DefaultBodyLimit::max(1024)))
        .route("/login", post(login))
        .route("/me", get(me))
        .route("/peer", get(peer))
        .route(
            "/form",
            post(|axum::Form(f): axum::Form<NewItem>| async move { f.name }),
        )
        .with_state(App::default())
}

fn client() -> reqwest::ClientBuilder {
    // no_proxy: a proxy variable in the environment must not capture loopback test traffic.
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
}

/// Retry GETs on transport errors and 502/503/504, with a bounded number of attempts and exponential backoff.
async fn get_with_retry(
    c: &reqwest::Client,
    url: &str,
    attempts: u32,
) -> reqwest::Result<(u32, reqwest::Response)> {
    let mut delay = Duration::from_millis(1);
    for attempt in 1.. {
        let outcome = c.get(url).send().await;
        let retryable = match &outcome {
            Ok(r) => matches!(r.status().as_u16(), 502..=504),
            Err(e) => e.is_connect() || e.is_timeout(),
        };
        if !retryable || attempt >= attempts {
            return outcome.map(|r| (attempt, r));
        }
        tokio::time::sleep(delay).await;
        delay *= 2;
    }
    unreachable!()
}

async fn json_paths_queries_errors(c: &reqwest::Client, base: &str) {
    for name in ["apple", "apricot", "banana"] {
        let r = c
            .post(format!("{base}/items"))
            .json(&json!({ "name": name }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), reqwest::StatusCode::CREATED);
        let location = r.headers()["location"].to_str().unwrap().to_string();
        let made: Item = r.json().await.unwrap();
        assert_eq!(location, format!("/items/{}", made.id));
    }
    let one: Item = c
        .get(format!("{base}/items/2"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        one,
        Item {
            id: 2,
            name: "apricot".into()
        }
    );
    let some: Vec<Item> = c
        .get(format!("{base}/items"))
        .query(&[("prefix", "ap"), ("limit", "1")])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(some.len(), 1);

    // A 404 is a successful HTTP exchange: send() returns Ok.
    let r = c.get(format!("{base}/items/99")).send().await.unwrap();
    assert_eq!(r.status(), 404);
    assert_eq!(
        r.json::<Value>().await.unwrap(),
        json!({"error": "not_found"})
    );
    let e = c
        .get(format!("{base}/items/99"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap_err();
    assert!(e.is_status() && e.status() == Some(reqwest::StatusCode::NOT_FOUND));
    // Rejections produced by axum's extractors, before the handler runs.
    let status = async |req: reqwest::RequestBuilder| req.send().await.unwrap().status().as_u16();
    assert_eq!(
        status(c.get(format!("{base}/items/abc"))).await,
        400,
        "path does not parse"
    );
    assert_eq!(
        status(c.get(format!("{base}/items?limit=x"))).await,
        400,
        "query does not parse"
    );
    assert_eq!(
        status(c.post(format!("{base}/items")).body("{}")).await,
        415,
        "missing content-type"
    );
    let as_json = |body: &'static str| {
        c.post(format!("{base}/items"))
            .header("content-type", "application/json")
            .body(body)
    };
    assert_eq!(status(as_json("{")).await, 400, "syntax error");
    assert_eq!(status(as_json("{}")).await, 422, "valid JSON, wrong shape");
    assert_eq!(
        status(as_json(r#"{"name":" "}"#)).await,
        422,
        "our own validation"
    );
    assert_eq!(status(c.delete(format!("{base}/items"))).await, 405);
    assert_eq!(status(c.get(format!("{base}/nope"))).await, 404);
    // Decoding errors are a different error kind from status errors.
    let e = c
        .get(format!("{base}/new"))
        .send()
        .await
        .unwrap()
        .json::<Item>()
        .await
        .unwrap_err();
    assert!(e.is_decode());
}

async fn headers_and_auth(base: &str) {
    let mut defaults = reqwest::header::HeaderMap::new();
    defaults.insert("x-team", "cookbook".parse().unwrap());
    let c = client()
        .default_headers(defaults)
        .user_agent("cookbook-client/1")
        .build()
        .unwrap();
    let seen: BTreeMap<String, String> = c
        .get(format!("{base}/headers"))
        .bearer_auth("t0k")
        .header("X-Request-Id", "r1")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(seen["x-team"], "cookbook");
    assert_eq!(seen["user-agent"], "cookbook-client/1");
    assert_eq!(seen["authorization"], "Bearer t0k");
    assert_eq!(
        seen["x-request-id"], "r1",
        "header names are case-insensitive and sent lower-case"
    );
    assert!(
        "bad\nvalue"
            .parse::<reqwest::header::HeaderValue>()
            .is_err(),
        "header injection is rejected"
    );
}

async fn timeouts_connect_redirects(c: &reqwest::Client, base: &str) {
    // The handler needs 30 s; a 100 ms request timeout must fire.
    let e = c
        .get(format!("{base}/slow"))
        .timeout(Duration::from_millis(100))
        .send()
        .await
        .unwrap_err();
    assert!(e.is_timeout() && !e.is_connect(), "{e:?}");
    // A port nobody listens on: bind to get a free one, then close it.
    let dead = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let e = c.get(format!("http://{dead}/")).send().await.unwrap_err();
    assert!(e.is_connect(), "{e:?}");

    let r = c.get(format!("{base}/old")).send().await.unwrap();
    assert_eq!(
        r.url().path(),
        "/new",
        "followed by default; url() is the FINAL one"
    );
    assert_eq!(r.text().await.unwrap(), "new home");
    let manual = client()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let r = manual.get(format!("{base}/old")).send().await.unwrap();
    assert_eq!(
        (
            r.status().as_u16(),
            r.headers()["location"].to_str().unwrap()
        ),
        (307, "/new")
    );
    let limited = client()
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()
        .unwrap();
    assert!(
        limited
            .get(format!("{base}/loop"))
            .send()
            .await
            .unwrap_err()
            .is_redirect()
    );
}

async fn retries(c: &reqwest::Client, base: &str) {
    let (attempts, r) = get_with_retry(c, &format!("{base}/flaky"), 5)
        .await
        .unwrap();
    assert_eq!((attempts, r.status().as_u16()), (3, 200));
    let (attempts, r) = get_with_retry(c, &format!("{base}/items/99"), 5)
        .await
        .unwrap();
    assert_eq!(
        (attempts, r.status().as_u16()),
        (1, 404),
        "client errors are not retried"
    );
    let dead = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    assert!(
        get_with_retry(c, &format!("http://{dead}/"), 3)
            .await
            .unwrap_err()
            .is_connect(),
        "the budget is bounded"
    );
}

async fn streaming_multipart_limits(c: &reqwest::Client, base: &str) {
    let r = c.get(format!("{base}/download")).send().await.unwrap();
    assert_eq!(
        r.content_length(),
        None,
        "a streamed body has no length up front"
    );
    let mut stream = r.bytes_stream();
    let mut all = vec![];
    while let Some(chunk) = stream.next().await {
        all.extend_from_slice(&chunk.unwrap()); // network chunk boundaries need not match server chunks
    }
    assert_eq!(all.len(), 100_000);
    assert!(
        all.chunks(1000)
            .enumerate()
            .all(|(i, c)| c.iter().all(|&b| b == i as u8))
    );
    // Streaming request body.
    let body = reqwest::Body::wrap_stream(futures_util::stream::iter(
        ["ab", "cd"].map(Ok::<_, std::io::Error>),
    ));
    assert_eq!(
        c.post(format!("{base}/echo"))
            .body(body)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "abcd"
    );

    let form = reqwest::multipart::Form::new()
        .text("title", "report")
        .part(
            "file",
            reqwest::multipart::Part::bytes(vec![1u8; 300])
                .file_name("r.bin")
                .mime_str("application/octet-stream")
                .unwrap(),
        );
    let got: Value = c
        .post(format!("{base}/upload"))
        .multipart(form)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        got,
        json!([{"name":"title","file":null,"type":null,"len":6}, {"name":"file","file":"r.bin","type":"application/octet-stream","len":300}])
    );
    // Server-side body limit (the default for body extractors is 2 MB; this route sets 1 KiB).
    assert_eq!(
        c.post(format!("{base}/echo"))
            .body(vec![0u8; 1024])
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        c.post(format!("{base}/echo"))
            .body(vec![0u8; 1025])
            .send()
            .await
            .unwrap()
            .status(),
        413
    );
}

async fn cookies_and_connection_reuse(base: &str) {
    let jar = client().cookie_store(true).build().unwrap();
    let me = |c: reqwest::Client| {
        let url = format!("{base}/me");
        async move { c.get(url).send().await.unwrap() }
    };
    assert_eq!(me(jar.clone()).await.status(), 401);
    jar.post(format!("{base}/login"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    assert_eq!(me(jar.clone()).await.text().await.unwrap(), "ann");
    assert_eq!(
        me(client().build().unwrap()).await.status(),
        401,
        "cookies belong to the client's jar"
    );

    // The server reports the client's TCP port: the same port means the same connection.
    let port = |c: &reqwest::Client| {
        let req = c.get(format!("{base}/peer"));
        async move { req.send().await.unwrap().text().await.unwrap() } // reading the body returns the connection to the pool
    };
    let shared = client().build().unwrap();
    let (a, b) = (port(&shared).await, port(&shared).await);
    assert_eq!(a, b, "one Client reuses the pooled connection");
    let other = client().build().unwrap();
    assert_ne!(port(&other).await, a, "another Client opens its own");
}

// ---------------- composition, middleware and custom extractors, tested without sockets ----------------
/// A typed extractor: handlers that take `Tenant` cannot run without a valid header.
struct Tenant(String);
impl<S: Send + Sync> axum::extract::FromRequestParts<S> for Tenant {
    type Rejection = (StatusCode, Json<Value>);
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _: &S,
    ) -> Result<Self, Self::Rejection> {
        parts
            .headers
            .get("x-tenant")
            .and_then(|v| v.to_str().ok())
            .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_alphanumeric()))
            .map(|v| Tenant(v.to_owned()))
            .ok_or((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "bad_tenant"})),
            ))
    }
}
/// Appends its tag on the way in and on the way out, which makes the layer order visible.
async fn tag(
    State(name): State<&'static str>,
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    req.headers_mut().append("x-trace", name.parse().unwrap());
    let mut res = next.run(req).await;
    res.headers_mut()
        .append("x-trace-out", name.parse().unwrap());
    res
}
async fn trace_of(headers: HeaderMap) -> String {
    headers
        .get_all("x-trace")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .collect::<Vec<_>>()
        .join(",")
}
async fn require_key(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<Response, StatusCode> {
    match req.headers().get("x-key").map(|v| v.as_bytes()) {
        Some(b"k1") => Ok(next.run(req).await),
        _ => Err(StatusCode::UNAUTHORIZED), // returning without next.run short-circuits: the handler never runs
    }
}
/// Replaces axum's plain-text JSON rejections with the API's own error shape.
async fn strict_create(
    body: Result<Json<NewItem>, axum::extract::rejection::JsonRejection>,
) -> Response {
    use axum::extract::rejection::JsonRejection;
    match body {
        Ok(Json(item)) => item.name.into_response(),
        Err(rejection) => {
            let code = match &rejection {
                JsonRejection::JsonSyntaxError(_) => "json_syntax",
                JsonRejection::JsonDataError(_) => "json_shape",
                JsonRejection::MissingJsonContentType(_) => "content_type",
                _ => "body", // the enum is non_exhaustive
            };
            (rejection.status(), Json(json!({ "error": code }))).into_response()
        }
    }
}

async fn composition_without_sockets() {
    use axum::middleware::{from_fn, from_fn_with_state};
    use tower::ServiceExt;
    /// One request through the Router as a plain tower Service: no port, no client, no network.
    async fn call(app: &Router, req: axum::http::Request<Body>) -> (StatusCode, HeaderMap, String) {
        let res = app
            .clone()
            .oneshot(req)
            .await
            .expect("Router's error type is Infallible");
        let (parts, body) = res.into_parts();
        let bytes = axum::body::to_bytes(body, 64 * 1024).await.unwrap(); // always bound the size
        (
            parts.status,
            parts.headers,
            String::from_utf8(bytes.to_vec()).unwrap(),
        )
    }
    let get_req = |path: &str, headers: &[(&str, &str)]| {
        let mut b = axum::http::Request::get(path);
        for (k, v) in headers {
            b = b.header(*k, *v);
        }
        b.body(Body::empty()).unwrap()
    };

    // Layer order and coverage.
    let app = Router::new()
        .route("/early", get(trace_of))
        .layer(from_fn_with_state("first", tag))
        .layer(from_fn_with_state("second", tag))
        .route("/late", get(trace_of));
    let (_, headers, body) = call(&app, get_req("/early", &[])).await;
    assert_eq!(
        body, "second,first",
        "the LAST layer added is outermost and sees the request first"
    );
    let out: Vec<_> = headers
        .get_all("x-trace-out")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    assert_eq!(out, ["first", "second"], "and sees the response last");
    assert_eq!(
        call(&app, get_req("/late", &[])).await.2,
        "",
        "PITFALL: a route added after .layer() is not wrapped"
    );

    // layer vs route_layer for authentication, plus nest, merge and fallback.
    let admin = Router::new().route("/stats", get(|| async { "stats" }));
    let public = Router::new().route("/health", get(|| async { "ok" }));
    let build = |only_matched: bool| {
        let admin = if only_matched {
            admin.clone().route_layer(from_fn(require_key))
        } else {
            admin.clone().layer(from_fn(require_key))
        };
        Router::new()
            .nest("/admin", admin)
            .merge(public.clone())
            .route("/whoami", get(|Tenant(t): Tenant| async move { t }))
            .route("/strict", post(strict_create))
            .fallback(|uri: axum::http::Uri| async move {
                (
                    StatusCode::NOT_FOUND,
                    format!("no route for {}", uri.path()),
                )
            })
    };
    let app = build(true);
    assert_eq!(call(&app, get_req("/admin/stats", &[])).await.0, 401);
    assert_eq!(
        call(&app, get_req("/admin/stats", &[("x-key", "k1")]))
            .await
            .2,
        "stats"
    );
    assert_eq!(
        call(&app, get_req("/health", &[])).await.2,
        "ok",
        "merged routes are outside the admin layer"
    );
    let (status, _, body) = call(&app, get_req("/admin/nope", &[])).await;
    assert_eq!(
        (status, body.as_str()),
        (StatusCode::NOT_FOUND, "no route for /admin/nope")
    );

    assert_eq!(
        call(&app, get_req("/whoami", &[("x-tenant", "acme")]))
            .await
            .2,
        "acme"
    );
    for headers in [&[][..], &[("x-tenant", "")], &[("x-tenant", "../etc")]] {
        let (status, _, body) = call(&app, get_req("/whoami", headers)).await;
        assert_eq!(
            (status.as_u16(), body.as_str()),
            (400, r#"{"error":"bad_tenant"}"#),
            "{headers:?}"
        );
    }
    for (content_type, body, status, code) in [
        (Some("application/json"), "{", 400, "json_syntax"),
        (Some("application/json"), "{}", 422, "json_shape"),
        (None, "{}", 415, "content_type"),
    ] {
        let mut req = axum::http::Request::post("/strict");
        if let Some(c) = content_type {
            req = req.header("content-type", c);
        }
        let got = call(&app, req.body(Body::from(body)).unwrap()).await;
        assert_eq!(
            (got.0.as_u16(), got.2),
            (status, format!(r#"{{"error":"{code}"}}"#))
        );
    }
}

async fn client_request_details(c: &reqwest::Client, base: &str) {
    // form(): application/x-www-form-urlencoded; axum's Form extractor reads it back.
    let r = c
        .post(format!("{base}/form"))
        .form(&[("name", "a b&c=d")])
        .send()
        .await
        .unwrap();
    assert_eq!(r.text().await.unwrap(), "a b&c=d");
    let seen: BTreeMap<String, String> = c
        .get(format!("{base}/headers"))
        .basic_auth("ann", Some("pw"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(seen["authorization"], "Basic YW5uOnB3");

    // try_clone is what a generic retry layer needs, and it is None for streaming bodies: they can be read only once.
    let url = format!("{base}/echo");
    let buffered = c.post(&url).body("abc");
    let copy = buffered.try_clone().expect("in-memory body");
    for req in [buffered, copy] {
        assert_eq!(req.send().await.unwrap().text().await.unwrap(), "abc");
    }
    let stream = futures_util::stream::iter(["x"].map(Ok::<_, std::io::Error>));
    assert!(
        c.post(&url)
            .body(reqwest::Body::wrap_stream(stream))
            .try_clone()
            .is_none()
    );

    // chunk() reads a large body piece by piece with no Stream trait, which allows enforcing a size cap while reading.
    let mut r = c.get(format!("{base}/download")).send().await.unwrap();
    let (mut total, cap) = (0, 10_000);
    let mut rejected = false;
    while let Some(chunk) = r.chunk().await.unwrap() {
        total += chunk.len();
        if total > cap {
            rejected = true;
            break;
        }
    }
    assert!(rejected, "the 100 kB download exceeds the 10 kB cap");

    // PITFALL: error messages include the full URL, query string and all.
    let e = c
        .get(format!("{base}/items/99?api_key=SECRET123"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap_err();
    assert!(e.to_string().contains("SECRET123"), "{e}");
    assert!(e.url().is_some());
    let safe = e.without_url();
    assert!(
        !safe.to_string().contains("SECRET123") && !format!("{safe:?}").contains("SECRET123"),
        "{safe:?}"
    );
    assert_eq!(
        safe.status(),
        Some(reqwest::StatusCode::NOT_FOUND),
        "the rest is preserved"
    );
}

async fn run() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            router().into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async {
            let _ = stopped.await;
        })
        .await
    });
    {
        let c = client().build().unwrap();
        json_paths_queries_errors(&c, &base).await;
        headers_and_auth(&base).await;
        timeouts_connect_redirects(&c, &base).await;
        retries(&c, &base).await;
        streaming_multipart_limits(&c, &base).await;
        cookies_and_connection_reuse(&base).await;
        client_request_details(&c, &base).await;
        composition_without_sockets().await;
    } // clients dropped here, closing their idle connections
    // Graceful shutdown waits for in-flight requests. The abandoned /slow request was cancelled when its client hung up.
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), server)
        .await
        .expect("server did not stop")
        .unwrap()
        .unwrap();
}

pub fn main() {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(run());
    println!("axum + reqwest: all sections passed");
}
