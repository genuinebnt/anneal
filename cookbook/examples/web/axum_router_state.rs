//! RECIPE 27 (axum): routes, shared state, JSON in/out, path params, and testing a router WITHOUT a socket.
//! LEVEL: intermediate
//! Use in: any JSON service over HTTP.   Run: cargo run --release --example axum_router_state
//! COMMON USES: every REST service in Rust: routes, shared state, JSON extractors; handlers tested without sockets.
//!
//! Pattern: `Router::new().route(path, method(handler)).with_state(state)`. Handlers are plain async fns
//! whose arguments are *extractors* (`State`, `Path`, `Json`). Because a Router is a tower `Service`, tests
//! call `router.oneshot(request)` in-process: fast, no ports, deterministic.
//! LIMITS are part of the router: a body size limit per request, and a cap on how many items are stored.
//! Docs: https://docs.rs/axum/latest/axum/  (path params use `{id}` syntax in axum 0.8)
#![allow(dead_code)]
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Path, State},
    http::{Request, StatusCode, header},
    response::Response,
    routing::{get, post},
};
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tower::ServiceExt; // for `oneshot`

const MAX_BODY: usize = 16 * 1024; // axum's own default is 2 MB
const MAX_WORKFLOWS: usize = 1_000;

#[derive(Clone, Serialize, Deserialize)]
struct Workflow {
    name: String,
}

#[derive(Serialize)]
struct Created {
    id: u32,
}

#[derive(Default)]
struct Store {
    last_id: u32,
    items: HashMap<u32, Workflow>,
}

#[derive(Clone, Default)]
struct AppState {
    store: Arc<Mutex<Store>>,
}

async fn create(
    State(st): State<AppState>,
    Json(w): Json<Workflow>,
) -> Result<(StatusCode, Json<Created>), StatusCode> {
    // a std Mutex is fine here: the guard is never held across an `.await`
    let mut store = st.store.lock().unwrap();
    if store.items.len() >= MAX_WORKFLOWS {
        return Err(StatusCode::INSUFFICIENT_STORAGE);
    }
    // `+= 1` would panic while holding the lock (debug) or wrap and reuse ids (release)
    let id = store
        .last_id
        .checked_add(1)
        .ok_or(StatusCode::INSUFFICIENT_STORAGE)?;
    store.last_id = id;
    store.items.insert(id, w);
    Ok((StatusCode::CREATED, Json(Created { id })))
}

async fn get_one(
    State(st): State<AppState>,
    Path(id): Path<u32>,
) -> Result<Json<Workflow>, StatusCode> {
    st.store
        .lock()
        .unwrap()
        .items
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/v1/workflows", post(create))
        .route("/v1/workflows/{id}", get(get_one))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(state)
}

/// One request through the router, in this process. No port is opened.
async fn send(app: &Router, request: Request<Body>) -> (Response<()>, String) {
    let response = app.clone().oneshot(request).await.unwrap();
    let (parts, body) = response.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    (Response::from_parts(parts, ()), text)
}

async fn call(app: &Router, method: &str, uri: &str, body: &str) -> (StatusCode, String) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let (response, text) = send(app, req).await;
    (response.status(), text)
}

#[tokio::main]
async fn main() {
    let router = app(AppState::default());
    let (s, b) = call(&router, "POST", "/v1/workflows", r#"{"name":"video"}"#).await;
    println!("POST create      -> {s} {b}");
    assert_eq!((s, b.as_str()), (StatusCode::CREATED, r#"{"id":1}"#));

    // what the handlers return, and what axum answers before a handler ever runs
    let huge = format!(r#"{{"name":"{}"}}"#, "x".repeat(MAX_BODY));
    for (label, method, uri, body, want) in [
        ("GET existing", "GET", "/v1/workflows/1", "", 200),
        ("GET missing", "GET", "/v1/workflows/99", "", 404),
        ("id not a number", "GET", "/v1/workflows/abc", "", 400),
        ("unknown path", "GET", "/v2", "", 404),
        ("wrong method", "DELETE", "/v1/workflows/1", "", 405),
        ("bad JSON", "POST", "/v1/workflows", "{not json", 400),
        (
            "wrong shape",
            "POST",
            "/v1/workflows",
            r#"{"nam":"typo"}"#,
            422,
        ),
        ("body too large", "POST", "/v1/workflows", &huge, 413),
    ] {
        let (s, b) = call(&router, method, uri, body).await;
        println!("{label:<16} -> {s} {b}");
        assert_eq!(s.as_u16(), want, "{label}");
    }

    let no_type = Request::post("/v1/workflows")
        .body(Body::from(r#"{"name":"video"}"#))
        .unwrap();
    let (r, b) = send(&router, no_type).await;
    println!("no content-type  -> {} {b}", r.status());
    assert_eq!(r.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let wrong = Request::delete("/v1/workflows/1")
        .body(Body::empty())
        .unwrap();
    let (r, _) = send(&router, wrong).await;
    let allow = r.headers()[header::ALLOW].to_str().unwrap();
    println!("405 sends Allow  -> {allow}");
    assert!(allow.contains("GET"));

    // the store is capped (one item is already in it)
    for n in 2..=MAX_WORKFLOWS + 1 {
        let (s, _) = call(&router, "POST", "/v1/workflows", r#"{"name":"n"}"#).await;
        let want = if n <= MAX_WORKFLOWS { 201 } else { 507 };
        assert_eq!(s.as_u16(), want, "item {n}");
    }

    // ids that have run out are refused, and the service keeps working afterwards
    let state = AppState::default();
    state.store.lock().unwrap().last_id = u32::MAX;
    let router = app(state);
    for _ in 0..2 {
        let (s, _) = call(&router, "POST", "/v1/workflows", r#"{"name":"late"}"#).await;
        assert_eq!(s, StatusCode::INSUFFICIENT_STORAGE);
    }
    println!("all router checks passed");
}
