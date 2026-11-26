//! BLOCK 12 (part): liveness, readiness, metrics, and the redacted config dump.
//! These routes are unauthenticated: expose them on an internal port or block them at the edge.

use axum::Json;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use serde_json::json;

use crate::jobs::queue;
use crate::state::AppState;

/// Liveness: "the process works". Never check dependencies here, or a database outage makes the
/// orchestrator restart every healthy replica.
pub async fn healthz() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

/// Readiness: "send me traffic". False while draining or when the database does not answer.
pub async fn readyz(State(app): State<AppState>) -> impl IntoResponse {
    let status = if app.shutdown.is_cancelled() {
        "draining"
    } else if app.db.ping().await.is_err() {
        "database unavailable"
    } else {
        "ready"
    };
    let code = if status == "ready" { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (code, Json(json!({ "status": status })))
}

pub async fn metrics(State(app): State<AppState>) -> impl IntoResponse {
    let mut text = app.metrics.render();
    if let Ok(mut c) = app.db.pool().acquire().await
        && let Ok(counts) = queue::counts_by_state(&mut c).await
    {
        // A growing `dead` count is the signal to look at the dead-letter queue.
        text.push_str("# TYPE jobs gauge\n");
        for (state, n) in counts {
            text.push_str(&format!("jobs{{state=\"{state}\"}} {n}\n"));
        }
    }
    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], text)
}

pub async fn debug_config(State(app): State<AppState>) -> impl IntoResponse {
    Json(app.cfg.redacted())
}
