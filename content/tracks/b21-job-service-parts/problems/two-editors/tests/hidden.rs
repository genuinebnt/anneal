use solution::*;

#[allow(unused_imports)]
use axum::Router;
#[allow(unused_imports)]
use axum::body::{Body, to_bytes};
#[allow(unused_imports)]
use axum::http::Request;
#[allow(unused_imports)]
use serde_json::{Value, json};
#[allow(unused_imports)]
use tower::ServiceExt;

#[allow(dead_code)]
pub fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct Reply {
    pub status: u16,
    pub etag: Option<String>,
    pub location: Option<String>,
    pub body: Value,
}

#[allow(dead_code)]
pub async fn send(app: &Router, method: &str, slug: &str, headers: &[(&str, &str)], body: &str) -> Reply {
    let mut request = Request::builder().method(method).uri(format!("/pages/{slug}"));
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = app.clone().oneshot(request.body(Body::from(body.to_string())).unwrap()).await.unwrap();
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let text = |name: &str| headers.get(name).map(|v| v.to_str().unwrap().to_string());
    let (etag, location) = (text("etag"), text("location"));
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    Reply { status, etag, location, body }
}

#[allow(dead_code)]
pub async fn create(app: &Router, slug: &str, body: &str) -> Reply {
    send(app, "PUT", slug, &[("if-none-match", "*")], body).await
}

#[allow(dead_code)]
pub async fn update(app: &Router, slug: &str, tag: &str, body: &str) -> Reply {
    send(app, "PUT", slug, &[("if-match", tag)], body).await
}

#[allow(dead_code)]
pub async fn read(app: &Router, slug: &str) -> Reply {
    send(app, "GET", slug, &[], "").await
}

#[allow(dead_code)]
/// (status, etag, body) of a read.
pub async fn seen(app: &Router, slug: &str) -> (u16, Option<String>, Value) {
    let r = read(app, slug).await;
    (r.status, r.etag, r.body)
}

#[allow(dead_code)]
pub fn tag(v: u64) -> Option<String> {
    Some(format!("\"v{v}\""))
}

#[allow(dead_code)]
pub fn page(title: &str, body: &str) -> Value {
    json!({"title": title, "body": body})
}

#[test]
fn if_match_never_creates() {
    run(async {
        let app = router();
        let r = update(&app, "a", "\"v1\"", r#"{"title":"x"}"#).await;
        check!("If-Match on a missing page: (status, etag)", (r.status, r.etag), (412, None));
        check!("still missing", read(&app, "a").await.status, 404);
    });
}

#[test]
fn only_the_exact_tag_matches() {
    run(async {
        let app = router();
        create(&app, "a", r#"{"title":"x"}"#).await;
        for bad in ["v1", "\"1\"", "\"v2\"", "\"v10\"", "\"v01\"", "W/\"v1\"", "*", "\"v1\", \"v2\"", ""] {
            let r = update(&app, "a", bad, r#"{"title":"y"}"#).await;
            check!(format!("If-Match: {bad}"), (r.status, r.etag), (412, tag(1)));
        }
        check!("unchanged", seen(&app, "a").await, (200, tag(1), page("x", "")));
    });
}

#[test]
fn confused_preconditions_are_bad_requests() {
    run(async {
        let app = router();
        create(&app, "a", r#"{"title":"x"}"#).await;
        let both = send(&app, "PUT", "a", &[("if-match", "\"v1\""), ("if-none-match", "*")], r#"{"title":"y"}"#).await;
        check!("both headers", both.status, 400);
        let tagged = send(&app, "PUT", "b", &[("if-none-match", "\"v1\"")], r#"{"title":"y"}"#).await;
        check!("If-None-Match with a tag", tagged.status, 400);
        check!("nothing changed", (read(&app, "a").await.etag, read(&app, "b").await.status), (tag(1), 404));
    });
}

#[test]
fn bad_bodies_change_nothing() {
    run(async {
        let app = router();
        create(&app, "a", r#"{"title":"x"}"#).await;
        for bad in ["", "{", "[]", "null", r#"{}"#, r#"{"body":"b"}"#, r#"{"title":5}"#, r#"{"title":"t","body":null}"#, r#"{"title":"t","extra":1}"#, r#"{"title":"t","Body":"b"}"#, r#"{"title":"t"} x"#] {
            check!(format!("update with body {bad:?}"), update(&app, "a", "\"v1\"", bad).await.status, 400);
            check!(format!("create with body {bad:?}"), create(&app, "b", bad).await.status, 400);
        }
        check!("a is unchanged and b does not exist", (seen(&app, "a").await, read(&app, "b").await.status), ((200, tag(1), page("x", "")), 404));
    });
}

#[test]
fn checks_run_in_the_stated_order() {
    run(async {
        let app = router();
        create(&app, "a", r#"{"title":"x"}"#).await;
        check!("no precondition and a bad body", send(&app, "PUT", "a", &[], "{").await.status, 428);
        check!("a stale tag and a bad body", update(&app, "a", "\"v9\"", "{").await.status, 400);
    });
}

#[test]
fn content_type_does_not_matter() {
    run(async {
        let app = router();
        let r = send(&app, "PUT", "a", &[("if-none-match", "*"), ("content-type", "text/plain")], r#"{"title":"x"}"#).await;
        check!("text/plain body", r.status, 201);
    });
}

#[test]
fn replacing_drops_the_old_body() {
    run(async {
        let app = router();
        create(&app, "a", r#"{"title":"x","body":"long text"}"#).await;
        update(&app, "a", "\"v1\"", r#"{"title":"x"}"#).await;
        check!("a write replaces the whole page", read(&app, "a").await.body, page("x", ""));
    });
}

#[test]
fn pages_are_independent_and_services_start_empty() {
    run(async {
        let app = router();
        create(&app, "a", r#"{"title":"a"}"#).await;
        update(&app, "a", "\"v1\"", r#"{"title":"a2"}"#).await;
        let b = create(&app, "b", r#"{"title":"b"}"#).await;
        check!("a new page starts at v1", (b.status, b.etag), (201, tag(1)));
        check!("another router() has no pages", read(&router(), "a").await.status, 404);
    });
}

#[test]
fn only_one_of_many_simultaneous_writes_wins() {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().unwrap();
    runtime.block_on(async {
        let app = router();
        create(&app, "a", r#"{"title":"x"}"#).await;
        for round in 1..=100u64 {
            let current = format!("\"v{round}\"");
            let mut tasks = Vec::new();
            for i in 0..8 {
                let (app, current) = (app.clone(), current.clone());
                tasks.push(tokio::spawn(async move { update(&app, "a", &current, &format!(r#"{{"title":"{i}"}}"#)).await.status }));
            }
            let mut statuses = Vec::new();
            for t in tasks {
                statuses.push(t.await.unwrap());
            }
            let wins = statuses.iter().filter(|s| **s == 200).count();
            check!(format!("round {round}: statuses {statuses:?}, number of 200s"), wins, 1);
        }
        check!("version after 100 rounds", read(&app, "a").await.etag, tag(101));
    });
}

#[test]
fn random_sessions_match_a_model() {
    run(async {
        let mut rng = anneal_prelude::Rng::new(21301);
        for _ in 0..150 {
            let app = router();
            let mut model: std::collections::HashMap<&str, (u64, String)> = Default::default();
            let mut history = Vec::new();
            for step in 0..15 {
                let slug = *rng.pick(&["a", "b"]);
                let title = format!("t{step}");
                let body = json!({"title": title}).to_string();
                let current = model.get(slug).map(|(v, _)| *v);
                let guess = (rng.below(4) as u64) + 1;
                let creating = rng.below(3) == 0;
                history.push(if creating { format!("create {slug}") } else { format!("update {slug} from v{guess}") });
                let (got, want) = if creating {
                    let want = if current.is_none() { (201, tag(1)) } else { (412, current.and_then(tag)) };
                    let r = create(&app, slug, &body).await;
                    ((r.status, r.etag), want)
                } else {
                    let want = match current {
                        Some(v) if v == guess => (200, tag(v + 1)),
                        other => (412, other.and_then(tag)),
                    };
                    let r = update(&app, slug, &format!("\"v{guess}\""), &body).await;
                    ((r.status, r.etag), want)
                };
                if want.0 < 300 {
                    model.insert(slug, (current.unwrap_or(0) + 1, title));
                }
                check!(format!("{history:?}: last (status, etag)"), got, want);
                let stored = seen(&app, slug).await;
                let expected = match model.get(slug) {
                    Some((v, t)) => (200, tag(*v), page(t, "")),
                    None => (404, None, Value::Null),
                };
                check!(format!("{history:?}: page {slug} afterwards"), stored, expected);
            }
        }
    });
}

#[test]
fn no_write_slips_between_the_comparison_and_the_write() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, OnceLock};
    for creating in [true, false] {
        let app = Arc::new(OnceLock::<Router>::new());
        let armed = Arc::new(AtomicBool::new(false));
        let rival = Arc::new(Mutex::new(None));
        let (in_hook, fire, slot) = (app.clone(), armed.clone(), rival.clone());
        let router = router_with_hook(move |_| {
            if !fire.swap(false, Ordering::SeqCst) {
                return;
            }
            // While one write stands between its comparison and its change, a second one arrives.
            let app = in_hook.get().expect("set").clone();
            let (done, wait) = std::sync::mpsc::channel();
            let thread = std::thread::spawn(move || {
                let status = run(async {
                    let body = r#"{"title":"second"}"#;
                    let reply = if creating { create(&app, "a", body).await } else { update(&app, "a", "\"v1\"", body).await };
                    reply.status
                });
                let _ = done.send(());
                status
            });
            // A write that has to wait for the first cannot finish here, so do not wait for ever.
            let _ = wait.recv_timeout(std::time::Duration::from_millis(500));
            *slot.lock().unwrap() = Some(thread);
        });
        let _ = app.set(router.clone());
        run(async {
            if !creating {
                create(&router, "a", r#"{"title":"x"}"#).await;
            }
            armed.store(true, Ordering::SeqCst);
            let body = r#"{"title":"first"}"#;
            let first = if creating { create(&router, "a", body).await } else { update(&router, "a", "\"v1\"", body).await };
            let thread = rival.lock().unwrap().take().expect("the hook was not called for a precondition that holds");
            let second = thread.join().expect("the second write panicked");
            let mut statuses = [first.status, second];
            statuses.sort();
            let want = if creating { [201, 412] } else { [200, 412] };
            check!(format!("two writes at once (creating: {creating}): their statuses, sorted"), statuses, want);
        });
    }
}

#[test]
fn a_filled_array_is_not_a_page() {
    run(async {
        let app = router();
        create(&app, "a", r#"{"title":"x"}"#).await;
        for bad in [r#"["t","b"]"#, r#"["t"]"#, r#" ["t","b"] "#] {
            check!(format!("update with body {bad:?}"), update(&app, "a", "\"v1\"", bad).await.status, 400);
            check!(format!("create with body {bad:?}"), create(&app, "b", bad).await.status, 400);
        }
        check!("a is unchanged and b does not exist", (seen(&app, "a").await, read(&app, "b").await.status), ((200, tag(1), page("x", "")), 404));
    });
}
