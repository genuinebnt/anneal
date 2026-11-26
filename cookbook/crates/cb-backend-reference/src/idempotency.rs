//! BLOCK 6: idempotency keys, so a client can safely retry a POST whose response it never received.
//!
//! Protocol: claim `(scope, key)` atomically; run the handler; store the response. A retry replays it.
//! * same key, different request: 409 (a client bug; replaying would silently drop the new data);
//! * same key while the first is still running: 409;
//! * 5xx, 408, 409, 429: the claim is released so that a retry really retries.
//!
//! The scope includes org and user, so tenants can neither collide with nor read each other's replays.
//! The handler runs in its own task: if the client disconnects or the request times out, the work still
//! finishes and the response is still recorded, instead of leaving the key stuck.
//! LIMIT (be honest about it): the domain commit and the stored response are separate transactions. A
//! process crash between them leaves the key "in progress" until the purge job deletes it (24 h).
//! Closing that gap means storing the response inside the handler's own transaction.
//! Do NOT use on routes whose response contains secrets (API key creation): it would be stored in clear.
//! Recipe: `examples/web/axum_idempotency.rs`.

use axum::body::{Body, to_bytes};
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};

use crate::auth::policy::Principal;
use crate::db::Db;
use crate::error::{AppError, AppResult, REQUEST_ID, current_request_id};
use crate::state::AppState;

pub const KEY_HEADER: &str = "idempotency-key";
pub const REPLAY_HEADER: &str = "idempotency-replayed";
const MAX_RESPONSE: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Stored {
    pub status: i64,
    pub content_type: Option<String>,
    pub etag: Option<String>,
    pub response: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
#[must_use]
pub enum Claim {
    /// We own the key: run the request.
    Fresh,
    Replay(Stored),
    Mismatch,
    InProgress,
}

/// Fields are length-prefixed so that different splits cannot produce the same hash input.
#[must_use]
pub fn fingerprint(method: &str, target: &str, body: &[u8]) -> String {
    let mut h = Sha256::new();
    for part in [method.as_bytes(), target.as_bytes(), body] {
        h.update((part.len() as u64).to_le_bytes());
        h.update(part);
    }
    hex::encode(h.finalize())
}

pub async fn claim(db: &Db, scope: &str, key: &str, print: &str, now: i64) -> Result<Claim, sqlx::Error> {
    let mut tx = db.begin().await?;
    // The INSERT is the lock: of any number of concurrent duplicates exactly one inserts the row.
    let inserted = sqlx::query(
        "INSERT INTO idempotency (scope, key, fingerprint, state, created_at) VALUES (?,?,?,'in_progress',?) \
         ON CONFLICT DO NOTHING",
    )
    .bind(scope)
    .bind(key)
    .bind(print)
    .bind(now)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if inserted {
        tx.commit().await?;
        return Ok(Claim::Fresh);
    }
    let (stored_print, state): (String, String) =
        sqlx::query_as("SELECT fingerprint, state FROM idempotency WHERE scope = ? AND key = ?")
            .bind(scope)
            .bind(key)
            .fetch_one(&mut *tx)
            .await?;
    if stored_print != print {
        return Ok(Claim::Mismatch);
    }
    if state != "done" {
        return Ok(Claim::InProgress);
    }
    let stored =
        sqlx::query_as("SELECT status, content_type, etag, response FROM idempotency WHERE scope = ? AND key = ?")
            .bind(scope)
            .bind(key)
            .fetch_one(&mut *tx)
            .await?;
    Ok(Claim::Replay(stored))
}

pub async fn complete(db: &Db, scope: &str, key: &str, s: &Stored) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE idempotency SET state = 'done', status = ?, content_type = ?, etag = ?, response = ? \
         WHERE scope = ? AND key = ?",
    )
    .bind(s.status)
    .bind(&s.content_type)
    .bind(&s.etag)
    .bind(&s.response)
    .bind(scope)
    .bind(key)
    .execute(db.pool())
    .await
    .map(|_| ())
}

pub async fn release(db: &Db, scope: &str, key: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM idempotency WHERE scope = ? AND key = ? AND state = 'in_progress'")
        .bind(scope)
        .bind(key)
        .execute(db.pool())
        .await
        .map(|_| ())
}

fn is_final(status: StatusCode) -> bool {
    !status.is_server_error()
        && !matches!(status, StatusCode::REQUEST_TIMEOUT | StatusCode::CONFLICT | StatusCode::TOO_MANY_REQUESTS)
}

fn replay(s: Stored) -> Response {
    let status = u16::try_from(s.status).ok().and_then(|c| StatusCode::from_u16(c).ok());
    let mut res = (status.unwrap_or(StatusCode::INTERNAL_SERVER_ERROR), Body::from(s.response)).into_response();
    let h = res.headers_mut();
    h.remove(header::CONTENT_TYPE);
    for (name, value) in [(header::CONTENT_TYPE, s.content_type), (header::ETAG, s.etag)] {
        if let Some(v) = value.and_then(|v| HeaderValue::from_str(&v).ok()) {
            h.insert(name, v);
        }
    }
    h.insert(REPLAY_HEADER, HeaderValue::from_static("true"));
    res
}

/// Record the outcome of a fresh request and hand the response back unchanged.
async fn settle(db: &Db, scope: &str, key: &str, res: Response) -> AppResult<Response> {
    if !is_final(res.status()) {
        release(db, scope, key).await?;
        return Ok(res);
    }
    let (parts, body) = res.into_parts();
    let bytes = to_bytes(body, MAX_RESPONSE).await.map_err(AppError::internal)?;
    let text = |name| parts.headers.get(name).and_then(|v: &HeaderValue| v.to_str().ok()).map(str::to_string);
    let stored = Stored {
        status: i64::from(parts.status.as_u16()),
        content_type: text(header::CONTENT_TYPE),
        etag: text(header::ETAG),
        response: bytes.to_vec(),
    };
    complete(db, scope, key, &stored).await?;
    Ok(Response::from_parts(parts, Body::from(bytes)))
}

async fn run(app: AppState, key: String, req: Request, next: Next) -> AppResult<Response> {
    let principal = req
        .extensions()
        .get::<Principal>()
        .ok_or_else(|| AppError::internal(anyhow::anyhow!("idempotency layer needs authentication outside it")))?;
    let scope = format!("{}:{}", principal.org_id, principal.user_id);
    if key.is_empty() || key.len() > 128 || !key.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(AppError::BadRequest("Idempotency-Key must be 1 to 128 visible ASCII characters".into()));
    }
    let (parts, body) = req.into_parts();
    let bytes = to_bytes(body, app.cfg.server.body_limit_bytes).await.map_err(|_| AppError::PayloadTooLarge)?;
    let target = parts.uri.path_and_query().map_or_else(|| parts.uri.path().to_string(), ToString::to_string);
    let print = fingerprint(parts.method.as_str(), &target, &bytes);
    match claim(&app.db, &scope, &key, &print, app.now_ms()).await? {
        Claim::Replay(stored) => Ok(replay(stored)),
        Claim::Mismatch => Err(AppError::Conflict("Idempotency-Key was used for a different request".into())),
        Claim::InProgress => Err(AppError::Conflict("a request with this Idempotency-Key is in progress".into())),
        Claim::Fresh => {
            let req = Request::from_parts(parts, Body::from(bytes));
            let request_id = current_request_id().unwrap_or_default();
            let (db, s, k) = (app.db.clone(), scope.clone(), key.clone());
            let task = tokio::spawn(REQUEST_ID.scope(request_id, async move {
                let res = next.run(req).await;
                settle(&db, &s, &k, res).await
            }));
            match task.await {
                Ok(result) => result,
                Err(panic) => {
                    release(&app.db, &scope, &key).await?;
                    Err(AppError::internal(panic))
                }
            }
        }
    }
}

pub async fn middleware(State(app): State<AppState>, req: Request, next: Next) -> Response {
    let Some(value) = req.headers().get(KEY_HEADER) else { return next.run(req).await };
    let key = String::from_utf8_lossy(value.as_bytes()).into_owned();
    run(app, key, req, next).await.unwrap_or_else(IntoResponse::into_response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn claim_lifecycle() {
        let db = Db::memory().await.unwrap();
        assert_eq!(claim(&db, "s", "k", "f1", 1).await.unwrap(), Claim::Fresh);
        assert_eq!(claim(&db, "s", "k", "f1", 2).await.unwrap(), Claim::InProgress);
        assert_eq!(claim(&db, "s", "k", "f2", 2).await.unwrap(), Claim::Mismatch);
        assert_eq!(claim(&db, "other", "k", "f2", 2).await.unwrap(), Claim::Fresh, "scopes are separate");
        let stored = Stored {
            status: 201,
            content_type: Some("application/json".into()),
            etag: None,
            response: b"{}".to_vec(),
        };
        complete(&db, "s", "k", &stored).await.unwrap();
        assert_eq!(claim(&db, "s", "k", "f1", 3).await.unwrap(), Claim::Replay(stored));
        release(&db, "s", "k").await.unwrap(); // must not delete a completed record
        assert!(matches!(claim(&db, "s", "k", "f1", 4).await.unwrap(), Claim::Replay(_)));
        release(&db, "other", "k").await.unwrap();
        assert_eq!(claim(&db, "other", "k", "f2", 5).await.unwrap(), Claim::Fresh);
    }

    #[test]
    fn fingerprint_depends_on_every_part_and_on_the_split() {
        let base = fingerprint("POST", "/a", b"x");
        assert_ne!(base, fingerprint("PUT", "/a", b"x"));
        assert_ne!(base, fingerprint("POST", "/b", b"x"));
        assert_ne!(base, fingerprint("POST", "/a", b"y"));
        assert_ne!(fingerprint("POST", "/ab", b"c"), fingerprint("POST", "/a", b"bc"));
    }

    #[test]
    fn only_settled_outcomes_are_stored() {
        for s in [200, 201, 400, 403, 404, 422] {
            assert!(is_final(StatusCode::from_u16(s).unwrap()), "{s}");
        }
        for s in [408, 409, 429, 500, 503] {
            assert!(!is_final(StatusCode::from_u16(s).unwrap()), "{s}");
        }
    }
}
