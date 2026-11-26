//! `/api/series`: progress comes from the stored attempts and review dates only. Each test gets its own database from
//! `#[sqlx::test]`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anneal_api::auth::AuthConfig;
use anneal_api::lsp::LspConfig;
use anneal_api::{AppState, app};
use anneal_content::Catalog;
use anneal_runner::{Runner, RunnerConfig, Sandbox};
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

const ONE: &str = "l2-two-mutable-borrows-of-self";
const TWO: &str = "d9-find-center-of-star-graph";
const STEP: &str = "d9-network-delay-time";

const SERIES: &str = r#"
[series]
title = "A path"
summary = "Two parts, one to write and a step."

[[stage]]
id = "first"
name = "First"

[[part]]
id = "l2-two-mutable-borrows-of-self"
stage = "first"
new_idea = "Borrow one field at a time"
blurb = "Change two things at once"
used_again = ["d9-find-center-of-star-graph"]

[[part]]
id = "d9-find-center-of-star-graph"
stage = "first"
minutes = 15
required = ["l2-two-mutable-borrows-of-self"]
new_idea = "Count once"
reuses = "part 1"

[[part]]
stage = "first"
title = "Not there yet"
status = "to-write"

[[capstone]]
id = "d9-network-delay-time"
title = "The whole thing"
required = ["d9-find-center-of-star-graph"]

[[revisit]]
part = "l2-two-mutable-borrows-of-self"
after = "d9-find-center-of-star-graph"
how = "Solve part 1 again."

[[readiness]]
group = "Graphs"
concept = "counting"
parts = ["l2-two-mutable-borrows-of-self", "d9-find-center-of-star-graph"]
"#;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../content/fixtures/runnable")
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// The runnable fixtures with a series over their three problems.
fn test_app(db: PgPool, name: &str) -> Router {
    let content = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("series-content-{name}"));
    let _ = std::fs::remove_dir_all(&content);
    copy_dir(&fixtures(), &content);
    std::fs::create_dir_all(content.join("series")).unwrap();
    std::fs::write(content.join("series/path.toml"), SERIES).unwrap();
    let loaded = Catalog::load(&content).expect("content");
    assert!(loaded.issues.is_empty(), "{:?}", loaded.issues);
    let work = Path::new(env!("CARGO_TARGET_TMPDIR")).join("anneal-series");
    app(
        AppState {
            catalog: Arc::new(loaded.catalog),
            runner: Arc::new(Runner::new(RunnerConfig::new(Sandbox::Host, &work))),
            db,
            lsp: LspConfig::new("rust-analyzer", &work),
            auth: AuthConfig::disabled(),
            courses: Arc::new(Vec::new()),
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

async fn get(app: &Router, path: &str) -> Value {
    let (status, body) = call(app, Method::GET, path, None).await;
    assert_eq!(status, StatusCode::OK, "{path}: {body}");
    body
}

/// An attempt as the app stores one, solved `days_ago` days ago.
async fn attempt(db: &PgPool, id: &str, kind: &str, days_ago: i32, assisted: bool) {
    sqlx::query("INSERT INTO attempts (problem_id, kind, started_at, solved_at, assisted) VALUES ($1, $2, now() - make_interval(days => $3), now() - make_interval(days => $3), $4)")
        .bind(id)
        .bind(kind)
        .bind(days_ago)
        .bind(assisted)
        .execute(db)
        .await
        .unwrap();
}

async fn stored_rows(db: &PgPool) -> (i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM attempts), (SELECT count(*) FROM reviews), (SELECT count(*) FROM drafts)").fetch_one(db).await.unwrap()
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn a_new_reader_starts_at_part_one_and_reading_stores_nothing(db: PgPool) {
    let app = test_app(db.clone(), "new");
    let list = get(&app, "/api/series").await;
    assert_eq!(list[0]["id"], "path");
    assert_eq!(list[0]["segments"], json!([false, false, false]));
    assert_eq!((&list[0]["solved"], &list[0]["to_write"], &list[0]["steps"]), (&json!(0), &json!(1), &json!(1)));
    let view = get(&app, "/api/series/path").await;
    assert_eq!(view["next"]["id"], ONE);
    assert_eq!(view["next_why"], "This is where the series starts. The main idea: change two things at once.");
    let rows = view["groups"][0]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 4, "three parts and the repeat");
    assert_eq!(rows[0]["idea"], "Change two things at once", "the blurb stands in for the idea");
    assert_eq!((&rows[0]["state"], &rows[1]["state"], &rows[2]["state"]), (&json!("next"), &json!("open"), &json!("to_write")));
    assert_eq!(rows[2]["id"], Value::Null);
    assert_eq!((&rows[3]["row"], &rows[3]["state"]), (&json!("repeat"), &json!("waiting")));
    assert_eq!(view["capstone"]["steps"][0]["state"], "open");
    assert_eq!(view["needs"][0]["parts"], "1 and 2");

    let place = get(&app, &format!("/api/problems/{TWO}/series")).await;
    assert_eq!((&place["kind"], &place["number"], &place["total"], &place["minutes"]), (&json!("part"), &json!(2), &json!(3), &json!(15)));
    assert_eq!(place["required"][0]["id"], ONE);
    assert_eq!(place["done"], Value::Null, "the completion view is not sent before a solve");
    assert_eq!(get(&app, &format!("/api/problems/{STEP}/series")).await["kind"], "step");
    // the pager's neighbours follow the parts and then the capstone, passing over what is still to be written
    let sides = |place: Value| (place["prev"].clone(), place["next"].clone());
    assert_eq!(sides(get(&app, &format!("/api/problems/{ONE}/series")).await), (Value::Null, json!(TWO)));
    assert_eq!(sides(place.clone()), (json!(ONE), json!(STEP)));
    assert_eq!(sides(get(&app, &format!("/api/problems/{STEP}/series")).await), (json!(TWO), Value::Null));
    assert_eq!(get(&app, "/api/problems/no-such-problem/series").await, Value::Null);
    assert_eq!(call(&app, Method::GET, "/api/series/none", None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(stored_rows(&db).await, (0, 0, 0));
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn progress_and_repeats_come_from_attempts_and_review_dates(db: PgPool) {
    let app = test_app(db.clone(), "progress");
    attempt(&db, ONE, "practice", 9, false).await;
    attempt(&db, TWO, "practice", 5, true).await;
    sqlx::query("INSERT INTO reviews (problem_id, step, due_at, last_result, history, stability, difficulty, last_review, last_grade, reps) VALUES ($1, 1, now() + interval '6 days', 'unassisted', '[]', 6, 5, now(), 'good', 1)")
        .bind(ONE)
        .execute(&db)
        .await
        .unwrap();
    let view = get(&app, "/api/series/path").await;
    let rows = view["groups"][0]["rows"].as_array().unwrap();
    assert_eq!((&rows[0]["state"], &rows[1]["state"], &rows[3]["state"]), (&json!("solved"), &json!("assisted"), &json!("scheduled")));
    assert_eq!(view["solved"], 2);
    assert_eq!(view["next"]["id"], STEP, "a part still to be written is passed over");

    sqlx::query("UPDATE reviews SET due_at = now() - interval '1 day'").execute(&db).await.unwrap();
    assert_eq!(get(&app, "/api/series/path").await["groups"][0]["rows"][3]["state"], "due");

    let done = get(&app, &format!("/api/problems/{TWO}/series")).await["done"].clone();
    assert_eq!(done["practised"], json!([{ "label": "New", "text": "Count once" }, { "label": "Again", "text": "part 1" }]));
    assert_eq!(done["repeat"][0]["text"], "Solve part 1 again.");
    assert_eq!(done["next"]["id"], STEP);
    assert_eq!(done["skipped"], json!(["Part 3"]));

    // a re-solve of part 1 after part 2 was first solved is the planned repeat
    attempt(&db, ONE, "resolve", 0, false).await;
    assert_eq!(get(&app, "/api/series/path").await["groups"][0]["rows"][3]["state"], "done");
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn a_passing_submit_moves_the_series_on_and_a_re_solve_keeps_the_credit(db: PgPool) {
    let app = test_app(db.clone(), "submit");
    let code = std::fs::read_to_string(fixtures().join("tracks/l2-borrowing/problems/two-mutable-borrows-of-self/solution.rs")).unwrap();
    let (status, out) = call(&app, Method::POST, &format!("/api/problems/{ONE}/submit"), Some(json!({ "code": code }))).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["attempt"]["solved"], true, "{out}");
    let view = get(&app, "/api/series/path").await;
    assert_eq!(view["segments"], json!([true, false, false]));
    assert_eq!(view["next"]["id"], TWO);
    let done = get(&app, &format!("/api/problems/{ONE}/series")).await["done"].clone();
    assert!(done["repeat"][0]["due"].is_string(), "the app's own re-solve date: {done}");
    assert_eq!(done["returns"][0]["label"], "Part 2");

    // starting the re-solve opens a new attempt; the series still counts the part as solved
    assert_eq!(call(&app, Method::POST, &format!("/api/problems/{ONE}/resolve"), None).await.0, StatusCode::OK);
    assert_eq!(get(&app, "/api/series/path").await["solved"], 1);
}
