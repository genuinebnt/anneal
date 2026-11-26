//! RECIPE 29 (axum): Server-Sent Events with `Last-Event-ID` replay.
//! LEVEL: intermediate
//! Use in: SSE / execution event stream stage.   Run: cargo run --release --example axum_sse_replay
//! COMMON USES: live progress feeds: build logs, job status, notifications; reconnect-safe via Last-Event-ID.
//!
//! Pattern: persist events with increasing ids. When a client reconnects it sends `Last-Event-ID: N`; the
//! server replays only events with id > N, then continues live. `KeepAlive` sends comment lines so proxies do
//! not close idle connections. Here the "live" part is omitted; the replay logic is the lesson.
//! Spec: https://html.spec.whatwg.org/multipage/server-sent-events.html
#![allow(dead_code)]
use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request},
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
};
use futures_util::stream::{self, Stream};
use http_body_util::BodyExt;
use std::convert::Infallible;
use tower::ServiceExt;

// pretend these were persisted for execution ex-123
const LOG: [(u64, &str, &str); 4] = [
    (1, "workflow.started", r#"{"execution_id":"ex-123"}"#),
    (2, "task.started", r#"{"task_id":"upload"}"#),
    (
        3,
        "task.completed",
        r#"{"task_id":"upload","duration_ms":1200}"#,
    ),
    (4, "workflow.completed", r#"{"execution_id":"ex-123"}"#),
];

async fn events(headers: HeaderMap) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let last: u64 = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let replay = LOG
        .iter()
        .filter(move |(id, _, _)| *id > last)
        .map(|(id, name, data)| Ok(Event::default().id(id.to_string()).event(*name).data(*data)))
        .collect::<Vec<_>>();
    Sse::new(stream::iter(replay)).keep_alive(KeepAlive::default())
}

async fn fetch(app: &Router, last_event_id: Option<&str>) -> String {
    let mut req = Request::builder().uri("/v1/executions/ex-123/events");
    if let Some(id) = last_event_id {
        req = req.header("last-event-id", id);
    }
    let resp = app
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    String::from_utf8(
        resp.into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap()
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/v1/executions/ex-123/events", get(events));
    let full = fetch(&app, None).await;
    println!("--- first connection ---\n{full}");
    assert!(full.contains("id: 1") && full.contains("id: 4"));

    let resumed = fetch(&app, Some("2")).await;
    println!("--- reconnect with Last-Event-ID: 2 ---\n{resumed}");
    assert!(!resumed.contains("id: 1") && !resumed.contains("id: 2"));
    assert!(resumed.contains("id: 3") && resumed.contains("id: 4"));
}
