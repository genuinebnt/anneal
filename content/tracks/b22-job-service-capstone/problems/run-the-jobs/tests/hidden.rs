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
#[should_panic]
fn zero_slots_panics() {
    run(async {
        let _ = service(0);
    });
}

#[test]
fn queued_until_a_slot_is_free() {
    run(async {
        let (s, p) = service(1);
        let a = add(&s, "a", 30);
        let b = add(&s, "b", 30);
        settle().await;
        check!("(first, second, work calls)", (status(&s, a), status(&s, b), p.called().len()), (Status::Running, Status::Queued, 1));
        sleep(ms(30)).await;
        check!("after the first ended", (status(&s, a), status(&s, b)), (Status::Succeeded, Status::Running));
    });
}

#[test]
fn the_job_is_running_before_its_work_is_called() {
    run(async {
        let cell: Arc<std::sync::OnceLock<JobService>> = Arc::default();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (c, v) = (cell.clone(), seen.clone());
        let s = JobService::new(1, move |_| {
            v.lock().unwrap().push(c.get().and_then(|s| s.store().get(1)).map(|j| j.status));
            async { Ok(String::new()) }
        });
        let _ = cell.set(s.clone());
        add(&s, "a", 1);
        sleep(ms(5)).await;
        check!("status seen from inside the work call", seen.lock().unwrap().clone(), vec![Some(Status::Running)]);
    });
}

#[test]
fn work_gets_the_spec() {
    run(async {
        let s = JobService::new(1, |spec: Spec| async move { Ok(format!("{} {} {} {} {}", spec.source, spec.width, spec.height, spec.format, spec.work_ms)) });
        let wanted = Spec { source: "z".into(), width: 7, height: 8, format: "webp".into(), work_ms: 9 };
        s.submit("k", wanted).unwrap();
        sleep(ms(5)).await;
        check!("output built from the spec", job(&s, 1).output, Some("z 7 8 webp 9".to_string()));
    });
}

#[test]
fn a_panic_while_making_the_future() {
    run(async {
        let (s, _) = service(1);
        let a = add(&s, "early.png", 5);
        let b = add(&s, "next", 5);
        sleep(ms(20)).await;
        check!("the job", ending(&s, a), (Status::Failed, None, Some("worker panicked: no decoder for early.png".to_string())));
        check!("the next one", status(&s, b), Status::Succeeded);
    });
}

#[test]
fn panics_with_other_payloads() {
    run(async {
        let (s, _) = service(1);
        let a = add(&s, "payload", 1);
        sleep(ms(10)).await;
        check!("a u8 payload", job(&s, a).error, Some("worker panicked: unknown panic".to_string()));
        let s = JobService::new(1, |_| async {
            if true {
                panic!("static text");
            }
            Ok(String::new())
        });
        s.submit("k", spec("a", 1)).unwrap();
        sleep(ms(10)).await;
        check!("a &str payload", job(&s, 1).error, Some("worker panicked: static text".to_string()));
    });
}

#[test]
fn slots_come_back_every_time() {
    run(async {
        let (s, p) = service(2);
        for i in 0..30 {
            let kind = ["panic", "early", "bad", "ok", "payload"][i % 5];
            add(&s, &format!("{kind}-{i}"), 3);
        }
        let last: Vec<u64> = ["x", "y"].iter().map(|n| add(&s, n, 10)).collect();
        sleep(ms(1000)).await;
        check!("the last two ran", (status(&s, last[0]), status(&s, last[1])), (Status::Succeeded, Status::Succeeded));
        let calls = p.calls();
        check!("the last two ran together", calls[30].0, calls[31].0);
    });
}

#[test]
fn replays_and_refusals_run_nothing() {
    run(async {
        let (s, p) = service(3);
        add(&s, "a", 5);
        let replay = s.submit("a", spec("a", 5)).map(|r| r.1);
        let reuse = s.submit("a", spec("b", 5));
        sleep(ms(20)).await;
        let late = s.submit("a", spec("a", 5)).map(|r| (r.0.status, r.1));
        sleep(ms(20)).await;
        check!("(replay, reuse, replay after the end)", (replay, reuse, late), (Ok(false), Err(ApiError::KeyReuse), Ok((Status::Succeeded, false))));
        check!("work calls", p.called(), vec!["a".to_string()]);
    });
}

#[test]
fn submit_returns_before_any_work() {
    run(async {
        let (s, p) = service(1);
        let (view, created) = s.submit("k", spec("a", 5)).unwrap();
        check!("(status, created, work calls on return)", (view.status, created, p.called().len()), (Status::Queued, true, 0));
    });
}

#[test]
fn finished_workers_are_not_kept() {
    run(async {
        let (s, _) = service(4);
        for round in 0..10 {
            for i in 0..20 {
                add(&s, &format!("{round}-{i}"), 1);
            }
            sleep(ms(100)).await;
            add(&s, &format!("{round}-last"), 1);
            check!(format!("round {round}: held_tasks() right after a submit with one job unfinished"), s.held_tasks(), 1);
            sleep(ms(10)).await;
        }
    });
}

#[test]
fn clones_are_one_service() {
    run(async {
        let (s, p) = service(1);
        let other = s.clone();
        add(&s, "a", 10);
        add(&other, "b", 10);
        sleep(ms(50)).await;
        check!("shared slots: (ids visible, most at once)", (other.store().get(2).is_some(), p.peak()), (true, 1));
    });
}

#[test]
fn a_busy_multi_thread_runtime() {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_time().build().unwrap();
    runtime.block_on(async {
        let running = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let (r, k) = (running.clone(), peak.clone());
        let s = JobService::new(3, move |_| {
            let (r, k) = (r.clone(), k.clone());
            async move {
                let now = r.fetch_add(1, Ordering::SeqCst) + 1;
                k.fetch_max(now, Ordering::SeqCst);
                tokio::task::yield_now().await;
                r.fetch_sub(1, Ordering::SeqCst);
                Ok(String::new())
            }
        });
        for i in 0..2000 {
            s.submit(&i.to_string(), spec("x", 0)).unwrap();
        }
        for _ in 0..2000 {
            if (1..=2000).all(|id| status(&s, id) == Status::Succeeded) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
            tokio::task::yield_now().await;
        }
        check!("all 2000 succeeded", (1..=2000).all(|id| status(&s, id) == Status::Succeeded), true);
        check!("limit of 3 respected", peak.load(Ordering::SeqCst) <= 3, true);
    });
}

#[test]
fn random_loads_match_a_simulation() {
    let mut rng = anneal_prelude::Rng::new(22004);
    for _ in 0..150 {
        let limit = 1 + rng.below(4);
        let n = 1 + rng.below(10);
        let jobs: Vec<(u64, usize)> = (0..n).map(|_| (1 + rng.below(9) as u64, rng.below(4))).collect();
        // Jobs take slots in id order: each starts when the earliest slot is free.
        // A panic while the future is being made (kind 3) takes no time at all.
        let mut free = vec![0u64; limit];
        let want_starts: Vec<u64> = jobs
            .iter()
            .map(|&(took, kind)| {
                let slot = free.iter_mut().min().unwrap();
                let at = *slot;
                *slot += if kind == 3 { 0 } else { took };
                at
            })
            .collect();
        let got = run(async {
            let (s, p) = service(limit);
            for (i, &(took, kind)) in jobs.iter().enumerate() {
                add(&s, &format!("{}-{i}", ["ok", "bad", "panic", "early"][kind]), if kind == 3 { 0 } else { took });
            }
            sleep(ms(500)).await;
            let statuses: Vec<Status> = (1..=n as u64).map(|id| status(&s, id)).collect();
            (p.calls().into_iter().map(|c| c.0).collect::<Vec<u64>>(), statuses)
        });
        let want_statuses: Vec<Status> = jobs.iter().map(|j| if j.1 == 0 { Status::Succeeded } else { Status::Failed }).collect();
        check!(format!("limit {limit}, (ms, kind) {jobs:?}: (start times, statuses)"), got, (want_starts, want_statuses));
    }
}
