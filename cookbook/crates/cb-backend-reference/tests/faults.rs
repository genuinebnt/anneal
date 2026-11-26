//! Fault injection: failing statements, a locked database, a failing mailer and webhook receiver,
//! stuck dependencies, panics and cancelled requests.
//!
//! SQLite triggers are the injection tool: `RAISE(ABORT)` makes one chosen statement fail inside the
//! application's own transaction, with no test hooks in production code.

mod common;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, Request, StatusCode};
use axum::routing::post;
use backend_ref::jobs::webhook::{DELIVERY_HEADER, SIGNATURE_HEADER, verify};
use backend_ref::testkit::{PASSWORD, Req, TestApp};
use common::GatedStore;
use serde_json::{Value, json};
use sqlx::Connection;
use tower::ServiceExt;

/// Each target gets its own trigger name: a pooled connection with a cached schema may refuse to
/// re-create a name that another connection has just dropped.
async fn inject(t: &TestApp, table: &str, event: &str) {
    let sql = format!(
        "CREATE TRIGGER boom_{table} BEFORE {event} ON {table} \
         BEGIN SELECT RAISE(ABORT, 'injected fault: {table}'); END"
    );
    sqlx::query(&sql).execute(t.app.db.pool()).await.unwrap();
}

async fn heal(t: &TestApp, table: &str) {
    sqlx::query(&format!("DROP TRIGGER boom_{table}")).execute(t.app.db.pool()).await.unwrap();
}

async fn fingerprint(t: &TestApp) -> String {
    sqlx::query_scalar(
        "SELECT (SELECT count(*) || ':' || coalesce(sum(version),0) || ':' || count(deleted_at) FROM projects) \
         || '|' || (SELECT count(*) || ':' || coalesce(sum(version),0) || ':' || count(deleted_at) FROM tasks) \
         || '|' || (SELECT count(*) FROM tasks_fts) || '|' || (SELECT count(*) FROM comments) \
         || '|' || (SELECT count(*) FROM attachments) || '|' || (SELECT count(*) FROM webhooks) \
         || '|' || (SELECT count(*) FROM audit_log) || '|' || (SELECT count(*) FROM outbox)",
    )
    .fetch_one(t.app.db.pool())
    .await
    .unwrap()
}

/// THE transaction guarantee: domain write, audit row and outbox event commit together or not at all.
/// Whichever of the three fails, nothing may remain.
#[tokio::test]
async fn writes_audit_and_outbox_are_atomic() {
    let t = TestApp::new().await;
    let s = t.signup("Acme", "a@acme.io").await;
    let project = t.project(&s, "p").await;
    let task = t.task(&s, &project, "t").await;

    let writes = || {
        vec![
            ("create project", Req::post("/v1/projects").json(&json!({ "name": "n" }))),
            (
                "update project",
                Req::patch(format!("/v1/projects/{project}")).if_match(1).json(&json!({ "name": "n" })),
            ),
            ("delete project", Req::delete(format!("/v1/projects/{project}")).if_match(1)),
            ("create task", Req::post(format!("/v1/projects/{project}/tasks")).json(&json!({ "title": "n" }))),
            ("update task", Req::patch(format!("/v1/tasks/{task}")).if_match(1).json(&json!({ "title": "n" }))),
            ("delete task", Req::delete(format!("/v1/tasks/{task}"))),
            ("comment", Req::post(format!("/v1/tasks/{task}/comments")).json(&json!({ "body": "n" }))),
            ("upload", Req::post(format!("/v1/tasks/{task}/attachments")).file("a.txt", b"hello")),
            ("webhook", Req::post("/v1/webhooks").json(&json!({ "url": "http://127.0.0.1:9/h" }))),
        ]
    };
    let before = fingerprint(&t).await;
    for table in ["audit_log", "outbox"] {
        inject(&t, table, "INSERT").await;
        for (name, req) in writes() {
            let r = t.send(req.auth(&s.access)).await;
            assert_eq!(r.status, 500, "{name} with failing {table}: {r:?}");
            assert!(!String::from_utf8_lossy(&r.body).contains("injected"), "internal cause leaked");
            assert_eq!(fingerprint(&t).await, before, "{name} left a partial write when {table} failed");
        }
        heal(&t, table).await;
    }
    assert!(t.store.is_empty(), "the object of a failed upload must be deleted");
}

#[tokio::test]
async fn cascade_delete_is_atomic() {
    let t = TestApp::new().await;
    let s = t.signup("Acme", "a@acme.io").await;
    let project = t.project(&s, "p").await;
    t.task(&s, &project, "t").await;
    let before = fingerprint(&t).await;
    // The project row is updated first, then the tasks: fail the second step.
    inject(&t, "tasks", "UPDATE").await;
    t.send(Req::delete(format!("/v1/projects/{project}")).auth(&s.access).if_match(1)).await.expect(500);
    assert_eq!(fingerprint(&t).await, before);
}

#[tokio::test]
async fn registration_is_atomic_with_its_mail_job() {
    let t = TestApp::new().await;
    inject(&t, "jobs", "INSERT").await;
    let body = json!({ "org_name": "Acme", "email": "a@acme.io", "password": PASSWORD });
    t.send(Req::post("/v1/auth/register").json(&body)).await.expect(500);
    for table in ["orgs", "users", "email_tokens", "audit_log"] {
        assert_eq!(t.count(table).await, 0, "{table}");
    }
    heal(&t, "jobs").await;
    t.send(Req::post("/v1/auth/register").json(&body)).await.expect(201);
}

#[tokio::test]
async fn refresh_rotation_is_atomic() {
    let t = TestApp::new().await;
    let s = t.signup("Acme", "a@acme.io").await;
    let refresh = || Req::post("/v1/auth/refresh").json(&json!({ "refresh_token": s.refresh }));
    // The old token is marked as used, then inserting the new one fails.
    inject(&t, "refresh_tokens", "INSERT").await;
    t.send(refresh()).await.expect(500);
    heal(&t, "refresh_tokens").await;
    // Had the mark committed alone, this retry would look like token theft and log the user out.
    t.send(refresh()).await.expect(200);
}

#[tokio::test]
async fn a_locked_database_is_503_with_retry_after() {
    let t = TestApp::with(|c| c.db.busy_timeout_ms = 30).await;
    let s = t.signup("Acme", "a@acme.io").await;
    let path = t.app.db.temp_path().unwrap();
    let mut other = sqlx::SqliteConnection::connect(&format!("sqlite://{}", path.display())).await.unwrap();
    sqlx::query("BEGIN IMMEDIATE").execute(&mut other).await.unwrap();

    let create = || Req::post("/v1/projects").auth(&s.access).json(&json!({ "name": "p" }));
    let r = t.send(create()).await.expect(503);
    assert_eq!(r.header("retry-after"), Some("1"));
    // WAL: readers are not blocked by the writer.
    t.send(Req::get("/v1/projects").auth(&s.access)).await.expect(200);

    sqlx::query("ROLLBACK").execute(&mut other).await.unwrap();
    t.send(create()).await.expect(201);
}

#[tokio::test]
async fn registration_survives_a_mail_outage() {
    let t = TestApp::new().await;
    let body = json!({ "org_name": "Acme", "email": "a@acme.io", "password": PASSWORD });
    t.mailer.fail_next(2);
    t.send(Req::post("/v1/auth/register").json(&body)).await.expect(201);
    t.run_background().await;
    assert!(t.mailer.sent().is_empty());
    for delay in [2, 4] {
        t.clock.advance_secs(delay);
        t.run_background().await;
    }
    assert_eq!(t.mailer.sent().len(), 1);
}

#[derive(Default)]
struct Receiver {
    failures_left: AtomicU32,
    hits: Mutex<Vec<(HeaderMap, Bytes)>>,
}

/// A real HTTP receiver on loopback that fails the first `failures` deliveries.
async fn receiver(failures: u32) -> (Arc<Receiver>, String) {
    let state = Arc::new(Receiver { failures_left: AtomicU32::new(failures), ..Default::default() });
    let shared = state.clone();
    let app = axum::Router::new().route(
        "/hook",
        post(move |headers: HeaderMap, body: Bytes| async move {
            shared.hits.lock().unwrap().push((headers, body));
            let failing = shared.failures_left.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1));
            if failing.is_ok() { StatusCode::INTERNAL_SERVER_ERROR } else { StatusCode::OK }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/hook", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    (state, url)
}

async fn webhook_job(t: &TestApp) -> (String, i64) {
    sqlx::query_as("SELECT state, attempts FROM jobs WHERE kind = 'webhook'")
        .fetch_one(t.app.db.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn webhooks_are_signed_and_retried_with_exponential_backoff() {
    let t = TestApp::new().await;
    let s = t.signup("Acme", "a@acme.io").await;
    let (seen, url) = receiver(2).await;
    let created = t.send(Req::post("/v1/webhooks").auth(&s.access).json(&json!({ "url": url }))).await.expect(201);
    let secret = created.str("secret");
    t.run_background().await; // relays webhook.created, to the new hook itself
    sqlx::query("DELETE FROM jobs").execute(t.app.db.pool()).await.unwrap();
    seen.hits.lock().unwrap().clear();
    seen.failures_left.store(2, Ordering::SeqCst);

    let project = t.project(&s, "p").await;
    assert_eq!(t.run_background().await, 1);
    assert_eq!(webhook_job(&t).await, ("queued".into(), 1));
    t.clock.advance_ms(1_999);
    assert_eq!(t.run_background().await, 0, "first retry waits 2 s");
    t.clock.advance_ms(1);
    assert_eq!(t.run_background().await, 1);
    t.clock.advance_ms(3_999);
    assert_eq!(t.run_background().await, 0, "second retry waits 4 s");
    t.clock.advance_ms(1);
    assert_eq!(t.run_background().await, 1);
    assert_eq!(webhook_job(&t).await, ("done".into(), 3));

    let hits = seen.hits.lock().unwrap();
    assert_eq!(hits.len(), 3);
    let deliveries: std::collections::HashSet<_> = hits.iter().map(|(h, _)| h[DELIVERY_HEADER].clone()).collect();
    assert_eq!(deliveries.len(), 1, "retries keep one delivery id so the receiver can dedupe");
    for (headers, body) in hits.iter() {
        let signature = headers[SIGNATURE_HEADER].to_str().unwrap();
        // Signature timestamps differ per attempt; check each against its own `t=`.
        let sent_at: i64 = signature[2..signature.find(',').unwrap()].parse().unwrap();
        assert!(verify(&secret, signature, body, sent_at, 0));
        assert!(!verify("wrong-secret", signature, body, sent_at, 0));
        let event: Value = serde_json::from_slice(body).unwrap();
        assert_eq!(event["topic"], "project.created");
        assert_eq!(event["data"]["id"], json!(project));
    }
}

#[tokio::test]
async fn a_dead_receiver_ends_in_the_dead_letter_queue() {
    let t = TestApp::with(|c| c.jobs.max_attempts = 3).await;
    let s = t.signup("Acme", "a@acme.io").await;
    let (seen, url) = receiver(u32::MAX).await;
    t.send(Req::post("/v1/webhooks").auth(&s.access).json(&json!({ "url": url }))).await.expect(201);
    for _ in 0..10 {
        t.run_background().await;
        t.clock.advance_secs(3_600);
    }
    assert_eq!(webhook_job(&t).await, ("dead".into(), 3));
    assert_eq!(seen.hits.lock().unwrap().len(), 3);
    let metrics = t.send(Req::get("/metrics")).await.expect(200);
    assert!(String::from_utf8_lossy(&metrics.body).contains("jobs{state=\"dead\"} 1"));
}

#[tokio::test]
async fn a_redirecting_receiver_is_not_followed() {
    let t = TestApp::with(|c| c.jobs.max_attempts = 1).await;
    let s = t.signup("Acme", "a@acme.io").await;
    let (target, target_url) = receiver(0).await;
    let redirect = axum::Router::new()
        .route("/hook", post(move || async move { axum::response::Redirect::temporary(&target_url) }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/hook", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, redirect).await });
    t.send(Req::post("/v1/webhooks").auth(&s.access).json(&json!({ "url": url }))).await.expect(201);
    t.run_background().await;
    assert_eq!(webhook_job(&t).await, ("dead".into(), 1));
    assert!(target.hits.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_stuck_dependency_times_out_with_503() {
    let store = GatedStore::new(); // `get` never returns: nobody releases it
    let t = TestApp::with_store(|c| c.server.request_timeout_ms = 500, store.clone()).await;
    let url = format!("/v1/files/o/k?exp={}&sig={}", i64::MAX, t.app.presigner.sign("o/k", i64::MAX));
    let s = t.signup("Acme", "a@acme.io").await;
    let project = t.project(&s, "p").await;
    let task = t.task(&s, &project, "t").await;
    sqlx::query(
        "INSERT INTO attachments (id, org_id, task_id, object_key, filename, content_type, size, sha256, created_at) \
         VALUES ('att', ?, ?, 'o/k', 'f', 'text/plain', 1, 'x', 0)",
    )
    .bind(&s.org_id)
    .bind(&task)
    .execute(t.app.db.pool())
    .await
    .unwrap();
    let r = t.send(Req::get(url)).await.expect(503);
    assert_eq!(r.header("retry-after"), Some("1"));
    assert_eq!(r.header("content-type"), Some("application/problem+json"));
}

/// The client goes away mid-request. The idempotent work must still finish and be recorded, otherwise
/// the retry finds the key stuck "in progress".
#[tokio::test]
async fn cancelled_idempotent_request_still_completes() {
    use backend_ref::auth::policy::{Principal, Role, Via};
    use tokio::sync::Semaphore;

    let t = TestApp::new().await;
    let gate = Arc::new((Semaphore::new(0), Semaphore::new(0), AtomicU32::new(0)));
    let inner = gate.clone();
    let principal =
        Principal { user_id: "u".into(), org_id: "o".into(), role: Role::Owner, via: Via::Session("s".into()) };
    let router = axum::Router::new()
        .route(
            "/work",
            post(move || async move {
                inner.2.fetch_add(1, Ordering::SeqCst);
                inner.0.add_permits(1);
                inner.1.acquire().await.unwrap().forget();
                "done"
            }),
        )
        .route_layer(axum::middleware::from_fn_with_state(t.app.clone(), backend_ref::idempotency::middleware))
        .layer(axum::Extension(principal));
    let request = || Request::post("/work").header("idempotency-key", "k").body(Body::empty()).unwrap();

    let client = tokio::spawn(router.clone().oneshot(request()));
    gate.0.acquire().await.unwrap().forget(); // the handler is running
    client.abort();
    assert!(client.await.unwrap_err().is_cancelled());

    // Guarded: if the duplicate wrongly ran the handler it would block on the gate for ever.
    let busy = tokio::time::timeout(Duration::from_secs(10), router.clone().oneshot(request()))
        .await
        .expect("the duplicate ran the handler instead of being refused")
        .unwrap();
    assert_eq!(busy.status(), 409, "still running");
    gate.1.add_permits(1);
    // Wait for the detached task to record the response. Bounded, and independent of timing.
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let state: String =
                sqlx::query_scalar("SELECT state FROM idempotency").fetch_one(t.app.db.pool()).await.unwrap();
            if state == "done" {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the cancelled request never completed");
    let replayed = router.oneshot(request()).await.unwrap();
    assert_eq!(replayed.status(), 200);
    assert_eq!(replayed.headers()[backend_ref::idempotency::REPLAY_HEADER], "true");
    assert_eq!(gate.2.load(Ordering::SeqCst), 1, "the handler ran exactly once");
}
