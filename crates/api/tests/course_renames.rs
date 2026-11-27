//! A course stage that lists `renamed_from` takes its stored progress from the stages it replaces (a rename, a split, a merge, a stage
//! renamed twice) without changing their rows, and a reset archives and removes those rows too. Each test gets its own database from
//! `#[sqlx::test]`, and a small course written to a temporary directory.

use std::fs;
use std::path::{Path, PathBuf};
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
use chrono::{DateTime, TimeZone, Utc};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

/// Module 1a: `1a-01` is `0x-01` renamed; `0x-02` was split into `1a-02` and `1a-03`.
/// Module 1b: `1b-01` merges `0x-03` and `0x-04`; `1b-02` merges `0x-05` and the stage once called `0x-06`, then `0y-06`; `1b-03` is new.
const STAGES: &[(&str, &str, &str)] = &[
    ("01-a", "1a-01", r#"renamed_from = ["0x-01"]"#),
    ("01-a", "1a-02", r#"renamed_from = ["0x-02"]"#),
    ("01-a", "1a-03", r#"renamed_from = ["0x-02"]"#),
    ("02-b", "1b-01", r#"renamed_from = ["0x-03", "0x-04"]"#),
    ("02-b", "1b-02", r#"renamed_from = ["0x-05", ["0y-06", "0x-06"]]"#),
    ("02-b", "1b-03", ""),
];

fn course_dir(tag: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("anneal-course-renames-{tag}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("course.toml"), "id = \"rn\"\ntitle = \"Renames\"\n\n[[project]]\nnumber = 1\ntitle = \"One\"\n").unwrap();
    for (module, id, extra) in STAGES {
        let m = root.join("modules").join(module);
        fs::create_dir_all(&m).unwrap();
        fs::write(m.join("module.toml"), format!("code = \"{}\"\ntitle = \"M\"\n", &id[..2])).unwrap();
        let st = m.join("stages").join(&id[3..]);
        fs::create_dir_all(&st).unwrap();
        fs::write(st.join("stage.toml"), format!("id = \"{id}\"\ntitle = \"S\"\nkind = \"build\"\ndifficulty = \"easy\"\ntests = [\"t\"]\n{extra}\n")).unwrap();
        fs::write(st.join("stage.md"), "Intro.\n\n## The task\n\nDo it.\n\n## Hints\n\n### First\n\nA nudge.\n\n### Second\n\nThe trap.\n").unwrap();
    }
    root
}

fn course_app(db: PgPool, tag: &str) -> Router {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../content/fixtures/runnable");
    let loaded = Catalog::load(&fixtures).expect("content");
    let course = Course::load(&course_dir(tag)).expect("the course loads");
    let work = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("anneal-course-renames-work-{tag}"));
    app(
        AppState {
            catalog: Arc::new(loaded.catalog),
            runner: Arc::new(Runner::new(RunnerConfig::new(Sandbox::Host, &work))),
            db,
            lsp: LspConfig::new("rust-analyzer", &work),
            auth: AuthConfig::disabled(),
            courses: Arc::new(vec![course]),
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

fn day(d: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, d, 12, 0, 0).unwrap()
}

/// Stored progress under the old ids, as the owner's database holds it: `(stage, solved on day, assisted, hints opened)`.
const OLD: &[(&str, Option<u32>, bool, i32)] = &[
    ("0x-01", Some(1), false, 0),
    ("0x-02", Some(2), false, 1),
    ("0x-03", Some(3), false, 0),
    ("0x-04", None, true, 2),
    ("0x-05", Some(5), true, 1),
    ("0x-06", Some(6), false, 0),
];

async fn seed_old_progress(db: &PgPool) {
    for (id, solved, assisted, hints) in OLD {
        sqlx::query("INSERT INTO course_stage_state (course, stage_id, hints_revealed, solved_at, assisted) VALUES ('rn', $1, $2, $3, $4)")
            .bind(id)
            .bind(hints)
            .bind(solved.map(day))
            .bind(assisted)
            .execute(db)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO course_runs (course, stage_id, ok, passed, total, at) VALUES ('rn', '0x-01', false, 0, 1, $1), ('rn', '0x-01', true, 1, 1, $2)")
        .bind(day(1) - chrono::Duration::hours(1))
        .bind(day(1))
        .execute(db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO course_solutions (course, stage_id, files) VALUES ('rn', '0x-01', $1), ('rn', '0x-03', $2), ('rn', '0x-04', $3)")
        .bind(json!([{ "path": "src/a.rs", "lines": ["+a"] }]))
        .bind(json!([{ "path": "src/b.rs", "lines": ["+b"] }]))
        .bind(json!([{ "path": "src/c.rs", "lines": ["+c"] }]))
        .execute(db)
        .await
        .unwrap();
}

/// Every row stored under an old id, to check nothing changed them.
async fn old_rows(db: &PgPool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT x FROM (
             SELECT 'state ' || to_jsonb(t)::text AS x FROM course_stage_state t WHERE stage_id LIKE '0%'
             UNION ALL SELECT 'run ' || to_jsonb(t)::text FROM course_runs t WHERE stage_id LIKE '0%'
             UNION ALL SELECT 'solution ' || stage_id || files::text FROM course_solutions WHERE stage_id LIKE '0%') r ORDER BY x",
    )
    .fetch_all(db)
    .await
    .unwrap()
}

fn stage_states(tree: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for m in tree["projects"][0]["modules"].as_array().unwrap() {
        for s in m["stages"].as_array().unwrap() {
            out.push((s["id"].as_str().unwrap().to_owned(), s["state"].as_str().unwrap().to_owned()));
        }
    }
    out
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn renamed_split_and_merged_stages_take_their_progress_from_the_stages_they_replace(db: PgPool) {
    let app = course_app(db.clone(), "read");
    seed_old_progress(&db).await;
    let before = old_rows(&db).await;

    let tree = get(&app, "/api/courses/rn").await;
    let want = [
        ("1a-01", "solved"),   // renamed
        ("1a-02", "solved"),   // each part of a split takes over the whole
        ("1a-03", "solved"),
        ("1b-01", "todo"),     // a merge needs every half: 0x-04 never passed
        ("1b-02", "assisted"), // both passed (one of them under its first id), one with help
        ("1b-03", "todo"),
    ];
    assert_eq!(stage_states(&tree), want.map(|(a, b)| (a.to_owned(), b.to_owned())));
    assert_eq!(tree["done"], 4, "{tree}");
    assert_eq!(tree["current"], "1b-01", "the first stage not passed");
    assert_eq!(tree["projects"][0]["modules"][0]["stages"][0]["runs"], 2, "the old id's runs count");
    assert_eq!(tree["last_run"]["stage_id"], "1a-01", "the latest run points to the stage that replaced its stage");

    let page = get(&app, "/api/courses/rn/stages/1a-01").await;
    assert_eq!(page["state"], "solved");
    assert_eq!(page["last_run"]["ok"], true, "the old id's last run: {page}");
    assert_eq!(page["runs"].as_array().unwrap().len(), 2);
    assert_eq!(page["solution"]["open"], true, "a passed stage shows its solution");
    assert_eq!(page["solution"]["files"][0]["path"], "src/a.rs", "the old id's solution");
    assert_eq!(page["hints"]["revealed"].as_array().unwrap().len(), 0, "hints opened under an old id do not carry over");
    assert_eq!(page["up_next"]["id"], "1a-02");

    let merged = get(&app, "/api/courses/rn/stages/1b-01").await;
    assert_eq!(merged["state"], "todo");
    assert_eq!(merged["solution"]["available"], true);
    let module: Vec<&str> = merged["module"]["stages"].as_array().unwrap().iter().map(|s| s["state"].as_str().unwrap()).collect();
    assert_eq!(module, ["todo", "assisted", "todo"]);

    let chain = get(&app, "/api/courses/rn/stages/1b-02").await;
    assert_eq!(chain["state"], "assisted");
    assert_eq!(chain["up_next"]["id"], "1b-03");

    assert_eq!(old_rows(&db).await, before, "reading never changes the old rows");
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn a_stage_writes_its_own_row_and_from_then_on_its_own_row_wins(db: PgPool) {
    let app = course_app(db.clone(), "write");
    seed_old_progress(&db).await;
    let before = old_rows(&db).await;

    // opening a hint of a stage that passed under its old id keeps the pass (and when it was), and is not counted as help
    let (status, page) = call(&app, Method::POST, "/api/courses/rn/stages/1a-02/hints", None).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["state"], "solved", "{page}");
    assert_eq!(page["hints"]["revealed"].as_array().unwrap().len(), 1);
    let own: (Option<DateTime<Utc>>, bool, i32) =
        sqlx::query_as("SELECT solved_at, assisted, hints_revealed FROM course_stage_state WHERE course = 'rn' AND stage_id = '1a-02'").fetch_one(&db).await.unwrap();
    assert_eq!(own, (Some(day(2)), false, 1), "the new row holds the inherited pass");

    // the merge that did not pass: a hint now is help, and passing records the pass under the new id
    call(&app, Method::POST, "/api/courses/rn/stages/1b-01/hints", None).await;
    let pass = json!({ "stage_id": "1b-01", "tests": [{ "name": "t", "ok": true }] });
    assert_eq!(call(&app, Method::POST, "/api/courses/rn/runs", Some(pass)).await.0, StatusCode::OK);
    let page = get(&app, "/api/courses/rn/stages/1b-01").await;
    assert_eq!(page["state"], "assisted");
    assert_eq!(page["runs"].as_array().unwrap().len(), 1, "its own runs replace the old ones in the history");

    // a stage's own row wins over the stages it replaces, whatever they hold
    sqlx::query("INSERT INTO course_stage_state (course, stage_id) VALUES ('rn', '1a-03')").execute(&db).await.unwrap();
    assert_eq!(get(&app, "/api/courses/rn/stages/1a-03").await["state"], "todo");
    // and its own runs win in the counts too
    let fail = json!({ "stage_id": "1a-01", "tests": [{ "name": "t", "ok": false }] });
    call(&app, Method::POST, "/api/courses/rn/runs", Some(fail)).await;
    let tree = get(&app, "/api/courses/rn").await;
    assert_eq!(tree["projects"][0]["modules"][0]["stages"][0]["runs"], 1);
    assert_eq!(tree["projects"][0]["modules"][0]["stages"][0]["state"], "solved", "a failed run leaves the inherited pass");

    assert_eq!(old_rows(&db).await, before, "writes go under the new ids only");
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn a_reset_archives_and_removes_the_rows_of_the_stages_a_stage_replaces(db: PgPool) {
    let app = course_app(db.clone(), "reset");
    seed_old_progress(&db).await;
    let (status, r) = call(&app, Method::POST, "/api/courses/rn/reset", Some(json!({ "module": "1a" }))).await;
    assert_eq!(status, StatusCode::OK, "{r}");
    assert_eq!(r["stages"], 3);
    assert_eq!(r["stage_states_removed"], 2, "0x-01 and 0x-02: {r}");
    assert_eq!(r["runs_removed"], 2);

    let tree = get(&app, "/api/courses/rn").await;
    let states: Vec<String> = stage_states(&tree).into_iter().map(|(_, s)| s).collect();
    assert_eq!(states, ["todo", "todo", "todo", "todo", "assisted", "todo"], "nothing brings module 1a's old passes back; 1b is untouched");

    let archived: Vec<String> = sqlx::query_scalar(
        "SELECT r->>'stage_id' FROM data_archive, jsonb_array_elements(rows) r WHERE source = 'reset_course:rn' ORDER BY 1",
    )
    .fetch_all(&db)
    .await
    .unwrap();
    assert_eq!(archived, ["0x-01", "0x-01", "0x-01", "0x-02"], "two runs and two stage rows, archived before the delete");
    let left: Vec<String> = sqlx::query_scalar("SELECT stage_id FROM course_stage_state WHERE course = 'rn' ORDER BY 1").fetch_all(&db).await.unwrap();
    assert_eq!(left, ["0x-03", "0x-04", "0x-05", "0x-06"]);
    let solutions: i64 = sqlx::query_scalar("SELECT count(*) FROM course_solutions WHERE course = 'rn'").fetch_one(&db).await.unwrap();
    assert_eq!(solutions, 3, "a reset keeps solutions, as before");
}
