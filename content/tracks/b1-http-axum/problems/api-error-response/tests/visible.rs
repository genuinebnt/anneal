use solution::*;

use axum::body::{Body, to_bytes};
use axum::http::Request;
use axum::response::IntoResponse;
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

#[allow(dead_code)]
fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
}

#[allow(dead_code)]
async fn parts(response: axum::response::Response) -> (u16, Value) {
    let status = response.status().as_u16();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
    (status, body)
}

/// Sends one request to the router, without a socket.
#[allow(dead_code)]
async fn send(app: &axum::Router, method: &str, path: &str, content_type: Option<&str>, body: &str) -> (u16, Value) {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(ct) = content_type {
        request = request.header("content-type", ct);
    }
    let response = app.clone().oneshot(request.body(Body::from(body.to_string())).unwrap()).await.unwrap();
    parts(response).await
}

const JSON: Option<&str> = Some("application/json");

#[test]
fn create_then_fetch() {
    run(async {
        let app = router(Arc::new(Store::default()));
        check!("POST ada", send(&app, "POST", "/users", JSON, r#"{"name":" ada "}"#).await, (201, json!({"id":1,"name":"ada"})));
        check!("GET /users/1", send(&app, "GET", "/users/1", None, "").await, (200, json!({"id":1,"name":"ada"})));
    });
}

#[test]
fn not_found_shape() {
    run(async {
        let app = router(Arc::new(Store::default()));
        check!("GET /users/7", send(&app, "GET", "/users/7", None, "").await, (404, json!({"error":{"code":"not_found","message":"user 7 not found"}})));
    });
}

#[test]
fn validation_lists_fields() {
    run(async {
        let app = router(Arc::new(Store::default()));
        let want = json!({"error":{"code":"invalid","message":"validation failed","fields":[{"field":"name","problem":"required"}]}});
        check!("blank name", send(&app, "POST", "/users", JSON, r#"{"name":"  "}"#).await, (422, want));
    });
}

#[test]
fn internal_errors_do_not_leak() {
    run(async {
        let store = Arc::new(Store::default());
        store.down.store(true, std::sync::atomic::Ordering::SeqCst);
        let got = send(&router(store), "POST", "/users", JSON, r#"{"name":"ada"}"#).await;
        check!("database down", got, (500, json!({"error":{"code":"internal","message":"internal error"}})));
    });
}

#[test]
fn error_type_works_on_its_own() {
    run(async {
        let got = parts(ApiError::Conflict("busy".into()).into_response()).await;
        check!("Conflict(\"busy\")", got, (409, json!({"error":{"code":"conflict","message":"busy"}})));
    });
}
