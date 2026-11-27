use solution::*;

#[allow(unused_imports)]
use std::time::Duration;

#[allow(dead_code)]
pub fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap().block_on(f)
}

#[allow(unused_imports)]
use axum::Router;
#[allow(unused_imports)]
use axum::body::{Body, to_bytes};
#[allow(unused_imports)]
use axum::http::{HeaderMap, Request};
#[allow(unused_imports)]
use serde_json::{Value, json};
#[allow(unused_imports)]
use tower::ServiceExt;

#[allow(dead_code)]
pub struct Reply {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: String,
}

#[allow(dead_code)]
impl Reply {
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or_else(|_| Value::String(self.body.clone()))
    }

    pub fn header(&self, name: &str) -> Option<String> {
        self.headers.get(name).map(|v| String::from_utf8_lossy(v.as_bytes()).into_owned())
    }

    /// (status, error code)
    pub fn error(&self) -> (u16, String) {
        (self.status, self.json()["error"]["code"].as_str().unwrap_or("<no error code>").to_string())
    }
}

#[allow(dead_code)]
pub fn failure(status: u16, code: &str) -> (u16, String) {
    (status, code.to_string())
}

#[allow(dead_code)]
pub async fn send(app: &Router, method: &str, uri: &str, headers: &[(&str, &str)], body: &str) -> Reply {
    let mut request = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let request = request.body(Body::from(body.to_string())).unwrap();
    let exchange = async {
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let bytes = to_bytes(response.into_body(), 1 << 22).await.unwrap();
        Reply { status, headers, body: String::from_utf8_lossy(&bytes).into_owned() }
    };
    tokio::time::timeout(std::time::Duration::from_secs(3600), exchange).await.expect("the response did not finish")
}

#[allow(dead_code)]
pub async fn get(app: &Router, uri: &str) -> Reply {
    send(app, "GET", uri, &[], "").await
}

#[allow(dead_code)]
pub async fn post_job(app: &Router, key: &str, body: &str) -> Reply {
    send(app, "POST", "/v1/jobs", &[("idempotency-key", key)], body).await
}

#[allow(dead_code)]
pub fn job_json(source: &str, work_ms: u64) -> String {
    json!({"source": source, "width": 10, "height": 20, "work_ms": work_ms}).to_string()
}

#[allow(dead_code)]
pub fn ids(page: &Value) -> Vec<u64> {
    page["items"].as_array().map(|a| a.iter().map(|j| j["id"].as_u64().unwrap_or(0)).collect()).unwrap_or_default()
}

fn app() -> Router {
    router(JobStore::new())
}

#[test]
fn post_then_get() {
    run(async {
        let app = app();
        let r = post_job(&app, "k1", r#"{"source":"cat.png","width":640,"height":480}"#).await;
        let want = json!({"id":1,"status":"queued","source":"cat.png","width":640,"height":480,"format":"png","created_at":1,"updated_at":1,"output":null,"error":null});
        check!("POST: (status, location, body)", (r.status, r.header("location"), r.json()), (201, Some("/v1/jobs/1".to_string()), want.clone()));
        let r = get(&app, "/v1/jobs/1").await;
        check!("GET /v1/jobs/1", (r.status, r.json()), (200, want));
    });
}

#[test]
fn a_retry_is_marked() {
    run(async {
        let app = app();
        post_job(&app, "k1", &job_json("a", 1)).await;
        let r = post_job(&app, "k1", &job_json("a", 1)).await;
        check!("retry: (status, idempotent-replayed, id)", (r.status, r.header("idempotent-replayed"), r.json()["id"].clone()), (200, Some("true".to_string()), json!(1)));
    });
}

#[test]
fn the_key_is_required() {
    run(async {
        let r = send(&app(), "POST", "/v1/jobs", &[], &job_json("a", 1)).await;
        check!("no Idempotency-Key", r.error(), failure(400, "missing_idempotency_key"));
    });
}

#[test]
fn two_pages() {
    run(async {
        let app = app();
        for k in ["a", "b", "c"] {
            post_job(&app, k, &job_json(k, 1)).await;
        }
        let first = get(&app, "/v1/jobs?limit=2").await.json();
        check!("page 1: (ids, next_after)", (ids(&first), first["next_after"].clone()), (vec![1, 2], json!(2)));
        let second = get(&app, "/v1/jobs?limit=2&after=2").await.json();
        check!("page 2: (ids, next_after)", (ids(&second), second["next_after"].clone()), (vec![3], Value::Null));
    });
}

#[test]
fn unknown_job() {
    run(async {
        check!("GET /v1/jobs/9", get(&app(), "/v1/jobs/9").await.error(), failure(404, "job_not_found"));
    });
}
