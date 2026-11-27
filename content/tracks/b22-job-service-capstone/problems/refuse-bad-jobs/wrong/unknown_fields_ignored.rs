use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

// ---------------------------------------------------------------- errors

/// Every way the service refuses a request.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ApiError {
    #[error("the Idempotency-Key header is required")]
    MissingKey,
    #[error("the body is not a job in JSON: {0}")]
    InvalidJson(String),
    #[error("the job did not pass validation")]
    Validation(Vec<String>),
    #[error("this Idempotency-Key was already used for a different job")]
    KeyReuse,
    #[error("job {0} does not exist")]
    NotFound(String),
    #[error("the job is already {0} and cannot be cancelled")]
    NotCancellable(String),
    #[error("invalid query parameter: {0}")]
    BadQuery(String),
    #[error("the service is shutting down")]
    ShuttingDown,
    #[error("no such route")]
    NoRoute,
    #[error("method not allowed")]
    MethodNotAllowed,
}

impl ApiError {
    /// The status, and the code that clients may rely on.
    fn parts(&self) -> (StatusCode, &'static str) {
        use StatusCode as S;
        match self {
            Self::MissingKey => (S::BAD_REQUEST, "missing_idempotency_key"),
            Self::InvalidJson(_) => (S::UNPROCESSABLE_ENTITY, "invalid_json"),
            Self::Validation(_) => (S::UNPROCESSABLE_ENTITY, "validation_failed"),
            Self::KeyReuse => (S::CONFLICT, "idempotency_key_reuse"),
            Self::NotFound(_) => (S::NOT_FOUND, "job_not_found"),
            Self::NotCancellable(_) => (S::CONFLICT, "not_cancellable"),
            Self::BadQuery(_) => (S::BAD_REQUEST, "bad_query"),
            Self::ShuttingDown => (S::SERVICE_UNAVAILABLE, "shutting_down"),
            Self::NoRoute => (S::NOT_FOUND, "no_route"),
            Self::MethodNotAllowed => (S::METHOD_NOT_ALLOWED, "method_not_allowed"),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = self.parts();
        let message = self.to_string();
        let details = match self {
            Self::Validation(problems) => problems,
            _ => Vec::new(),
        };
        let body = json!({"error": {"code": code, "message": message, "details": details}});
        (status, Json(body)).into_response()
    }
}

// ---------------------------------------------------------------- the job a client asks for

fn default_format() -> String {
    "png".to_string()
}

fn default_work_ms() -> u64 {
    20
}

/// An image to resize.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Spec {
    pub source: String,
    pub width: u32,
    pub height: u32,
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_work_ms")]
    pub work_ms: u64,
}

impl Spec {
    /// All the problems, in field order, so the client can fix them in one go.
    fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.source.trim().is_empty() {
            problems.push("source: must not be blank".to_string());
        }
        for (field, value) in [("width", self.width), ("height", self.height)] {
            if !(1..=8192).contains(&value) {
                problems.push(format!("{field}: must be from 1 to 8192, got {value}"));
            }
        }
        if !["png", "jpg", "webp"].contains(&self.format.as_str()) {
            problems.push("format: must be png, jpg or webp".to_string());
        }
        if self.work_ms > 10_000 {
            problems.push("work_ms: must be at most 10000".to_string());
        }
        problems
    }
}

pub fn parse_spec(body: &[u8]) -> Result<Spec, ApiError> {
    // serde also reads a struct from an array, such as `["a.png", 10, 20]`, so ask for an object first.
    let first = body.iter().find(|byte| !b" \t\r\n".contains(byte));
    if first != Some(&b'{') {
        return Err(ApiError::InvalidJson("expected a JSON object".to_string()));
    }
    let spec: Spec =
        serde_json::from_slice(body).map_err(|e| ApiError::InvalidJson(e.to_string()))?;
    let problems = spec.problems();
    if problems.is_empty() {
        Ok(spec)
    } else {
        Err(ApiError::Validation(problems))
    }
}
