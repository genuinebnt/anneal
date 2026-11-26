//! CAPSTONE 06 (axum): an in-process image-resize job service (idempotent API, workers, SSE, graceful shutdown)
//! LEVEL: capstone
//! Use in: any "submit work over HTTP, run it in the background, watch it" service.   Run: cargo run --release --example job_service_axum
#![allow(dead_code)]
//!
//! WHAT: a small job service for image-resize jobs. Clients POST a job, poll it, list jobs page by page, cancel it,
//! and stream its event log over Server-Sent Events. Everything runs in-process and is driven through
//! `tower::ServiceExt::oneshot`, so there are no sockets and the example is fast and deterministic.
//! WHY: it combines recipes axum_router_state (router state), axum_errors (error type), axum_sse_replay (SSE replay), axum_middleware (middleware), axum_idempotency
//! (idempotency) and worker_pool (worker pool) into the shape real services have.
//! HOW:
//!  * `App` is cheap-to-clone shared state (`Arc<Inner>`): job store behind a mutex, a logical clock (a counter, so
//!    timestamps are deterministic), a `Semaphore` that bounds concurrency, and a `JoinSet` of worker tasks.
//!  * Each job gets one tokio task. It waits for a permit (or for cancellation), runs the resize in a CHILD task so a
//!    panic surfaces as a `JoinError` and becomes `failed`, and `select!`s on a `watch` channel for cancellation.
//!  * Every state change appends to a per-job event log (ids 1, 2, 3, ...), which SSE replays from `Last-Event-ID`.
//!  * `ApiError` maps every failure to one JSON shape with a stable machine-readable `code`.
//!  * Shutdown flips `accepting` (new POSTs get 503), then drains the `JoinSet` and reports counts.
//! COMMON USES: thumbnail/transcode pipelines, report generation, webhook delivery, import/export jobs, CI-style
//! runners; patterns: idempotency keys for safe retries, cursor pagination, outbox/event-log replay, bulkheads
//! (semaphore), panic isolation, cooperative cancellation, drain-on-SIGTERM.
//! Read: https://docs.rs/axum/latest/axum/ https://tokio.rs/tokio/tutorial https://stripe.com/blog/idempotency
use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{Path, Query, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use futures_util::stream::{self, Stream};
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap},
    convert::Infallible,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::SeqCst},
    },
    time::Duration,
};
use tokio::{
    sync::{Semaphore, watch},
    task::JoinSet,
};
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
    #[error("this Idempotency-Key was already used with a different request body")]
    KeyReuse,
    #[error("job {0} does not exist")]
    NotFound(String),
    #[error("job is already {0} and cannot be cancelled")]
    NotCancellable(&'static str),
    #[error("invalid query parameter: {0}")]
    BadQuery(String),
    #[error("service is shutting down and not accepting new jobs")]
    ShuttingDown,
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
    clock: AtomicU64,
    permits: Arc<Semaphore>,
    tasks: Mutex<JoinSet<()>>,
    accepting: AtomicBool,
    in_flight: AtomicUsize,
    max_in_flight: AtomicUsize,
}

#[derive(Clone)]
struct App {
    inner: Arc<Inner>,
}

/// RAII marker for "a job is executing now"; the decrement also happens if the task is aborted.
struct FlightGuard(Arc<Inner>);

impl Drop for FlightGuard {
    fn drop(&mut self) {
        self.0.in_flight.fetch_sub(1, SeqCst);
    }
}

impl App {
    fn new(max_concurrent: usize) -> Self {
        let inner = Inner {
            store: Mutex::default(),
            clock: AtomicU64::new(0),
            permits: Arc::new(Semaphore::new(max_concurrent)),
            tasks: Mutex::default(),
            accepting: AtomicBool::new(true),
            in_flight: AtomicUsize::new(0),
            max_in_flight: AtomicUsize::new(0),
        };
        App {
            inner: Arc::new(inner),
        }
    }

    /// A poisoned lock only means some thread panicked while holding it; our updates are single-step, so keep going.
    fn store(&self) -> MutexGuard<'_, Store> {
        self.inner.store.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn tick(&self) -> u64 {
        self.inner.clock.fetch_add(1, SeqCst) + 1
    }

    /// Create the job (or return the one this key already made) and start its worker.
    /// The key lookup and the insert happen under one lock, so two racing retries cannot both create a job.
    fn submit(&self, key: &str, spec: Spec) -> Result<(Value, bool), ApiError> {
        if !self.inner.accepting.load(SeqCst) {
            return Err(ApiError::ShuttingDown);
        }
        let mut store = self.store();
        if let Some(id) = store.keys.get(key).copied() {
            let job = &store.jobs[&id];
            return if job.spec == spec {
                Ok((job.view(), false))
            } else {
                Err(ApiError::KeyReuse)
            };
        }
        store.next_id += 1;
        let id = store.next_id;
        let (cancel, cancel_rx) = watch::channel(false);
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
            cancel,
        };
        job.record(Status::Queued, "job.queued", now);
        let view = job.view();
        store.jobs.insert(id, job);
        store.keys.insert(key.to_string(), id);
        drop(store);
        self.spawn_worker(id, cancel_rx);
        Ok((view, true))
    }

    fn spawn_worker(&self, id: u64, cancel: watch::Receiver<bool>) {
        let app = self.clone();
        let mut tasks = self.inner.tasks.lock().unwrap_or_else(|e| e.into_inner());
        while tasks.try_join_next().is_some() {} // reap finished workers so the set does not grow forever
        tasks.spawn(run_job(app, id, cancel));
    }

    fn mark_running(&self, id: u64) -> Spec {
        let now = self.tick();
        let mut store = self.store();
        let job = store.jobs.get_mut(&id).expect("job exists");
        job.record(Status::Running, "job.started", now);
        job.spec.clone()
    }

    fn finish(&self, id: u64, status: Status, output: Option<String>, error: Option<String>) {
        let now = self.tick();
        let name = match status {
            Status::Succeeded => "job.succeeded",
            Status::Cancelled => "job.cancelled",
            _ => "job.failed",
        };
        let mut store = self.store();
        let job = store.jobs.get_mut(&id).expect("job exists");
        job.output = output;
        job.error = error;
        job.record(status, name, now);
    }

    fn enter_flight(&self) -> FlightGuard {
        let now = self.inner.in_flight.fetch_add(1, SeqCst) + 1;
        self.inner.max_in_flight.fetch_max(now, SeqCst);
        FlightGuard(self.inner.clone())
    }

    /// Stop accepting, wait for every worker (queued and running), then count outcomes.
    async fn shutdown(&self) -> ShutdownReport {
        self.inner.accepting.store(false, SeqCst);
        let mut tasks =
            std::mem::take(&mut *self.inner.tasks.lock().unwrap_or_else(|e| e.into_inner()));
        while tasks.join_next().await.is_some() {}
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
    // Waiting for a permit is cancellable too: a queued job can be cancelled without ever running.
    let permit = tokio::select! {
        p = app.inner.permits.clone().acquire_owned() => Some(p.expect("semaphore is never closed")),
        _ = cancelled(&mut cancel) => None,
    };
    let Some(_permit) = permit else {
        app.finish(id, Status::Cancelled, None, None);
        return;
    };
    let spec = app.mark_running(id);
    let _flight = app.enter_flight();
    tracing::info!(job = id, source = %spec.source, "job started");
    // The resize runs in a CHILD task: if it panics, only the child dies and we see a JoinError here.
    let work = tokio::spawn(resize(spec));
    let abort = work.abort_handle();
    tokio::select! {
        joined = work => match joined {
            Ok(Ok(out)) => app.finish(id, Status::Succeeded, Some(out), None),
            Ok(Err(msg)) => app.finish(id, Status::Failed, None, Some(msg)),
            Err(e) => app.finish(id, Status::Failed, None, Some(format!("worker panicked: {}", panic_message(e)))),
        },
        _ = cancelled(&mut cancel) => {
            abort.abort(); // cooperative: stop at the child's next .await point
            app.finish(id, Status::Cancelled, None, None);
        }
    }
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
    body: Bytes,
) -> Result<Response, ApiError> {
    let key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .filter(|k| !k.is_empty());
    let key = key.ok_or(ApiError::MissingKey)?;
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

fn parse_id(raw: &str) -> Result<u64, ApiError> {
    raw.parse().map_err(|_| ApiError::NotFound(raw.to_string()))
}

async fn get_job(State(app): State<App>, Path(raw): Path<String>) -> Result<Json<Value>, ApiError> {
    let id = parse_id(&raw)?;
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
    let mut page: Vec<&Job> = store
        .jobs
        .range(after + 1..)
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
    Path(raw): Path<String>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let id = parse_id(&raw)?;
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
    Path(raw): Path<String>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let id = parse_id(&raw)?;
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
    Ok(Sse::new(stream::iter(replay)).keep_alive(KeepAlive::default()))
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

#[tokio::main]
async fn main() {
    // Logs go to stderr so the stdout transcript stays readable; try_init because a host may have set one already.
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_writer(std::io::stderr)
        .try_init();
    // Deliberate panics are part of the demo; report them through tracing instead of the default noisy hook.
    std::panic::set_hook(Box::new(|info| {
        tracing::warn!("panic caught by worker isolation: {info}")
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
    println!("all c06 job-service checks passed (main app: {main_report:?})");
}
