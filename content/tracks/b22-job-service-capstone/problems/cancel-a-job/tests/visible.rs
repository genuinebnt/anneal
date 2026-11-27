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

#[test]
fn cancel_a_running_job() {
    run(async {
        let (s, p) = service(1);
        let id = add(&s, "long", 5_000);
        settle().await;
        check!("cancel is accepted", s.store().cancel(id).map(|j| j.id), Ok(id));
        settle().await;
        check!("(status, work futures alive)", (status(&s, id), p.alive()), (Status::Cancelled, 0));
        check!("history", history(&s, id), vec!["job.queued", "job.started", "job.cancelled"]);
    });
}

#[test]
fn cancel_a_queued_job() {
    run(async {
        let (s, p) = service(1);
        add(&s, "first", 100);
        let id = add(&s, "second", 5);
        settle().await;
        let _ = s.store().cancel(id);
        settle().await;
        check!("cancelled without waiting for a slot", status(&s, id), Status::Cancelled);
        sleep(ms(200)).await;
        check!("it never ran", (p.called(), history(&s, id)), (vec!["first".to_string()], vec!["job.queued", "job.cancelled"]));
    });
}

#[test]
fn over_http() {
    run(async {
        let (s, _) = service(1);
        let app = router(s);
        post_job(&app, "k", &job_json("a", 500)).await;
        settle().await;
        let r = send(&app, "POST", "/v1/jobs/1/cancel", &[], "").await;
        check!("cancel: (status, job id)", (r.status, r.json()["id"].clone()), (202, json!(1)));
        settle().await;
        check!("later", get(&app, "/v1/jobs/1").await.json()["status"].clone(), json!("cancelled"));
    });
}

#[test]
fn a_finished_job_cannot_be_cancelled() {
    run(async {
        let (s, _) = service(1);
        add(&s, "quick", 1);
        sleep(ms(10)).await;
        let app = router(s);
        let r = send(&app, "POST", "/v1/jobs/1/cancel", &[], "").await;
        check!("cancel after success", r.error(), failure(409, "not_cancellable"));
    });
}

#[test]
fn an_unknown_job() {
    run(async {
        let (s, _) = service(1);
        check!("cancel(5) on an empty store", s.store().cancel(5), Err(ApiError::NotFound("5".to_string())));
    });
}
