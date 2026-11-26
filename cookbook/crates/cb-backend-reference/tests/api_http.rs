//! API tests (in process): HTTP behaviour, concurrency control, pagination, idempotency, rate limits,
//! caching, uploads, the OpenAPI contract and observability.

use axum::http::Method;
use backend_ref::http::{openapi::openapi_path, routes};
use backend_ref::idempotency::REPLAY_HEADER;
use backend_ref::testkit::{Req, TestApp};
use serde_json::{Value, json};

#[tokio::test]
async fn errors_are_problem_json_for_every_layer() {
    let t = TestApp::new().await;
    let s = t.signup("Acme", "a@acme.io").await;
    let post = || Req::post("/v1/projects").auth(&s.access);
    let cases = [
        (Req::get("/nope"), 404),
        (Req::new(Method::PUT, "/v1/projects"), 405),
        (post().raw("text/plain", r#"{"name":"x"}"#), 415),
        (post().json(&json!({ "name": "x" })).header("accept", "text/html"), 406),
        (post().raw("application/json", "{not json"), 400),
        (post().json(&json!({ "name": "x", "nmae": "typo" })), 422),
        (post().json(&json!({ "name": "" })), 422),
        (post().raw("application/json", vec![b' '; t.app.cfg.server.body_limit_bytes + 1]), 413),
        // The idempotency layer buffers the body itself and must apply the same limit.
        (post().header("idempotency-key", "big").raw("application/json", vec![b' '; 70_000]), 413),
        (Req::get("/v1/tasks?staus=open").auth(&s.access), 422),
        (Req::get("/v1/tasks?cursor=!!!").auth(&s.access), 400),
        (Req::get("/v1/projects?cursor=!!!").auth(&s.access), 400),
    ];
    for (i, (req, status)) in cases.into_iter().enumerate() {
        let r = t.send(req).await;
        assert_eq!(r.status, status, "case {i}: {r:?}");
        assert_eq!(r.header("content-type"), Some("application/problem+json"), "case {i}");
        assert_eq!(r.json()["status"], status, "case {i}");
    }
    assert_eq!(t.count("projects").await, 0, "a refused request (406 included) must not write");
}

#[tokio::test]
async fn validation_reports_every_field() {
    let t = TestApp::new().await;
    let body = json!({ "org_name": "", "email": "nope", "password": "short" });
    let r = t.send(Req::post("/v1/auth/register").json(&body)).await.expect(422);
    let fields: Vec<String> =
        r.json()["errors"].as_array().unwrap().iter().map(|e| e["field"].as_str().unwrap().into()).collect();
    assert_eq!(fields, ["org_name", "email", "password"]);
}

#[tokio::test]
async fn security_headers_request_id_and_cors() {
    let t = TestApp::with(|c| c.server.cors_origins = vec!["https://app.example".into()]).await;
    for path in ["/healthz", "/nope", "/v1/me"] {
        let r = t.send(Req::get(path)).await;
        assert_eq!(r.header("x-content-type-options"), Some("nosniff"), "{path}");
        assert_eq!(r.header("x-frame-options"), Some("DENY"), "{path}");
        assert_eq!(r.header("cache-control"), Some("no-store"), "{path}");
        assert!(r.header("content-security-policy").is_some(), "{path}");
        assert!(r.header("strict-transport-security").is_none(), "no HSTS outside prod");
    }
    let r = t.send(Req::get("/v1/me").header("x-request-id", "abc-123")).await.expect(401);
    assert_eq!(r.header("x-request-id"), Some("abc-123"));
    assert_eq!(r.json()["request_id"], "abc-123");
    let r = t.send(Req::get("/healthz").header("x-request-id", "bad id <script>")).await;
    assert_ne!(r.header("x-request-id"), Some("bad id <script>"), "unsafe ids are replaced");

    let preflight = |origin: &str| {
        Req::new(Method::OPTIONS, "/v1/projects")
            .header("origin", origin)
            .header("access-control-request-method", "POST")
    };
    let ok = t.send(preflight("https://app.example")).await.expect(200);
    assert_eq!(ok.header("access-control-allow-origin"), Some("https://app.example"));
    let no = t.send(preflight("https://evil.example")).await;
    assert_eq!(no.header("access-control-allow-origin"), None);
}

#[tokio::test]
async fn large_responses_are_compressed_on_request() {
    let t = TestApp::new().await;
    let r = t.send(Req::get("/openapi.json").header("accept-encoding", "gzip")).await.expect(200);
    assert_eq!(r.header("content-encoding"), Some("gzip"));
    assert_eq!(&r.body[..2], [0x1f, 0x8b]);
    assert!(t.send(Req::get("/openapi.json")).await.header("content-encoding").is_none());
}

#[tokio::test]
async fn etags_drive_conditional_reads_and_optimistic_writes() {
    let t = TestApp::new().await;
    let s = t.signup("Acme", "a@acme.io").await;
    let id = t.project(&s, "v1").await;
    let url = format!("/v1/projects/{id}");
    let first = t.send(Req::get(&url).auth(&s.access)).await.expect(200);
    let etag = first.header("etag").unwrap().to_string();
    let cached = t.send(Req::get(&url).auth(&s.access).header("if-none-match", &etag)).await.expect(304);
    assert!(cached.body.is_empty());

    let patch = |name: &str| Req::patch(&url).auth(&s.access).json(&json!({ "name": name }));
    t.send(patch("no precondition")).await.expect(428);
    let updated = t.send(patch("v2").header("if-match", &etag)).await.expect(200);
    assert_eq!(updated.header("etag"), Some("\"v2\""));
    t.send(patch("lost update").header("if-match", &etag)).await.expect(412);
    let now = t.send(Req::get(&url).auth(&s.access).header("if-none-match", &etag)).await.expect(200);
    assert_eq!(now.str("name"), "v2");
    t.send(Req::delete(&url).auth(&s.access)).await.expect(428);
    t.send(Req::delete(&url).auth(&s.access).if_match(1)).await.expect(412);
    t.send(Req::delete(&url).auth(&s.access).if_match(2)).await.expect(204);
}

#[tokio::test]
async fn deleting_a_project_hides_its_tasks_everywhere() {
    let t = TestApp::new().await;
    let s = t.signup("Acme", "a@acme.io").await;
    let project = t.project(&s, "p").await;
    let task = t.task(&s, &project, "zebra migration").await;
    t.send(Req::delete(format!("/v1/projects/{project}")).auth(&s.access).if_match(1)).await.expect(204);
    t.send(Req::get(format!("/v1/tasks/{task}")).auth(&s.access)).await.expect(404);
    for path in ["/v1/tasks", "/v1/tasks?q=zebra"] {
        let r = t.send(Req::get(path).auth(&s.access)).await.expect(200);
        assert_eq!(r.json()["items"], json!([]), "{path}");
    }
}

async fn all_ids(t: &TestApp, token: &str, base: &str) -> Vec<String> {
    let mut ids = vec![];
    let mut url = base.to_string();
    for _ in 0..100 {
        let page = t.send(Req::get(&url).auth(token)).await.expect(200).json();
        ids.extend(page["items"].as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap().to_string()));
        match page["next_cursor"].as_str() {
            Some(c) => url = format!("{base}&cursor={c}"),
            None => return ids,
        }
    }
    panic!("pagination did not terminate");
}

#[tokio::test]
async fn pagination_filtering_and_search_over_http() {
    let t = TestApp::new().await;
    let s = t.signup("Acme", "a@acme.io").await;
    let project = t.project(&s, "p").await;
    let mut created = vec![];
    for i in 0..23 {
        let title = if i % 5 == 0 { format!("fix login {i}") } else { format!("chore {i}") };
        created.push(t.task(&s, &project, &title).await);
        t.clock.advance_ms(i64::from(i % 3)); // some equal timestamps
    }
    for sort in ["created_at", "-created_at", "title", "-priority"] {
        let mut ids = all_ids(&t, &s.access, &format!("/v1/tasks?limit=4&sort={sort}")).await;
        assert_eq!(ids.len(), 23, "{sort}");
        ids.sort();
        let mut want = created.clone();
        want.sort();
        assert_eq!(ids, want, "{sort}: every task exactly once");
    }
    let found = all_ids(&t, &s.access, "/v1/tasks?limit=2&q=logi").await;
    assert_eq!(found.len(), 5);
    assert_eq!(all_ids(&t, &s.access, &format!("/v1/projects/{project}/tasks?limit=50&status=done")).await.len(), 0);

    // A cursor is bound to the sort order that produced it.
    let page = t.send(Req::get("/v1/tasks?limit=2&sort=title").auth(&s.access)).await.expect(200).str("next_cursor");
    t.send(Req::get(format!("/v1/tasks?sort=-priority&cursor={page}")).auth(&s.access)).await.expect(400);

    for i in 0..5 {
        t.project(&s, &format!("extra {i}")).await;
    }
    assert_eq!(all_ids(&t, &s.access, "/v1/projects?limit=2").await.len(), 6);
}

#[tokio::test]
async fn idempotency_keys_replay_and_detect_mismatches() {
    let t = TestApp::new().await;
    let s = t.signup("Acme", "a@acme.io").await;
    let create = |key: &str, name: &str| {
        Req::post("/v1/projects").auth(&s.access).header("idempotency-key", key).json(&json!({ "name": name }))
    };
    let first = t.send(create("k1", "one")).await.expect(201);
    assert_eq!(first.header(REPLAY_HEADER), None);
    let again = t.send(create("k1", "one")).await.expect(201);
    assert_eq!(again.header(REPLAY_HEADER), Some("true"));
    assert_eq!(again.body, first.body);
    assert_eq!(again.header("etag"), first.header("etag"));
    assert_eq!(t.count("projects").await, 1);
    t.send(create("k1", "different")).await.expect(409);
    t.send(create(&"x".repeat(129), "one")).await.expect(400);

    // The same key used by another user is unrelated (no cross-tenant replay).
    let other = t.signup("Other", "o@other.io").await;
    let req =
        Req::post("/v1/projects").auth(&other.access).header("idempotency-key", "k1").json(&json!({ "name": "one" }));
    let r = t.send(req).await.expect(201);
    assert_eq!(r.header(REPLAY_HEADER), None);
    assert_ne!(r.str("id"), first.str("id"));

    // The same key on another route is a different request.
    let project = first.str("id");
    let task = Req::post(format!("/v1/projects/{project}/tasks"))
        .auth(&s.access)
        .header("idempotency-key", "k1")
        .json(&json!({ "name": "one" }));
    t.send(task).await.expect(409);

    // A deterministic client error is replayed too.
    t.send(create("k2", "")).await.expect(422);
    assert_eq!(t.send(create("k2", "")).await.expect(422).header(REPLAY_HEADER), Some("true"));
}

#[tokio::test]
async fn failed_requests_release_the_idempotency_key() {
    let t = TestApp::new().await;
    let s = t.signup("Acme", "a@acme.io").await;
    let create =
        || Req::post("/v1/projects").auth(&s.access).header("idempotency-key", "k").json(&json!({ "name": "p" }));
    sqlx::query("CREATE TRIGGER boom BEFORE INSERT ON projects BEGIN SELECT RAISE(ABORT, 'injected'); END")
        .execute(t.app.db.pool())
        .await
        .unwrap();
    t.send(create()).await.expect(500);
    sqlx::query("DROP TRIGGER boom").execute(t.app.db.pool()).await.unwrap();
    let ok = t.send(create()).await.expect(201);
    assert_eq!(ok.header(REPLAY_HEADER), None, "the retry really ran");
}

#[tokio::test]
async fn rate_limit_is_per_principal_with_headers() {
    let t = TestApp::new().await;
    let a = t.signup("Acme", "a@acme.io").await;
    let b = t.signup("Other", "b@other.io").await;
    let key =
        t.send(Req::post("/v1/api-keys").auth(&a.access).json(&json!({ "name": "k" }))).await.expect(201).str("key");
    // A second app instance over the same database, with a tiny bucket.
    let mut cfg = t.app.cfg.clone();
    cfg.rate_limit.capacity = 3;
    cfg.rate_limit.refill_per_sec = 1.0;
    let deps = backend_ref::state::Deps {
        clock: t.clock.clone(),
        ids: std::sync::Arc::new(backend_ref::ports::UuidIds),
        mailer: t.mailer.clone(),
        store: std::sync::Arc::new(t.store.clone()),
    };
    let app = backend_ref::state::App::build(cfg, t.app.db.clone(), deps).unwrap();
    let limited = TestApp {
        router: backend_ref::http::router(app.clone()),
        app,
        clock: t.clock.clone(),
        mailer: t.mailer.clone(),
        store: t.store.clone(),
    };

    let me = |token: &str| Req::get("/v1/me").auth(token);
    for remaining in ["2", "1", "0"] {
        let r = limited.send(me(&a.access)).await.expect(200);
        assert_eq!(r.header("ratelimit-limit"), Some("3"));
        assert_eq!(r.header("ratelimit-remaining"), Some(remaining));
    }
    let r = limited.send(me(&a.access)).await.expect(429);
    assert_eq!(r.header("retry-after"), Some("1"));
    assert_eq!(r.header("ratelimit-remaining"), Some("0"));
    limited.send(me(&b.access)).await.expect(200);
    limited.send(me(&key)).await.expect(200); // API keys have their own bucket
    limited.clock.advance_secs(1);
    limited.send(me(&a.access)).await.expect(200);
    limited.send(me(&a.access)).await.expect(429);
}

#[tokio::test]
async fn stats_are_cached_and_invalidated_by_writes() {
    let t = TestApp::new().await;
    let s = t.signup("Acme", "a@acme.io").await;
    let project = t.project(&s, "p").await;
    let task = t.task(&s, &project, "one").await;
    let url = format!("/v1/projects/{project}/stats");
    let stats = || Req::get(&url).auth(&s.access);
    let first = t.send(stats()).await.expect(200);
    assert_eq!(first.json()["by_status"], json!({ "open": 1 }));
    let etag = first.header("etag").unwrap().to_string();
    t.send(stats().header("if-none-match", &etag)).await.expect(304);

    // Proof that it IS cached: a change made behind the application's back is not visible...
    sqlx::query("UPDATE tasks SET status = 'done'").execute(t.app.db.pool()).await.unwrap();
    assert_eq!(t.send(stats()).await.json()["by_status"], json!({ "open": 1 }));
    // ...while every write through the API invalidates immediately.
    t.task(&s, &project, "two").await;
    assert_eq!(t.send(stats()).await.json()["by_status"], json!({ "done": 1, "open": 1 }));
    let patch =
        Req::patch(format!("/v1/tasks/{task}")).auth(&s.access).if_match(1).json(&json!({ "status": "in_progress" }));
    t.send(patch).await.expect(200);
    let after = t.send(stats().header("if-none-match", &etag)).await.expect(200);
    assert_eq!(after.json()["by_status"], json!({ "in_progress": 1, "open": 1 }));
    t.send(Req::delete(format!("/v1/tasks/{task}")).auth(&s.access)).await.expect(204);
    assert_eq!(t.send(stats()).await.json()["by_status"], json!({ "open": 1 }));
}

#[tokio::test]
async fn upload_and_presigned_download() {
    let t = TestApp::with(|c| c.server.upload_limit_bytes = 200_000).await;
    let s = t.signup("Acme", "a@acme.io").await;
    let project = t.project(&s, "p").await;
    let task = t.task(&s, &project, "t").await;
    let url = format!("/v1/tasks/{task}/attachments");
    let upload = |name: &str, data: &[u8]| Req::post(&url).auth(&s.access).file(name, data);

    // Larger than the global JSON body limit, smaller than the upload limit.
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.resize(150_000, 7);
    let a = t.send(upload("../../etc/cat pic.png", &png)).await.expect(201).json();
    assert_eq!(a["content_type"], "image/png", "sniffed, although the part claimed text/html");
    assert_eq!(a["filename"], "cat_pic.png");
    assert_eq!(a["size"], 150_000);
    assert_eq!(a["sha256"], hex::encode(<sha2::Sha256 as sha2::Digest>::digest(&png)));

    t.send(upload("big.txt", &vec![b'a'; 200_001])).await.expect(413);
    t.send(upload("evil.png", b"MZ\x90\x00\x03\x00\x00\x00")).await.expect(415);
    t.send(upload("empty.txt", b"")).await.expect(415);
    t.send(Req::post(&url).auth(&s.access).json(&json!({}))).await.expect(415);
    assert_eq!((t.store.len(), t.count("attachments").await), (1, 1), "rejected uploads leave nothing");

    let id = a["id"].as_str().unwrap();
    let link = t.send(Req::get(format!("/v1/attachments/{id}/url")).auth(&s.access)).await.expect(200).str("url");
    let file = t.send(Req::get(&link)).await.expect(200); // no credentials
    assert_eq!(&file.body[..], &png[..]);
    assert_eq!(file.header("content-type"), Some("image/png"));
    assert_eq!(file.header("content-disposition"), Some("attachment; filename=\"cat_pic.png\""));
    assert_eq!(file.header("x-content-type-options"), Some("nosniff"));

    let last = link.chars().last().unwrap();
    let tampered = format!("{}{}", &link[..link.len() - 1], if last == '0' { '1' } else { '0' });
    t.send(Req::get(tampered)).await.expect(403);
    t.send(Req::get(link.replace("exp=", "exp=9"))).await.expect(403);
    t.send(Req::get(link.split('?').next().unwrap())).await.expect(403);
    t.clock.advance_secs(t.app.cfg.storage.download_ttl_secs + 1);
    t.send(Req::get(&link)).await.expect(403);
}

#[tokio::test]
async fn openapi_documents_exactly_the_served_routes() {
    let t = TestApp::with(|c| c.debug_routes = true).await;
    let doc = t.send(Req::get("/openapi.json")).await.expect(200).json();
    let defs = routes(true);
    let documented: usize = doc["paths"].as_object().unwrap().values().map(|p| p.as_object().unwrap().len()).sum();
    assert_eq!(documented, defs.len(), "two routes share a method and path, or one is missing");
    for r in &defs {
        let op = &doc["paths"][openapi_path(r.path)][r.method.as_str().to_lowercase()];
        assert!(op["summary"].as_str().is_some_and(|s| !s.is_empty()), "{} {}", r.method, r.path);
        // Served: a probe must reach a handler (or its auth layer), not the 404/405 fallbacks.
        let concrete = r.path.replace("{id}", "x").replace("{*key}", "a/b");
        let probe = t.send(Req::new(r.method.clone(), &concrete)).await;
        let fallback = probe.status == 405 || (probe.status == 404 && probe.json()["detail"] == "route not found");
        assert!(!fallback, "{} {} is documented but not served", r.method, r.path);
        // Documented security must match behaviour.
        let secured = !op["security"].is_null();
        assert_eq!(probe.status == 401, secured, "{} {} answered {}", r.method, r.path, probe.status);
    }
    // Every $ref resolves.
    let text = doc.to_string();
    for name in text.split("#/components/schemas/").skip(1).map(|s| s.split('"').next().unwrap()) {
        assert!(doc["components"]["schemas"][name].is_object(), "dangling schema {name}");
    }
}

/// The schemas are hand-written, so check them against what the server really requires.
#[tokio::test]
async fn openapi_request_schemas_match_the_extractors() {
    let t = TestApp::new().await;
    let s = t.signup("Acme", "a@acme.io").await;
    let project = t.project(&s, "p").await;
    let task = t.task(&s, &project, "t").await;
    let doc = t.send(Req::get("/openapi.json")).await.expect(200).json();
    for r in routes(false) {
        let backend_ref::http::Body::Json(name) = r.body else { continue };
        let schema = &doc["components"]["schemas"][name];
        let id = if r.path.starts_with("/v1/tasks") { &task } else { &project };
        let path = r.path.replace("{id}", id);
        // A body with every documented property is never rejected for its SHAPE.
        let full: serde_json::Map<String, Value> = schema["properties"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), if v["type"] == "integer" { json!(1) } else { json!("x") }))
            .collect();
        let req = Req::new(r.method.clone(), &path).auth(&s.access).if_match(1).json(&Value::Object(full.clone()));
        let reply = t.send(req).await;
        let detail = String::from_utf8_lossy(&reply.body).into_owned();
        assert!(!detail.contains("unknown field") && !detail.contains("missing field"), "{name}: {detail}");
        // Omitting any required property is.
        for required in schema["required"].as_array().unwrap() {
            let mut partial = full.clone();
            partial.remove(required.as_str().unwrap());
            let req = Req::new(r.method.clone(), &path).auth(&s.access).if_match(1).json(&Value::Object(partial));
            let reply = t.send(req).await.expect(422);
            assert!(reply.str("detail").contains("missing field"), "{name} without {required}: {reply:?}");
        }
    }
}

#[tokio::test]
async fn health_readiness_metrics_and_debug_routes() {
    let t = TestApp::new().await;
    t.send(Req::get("/healthz")).await.expect(200);
    t.send(Req::get("/readyz")).await.expect(200);
    t.send(Req::get("/debug/config")).await.expect(404);
    let s = t.signup("Acme", "a@acme.io").await;
    let id = t.project(&s, "p").await;
    t.send(Req::get(format!("/v1/projects/{id}")).auth(&s.access)).await.expect(200);
    t.send(Req::get("/random/abc")).await.expect(404);
    let text = String::from_utf8(t.send(Req::get("/metrics")).await.expect(200).body.to_vec()).unwrap();
    assert!(text.contains(r#"http_requests_total{method="GET",route="/v1/projects/{id}",status="200"} 1"#), "{text}");
    assert!(text.contains(r#"route="unmatched",status="404""#));
    assert!(!text.contains(&id) && !text.contains("/random/abc"), "raw paths must not become labels");
    assert!(text.contains(r#"jobs{state="done"}"#));

    t.app.shutdown.cancel();
    assert_eq!(t.send(Req::get("/readyz")).await.expect(503).str("status"), "draining");
    t.send(Req::get("/healthz")).await.expect(200);

    let debug = TestApp::with(|c| c.debug_routes = true).await;
    let body = debug.send(Req::get("/debug/config")).await.expect(200).json().to_string();
    assert!(!body.contains(debug.app.cfg.auth.token_secret.expose()));
    assert!(body.contains("***"));
}

#[tokio::test]
async fn audit_log_records_who_did_what() {
    let t = TestApp::new().await;
    let s = t.signup("Acme", "a@acme.io").await;
    let project = t.project(&s, "p").await;
    let task = t.task(&s, &project, "t").await;
    t.send(Req::delete(format!("/v1/tasks/{task}")).auth(&s.access)).await.expect(204);
    let log = t.send(Req::get("/v1/audit").auth(&s.access)).await.expect(200).json();
    let rows: Vec<(&str, &str)> = log["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["action"].as_str().unwrap(), r["actor"].as_str().unwrap()))
        .collect();
    let me = s.user_id.as_str();
    let want =
        ["user.registered", "user.verified", "session.created", "project.created", "task.created", "task.deleted"];
    assert_eq!(rows, want.map(|a| (a, me)));
}
