//! CAPSTONE 06 (axum): an in-process image-resize job service (idempotent API, workers, SSE, graceful shutdown)
//! LEVEL: capstone
//! Use in: any "submit work over HTTP, run it in the background, watch it" service.   Run: cargo run --release --example job_service_axum
#![allow(dead_code)]
//!
//! WHAT: a small job service for image-resize jobs. Clients POST a job, poll it, list jobs page by page, cancel it,
//! and replay its event log over Server-Sent Events. Everything runs in-process and is driven through
//! `tower::ServiceExt::oneshot`, so there are no sockets and the example is fast and deterministic.
//! WHY: it combines recipes axum_router_state (router state), axum_errors (error type), axum_sse_replay (SSE replay), axum_middleware (middleware), axum_idempotency
//! (idempotency) and worker_pool (worker pool) into the shape real services have.
//! HOW:
//!  * `App` is cheap-to-clone shared state (`Arc<Inner>`): job store behind a mutex (it also holds a logical clock, a
//!    counter, so timestamps are deterministic, and the "closed" flag), a `Semaphore` that bounds concurrency, and a
//!    `watch` count of unfinished jobs.
//!  * Each job gets one tokio task. It waits for a permit (or for cancellation), runs the resize in a CHILD task so a
//!    panic surfaces as a `JoinError` and becomes `failed`, and `select!`s on a `watch` channel for cancellation.
//!    An accepted cancel always wins: the job ends `cancelled`, a job still queued never starts, and an aborted child
//!    is awaited before the job is reported and its slot is freed.
//!  * Every state change appends to a per-job event log (ids 1, 2, 3, ...). SSE replays the entries after
//!    `Last-Event-ID` and then ends the response. It does NOT follow a running job: a client must reconnect.
//!  * `ApiError` gives every error, including unknown routes, wrong methods and extractor rejections on these routes,
//!    one JSON shape with a stable machine-readable `code`.
//!  * Shutdown closes the store (a new key gets 503, a retry of a known key still gets its job), waits until no job is
//!    unfinished and reports counts. It only watches a counter, so any number of callers may wait, and dropping the
//!    future (say, after a timeout) stops no job.
//! COMMON USES: thumbnail/transcode pipelines, report generation, webhook delivery, import/export jobs, CI-style
//! runners; patterns: idempotency keys for safe retries, cursor pagination, outbox/event-log replay, bulkheads
//! (semaphore), panic isolation, cooperative cancellation, drain before exit (no signal handling here).
//! Read: https://docs.rs/axum/latest/axum/ https://tokio.rs/tokio/tutorial https://stripe.com/blog/idempotency
use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{
        Path, Query, Request, State,
        rejection::{BytesRejection, PathRejection},
    },
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
    routing::{get, post},
};
use futures_util::{
    FutureExt,
    stream::{self, Stream},
};
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap},
    convert::Infallible,
    ops::Bound,
    sync::{
        Arc, Mutex, MutexGuard, OnceLock,
        atomic::{AtomicU64, AtomicUsize, Ordering::SeqCst},
    },
    time::Duration,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, watch};
use tower::ServiceExt;
use tracing::Instrument;

// ---------------------------------------------------------------- errors

/// One error type for the whole API. Handlers return `Result<_, ApiError>`; `?` does the rest.
#[derive(Debug, thiserror::Error)]
enum ApiError {
    #[error("the Idempotency-Key header is required for POST /v1/jobs")]
    MissingKey,
    #[error("request body is not valid JSON for a job: {0}")]
    InvalidJson(String),
    #[error("job specification failed validation")]
    Validation(Vec<String>),
    #[error("this Idempotency-Key was already used for a different job")]
    KeyReuse,
    #[error("job {0} does not exist")]
    NotFound(String),
    #[error("job is already {0} and cannot be cancelled")]
    NotCancellable(&'static str),
    #[error("invalid query parameter: {0}")]
    BadQuery(String),
    #[error("service is shutting down and not accepting new jobs")]
    ShuttingDown,
    #[error("no such route")]
    NoRoute,
    #[error("method not allowed")]
    MethodNotAllowed,
    #[error("cannot read the request body: {1}")]
    BadBody(StatusCode, String),
}

impl ApiError {
    /// (HTTP status, stable code). Clients switch on the code; the message is for humans and may change.
    fn parts(&self) -> (StatusCode, &'static str) {
        match self {
            Self::MissingKey => (StatusCode::BAD_REQUEST, "missing_idempotency_key"),
            Self::InvalidJson(_) => (StatusCode::UNPROCESSABLE_ENTITY, "invalid_json"),
            Self::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "validation_failed"),
            Self::KeyReuse => (StatusCode::CONFLICT, "idempotency_key_reuse"),
            Self::NotFound(_) => (StatusCode::NOT_FOUND, "job_not_found"),
            Self::NotCancellable(_) => (StatusCode::CONFLICT, "not_cancellable"),
            Self::BadQuery(_) => (StatusCode::BAD_REQUEST, "bad_query"),
            Self::ShuttingDown => (StatusCode::SERVICE_UNAVAILABLE, "shutting_down"),
            Self::NoRoute => (StatusCode::NOT_FOUND, "no_route"),
            Self::MethodNotAllowed => (StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed"),
            Self::BadBody(status, _) => (*status, "bad_body"), // 413 when the body is too large
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = self.parts();
        let details = match &self {
            Self::Validation(d) => json!(d),
            _ => json!([]),
        };
        let body =
            json!({"error": {"code": code, "message": self.to_string(), "details": details}});
        (status, Json(body)).into_response()
    }
}

// ---------------------------------------------------------------- domain

fn default_format() -> String {
    "png".into()
}
fn default_work_ms() -> u64 {
    20
}

/// What the client asked for. `work_ms` simulates how long the resize takes.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    source: String,
    width: u32,
    height: u32,
    #[serde(default = "default_format")]
    format: String,
    #[serde(default = "default_work_ms")]
    work_ms: u64,
}

impl Spec {
    /// Collect ALL problems so the client can fix them in one round trip.
    fn validate(&self) -> Result<(), Vec<String>> {
        let mut problems = vec![];
        if self.source.trim().is_empty() {
            problems.push("source must not be empty".to_string());
        }
        for (name, v) in [("width", self.width), ("height", self.height)] {
            if !(1..=8192).contains(&v) {
                problems.push(format!("{name} must be between 1 and 8192, got {v}"));
            }
        }
        if !["png", "jpg", "webp"].contains(&self.format.as_str()) {
            problems.push(format!(
                "format must be png, jpg or webp, got {}",
                self.format
            ));
        }
        if self.work_ms > 10_000 {
            problems.push("work_ms must be at most 10000".to_string());
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Status {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl Status {
    fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
    fn label(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

struct LogEvent {
    id: u64,
    name: &'static str,
    data: String,
}

struct Job {
    id: u64,
    spec: Spec,
    status: Status,
    created_at: u64,
    updated_at: u64,
    output: Option<String>,
    error: Option<String>,
    events: Vec<LogEvent>,
    cancel: watch::Sender<bool>,
    /// The work future holds a clone while it is alive.
    work_alive: Arc<()>,
    /// Set if the job became final while its work future still existed. It must stay false.
    outlived_by_work: bool,
}

impl Job {
    /// Every transition goes through here, so status, timestamp and event log can never disagree.
    fn record(&mut self, status: Status, name: &'static str, now: u64) {
        self.status = status;
        self.updated_at = now;
        let id = self.events.len() as u64 + 1; // ids are per job and strictly increasing
        let data = json!({"job_id": self.id, "status": status.label()}).to_string();
        self.events.push(LogEvent { id, name, data });
    }

    fn view(&self) -> Value {
        json!({
            "id": self.id, "status": self.status, "source": self.spec.source,
            "width": self.spec.width, "height": self.spec.height, "format": self.spec.format,
            "created_at": self.created_at, "updated_at": self.updated_at,
            "output": self.output, "error": self.error,
        })
    }
}

#[derive(Default)]
struct Store {
    jobs: BTreeMap<u64, Job>, // BTreeMap: iteration order == id order, which makes pagination stable
    keys: HashMap<String, u64>, // Idempotency-Key -> job id
    next_id: u64,
    clock: u64,   // in the store, so time order is the order in which changes were made
    closed: bool, // in the store, so closing and creating cannot overlap
}

impl Store {
    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    /// Create the job, or return the one this key already made. The `bool` says whether it is new.
    /// `&mut self` means the caller holds the lock: the key look-up, the closed check and the insert are one step.
    /// `admitted` runs right after a new job has passed the closed check.
    fn submit(
        &mut self,
        key: &str,
        spec: Spec,
        admitted: impl FnOnce(),
    ) -> Result<(u64, bool), ApiError> {
        // The retry check comes first: a client retrying an accepted job gets it back, even during shutdown.
        if let Some(id) = self.keys.get(key).copied() {
            return if self.jobs[&id].spec == spec {
                Ok((id, false))
            } else {
                Err(ApiError::KeyReuse)
            };
        }
        if self.closed {
            return Err(ApiError::ShuttingDown);
        }
        admitted();
        self.next_id += 1;
        let id = self.next_id;
        let now = self.tick();
        let mut job = Job {
            id,
            spec,
            status: Status::Queued,
            created_at: now,
            updated_at: now,
            output: None,
            error: None,
            events: vec![],
            cancel: watch::Sender::new(false),
            work_alive: Arc::new(()),
            outlived_by_work: false,
        };
        job.record(Status::Queued, "job.queued", now);
        self.jobs.insert(id, job);
        self.keys.insert(key.to_string(), id);
        Ok((id, true))
    }
}

#[derive(Debug, PartialEq)]
struct ShutdownReport {
    total: usize,
    succeeded: usize,
    failed: usize,
    cancelled: usize,
    unfinished: usize,
}

// ---------------------------------------------------------------- shared state and workers

struct Inner {
    store: Mutex<Store>,
    permits: Arc<Semaphore>,
    unfinished: watch::Sender<usize>, // jobs not yet final; changed only while the store is locked
    on_admit: OnceLock<Box<dyn Fn() + Send + Sync>>, // only the edge checks set it, to pause a submit
    in_flight: AtomicUsize,
    max_in_flight: AtomicUsize,
}

#[derive(Clone)]
struct App {
    inner: Arc<Inner>,
}

/// RAII marker for "a job is executing now"; the decrement also happens if the task is dropped.
struct FlightGuard(Arc<Inner>);

impl Drop for FlightGuard {
    fn drop(&mut self) {
        self.0.in_flight.fetch_sub(1, SeqCst);
    }
}

/// What a running job occupies. `finish` frees it in the same step that makes the job final.
struct Slot {
    _flight: FlightGuard,
    _permit: OwnedSemaphorePermit,
}

impl App {
    fn new(max_concurrent: usize) -> Self {
        assert!(max_concurrent > 0, "with no slot, no job could ever run");
        let inner = Inner {
            store: Mutex::default(),
            permits: Arc::new(Semaphore::new(max_concurrent)),
            unfinished: watch::Sender::new(0),
            on_admit: OnceLock::new(),
            in_flight: AtomicUsize::new(0),
            max_in_flight: AtomicUsize::new(0),
        };
        App {
            inner: Arc::new(inner),
        }
    }

    /// A poisoned lock only means some thread panicked while holding it; no change is left half done by a panic we
    /// expect, so keep going.
    fn store(&self) -> MutexGuard<'_, Store> {
        self.inner.store.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Create the job (or return the one this key already made) and start its worker.
    /// Everything is decided under one lock: two racing retries cannot both create a job, and no job can appear
    /// after `shutdown` has found nothing left to wait for.
    fn submit(&self, key: &str, spec: Spec) -> Result<(Value, bool), ApiError> {
        let mut store = self.store();
        let (id, created) = store.submit(key, spec, || {
            if let Some(hook) = self.inner.on_admit.get() {
                hook();
            }
        })?;
        let job = &store.jobs[&id];
        let view = job.view();
        let cancel = job.cancel.subscribe();
        if created {
            self.inner.unfinished.send_modify(|n| *n += 1);
        }
        drop(store);
        if created {
            // Nobody owns the handle: the counter is how `shutdown` waits, so nothing it does can abort a worker.
            tokio::spawn(run_job(self.clone(), id, cancel));
        }
        Ok((view, created))
    }

    /// Queued to running, plus a token for the work future. `None` once a cancel has been accepted: the check and
    /// the change happen under the lock that `cancel_job` holds, so no cancel can slip in between.
    fn mark_running(&self, id: u64) -> Option<(Spec, Arc<()>)> {
        let mut store = self.store();
        let job = &store.jobs[&id];
        if job.status != Status::Queued || *job.cancel.borrow() {
            return None; // a refusal changes nothing, the clock included
        }
        let now = store.tick();
        let job = store.jobs.get_mut(&id).expect("job exists");
        job.record(Status::Running, "job.started", now);
        Some((job.spec.clone(), job.work_alive.clone()))
    }

    /// Make the job final, once. An accepted cancel wins, even over work that finished before the worker saw it.
    fn finish(&self, id: u64, outcome: Result<String, String>, slot: Option<Slot>) {
        let mut store = self.store();
        if store.jobs[&id].status.is_terminal() {
            return;
        }
        let now = store.tick();
        let job = store.jobs.get_mut(&id).expect("job exists");
        let (status, name) = if *job.cancel.borrow() {
            (Status::Cancelled, "job.cancelled")
        } else {
            match outcome {
                Ok(output) => {
                    job.output = Some(output);
                    (Status::Succeeded, "job.succeeded")
                }
                Err(error) => {
                    job.error = Some(error);
                    (Status::Failed, "job.failed")
                }
            }
        };
        job.outlived_by_work = Arc::strong_count(&job.work_alive) > 1;
        job.record(status, name, now);
        drop(slot); // still locked: nobody can see a final job that holds a slot
        self.inner.unfinished.send_modify(|n| *n -= 1);
    }

    fn enter_flight(&self) -> FlightGuard {
        let now = self.inner.in_flight.fetch_add(1, SeqCst) + 1;
        self.inner.max_in_flight.fetch_max(now, SeqCst);
        FlightGuard(self.inner.clone())
    }

    /// Stop accepting new jobs, wait until every accepted job is final (queued and running), then count outcomes.
    /// Cancel-safe: giving up on this future stops no job (the service stays closed), and it may be called again.
    async fn shutdown(&self) -> ShutdownReport {
        self.store().closed = true;
        let mut unfinished = self.inner.unfinished.subscribe();
        let _ = unfinished.wait_for(|n| *n == 0).await; // the sender lives in `self`, so this cannot fail
        let store = self.store();
        let count = |s: Status| store.jobs.values().filter(|j| j.status == s).count();
        let unfinished = store
            .jobs
            .values()
            .filter(|j| !j.status.is_terminal())
            .count();
        ShutdownReport {
            total: store.jobs.len(),
            succeeded: count(Status::Succeeded),
            failed: count(Status::Failed),
            cancelled: count(Status::Cancelled),
            unfinished,
        }
    }
}

/// Resolves when cancellation was requested. `let _` drops the watch guard immediately (never held across an await).
async fn cancelled(rx: &mut watch::Receiver<bool>) {
    let _ = rx.wait_for(|c| *c).await;
}

async fn run_job(app: App, id: u64, mut cancel: watch::Receiver<bool>) {
    // Waiting for a permit is cancellable too. `biased` looks at the cancel first.
    let permit = tokio::select! {
        biased;
        _ = cancelled(&mut cancel) => None,
        p = app.inner.permits.clone().acquire_owned() => Some(p.expect("semaphore is never closed")),
    };
    let Some(permit) = permit else {
        app.finish(id, Err("cancelled".into()), None); // the accepted cancel decides the status
        return;
    };
    // A cancel may have arrived since the select; `mark_running` has the last word.
    let Some((spec, alive)) = app.mark_running(id) else {
        app.finish(id, Err("cancelled".into()), None);
        return;
    };
    let slot = Slot {
        _flight: app.enter_flight(),
        _permit: permit,
    };
    tracing::info!(job = id, source = %spec.source, "job started");
    // The resize runs in a CHILD task: if it panics, only the child dies and we see a JoinError here.
    let mut work = tokio::spawn(async move {
        let _alive = alive;
        resize(spec).await
    });
    let joined = tokio::select! {
        biased;
        _ = cancelled(&mut cancel) => {
            work.abort(); // cooperative: stops at the child's next .await point...
            (&mut work).await // ...so wait until it really is gone before reporting and freeing the slot
        }
        joined = &mut work => joined,
    };
    let outcome = joined.unwrap_or_else(|e| Err(format!("worker panicked: {}", panic_message(e))));
    app.finish(id, outcome, Some(slot));
    tracing::info!(job = id, "job finished");
}

fn panic_message(e: tokio::task::JoinError) -> String {
    match e.try_into_panic() {
        Ok(p) => p
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "unknown panic".into()),
        Err(e) => e.to_string(), // not a panic: the task was aborted
    }
}

/// The "image work". Magic source names let tests trigger failures deterministically.
async fn resize(spec: Spec) -> Result<String, String> {
    tokio::time::sleep(Duration::from_millis(spec.work_ms)).await;
    if spec.source.starts_with("panic") {
        panic!("decoder crashed on {}", spec.source);
    }
    if spec.source.ends_with(".corrupt") {
        return Err(format!("{} is not a valid image", spec.source));
    }
    Ok(format!(
        "{}_{}x{}.{}",
        spec.source, spec.width, spec.height, spec.format
    ))
}

// ---------------------------------------------------------------- HTTP layer

fn router(app: App) -> Router {
    Router::new()
        .route("/v1/jobs", post(create_job).get(list_jobs))
        .route("/v1/jobs/{id}", get(get_job))
        .route("/v1/jobs/{id}/cancel", post(cancel_job))
        .route("/v1/jobs/{id}/events", get(job_events))
        // Without these two, axum answers 404 and 405 with an empty body. They come before `layer`, which wraps them.
        .fallback(|| async { ApiError::NoRoute })
        .method_not_allowed_fallback(|| async { ApiError::MethodNotAllowed })
        .layer(middleware::from_fn(request_id))
        .with_state(app)
}

/// Echo a sane client-supplied `x-request-id`, otherwise generate one; log every request inside a span carrying it.
async fn request_id(req: Request, next: Next) -> Response {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_graphic()))
        .map(str::to_owned)
        .unwrap_or_else(|| format!("req-{:06}", NEXT.fetch_add(1, SeqCst) + 1));
    let span = tracing::info_span!("request", request_id = %id, method = %req.method(), path = %req.uri().path());
    let mut resp = next.run(req).instrument(span.clone()).await;
    span.in_scope(|| tracing::info!(status = resp.status().as_u16(), "request done"));
    resp.headers_mut().insert(
        "x-request-id",
        HeaderValue::from_str(&id).expect("validated ascii"),
    );
    resp
}

async fn create_job(
    State(app): State<App>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>, // taken as a Result so that a rejection also gets our error shape
) -> Result<Response, ApiError> {
    let key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .filter(|k| !k.is_empty());
    let key = key.ok_or(ApiError::MissingKey)?;
    let body = body.map_err(|e| ApiError::BadBody(e.status(), e.body_text()))?;
    // serde also reads a struct from an array, such as `["a.png", 640, 480]`, so ask for an object first.
    if body.iter().find(|byte| !b" \t\r\n".contains(byte)) != Some(&b'{') {
        return Err(ApiError::InvalidJson("expected a JSON object".into()));
    }
    let spec: Spec =
        serde_json::from_slice(&body).map_err(|e| ApiError::InvalidJson(e.to_string()))?;
    spec.validate().map_err(ApiError::Validation)?;
    let (view, created) = app.submit(key, spec)?;
    if created {
        let location = format!("/v1/jobs/{}", view["id"]);
        Ok((StatusCode::CREATED, [("location", location)], Json(view)).into_response())
    } else {
        Ok((
            StatusCode::OK,
            [("idempotent-replayed", "true")],
            Json(view),
        )
            .into_response())
    }
}

/// The raw text and the id. A segment axum cannot decode (`%FF`) is just another id that names no job.
fn parse_id(path: Result<Path<String>, PathRejection>) -> Result<(String, u64), ApiError> {
    let Path(raw) = path.map_err(|_| ApiError::NotFound("with an unreadable id".into()))?;
    match raw.parse() {
        Ok(id) => Ok((raw, id)),
        Err(_) => Err(ApiError::NotFound(raw)),
    }
}

async fn get_job(
    State(app): State<App>,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<Value>, ApiError> {
    let (raw, id) = parse_id(path)?;
    let store = app.store();
    store
        .jobs
        .get(&id)
        .map(|j| Json(j.view()))
        .ok_or(ApiError::NotFound(raw))
}

/// Cursor pagination: `after` is the last id the client has seen. Unlike offsets, it stays correct while new jobs arrive.
async fn list_jobs(
    State(app): State<App>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let num = |name: &str, default: u64| -> Result<u64, ApiError> {
        q.get(name).map_or(Ok(default), |v| {
            v.parse()
                .map_err(|_| ApiError::BadQuery(format!("{name}={v}")))
        })
    };
    let limit = num("limit", 10)?.clamp(1, 100) as usize;
    let after = num("after", 0)?;
    let store = app.store();
    // `Excluded(after)`, not `after + 1`: that overflows for `after=u64::MAX`.
    let mut page: Vec<&Job> = store
        .jobs
        .range((Bound::Excluded(after), Bound::Unbounded))
        .map(|(_, j)| j)
        .take(limit + 1)
        .collect();
    let has_more = page.len() > limit;
    page.truncate(limit); // we fetched one extra row just to learn whether another page exists
    let next_after = if has_more {
        page.last().map(|j| j.id)
    } else {
        None
    };
    Ok(Json(
        json!({"items": page.iter().map(|j| j.view()).collect::<Vec<_>>(), "next_after": next_after}),
    ))
}

async fn cancel_job(
    State(app): State<App>,
    path: Result<Path<String>, PathRejection>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let (raw, id) = parse_id(path)?;
    let store = app.store();
    let job = store.jobs.get(&id).ok_or(ApiError::NotFound(raw))?;
    if job.status.is_terminal() {
        return Err(ApiError::NotCancellable(job.status.label()));
    }
    job.cancel.send_replace(true);
    // 202: cancellation is requested, the worker applies it asynchronously; clients poll or watch the events.
    Ok((StatusCode::ACCEPTED, Json(job.view())))
}

async fn job_events(
    State(app): State<App>,
    path: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let (raw, id) = parse_id(path)?;
    let last: u64 = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let store = app.store();
    let job = store.jobs.get(&id).ok_or(ApiError::NotFound(raw))?;
    let replay: Vec<_> = job
        .events
        .iter()
        .filter(|e| e.id > last)
        .map(|e| {
            Ok(Event::default()
                .id(e.id.to_string())
                .event(e.name)
                .data(e.data.clone()))
        })
        .collect();
    // A replay of what exists now: the response then ends, even if the job is still running.
    Ok(Sse::new(stream::iter(replay)))
}

// ---------------------------------------------------------------- test client helpers

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).expect("JSON body")
    }
    fn error_code(&self) -> String {
        self.json()["error"]["code"]
            .as_str()
            .expect("error code")
            .to_string()
    }
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Reply {
    let mut b = axum::http::Request::builder().method(method).uri(uri);
    for (k, v) in headers {
        b = b.header(*k, *v);
    }
    if !body.is_empty() {
        b = b.header("content-type", "application/json");
    }
    let resp = app
        .clone()
        .oneshot(b.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let (status, headers) = (resp.status(), resp.headers().clone());
    let body = String::from_utf8(
        resp.into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    let shown: String = body.replace('\n', " | ").chars().take(110).collect();
    println!("  {method:<4} {uri:<24} -> {status} {shown}");
    Reply {
        status,
        headers,
        body,
    }
}

fn spec_json(source: &str, work_ms: u64) -> String {
    json!({"source": source, "width": 640, "height": 480, "format": "webp", "work_ms": work_ms})
        .to_string()
}

async fn create(app: &Router, key: &str, body: &str) -> Reply {
    call(app, "POST", "/v1/jobs", &[("idempotency-key", key)], body).await
}

/// Poll until the job reaches `want` (generous 10 s margin; normally takes milliseconds).
async fn wait_status(app: &Router, id: u64, want: &str) -> Value {
    for _ in 0..1000 {
        let resp = app
            .clone()
            .oneshot(get_req(&format!("/v1/jobs/{id}")))
            .await
            .unwrap();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let v: Value = serde_json::from_slice(&bytes).unwrap();
        if v["status"] == want {
            return v;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("job {id} never reached {want}");
}

fn get_req(uri: &str) -> axum::http::Request<Body> {
    axum::http::Request::builder()
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

async fn wait_all_terminal(app: &App) {
    for _ in 0..1000 {
        if app.store().jobs.values().all(|j| j.status.is_terminal()) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("jobs never finished");
}

// ---------------------------------------------------------------- main: the scenarios

fn main() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime.block_on(scenarios());
    regressions();
    println!("all c06 job-service checks passed");
}

async fn scenarios() {
    // Logs go to stderr so the stdout transcript stays readable; try_init because a host may have set one already.
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_writer(std::io::stderr)
        .try_init();
    // Deliberate panics are part of the demo; report them through tracing instead of the default noisy hook.
    // Only the deliberate ones: any other panic, such as a failed assert, keeps the default report.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let text = info.payload().downcast_ref::<String>();
        if text.is_some_and(|t| t.starts_with("decoder crashed")) {
            tracing::warn!("panic caught by worker isolation: {info}")
        } else {
            default_hook(info)
        }
    }));

    let state = App::new(3);
    let app = router(state.clone());

    println!("== create, idempotent replay, errors");
    let first = create(&app, "key-1", &spec_json("cat.png", 30)).await;
    assert_eq!(first.status, StatusCode::CREATED);
    let id1 = first.json()["id"].as_u64().unwrap();
    assert_eq!(
        first.headers["location"],
        format!("/v1/jobs/{id1}").as_str()
    );
    let replay = create(&app, "key-1", &spec_json("cat.png", 30)).await;
    assert_eq!(replay.status, StatusCode::OK);
    assert_eq!(replay.headers["idempotent-replayed"], "true");
    assert_eq!(replay.json()["id"].as_u64(), Some(id1));
    assert_eq!(
        state.store().jobs.len(),
        1,
        "a retry must not create a second job"
    );
    let reuse = create(&app, "key-1", &spec_json("dog.png", 30)).await;
    assert_eq!(
        (reuse.status, reuse.error_code().as_str()),
        (StatusCode::CONFLICT, "idempotency_key_reuse")
    );
    let nokey = call(&app, "POST", "/v1/jobs", &[], &spec_json("cat.png", 30)).await;
    assert_eq!(
        (nokey.status, nokey.error_code().as_str()),
        (StatusCode::BAD_REQUEST, "missing_idempotency_key")
    );
    let bad = create(
        &app,
        "key-bad",
        r#"{"source":"","width":0,"height":9000,"format":"gif"}"#,
    )
    .await;
    assert_eq!(
        (bad.status, bad.error_code().as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, "validation_failed")
    );
    assert_eq!(
        bad.json()["error"]["details"].as_array().unwrap().len(),
        4,
        "all problems reported at once"
    );
    let junk = create(&app, "key-junk", "{not json").await;
    assert_eq!(
        (junk.status, junk.error_code().as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, "invalid_json")
    );
    assert_eq!(
        state.store().jobs.len(),
        1,
        "invalid requests must not leave jobs behind"
    );

    println!("== request ids");
    let echoed = call(
        &app,
        "GET",
        "/v1/jobs/999",
        &[("x-request-id", "trace-abc")],
        "",
    )
    .await;
    assert_eq!(echoed.headers["x-request-id"], "trace-abc");
    assert_eq!(echoed.error_code(), "job_not_found");
    assert!(echoed.headers["x-request-id"] != "", "echoed");
    let generated = call(&app, "GET", "/v1/jobs/not-a-number", &[], "").await;
    assert!(
        generated.headers["x-request-id"]
            .to_str()
            .unwrap()
            .starts_with("req-")
    );

    println!("== get a job through its lifecycle");
    let done = wait_status(&app, id1, "succeeded").await;
    assert_eq!(done["output"], "cat.png_640x480.webp");
    assert!(
        done["created_at"].as_u64() < done["updated_at"].as_u64(),
        "logical clock moves forward"
    );
    let got = call(&app, "GET", &format!("/v1/jobs/{id1}"), &[], "").await;
    assert_eq!(got.json()["status"], "succeeded");

    println!("== pagination across pages (stable order)");
    for n in 0..6 {
        let r = create(
            &app,
            &format!("page-{n}"),
            &spec_json(&format!("img{n}.png"), 5),
        )
        .await;
        assert_eq!(r.status, StatusCode::CREATED);
    }
    let (mut seen, mut after, mut pages) = (vec![], 0u64, 0);
    loop {
        let uri = format!("/v1/jobs?limit=3&after={after}");
        let page = call(&app, "GET", &uri, &[], "").await.json();
        pages += 1;
        seen.extend(
            page["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|j| j["id"].as_u64().unwrap()),
        );
        match page["next_after"].as_u64() {
            Some(next) => after = next,
            None => break,
        }
    }
    assert_eq!((pages, seen.len()), (3, 7), "7 jobs, 3 per page");
    assert!(
        seen.windows(2).all(|w| w[0] < w[1]),
        "ids strictly increasing, no duplicates, no gaps"
    );
    let badq = call(&app, "GET", "/v1/jobs?limit=abc", &[], "").await;
    assert_eq!(badq.error_code(), "bad_query");
    wait_all_terminal(&state).await;

    println!("== cancel a running job, then cancel a finished one");
    let long = create(&app, "long", &spec_json("huge.png", 5_000))
        .await
        .json()["id"]
        .as_u64()
        .unwrap();
    wait_status(&app, long, "running").await;
    let cancel = call(&app, "POST", &format!("/v1/jobs/{long}/cancel"), &[], "").await;
    assert_eq!(cancel.status, StatusCode::ACCEPTED);
    wait_status(&app, long, "cancelled").await;
    let again = call(&app, "POST", &format!("/v1/jobs/{long}/cancel"), &[], "").await;
    assert_eq!(
        (again.status, again.error_code().as_str()),
        (StatusCode::CONFLICT, "not_cancellable")
    );
    let finished = call(&app, "POST", &format!("/v1/jobs/{id1}/cancel"), &[], "").await;
    assert_eq!(
        (finished.status, finished.error_code().as_str()),
        (StatusCode::CONFLICT, "not_cancellable")
    );
    assert_eq!(
        state.inner.in_flight.load(SeqCst),
        0,
        "cancelled job released its slot"
    );

    println!("== SSE replay with and without Last-Event-ID");
    let full = call(&app, "GET", &format!("/v1/jobs/{id1}/events"), &[], "").await;
    assert_eq!(full.headers["content-type"], "text/event-stream");
    for needle in [
        "id: 1",
        "event: job.queued",
        "id: 2",
        "event: job.started",
        "id: 3",
        "event: job.succeeded",
    ] {
        assert!(full.body.contains(needle), "missing {needle}");
    }
    let resumed = call(
        &app,
        "GET",
        &format!("/v1/jobs/{id1}/events"),
        &[("last-event-id", "2")],
        "",
    )
    .await;
    assert!(!resumed.body.contains("id: 1") && !resumed.body.contains("id: 2"));
    assert!(resumed.body.contains("id: 3") && resumed.body.contains("job.succeeded"));
    let cancelled_log = call(&app, "GET", &format!("/v1/jobs/{long}/events"), &[], "").await;
    assert!(cancelled_log.body.contains("event: job.cancelled"));
    assert_eq!(
        call(&app, "GET", "/v1/jobs/4242/events", &[], "")
            .await
            .error_code(),
        "job_not_found"
    );

    println!("== a panicking job fails; the others still succeed");
    let ids: Vec<u64> = {
        let mut v = vec![];
        for (k, src) in [
            ("p-a", "ok-a.png"),
            ("p-b", "panic-me.png"),
            ("p-c", "ok-c.png"),
            ("p-d", "bad.corrupt"),
        ] {
            v.push(
                create(&app, k, &spec_json(src, 10)).await.json()["id"]
                    .as_u64()
                    .unwrap(),
            );
        }
        v
    };
    let outcomes = [
        wait_status(&app, ids[0], "succeeded").await,
        wait_status(&app, ids[1], "failed").await,
        wait_status(&app, ids[2], "succeeded").await,
        wait_status(&app, ids[3], "failed").await,
    ];
    assert!(
        outcomes[1]["error"]
            .as_str()
            .unwrap()
            .contains("decoder crashed on panic-me.png")
    );
    assert!(
        outcomes[3]["error"]
            .as_str()
            .unwrap()
            .contains("not a valid image")
    );
    let after_panic = create(&app, "after-panic", &spec_json("fine.png", 5))
        .await
        .json()["id"]
        .as_u64()
        .unwrap();
    wait_status(&app, after_panic, "succeeded").await; // the service (and its permits) survived

    println!("== concurrency never exceeds the limit (limit = 2, 8 jobs)");
    let limited = App::new(2);
    let limited_router = router(limited.clone());
    for n in 0..8 {
        create(
            &limited_router,
            &format!("c-{n}"),
            &spec_json("burst.png", 60),
        )
        .await;
    }
    wait_all_terminal(&limited).await;
    let peak = limited.inner.max_in_flight.load(SeqCst);
    println!("  peak in flight: {peak}");
    assert!(
        (1..=2).contains(&peak),
        "peak {peak} exceeded the limit of 2"
    );
    assert_eq!(
        limited.inner.permits.available_permits(),
        2,
        "all permits returned"
    );

    println!("== graceful shutdown drains running jobs");
    let draining = App::new(2);
    let draining_router = router(draining.clone());
    for n in 0..4 {
        create(
            &draining_router,
            &format!("d-{n}"),
            &spec_json("last.png", 80),
        )
        .await;
    }
    let report = draining.shutdown().await; // returns only after queued + running jobs finished
    println!("  report: {report:?}");
    assert_eq!(
        report,
        ShutdownReport {
            total: 4,
            succeeded: 4,
            failed: 0,
            cancelled: 0,
            unfinished: 0
        }
    );
    let late = create(&draining_router, "late", &spec_json("late.png", 5)).await;
    assert_eq!(
        (late.status, late.error_code().as_str()),
        (StatusCode::SERVICE_UNAVAILABLE, "shutting_down")
    );
    let read_after = call(&draining_router, "GET", "/v1/jobs", &[], "").await;
    assert_eq!(
        read_after.status,
        StatusCode::OK,
        "reads still work while draining"
    );

    let main_report = state.shutdown().await;
    assert_eq!(main_report.unfinished, 0);
    println!("  main app: {main_report:?}");
}

// ---------------------------------------------------------------- edge cases: shutdown, cancel and bad requests

/// A single-threaded runtime: a spawned worker runs only when the check awaits, so the order of events is fixed.
fn block_on<F: Future>(check: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(check)
}

fn spec(work_ms: u64) -> Spec {
    serde_json::from_str(&spec_json("edge.png", work_ms)).expect("valid spec")
}

fn add(app: &App, key: &str, work_ms: u64) -> u64 {
    let (view, _) = app.submit(key, spec(work_ms)).expect("accepted");
    view["id"].as_u64().expect("id")
}

/// What an accepted `POST .../cancel` does to the job.
fn request_cancel(app: &App, id: u64) {
    app.store().jobs[&id].cancel.send_replace(true);
}

fn regressions() {
    println!("== edge: a cursor at u64::MAX is an empty page");
    block_on(async {
        let app = App::new(1);
        add(&app, "k", 1); // a cursor that wrapped round to 0 would list this job
        let uri = format!("/v1/jobs?after={}", u64::MAX);
        let page = call(&router(app.clone()), "GET", &uri, &[], "").await;
        assert_eq!(page.status, StatusCode::OK);
        assert_eq!(page.json(), json!({"items": [], "next_after": null}));
        assert!(!app.inner.store.is_poisoned());
        app.shutdown().await;
    });

    println!("== edge: giving up on shutdown stops no job");
    block_on(async {
        let app = App::new(1);
        for n in 0..3 {
            add(&app, &n.to_string(), 1);
        }
        // Poll once, then drop the future, as a timeout would.
        assert!(app.shutdown().now_or_never().is_none());
        let report = app.shutdown().await;
        assert_eq!((report.succeeded, report.unfinished), (3, 0));
    });

    println!("== edge: every caller of shutdown waits; retries still get their job");
    block_on(async {
        let app = App::new(1);
        let id = add(&app, "k", 5);
        let mut first = std::pin::pin!(app.shutdown());
        assert!(futures_util::poll!(first.as_mut()).is_pending());
        assert!(app.shutdown().now_or_never().is_none(), "returned early");

        let http = router(app.clone());
        let retry = create(&http, "k", &spec_json("edge.png", 5)).await;
        assert_eq!(retry.status, StatusCode::OK);
        assert_eq!(retry.json()["id"].as_u64(), Some(id));
        let other = create(&http, "k", &spec_json("other.png", 5)).await;
        assert_eq!(other.error_code(), "idempotency_key_reuse");
        let new = create(&http, "new", &spec_json("edge.png", 5)).await;
        assert_eq!(new.error_code(), "shutting_down");

        let (a, b) = tokio::join!(first, app.shutdown());
        assert_eq!((a.unfinished, b.unfinished), (0, 0));
    });

    println!("== edge: the closed flag is checked under the store lock");
    block_on(async {
        let app = App::new(1);
        app.shutdown().await;
        // The method that inserts, called with the lock held, must refuse by itself.
        let refused = app.store().submit("late", spec(1), || {});
        assert!(matches!(refused, Err(ApiError::ShuttingDown)));
    });
    block_on(async {
        // Through `App::submit`, which a check under one lock and an insert under another would get wrong.
        let app = App::new(1);
        let closer = Arc::new(Mutex::new(None));
        let (in_hook, slot) = (app.clone(), closer.clone());
        let hook = move || {
            // While the submit stands between the check and the insert, another thread polls shutdown once.
            let app = in_hook.clone();
            let (done, wait) = std::sync::mpsc::channel();
            let thread = std::thread::spawn(move || {
                let first_poll = app.shutdown().now_or_never();
                let _ = done.send(());
                first_poll
            });
            // A shutdown that has to wait for the submit cannot finish here, so do not wait for ever.
            let _ = wait.recv_timeout(Duration::from_millis(500));
            *slot.lock().expect("slot") = Some(thread);
        };
        assert!(app.inner.on_admit.set(Box::new(hook)).is_ok());
        let accepted = usize::from(app.submit("k", spec(1)).is_ok());
        let thread = closer.lock().expect("slot").take().expect("hook ran");
        if let Some(report) = thread.join().expect("no panic") {
            assert_eq!(
                report.total, accepted,
                "shutdown returned without an accepted job"
            );
        }
        assert_eq!(app.shutdown().await.total, accepted);
    });

    println!("== edge: a refused change leaves the clock alone");
    block_on(async {
        let app = App::new(1);
        let id = add(&app, "k", 1);
        request_cancel(&app, id);
        let before = app.store().clock;
        assert!(app.mark_running(id).is_none());
        assert_eq!(app.store().clock, before, "a refused start");
        app.shutdown().await;
        let before = app.store().clock;
        app.finish(id, Ok("again".into()), None);
        assert_eq!(app.store().clock, before, "a second finish");
    });

    // A stress run on top: submits race with shutdown on several threads.
    let threads = tokio::runtime::Runtime::new().expect("runtime");
    for _ in 0..50 {
        let app = App::new(4);
        threads.block_on(async {
            let submits: Vec<_> = (0..20)
                .map(|n| {
                    let app = app.clone();
                    tokio::spawn(async move { app.submit(&n.to_string(), spec(0)).is_ok() })
                })
                .collect();
            tokio::task::yield_now().await;
            let report = app.shutdown().await;
            let mut accepted = 0;
            for submit in submits {
                accepted += usize::from(submit.await.expect("no panic"));
            }
            assert_eq!((report.total, report.unfinished), (accepted, 0));
        });
    }

    println!("== edge: a job cancelled before it runs never starts");
    block_on(async {
        // A free slot and a cancel are both ready when each worker first runs.
        let app = App::new(64);
        for n in 0..64 {
            let id = add(&app, &n.to_string(), 1);
            request_cancel(&app, id);
        }
        assert_eq!(app.shutdown().await.cancelled, 64);
        let store = app.store();
        let mut events = store.jobs.values().flat_map(|j| &j.events);
        assert!(events.all(|e| e.name != "job.started"));
    });
    block_on(async {
        // A cancel that lands between getting a slot and starting.
        let app = App::new(1);
        let id = add(&app, "k", 1);
        request_cancel(&app, id);
        assert!(app.mark_running(id).is_none());
        app.shutdown().await;
    });

    println!("== edge: an accepted cancel wins over a result that is already there");
    block_on(async {
        let app = App::new(1);
        let id = add(&app, "k", 1);
        drop(app.mark_running(id).expect("queued"));
        request_cancel(&app, id);
        app.finish(id, Ok("too-late".into()), None);
        let job = app.store().jobs[&id].view();
        assert_eq!(job["status"], "cancelled");
        assert!(job["output"].is_null());
        app.shutdown().await;
    });

    println!("== edge: cancelled is reported only after the work is gone");
    block_on(async {
        let app = App::new(1);
        let id = add(&app, "k", 10_000);
        while app.store().jobs[&id].status != Status::Running {
            tokio::task::yield_now().await;
        }
        request_cancel(&app, id);
        assert_eq!(app.shutdown().await.cancelled, 1);
        assert!(!app.store().jobs[&id].outlived_by_work);
    });

    println!("== edge: unknown routes and rejected requests use the error shape");
    block_on(async {
        let http = router(App::new(1));
        let huge = "x".repeat(3_000_000); // over axum's default body limit of 2 MB
        for (method, uri, body, status, code) in [
            ("GET", "/nope", "", 404, "no_route"),
            ("DELETE", "/v1/jobs", "", 405, "method_not_allowed"),
            ("GET", "/v1/jobs/%FF", "", 404, "job_not_found"),
            ("POST", "/v1/jobs/%FF/cancel", "", 404, "job_not_found"),
            ("GET", "/v1/jobs/%FF/events", "", 404, "job_not_found"),
            ("POST", "/v1/jobs", huge.as_str(), 413, "bad_body"),
            (
                "POST",
                "/v1/jobs",
                r#"["a.png",1,1,"png",20]"#,
                422,
                "invalid_json",
            ),
        ] {
            let reply = call(&http, method, uri, &[("idempotency-key", "k")], body).await;
            assert_eq!(reply.status.as_u16(), status, "{method} {uri}");
            assert_eq!(reply.error_code(), code, "{method} {uri}");
            assert!(reply.headers.contains_key("x-request-id"));
        }
    });

    println!("== edge: a limit of zero is refused, not a queue that never moves");
    assert!(std::panic::catch_unwind(|| App::new(0)).is_err());

    println!("== edge: SSE replays and ends; it does not follow");
    block_on(async {
        let app = App::new(1);
        let id = add(&app, "k", 10_000);
        let uri = format!("/v1/jobs/{id}/events");
        let replay = call(&router(app.clone()), "GET", &uri, &[], "").await;
        assert!(replay.body.contains("event: job.queued"));
        assert!(!app.store().jobs[&id].status.is_terminal());
        request_cancel(&app, id);
        app.shutdown().await;
    });
}
