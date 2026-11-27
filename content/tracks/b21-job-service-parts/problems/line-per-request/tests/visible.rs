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
fn one_line_for_a_request() {
    let (reply, events) = capture(async { send(&app(), "GET", "/work/5", &[("x-client", "web-1")]).await });
    check!("response passes through", reply, (200, "done".to_string()));
    check!("events from observe: (level, status, [method, path, client])", lines(&events), vec![line("INFO", "200", "GET", "/work/5", "web-1")]);
}

#[test]
fn the_handler_is_inside_the_span() {
    let (_, events) = capture(async { send(&app(), "GET", "/work/5", &[]).await });
    let inside: Vec<Vec<&str>> = events.iter().filter(|e| e.from_handler).map(|e| e.spans.iter().map(|s| s.name).collect()).collect();
    check!("spans around the handler's two events", inside, vec![vec!["http"], vec!["http"]]);
}

#[test]
fn the_query_is_not_logged() {
    let (_, events) = capture(async { send(&app(), "GET", "/work/1?token=secret", &[]).await });
    check!("path", lines(&events), vec![line("INFO", "200", "GET", "/work/1", "unknown")]);
}

#[test]
fn unknown_paths_are_logged_too() {
    let (reply, events) = capture(async { send(&app(), "GET", "/nope", &[]).await });
    check!("status", reply.0, 404);
    check!("events", lines(&events), vec![line("INFO", "404", "GET", "/nope", "unknown")]);
}

#[test]
fn two_requests_two_lines() {
    let (_, events) = capture(async {
        let app = app();
        send(&app, "GET", "/work/1", &[]).await;
        send(&app, "POST", "/items", &[]).await;
    });
    check!("events", lines(&events), vec![line("INFO", "200", "GET", "/work/1", "unknown"), line("INFO", "201", "POST", "/items", "unknown")]);
}
