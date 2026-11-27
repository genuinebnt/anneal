use axum::response::{IntoResponse, Response};

/// Every way the service refuses a request.
#[derive(Debug, Clone, PartialEq)]
pub enum ApiError {
    MissingKey,
    InvalidJson(String),
    Validation(Vec<String>),
    KeyReuse,
    NotFound(String),
    NotCancellable(String),
    BadQuery(String),
    ShuttingDown,
    NoRoute,
    MethodNotAllowed,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        todo!()
    }
}

/// An image to resize.
#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    pub source: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub work_ms: u64,
}

pub fn parse_spec(body: &[u8]) -> Result<Spec, ApiError> {
    todo!()
}
