//! BLOCK 11: Server-Sent Events, one stream per tenant, resumable with `Last-Event-ID`.
//!
//! The database is the source of truth; the broadcast channel only says "look again". So a slow client
//! cannot lose events (it reads from its own position), a lagged or coalesced wake-up is harmless, and a
//! reconnect with `Last-Event-ID` replays exactly what was missed.
//! Long-lived streams need two things short requests do not: re-checking the credential (a revoked key
//! must stop receiving data) and ending on shutdown (or graceful shutdown waits for them forever).
//! WebSocket is not implemented: this feed is one-way, which is what SSE is for.
//! Recipe: `examples/web/axum_sse_replay.rs`.

use std::collections::VecDeque;
use std::convert::Infallible;

use axum::extract::{Extension, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::Stream;
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::error::RecvError;

use crate::auth::policy::{Action, Principal};
use crate::auth::service;
use crate::error::{AppError, AppResult};
use crate::http::middleware::Credential;
use crate::jobs::relay::{EventRow, events_after};
use crate::state::AppState;

const BATCH: i64 = 100;

struct Feed {
    app: AppState,
    principal: Principal,
    credential: Credential,
    wake: Receiver<()>,
    last_seq: i64,
    buffered: VecDeque<EventRow>,
}

impl Feed {
    async fn still_allowed(&self) -> bool {
        let now = service::authenticate(&self.app, &self.credential.0).await;
        now.is_ok_and(|p| p.org_id == self.principal.org_id)
    }

    /// The next event, waiting if needed. `None` ends the stream; the client reconnects and resumes.
    async fn next(&mut self) -> Option<EventRow> {
        loop {
            if self.app.shutdown.is_cancelled() {
                return None;
            }
            if let Some(event) = self.buffered.pop_front() {
                self.last_seq = event.seq;
                return Some(event);
            }
            if !self.still_allowed().await {
                return None;
            }
            // No lost wake-up: we subscribed before this query, so an event published after it still
            // leaves a message in `wake`.
            let found = events_after(&self.app.db, &self.principal.org_id, self.last_seq, BATCH).await.ok()?;
            if !found.is_empty() {
                self.buffered = found.into();
                continue;
            }
            tokio::select! {
                () = self.app.shutdown.cancelled() => return None,
                woken = self.wake.recv() => {
                    if matches!(woken, Err(RecvError::Closed)) {
                        return None;
                    }
                }
            }
        }
    }
}

fn last_event_id(headers: &HeaderMap) -> AppResult<Option<i64>> {
    let Some(value) = headers.get("last-event-id") else { return Ok(None) };
    value
        .to_str()
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|n| *n >= 0)
        .map(Some)
        .ok_or_else(|| AppError::BadRequest("Last-Event-ID must be a non-negative integer".into()))
}

pub async fn stream(
    State(app): State<AppState>,
    Extension(principal): Extension<Principal>,
    Extension(credential): Extension<Credential>,
    headers: HeaderMap,
) -> AppResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    principal.require(Action::Read)?;
    let wake = app.events.subscribe();
    let last_seq = match last_event_id(&headers)? {
        Some(seq) => seq,
        // A first connection starts "now"; it does not replay history.
        None => {
            sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) FROM events WHERE org_id = ?")
                .bind(&principal.org_id)
                .fetch_one(app.db.pool())
                .await?
        }
    };
    let feed = Feed { app, principal, credential, wake, last_seq, buffered: VecDeque::new() };
    let events = futures_util::stream::unfold(feed, |mut feed| async move {
        let row = feed.next().await?;
        let event = Event::default().id(row.seq.to_string()).event(row.topic).data(row.payload);
        Some((Ok::<_, Infallible>(event), feed))
    });
    Ok(Sse::new(events).keep_alive(KeepAlive::default()))
}
