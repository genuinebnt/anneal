use solution::*;

#[allow(unused_imports)]
use std::time::Duration;

#[allow(dead_code)]
pub fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap().block_on(f)
}

#[allow(unused_imports)]
use axum::Router;
#[allow(unused_imports)]
use axum::body::{Body, to_bytes};
#[allow(unused_imports)]
use axum::http::{HeaderMap, Request};
#[allow(unused_imports)]
use serde_json::{Value, json};
#[allow(unused_imports)]
use tower::ServiceExt;

#[allow(dead_code)]
pub struct Reply {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: String,
}

#[allow(dead_code)]
impl Reply {
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or_else(|_| Value::String(self.body.clone()))
    }

    pub fn header(&self, name: &str) -> Option<String> {
        self.headers.get(name).map(|v| String::from_utf8_lossy(v.as_bytes()).into_owned())
    }

    /// (status, error code)
    pub fn error(&self) -> (u16, String) {
        (self.status, self.json()["error"]["code"].as_str().unwrap_or("<no error code>").to_string())
    }
}

#[allow(dead_code)]
pub fn failure(status: u16, code: &str) -> (u16, String) {
    (status, code.to_string())
}

#[allow(dead_code)]
pub async fn send(app: &Router, method: &str, uri: &str, headers: &[(&str, &str)], body: &str) -> Reply {
    let mut request = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let request = request.body(Body::from(body.to_string())).unwrap();
    let exchange = async {
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let bytes = to_bytes(response.into_body(), 1 << 22).await.unwrap();
        Reply { status, headers, body: String::from_utf8_lossy(&bytes).into_owned() }
    };
    tokio::time::timeout(std::time::Duration::from_secs(3600), exchange).await.expect("the response did not finish")
}

#[allow(dead_code)]
pub async fn get(app: &Router, uri: &str) -> Reply {
    send(app, "GET", uri, &[], "").await
}

#[allow(dead_code)]
pub async fn post_job(app: &Router, key: &str, body: &str) -> Reply {
    send(app, "POST", "/v1/jobs", &[("idempotency-key", key)], body).await
}

#[allow(dead_code)]
pub fn job_json(source: &str, work_ms: u64) -> String {
    json!({"source": source, "width": 10, "height": 20, "work_ms": work_ms}).to_string()
}

#[allow(dead_code)]
pub fn spec(source: &str, work_ms: u64) -> Spec {
    Spec { source: source.to_string(), width: 10, height: 20, format: "png".to_string(), work_ms }
}

#[allow(unused_imports)]
use std::future::Future;
#[allow(unused_imports)]
use std::pin::Pin;
#[allow(unused_imports)]
use std::sync::atomic::{AtomicUsize, Ordering};
#[allow(unused_imports)]
use std::sync::{Arc, Mutex};
#[allow(unused_imports)]
use tokio::time::{Instant, sleep};

#[allow(dead_code)]
pub fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[allow(dead_code)]
pub async fn settle() {
    sleep(ms(1)).await;
}

#[allow(dead_code)]
pub struct Alive(pub Arc<AtomicUsize>);

#[allow(dead_code)]
impl Drop for Alive {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[allow(dead_code)]
/// The work the tests give the service. It takes `work_ms` of virtual time, and the source says how it ends:
/// `bad..` returns an error, `panic..` and `payload..` panic, `early..` panics before there is a future.
#[derive(Clone)]
pub struct Probe {
    pub zero: Instant,
    /// (virtual ms, source) of every call, in order.
    pub calls: Arc<Mutex<Vec<(u64, String)>>>,
    pub finished: Arc<Mutex<Vec<String>>>,
    /// Work futures that exist now, and the most there ever were.
    pub alive: Arc<AtomicUsize>,
    pub peak: Arc<AtomicUsize>,
}

#[allow(dead_code)]
pub type Work = Pin<Box<dyn Future<Output = Result<String, String>> + Send>>;

#[allow(dead_code)]
#[allow(clippy::new_without_default)]
impl Probe {
    pub fn new() -> Self {
        Probe { zero: Instant::now(), calls: Default::default(), finished: Default::default(), alive: Default::default(), peak: Default::default() }
    }

    pub fn work(&self) -> impl Fn(Spec) -> Work + Send + Sync + 'static + use<> {
        let probe = self.clone();
        move |spec: Spec| -> Work {
            let at = probe.zero.elapsed().as_millis() as u64;
            probe.calls.lock().unwrap().push((at, spec.source.clone()));
            if spec.source.starts_with("early") {
                panic!("no decoder for {}", spec.source);
            }
            let now = probe.alive.fetch_add(1, Ordering::SeqCst) + 1;
            probe.peak.fetch_max(now, Ordering::SeqCst);
            let alive = Alive(probe.alive.clone());
            let finished = probe.finished.clone();
            Box::pin(async move {
                let _alive = alive;
                sleep(ms(spec.work_ms)).await;
                if spec.source.starts_with("panic") {
                    panic!("decoder crashed on {}", spec.source);
                }
                if spec.source.starts_with("payload") {
                    std::panic::panic_any(7u8);
                }
                finished.lock().unwrap().push(spec.source.clone());
                if spec.source.starts_with("bad") {
                    return Err(format!("{} is not an image", spec.source));
                }
                Ok(format!("done {}", spec.source))
            })
        }
    }

    pub fn calls(&self) -> Vec<(u64, String)> {
        self.calls.lock().unwrap().clone()
    }

    pub fn called(&self) -> Vec<String> {
        self.calls().into_iter().map(|c| c.1).collect()
    }

    pub fn alive(&self) -> usize {
        self.alive.load(Ordering::SeqCst)
    }

    pub fn peak(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }
}

#[allow(dead_code)]
pub fn service(limit: usize) -> (JobService, Probe) {
    let probe = Probe::new();
    (JobService::new(limit, probe.work()), probe)
}

#[allow(dead_code)]
/// Submits a new job and returns its id.
pub fn add(service: &JobService, source: &str, work_ms: u64) -> u64 {
    service.submit(source, spec(source, work_ms)).expect("accepted").0.id
}

#[allow(dead_code)]
pub fn job(service: &JobService, id: u64) -> JobView {
    service.store().get(id).expect("the job exists")
}

#[allow(dead_code)]
pub fn status(service: &JobService, id: u64) -> Status {
    job(service, id).status
}

#[allow(dead_code)]
/// (status, output, error)
pub fn ending(service: &JobService, id: u64) -> (Status, Option<String>, Option<String>) {
    let j = job(service, id);
    (j.status, j.output, j.error)
}

#[allow(dead_code)]
pub fn history(service: &JobService, id: u64) -> Vec<&'static str> {
    service.store().events(id, 0).expect("the job exists").iter().map(|e| e.name).collect()
}

#[allow(unused_imports)]
use axum::body::BodyDataStream;
#[allow(unused_imports)]
use futures::StreamExt;

#[allow(dead_code)]
/// Nothing arrived in time.
#[derive(Debug, PartialEq)]
pub struct Silent;

#[allow(dead_code)]
pub struct Client {
    pub status: u16,
    pub content_type: String,
    body: BodyDataStream,
    buffer: String,
}

#[allow(dead_code)]
#[derive(Debug, PartialEq)]
pub enum Got {
    /// (id, event, data)
    Event(u64, String, Value),
    /// Nothing but comments within a second.
    Quiet,
    End,
    Other(String),
}

#[allow(dead_code)]
/// The event the service sends for entry `id` of `job`.
pub fn event(job: u64, id: u64, status: &str) -> Got {
    let name = if status == "running" { "started" } else { status };
    Got::Event(id, format!("job.{name}"), json!({"job_id": job, "status": status}))
}

#[allow(dead_code)]
pub async fn listen(app: &Router, job: u64, last_event_id: Option<&str>) -> Client {
    let mut request = Request::builder().uri(format!("/v1/jobs/{job}/events"));
    if let Some(last) = last_event_id {
        request = request.header("last-event-id", last);
    }
    let response = tokio::time::timeout(Duration::from_millis(5), app.clone().oneshot(request.body(Body::empty()).unwrap())).await.expect("no response: it must not wait for the job").unwrap();
    let content_type = response.headers().get("content-type").map(|v| v.to_str().unwrap().to_string()).unwrap_or_default();
    Client { status: response.status().as_u16(), content_type, body: response.into_body().into_data_stream(), buffer: String::new() }
}

#[allow(dead_code)]
impl Client {
    /// The next block of lines. `Ok(None)` at the end of the body, `Err` if nothing came within `wait`.
    pub async fn block(&mut self, wait: Duration) -> Result<Option<String>, Silent> {
        loop {
            if let Some(end) = self.buffer.find("\n\n") {
                return Ok(Some(self.buffer.drain(..end + 2).collect()));
            }
            match tokio::time::timeout(wait, self.body.next()).await {
                Err(_) => return Err(Silent),
                Ok(None) => return Ok(None),
                Ok(Some(bytes)) => self.buffer.push_str(std::str::from_utf8(&bytes.expect("body error")).expect("utf-8")),
            }
        }
    }

    pub async fn read_within(&mut self, wait: Duration) -> Got {
        for _ in 0..1000 {
            let text = match self.block(wait).await {
                Err(Silent) => return Got::Quiet,
                Ok(None) => return Got::End,
                Ok(Some(text)) => text,
            };
            let (mut id, mut name, mut data) = (None, None, None);
            for line in text.lines().filter(|l| !l.is_empty() && !l.starts_with(':')) {
                let (field, value) = line.split_once(':').unwrap_or((line, ""));
                let value = value.strip_prefix(' ').unwrap_or(value).to_string();
                match field {
                    "id" => id = value.parse::<u64>().ok(),
                    "event" => name = Some(value),
                    "data" => data = serde_json::from_str::<Value>(&value).ok(),
                    _ => {}
                }
            }
            if id.is_none() && name.is_none() && data.is_none() {
                continue;
            }
            return match (id, name, data) {
                (Some(id), Some(name), Some(data)) => Got::Event(id, name, data),
                _ => Got::Other(text),
            };
        }
        Got::Other("1000 blocks without an event".to_string())
    }

    pub async fn read(&mut self) -> Got {
        self.read_within(Duration::from_secs(1)).await
    }

    /// Every event until the body ends.
    pub async fn rest(&mut self) -> Vec<Got> {
        let mut all = Vec::new();
        for _ in 0..100 {
            match self.read_within(Duration::from_secs(14)).await {
                Got::End => return all,
                got => all.push(got),
            }
        }
        all
    }
}

#[allow(dead_code)]
pub fn report(total: usize, succeeded: usize, failed: usize, cancelled: usize) -> ShutdownReport {
    ShutdownReport { total, succeeded, failed, cancelled, unfinished: 0 }
}

#[allow(dead_code)]
pub fn shutting_down(s: &JobService) -> tokio::task::JoinHandle<ShutdownReport> {
    let s = s.clone();
    tokio::spawn(async move { s.shutdown().await })
}

#[allow(dead_code)]
pub async fn returned(handle: tokio::task::JoinHandle<ShutdownReport>) -> ShutdownReport {
    tokio::time::timeout(Duration::from_secs(3600), handle).await.expect("shutdown never returned").unwrap()
}

#[test]
fn waits_for_running_and_queued_jobs() {
    run(async {
        let (s, _) = service(1);
        add(&s, "a", 30);
        add(&s, "b", 30);
        let start = Instant::now();
        let got = returned(shutting_down(&s)).await;
        check!("two 30 ms jobs, one slot: (report, elapsed)", (got, start.elapsed()), (report(2, 2, 0, 0), ms(60)));
    });
}

#[test]
fn counts_every_ending() {
    run(async {
        let (s, _) = service(4);
        for source in ["ok", "bad", "panic", "long"] {
            add(&s, source, if source == "long" { 500 } else { 5 });
        }
        sleep(ms(20)).await;
        let _ = s.store().cancel(4);
        check!("report", returned(shutting_down(&s)).await, report(4, 1, 2, 1));
    });
}

#[test]
fn new_jobs_are_refused_during_the_drain() {
    run(async {
        let (s, _) = service(1);
        let app = router(s.clone());
        post_job(&app, "a", &job_json("a", 100)).await;
        let waiting = shutting_down(&s);
        settle().await;
        let r = post_job(&app, "b", &job_json("b", 1)).await;
        check!("a new job", r.error(), failure(503, "shutting_down"));
        check!("reading still works", get(&app, "/v1/jobs/1").await.status, 200);
        check!("report", returned(waiting).await, report(1, 1, 0, 0));
    });
}

#[test]
fn a_retry_still_gets_its_job() {
    run(async {
        let (s, _) = service(1);
        let app = router(s.clone());
        post_job(&app, "a", &job_json("a", 5)).await;
        returned(shutting_down(&s)).await;
        let r = post_job(&app, "a", &job_json("a", 5)).await;
        check!("retry after shutdown: (status, job status)", (r.status, r.json()["status"].clone()), (200, json!("succeeded")));
    });
}

#[test]
fn an_idle_service() {
    run(async {
        let (s, _) = service(1);
        let start = Instant::now();
        check!("(report, elapsed)", (returned(shutting_down(&s)).await, start.elapsed()), (report(0, 0, 0, 0), ms(0)));
    });
}
