//! RECIPE 28 (axum): one error type -> consistent HTTP error responses.
//! LEVEL: intermediate
//! Use in: any JSON API ("one error format and meaningful status codes").   Run: cargo run --release --example axum_errors
//! COMMON USES: public APIs that need one stable error shape and no leaked internals.
//!
//! Pattern: handlers return `Result<T, AppError>`. `AppError` implements `IntoResponse`, mapping each variant
//! to a status code and ONE JSON shape `{"error":{"code":..,"message":..}}`. Internal errors are logged but their
//! details are never leaked to the client. Errors are values, as with any error enum.
//! LIMIT: the shape covers only what a handler returns as `AppError`. axum's own rejections keep their own bodies: an
//! unmatched route (404) and a wrong method (405) have empty ones, and a path that is not UTF-8 gets a plain-text 400. For
//! one shape everywhere add fallbacks and wrap the extractors. This demo logs with `eprintln!` and sends the variant's
//! message, so never put a secret in the text of a variant the client sees.
#![allow(dead_code)]
use axum::{
    Json, Router,
    body::Body,
    extract::Path,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("workflow `{0}` not found")]
    NotFound(String),
    #[error("validation failed")]
    Invalid(Vec<String>),
    #[error("internal error")]
    Internal(#[source] std::io::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, details) = match &self {
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found", vec![]),
            AppError::Invalid(d) => (StatusCode::UNPROCESSABLE_ENTITY, "invalid", d.clone()),
            AppError::Internal(e) => {
                eprintln!("internal error: {e}"); // log the detail, do not send it
                (StatusCode::INTERNAL_SERVER_ERROR, "internal", vec![])
            }
        };
        let message = self.to_string();
        (
            status,
            Json(json!({ "error": { "code": code, "message": message, "details": details } })),
        )
            .into_response()
    }
}

async fn show(Path(id): Path<String>) -> Result<Json<serde_json::Value>, AppError> {
    match id.as_str() {
        "video" => Ok(Json(json!({ "id": "video" }))),
        "bad" => Err(AppError::Invalid(vec![
            "task `b` depends on unknown `ghost`".into(),
        ])),
        "boom" => Err(AppError::Internal(std::io::Error::other(
            "disk exploded at /secret/path",
        ))),
        other => Err(AppError::NotFound(other.to_string())),
    }
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/v1/workflows/{id}", get(show));
    for (id, want) in [("video", 200), ("nope", 404), ("bad", 422), ("boom", 500)] {
        let req = Request::builder()
            .uri(format!("/v1/workflows/{id}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let body = String::from_utf8(
            resp.into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap();
        println!("{id:>6} -> {status} {body}");
        assert_eq!(status.as_u16(), want);
        assert!(
            !body.contains("/secret/path"),
            "internal details must not leak"
        );
    }
}
