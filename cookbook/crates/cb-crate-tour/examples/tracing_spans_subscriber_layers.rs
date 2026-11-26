//! RECIPE 1113 (crates): tracing: events, spans, #[instrument], filtering, fmt/JSON output, custom layers and async context
//! LEVEL: intermediate
//! Use in: structured logs and request correlation in services, especially async ones where thread ids mean little.   Run: cargo run --release -p cb-crate-tour --example tracing_spans_subscriber_layers
//! COMMON USES: per-request spans in web servers, JSON logs for aggregators, test assertions on emitted events, metrics from events.
//! METHODS: tracing::{trace!, debug!, info!, warn!, error!, event!}, tracing::info_span!, #[tracing::instrument] (skip, fields, ret, err, level, name), Span::enter, Span::in_scope, Span::entered, Span::record, Span::current, tracing::field::Empty, tracing::Instrument::instrument, tracing::enabled!, tracing::subscriber::with_default, tracing_subscriber::fmt, fmt::Layer::with_writer, with_target, without_time, with_ansi, json, flatten_event, FmtSpan, tracing_subscriber::EnvFilter::try_new, tracing_subscriber::filter::Targets, filter_fn, Layer::with_filter, tracing_subscriber::registry, SubscriberExt::with, Layer::{on_event, on_new_span, on_enter, on_exit, on_close}, tracing::field::Visit, tracing_subscriber::registry::LookupSpan, Instrument::in_current_span, info_span!(parent: ..), Span::{follows_from, none, is_disabled}, Layer::on_follows_from, tracing_subscriber::reload::{Layer::new, Handle::{reload, modify}}, EnvFilter span/field directives, Option<Layer>, tracing::subscriber::set_default
//! DOCS: https://docs.rs/tracing/0.1/tracing/ https://docs.rs/tracing-subscriber/0.3/tracing_subscriber/
//! CRATES: tracing = "0.1", tracing-subscriber = "0.3" (env-filter, json), tokio = "1", serde_json = "1"
//!
//! MODEL. An EVENT is a point in time with fields. A SPAN is a period with fields; events inside it inherit its context.
//! The SUBSCRIBER decides what happens to both. Libraries only emit; only the BINARY installs a subscriber
//! (`.init()` / set_global_default, once). This file uses `with_default`, which is scoped to the current thread, so
//! sections do not interfere with each other or with a test harness.
//!
//! CHEAT SHEET
//!   field with value            info!(user_id = 7, "msg")       Display: %value      Debug: ?value
//!   span around a function      #[instrument(skip(password), fields(request_id = %id), err)]
//!   fill a field later          declare tracing::field::Empty, then span.record("name", value)
//!   async                       future.instrument(span).await   (NEVER hold span.enter() across .await)
//!   spawned task                tokio::spawn(fut.in_current_span())   (a new task does NOT inherit the span)
//!   caused by, but outlives     background_span.follows_from(request_span.id())   (not parent/child)
//!   change filters at runtime   wrap the filter in reload::Layer and keep the Handle
//!   choose what is recorded     EnvFilter "warn,my_crate=debug,my_crate::db=trace"
//!
//! METRICS: counting events in a Layer (below) is fine for counters. For real metrics use a metrics library or an
//! OpenTelemetry layer (not covered here). SPAN TIMING: FmtSpan::CLOSE prints busy and idle time.
//!
//! Read: https://tokio.rs/tokio/topics/tracing ; https://docs.rs/tracing/latest/tracing/span/struct.Span.html#in-asynchronous-code
//!
//! TRY NEXT:
//!   - [S] Remove skip(password) and see what leaks.
//!   - [M] Write a layer that records a latency histogram per span name using on_new_span and on_close.
//!   - [A] PROJECT: give each request in `reqwest_axum_http_roundtrip` a span carrying a request id.
#![allow(dead_code)]
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::sync::{Arc, Mutex};
use tracing::field::{Field, Visit};
use tracing::{
    Instrument, Level, Subscriber, debug, error, info, info_span, instrument, trace, warn,
};
use tracing_subscriber::filter::{EnvFilter, Targets, filter_fn};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{Layer, registry};

/// An in-memory sink so output can be asserted.
#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);
impl Buffer {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}
impl Write for Buffer {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl<'a> MakeWriter<'a> for Buffer {
    type Writer = Buffer;
    fn make_writer(&'a self) -> Buffer {
        self.clone()
    }
}

/// A custom layer that stores every event with its fields and the path of spans around it.
#[derive(Debug, Clone, PartialEq)]
struct Captured {
    level: Level,
    target: String,
    fields: BTreeMap<String, String>,
    spans: Vec<String>,
}
#[derive(Clone, Default)]
struct Capture {
    events: Arc<Mutex<Vec<Captured>>>,
    log: Arc<Mutex<Vec<String>>>, // span lifecycle
}
#[derive(Default)]
struct Fields(BTreeMap<String, String>);
impl Visit for Fields {
    // record_debug is the only required method; override the typed ones to avoid quotes around strings.
    fn record_debug(&mut self, f: &Field, v: &dyn std::fmt::Debug) {
        self.0.insert(f.name().into(), format!("{v:?}"));
    }
    fn record_str(&mut self, f: &Field, v: &str) {
        self.0.insert(f.name().into(), v.into());
    }
}
/// Span fields are not stored by the registry for you: keep them in the span's extensions.
struct SpanFields(BTreeMap<String, String>);
impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for Capture {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: Context<'_, S>,
    ) {
        let mut f = Fields::default();
        attrs.record(&mut f);
        ctx.span(id)
            .unwrap()
            .extensions_mut()
            .insert(SpanFields(f.0));
        self.log
            .lock()
            .unwrap()
            .push(format!("new {}", attrs.metadata().name()));
    }
    fn on_record(
        &self,
        id: &tracing::span::Id,
        values: &tracing::span::Record<'_>,
        ctx: Context<'_, S>,
    ) {
        let mut f = Fields::default();
        values.record(&mut f);
        if let Some(s) = ctx
            .span(id)
            .unwrap()
            .extensions_mut()
            .get_mut::<SpanFields>()
        {
            s.0.extend(f.0);
        }
    }
    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        let mut f = Fields::default();
        event.record(&mut f);
        let spans = ctx.event_scope(event).map_or_else(Vec::new, |scope| {
            scope
                .from_root()
                .map(|s| {
                    let ext = s.extensions();
                    let kv: Vec<String> = ext.get::<SpanFields>().map_or_else(Vec::new, |f| {
                        f.0.iter().map(|(k, v)| format!("{k}={v}")).collect()
                    });
                    format!("{}{{{}}}", s.name(), kv.join(","))
                })
                .collect()
        });
        let m = event.metadata();
        self.events.lock().unwrap().push(Captured {
            level: *m.level(),
            target: m.target().into(),
            fields: f.0,
            spans,
        });
    }
    fn on_enter(&self, id: &tracing::span::Id, ctx: Context<'_, S>) {
        self.log
            .lock()
            .unwrap()
            .push(format!("enter {}", ctx.span(id).unwrap().name()));
    }
    fn on_exit(&self, id: &tracing::span::Id, ctx: Context<'_, S>) {
        self.log
            .lock()
            .unwrap()
            .push(format!("exit {}", ctx.span(id).unwrap().name()));
    }
    fn on_close(&self, id: tracing::span::Id, ctx: Context<'_, S>) {
        self.log
            .lock()
            .unwrap()
            .push(format!("close {}", ctx.span(&id).unwrap().name()));
    }
}

fn captured(f: impl FnOnce()) -> (Vec<Captured>, Vec<String>) {
    let c = Capture::default();
    tracing::subscriber::with_default(registry().with(c.clone()), f);
    let events = c.events.lock().unwrap().clone();
    let log = c.log.lock().unwrap().clone();
    (events, log)
}

fn events_and_fields() {
    let (events, _) = captured(|| {
        let path = std::path::Path::new("/tmp/x");
        info!(user_id = 7, ok = true, path = %path.display(), raw = ?path, "saved {} bytes", 42);
        warn!(target: "audit", "role changed");
        let attempts = 3;
        error!(attempts, "gave up"); // shorthand: field name = variable name
    });
    let f = &events[0].fields;
    assert_eq!(
        (
            f["user_id"].as_str(),
            f["ok"].as_str(),
            f["path"].as_str(),
            f["raw"].as_str()
        ),
        ("7", "true", "/tmp/x", "\"/tmp/x\"")
    );
    assert_eq!(
        f["message"], "saved 42 bytes",
        "the format string becomes the `message` field"
    );
    assert_eq!(
        (events[1].target.as_str(), events[1].level),
        ("audit", Level::WARN)
    );
    assert_eq!(events[2].fields["attempts"], "3");
    assert!(
        events[0].target.contains("tracing_spans"),
        "default target is the module path"
    );
}

#[derive(Debug)]
struct Declined;
impl std::fmt::Display for Declined {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("card declined")
    }
}
#[instrument(name = "charge", skip(card_number), fields(last4 = %&card_number[card_number.len() - 4..], outcome = tracing::field::Empty), err)]
fn charge(user: u32, card_number: &str, cents: u64) -> Result<(), Declined> {
    info!("charging");
    let ok = cents <= 1000;
    tracing::Span::current().record("outcome", if ok { "ok" } else { "declined" });
    debug!("decided");
    if ok { Ok(()) } else { Err(Declined) }
}
#[instrument(ret, level = "debug")]
fn double(n: u32) -> u32 {
    n * 2
}

fn spans_and_instrument() {
    let (events, log) = captured(|| {
        let request = info_span!("request", id = "r1");
        let _guard = request.enter(); // fine in SYNC code
        charge(5, "4111111111111111", 5000).unwrap_err();
        assert_eq!(double(4), 8);
    });
    let first = &events[0];
    assert_eq!(first.spans.len(), 2, "{first:?}");
    assert_eq!(first.spans[0], "request{id=r1}");
    let inner = &first.spans[1];
    assert!(
        inner.starts_with("charge{")
            && inner.contains("user=5")
            && inner.contains("cents=5000")
            && inner.contains("last4=1111"),
        "{inner}"
    );
    assert!(
        !format!("{events:?}").contains("4111111111111111"),
        "skip() kept the secret out"
    );
    assert!(
        events[1].spans[1].contains("outcome=declined"),
        "recorded after the span was created"
    );
    // `err` emits an ERROR event with the error's Display; `ret` emits the return value at the span's level.
    let err = events.iter().find(|e| e.level == Level::ERROR).unwrap();
    assert_eq!(err.fields["error"], "card declined");
    assert!(events.iter().any(
        |e| e.level == Level::DEBUG && e.fields.get("return").map(String::as_str) == Some("8")
    ));
    // Lifecycle: enter/exit may happen many times, close exactly once, children before parents.
    let closes: Vec<&String> = log.iter().filter(|l| l.starts_with("close")).collect();
    assert_eq!(closes, ["close charge", "close double", "close request"]);

    let (events, _) = captured(|| {
        let s = info_span!("job", n = 1);
        s.in_scope(|| info!("inside"));
        info!("outside");
        let _e = info_span!("owned").entered();
        info!("in owned");
    });
    assert_eq!(
        events.iter().map(|e| e.spans.join("/")).collect::<Vec<_>>(),
        ["job{n=1}", "", "owned{}"]
    );
}

/// Two tasks interleave on ONE thread. `.instrument(span)` enters the span on every poll and leaves on every yield.
fn async_context_propagation() {
    async fn step(who: u32) {
        for i in 0..3 {
            info!(who, i, "step");
            tokio::task::yield_now().await;
        }
    }
    let (events, log) = captured(|| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let a = tokio::spawn(step(1).instrument(info_span!("task", who = 1)));
            let b = tokio::spawn(step(2).instrument(info_span!("task", who = 2)));
            a.await.unwrap();
            b.await.unwrap();
        });
    });
    assert_eq!(events.len(), 6);
    for e in &events {
        assert_eq!(
            e.spans,
            [format!("task{{who={}}}", e.fields["who"])],
            "every event is attributed to ITS task"
        );
    }
    assert!(
        events
            .windows(2)
            .any(|w| w[0].fields["who"] != w[1].fields["who"]),
        "the tasks really interleaved"
    );
    assert!(
        log.iter().filter(|l| *l == "enter task").count() >= 6,
        "entered once per poll"
    );
    // The WRONG way, not run: `let _g = span.enter(); something.await;` leaves the span entered while another task
    // runs on this thread, so that task's events get the wrong context.
}

fn emit_levels() {
    error!("e");
    warn!("w");
    info!("i");
    debug!("d");
    trace!("t");
    info!(target: "app::db", "db-info");
    trace!(target: "app::db", "db-trace");
}
fn filtering() {
    let run = |directives: &str| {
        let c = Capture::default();
        let s = registry().with(
            c.clone()
                .with_filter(EnvFilter::try_new(directives).unwrap()),
        );
        tracing::subscriber::with_default(s, emit_levels);
        let v = c
            .events
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.fields["message"].clone())
            .collect::<Vec<_>>();
        v
    };
    assert_eq!(run("warn"), ["e", "w"]);
    assert_eq!(run("info"), ["e", "w", "i", "db-info"]);
    assert_eq!(
        run("warn,app::db=trace"),
        ["e", "w", "db-info", "db-trace"],
        "the most specific directive wins"
    );
    assert_eq!(
        run("off,app=info"),
        ["db-info"],
        "a target directive matches by prefix"
    );
    assert!(run("off").is_empty());
    assert!(EnvFilter::try_new("app=loud").is_err());
    // Different outputs, different filters, one subscriber: per-layer filtering.
    let (all, errors) = (Capture::default(), Capture::default());
    let s = registry()
        .with(
            all.clone()
                .with_filter(Targets::new().with_default(Level::TRACE)),
        )
        .with(
            errors
                .clone()
                .with_filter(filter_fn(|m| *m.level() == Level::ERROR)),
        );
    tracing::subscriber::with_default(s, emit_levels);
    assert_eq!(
        (
            all.events.lock().unwrap().len(),
            errors.events.lock().unwrap().len()
        ),
        (7, 1)
    );
    // enabled! lets you skip expensive work that only feeds a disabled level.
    let s = tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_writer(std::io::sink)
        .finish();
    tracing::subscriber::with_default(s, || {
        assert!(tracing::enabled!(Level::INFO) && !tracing::enabled!(Level::DEBUG));
        let mut evaluated = false;
        debug!(
            x = {
                evaluated = true;
                1
            }
        );
        assert!(
            !evaluated,
            "fields of a disabled event are not even evaluated"
        );
    });
}

fn fmt_and_json_output() {
    let text = Buffer::default();
    let s = tracing_subscriber::fmt()
        .with_writer(text.clone())
        .without_time()
        .with_ansi(false)
        .with_target(false)
        .finish();
    tracing::subscriber::with_default(s, || {
        info_span!("req", id = 9).in_scope(|| info!(status = 200, "done"))
    });
    assert_eq!(text.text(), " INFO req{id=9}: done status=200\n");

    let json = Buffer::default();
    let s = tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_writer(json.clone())
        .finish();
    tracing::subscriber::with_default(s, || {
        info_span!("req", id = 9).in_scope(|| warn!(status = 503, "upstream \"down\""))
    });
    let v: serde_json::Value = serde_json::from_str(json.text().lines().next().unwrap()).unwrap();
    assert_eq!(
        (
            v["level"].as_str(),
            v["status"].as_u64(),
            v["message"].as_str()
        ),
        (Some("WARN"), Some(503), Some("upstream \"down\""))
    );
    assert_eq!(
        (v["span"]["name"].as_str(), v["span"]["id"].as_u64()),
        (Some("req"), Some(9))
    );
    assert!(v["timestamp"].is_string());

    let timing = Buffer::default();
    let s = tracing_subscriber::fmt()
        .with_writer(timing.clone())
        .with_ansi(false)
        .with_span_events(FmtSpan::CLOSE)
        .finish();
    tracing::subscriber::with_default(s, || info_span!("work").in_scope(|| {}));
    let out = timing.text();
    assert!(
        out.contains("close") && out.contains("time.busy=") && out.contains("time.idle="),
        "{out}"
    );
}

/// A metrics-style layer: count events by level.
#[derive(Clone, Default)]
struct LevelCounter(Arc<Mutex<HashMap<Level, u64>>>);
impl<S: Subscriber> Layer<S> for LevelCounter {
    fn on_event(&self, e: &tracing::Event<'_>, _: Context<'_, S>) {
        *self
            .0
            .lock()
            .unwrap()
            .entry(*e.metadata().level())
            .or_default() += 1;
    }
}
fn counters() {
    let c = LevelCounter::default();
    tracing::subscriber::with_default(registry().with(c.clone()), || {
        (0..10).for_each(|_| emit_levels())
    });
    let m = c.0.lock().unwrap();
    assert_eq!(
        (m[&Level::ERROR], m[&Level::INFO], m[&Level::TRACE]),
        (10, 20, 20)
    );
}

/// Runs `f` under a Capture and returns "span/path: message" for each event.
fn attributed(f: impl FnOnce()) -> Vec<String> {
    captured(f)
        .0
        .iter()
        .map(|e| format!("{}: {}", e.spans.join("/"), e.fields["message"]))
        .collect()
}

/// The two ways async code loses or STEALS context, both shown running.
fn async_context_mistakes() {
    let rt = || {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
    };
    // 1. The broken `enter` guard held across .await. join! runs both futures in one task, and the oneshot channels fix
    //    the order: `a` suspends while its span is entered, then `b` logs.
    let got = attributed(|| {
        rt().block_on(async {
            let (a_suspended, wait_a) = tokio::sync::oneshot::channel();
            let (b_done, wait_b) = tokio::sync::oneshot::channel();
            let a = async {
                let span = info_span!("a");
                let _wrong = span.enter();
                a_suspended.send(()).unwrap();
                wait_b.await.unwrap();
                info!("from a");
            };
            let b = async {
                wait_a.await.unwrap();
                info!("from b");
                b_done.send(()).unwrap();
            };
            tokio::join!(a, b);
        });
    });
    assert_eq!(
        got,
        ["a{}: from b", "a{}: from a"],
        "b's event is filed under a's span"
    );

    // 2. tokio::spawn starts a new task; the poll of that task is outside the caller's span.
    let got = attributed(|| {
        rt().block_on(
            async {
                tokio::spawn(async { info!("plain spawn") }).await.unwrap();
                tokio::spawn(async { info!("in_current_span") }.in_current_span())
                    .await
                    .unwrap();
            }
            .instrument(info_span!("request")),
        );
    });
    assert_eq!(got, [": plain spawn", "request{}: in_current_span"]);
}

#[derive(Clone, Default)]
struct Links(Arc<Mutex<Vec<(String, String)>>>);
impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for Links {
    fn on_follows_from(
        &self,
        span: &tracing::span::Id,
        follows: &tracing::span::Id,
        ctx: Context<'_, S>,
    ) {
        let name = |id| ctx.span(id).unwrap().name().to_string();
        self.0.lock().unwrap().push((name(span), name(follows)));
    }
}

fn explicit_parents_and_links() {
    let got = attributed(|| {
        let a = info_span!("a");
        let _in_b = info_span!("b").entered();
        // Contextual parent (the default) versus explicit ones.
        info_span!("child").in_scope(|| info!("contextual"));
        info_span!(parent: &a, "child").in_scope(|| info!("explicit"));
        info_span!(parent: None, "root").in_scope(|| info!("detached"));
        info!(parent: &a, "an event can name its parent too");
    });
    assert_eq!(
        got,
        [
            "b{}/child{}: contextual",
            "a{}/child{}: explicit",
            "root{}: detached",
            "a{}: an event can name its parent too"
        ]
    );

    let links = Links::default();
    tracing::subscriber::with_default(registry().with(links.clone()), || {
        let request = info_span!("request");
        let job = info_span!(parent: None, "background_job");
        job.follows_from(request.id()); // "was caused by", without making the request wait for or contain the job
    });
    assert_eq!(
        *links.0.lock().unwrap(),
        [("background_job".to_string(), "request".to_string())]
    );
    assert!(tracing::Span::none().is_disabled());
}

fn filtering_pitfalls_and_reload() {
    use tracing_subscriber::reload;
    let messages = |c: &Capture| {
        c.events
            .lock()
            .unwrap()
            .iter()
            .map(|e| format!("{}: {}", e.spans.join("/"), e.fields["message"]))
            .collect::<Vec<_>>()
    };
    let work = || {
        info_span!("request", id = "r1").in_scope(|| {
            debug!("details");
            warn!("slow");
        });
        info_span!("request", id = "r2").in_scope(|| {
            debug!("details");
            warn!("slow");
        });
    };
    let run = |directives: &str| {
        let c = Capture::default();
        tracing::subscriber::with_default(
            registry().with(
                c.clone()
                    .with_filter(EnvFilter::try_new(directives).unwrap()),
            ),
            work,
        );
        messages(&c)
    };
    // PITFALL: spans have levels too. At "warn" the INFO span is disabled, so the warnings lose their request id.
    assert_eq!(run("warn"), [": slow", ": slow"]);
    assert_eq!(
        run("info"),
        ["request{id=r1}: slow", "request{id=r2}: slow"]
    );
    // Directives can select by span and field value: turn on debug for ONE request. Observed here: because a directive
    // names the span, `request` spans are also recorded for r1, so its warning keeps its context.
    assert_eq!(
        run("warn,[request{id=r2}]=debug"),
        [
            "request{id=r1}: slow",
            "request{id=r2}: details",
            "request{id=r2}: slow"
        ]
    );

    // PITFALL: a filter added as an ordinary layer is GLOBAL: it also silences the layers meant to see everything.
    let all = Capture::default();
    tracing::subscriber::with_default(
        registry().with(EnvFilter::new("error")).with(all.clone()),
        emit_levels,
    );
    assert_eq!(messages(&all), [": e"]);
    // Attached with with_filter it affects only its own layer (shown in `filtering` above).

    // Option<Layer> is a layer: switch outputs on from configuration without changing the subscriber's type.
    let (on, off) = (Capture::default(), Capture::default());
    let s = registry().with(Some(on.clone())).with(None::<Capture>);
    tracing::subscriber::with_default(s, || info!("x"));
    assert_eq!((messages(&on).len(), messages(&off).len()), (1, 0));

    // reload: change the level of a RUNNING process (admin endpoint, signal handler). Callsite caches are refreshed.
    let seen = Capture::default();
    let (filter, handle) = reload::Layer::new(EnvFilter::new("warn"));
    // set_default is the guard form of with_default: it lasts until the guard is dropped, on this thread only.
    let guard = tracing::subscriber::set_default(registry().with(filter).with(seen.clone()));
    let log_one = || debug!("tick");
    log_one();
    assert_eq!(messages(&seen).len(), 0);
    handle.reload(EnvFilter::new("debug")).unwrap();
    log_one(); // the SAME callsite is now enabled
    assert_eq!(messages(&seen), [": tick"]);
    handle.modify(|f| *f = EnvFilter::new("off")).unwrap();
    log_one();
    assert_eq!(messages(&seen).len(), 1);
    drop(guard);
    // The handle does not keep the subscriber alive: once it is gone, reload reports an error.
    assert!(handle.reload(EnvFilter::new("info")).is_err());
}

pub fn main() {
    events_and_fields();
    spans_and_instrument();
    async_context_propagation();
    filtering();
    fmt_and_json_output();
    counters();
    async_context_mistakes();
    explicit_parents_and_links();
    filtering_pitfalls_and_reload();
    println!("tracing: all sections passed");
}
