//! Cross-cutting layers: request id + trace span + metrics, security headers, timeout, panic guard,
//! authentication + rate limiting, content negotiation.
//! Recipes: `examples/web/axum_middleware.rs`, `examples/web/cors_security_headers.rs`.

use std::any::Any;
use std::time::Instant;

use axum::extract::{MatchedPath, Request, State};
use axum::http::{HeaderName, HeaderValue, Method, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::Instrument;

use super::extract::accepts_json;
use crate::auth::policy::Via;
use crate::auth::service;
use crate::config::{Config, Environment};
use crate::error::{AppError, REQUEST_ID};
use crate::state::AppState;

pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// The raw bearer credential, kept so long-lived streams can re-check that it is still valid.
#[derive(Clone)]
pub struct Credential(pub String);

/// Accept a caller's id (it links logs across services) only if it is harmless to write into logs.
fn incoming_request_id(req: &Request) -> Option<String> {
    let v = req.headers().get(REQUEST_ID_HEADER)?.to_str().ok()?;
    let safe =
        !v.is_empty() && v.len() <= 64 && v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    safe.then(|| v.to_string())
}

/// Outermost layer: everything below runs inside the span and the request-id scope.
pub async fn observe(State(app): State<AppState>, req: Request, next: Next) -> Response {
    let id = incoming_request_id(&req).unwrap_or_else(|| app.new_id());
    // The route TEMPLATE, never the raw path: bounded metric cardinality and no ids or signatures in logs.
    let route =
        req.extensions().get::<MatchedPath>().map_or_else(|| "unmatched".to_string(), |m| m.as_str().to_string());
    let method = req.method().clone();
    let span = tracing::info_span!("request", request_id = %id, method = %method, route = %route);
    let started = Instant::now();
    let mut res = REQUEST_ID.scope(id.clone(), next.run(req)).instrument(span.clone()).await;
    let elapsed = started.elapsed();
    app.metrics.observe(method.as_str(), &route, res.status().as_u16(), elapsed.as_secs_f64());
    span.in_scope(|| {
        tracing::info!(status = res.status().as_u16(), duration_ms = elapsed.as_millis() as u64, "request completed");
    });
    if let Ok(v) = HeaderValue::from_str(&id) {
        res.headers_mut().insert(REQUEST_ID_HEADER, v);
    }
    res
}

pub async fn security_headers(State(app): State<AppState>, req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    let fixed = [
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::X_FRAME_OPTIONS, "DENY"),
        (header::REFERRER_POLICY, "no-referrer"),
        // This is a JSON API: nothing it returns should ever load or be framed.
        (header::CONTENT_SECURITY_POLICY, "default-src 'none'; frame-ancestors 'none'"),
    ];
    for (name, value) in fixed {
        h.insert(name, HeaderValue::from_static(value));
    }
    // Handlers may choose a caching policy; the default must be the safe one (tokens, tenant data).
    h.entry(header::CACHE_CONTROL).or_insert(HeaderValue::from_static("no-store"));
    // Only meaningful over TLS, and sticky in browsers, so never send it from a dev server.
    if app.cfg.environment == Environment::Prod {
        h.insert(header::STRICT_TRANSPORT_SECURITY, HeaderValue::from_static("max-age=31536000; includeSubDomains"));
    }
    res
}

/// Explicit origin allow-list. Tokens travel in `Authorization`, not cookies, so credentials mode is off.
pub fn cors(cfg: &Config) -> CorsLayer {
    let origins: Vec<HeaderValue> = cfg.server.cors_origins.iter().filter_map(|o| o.parse().ok()).collect();
    let names = |list: &[&'static str]| list.iter().map(|n| HeaderName::from_static(n)).collect::<Vec<_>>();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE])
        .allow_headers(names(&[
            "authorization",
            "content-type",
            "if-match",
            "if-none-match",
            "idempotency-key",
            "last-event-id",
            REQUEST_ID_HEADER,
        ]))
        .expose_headers(names(&[
            "etag",
            "retry-after",
            "ratelimit-limit",
            "ratelimit-remaining",
            "ratelimit-reset",
            "idempotency-replayed",
            REQUEST_ID_HEADER,
        ]))
}

/// Bounds the time to PRODUCE a response. Streaming bodies (SSE) continue after this returns.
/// Dropping the handler future rolls back any open transaction.
pub async fn timeout(State(app): State<AppState>, req: Request, next: Next) -> Response {
    tokio::time::timeout(app.cfg.request_timeout(), next.run(req))
        .await
        .unwrap_or_else(|_| AppError::Unavailable { retry_after_secs: 1 }.into_response())
}

/// A panic must become a 500, not a dropped connection, and must not leak its message.
pub fn panic_response(_: Box<dyn Any + Send + 'static>) -> Response {
    AppError::internal(anyhow::anyhow!("handler panicked")).into_response()
}

fn bearer(req: &Request) -> Option<&str> {
    let value = req.headers().get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then(|| token.trim())
}

/// Authenticate, then spend one rate-limit token. The bucket is per API key, or per user for sessions.
/// Unauthenticated traffic is NOT limited here: limit it at the edge, by client address.
pub async fn authenticate(State(app): State<AppState>, mut req: Request, next: Next) -> Response {
    let Some(credential) = bearer(&req).map(str::to_string) else {
        return AppError::Unauthenticated("missing bearer token").into_response();
    };
    let principal = match service::authenticate(&app, &credential).await {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let bucket = match &principal.via {
        Via::ApiKey(id) => format!("key:{id}"),
        Via::Session(_) => format!("user:{}", principal.user_id),
    };
    let decision = app.limiter.check(&bucket, app.now_ms());
    let mut res = if decision.allowed {
        req.extensions_mut().insert(principal);
        req.extensions_mut().insert(Credential(credential));
        next.run(req).await
    } else {
        AppError::RateLimited { retry_after_secs: decision.retry_after_secs }.into_response()
    };
    decision.apply(res.headers_mut());
    res
}

/// Refuse BEFORE the handler runs: a write must not happen if we cannot send its result.
pub async fn require_json_accept(req: Request, next: Next) -> Response {
    if !accepts_json(req.headers()) {
        return AppError::NotAcceptable.into_response();
    }
    next.run(req).await
}
