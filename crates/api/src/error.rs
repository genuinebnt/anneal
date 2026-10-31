use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0} not found")]
    NotFound(String),
    /// The problem is still a draft: it has no tests to run yet.
    #[error("{0} is a draft and can't be run yet")]
    NotReady(String),
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Busy(String),
    /// A practice problem whose LeetCode problem hasn't been attempted yet.
    #[error("{0}")]
    Locked(String),
    /// Something outside anneal (LeetCode) didn't answer.
    #[error("{0}")]
    Upstream(String),
    #[error("sign in first")]
    Unauthorized,
    #[error("wrong passphrase")]
    WrongPassphrase,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Runner(#[from] anneal_runner::RunnerError),
    /// A failure inside anneal that is not the database or the runner (the meaning model, for one).
    #[error("{0:#}")]
    Internal(anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            ApiError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::NotReady(_) => (StatusCode::CONFLICT, "not_ready"),
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            ApiError::Locked(_) => (StatusCode::LOCKED, "locked"),
            ApiError::Upstream(_) => (StatusCode::BAD_GATEWAY, "upstream"),
            ApiError::Busy(_) => (StatusCode::TOO_MANY_REQUESTS, "busy"),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            ApiError::WrongPassphrase => (StatusCode::UNAUTHORIZED, "wrong_passphrase"),
            ApiError::Db(_) | ApiError::Runner(_) | ApiError::Internal(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal")
            }
        };
        if status.is_server_error() {
            tracing::error!(error = %self, "request failed");
        }
        (
            status,
            Json(json!({ "error": code, "message": self.to_string() })),
        )
            .into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
