use solution::*;

#[allow(unused_imports)]
use axum::Router;
#[allow(unused_imports)]
use axum::body::{Body, to_bytes};
#[allow(unused_imports)]
use axum::http::Request;
#[allow(unused_imports)]
use axum::routing::{get, post};
#[allow(unused_imports)]
use std::cell::RefCell;
#[allow(unused_imports)]
use std::collections::BTreeMap;
#[allow(unused_imports)]
use std::time::Duration;
#[allow(unused_imports)]
use tower::ServiceExt;
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
pub struct SpanSeen {
    pub name: &'static str,
    pub level: String,
    pub fields: Fields,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct EventSeen {
    pub from_handler: bool,
    pub level: String,
    pub fields: Fields,
    /// The spans around it, outermost first.
    pub spans: Vec<SpanSeen>,
}

thread_local! {
    static EVENTS: RefCell<Vec<EventSeen>> = const { RefCell::new(Vec::new()) };
}

#[allow(dead_code)]
struct Capture;

#[allow(dead_code)]
impl<S: tracing::Subscriber + for<'a> LookupSpan<'a>> Layer<S> for Capture {
    fn on_new_span(&self, attrs: &tracing::span::Attributes<'_>, id: &tracing::span::Id, ctx: Context<'_, S>) {
        let mut fields = Fields::default();
        attrs.record(&mut fields);
        let meta = attrs.metadata();
        ctx.span(id).unwrap().extensions_mut().insert(SpanSeen { name: meta.name(), level: meta.level().to_string(), fields });
    }

    fn on_record(&self, id: &tracing::span::Id, values: &tracing::span::Record<'_>, ctx: Context<'_, S>) {
        let span = ctx.span(id).unwrap();
        let mut extensions = span.extensions_mut();
        values.record(&mut extensions.get_mut::<SpanSeen>().unwrap().fields);
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        let target = event.metadata().target();
        let from_handler = target == "handler";
        if !from_handler && !target.starts_with("solution") {
            return;
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        let spans = ctx.event_scope(event).map(|scope| scope.from_root().map(|s| s.extensions().get::<SpanSeen>().unwrap().clone()).collect()).unwrap_or_default();
        EVENTS.with_borrow_mut(|events| events.push(EventSeen { from_handler, level: event.metadata().level().to_string(), fields, spans }));
    }
}

#[allow(dead_code)]
/// Runs `f` on a current-thread runtime and returns its result with every event logged on this thread.
pub fn capture<F: std::future::Future>(f: F) -> (F::Output, Vec<EventSeen>) {
    static INSTALL: std::sync::Once = std::sync::Once::new();
    INSTALL.call_once(|| {
        tracing::subscriber::set_global_default(tracing_subscriber::registry().with(Capture)).expect("no other subscriber is installed");
    });
    EVENTS.with_borrow_mut(Vec::clear);
    let output = tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap().block_on(f);
    (output, EVENTS.with_borrow_mut(std::mem::take))
}

#[allow(dead_code)]
async fn work(axum::extract::Path(ms): axum::extract::Path<u64>) -> &'static str {
    tracing::info!(target: "handler", tag = ms, "start");
    tokio::time::sleep(Duration::from_millis(ms)).await;
    tracing::info!(target: "handler", tag = ms, "end");
    "done"
}

#[allow(dead_code)]
pub fn app() -> Router {
    observe(
        Router::new()
            .route("/work/{ms}", get(work))
            .route("/items", post(|| async { (axum::http::StatusCode::CREATED, [("x-made", "yes")], "made") })),
    )
}

#[allow(dead_code)]
pub async fn send(app: &Router, method: &str, uri: &str, headers: &[(&str, &str)]) -> (u16, String) {
    let mut request = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = app.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status().as_u16();
    let body = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, String::from_utf8_lossy(&body).into_owned())
}

#[allow(dead_code)]
pub fn field(fields: &Fields, name: &str) -> String {
    fields.0.get(name).cloned().unwrap_or_else(|| "<missing>".to_string())
}

#[allow(dead_code)]
/// For each event from `observe`: (level, status, [method, path, client] of its `http` span).
pub fn lines(events: &[EventSeen]) -> Vec<(String, String, [String; 3])> {
    events
        .iter()
        .filter(|e| !e.from_handler)
        .map(|e| {
            let spans: Vec<&SpanSeen> = e.spans.iter().filter(|s| s.name == "http").collect();
            let of = |name: &str| match spans.as_slice() {
                [span] => field(&span.fields, name),
                other => format!("<{} http spans>", other.len()),
            };
            (e.level.clone(), field(&e.fields, "status"), [of("method"), of("path"), of("client")])
        })
        .collect()
}

#[allow(dead_code)]
pub fn line(level: &str, status: &str, method: &str, path: &str, client: &str) -> (String, String, [String; 3]) {
    (level.to_string(), status.to_string(), [method.to_string(), path.to_string(), client.to_string()])
}

#[test]
fn wrong_methods_are_logged_too() {
    let (reply, events) = capture(async { send(&app(), "DELETE", "/items", &[]).await });
    check!("status", reply.0, 405);
    check!("events", lines(&events), vec![line("INFO", "405", "DELETE", "/items", "unknown")]);
}

#[test]
fn headers_and_body_pass_through() {
    let (got, _) = capture(async {
        let request = Request::builder().method("POST").uri("/items").body(Body::empty()).unwrap();
        let response = app().oneshot(request).await.unwrap();
        let made = response.headers().get("x-made").map(|v| v.to_str().unwrap().to_string());
        let extra: Vec<String> = response.headers().keys().map(|k| k.to_string()).filter(|k| !["x-made", "content-type", "content-length"].contains(&k.as_str())).collect();
        let status = response.status().as_u16();
        let body = to_bytes(response.into_body(), 100).await.unwrap();
        (status, made, extra, body.to_vec())
    });
    check!("(status, x-made, added headers, body)", got, (201, Some("yes".to_string()), vec![], b"made".to_vec()));
}

#[test]
fn the_span_is_at_info() {
    let (_, events) = capture(async { send(&app(), "GET", "/work/1", &[]).await });
    let levels: Vec<String> = events.iter().flat_map(|e| e.spans.iter().map(|s| s.level.clone())).collect();
    check!("level of the span around each of the three events", levels, vec!["INFO".to_string(); 3]);
}

#[test]
fn client_names_are_checked() {
    let long = "a".repeat(33);
    let ok = "Ab-9".repeat(8);
    for (sent, want) in [("", "unknown"), ("a b", "unknown"), ("a_b", "unknown"), ("x\"y", "unknown"), ("é", "unknown"), (long.as_str(), "unknown"), (ok.as_str(), ok.as_str()), ("-", "-"), ("unknown", "unknown")] {
        let (_, events) = capture(async { send(&app(), "GET", "/work/1", &[("x-client", sent)]).await });
        check!(format!("x-client: {sent:?}"), lines(&events), vec![line("INFO", "200", "GET", "/work/1", want)]);
    }
}

#[test]
fn interleaved_requests_stay_apart() {
    let (_, events) = capture(async {
        let app = app();
        let a = send(&app, "GET", "/work/30", &[("x-client", "a")]);
        let b = send(&app, "GET", "/work/10", &[("x-client", "b")]);
        let c = send(&app, "GET", "/work/20", &[("x-client", "c")]);
        tokio::join!(a, b, c)
    });
    for e in events.iter().filter(|e| e.from_handler) {
        let tag = field(&e.fields, "tag");
        let paths: Vec<String> = e.spans.iter().map(|s| field(&s.fields, "path")).collect();
        check!(format!("spans around a handler event of /work/{tag}"), paths, vec![format!("/work/{tag}")]);
    }
    check!("one line each, in the order they finish", lines(&events), vec![line("INFO", "200", "GET", "/work/10", "b"), line("INFO", "200", "GET", "/work/20", "c"), line("INFO", "200", "GET", "/work/30", "a")]);
}

#[test]
fn an_abandoned_request() {
    let (_, events) = capture(async {
        let gave_up = tokio::time::timeout(Duration::from_millis(10), send(&app(), "GET", "/work/500", &[("x-client", "bye")])).await;
        assert!(gave_up.is_err());
        tokio::time::sleep(Duration::from_secs(2)).await;
    });
    check!("events from observe", lines(&events), vec![line("WARN", "<missing>", "GET", "/work/500", "bye")]);
}

#[test]
fn abandoned_while_another_request_runs() {
    let (_, events) = capture(async {
        let app = app();
        let slow = tokio::time::timeout(Duration::from_millis(10), send(&app, "GET", "/work/500", &[]));
        let other = send(&app, "GET", "/work/40", &[]);
        tokio::join!(slow, other)
    });
    check!("events", lines(&events), vec![line("WARN", "<missing>", "GET", "/work/500", "unknown"), line("INFO", "200", "GET", "/work/40", "unknown")]);
}

#[test]
fn nothing_is_logged_before_a_request() {
    let (_, events) = capture(async {
        let _unused = app();
    });
    check!("events", events.len(), 0);
}

#[test]
fn logging_does_not_leak_between_requests() {
    let (_, events) = capture(async {
        let app = app();
        send(&app, "GET", "/work/1", &[("x-client", "first")]).await;
        tracing::info!(target: "handler", tag = "outside", "between requests");
        send(&app, "GET", "/work/2", &[]).await;
    });
    let outside: Vec<usize> = events.iter().filter(|e| field(&e.fields, "tag") == "outside").map(|e| e.spans.len()).collect();
    check!("spans around an event logged between requests", outside, vec![0]);
    check!("events", lines(&events), vec![line("INFO", "200", "GET", "/work/1", "first"), line("INFO", "200", "GET", "/work/2", "unknown")]);
}

#[test]
fn random_batches() {
    let mut rng = anneal_prelude::Rng::new(21309);
    for _ in 0..120 {
        // Distinct handler times, so each request can be told apart.
        let mut times: Vec<u64> = (1..=30).collect();
        rng.shuffle(&mut times);
        let n = rng.below(5) + 1;
        // (handler ms, gives up after)
        let plan: Vec<(u64, Option<u64>)> = (0..n).map(|i| (times[i], rng.bool().then(|| rng.below(35) as u64))).map(|(t, g)| (t * 2, g.map(|g| g * 2 + 1))).collect();
        let (_, events) = capture(async {
            let app = app();
            let tasks: Vec<_> = plan
                .iter()
                .map(|&(took, give_up)| {
                    let app = app.clone();
                    async move {
                        let request = format!("/work/{took}");
                        let reply = send(&app, "GET", &request, &[]);
                        match give_up {
                            Some(after) => drop(tokio::time::timeout(Duration::from_millis(after), reply).await),
                            None => drop(reply.await),
                        }
                    }
                })
                .collect();
            futures::future::join_all(tasks).await;
        });
        let mut got = lines(&events);
        got.sort();
        let mut want: Vec<_> = plan
            .iter()
            .map(|&(took, give_up)| {
                let abandoned = give_up.is_some_and(|g| g < took);
                line(if abandoned { "WARN" } else { "INFO" }, if abandoned { "<missing>" } else { "200" }, "GET", &format!("/work/{took}"), "unknown")
            })
            .collect();
        want.sort();
        check!(format!("(handler ms, gives up after) {plan:?}"), got, want);
        for e in events.iter().filter(|e| e.from_handler) {
            let tag = field(&e.fields, "tag");
            let paths: Vec<String> = e.spans.iter().map(|s| field(&s.fields, "path")).collect();
            check!(format!("{plan:?}: spans around a handler event of /work/{tag}"), paths, vec![format!("/work/{tag}")]);
        }
    }
}
