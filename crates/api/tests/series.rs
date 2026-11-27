//! `/api/series`: progress comes from the stored attempts and review dates only. Each test gets its own database from
//! `#[sqlx::test]`.

use std::collections::BTreeMap;
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
    fixture_app(db, name, &[("path", SERIES)])
}

/// The runnable fixtures with these series files over them.
fn fixture_app(db: PgPool, name: &str, files: &[(&str, &str)]) -> Router {
    let content = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("series-content-{name}"));
    let _ = std::fs::remove_dir_all(&content);
    copy_dir(&fixtures(), &content);
    std::fs::create_dir_all(content.join("series")).unwrap();
    for (id, text) in files {
        std::fs::write(content.join(format!("series/{id}.toml")), text).unwrap();
    }
    app_over(db, &content)
}

fn app_over(db: PgPool, content: &Path) -> Router {
    let loaded = Catalog::load(content).expect("content");
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

/// A series of these problems, in order.
fn series_of(title: &str, rank: Option<u32>, ids: &[&str]) -> String {
    let rank = rank.map_or_else(String::new, |r| format!("rank = {r}\n"));
    let parts: String = ids.iter().map(|id| format!("[[part]]\nid = \"{id}\"\nstage = \"s\"\n")).collect();
    format!("[series]\ntitle = \"{title}\"\n{rank}[[stage]]\nid = \"s\"\nname = \"S\"\n{parts}")
}

/// `TWO` is in all three; `ONE` is only in `alpha` and `STEP` only in `zeta`. Only `zeta` has a rank.
fn shared_app(db: PgPool, name: &str) -> Router {
    let (alpha, beta, zeta) = (series_of("Alpha", None, &[ONE, TWO]), series_of("Beta", None, &[TWO]), series_of("Zeta", Some(1), &[TWO, STEP]));
    fixture_app(db, name, &[("alpha", &alpha), ("beta", &beta), ("zeta", &zeta)])
}

/// The series shown for a problem, then the others it can be switched to.
async fn shown(app: &Router, id: &str, query: &str) -> Vec<String> {
    let place = get(app, &format!("/api/problems/{id}/series{query}")).await;
    let others = place["others"].as_array().unwrap().iter().map(|o| o["id"].as_str().unwrap().to_owned());
    std::iter::once(place["series"]["id"].as_str().unwrap().to_owned()).chain(others).collect()
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn the_series_the_reader_came_from_is_the_one_shown(db: PgPool) {
    let app = shared_app(db.clone(), "asked");
    assert_eq!(shown(&app, TWO, "").await, ["zeta", "alpha", "beta"], "ranked first, then by id");
    assert_eq!(shown(&app, TWO, "?series=alpha").await, ["alpha", "zeta", "beta"]);
    assert_eq!(shown(&app, TWO, "?series=beta").await, ["beta", "zeta", "alpha"]);
    // everything about the place is that series' own
    let place = get(&app, &format!("/api/problems/{TWO}/series?series=alpha")).await;
    assert_eq!((&place["series"]["title"], &place["number"], &place["total"]), (&json!("Alpha"), &json!(2), &json!(2)));
    assert_eq!(place["others"][0], json!({ "id": "zeta", "title": "Zeta", "kind": "part", "number": 1 }));
    // an unknown series, an empty one and one that lacks the problem are passed over
    for query in ["?series=none", "?series=", "?series=..%2Fzeta"] {
        assert_eq!(shown(&app, TWO, query).await, ["zeta", "alpha", "beta"], "{query}");
    }
    assert_eq!(shown(&app, ONE, "?series=zeta").await, ["alpha"]);
    assert_eq!(get(&app, "/api/problems/no-such-problem/series?series=zeta").await, Value::Null);
    assert_eq!(stored_rows(&db).await, (0, 0, 0));
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn without_one_progress_that_only_one_series_has_decides(db: PgPool) {
    let app = shared_app(db.clone(), "choice");
    // solving the shared problem says nothing about which series is being followed, so a Submit can't move the reader
    attempt(&db, TWO, "practice", 3, false).await;
    assert_eq!(shown(&app, TWO, "").await, ["zeta", "alpha", "beta"]);
    // something only `alpha` has: it wins over the rank
    attempt(&db, ONE, "practice", 2, true).await;
    assert_eq!(shown(&app, TWO, "").await, ["alpha", "zeta", "beta"]);
    assert_eq!(shown(&app, TWO, "?series=zeta").await, ["zeta", "alpha", "beta"], "what was asked for still wins");
    let next = get(&app, &format!("/api/problems/{TWO}/series?series=zeta")).await["done"]["next"]["id"].clone();
    assert_eq!(next, STEP, "the next one is that series' next");
    assert_eq!(get(&app, &format!("/api/problems/{TWO}/series")).await["done"]["next"], Value::Null, "`alpha` ends here");
    // progress in both: back to the stable order
    attempt(&db, STEP, "practice", 1, false).await;
    assert_eq!(shown(&app, TWO, "").await, ["zeta", "alpha", "beta"]);
}

/// With the real content: a reader with nothing that says otherwise sees the web-server series for each of its problems,
/// as they did when it was the only one.
#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn the_web_server_series_keeps_its_problems(db: PgPool) {
    let app = app_over(db, &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"));
    let view = get(&app, "/api/series/web-server").await;
    let rows = view["groups"].as_array().unwrap().iter().flat_map(|g| g["rows"].as_array().unwrap()).filter(|r| r["row"] == "item");
    let ids: Vec<&str> = rows.chain(view["capstone"]["steps"].as_array().unwrap()).filter_map(|r| r["id"].as_str()).collect();
    let mut shared = 0;
    for id in ids {
        let here = shown(&app, id, "").await;
        assert_eq!(here[0], "web-server", "{id}");
        shared += usize::from(here.len() > 1);
    }
    assert!(shared > 0, "the test needs a problem that another series has too");
}

/// The exercises that several series share. Each is shown in whichever of its series was asked for, with that series' own
/// number, neighbours and switch. A new overlap fails here, so that someone decides which series comes first.
#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn each_shared_exercise_works_in_each_of_its_series(db: PgPool) {
    // in the stable order, so the first is the one shown when nothing says otherwise
    const SHARED: [(&str, &[&str]); 33] = [
        ("b1-api-error-response", &["job-service", "type-ahead"]),
        ("b4-length-prefixed-codec", &["web-server", "mini-redis"]),
        ("b7-how-long-requests-take", &["web-server", "type-ahead"]),
        ("b7-requests-in-pieces", &["web-server", "mini-redis"]),
        ("b7-talk-over-a-connection", &["web-server", "mini-redis"]),
        ("c1-blocking-queue", &["web-server", "async-runtime"]),
        ("c2-thread-pool", &["web-server", "async-runtime"]),
        ("c3-atomic-stats", &["web-server", "rate-limiter"]),
        ("c4-bounded-fan-out", &["job-service", "mini-redis"]),
        ("c4-drain-with-deadline", &["job-service", "mini-redis"]),
        ("c4-fix-cancel-safe-reader", &["mini-redis", "async-runtime"]),
        ("f4-lru-cache-arena", &["toy-filesystem", "type-ahead"]),
        ("f7-wire-format-json", &["job-service", "event-ledger"]),
        ("l1-raii-span-guard", &["web-server", "rate-limiter", "scripting-language"]),
        ("l3-binary-frame-parser", &["key-value-store", "toy-filesystem"]),
        ("l3-zero-copy-tokenizer", &["regex-engine", "scripting-language", "sql-engine"]),
        ("l5-const-fn-crc-table", &["key-value-store", "mini-redis", "toy-filesystem"]),
        ("l8-import-errors", &["key-value-store", "event-ledger"]),
        ("m3-claim-without-a-lock", &["rate-limiter", "lease-simulator"]),
        ("s4-btreemap-ranges", &["key-value-store", "mini-redis"]),
        ("s6-merge-sorted-streams", &["key-value-store", "query-optimizer"]),
        ("y10-trigram-index", &["search-engine", "type-ahead"]),
        ("y14-pratt-parser", &["scripting-language", "sql-engine"]),
        ("y15-expand-the-braces", &["regex-engine", "scripting-language"]),
        ("y15-lay-a-program-out-flat", &["regex-engine", "scripting-language"]),
        ("y15-print-it-so-it-reads-back", &["regex-engine", "query-optimizer"]),
        ("y17-after-the-power-cut", &["key-value-store", "toy-filesystem"]),
        ("y17-append-that-fails", &["key-value-store", "event-ledger"]),
        ("y17-unsaved-changes", &["key-value-store", "toy-filesystem"]),
        ("y18-records-and-frames", &["key-value-store", "mini-redis"]),
        ("y41-rolls-you-can-replay", &["lease-simulator", "ray-tracer"]),
        ("y5-catch-the-broken-queue", &["key-value-store", "regex-engine", "event-ledger"]),
        ("y9-kv-store-wal", &["key-value-store", "mini-redis", "event-ledger"]),
    ];
    let app = app_over(db.clone(), &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"));
    // each series' written items in order, as its page lists them
    let mut paths = Vec::new();
    let mut holders: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for summary in get(&app, "/api/series").await.as_array().unwrap() {
        let id = summary["id"].as_str().unwrap().to_owned();
        let view = get(&app, &format!("/api/series/{id}")).await;
        let rows = view["groups"].as_array().unwrap().iter().flat_map(|g| g["rows"].as_array().unwrap()).filter(|r| r["row"] == "item");
        let mut items: Vec<Value> = rows.chain(view["capstone"]["steps"].as_array().unwrap()).filter(|r| r["id"].is_string()).cloned().collect();
        items.sort_by_key(|r| (r["kind"] != "part", r["number"].as_u64()));
        for item in &items {
            holders.entry(item["id"].as_str().unwrap().to_owned()).or_default().push(id.clone());
        }
        paths.push((id, items));
    }
    holders.retain(|_, in_series| in_series.len() > 1);
    let sorted = |mut v: Vec<String>| {
        v.sort();
        v
    };
    let found: Vec<(String, Vec<String>)> = holders.into_iter().map(|(id, v)| (id, sorted(v))).collect();
    let expected: Vec<(String, Vec<String>)> = SHARED.iter().map(|(id, v)| ((*id).to_owned(), sorted(v.iter().map(|x| (*x).to_owned()).collect()))).collect();
    assert_eq!(found, expected);

    for (id, order) in SHARED {
        assert_eq!(shown(&app, id, "").await, order, "{id}");
        assert_eq!(shown(&app, id, "?series=nope").await, order, "{id}");
        for &asked in order {
            // the one asked for, then the rest in the stable order
            let others: Vec<&str> = order.iter().copied().filter(|s| *s != asked).collect();
            let want: Vec<&str> = std::iter::once(asked).chain(others.iter().copied()).collect();
            assert_eq!(shown(&app, id, &format!("?series={asked}")).await, want, "{id}");
            let place = get(&app, &format!("/api/problems/{id}/series?series={asked}")).await;
            let items = &paths.iter().find(|(s, _)| s == asked).unwrap().1;
            let at = items.iter().position(|r| r["id"] == id).unwrap();
            assert_eq!(place["number"], items[at]["number"], "{id} in {asked}");
            assert_eq!(place["minutes"], items[at]["minutes"], "{id} in {asked}");
            let side = |i: Option<usize>| i.and_then(|i| items.get(i)).map_or(Value::Null, |r| r["id"].clone());
            assert_eq!((&place["prev"], &place["next"]), (&side(at.checked_sub(1)), &side(Some(at + 1))), "{id} in {asked}");
            for (i, other) in others.iter().enumerate() {
                let there = &paths.iter().find(|(s, _)| s == other).unwrap().1;
                assert_eq!(place["others"][i]["number"], there.iter().find(|r| r["id"] == id).unwrap()["number"], "{id} from {asked}");
            }
        }
    }
    assert_eq!(stored_rows(&db).await, (0, 0, 0));
}
