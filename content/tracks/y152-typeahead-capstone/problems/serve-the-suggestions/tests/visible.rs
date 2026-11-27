use solution::*;

use axum::body::{Body, to_bytes};
use axum::http::Request;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tower::ServiceExt;

/// `micros()` moves 100 on at every call.
#[derive(Default)]
struct Ticking {
    seconds: AtomicU64,
    micros: AtomicU64,
}

impl Clock for Ticking {
    fn seconds(&self) -> u64 {
        self.seconds.load(Ordering::SeqCst)
    }
    fn micros(&self) -> u64 {
        self.micros.fetch_add(100, Ordering::SeqCst)
    }
}

fn app() -> axum::Router {
    let service = Arc::new(Service::new(64, Box::new(classify)));
    service.add(&[("python", 12), ("pytorch", 4), ("javascript", 15)], 0);
    router(service, Arc::new(Ticking::default()))
}

fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
}

async fn send(app: &axum::Router, method: &str, path: &str, body: &str) -> (u16, Value) {
    let request = Request::builder().method(method).uri(path).header("content-type", "application/json");
    let response = app.clone().oneshot(request.body(Body::from(body.to_string())).unwrap()).await.unwrap();
    let status = response.status().as_u16();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
    (status, body)
}

#[test]
fn a_typo_over_http() {
    run(async {
        let app = app();
        let want = json!({"q":"Pyhton","key":"pyhton","cached":false,"suggestions":[{"term":"python","score":12.0,"kind":"typo","dist":1}]});
        check!("GET /suggest?q=Pyhton", send(&app, "GET", "/suggest?q=Pyhton", "").await, (200, want));
    });
}

#[test]
fn choose_and_see_it_move() {
    run(async {
        let app = app();
        check!("event", send(&app, "POST", "/events", r#"{"term":"PyTorch"}"#).await, (200, json!({"ok":true})));
        let got = send(&app, "GET", "/suggest?q=py&limit=1", "").await;
        check!("one suggestion", got.1["suggestions"][0]["term"].clone(), json!("python"));
        check!("unknown", send(&app, "POST", "/events", r#"{"term":"zig"}"#).await.0, 404);
    });
}

#[test]
fn new_terms_appear() {
    run(async {
        let app = app();
        let body = r#"{"terms":[{"term":"zig","count":3},{"term":"Python"}]}"#;
        check!("index", send(&app, "POST", "/index", body).await, (200, json!({"added":1,"existing":1})));
        let got = send(&app, "GET", "/suggest?q=zi", "").await;
        check!("zig", got.1["suggestions"][0].clone(), json!({"term":"zig","score":3.0,"kind":"prefix","dist":0}));
    });
}

#[test]
fn metrics_count_the_requests() {
    run(async {
        let app = app();
        send(&app, "GET", "/suggest?q=py", "").await;
        send(&app, "GET", "/suggest?q=py", "").await;
        let (status, body) = send(&app, "GET", "/metrics", "").await;
        let text = body.as_str().unwrap_or("").to_string();
        check!("status", status, 200);
        for line in ["typeahead_suggest_requests_total 2", "typeahead_cache_hits_total 1", "typeahead_indexed_terms 3", "typeahead_suggest_latency_us_sum 200"] {
            check!(format!("{line} in:\n{text}"), text.lines().any(|l| l == line), true);
        }
    });
}

#[test]
fn a_limit_that_is_no_number_is_a_client_error() {
    run(async {
        let status = send(&app(), "GET", "/suggest?q=py&limit=lots", "").await.0;
        check!("4xx", (400..500).contains(&status), true);
    });
}
