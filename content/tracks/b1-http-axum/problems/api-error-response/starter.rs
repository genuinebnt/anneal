use axum::response::{IntoResponse, Response};
use axum::Router;
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

/// An in-memory stand-in for a database. User ids start at 1.
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

    /// `Ok(None)` when the name is taken.
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
        todo!()
    }
}

pub fn router(store: Arc<Store>) -> Router {
    todo!()
}
