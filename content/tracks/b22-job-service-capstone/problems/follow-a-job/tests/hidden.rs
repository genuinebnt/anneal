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

#[test]
fn failures_and_cancels_end_the_stream() {
    run(async {
        let (s, _) = service(2);
        add(&s, "bad", 500);
        add(&s, "long", 60_000);
        let app = router(s.clone());
        let mut failing = listen(&app, 1, None).await;
        let mut cancelled = listen(&app, 2, None).await;
        sleep(ms(600)).await;
        check!("a failing job", failing.rest().await, vec![event(1, 1, "queued"), event(1, 2, "running"), event(1, 3, "failed")]);
        let _ = s.store().cancel(2);
        check!("a cancelled job", cancelled.rest().await, vec![event(2, 1, "queued"), event(2, 2, "running"), event(2, 3, "cancelled")]);
    });
}

#[test]
fn an_id_that_does_not_decode() {
    run(async {
        let (s, _) = service(1);
        check!("GET /v1/jobs/%FF/events", get(&router(s), "/v1/jobs/%FF/events").await.error(), failure(404, "job_not_found"));
    });
}

#[test]
fn rubbish_in_last_event_id_means_zero() {
    run(async {
        let (s, _) = service(1);
        add(&s, "a", 1);
        sleep(ms(10)).await;
        let app = router(s);
        for rubbish in ["", "abc", "-1", "2.5", "99999999999999999999"] {
            let got = listen(&app, 1, Some(rubbish)).await.rest().await;
            check!(format!("Last-Event-ID {rubbish:?}: number of events"), got.len(), 3);
        }
        let got = listen(&app, 1, Some("18446744073709551615")).await.rest().await;
        check!("Last-Event-ID u64::MAX", got, vec![]);
    });
}

#[test]
fn ahead_of_a_live_job() {
    run(async {
        let (s, _) = service(1);
        add(&s, "a", 100);
        settle().await;
        let app = router(s);
        let mut c = listen(&app, 1, Some("2")).await;
        check!("nothing above 2 yet", c.read_within(ms(10)).await, Got::Quiet);
        check!("only the third entry", c.rest().await, vec![event(1, 3, "succeeded")]);
    });
}

#[test]
fn each_event_comes_as_it_happens() {
    run(async {
        let (s, _) = service(1);
        add(&s, "a", 30_000);
        let mut c = listen(&router(s.clone()), 1, None).await;
        check!("queued", c.read().await, event(1, 1, "queued"));
        check!("running", c.read().await, event(1, 2, "running"));
        check!("then quiet while the work goes on", c.read().await, Got::Quiet);
    });
}

#[test]
fn quiet_streams_send_comments() {
    run(async {
        let (s, _) = service(1);
        add(&s, "a", 120_000);
        let mut c = listen(&router(s.clone()), 1, None).await;
        c.read().await;
        c.read().await;
        for round in 0..3 {
            let got = c.block(ms(15_100)).await.map(|b| b.map(|b| b.lines().all(|l| l.is_empty() || l.starts_with(':'))));
            check!(format!("quiet period {round}: a comment within 15 s"), got, Ok(Some(true)));
        }
    });
}

#[test]
fn many_followers_and_one_that_never_reads() {
    run(async {
        let (s, _) = service(1);
        add(&s, "a", 50);
        let app = router(s.clone());
        let _silent = listen(&app, 1, None).await;
        let mut readers = Vec::new();
        for _ in 0..20 {
            readers.push(listen(&app, 1, None).await);
        }
        sleep(ms(100)).await;
        check!("the job is not held back", status(&s, 1), Status::Succeeded);
        for (i, mut c) in readers.into_iter().enumerate() {
            check!(format!("follower {i}"), c.rest().await.len(), 3);
        }
    });
}

#[test]
fn a_late_reader_misses_nothing() {
    run(async {
        let (s, _) = service(1);
        add(&s, "a", 10);
        let mut c = listen(&router(s.clone()), 1, None).await;
        // Everything happens before the first read of the body.
        sleep(ms(100)).await;
        check!("read afterwards", c.rest().await, vec![event(1, 1, "queued"), event(1, 2, "running"), event(1, 3, "succeeded")]);
    });
}

#[test]
fn streams_belong_to_their_job() {
    run(async {
        let (s, _) = service(1);
        add(&s, "a", 40);
        add(&s, "b", 40);
        let app = router(s);
        let mut second = listen(&app, 2, None).await;
        let mut first = listen(&app, 1, None).await;
        check!("job 1", first.rest().await, vec![event(1, 1, "queued"), event(1, 2, "running"), event(1, 3, "succeeded")]);
        check!("job 2", second.rest().await, vec![event(2, 1, "queued"), event(2, 2, "running"), event(2, 3, "succeeded")]);
    });
}

#[test]
fn a_change_made_through_the_store_is_followed() {
    run(async {
        let (s, _) = service(1);
        add(&s, "holds the slot", 600_000);
        add(&s, "waits", 5);
        let mut c = listen(&router(s.clone()), 2, Some("1")).await;
        check!("quiet", c.read().await, Got::Quiet);
        s.store().finish(2, Outcome::Cancelled);
        check!("woken by finish", c.rest().await, vec![event(2, 2, "cancelled")]);
    });
}

#[test]
fn random_listeners_get_exactly_the_rest() {
    let mut rng = anneal_prelude::Rng::new(22006);
    for _ in 0..120 {
        let took = 1 + rng.below(20) as u64;
        let kind = *rng.pick(&["ok", "bad", "panic"]);
        let cancel_at = rng.bool().then(|| rng.below(40) as u64);
        let ahead = rng.below(30) as u64;
        let join_at = rng.below(30) as u64;
        let last = rng.below(5) as u64;
        let shown = format!("{kind} job of {took} ms behind one of {ahead} ms, cancel {cancel_at:?}, join at {join_at} ms with Last-Event-ID {last}");
        run(async {
            let (s, _) = service(1);
            add(&s, "ahead", ahead);
            add(&s, kind, took);
            if let Some(at) = cancel_at {
                let s = s.clone();
                tokio::spawn(async move {
                    sleep(ms(at)).await;
                    let _ = s.store().cancel(2);
                });
            }
            sleep(ms(join_at)).await;
            let mut c = listen(&router(s.clone()), 2, Some(&last.to_string())).await;
            let got = c.rest().await;
            check!(format!("{shown}: the job ended"), status(&s, 2).is_final(), true);
            let want: Vec<Got> = s.store().events(2, last).unwrap().into_iter().map(|e| Got::Event(e.id, e.name.to_string(), serde_json::from_str(&e.data).unwrap())).collect();
            check!(shown, got, want);
        });
    }
}
