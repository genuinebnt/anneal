//! `PUT /api/courses/{course}/concepts/{id}/read`: marking a concept unread keeps its row (and when it was first read). Each test gets its
//! own database from `#[sqlx::test]`.

use std::path::Path;
use std::sync::Arc;

use anneal_api::auth::AuthConfig;
use anneal_api::lsp::LspConfig;
use anneal_api::{AppState, app};
use anneal_content::Catalog;
use anneal_content::course::Course;
use anneal_runner::{Runner, RunnerConfig, Sandbox};
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

const CONCEPT: &str = "buffer-pool-anatomy";

fn course_app(db: PgPool) -> Router {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../content/fixtures/runnable");
    let loaded = Catalog::load(&fixtures).expect("content");
    let bustub = Course::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../courses/bustub")).expect("the BusTub course");
    let work = Path::new(env!("CARGO_TARGET_TMPDIR")).join("anneal-course-concepts");
    app(
        AppState {
            catalog: Arc::new(loaded.catalog),
            runner: Arc::new(Runner::new(RunnerConfig::new(Sandbox::Host, &work))),
            db,
            lsp: LspConfig::new("rust-analyzer", &work),
            auth: AuthConfig::disabled(),
            courses: Arc::new(vec![bustub]),
            meaning: anneal_api::meaning::Meaning::off(),
            ai: None,
        },
        None,
    )
}

async fn call(app: &Router, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let req = Request::builder().method(method).uri(path).header("content-type", "application/json");
    let req = req.body(body.map_or_else(Body::empty, |b| Body::from(b.to_string()))).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn set_read(app: &Router, read: bool) {
    let (status, body) = call(app, Method::PUT, &format!("/api/courses/bustub/concepts/{CONCEPT}/read"), Some(json!({ "read": read }))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

async fn shown_as_read(app: &Router) -> bool {
    let (status, body) = call(app, Method::GET, &format!("/api/courses/bustub/concepts/{CONCEPT}"), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["read"].as_bool().expect("the concept page says whether it was read")
}

async fn row(db: &PgPool) -> Option<(chrono::DateTime<chrono::Utc>, Option<chrono::DateTime<chrono::Utc>>)> {
    sqlx::query_as("SELECT read_at, unread_at FROM course_concept_state WHERE course = 'bustub' AND concept = $1")
        .bind(CONCEPT)
        .fetch_optional(db)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn marking_a_concept_unread_keeps_its_row_and_when_it_was_first_read(db: PgPool) {
    let app = course_app(db.clone());
    assert!(!shown_as_read(&app).await, "a concept starts unread");
    set_read(&app, true).await;
    assert!(shown_as_read(&app).await, "marked read");
    let (first_read, unread) = row(&db).await.expect("reading stores a row");
    assert_eq!(unread, None);

    set_read(&app, false).await;
    assert!(!shown_as_read(&app).await, "marked unread, the page says so");
    let (read_at, unread) = row(&db).await.expect("marking unread must not delete the row: the owner's history is never lost");
    assert_eq!(read_at, first_read, "when it was first read is kept");
    assert!(unread.is_some(), "the row is marked unread instead");

    set_read(&app, true).await;
    assert!(shown_as_read(&app).await, "read again");
    let (read_at, unread) = row(&db).await.unwrap();
    assert_eq!((read_at, unread), (first_read, None), "reading again clears the mark and keeps the first reading");
}
