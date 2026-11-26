//! RECIPE 27 (axum): routes, shared state, JSON in/out, path params, and testing a router WITHOUT a socket.
//! LEVEL: intermediate
//! Use in: REST API stage.   Run: cargo run --release --example axum_router_state
//! COMMON USES: every REST service in Rust: routes, shared state, JSON extractors; handlers tested without sockets.
//!
//! Pattern: `Router::new().route(path, method(handler)).with_state(state)`. Handlers are plain async fns
//! whose arguments are *extractors* (`State`, `Path`, `Json`). Because a Router is a tower `Service`, tests
//! call `router.oneshot(request)` in-process: fast, no ports, deterministic.
//! Docs: https://docs.rs/axum/latest/axum/  (path params use `{id}` syntax in axum 0.8)
#![allow(dead_code)]
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::{Request, StatusCode},
    routing::{get, post},
};
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tower::ServiceExt; // for `oneshot`

#[derive(Clone, Serialize, Deserialize)]
struct Workflow {
    name: String,
}

#[derive(Serialize)]
struct Created {
    id: u32,
}

#[derive(Clone, Default)]
struct AppState {
    inner: Arc<Mutex<(u32, HashMap<u32, Workflow>)>>, // (next id, items)
}

async fn create(
    State(st): State<AppState>,
    Json(w): Json<Workflow>,
) -> (StatusCode, Json<Created>) {
    let mut g = st.inner.lock().unwrap();
    g.0 += 1;
    let id = g.0;
    g.1.insert(id, w);
    (StatusCode::CREATED, Json(Created { id }))
}

async fn get_one(
    State(st): State<AppState>,
    Path(id): Path<u32>,
) -> Result<Json<Workflow>, StatusCode> {
    st.inner
        .lock()
        .unwrap()
        .1
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

fn app() -> Router {
    Router::new()
        .route("/v1/workflows", post(create))
        .route("/v1/workflows/{id}", get(get_one))
        .with_state(AppState::default())
}

async fn call(app: &Router, method: &str, uri: &str, body: &str) -> (StatusCode, String) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::main]
async fn main() {
    let app = app();
    let (s, b) = call(&app, "POST", "/v1/workflows", r#"{"name":"video"}"#).await;
    println!("POST create    -> {s} {b}");
    assert_eq!(s, StatusCode::CREATED);
    let (s, b) = call(&app, "GET", "/v1/workflows/1", "").await;
    println!("GET existing   -> {s} {b}");
    assert_eq!(s, StatusCode::OK);
    let (s, _) = call(&app, "GET", "/v1/workflows/99", "").await;
    println!("GET missing    -> {s}");
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, b) = call(&app, "POST", "/v1/workflows", "{not json").await;
    println!("POST bad json  -> {s} {b}");
    let (s, b) = call(&app, "POST", "/v1/workflows", r#"{"nam":"typo"}"#).await;
    println!("POST wrong shape -> {s} {b}");
}
