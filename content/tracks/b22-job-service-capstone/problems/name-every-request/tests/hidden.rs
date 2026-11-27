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

#[allow(unused_imports)]
use std::cell::RefCell;
#[allow(unused_imports)]
use std::collections::BTreeMap;
#[allow(unused_imports)]
use tracing::field::{Field, Visit};
#[allow(unused_imports)]
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
#[allow(unused_imports)]
use tracing_subscriber::registry::LookupSpan;

#[allow(dead_code)]
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Fields(pub BTreeMap<String, String>);

#[allow(dead_code)]
impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let text = format!("{value:?}");
        let text = text.strip_prefix('"').and_then(|t| t.strip_suffix('"')).map(str::to_string).unwrap_or(text);
        self.0.insert(field.name().to_string(), text);
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct EventSeen {
    pub level: String,
    pub fields: Fields,
    /// Name and fields of the spans around it, outermost first.
    pub spans: Vec<(&'static str, Fields)>,
}

thread_local! {
    static EVENTS: RefCell<Vec<EventSeen>> = const { RefCell::new(Vec::new()) };
}

#[allow(dead_code)]
struct SpanFields(Fields);

#[allow(dead_code)]
struct Capture;

#[allow(dead_code)]
impl<S: tracing::Subscriber + for<'a> LookupSpan<'a>> Layer<S> for Capture {
    fn on_new_span(&self, attrs: &tracing::span::Attributes<'_>, id: &tracing::span::Id, ctx: Context<'_, S>) {
        let mut fields = Fields::default();
        attrs.record(&mut fields);
        ctx.span(id).unwrap().extensions_mut().insert(SpanFields(fields));
    }

    fn on_record(&self, id: &tracing::span::Id, values: &tracing::span::Record<'_>, ctx: Context<'_, S>) {
        let span = ctx.span(id).unwrap();
        let mut extensions = span.extensions_mut();
        values.record(&mut extensions.get_mut::<SpanFields>().unwrap().0);
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        if !event.metadata().target().starts_with("solution") {
            return;
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        let spans = ctx.event_scope(event).map(|scope| scope.from_root().map(|s| (s.name(), s.extensions().get::<SpanFields>().unwrap().0.clone())).collect()).unwrap_or_default();
        EVENTS.with_borrow_mut(|events| events.push(EventSeen { level: event.metadata().level().to_string(), fields, spans }));
    }
}

#[allow(dead_code)]
/// Runs `f` and returns its result with every event the solution logged on this thread.
pub fn capture<F: std::future::Future>(f: F) -> (F::Output, Vec<EventSeen>) {
    static INSTALL: std::sync::Once = std::sync::Once::new();
    INSTALL.call_once(|| {
        tracing::subscriber::set_global_default(tracing_subscriber::registry().with(Capture)).expect("no other subscriber is installed");
    });
    EVENTS.with_borrow_mut(Vec::clear);
    let output = run(f);
    (output, EVENTS.with_borrow_mut(std::mem::take))
}

#[allow(dead_code)]
/// For each event with a `status` field inside a `request` span: (level, status, [request_id, method, path]).
pub fn lines(events: &[EventSeen]) -> Vec<(String, String, [String; 3])> {
    events
        .iter()
        .filter(|e| e.fields.0.contains_key("status"))
        .filter_map(|e| {
            let spans: Vec<&Fields> = e.spans.iter().filter(|s| s.0 == "request").map(|s| &s.1).collect();
            let [span] = spans.as_slice() else {
                return (!spans.is_empty()).then(|| ("nested request spans".to_string(), String::new(), Default::default()));
            };
            let of = |name: &str| span.0.get(name).cloned().unwrap_or_else(|| "<missing>".to_string());
            Some((e.level.clone(), e.fields.0["status"].clone(), [of("request_id"), of("method"), of("path")]))
        })
        .collect()
}

#[allow(dead_code)]
pub fn line(status: u16, id: &str, method: &str, path: &str) -> (String, String, [String; 3]) {
    ("INFO".to_string(), status.to_string(), [id.to_string(), method.to_string(), path.to_string()])
}

#[allow(dead_code)]
pub async fn get_as(app: &Router, uri: &str, id: &str) -> Reply {
    send(app, "GET", uri, &[("x-request-id", id)], "").await
}

#[allow(dead_code)]
pub fn app() -> Router {
    router(service(2).0)
}

#[test]
fn what_is_accepted() {
    run(async {
        let app = app();
        let longest = "x".repeat(64);
        for id in ["a", "!~#$%^&*()_+=[]{};:,.<>?/|", "req-1", longest.as_str()] {
            check!(format!("x-request-id {id:?}"), get_as(&app, "/v1/jobs", id).await.header("x-request-id"), Some(id.to_string()));
        }
    });
}

#[test]
fn what_is_replaced() {
    run(async {
        let app = app();
        let long = "x".repeat(65);
        for bad in ["", "has space", "tab\there", "é", long.as_str()] {
            let got = get_as(&app, "/v1/jobs", bad).await.header("x-request-id").unwrap_or_default();
            check!(format!("x-request-id {bad:?} gave {got:?}: replaced by a req- id"), got.starts_with("req-") && got != bad, true);
        }
    });
}

#[test]
fn made_ids_never_repeat() {
    run(async {
        let (first, second) = (app(), app());
        let mut seen = std::collections::HashSet::new();
        for _ in 0..500 {
            for app in [&first, &second] {
                let id = get(app, "/v1/jobs").await.header("x-request-id").unwrap_or_default();
                check!(format!("{id:?} is new, across two routers"), seen.insert(id.clone()), true);
            }
        }
    });
}

#[test]
fn every_kind_of_answer_has_the_header() {
    run(async {
        let app = app();
        post_job(&app, "k", &job_json("a", 1)).await;
        sleep(ms(10)).await;
        let mut got = Vec::new();
        for (method, uri, body) in [("POST", "/v1/jobs", "{"), ("DELETE", "/v1/jobs", ""), ("GET", "/v1/jobs/9", ""), ("POST", "/v1/jobs/1/cancel", ""), ("GET", "/v1/jobs?limit=x", ""), ("GET", "/v1/jobs/1/events", ""), ("GET", "/nope", "")] {
            let r = send(&app, method, uri, &[("idempotency-key", "k"), ("x-request-id", "same")], body).await;
            got.push((method, uri, r.status, r.header("x-request-id")));
        }
        let want: Vec<_> = [("POST", "/v1/jobs", 422), ("DELETE", "/v1/jobs", 405), ("GET", "/v1/jobs/9", 404), ("POST", "/v1/jobs/1/cancel", 409), ("GET", "/v1/jobs?limit=x", 400), ("GET", "/v1/jobs/1/events", 200), ("GET", "/nope", 404)]
            .into_iter()
            .map(|(m, u, s)| (m, u, s, Some("same".to_string())))
            .collect();
        check!("(method, uri, status, x-request-id)", got, want);
    });
}

#[test]
fn responses_are_otherwise_unchanged() {
    run(async {
        let app = app();
        post_job(&app, "k", &job_json("a", 1)).await;
        let r = post_job(&app, "k", &job_json("a", 1)).await;
        check!("replay: (status, marker, content type)", (r.status, r.header("idempotent-replayed"), r.header("content-type")), (200, Some("true".to_string()), Some("application/json".to_string())));
    });
}

#[test]
fn an_event_stream_is_not_held_back() {
    run(async {
        let (s, _) = service(1);
        add(&s, "a", 60_000);
        let mut c = listen(&router(s), 1, None).await;
        check!("first event while the job runs", c.read().await, event(1, 1, "queued"));
    });
}

#[test]
fn failures_are_logged_with_their_status() {
    let (ids, events) = capture(async {
        let app = app();
        get_as(&app, "/nope?secret=1", "n1").await;
        send(&app, "DELETE", "/v1/jobs", &[("x-request-id", "n2")], "").await;
        send(&app, "POST", "/v1/jobs", &[("x-request-id", "n3")], "{}").await;
        get(&app, "/v1/jobs/7").await.header("x-request-id").unwrap_or_default()
    });
    check!("lines", lines(&events), vec![line(404, "n1", "GET", "/nope"), line(405, "n2", "DELETE", "/v1/jobs"), line(400, "n3", "POST", "/v1/jobs"), line(404, &ids, "GET", "/v1/jobs/7")]);
}

#[test]
fn concurrent_requests_do_not_mix() {
    let (_, events) = capture(async {
        let app = app();
        let uris: Vec<(String, String)> = (0..20).map(|i| (format!("/v1/jobs/{i}"), format!("id-{i}"))).collect();
        let all = uris.iter().map(|(uri, id)| get_as(&app, uri, id));
        futures::future::join_all(all).await;
    });
    let mut got = lines(&events);
    got.sort();
    let mut want: Vec<_> = (0..20).map(|i| line(404, &format!("id-{i}"), "GET", &format!("/v1/jobs/{i}"))).collect();
    want.sort();
    check!("one line each", got, want);
}

#[test]
fn the_rules_of_earlier_steps_still_hold() {
    run(async {
        let (s, p) = service(1);
        let app = router(s.clone());
        post_job(&app, "a", &job_json("a", 40)).await;
        post_job(&app, "b", &job_json("b", 40)).await;
        post_job(&app, "a", &job_json("a", 40)).await;
        send(&app, "POST", "/v1/jobs/2/cancel", &[], "").await;
        let got = tokio::time::timeout(ms(10_000), s.shutdown()).await.expect("shutdown never returned");
        check!("report", (got.total, got.succeeded, got.cancelled, got.unfinished), (2, 1, 1, 0));
        check!("work calls and limit", (p.called(), p.peak()), (vec!["a".to_string()], 1));
        check!("new work is refused", post_job(&app, "c", &job_json("c", 1)).await.error(), failure(503, "shutting_down"));
        check!("the largest cursor", get(&app, "/v1/jobs?after=18446744073709551615").await.status, 200);
    });
}

#[test]
fn random_ids_follow_the_rule() {
    let mut rng = anneal_prelude::Rng::new(22008);
    for _ in 0..60 {
        let len = rng.below(70);
        let id = rng.string(len, "aaaaaaaaaaaaaaaaaaaaZ9-_.:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ");
        let (got, events) = capture(async { get_as(&app(), "/v1/jobs", &id).await.header("x-request-id").unwrap_or_default() });
        let keep = (1..=64).contains(&id.len()) && !id.contains(' ');
        if keep {
            check!(format!("{id:?} is kept"), got.clone(), id.clone());
        } else {
            check!(format!("{id:?} is replaced"), got.starts_with("req-"), true);
        }
        check!(format!("{id:?}: logged under the id sent back"), lines(&events), vec![line(200, &got, "GET", "/v1/jobs")]);
    }
}
