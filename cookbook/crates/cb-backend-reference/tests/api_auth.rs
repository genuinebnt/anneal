//! API tests (in process, `oneshot`): accounts, sessions, API keys, RBAC and tenant isolation.

use axum::http::Method;
use backend_ref::testkit::{PASSWORD, Req, Session, TestApp};
use serde_json::json;

#[tokio::test]
async fn registration_requires_email_verification_before_login() {
    let t = TestApp::new().await;
    let body = json!({ "org_name": "Acme", "email": "Ann@Acme.io", "password": PASSWORD });
    t.send(Req::post("/v1/auth/register").json(&body)).await.expect(201);
    t.login("ann@acme.io").await.expect(403);
    assert!(t.mailer.sent().is_empty(), "mail is sent by a job, not inside the request");
    t.run_background().await;
    let token = t.mailer.last_token_for("ann@acme.io").unwrap();
    let verify = || Req::post("/v1/auth/verify").json(&json!({ "token": token }));
    t.send(verify()).await.expect(204);
    t.send(verify()).await.expect(400); // single use
    t.login("ANN@acme.io").await.expect(200);
    // The same address cannot register twice, whatever its case.
    t.send(Req::post("/v1/auth/register").json(&body)).await.expect(409);
    assert_eq!(t.count("orgs").await, 1, "the failed registration must not leave an organisation behind");
}

#[tokio::test]
async fn verification_token_expires() {
    let t = TestApp::new().await;
    let body = json!({ "org_name": "Acme", "email": "a@acme.io", "password": PASSWORD });
    t.send(Req::post("/v1/auth/register").json(&body)).await.expect(201);
    t.run_background().await;
    let token = t.mailer.last_token_for("a@acme.io").unwrap();
    t.clock.advance_secs(t.app.cfg.auth.verify_ttl_secs);
    t.send(Req::post("/v1/auth/verify").json(&json!({ "token": token }))).await.expect(400);
}

#[tokio::test]
async fn live_secrets_are_not_left_in_the_database() {
    let t = TestApp::new().await;
    let s = t.signup("Acme", "a@acme.io").await;
    let mail_token = t.mailer.last_token_for("a@acme.io").unwrap();
    let key =
        t.send(Req::post("/v1/api-keys").auth(&s.access).json(&json!({ "name": "ci" }))).await.expect(201).str("key");
    let dump: String = sqlx::query_scalar(
        "SELECT group_concat(x, ' ') FROM (SELECT payload x FROM jobs UNION ALL SELECT token_hash FROM email_tokens \
         UNION ALL SELECT token_hash FROM refresh_tokens UNION ALL SELECT key_hash FROM api_keys \
         UNION ALL SELECT password_hash FROM users UNION ALL SELECT detail FROM audit_log)",
    )
    .fetch_one(t.app.db.pool())
    .await
    .unwrap();
    for secret in [&mail_token, &s.refresh, &s.access, &key, &PASSWORD.to_string()] {
        assert!(!dump.contains(secret.as_str()), "a secret is stored in clear");
    }
}

#[tokio::test]
async fn wrong_password_and_unknown_email_are_indistinguishable() {
    let t = TestApp::new().await;
    t.signup("Acme", "a@acme.io").await;
    let attempt = |email: &str| {
        Req::post("/v1/auth/login")
            .header("x-request-id", "same")
            .json(&json!({ "email": email, "password": "wrong-password-123" }))
    };
    let wrong = t.send(attempt("a@acme.io")).await.expect(401);
    let unknown = t.send(attempt("nobody@acme.io")).await.expect(401);
    assert_eq!(wrong.body, unknown.body);
}

#[tokio::test]
async fn login_is_throttled_per_account_and_recovers() {
    let t = TestApp::with(|c| c.auth.login_max_failures = 3).await;
    t.signup("Acme", "a@acme.io").await;
    let bad = || Req::post("/v1/auth/login").json(&json!({ "email": "a@acme.io", "password": "wrong-password-123" }));
    for _ in 0..3 {
        t.send(bad()).await.expect(401);
    }
    let blocked = t.login("a@acme.io").await.expect(429); // even the right password
    assert!(blocked.header("retry-after").is_some());
    t.clock.advance_secs(t.app.cfg.auth.login_window_secs);
    t.login("a@acme.io").await.expect(200);
}

fn refresh_req(token: &str) -> Req {
    Req::post("/v1/auth/refresh").json(&json!({ "refresh_token": token }))
}

#[tokio::test]
async fn refresh_rotates_and_reuse_revokes_the_whole_family() {
    let t = TestApp::new().await;
    let victim = t.signup("Acme", "a@acme.io").await;
    let second = t.send(refresh_req(&victim.refresh)).await.expect(200);
    let (access2, refresh2) = (second.str("access_token"), second.str("refresh_token"));
    assert_ne!(refresh2, victim.refresh);
    t.send(Req::get("/v1/me").auth(&access2)).await.expect(200);
    let other_session = t.login("a@acme.io").await.expect(200).str("access_token");

    // The old token shows up again: someone else holds it.
    t.send(refresh_req(&victim.refresh)).await.expect(401);
    t.send(refresh_req(&refresh2)).await.expect(401);
    t.send(Req::get("/v1/me").auth(&access2)).await.expect(401);
    t.send(Req::get("/v1/me").auth(&victim.access)).await.expect(401);
    t.send(Req::get("/v1/me").auth(&other_session)).await.expect(200); // other logins survive

    let detected: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'session.reuse_detected'")
        .fetch_one(t.app.db.pool())
        .await
        .unwrap();
    assert_eq!(detected, 1);
}

#[tokio::test]
async fn access_and_refresh_tokens_expire_and_logout_is_immediate() {
    let t = TestApp::new().await;
    let s = t.signup("Acme", "a@acme.io").await;
    t.clock.advance_secs(t.app.cfg.auth.access_ttl_secs);
    t.send(Req::get("/v1/me").auth(&s.access)).await.expect(401);
    let fresh = t.send(refresh_req(&s.refresh)).await.expect(200);
    let access = fresh.str("access_token");
    t.send(Req::post("/v1/auth/logout").auth(&access)).await.expect(204);
    t.send(Req::get("/v1/me").auth(&access)).await.expect(401);
    t.send(refresh_req(&fresh.str("refresh_token"))).await.expect(401);

    let again = t.login("a@acme.io").await.expect(200).str("refresh_token");
    t.clock.advance_secs(t.app.cfg.auth.refresh_ttl_secs);
    t.send(refresh_req(&again)).await.expect(401);
}

#[tokio::test]
async fn bad_credentials_are_401_with_a_challenge() {
    let t = TestApp::new().await;
    for req in [
        Req::get("/v1/me"),
        Req::get("/v1/me").auth("garbage"),
        Req::get("/v1/me").auth("bk_does-not-exist"),
        Req::get("/v1/me").header("authorization", "Basic abc"),
    ] {
        let r = t.send(req).await.expect(401);
        assert_eq!(r.header("www-authenticate"), Some("Bearer"));
        assert_eq!(r.header("content-type"), Some("application/problem+json"));
    }
}

#[tokio::test]
async fn api_keys_act_as_their_user_until_revoked() {
    let t = TestApp::new().await;
    let s = t.signup("Acme", "a@acme.io").await;
    let created = t.send(Req::post("/v1/api-keys").auth(&s.access).json(&json!({ "name": "ci" }))).await.expect(201);
    let (id, key) = (created.str("id"), created.str("key"));
    assert_eq!(t.send(Req::get("/v1/me").auth(&key)).await.expect(200).str("user_id"), s.user_id);
    // A stolen key cannot mint more keys, and has no session to log out.
    t.send(Req::post("/v1/api-keys").auth(&key).json(&json!({ "name": "more" }))).await.expect(403);
    t.send(Req::post("/v1/auth/logout").auth(&key)).await.expect(400);

    let member = t.member(&s, "m@acme.io", "member").await;
    t.send(Req::delete(format!("/v1/api-keys/{id}")).auth(&member.access)).await.expect(403);
    t.send(Req::delete(format!("/v1/api-keys/{id}")).auth(&s.access)).await.expect(204);
    t.send(Req::get("/v1/me").auth(&key)).await.expect(401);
}

#[tokio::test]
async fn roles_are_enforced_over_http() {
    let t = TestApp::new().await;
    let owner = t.signup("Acme", "owner@acme.io").await;
    let admin = t.member(&owner, "admin@acme.io", "admin").await;
    let alice = t.member(&owner, "alice@acme.io", "member").await;
    let bob = t.member(&owner, "bob@acme.io", "member").await;
    let viewer = t.member(&owner, "viewer@acme.io", "viewer").await;

    let project = t.project(&alice, "alice's").await;
    let task = t.task(&alice, &project, "alice's task").await;

    let new_project = || json!({ "name": "x" });
    t.send(Req::post("/v1/projects").auth(&viewer.access).json(&new_project())).await.expect(403);
    t.send(Req::get(format!("/v1/projects/{project}")).auth(&viewer.access)).await.expect(200);

    let rename = |who: &Session| {
        Req::patch(format!("/v1/tasks/{task}")).auth(&who.access).if_match(1).json(&json!({ "title": "renamed" }))
    };
    t.send(rename(&bob)).await.expect(403);
    t.send(rename(&viewer)).await.expect(403);
    t.send(Req::delete(format!("/v1/tasks/{task}")).auth(&bob.access)).await.expect(403);
    t.send(Req::delete(format!("/v1/projects/{project}")).auth(&bob.access).if_match(1)).await.expect(403);
    t.send(rename(&alice)).await.expect(200);

    // Assignment gives bob the right to modify, but not to delete.
    let assign = Req::patch(format!("/v1/tasks/{task}"))
        .auth(&admin.access)
        .if_match(2)
        .json(&json!({ "assignee_id": bob.user_id }));
    t.send(assign).await.expect(200);
    let done =
        Req::patch(format!("/v1/tasks/{task}")).auth(&bob.access).if_match(3).json(&json!({ "status": "done" }));
    t.send(done).await.expect(200);
    t.send(Req::delete(format!("/v1/tasks/{task}")).auth(&bob.access)).await.expect(403);
    t.send(Req::delete(format!("/v1/tasks/{task}")).auth(&admin.access)).await.expect(204);

    for (who, role, status) in [
        (&alice, "viewer", 403),
        (&admin, "admin", 403),
        (&admin, "owner", 403),
        (&admin, "viewer", 201),
        (&owner, "admin", 201),
    ] {
        let body =
            json!({ "email": format!("{}-{role}@acme.io", &who.email[..3]), "password": PASSWORD, "role": role });
        t.send(Req::post("/v1/members").auth(&who.access).json(&body)).await.expect(status);
    }
    t.send(Req::get("/v1/audit").auth(&alice.access)).await.expect(403);
    t.send(Req::get("/v1/webhooks").auth(&alice.access)).await.expect(403);
    t.send(Req::get("/v1/audit").auth(&admin.access)).await.expect(200);
    let hook = json!({ "url": "http://127.0.0.1:9/h" });
    t.send(Req::post("/v1/webhooks").auth(&alice.access).json(&hook)).await.expect(403);
}

/// Every route that takes an id, tried by the owner of ANOTHER organisation, who has the highest role.
#[tokio::test]
async fn another_tenant_cannot_touch_anything() {
    let t = TestApp::new().await;
    let a = t.signup("Acme", "a@acme.io").await;
    let mallory = t.signup("Evil", "m@evil.io").await;
    let project = t.project(&a, "secret roadmap").await;
    let task = t.task(&a, &project, "secret acquisition").await;
    let comment = json!({ "body": "secret" });
    t.send(Req::post(format!("/v1/tasks/{task}/comments")).auth(&a.access).json(&comment)).await.expect(201);
    let upload = Req::post(format!("/v1/tasks/{task}/attachments")).auth(&a.access).file("a.txt", b"secret");
    let attachment = t.send(upload).await.expect(201).str("id");
    let key =
        t.send(Req::post("/v1/api-keys").auth(&a.access).json(&json!({ "name": "k" }))).await.expect(201).str("id");
    let before: Vec<i64> = snapshot(&t).await;

    let p = format!("/v1/projects/{project}");
    let k = format!("/v1/tasks/{task}");
    let attempts: Vec<(Method, String, Option<serde_json::Value>)> = vec![
        (Method::GET, p.clone(), None),
        (Method::PATCH, p.clone(), Some(json!({ "name": "pwned" }))),
        (Method::DELETE, p.clone(), None),
        (Method::GET, format!("{p}/stats"), None),
        (Method::GET, format!("{p}/tasks"), None),
        (Method::POST, format!("{p}/tasks"), Some(json!({ "title": "pwned" }))),
        (Method::GET, k.clone(), None),
        (Method::PATCH, k.clone(), Some(json!({ "title": "pwned" }))),
        (Method::DELETE, k.clone(), None),
        (Method::GET, format!("{k}/comments"), None),
        (Method::POST, format!("{k}/comments"), Some(json!({ "body": "pwned" }))),
        (Method::GET, format!("{k}/attachments"), None),
        (Method::GET, format!("/v1/attachments/{attachment}/url"), None),
        (Method::DELETE, format!("/v1/api-keys/{key}"), None),
    ];
    // Warm the owner's cache first: a cache hit must not bypass the tenant check.
    t.send(Req::get(format!("{p}/stats")).auth(&a.access)).await.expect(200);
    for (method, path, body) in attempts {
        let mut req = Req::new(method.clone(), &path).auth(&mallory.access).if_match(1);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let r = t.send(req).await;
        assert_eq!(r.status, 404, "{method} {path}: {r:?}");
        assert!(!String::from_utf8_lossy(&r.body).contains("secret"));
    }
    let upload = Req::post(format!("{k}/attachments")).auth(&mallory.access).file("x.txt", b"pwned");
    t.send(upload).await.expect(404);
    assert_eq!(snapshot(&t).await, before, "nothing may have been written");

    for path in ["/v1/projects", "/v1/tasks", "/v1/tasks?q=secret", "/v1/audit", "/v1/webhooks"] {
        let r = t.send(Req::get(path).auth(&mallory.access)).await.expect(200);
        let text = r.json().to_string();
        assert!(!text.contains("secret") && !text.contains(&project) && !text.contains(&a.user_id), "{path}: {text}");
    }
    // Nor can a tenant pull a foreign user into its own data.
    let own = t.project(&mallory, "mine").await;
    let assign = Req::post(format!("/v1/projects/{own}/tasks"))
        .auth(&mallory.access)
        .json(&json!({ "title": "x", "assignee_id": a.user_id }));
    t.send(assign).await.expect(422);
}

async fn snapshot(t: &TestApp) -> Vec<i64> {
    let mut out = vec![];
    for table in ["projects", "tasks", "comments", "attachments", "outbox"] {
        out.push(t.count(table).await);
    }
    let versions: i64 = sqlx::query_scalar(
        "SELECT (SELECT sum(version) FROM projects) + (SELECT sum(version) FROM tasks) \
         + (SELECT count(*) FROM api_keys WHERE revoked_at IS NULL)",
    )
    .fetch_one(t.app.db.pool())
    .await
    .unwrap();
    out.push(versions);
    out
}
