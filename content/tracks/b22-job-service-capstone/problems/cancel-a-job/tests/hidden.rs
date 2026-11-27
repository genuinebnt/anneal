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
fn errors_name_the_state() {
    run(async {
        let (s, _) = service(1);
        let ok = add(&s, "ok", 1);
        let bad = add(&s, "bad", 1);
        let gone = add(&s, "gone", 1);
        let _ = s.store().cancel(gone);
        sleep(ms(20)).await;
        for (id, name) in [(ok, "succeeded"), (bad, "failed"), (gone, "cancelled")] {
            check!(format!("cancel a job that {name}"), s.store().cancel(id), Err(ApiError::NotCancellable(name.to_string())));
            check!(format!("job {id} is unchanged"), job(&s, id).status.as_str(), name);
        }
    });
}

#[test]
fn bad_ids_over_http() {
    run(async {
        let (s, _) = service(1);
        let app = router(s);
        for raw in ["1", "0", "abc", "-1", "%FF"] {
            let r = send(&app, "POST", &format!("/v1/jobs/{raw}/cancel"), &[], "").await;
            check!(format!("POST /v1/jobs/{raw}/cancel"), r.error(), failure(404, "job_not_found"));
        }
        check!("GET on the cancel route", get(&app, "/v1/jobs/1/cancel").await.error(), failure(405, "method_not_allowed"));
    });
}

#[test]
fn cancelled_before_its_worker_ever_ran() {
    run(async {
        for attempt in 0..200 {
            let (s, p) = service(1);
            let id = add(&s, "never", 5);
            // Nothing has yielded since the submit, so no worker has run yet.
            let accepted = s.store().cancel(id).is_ok();
            settle().await;
            check!(format!("attempt {attempt}: (accepted, status, work calls, history)"), (accepted, status(&s, id), p.called().len(), history(&s, id)), (true, Status::Cancelled, 0, vec!["job.queued", "job.cancelled"]));
        }
    });
}

#[test]
fn an_accepted_cancel_wins_over_a_late_result() {
    run(async {
        for attempt in 0..100 {
            let done = Arc::new(tokio::sync::Notify::new());
            let wait = done.clone();
            let s = JobService::new(1, move |_| {
                let wait = wait.clone();
                async move {
                    wait.notified().await;
                    Ok("finished anyway".to_string())
                }
            });
            s.submit("k", spec("a", 1)).unwrap();
            settle().await;
            // The work can now finish, but nobody has run since.
            done.notify_one();
            let accepted = s.store().cancel(1).is_ok();
            settle().await;
            check!(format!("attempt {attempt}: (accepted, status, output)"), (accepted, status(&s, 1), job(&s, 1).output), (true, Status::Cancelled, None));
        }
    });
}

#[test]
fn the_store_applies_the_same_rule() {
    let store = JobStore::new();
    store.submit("k", spec("a", 1)).unwrap();
    store.submit("l", spec("b", 1)).unwrap();
    store.start(1);
    let _ = store.cancel(1);
    check!("finish with success after an accepted cancel", store.finish(1, Outcome::Succeeded("x".into())), true);
    let job = store.get(1).unwrap();
    check!("(status, output)", (job.status, job.output), (Status::Cancelled, None));
    let _ = store.cancel(2);
    check!("start after an accepted cancel", store.start(2), false);
    check!("still queued until somebody finishes it", store.get(2).map(|j| j.status), Some(Status::Queued));
    check!("finish it", (store.finish(2, Outcome::Cancelled), store.get(2).map(|j| j.status)), (true, Some(Status::Cancelled)));
}

#[test]
fn cancelling_only_asks() {
    run(async {
        let (s, _) = service(1);
        let id = add(&s, "long", 5_000);
        settle().await;
        let first = s.store().cancel(id).map(|j| j.status);
        let second = s.store().cancel(id).map(|j| j.status);
        check!("two cancels before the worker reacts", (first, second), (Ok(Status::Running), Ok(Status::Running)));
        settle().await;
        check!("one history entry for it", history(&s, id), vec!["job.queued", "job.started", "job.cancelled"]);
    });
}

#[test]
fn the_slot_is_free_for_the_next_job() {
    run(async {
        let (s, p) = service(1);
        let a = add(&s, "a", 5_000);
        add(&s, "b", 5);
        sleep(ms(10)).await;
        let _ = s.store().cancel(a);
        sleep(ms(20)).await;
        check!("(calls, status of b, most at once)", (p.called(), status(&s, 2), p.peak()), (vec!["a".to_string(), "b".to_string()], Status::Succeeded, 1));
    });
}

#[test]
fn cancelled_queued_jobs_use_no_slot() {
    run(async {
        let (s, p) = service(1);
        let ids: Vec<u64> = (0..6).map(|i| add(&s, &format!("j{i}"), 10)).collect();
        settle().await;
        for &id in &ids[1..5] {
            let _ = s.store().cancel(id);
        }
        sleep(ms(100)).await;
        check!("(start ms, source)", p.calls(), vec![(0, "j0".to_string()), (10, "j5".to_string())]);
        check!("statuses", ids.iter().map(|&id| status(&s, id)).collect::<Vec<_>>(), [Status::Succeeded, Status::Cancelled, Status::Cancelled, Status::Cancelled, Status::Cancelled, Status::Succeeded].to_vec());
    });
}

#[test]
fn the_slot_waits_until_the_work_is_gone() {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_time().build().unwrap();
    runtime.block_on(async {
        let first_alive = Arc::new(AtomicUsize::new(0));
        let overlap = Arc::new(AtomicUsize::new(0));
        let (started_tx, started) = tokio::sync::oneshot::channel::<()>();
        let started_tx = Arc::new(Mutex::new(Some(started_tx)));
        // The busy work blocks inside a poll until the test lets it go, however slow the machine is.
        let (release, held) = std::sync::mpsc::channel::<()>();
        let held = Arc::new(Mutex::new(Some(held)));
        let (alive, seen) = (first_alive.clone(), overlap.clone());
        let s = JobService::new(1, move |spec: Spec| {
            let (alive, seen, tx) = (alive.clone(), seen.clone(), started_tx.clone());
            let held = held.clone();
            let guard = (spec.source == "busy").then(|| {
                alive.fetch_add(1, Ordering::SeqCst);
                Alive(alive.clone())
            });
            if spec.source != "busy" {
                seen.fetch_add(alive.load(Ordering::SeqCst), Ordering::SeqCst);
            }
            async move {
                if let Some(guard) = guard {
                    if let Some(tx) = tx.lock().unwrap().take() {
                        let _ = tx.send(());
                    }
                    // Holds its thread: an abort cannot take effect until this poll returns.
                    let held = held.lock().unwrap().take();
                    if let Some(held) = held {
                        let _ = held.recv_timeout(Duration::from_secs(8));
                    }
                    sleep(Duration::from_secs(60)).await;
                    drop(guard);
                }
                Ok(String::new())
            }
        });
        s.submit("1", spec("busy", 0)).unwrap();
        // Submitted only once job 1 holds the slot: order is not promised on this runtime.
        started.await.unwrap();
        s.submit("2", spec("next", 0)).unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        s.store().cancel(1).unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let early = (status(&s, 1), status(&s, 2));
        let _ = release.send(());
        for _ in 0..400 {
            if status(&s, 2).is_final() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        check!("while the cancelled work still holds its thread: (job 1, job 2)", early, (Status::Running, Status::Queued));
        check!("work of job 1 alive when job 2 was called", overlap.load(Ordering::SeqCst), 0);
        check!("in the end", (status(&s, 1), status(&s, 2)), (Status::Cancelled, Status::Succeeded));
    });
}

#[test]
fn random_cancels_keep_the_promises() {
    let mut rng = anneal_prelude::Rng::new(22005);
    for _ in 0..150 {
        let limit = 1 + rng.below(3);
        let n = 1 + rng.below(8);
        let durations: Vec<u64> = rng.vec(n, 1, 8);
        let cancels: Vec<(u64, u64)> = (0..rng.below(6)).map(|_| (rng.below(30) as u64, 1 + rng.below(n) as u64)).collect();
        let shown = format!("limit {limit}, durations {durations:?}, cancels (ms, id) {cancels:?}");
        run(async {
            let (s, p) = service(limit);
            for (i, &d) in durations.iter().enumerate() {
                add(&s, &i.to_string(), d);
            }
            let mut order = cancels.clone();
            order.sort();
            let mut accepted = std::collections::BTreeSet::new();
            let mut now = 0;
            for (at, id) in order {
                sleep(ms(at - now)).await;
                now = at;
                let was_final = status(&s, id).is_final();
                let ok = s.store().cancel(id).is_ok();
                check!(format!("{shown}: cancel {id} at {at} ms accepted"), ok, !was_final);
                if ok {
                    accepted.insert(id);
                }
            }
            sleep(ms(500)).await;
            for id in 1..=n as u64 {
                let want = if accepted.contains(&id) { Status::Cancelled } else { Status::Succeeded };
                check!(format!("{shown}: job {id}"), status(&s, id), want);
            }
            check!(format!("{shown}: (alive, within limit)"), (p.alive(), p.peak() <= limit), (0, true));
        });
    }
}
