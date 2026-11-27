use axum::body::Bytes;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::ops::Bound;
use std::sync::{Arc, Mutex, MutexGuard};

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
#[serde(deny_unknown_fields)]
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

// ---------------------------------------------------------------- jobs and the store

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn is_final(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

/// How a job ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Succeeded(String),
    Failed(String),
    Cancelled,
}

/// A job as clients see it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JobView {
    pub id: u64,
    pub status: Status,
    pub source: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub output: Option<String>,
    pub error: Option<String>,
}

/// One line of a job's history.
#[derive(Debug, Clone, PartialEq)]
pub struct LogEntry {
    pub id: u64,
    pub name: &'static str,
    pub data: String,
}

struct Job {
    view: JobView,
    log: Vec<LogEntry>,
}

impl Job {
    /// Every change of status goes through here, so the status, the time and the history cannot disagree.
    fn record(&mut self, status: Status, now: u64) {
        self.view.status = status;
        self.view.updated_at = now;
        let name = match status {
            Status::Queued => "job.queued",
            Status::Running => "job.started",
            Status::Succeeded => "job.succeeded",
            Status::Failed => "job.failed",
            Status::Cancelled => "job.cancelled",
        };
        let data = json!({"job_id": self.view.id, "status": status.as_str()});
        self.log.push(LogEntry {
            id: self.log.len() as u64 + 1,
            name,
            data: data.to_string(),
        });
    }
}

#[derive(Default)]
struct Shelf {
    /// Ordered by id.
    jobs: BTreeMap<u64, Job>,
    /// Idempotency key to the job it made and what was asked for.
    keys: HashMap<String, (u64, Spec)>,
    clock: u64,
}

impl Shelf {
    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }
}

/// All jobs. Clones are handles to the same store.
#[derive(Clone, Default)]
pub struct JobStore {
    state: Arc<Mutex<Shelf>>,
}

impl JobStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, Shelf> {
        // Each change is complete before the lock is released, so a poisoned lock is still usable.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The new job and `true`, or the job this key made before and `false`.
    pub fn submit(&self, key: &str, spec: Spec) -> Result<(JobView, bool), ApiError> {
        // One lock for the look-up and the insert: two racing retries cannot both create.
        let mut state = self.lock();
        if let Some((id, asked)) = state.keys.get(key) {
            if *asked != spec {
                return Err(ApiError::KeyReuse);
            }
            return Ok((state.jobs[id].view.clone(), false));
        }
        let id = state.jobs.len() as u64 + 1;
        let now = state.tick();
        let mut job = Job {
            view: JobView {
                id,
                status: Status::Queued,
                source: spec.source.clone(),
                width: spec.width,
                height: spec.height,
                format: spec.format.clone(),
                created_at: now,
                updated_at: now,
                output: None,
                error: None,
            },
            log: Vec::new(),
        };
        job.record(Status::Queued, now);
        let view = job.view.clone();
        state.jobs.insert(id, job);
        state.keys.insert(key.to_string(), (id, spec));
        Ok((view, true))
    }

    pub fn get(&self, id: u64) -> Option<JobView> {
        self.lock().jobs.get(&id).map(|j| j.view.clone())
    }

    /// Queued to running. `false`, with nothing changed, for any other job.
    pub fn start(&self, id: u64) -> bool {
        let mut state = self.lock();
        let now = state.clock + 1;
        match state.jobs.get_mut(&id) {
            Some(job) if job.view.status == Status::Queued => {
                job.record(Status::Running, now);
            }
            _ => return false,
        }
        state.clock = now;
        true
    }

    pub fn finish(&self, id: u64, outcome: Outcome) -> bool {
        let mut state = self.lock();
        let now = state.clock + 1;
        let Some(job) = state.jobs.get_mut(&id) else {
            return false;
        };
        let allowed = match job.view.status {
            Status::Running => true,
            Status::Queued => outcome == Outcome::Cancelled,
            _ => false,
        };
        if !allowed {
            return false;
        }
        let status = match outcome {
            Outcome::Succeeded(output) => {
                job.view.output = Some(output);
                Status::Succeeded
            }
            Outcome::Failed(error) => {
                job.view.error = Some(error);
                Status::Failed
            }
            Outcome::Cancelled => Status::Cancelled,
        };
        job.record(status, now);
        state.clock = now;
        true
    }

    /// Up to `limit` jobs with ids above `after`, and where the next page starts if there is one.
    fn list(&self, after: u64, limit: usize) -> (Vec<JobView>, Option<u64>) {
        let state = self.lock();
        // `Excluded(after)`, not `after + 1`, which overflows for the largest id.
        let rows = state.jobs.range((Bound::Excluded(after), Bound::Unbounded));
        let mut items: Vec<JobView> = rows.take(limit + 1).map(|(_, j)| j.view.clone()).collect();
        let more = items.len() > limit;
        items.truncate(limit);
        let next = items.last().filter(|_| more).map(|j| j.id);
        (items, next)
    }

    /// The history entries with an id above `after`.
    pub fn events(&self, id: u64, after: u64) -> Option<Vec<LogEntry>> {
        let state = self.lock();
        let job = state.jobs.get(&id)?;
        Some(job.log.iter().filter(|e| e.id > after).cloned().collect())
    }
}

// ---------------------------------------------------------------- the HTTP layer

pub fn router(store: JobStore) -> Router {
    Router::new()
        .route("/v1/jobs", post(create_job).get(list_jobs))
        .route("/v1/jobs/{id}", get(get_job))
        .fallback(|| async { ApiError::NoRoute })
        .method_not_allowed_fallback(|| async { ApiError::MethodNotAllowed })
        .with_state(store)
}

fn idempotency_key(headers: &HeaderMap) -> Result<&str, ApiError> {
    headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .filter(|key| !key.is_empty())
        .ok_or(ApiError::MissingKey)
}

/// 201 with the job's address for a new job; 200 and a marker for a repeat.
fn created_reply(view: JobView, created: bool) -> Response {
    if created {
        let location = [("location", format!("/v1/jobs/{}", view.id))];
        (StatusCode::CREATED, location, Json(view)).into_response()
    } else {
        let replayed = [("idempotent-replayed", "true")];
        (StatusCode::OK, replayed, Json(view)).into_response()
    }
}

async fn create_job(
    State(store): State<JobStore>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let spec = parse_spec(&body)?;
    let key = idempotency_key(&headers)?;
    let (view, created) = store.submit(key, spec)?;
    Ok(created_reply(view, created))
}

/// The raw text and the id. A segment axum cannot decode, such as `%FF`, names no job either.
fn job_id(path: Result<Path<String>, PathRejection>) -> Result<(String, u64), ApiError> {
    let Path(raw) = path.map_err(|rejection| ApiError::NotFound(rejection.body_text()))?;
    match raw.parse() {
        Ok(id) => Ok((raw, id)),
        Err(_) => Err(ApiError::NotFound(raw)),
    }
}

async fn get_job(
    State(store): State<JobStore>,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<JobView>, ApiError> {
    let (raw, id) = job_id(path)?;
    store.get(id).map(Json).ok_or(ApiError::NotFound(raw))
}

fn number(query: &HashMap<String, String>, name: &str, default: u64) -> Result<u64, ApiError> {
    match query.get(name) {
        None => Ok(default),
        Some(text) => text
            .parse()
            .map_err(|_| ApiError::BadQuery(format!("{name}={text}"))),
    }
}

async fn list_jobs(
    State(store): State<JobStore>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = number(&query, "limit", 10)?.clamp(1, 100) as usize;
    let after = number(&query, "after", 0)?;
    let (items, next_after) = store.list(after, limit);
    Ok(Json(json!({"items": items, "next_after": next_after})))
}
