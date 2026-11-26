use axum::extract::rejection::{JsonRejection, PathRejection};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub field: &'static str,
    pub problem: &'static str,
}

#[derive(Debug)]
pub enum ApiError {
    NotFound { id: u64 },
    Invalid(Vec<FieldError>),
    Conflict(String),
    BadRequest(String),
    UnsupportedMediaType,
    NoRoute,
    MethodNotAllowed,
    Internal(String),
}

#[derive(Default)]
pub struct Store {
    names: Mutex<Vec<String>>,
    pub down: AtomicBool,
}

impl Store {
    pub fn name_of(&self, id: u64) -> Option<String> {
        let index = usize::try_from(id.checked_sub(1)?).ok()?;
        self.names.lock().unwrap().get(index).cloned()
    }

    pub fn insert(&self, name: &str) -> Result<Option<u64>, String> {
        if self.down.load(Ordering::SeqCst) {
            return Err("connection refused: postgres://app:hunter2@10.0.0.7/users".to_string());
        }
        let mut names = self.names.lock().unwrap();
        if names.iter().any(|n| n == name) {
            return Ok(None);
        }
        names.push(name.to_string());
        Ok(Some(names.len() as u64))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            ApiError::NotFound { id } => (
                StatusCode::NOT_FOUND,
                "not_found",
                format!("user {id} not found"),
            ),
            ApiError::Invalid(_) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid",
                "validation failed".to_string(),
            ),
            ApiError::Conflict(what) => (StatusCode::CONFLICT, "conflict", what.clone()),
            ApiError::BadRequest(message) => {
                (StatusCode::BAD_REQUEST, "bad_request", message.clone())
            }
            ApiError::UnsupportedMediaType => (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported_media_type",
                "expected application/json".to_string(),
            ),
            ApiError::NoRoute => (
                StatusCode::NOT_FOUND,
                "no_route",
                "no such route".to_string(),
            ),
            ApiError::MethodNotAllowed => (
                StatusCode::METHOD_NOT_ALLOWED,
                "method_not_allowed",
                "method not allowed".to_string(),
            ),
            ApiError::Internal(cause) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal", cause.clone())
            }
        };
        let mut body = json!({ "error": { "code": code, "message": message } });
        if let ApiError::Invalid(fields) = self {
            body["error"]["fields"] = fields
                .iter()
                .map(|f| json!({ "field": f.field, "problem": f.problem }))
                .collect::<Value>();
        }
        (status, Json(body)).into_response()
    }
}

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        if rejection.status() == StatusCode::UNSUPPORTED_MEDIA_TYPE {
            ApiError::UnsupportedMediaType
        } else {
            ApiError::BadRequest(rejection.body_text())
        }
    }
}

impl From<PathRejection> for ApiError {
    fn from(rejection: PathRejection) -> Self {
        ApiError::BadRequest(rejection.body_text())
    }
}

#[derive(Deserialize)]
struct NewUser {
    name: String,
}

async fn show(
    State(store): State<Arc<Store>>,
    id: Result<Path<u64>, PathRejection>,
) -> Result<Json<Value>, ApiError> {
    let Path(id) = id?;
    let name = store.name_of(id).ok_or(ApiError::NotFound { id })?;
    Ok(Json(json!({ "id": id, "name": name })))
}

async fn create(
    State(store): State<Arc<Store>>,
    body: Result<Json<NewUser>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let Json(user) = body?;
    let name = user.name.trim();
    let problem = if name.is_empty() {
        Some("required")
    } else if name.chars().count() > 20 {
        Some("too long")
    } else {
        None
    };
    if let Some(problem) = problem {
        return Err(ApiError::Invalid(vec![FieldError {
            field: "name",
            problem,
        }]));
    }
    let id = store
        .insert(name)
        .map_err(ApiError::Internal)?
        .ok_or_else(|| ApiError::Conflict("name already taken".to_string()))?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id, "name": name }))))
}

pub fn router(store: Arc<Store>) -> Router {
    Router::new()
        .route("/users", post(create))
        .route("/users/{id}", get(show))
        .fallback(|| async { ApiError::NoRoute })
        .method_not_allowed_fallback(|| async { ApiError::MethodNotAllowed })
        .with_state(store)
}
