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
fn create_then_read() {
    run(async {
        let app = router();
        let r = create(&app, "home", r#"{"title":"Home","body":"Hello"}"#).await;
        check!("create /pages/home: (status, etag, location, body)", (r.status, r.etag, r.location, r.body), (201, tag(1), Some("/pages/home".to_string()), page("Home", "Hello")));
        check!("GET /pages/home", seen(&app, "home").await, (200, tag(1), page("Home", "Hello")));
    });
}

#[test]
fn update_with_the_current_tag() {
    run(async {
        let app = router();
        create(&app, "a", r#"{"title":"one"}"#).await;
        let r = update(&app, "a", "\"v1\"", r#"{"title":"two"}"#).await;
        check!("update from v1: (status, etag, body)", (r.status, r.etag, r.body), (200, tag(2), page("two", "")));
    });
}

#[test]
fn the_second_editor_is_refused() {
    run(async {
        let app = router();
        create(&app, "a", r#"{"title":"start"}"#).await;
        update(&app, "a", "\"v1\"", r#"{"title":"first editor"}"#).await;
        let r = update(&app, "a", "\"v1\"", r#"{"title":"second editor"}"#).await;
        check!("second write from v1: (status, etag)", (r.status, r.etag), (412, tag(2)));
        check!("the first edit is kept", seen(&app, "a").await, (200, tag(2), page("first editor", "")));
    });
}

#[test]
fn a_write_must_say_where_it_started() {
    run(async {
        let app = router();
        check!("PUT without a precondition", send(&app, "PUT", "a", &[], r#"{"title":"x"}"#).await.status, 428);
        check!("nothing was created", read(&app, "a").await.status, 404);
    });
}

#[test]
fn creating_twice_fails() {
    run(async {
        let app = router();
        create(&app, "a", r#"{"title":"mine"}"#).await;
        let r = create(&app, "a", r#"{"title":"yours"}"#).await;
        check!("second create: (status, etag)", (r.status, r.etag), (412, tag(1)));
    });
}

#[test]
fn unknown_page() {
    run(async {
        check!("GET /pages/nothing", read(&router(), "nothing").await.status, 404);
    });
}
