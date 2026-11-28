//! RECIPE 31 (axum): idempotency keys for POST.
//! LEVEL: intermediate
//! Use in: any POST that clients retry ("create this once, however often it is sent").   Run: cargo run --release --example axum_idempotency
//! COMMON USES: payments, order creation, job submission: safe client retries without duplicates (Stripe-style idempotency).
//!
//! Pattern: the client sends `Idempotency-Key: <uuid>`. First time: do the work, remember key -> result,
//! return 201. Retry with the same key: return the SAME result (200), do NOT create a second execution.
//! This is what makes client retries and at-least-once delivery safe. (Real systems also persist the key in
//! the DB in the same transaction as the work, and compare the request body hash.)
//! LIMITS: here the "work" is one counter under the same lock as the key table, which is why check-and-record is atomic. Keys are
//! not tied to a caller, route or body (a reused key silently gets the old result), live in memory only, and are never evicted.
//! An empty key is refused: clients that all send one would otherwise share one result.
//! Background: https://stripe.com/blog/idempotency
#![allow(dead_code)]
use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    routing::post,
};
use http_body_util::BodyExt;
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tower::ServiceExt;

#[derive(Clone, Default)]
struct St {
    inner: Arc<Mutex<(u32, HashMap<String, u32>)>>, // (last execution id, key -> id)
}

async fn start(State(st): State<St>, headers: HeaderMap) -> (StatusCode, Json<serde_json::Value>) {
    let key = headers.get("idempotency-key").and_then(|v| v.to_str().ok());
    let Some(key) = key.filter(|k| !k.is_empty()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"Idempotency-Key header required"})),
        );
    };
    let mut g = st.inner.lock().unwrap();
    if let Some(&id) = g.1.get(key) {
        return (
            StatusCode::OK,
            Json(json!({"execution_id": id, "replayed": true})),
        );
    }
    g.0 += 1;
    let id = g.0;
    g.1.insert(key.to_string(), id);
    (
        StatusCode::CREATED,
        Json(json!({"execution_id": id, "replayed": false})),
    )
}

async fn send(app: &Router, key: Option<&str>) -> (StatusCode, String) {
    let mut b = Request::builder()
        .method("POST")
        .uri("/v1/workflows/video/executions");
    if let Some(k) = key {
        b = b.header("idempotency-key", k);
    }
    let resp = app
        .clone()
        .oneshot(b.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    (
        status,
        String::from_utf8(
            resp.into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap(),
    )
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/v1/workflows/video/executions", post(start))
        .with_state(St::default());
    let a = send(&app, Some("key-1")).await;
    let b = send(&app, Some("key-1")).await; // client retry
    let c = send(&app, Some("key-2")).await;
    let d = send(&app, None).await;
    for (n, r) in [
        ("first", &a),
        ("retry", &b),
        ("new key", &c),
        ("no key", &d),
    ] {
        println!("{n:>8}: {} {}", r.0, r.1);
    }
    assert_eq!(
        (a.0, b.0, c.0, d.0),
        (
            StatusCode::CREATED,
            StatusCode::OK,
            StatusCode::CREATED,
            StatusCode::BAD_REQUEST
        )
    );
    assert!(a.1.contains(r#""execution_id":1"#) && b.1.contains(r#""execution_id":1"#));
    assert!(c.1.contains(r#""execution_id":2"#));
    let empty = send(&app, Some("")).await;
    assert_eq!(empty.0, StatusCode::BAD_REQUEST, "an empty key is no key");

    // 64 copies of one request at the same time, on every worker thread: exactly one creates.
    let mut copies = tokio::task::JoinSet::new();
    for _ in 0..64 {
        let app = app.clone();
        copies.spawn(async move { send(&app, Some("key-3")).await });
    }
    let (mut created, mut ids) = (0, std::collections::HashSet::new());
    while let Some(r) = copies.join_next().await {
        let (status, body) = r.unwrap();
        created += usize::from(status == StatusCode::CREATED);
        ids.insert(body.replace(r#","replayed":true"#, "").replace(r#","replayed":false"#, ""));
    }
    assert_eq!((created, ids.len()), (1, 1), "racing copies of one key create once");
    assert!(ids.contains(r#"{"execution_id":3}"#), "{ids:?}");
}
