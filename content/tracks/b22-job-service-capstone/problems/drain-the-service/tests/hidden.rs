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
fn submit_itself_refuses() {
    run(async {
        let (s, p) = service(1);
        add(&s, "a", 5);
        returned(shutting_down(&s)).await;
        let new = s.submit("b", spec("b", 5)).map(|r| r.1);
        let reuse = s.submit("a", spec("other", 5)).map(|r| r.1);
        let replay = s.submit("a", spec("a", 5)).map(|r| r.1);
        settle().await;
        check!("(new key, reused key, retry)", (new, reuse, replay), (Err(ApiError::ShuttingDown), Err(ApiError::KeyReuse), Ok(false)));
        check!("(jobs, work calls)", (s.store().get(2), p.called().len()), (None, 1));
    });
}

#[test]
fn closing_starts_with_the_first_poll() {
    run(async {
        let (s, _) = service(1);
        add(&s, "a", 1_000);
        let waiting = shutting_down(&s);
        settle().await;
        check!("a submit during the drain", s.submit("b", spec("b", 1)).map(|r| r.1), Err(ApiError::ShuttingDown));
        returned(waiting).await;
    });
}

#[test]
fn bad_requests_stay_bad_requests() {
    run(async {
        let (s, _) = service(1);
        let app = router(s.clone());
        returned(shutting_down(&s)).await;
        check!("no key", send(&app, "POST", "/v1/jobs", &[], &job_json("a", 1)).await.error(), failure(400, "missing_idempotency_key"));
        check!("bad job", post_job(&app, "k", "{}").await.error(), failure(422, "invalid_json"));
    });
}

#[test]
fn cancel_and_follow_during_the_drain() {
    run(async {
        let (s, _) = service(1);
        let app = router(s.clone());
        add(&s, "a", 60_000);
        add(&s, "b", 60_000);
        let waiting = shutting_down(&s);
        settle().await;
        let mut c = listen(&app, 2, None).await;
        check!("follow", c.read().await, event(2, 1, "queued"));
        check!("list", get(&app, "/v1/jobs").await.status, 200);
        for id in [1, 2] {
            let r = send(&app, "POST", &format!("/v1/jobs/{id}/cancel"), &[], "").await;
            check!(format!("cancel {id}"), r.status, 202);
        }
        let start = Instant::now();
        check!("report", returned(waiting).await, report(2, 0, 0, 2));
        check!("cancelling ends the drain early", start.elapsed() < ms(100), true);
        check!("the follower sees the end", c.rest().await, vec![event(2, 2, "cancelled")]);
    });
}

#[test]
fn giving_up_on_shutdown_stops_no_job() {
    run(async {
        let (s, p) = service(1);
        for n in ["a", "b", "c"] {
            add(&s, n, 50);
        }
        let gave_up = tokio::time::timeout(ms(70), s.shutdown()).await.is_err();
        check!("a 70 ms limit on a 150 ms drain", gave_up, true);
        check!("still closed", s.submit("d", spec("d", 1)).map(|r| r.1), Err(ApiError::ShuttingDown));
        let start = Instant::now();
        let got = returned(shutting_down(&s)).await;
        check!("second shutdown: (report, extra time)", (got, start.elapsed()), (report(3, 3, 0, 0), ms(80)));
        check!("all work ran to its end", p.finished.lock().unwrap().len(), 3);
    });
}

#[test]
fn every_caller_waits() {
    run(async {
        let (s, _) = service(1);
        add(&s, "a", 40);
        let first = shutting_down(&s);
        settle().await;
        let second = shutting_down(&s);
        settle().await;
        check!("both still waiting", (first.is_finished(), second.is_finished()), (false, false));
        check!("both reports", (returned(first).await, returned(second).await), (report(1, 1, 0, 0), report(1, 1, 0, 0)));
        check!("and once more", returned(shutting_down(&s)).await, report(1, 1, 0, 0));
    });
}

#[test]
fn the_work_is_gone_when_shutdown_returns() {
    run(async {
        let (s, p) = service(3);
        for i in 0..9 {
            add(&s, &format!("{i}"), 10 + i);
        }
        returned(shutting_down(&s)).await;
        check!("work futures alive", p.alive(), 0);
    });
}

#[test]
fn nothing_accepted_is_left_behind() {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_time().build().unwrap();
    runtime.block_on(async {
        for round in 0..400usize {
            let s = JobService::new(4, |_| async {
                tokio::task::yield_now().await;
                Ok(String::new())
            });
            let accepted = Arc::new(AtomicUsize::new(0));
            let clients: Vec<_> = (0..4)
                .map(|c| {
                    let (s, accepted) = (s.clone(), accepted.clone());
                    tokio::spawn(async move {
                        for i in 0..200 {
                            match s.submit(&format!("{c}-{i}"), spec("x", 0)) {
                                Ok(_) => accepted.fetch_add(1, Ordering::SeqCst),
                                Err(_) => break,
                            };
                            if i % 3 == round % 3 {
                                tokio::task::yield_now().await;
                            }
                        }
                    })
                })
                .collect();
            for _ in 0..round % 7 {
                tokio::task::yield_now().await;
            }
            let got = tokio::time::timeout(Duration::from_secs(5), s.shutdown()).await.expect("shutdown never returned");
            for c in clients {
                c.await.unwrap();
            }
            let accepted = accepted.load(Ordering::SeqCst);
            check!(format!("round {round}: (jobs in the report, succeeded, unfinished) with {accepted} accepted"), (got.total, got.succeeded, got.unfinished), (accepted, accepted, 0));
        }
    });
}

#[test]
fn many_jobs_drain() {
    run(async {
        let (s, _) = service(10);
        for i in 0..3_000 {
            add(&s, &format!("{i}"), 1);
        }
        check!("report", returned(shutting_down(&s)).await, report(3_000, 3_000, 0, 0));
    });
}

#[test]
fn random_drains() {
    let mut rng = anneal_prelude::Rng::new(22007);
    for _ in 0..120 {
        let limit = 1 + rng.below(3);
        let jobs: Vec<(u64, usize)> = (0..rng.below(9)).map(|_| (1 + rng.below(8) as u64, rng.below(3))).collect();
        let at = rng.below(20) as u64;
        // Slots are taken in id order, and the drain ends with the last job (or at once, if all are done).
        let mut free = vec![0u64; limit];
        let mut end = 0;
        for &(took, _) in &jobs {
            let slot = free.iter_mut().min().unwrap();
            *slot += took;
            end = end.max(*slot);
        }
        let count = |k| jobs.iter().filter(|j| j.1 == k).count();
        let want = (report(jobs.len(), count(0), count(1) + count(2), 0), end.max(at));
        let got = run(async {
            let (s, _) = service(limit);
            let start = Instant::now();
            for (i, &(took, kind)) in jobs.iter().enumerate() {
                add(&s, &format!("{}-{i}", ["ok", "bad", "panic"][kind]), took);
            }
            if at > 0 {
                sleep(ms(at)).await;
            }
            let r = returned(shutting_down(&s)).await;
            (r, start.elapsed().as_millis() as u64)
        });
        check!(format!("limit {limit}, (ms, kind) {jobs:?}, shutdown at {at} ms: (report, end ms)"), got, want);
    }
}

#[test]
fn closing_cannot_slip_between_the_check_and_the_creation() {
    use futures::FutureExt;
    run(async {
        let (s, _) = service(1);
        let closer = Arc::new(Mutex::new(None));
        let (in_hook, slot) = (s.clone(), closer.clone());
        s.store().on_admit(move || {
            // While submit stands between its check and the new job, another thread polls shutdown once.
            let s = in_hook.clone();
            let (done, wait) = std::sync::mpsc::channel();
            let thread = std::thread::spawn(move || {
                let first_poll = run(async { s.shutdown().now_or_never() });
                let _ = done.send(());
                first_poll
            });
            // A shutdown that has to wait for submit cannot finish here, so do not wait for ever.
            let _ = wait.recv_timeout(Duration::from_millis(500));
            *slot.lock().unwrap() = Some(thread);
        });
        let accepted = usize::from(s.submit("a", spec("a", 10)).is_ok());
        let thread = closer.lock().unwrap().take().expect("submit did not call the on_admit hook for a new job");
        if let Some(report) = thread.join().expect("shutdown panicked") {
            check!("a shutdown that returned while \"a\" was being admitted: (jobs in its report, jobs accepted)", report.total, accepted);
        }
        let report = returned(shutting_down(&s)).await;
        check!("in the end: (jobs in the report, jobs accepted)", report.total, accepted);
    });
}

#[test]
fn the_admission_hook_is_only_for_new_jobs() {
    run(async {
        let (s, _) = service(1);
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        s.store().on_admit(move || {
            counted.fetch_add(1, Ordering::SeqCst);
        });
        add(&s, "a", 1);
        add(&s, "a", 1);
        let _ = s.submit("a", spec("other", 1));
        returned(shutting_down(&s)).await;
        let _ = s.submit("late", spec("late", 1));
        check!("hook calls for one new job, a retry, a reused key and a refused key", calls.load(Ordering::SeqCst), 1);
    });
}

#[test]
fn a_queued_job_with_a_cancel_takes_any_finish() {
    // "finish(id, ..), with any outcome, records Cancelled and returns true. This holds for a queued job too."
    let store = JobStore::new();
    for key in ["a", "b", "c"] {
        store.submit(key, spec(key, 1)).unwrap();
    }
    for id in 1..=3 {
        let _ = store.cancel(id);
    }
    let finished = (
        store.finish(1, Outcome::Succeeded("x".into())),
        store.finish(2, Outcome::Failed("boom".into())),
        store.finish(3, Outcome::Cancelled),
    );
    check!("finish of three queued jobs after a cancel: (success, failure, cancel)", finished, (true, true, true));
    for id in 1..=3 {
        let job = store.get(id).unwrap();
        check!(format!("job {id}: (status, output, error)"), (job.status, job.output, job.error), (Status::Cancelled, None, None));
    }
    let names: Vec<_> = store.events(1, 0).unwrap().into_iter().map(|e| e.name).collect();
    check!("job 1 history", names, vec!["job.queued", "job.cancelled"]);
    check!("a queued job without a cancel still refuses a success", { store.submit("d", spec("d", 1)).unwrap(); store.finish(4, Outcome::Succeeded("x".into())) }, false);
}
