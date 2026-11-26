//! Full stack over a real socket: `server::serve` with its background tasks, driven by `reqwest`.
//! The timeouts here are only hang guards (30 s); no assertion depends on how fast anything is.

mod common;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use backend_ref::server::serve;
use backend_ref::testkit::{PASSWORD, Req, TestApp};
use common::GatedStore;
use serde_json::{Value, json};
use tokio::task::JoinHandle;

const GUARD: Duration = Duration::from_secs(30);

async fn start(t: &TestApp) -> (SocketAddr, JoinHandle<anyhow::Result<()>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    (addr, tokio::spawn(serve(t.app.clone(), listener)))
}

fn client() -> reqwest::Client {
    reqwest::Client::builder().timeout(GUARD).no_proxy().build().unwrap()
}

async fn eventually<T>(what: &str, mut probe: impl FnMut() -> Option<T>) -> T {
    tokio::time::timeout(GUARD, async {
        loop {
            if let Some(v) = probe() {
                return v;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("never happened: {what}"))
}

/// Read one SSE frame (skipping keep-alive comments). `None` = the stream ended.
async fn next_event(res: &mut reqwest::Response, buffer: &mut String) -> Option<(i64, String, Value)> {
    tokio::time::timeout(GUARD, async {
        loop {
            if let Some(end) = buffer.find("\n\n") {
                let frame: String = buffer.drain(..end + 2).collect();
                let field = |name: &str| frame.lines().find_map(|l| l.strip_prefix(name).map(str::trim));
                let Some(id) = field("id:") else { continue };
                let data = serde_json::from_str(field("data:").unwrap()).unwrap();
                return Some((id.parse().unwrap(), field("event:").unwrap().to_string(), data));
            }
            let chunk = res.chunk().await.expect("stream error")?;
            buffer.push_str(std::str::from_utf8(&chunk).unwrap());
        }
    })
    .await
    .expect("no event arrived")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_signup_flow_through_real_background_workers() {
    let t = TestApp::new().await;
    let (addr, server) = start(&t).await;
    let url = |path: &str| format!("http://{addr}{path}");
    let http = client();
    let body = json!({ "org_name": "Acme", "email": "a@acme.io", "password": PASSWORD });
    let r = http.post(url("/v1/auth/register")).json(&body).send().await.unwrap();
    assert_eq!(r.status(), 201);
    // Nothing is driven by hand: the server's own workers send the mail.
    let token = eventually("verification mail", || t.mailer.last_token_for("a@acme.io")).await;
    let r = http.post(url("/v1/auth/verify")).json(&json!({ "token": token })).send().await.unwrap();
    assert_eq!(r.status(), 204);
    let login = json!({ "email": "a@acme.io", "password": PASSWORD });
    let tokens: Value = http.post(url("/v1/auth/login")).json(&login).send().await.unwrap().json().await.unwrap();
    let access = tokens["access_token"].as_str().unwrap();
    let me = http.get(url("/v1/me")).bearer_auth(access).send().await.unwrap();
    assert_eq!(me.status(), 200);
    t.app.shutdown.cancel();
    tokio::time::timeout(GUARD, server).await.unwrap().unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn event_stream_is_per_tenant_and_resumes_from_last_event_id() {
    let t = TestApp::new().await;
    let a = t.signup("Acme", "a@acme.io").await;
    let b = t.signup("Other", "b@other.io").await;
    t.run_background().await;
    let (addr, server) = start(&t).await;
    let http = client();
    let open = |token: &str, last: Option<i64>| {
        let mut req = http.get(format!("http://{addr}/v1/events")).bearer_auth(token);
        if let Some(id) = last {
            req = req.header("last-event-id", id.to_string());
        }
        req.send()
    };

    let mut live = open(&a.access, None).await.unwrap();
    assert_eq!(live.status(), 200);
    assert_eq!(live.headers()["content-type"], "text/event-stream");
    let mut buffer = String::new();

    t.project(&b, "other tenant's").await;
    let first = t.project(&a, "first").await;
    let (id1, topic, data) = next_event(&mut live, &mut buffer).await.unwrap();
    assert_eq!((topic.as_str(), data["id"].as_str().unwrap()), ("project.created", first.as_str()));
    drop(live); // disconnect

    // Missed while away, and one event of another tenant interleaved.
    let second = t.project(&a, "second").await;
    t.project(&b, "again").await;
    let third = t.project(&a, "third").await;

    let mut resumed = open(&a.access, Some(id1)).await.unwrap();
    let mut buffer = String::new();
    let mut last = id1;
    for want in [second, third] {
        let (id, _, data) = next_event(&mut resumed, &mut buffer).await.unwrap();
        assert!(id > last);
        last = id;
        assert_eq!(data["id"].as_str().unwrap(), want, "no gaps, no duplicates, no foreign events");
    }

    let bad = http.get(format!("http://{addr}/v1/events")).bearer_auth(&a.access).header("last-event-id", "x");
    assert_eq!(bad.send().await.unwrap().status(), 400);

    // Shutdown must END the stream, or draining would wait for it until the grace period expires.
    t.app.shutdown.cancel();
    assert!(next_event(&mut resumed, &mut buffer).await.is_none());
    tokio::time::timeout(GUARD, server).await.unwrap().unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn event_stream_ends_when_the_session_is_revoked() {
    let t = TestApp::new().await;
    let s = t.signup("Acme", "a@acme.io").await;
    let other = t.login("a@acme.io").await.expect(200).str("access_token");
    let (addr, server) = start(&t).await;
    let mut stream = client().get(format!("http://{addr}/v1/events")).bearer_auth(&s.access).send().await.unwrap();
    let mut buffer = String::new();
    t.send(Req::post("/v1/auth/logout").auth(&s.access)).await.expect(204);
    // Another session keeps producing events; the revoked stream must not receive any of them.
    t.send(Req::post("/v1/projects").auth(&other).json(&json!({ "name": "after logout" }))).await.expect(201);
    while let Some((_, topic, _)) = next_event(&mut stream, &mut buffer).await {
        assert_ne!(topic, "project.created", "revoked session still receives data");
    }
    t.app.shutdown.cancel();
    server.await.unwrap().unwrap();
}

/// A request that is in flight when shutdown starts must complete normally.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn graceful_shutdown_drains_in_flight_requests() {
    let store = GatedStore::new();
    // A grace period far beyond the test guard: if the server returns, it is because it drained.
    let t = TestApp::with_store(
        |c| {
            c.server.shutdown_grace_ms = 600_000;
            c.server.request_timeout_ms = 600_000;
        },
        store.clone(),
    )
    .await;
    let s = t.signup("Acme", "a@acme.io").await;
    let project = t.project(&s, "p").await;
    let task = t.task(&s, &project, "t").await;
    let upload = Req::post(format!("/v1/tasks/{task}/attachments")).auth(&s.access).file("a.txt", b"payload");
    let id = t.send(upload).await.expect(201).str("id");
    let link = t.send(Req::get(format!("/v1/attachments/{id}/url")).auth(&s.access)).await.expect(200).str("url");

    let (addr, server) = start(&t).await;
    let download = tokio::spawn(client().get(format!("http://{addr}{link}")).send());
    store.wait_until_entered().await;
    t.app.shutdown.cancel();

    // The listener closes, while the old request is still being served.
    tokio::time::timeout(GUARD, async {
        while tokio::net::TcpStream::connect(addr).await.is_ok() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("still accepting connections");
    assert!(!server.is_finished(), "must wait for the request in flight");

    store.release.add_permits(1);
    let response = download.await.unwrap().unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(&response.bytes().await.unwrap()[..], b"payload");
    tokio::time::timeout(GUARD, server).await.unwrap().unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_gives_up_after_the_grace_period() {
    let store = GatedStore::new(); // never released
    let t = TestApp::with_store(
        |c| {
            c.server.shutdown_grace_ms = 50;
            c.server.request_timeout_ms = 600_000;
        },
        Arc::<GatedStore>::clone(&store),
    )
    .await;
    let (addr, server) = start(&t).await;
    let url = format!("http://{addr}/v1/files/o/k?exp={}&sig={}", i64::MAX, t.app.presigner.sign("o/k", i64::MAX));
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
    let stuck = tokio::spawn(client().get(url).send());
    store.wait_until_entered().await;
    t.app.shutdown.cancel();
    tokio::time::timeout(GUARD, server).await.expect("hung past the grace period").unwrap().unwrap();
    stuck.abort();
}
