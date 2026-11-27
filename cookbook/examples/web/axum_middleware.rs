//! RECIPE 30 (axum): middleware with `from_fn`: correlation ids on every request and response.
//! LEVEL: intermediate
//! Use in: REST API stage ("correlation IDs propagated across services").   Run: cargo run --release --example axum_middleware
//! COMMON USES: request ids/correlation ids across microservices; the same layer shape hosts auth, metrics, tracing.
//!
//! Pattern: a middleware is `async fn(Request, Next) -> Response`. It can inspect/modify the request before
//! calling `next.run(req)` and the response after. Here: reuse the caller's `x-request-id` or mint one, and
//! echo it back so logs across services can be joined. Layers wrap routes added BEFORE `.layer(..)`.
//! LIMITS: the caller's id is copied as it comes (any length the server accepts for a header), so check or replace it before
//! trusting it in logs, and never use it for access control. The minted ids come from a counter: they repeat after a restart
//! and between instances. Use a random or time-based unique id (a UUID, say) to join logs across services.
#![allow(dead_code)]
use axum::{
    Router,
    body::Body,
    extract::Request,
    http::{HeaderValue, header::HeaderName},
    middleware::{self, Next},
    response::Response,
    routing::get,
};
use std::sync::atomic::{AtomicU64, Ordering};
use tower::ServiceExt;

static NEXT: AtomicU64 = AtomicU64::new(1);
const RID: HeaderName = HeaderName::from_static("x-request-id");

async fn request_id(mut req: Request, next: Next) -> Response {
    let id = match req.headers().get(&RID) {
        Some(v) => v.clone(),
        None => {
            HeaderValue::from_str(&format!("req-{}", NEXT.fetch_add(1, Ordering::Relaxed))).unwrap()
        }
    };
    req.headers_mut().insert(RID, id.clone()); // handlers can read it too
    let mut resp = next.run(req).await;
    resp.headers_mut().insert(RID, id);
    resp
}

async fn ping() -> &'static str {
    "pong"
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/ping", get(ping))
        .layer(middleware::from_fn(request_id));

    let mut seen = std::collections::HashSet::new();
    for supplied in [None, None, Some("trace-abc")] {
        let mut b = Request::builder().uri("/ping");
        if let Some(s) = supplied {
            b = b.header("x-request-id", s);
        }
        let resp = app
            .clone()
            .oneshot(b.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let id = resp.headers()["x-request-id"].to_str().unwrap().to_string();
        println!("supplied {supplied:?} -> response x-request-id = {id}");
        assert!(seen.insert(id.clone()), "each request has its own id");
        if let Some(s) = supplied {
            assert_eq!(id, s, "caller's id must be preserved");
        }
    }
}
