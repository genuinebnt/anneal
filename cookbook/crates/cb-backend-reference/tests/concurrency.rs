//! Races on a multi-threaded runtime over a file-backed database (real locking, several connections).
//! Every assertion is an invariant that must hold under ANY interleaving, so these never depend on timing.
//! (The converse is the usual caveat: a passing run does not prove a race cannot happen.)

use std::sync::Arc;

use backend_ref::testkit::{PASSWORD, Reply, Req, TestApp};
use serde_json::json;
use tokio::sync::Barrier;
use tokio::task::JoinSet;

const PARALLEL: usize = 24;
const ROUNDS: usize = 5;

/// Fire `n` requests as simultaneously as the runtime allows.
async fn race(t: &Arc<TestApp>, n: usize, make: impl Fn(usize) -> Req) -> Vec<Reply> {
    let barrier = Arc::new(Barrier::new(n));
    let mut set = JoinSet::new();
    for i in 0..n {
        let (t, barrier, req) = (t.clone(), barrier.clone(), make(i));
        set.spawn(async move {
            barrier.wait().await;
            t.send(req).await
        });
    }
    set.join_all().await
}

fn statuses(replies: &[Reply], status: u16) -> usize {
    replies.iter().filter(|r| r.status == status).count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_updates_never_lose_a_write() {
    let t = Arc::new(TestApp::new().await);
    let s = t.signup("Acme", "a@acme.io").await;
    let project = t.project(&s, "p").await;
    let task = t.task(&s, &project, "t").await;
    for round in 0..ROUNDS {
        let version = round as i64 + 1;
        let replies = race(&t, PARALLEL, |i| {
            Req::patch(format!("/v1/tasks/{task}"))
                .auth(&s.access)
                .if_match(version)
                .json(&json!({ "title": format!("writer {i}") }))
        })
        .await;
        assert_eq!(statuses(&replies, 200), 1, "{replies:?}");
        assert_eq!(statuses(&replies, 412), PARALLEL - 1);
        let winner = replies.iter().find(|r| r.status == 200).unwrap();
        let stored = t.send(Req::get(format!("/v1/tasks/{task}")).auth(&s.access)).await.expect(200).json();
        assert_eq!(stored, winner.json(), "what the winner was told is what is stored");
    }
}

/// Read-modify-write with retry on 412: every increment must survive.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn retrying_clients_converge_without_losing_increments() {
    let t = Arc::new(TestApp::new().await);
    let s = t.signup("Acme", "a@acme.io").await;
    let project = t.project(&s, "0").await;
    let url = format!("/v1/projects/{project}");
    let mut set = JoinSet::new();
    for _ in 0..8 {
        let (t, token, url) = (t.clone(), s.access.clone(), url.clone());
        set.spawn(async move {
            loop {
                let current = t.send(Req::get(&url).auth(&token)).await.expect(200);
                let n: u32 = current.str("name").parse().unwrap();
                let patch = Req::patch(&url)
                    .auth(&token)
                    .header("if-match", current.header("etag").unwrap())
                    .json(&json!({ "name": (n + 1).to_string() }));
                match t.send(patch).await {
                    r if r.status == 200 => break,
                    r => assert_eq!(r.status, 412, "{r:?}"),
                }
            }
        });
    }
    set.join_all().await;
    assert_eq!(t.send(Req::get(&url).auth(&s.access)).await.expect(200).str("name"), "8");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parallel_duplicate_posts_create_one_resource() {
    let t = Arc::new(TestApp::new().await);
    let s = t.signup("Acme", "a@acme.io").await;
    for round in 0..ROUNDS {
        let key = format!("key-{round}");
        let make = |_| {
            Req::post("/v1/projects").auth(&s.access).header("idempotency-key", &key).json(&json!({ "name": "once" }))
        };
        let replies = race(&t, PARALLEL, make).await;
        assert_eq!(t.count("projects").await, round as i64 + 1, "duplicate executed: {replies:?}");
        // Each caller either got the one result or was told to retry.
        assert_eq!(statuses(&replies, 201) + statuses(&replies, 409), PARALLEL, "{replies:?}");
        assert!(statuses(&replies, 201) >= 1);
        let original = replies.iter().find(|r| r.status == 201).unwrap();
        assert!(replies.iter().filter(|r| r.status == 201).all(|r| r.body == original.body));
        let retry = t.send(make(0)).await.expect(201);
        assert_eq!(retry.body, original.body);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_refresh_token_cannot_be_rotated_twice() {
    let t = Arc::new(TestApp::new().await);
    let s = t.signup("Acme", "a@acme.io").await;
    for _ in 0..ROUNDS {
        let refresh = t.login("a@acme.io").await.expect(200).str("refresh_token");
        let replies =
            race(&t, 12, |_| Req::post("/v1/auth/refresh").json(&json!({ "refresh_token": refresh }))).await;
        assert_eq!(statuses(&replies, 200), 1, "{replies:?}");
        assert_eq!(statuses(&replies, 401), 11);
        // The others were reuse, so even the winner's new tokens are revoked.
        let winner = replies.iter().find(|r| r.status == 200).unwrap();
        t.send(Req::get("/v1/me").auth(&winner.str("access_token"))).await.expect(401);
    }
    t.send(Req::get("/v1/me").auth(&s.access)).await.expect(200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parallel_registrations_of_one_email_create_one_account() {
    let t = Arc::new(TestApp::new().await);
    let body = json!({ "org_name": "Acme", "email": "a@acme.io", "password": PASSWORD });
    let replies = race(&t, 12, |_| Req::post("/v1/auth/register").json(&body)).await;
    assert_eq!(statuses(&replies, 201), 1, "{replies:?}");
    assert_eq!(statuses(&replies, 409), 11);
    assert_eq!((t.count("orgs").await, t.count("users").await, t.count("jobs").await), (1, 1, 1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn competing_workers_run_each_job_once() {
    let t = Arc::new(TestApp::new().await);
    for i in 0..30 {
        let body = json!({ "org_name": "o", "email": format!("u{i}@x.io"), "password": PASSWORD });
        t.send(Req::post("/v1/auth/register").json(&body)).await.expect(201);
    }
    let mut set = JoinSet::new();
    for _ in 0..6 {
        let ctx = t.app.job_context();
        set.spawn(async move { backend_ref::jobs::worker::drain_due(&ctx).await.unwrap() });
    }
    assert_eq!(set.join_all().await.into_iter().sum::<usize>(), 30);
    let mut recipients: Vec<String> = t.mailer.sent().into_iter().map(|m| m.to).collect();
    recipients.sort();
    recipients.dedup();
    assert_eq!((recipients.len(), t.mailer.sent().len()), (30, 30));
}
