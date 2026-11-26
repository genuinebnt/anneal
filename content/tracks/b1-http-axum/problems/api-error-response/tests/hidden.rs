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

/// The status and `error.code`, after checking the shape: exactly `error`, with a non-empty string message.
#[allow(dead_code)]
fn shape((status, body): (u16, Value)) -> (u16, String) {
    let ok = body.as_object().is_some_and(|o| o.len() == 1) && body["error"]["message"].as_str().is_some_and(|m| !m.is_empty());
    let code = if ok { body["error"]["code"].as_str().unwrap_or("?").to_string() } else { format!("wrong shape: {body}") };
    (status, code)
}

#[test]
fn broken_json_is_bad_request() {
    run(async {
        let app = router(Arc::new(Store::default()));
        for body in ["", "{", "[1]", r#"{"name":5}"#, r#"{}"#, r#"{"name":"a"} x"#] {
            check!(format!("POST {body:?}"), shape(send(&app, "POST", "/users", JSON, body).await), (400, "bad_request".to_string()));
        }
    });
}

#[test]
fn content_type_is_required() {
    run(async {
        let app = router(Arc::new(Store::default()));
        for ct in [None, Some("text/plain")] {
            let got = send(&app, "POST", "/users", ct, r#"{"name":"a"}"#).await;
            check!(format!("content type {ct:?}"), got, (415, json!({"error":{"code":"unsupported_media_type","message":"expected application/json"}})));
        }
    });
}

#[test]
fn bad_ids_are_bad_requests() {
    run(async {
        let app = router(Arc::new(Store::default()));
        for path in ["/users/abc", "/users/-1", "/users/18446744073709551616"] {
            check!(format!("GET {path}"), shape(send(&app, "GET", path, None, "").await), (400, "bad_request".to_string()));
        }
    });
}

#[test]
fn id_zero_is_just_not_found() {
    run(async {
        let app = router(Arc::new(Store::default()));
        check!("GET /users/0", shape(send(&app, "GET", "/users/0", None, "").await), (404, "not_found".to_string()));
    });
}

#[test]
fn unknown_routes() {
    run(async {
        let app = router(Arc::new(Store::default()));
        for path in ["/", "/user/1", "/users/1/pets"] {
            let got = send(&app, "GET", path, None, "").await;
            check!(format!("GET {path}"), got, (404, json!({"error":{"code":"no_route","message":"no such route"}})));
        }
    });
}

#[test]
fn wrong_methods() {
    run(async {
        let app = router(Arc::new(Store::default()));
        for (method, path) in [("GET", "/users"), ("DELETE", "/users/1")] {
            let got = send(&app, method, path, None, "").await;
            check!(format!("{method} {path}"), got, (405, json!({"error":{"code":"method_not_allowed","message":"method not allowed"}})));
        }
    });
}

#[test]
fn name_length_is_counted_in_characters() {
    run(async {
        let app = router(Arc::new(Store::default()));
        let twenty = "é".repeat(20);
        let got = send(&app, "POST", "/users", JSON, &json!({"name": twenty}).to_string()).await;
        check!("20 two-byte characters", got.0, 201);
        let got = send(&app, "POST", "/users", JSON, &json!({"name": "x".repeat(21)}).to_string()).await;
        check!("21 characters", (got.0, got.1["error"]["fields"][0]["problem"].clone()), (422, json!("too long")));
    });
}

#[test]
fn duplicates_conflict_after_trimming() {
    run(async {
        let app = router(Arc::new(Store::default()));
        send(&app, "POST", "/users", JSON, r#"{"name":"ada"}"#).await;
        let got = send(&app, "POST", "/users", JSON, r#"{"name":"ada  "}"#).await;
        check!("ada again", got, (409, json!({"error":{"code":"conflict","message":"name already taken"}})));
    });
}

#[test]
fn validation_runs_before_the_database() {
    run(async {
        let store = Arc::new(Store::default());
        store.down.store(true, std::sync::atomic::Ordering::SeqCst);
        check!("blank name while the database is down", shape(send(&router(store), "POST", "/users", JSON, r#"{"name":""}"#).await), (422, "invalid".to_string()));
    });
}

#[test]
fn internal_never_shows_its_cause() {
    run(async {
        let got = parts(ApiError::Internal("secret-token-123".into()).into_response()).await;
        check!("the body contains the cause", got.1.to_string().contains("secret-token-123"), false);
        check!("status", got.0, 500);
    });
}

#[test]
fn responses_are_labelled_as_json() {
    run(async {
        let response = ApiError::NoRoute.into_response();
        let ct = response.headers().get("content-type").and_then(|v| v.to_str().ok()).map(str::to_string);
        check!("content-type of an error", ct, Some("application/json".to_string()));
    });
}

#[test]
fn random_sessions_match_a_model() {
    run(async {
        let mut rng = anneal_prelude::Rng::new(8118);
        for _ in 0..100 {
            let app = router(Arc::new(Store::default()));
            let mut model: Vec<String> = Vec::new();
            for _ in 0..10 {
                if rng.bool() {
                    let len = rng.below(3);
                    let raw = rng.string(len, "ab ");
                    let name = raw.trim().to_string();
                    let want = if name.is_empty() {
                        422
                    } else if model.contains(&name) {
                        409
                    } else {
                        model.push(name);
                        201
                    };
                    let got = send(&app, "POST", "/users", JSON, &json!({"name": raw}).to_string()).await.0;
                    check!(format!("POST {raw:?} with users {model:?}"), got, want);
                } else {
                    let id = rng.below(5) as u64;
                    let got = send(&app, "GET", &format!("/users/{id}"), None, "").await;
                    let found = id.checked_sub(1).and_then(|i| model.get(i as usize));
                    match found {
                        Some(name) => check!(format!("GET {id}"), got, (200, json!({"id": id, "name": name}))),
                        None => check!(format!("GET {id}"), got.0, 404),
                    }
                }
            }
        }
    });
}
