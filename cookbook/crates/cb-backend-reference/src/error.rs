//! BLOCK 3 (part): one error type for the whole app, rendered as RFC 9457 `application/problem+json`.
//!
//! Design: handlers return `Result<T, AppError>`; `?` converts lower-level errors; `IntoResponse` is the
//! ONLY place that decides status codes and bodies, so clients get one consistent error shape.
//! Pitfalls: (1) never put SQL/driver text in `detail` (information leak): internal errors are logged with
//! the request id and answered with a generic message; (2) 401 must carry `WWW-Authenticate`;
//! (3) 429/503 should carry `Retry-After`; (4) validation errors list EVERY bad field, not just the first.

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::json;

tokio::task_local! {
    /// Request id of the request being handled, set by the request-id middleware.
    pub static REQUEST_ID: String;
}

#[must_use]
pub fn current_request_id() -> Option<String> {
    REQUEST_ID.try_with(Clone::clone).ok()
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FieldError {
    pub field: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AppError {
    #[error("validation failed")]
    Validation(Vec<FieldError>),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("unauthenticated: {0}")]
    Unauthenticated(&'static str),
    #[error("forbidden")]
    Forbidden,
    #[error("{0} not found")]
    NotFound(&'static str),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("precondition failed (stale If-Match)")]
    PreconditionFailed,
    #[error("If-Match header required")]
    PreconditionRequired,
    #[error("unprocessable: {0}")]
    Unprocessable(String),
    #[error("rate limited")]
    RateLimited { retry_after_secs: u64 },
    #[error("unsupported media type: {0}")]
    UnsupportedMediaType(String),
    #[error("not acceptable")]
    NotAcceptable,
    #[error("payload too large")]
    PayloadTooLarge,
    #[error("service unavailable")]
    Unavailable { retry_after_secs: u64 },
    #[error("internal error")]
    Internal(#[source] anyhow::Error),
}

impl AppError {
    pub fn internal(e: impl Into<anyhow::Error>) -> Self {
        Self::Internal(e.into())
    }

    #[must_use]
    pub fn status(&self) -> StatusCode {
        match self {
            Self::Validation(_) | Self::Unprocessable(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthenticated(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::PreconditionFailed => StatusCode::PRECONDITION_FAILED,
            Self::PreconditionRequired => StatusCode::PRECONDITION_REQUIRED,
            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::UnsupportedMediaType(_) => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::NotAcceptable => StatusCode::NOT_ACCEPTABLE,
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Unavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Stable machine-readable slug; the `type` URI is derived from it. Clients switch on this, not on prose.
    #[must_use]
    pub fn slug(&self) -> &'static str {
        match self {
            Self::Validation(_) => "validation",
            Self::BadRequest(_) => "bad-request",
            Self::Unauthenticated(_) => "unauthenticated",
            Self::Forbidden => "forbidden",
            Self::NotFound(_) => "not-found",
            Self::Conflict(_) => "conflict",
            Self::PreconditionFailed => "precondition-failed",
            Self::PreconditionRequired => "precondition-required",
            Self::Unprocessable(_) => "unprocessable",
            Self::RateLimited { .. } => "rate-limited",
            Self::UnsupportedMediaType(_) => "unsupported-media-type",
            Self::NotAcceptable => "not-acceptable",
            Self::PayloadTooLarge => "payload-too-large",
            Self::Unavailable { .. } => "unavailable",
            Self::Internal(_) => "internal",
        }
    }

    fn detail(&self) -> String {
        match self {
            // Hide internals: the log line (with request id) has the real cause.
            Self::Internal(_) => "an internal error occurred".into(),
            other => other.to_string(),
        }
    }
}

/// SQLite result codes we care about (primary codes, as strings in sqlx).
fn sqlite_code(e: &sqlx::Error) -> Option<String> {
    e.as_database_error().and_then(|d| d.code()).map(|c| c.to_string())
}

#[must_use]
pub fn is_unique_violation(e: &sqlx::Error) -> bool {
    e.as_database_error().is_some_and(|d| d.is_unique_violation())
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        // SQLITE_BUSY (5) / SQLITE_LOCKED (6) / pool timeout: transient, tell the client to retry.
        let busy = matches!(sqlite_code(&e).as_deref(), Some("5" | "6" | "261" | "517"))
            || matches!(e, sqlx::Error::PoolTimedOut);
        if busy {
            return Self::Unavailable { retry_after_secs: 1 };
        }
        Self::Internal(e.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        if let Self::Internal(cause) = &self {
            tracing::error!(error = ?cause, request_id = ?current_request_id(), "internal error");
        }
        let mut body = json!({
            "type": format!("https://reference.invalid/problems/{}", self.slug()),
            "title": status.canonical_reason().unwrap_or("Error"),
            "status": status.as_u16(),
            "detail": self.detail(),
        });
        if let Some(id) = current_request_id() {
            body["request_id"] = json!(id);
        }
        if let Self::Validation(errors) = &self {
            body["errors"] = json!(errors);
        }
        let mut res = (status, body.to_string()).into_response();
        let h = res.headers_mut();
        h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/problem+json"));
        match &self {
            Self::Unauthenticated(_) => {
                h.insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
            }
            Self::RateLimited { retry_after_secs } | Self::Unavailable { retry_after_secs } => {
                h.insert(header::RETRY_AFTER, HeaderValue::from(*retry_after_secs));
            }
            _ => {}
        }
        res
    }
}

pub type AppResult<T> = Result<T, AppError>;

/// Accumulates field errors so one response reports everything wrong with a request.
#[derive(Debug, Default)]
pub struct Validator {
    errors: Vec<FieldError>,
}

impl Validator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fail(&mut self, field: &str, code: &str, message: impl Into<String>) {
        self.errors.push(FieldError { field: field.into(), code: code.into(), message: message.into() });
    }

    /// Length in chars (not bytes) after trimming, so "   " is empty and emoji count once.
    pub fn text(&mut self, field: &str, value: &str, min: usize, max: usize) -> &mut Self {
        let n = value.trim().chars().count();
        if n < min {
            self.fail(field, "too_short", format!("must have at least {min} characters"));
        } else if n > max {
            self.fail(field, "too_long", format!("must have at most {max} characters"));
        }
        self
    }

    pub fn email(&mut self, field: &str, value: &str) -> &mut Self {
        if !is_plausible_email(value) {
            self.fail(field, "invalid_email", "must be a valid email address");
        }
        self
    }

    pub fn range(&mut self, field: &str, value: i64, min: i64, max: i64) -> &mut Self {
        if !(min..=max).contains(&value) {
            self.fail(field, "out_of_range", format!("must be between {min} and {max}"));
        }
        self
    }

    pub fn one_of(&mut self, field: &str, value: &str, allowed: &[&str]) -> &mut Self {
        if !allowed.contains(&value) {
            self.fail(field, "invalid_value", format!("must be one of: {}", allowed.join(", ")));
        }
        self
    }

    pub fn finish(self) -> AppResult<()> {
        if self.errors.is_empty() { Ok(()) } else { Err(AppError::Validation(self.errors)) }
    }
}

/// Deliberately simple: full RFC 5322 validation is a trap; the real check is the verification email.
#[must_use]
pub fn is_plausible_email(s: &str) -> bool {
    let Some((local, domain)) = s.split_once('@') else { return false };
    s.len() <= 254
        && !local.is_empty()
        && local.len() <= 64
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains("..")
        && !s.chars().any(|c| c.is_whitespace() || c.is_control() || c == '@' && s.matches('@').count() > 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    async fn body_json(res: Response) -> serde_json::Value {
        let b = res.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&b).unwrap()
    }

    #[tokio::test]
    async fn validation_renders_problem_json_with_all_fields() {
        let mut v = Validator::new();
        v.text("name", "  ", 1, 10).email("email", "nope").range("priority", 99, 0, 5);
        let res = v.finish().unwrap_err().into_response();
        assert_eq!(res.status(), 422);
        assert_eq!(res.headers()[header::CONTENT_TYPE], "application/problem+json");
        let j = body_json(res).await;
        assert_eq!(j["type"], "https://reference.invalid/problems/validation");
        assert_eq!(j["errors"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn internal_errors_do_not_leak_causes() {
        let res = AppError::internal(anyhow::anyhow!("secret SQL: SELECT * FROM users")).into_response();
        assert_eq!(res.status(), 500);
        let j = body_json(res).await;
        assert!(!j.to_string().contains("SELECT"));
    }

    #[tokio::test]
    async fn auth_and_retry_headers() {
        let r = AppError::Unauthenticated("missing token").into_response();
        assert_eq!(r.headers()[header::WWW_AUTHENTICATE], "Bearer");
        let r = AppError::RateLimited { retry_after_secs: 7 }.into_response();
        assert_eq!(r.headers()[header::RETRY_AFTER], "7");
    }

    #[tokio::test]
    async fn request_id_is_included_when_in_scope() {
        let res = REQUEST_ID.scope("req-1".into(), async { AppError::Forbidden.into_response() }).await;
        assert_eq!(body_json(res).await["request_id"], "req-1");
    }

    #[test]
    fn email_plausibility() {
        for ok in ["a@b.co", "first.last@sub.example.org"] {
            assert!(is_plausible_email(ok), "{ok}");
        }
        for bad in ["", "a", "a@b", "@b.co", "a@@b.co", "a b@c.de", "a@.co", "a@b..co", "a@b.co."] {
            assert!(!is_plausible_email(bad), "{bad}");
        }
    }

    #[test]
    fn text_counts_chars_not_bytes() {
        let mut v = Validator::new();
        v.text("t", "ééé", 1, 3);
        assert!(v.finish().is_ok());
    }
}
