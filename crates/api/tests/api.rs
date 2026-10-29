//! Each test gets its own database from `#[sqlx::test]` (needs the compose Postgres)
//! and runs code with host cargo.

use std::path::Path;
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

const NDT: &str = "d9-network-delay-time";

const SENTINEL_BUG: &str = r#"use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub fn network_delay(times: &[(usize, usize, u32)], n: usize, k: usize) -> Option<u32> {
    let mut adj = vec![Vec::new(); n + 1];
    for &(u, v, w) in times {
        adj[u].push((v, w));
    }
    let mut dist = vec![u32::MAX; n + 1];
    let mut heap = BinaryHeap::new();
    dist[k] = 0;
    heap.push(Reverse((0, k)));
    while let Some(Reverse((d, u))) = heap.pop() {
        if d > dist[u] { continue; }
        for &(v, w) in &adj[u] {
            let nd = d + w;
            if nd < dist[v] {
                dist[v] = nd;
                heap.push(Reverse((nd, v)));
            }
        }
    }
    dist[1..].iter().copied().max()
}
"#;

/// D9's problem ids in track order, from the content the app serves: counts and positions follow the content
/// instead of being pinned, since the track keeps growing.
fn d9_ids() -> Vec<String> {
    let loaded = Catalog::load(&content_root()).expect("content");
    loaded.catalog.track("D9").expect("D9").problems.iter().map(|p| p.id.clone()).collect()
}

fn fixtures() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../content/fixtures")
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

/// The content the API tests run against, `crates/content/fixtures/runnable`: a two-problem slice of the old D9 Graphs track
/// and one L2 Borrowing problem, which the tests solve in Rust. (The DSA section's real problems are LeetCode links.)
fn content_root() -> std::path::PathBuf {
    fixtures().join("runnable")
}

fn test_app(db: PgPool) -> Router {
    test_app_with(db, &content_root())
}

fn test_app_with(db: PgPool, content: &Path) -> Router {
    test_app_auth(db, content, AuthConfig::disabled())
}

fn test_app_auth(db: PgPool, content: &Path, auth: AuthConfig) -> Router {
    test_app_full(db, content, auth, None)
}

fn test_app_full(db: PgPool, content: &Path, auth: AuthConfig, web_dist: Option<&Path>) -> Router {
    let loaded = Catalog::load(content).expect("content");
    assert!(loaded.issues.is_empty(), "{:?}", loaded.issues);
    let runner = Runner::new(RunnerConfig::new(
        Sandbox::Host,
        Path::new(env!("CARGO_TARGET_TMPDIR")).join("anneal-api"),
    ));
    app(
        AppState {
            catalog: Arc::new(loaded.catalog),
            runner: Arc::new(runner),
            db,
            lsp: LspConfig::new(
                "rust-analyzer",
                Path::new(env!("CARGO_TARGET_TMPDIR")).join("anneal-api"),
            ),
            auth,
            courses: Arc::new(vec![anneal_content::course::Course::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../courses/bustub")).expect("courses/bustub")]),
        },
        web_dist,
    )
}

async fn call(
    app: &Router,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    let req = req
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        },
    )
}

fn solution() -> String {
    std::fs::read_to_string(content_root().join("tracks/d9-graphs/problems/network-delay-time/solution.rs")).unwrap()
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn lists_tracks_with_stage_counts(db: PgPool) {
    let app = test_app(db);
    let (status, body) = call(&app, Method::GET, "/api/tracks", None).await;
    assert_eq!(status, StatusCode::OK);
    let d9 = body
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["code"] == "D9")
        .unwrap();
    assert_eq!(
        (
            d9["total"].as_u64(),
            d9["ready"].as_u64(),
            d9["solved"].as_u64()
        ),
        (Some(d9_ids().len() as u64), Some(d9_ids().len() as u64), Some(0))
    );
    assert_eq!(d9["readiness"], 0.0);
    assert_eq!(d9["stages"][0]["band"], "easy");
    assert_eq!(
        d9["stages"].as_array().unwrap().last().unwrap()["band"],
        "medium"
    );
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn new_problem_has_everything_locked_and_no_hidden_tests(db: PgPool) {
    let app = test_app(db);
    let (status, p) = call(&app, Method::GET, &format!("/api/problems/{NDT}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(p["stage"]["name"], "Shortest paths");
    assert_eq!(
        (p["position"].as_u64(), p["count"].as_u64()),
        (Some(d9_ids().iter().position(|id| id == NDT).unwrap() as u64 + 1), Some(d9_ids().len() as u64))
    );
    assert_eq!(p["hints"]["total"], 3);
    assert_eq!(p["hints"]["revealed"], json!([]));
    assert_eq!(
        p["hints"]["locked"],
        json!(["approach", "rust", "edge case"])
    );
    assert_eq!(
        p["solution"],
        json!({ "unlocked": false, "code": null, "notes": null })
    );
    assert_eq!(p["attempt"]["started"], false);
    assert!(!p.to_string().contains("dense_n100"), "hidden tests leaked");
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn run_then_submit_redacts_hidden_tests(db: PgPool) {
    let app = test_app(db);
    let (status, out) = call(
        &app,
        Method::POST,
        &format!("/api/problems/{NDT}/run"),
        Some(json!({ "code": SENTINEL_BUG })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(
        (out["run"]["passed"].as_u64(), out["run"]["total"].as_u64()),
        (Some(4), Some(5))
    );
    assert_eq!(out["attempt"]["started"], true);

    let (_, out) = call(
        &app,
        Method::POST,
        &format!("/api/problems/{NDT}/submit"),
        Some(json!({ "code": SENTINEL_BUG })),
    )
    .await;
    assert_eq!(
        (out["run"]["passed"].as_u64(), out["run"]["total"].as_u64()),
        (Some(9), Some(14))
    );
    let hidden = out["run"]["tests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["suite"] == "hidden" && t["outcome"] != "passed")
        .unwrap();
    assert_eq!(
        hidden["check"],
        json!({ "input": "withheld", "expected": "withheld", "got": "wrong answer" })
    );
    assert_eq!(hidden["panic"], Value::Null);
    assert_eq!(out["attempt"]["solved"], false);
    assert_eq!(out["solution"]["unlocked"], false);

    // Both runs are in the timeline, with the code, and the buffer was autosaved.
    let (_, p) = call(&app, Method::GET, &format!("/api/problems/{NDT}"), None).await;
    let kinds: Vec<&str> = p["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["run", "submit"]);
    assert_eq!(p["draft"], SENTINEL_BUG);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn passing_submit_solves_and_unlocks_the_solution(db: PgPool) {
    let app = test_app(db);
    let (_, before) = call(&app, Method::GET, &format!("/api/problems/{NDT}"), None).await;
    assert!(before["hidden_tests"].is_null(), "hidden tests stay hidden until solved");
    let (_, out) = call(
        &app,
        Method::POST,
        &format!("/api/problems/{NDT}/submit"),
        Some(json!({ "code": solution() })),
    )
    .await;
    assert_eq!(out["run"]["status"], "passed", "{out}");
    assert_eq!(
        (
            out["attempt"]["solved"].as_bool(),
            out["attempt"]["assisted"].as_bool()
        ),
        (Some(true), Some(false))
    );
    assert_eq!(out["solution"]["unlocked"], true);
    assert!(
        out["solution"]["code"]
            .as_str()
            .unwrap()
            .contains("then_some")
    );

    let (_, t) = call(&app, Method::GET, "/api/tracks/d9", None).await;
    let p = t["problems"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == NDT)
        .unwrap();
    assert_eq!(p["progress"], "solved");
    assert_eq!(t["solved"], 1);

    // Solved: the hidden test file is readable, and runs show hidden tests in full.
    let (_, after) = call(&app, Method::GET, &format!("/api/problems/{NDT}"), None).await;
    assert!(after["hidden_tests"].as_str().unwrap().contains("fn dense_n100"));
    let (_, bad) = call(&app, Method::POST, &format!("/api/problems/{NDT}/submit"), Some(json!({ "code": SENTINEL_BUG }))).await;
    let hidden = bad["run"]["tests"].as_array().unwrap().iter().find(|t| t["suite"] == "hidden").unwrap();
    assert_ne!(hidden["check"]["input"], "withheld", "{hidden}");
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn hints_reveal_one_at_a_time_and_mark_assisted(db: PgPool) {
    let app = test_app(db);
    let path = format!("/api/problems/{NDT}/hints");
    let (_, p) = call(&app, Method::POST, &path, None).await;
    assert_eq!(p["hints"]["revealed"].as_array().unwrap().len(), 1);
    assert_eq!(p["hints"]["revealed"][0]["kind"], "approach");
    assert_eq!(p["attempt"]["assisted"], true);
    for _ in 0..4 {
        call(&app, Method::POST, &path, None).await;
    }
    let (_, p) = call(&app, Method::GET, &format!("/api/problems/{NDT}"), None).await;
    assert_eq!(p["hints"]["revealed"].as_array().unwrap().len(), 3);
    assert_eq!(p["hints"]["locked"], json!([]));

    // An assisted solve still unlocks the solution, and stays assisted.
    let (_, out) = call(
        &app,
        Method::POST,
        &format!("/api/problems/{NDT}/submit"),
        Some(json!({ "code": solution() })),
    )
    .await;
    assert_eq!(
        (
            out["attempt"]["solved"].as_bool(),
            out["attempt"]["assisted"].as_bool()
        ),
        (Some(true), Some(true))
    );
    let (_, t) = call(&app, Method::GET, "/api/tracks/d9", None).await;
    assert_eq!(
        t["problems"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == NDT)
            .unwrap()["progress"],
        "assisted"
    );
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn reveal_anyway_unlocks_and_marks_assisted(db: PgPool) {
    let app = test_app(db);
    let (_, p) = call(
        &app,
        Method::POST,
        &format!("/api/problems/{NDT}/solution"),
        None,
    )
    .await;
    assert_eq!(p["solution"]["unlocked"], true);
    assert_eq!(p["solution"]["notes"]["time"], "O(E log E)");
    assert_eq!(p["attempt"]["assisted"], true);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn drafts_save_and_reset(db: PgPool) {
    let app = test_app(db);
    let (status, _) = call(
        &app,
        Method::PUT,
        &format!("/api/problems/{NDT}/draft"),
        Some(json!({ "code": "// wip" })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, p) = call(&app, Method::GET, &format!("/api/problems/{NDT}"), None).await;
    assert_eq!(p["draft"], "// wip");
    let (_, p) = call(
        &app,
        Method::DELETE,
        &format!("/api/problems/{NDT}/draft"),
        None,
    )
    .await;
    assert_eq!(p["draft"], Value::Null);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn draft_problems_and_unknown_ids_are_refused(db: PgPool) {
    // All shipped problems are ready, so use a one-problem catalog with a draft.
    let dir = tempfile::tempdir().unwrap();
    let track = dir.path().join("tracks/d99-fixture");
    std::fs::create_dir_all(track.join("problems/only-draft")).unwrap();
    std::fs::write(
        track.join("track.toml"),
        "code = \"D99\"\nname = \"Fixture\"\nsection = \"D\"\ntier = \"core\"\norder = 99\nsummary = \"x\"\n\n[[stages]]\nslug = \"s\"\nname = \"S\"\nband = \"easy\"\n",
    )
    .unwrap();
    std::fs::write(
        track.join("problems/only-draft/problem.toml"),
        "slug = \"only-draft\"\ntitle = \"Draft\"\nmode = \"write\"\nlevel = \"easy\"\nstage = \"s\"\norder = 1\nstatus = \"draft\"\ntags = []\n",
    )
    .unwrap();
    let app = test_app_with(db, dir.path());
    let (status, body) = call(
        &app,
        Method::POST,
        "/api/problems/d99-only-draft/run",
        Some(json!({ "code": "" })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "not_ready");
    let (status, _) = call(&app, Method::GET, "/api/problems/nope", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

const FIX: &str = "l2-two-mutable-borrows-of-self";

fn fix_file(name: &str) -> String {
    let dir = content_root().join("tracks/l2-borrowing/problems/two-mutable-borrows-of-self");
    std::fs::read_to_string(dir.join(name)).unwrap()
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn fix_this_rules_block_a_passing_clone_workaround(db: PgPool) {
    let app = test_app(db);
    // Collect the names by cloning them first: compiles, passes every test, breaks two rules.
    let clone = fix_file("starter.rs").replace(
        "        for item in self.items.iter_mut().filter(|i| i.qty < 5) {\n            item.qty += 10;\n            self.record(format!(\"restocked {}\", item.name));\n        }",
        "        let low: Vec<String> = self.items.iter().filter(|i| i.qty < 5).map(|i| i.name.clone()).collect();\n        for item in self.items.iter_mut().filter(|i| i.qty < 5) {\n            item.qty += 10;\n        }\n        for name in low {\n            self.record(format!(\"restocked {name}\"));\n        }",
    );
    assert_ne!(clone, fix_file("starter.rs"), "replacement didn't apply");
    let (status, out) = call(
        &app,
        Method::POST,
        &format!("/api/problems/{FIX}/submit"),
        Some(json!({ "code": clone })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["run"]["status"], "passed", "{out}");
    let rules: Vec<&str> = out["run"]["violations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["rule"].as_str().unwrap())
        .collect();
    assert_eq!(rules, ["clone", "lines"]);
    assert_eq!(out["run"]["violations"][0]["line"], 13);
    assert_eq!(out["attempt"]["solved"], false);
    assert_eq!(out["solution"]["unlocked"], false);

    // The one-line fix breaks no rule and solves it.
    let (_, out) = call(
        &app,
        Method::POST,
        &format!("/api/problems/{FIX}/submit"),
        Some(json!({ "code": fix_file("solution.rs") })),
    )
    .await;
    assert_eq!(out["run"]["violations"], json!([]));
    assert_eq!(out["attempt"]["solved"], true);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn activity_picks_the_next_problem_and_counts_the_streak(db: PgPool) {
    let app = test_app(db);
    let (status, a) = call(&app, Method::GET, "/api/activity?sections=D", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!((a["streak"].as_u64(), a["week"].as_array().map(Vec::len)), (Some(0), Some(7)));
    assert_eq!((a["next"]["reason"].as_str(), a["next"]["track_code"].as_str()), (Some("start"), Some("D9")));
    assert!(!a["next"]["excerpt"].as_str().unwrap().is_empty());

    let (_, out) = call(
        &app,
        Method::POST,
        &format!("/api/problems/{NDT}/submit"),
        Some(json!({ "code": solution() })),
    )
    .await;
    assert_eq!(out["run"]["status"], "passed");

    let (_, a) = call(&app, Method::GET, "/api/activity?sections=D", None).await;
    assert_eq!((a["streak"].as_u64(), a["week_solved"].as_u64(), a["week_unassisted"].as_u64()), (Some(1), Some(1), Some(1)));
    assert_eq!((a["recent"][0]["problem_id"].as_str(), a["recent"][0]["outcome"].as_str()), (Some(NDT), Some("solved")));
    // The solved problem's track becomes the current one; its first open problem is next.
    assert_eq!(
        (a["next"]["reason"].as_str(), a["next"]["problem_id"].as_str()),
        (Some("current"), Some(d9_ids()[0].as_str()))
    );

    // Other sections share the streak but not the recent list.
    let (_, r) = call(&app, Method::GET, "/api/activity?sections=L,S", None).await;
    assert_eq!((r["streak"].as_u64(), r["recent"].as_array().map(Vec::len)), (Some(1), Some(0)));

    let (_, tracks) = call(&app, Method::GET, "/api/tracks", None).await;
    let d9 = tracks.as_array().unwrap().iter().find(|t| t["code"] == "D9").unwrap();
    assert!(d9["readiness"].as_f64().unwrap() > 0.0);
}

/// A request with an optional cookie; returns status, the Set-Cookie value and the JSON body.
async fn raw(app: &Router, method: Method, path: &str, cookie: Option<&str>, body: Option<Value>) -> (StatusCode, Option<String>, Value) {
    let mut req = Request::builder().method(method).uri(path).header("content-type", "application/json");
    if let Some(c) = cookie {
        req = req.header("cookie", c);
    }
    let req = req.body(body.map_or_else(Body::empty, |b| Body::from(b.to_string()))).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let set_cookie = res.headers().get("set-cookie").map(|v| v.to_str().unwrap().to_owned());
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap() };
    (status, set_cookie, json)
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn passphrase_login_gates_the_api(db: PgPool) {
    use argon2::password_hash::{PasswordHasher, SaltString, rand_core::OsRng};
    let salt = SaltString::generate(&mut OsRng);
    let hash = argon2::Argon2::default().hash_password(b"open sesame", &salt).unwrap().to_string();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let app = test_app_auth(db, &root, AuthConfig::new(&hash, false).unwrap());

    assert_eq!(raw(&app, Method::GET, "/api/health", None, None).await.0, StatusCode::OK);
    let (status, _, body) = raw(&app, Method::GET, "/api/tracks", None, None).await;
    assert_eq!((status, body["error"].as_str()), (StatusCode::UNAUTHORIZED, Some("unauthorized")));
    let (_, _, s) = raw(&app, Method::GET, "/api/auth/session", None, None).await;
    assert_eq!(s, json!({ "required": true, "authenticated": false }));

    let (status, cookie, body) = raw(&app, Method::POST, "/api/auth/login", None, Some(json!({ "passphrase": "nope" }))).await;
    assert_eq!((status, cookie, body["error"].as_str()), (StatusCode::UNAUTHORIZED, None, Some("wrong_passphrase")));

    let (status, cookie, _) = raw(&app, Method::POST, "/api/auth/login", None, Some(json!({ "passphrase": "open sesame" }))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let cookie = cookie.unwrap();
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"), "{cookie}");
    let pair = cookie.split(';').next().unwrap().to_owned();

    // The routes that hand out or take in everything the owner has made are behind the login too.
    for (method, path) in [(Method::GET, "/api/export"), (Method::POST, "/api/import"), (Method::GET, "/api/feed"), (Method::POST, "/api/feed"), (Method::GET, "/api/week-summary"), (Method::GET, "/api/day/2026-10-29"), (Method::POST, "/api/plans/company"), (Method::GET, "/api/dsa/problems/lc-two-sum/forecast"), (Method::GET, "/api/prefs"), (Method::POST, "/api/prefs"), (Method::POST, "/api/dsa/problems/lc-two-sum/restore"), (Method::POST, "/api/dsa/problems/lc-two-sum/undo")] {
        assert_eq!(raw(&app, method, path, None, Some(json!({}))).await.0, StatusCode::UNAUTHORIZED, "{path} must need the login");
    }
    assert_eq!(raw(&app, Method::GET, "/api/tracks", Some(&pair), None).await.0, StatusCode::OK);
    let (_, _, s) = raw(&app, Method::GET, "/api/auth/session", Some(&pair), None).await;
    assert_eq!(s["authenticated"], true);
    assert_eq!(raw(&app, Method::GET, "/api/tracks", Some("anneal_session=forged"), None).await.0, StatusCode::UNAUTHORIZED);

    let (status, cleared, _) = raw(&app, Method::POST, "/api/auth/logout", Some(&pair), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(cleared.unwrap().contains("Max-Age=0"));
    assert_eq!(raw(&app, Method::GET, "/api/tracks", Some(&pair), None).await.0, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn editor_settings_round_trip_and_validate(db: PgPool) {
    let app = test_app(db);
    let (status, s) = call(&app, Method::GET, "/api/settings", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        s["editor"],
        json!({ "font_size": 13, "font_family": "Fira Code", "vim": false, "autocomplete": true, "rust_analyzer": true, "borrow_lanes": false, "live_clippy": true, "format_on_pause": false, "ligatures": false })
    );
    assert!(s["font_families"].as_array().unwrap().contains(&json!("Fira Code")));

    let e = json!({ "font_size": 16, "font_family": "Fira Code", "vim": true, "autocomplete": false, "rust_analyzer": false, "borrow_lanes": true, "live_clippy": false, "format_on_pause": true, "ligatures": true });
    assert_eq!(call(&app, Method::PUT, "/api/settings/editor", Some(e.clone())).await.0, StatusCode::OK);
    assert_eq!(call(&app, Method::GET, "/api/settings", None).await.1["editor"], e);

    // Settings saved before the workspace toggles existed read back with their defaults.
    let old = json!({ "font_size": 14, "font_family": "Fira Code", "vim": false });
    assert_eq!(call(&app, Method::PUT, "/api/settings/editor", Some(old)).await.0, StatusCode::OK);
    let editor = call(&app, Method::GET, "/api/settings", None).await.1["editor"].clone();
    assert_eq!((&editor["autocomplete"], &editor["rust_analyzer"], &editor["borrow_lanes"]), (&json!(true), &json!(true), &json!(false)));

    let too_big = json!({ "font_size": 40, "font_family": "Fira Code" });
    assert_eq!(call(&app, Method::PUT, "/api/settings/editor", Some(too_big)).await.0, StatusCode::BAD_REQUEST);
    let unknown = json!({ "font_size": 14, "font_family": "Comic Mono" });
    assert_eq!(call(&app, Method::PUT, "/api/settings/editor", Some(unknown)).await.0, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn appearance_accent_round_trips_and_validates(db: PgPool) {
    let app = test_app(db);
    let s = call(&app, Method::GET, "/api/settings", None).await.1;
    assert_eq!(s["appearance"], json!({ "accent": "copper" }));
    assert!(s["accents"].as_array().unwrap().contains(&json!("sky")));

    let a = json!({ "accent": "sky" });
    assert_eq!(call(&app, Method::PUT, "/api/settings/appearance", Some(a.clone())).await.0, StatusCode::OK);
    let s = call(&app, Method::GET, "/api/settings", None).await.1;
    assert_eq!(s["appearance"], a);
    // Saving one group leaves the other alone.
    assert_eq!(s["editor"]["font_size"], 13);

    let unknown = json!({ "accent": "chartreuse" });
    assert_eq!(call(&app, Method::PUT, "/api/settings/appearance", Some(unknown)).await.0, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn solving_schedules_reviews_and_feeds_the_dashboards(db: PgPool) {
    let app = test_app(db.clone());
    let submit = |app: &Router| {
        let app = app.clone();
        async move { call(&app, Method::POST, &format!("/api/problems/{NDT}/submit"), Some(json!({ "code": solution() }))).await }
    };

    // First, unassisted solve: graded "good", with its first review held on the consolidation day (a Sunday).
    call(&app, Method::POST, &format!("/api/problems/{NDT}/focus"), Some(json!({ "seconds": 90 }))).await;
    let (_, out) = submit(&app).await;
    assert_eq!(out["run"]["status"], "passed");
    let (status, r) = call(&app, Method::GET, "/api/reviews", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!((r["due_today"].as_u64(), r["in_rotation"].as_u64()), (Some(0), Some(1)));
    let (grade, reps, stability, due): (String, i32, f32, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as("SELECT last_grade, reps, stability, due_at FROM reviews WHERE problem_id = $1").bind(NDT).fetch_one(&db).await.unwrap();
    assert_eq!((grade.as_str(), reps), ("good", 1));
    assert!(stability > 1.0 && stability < 10.0, "a first good solve is remembered for days, not weeks: {stability}");
    assert_eq!(due.with_timezone(&chrono::Local).format("%a").to_string(), "Sun");
    assert!(due > chrono::Utc::now(), "the first review is in the future");
    let (_, stats) = call(&app, Method::GET, "/api/stats", None).await;
    assert_eq!((stats["first_run_pass"].as_f64(), stats["runs_per_solve"].as_f64()), (Some(100.0), Some(1.0)));

    // A re-solve starts a fresh attempt: hints locked, editor back to the starter.
    let (status, p) = call(&app, Method::POST, &format!("/api/problems/{NDT}/resolve"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!((p["attempt"]["resolve"].as_bool(), p["attempt"]["solved"].as_bool(), p["draft"].is_null()), (Some(true), Some(false), true));
    // The earlier solve still counts while the re-solve is open.
    let (_, tracks) = call(&app, Method::GET, "/api/tracks", None).await;
    let d9 = tracks.as_array().unwrap().iter().find(|t| t["code"] == "D9").unwrap();
    assert_eq!(d9["solved"], 1);

    // A re-solve that needed a hint is graded "hard": the problem is scheduled again, with less gained than a clean one.
    call(&app, Method::POST, &format!("/api/problems/{NDT}/hints"), None).await;
    submit(&app).await;
    let (grade, reps, history): (String, i32, serde_json::Value) =
        sqlx::query_as("SELECT last_grade, reps, to_jsonb(history) FROM reviews WHERE problem_id = $1").bind(NDT).fetch_one(&db).await.unwrap();
    assert_eq!((grade.as_str(), reps, history.as_array().unwrap().len()), ("hard", 2, 2));
    assert_eq!(history[1]["grade"], "hard");
    assert!(history[1]["recall_before"].as_f64().unwrap() > 0.5, "the history keeps the recall estimate at the time");
    let (_, r) = call(&app, Method::GET, "/api/reviews", None).await;
    assert_eq!(r["retention_30d"].as_f64(), Some(0.0));
    let (_, o) = call(&app, Method::GET, "/api/progress", None).await;
    assert_eq!((o["streak"].as_u64(), o["solved_year"].as_u64()), (Some(1), Some(2)));
    assert_eq!(o["heat"].as_array().unwrap().last().unwrap(), 2);
    assert_eq!(o["this_week"].as_array().unwrap().iter().map(|d| d["focus_seconds"].as_i64().unwrap()).sum::<i64>(), 90);
    assert_eq!(o["weekly"].as_array().unwrap().last().unwrap()["medium"], 2);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn run_builds_and_runs_the_scratch_main(db: PgPool) {
    let app = test_app(db);
    let (_, p) = call(&app, Method::GET, &format!("/api/problems/{NDT}"), None).await;
    assert!(p["scratch"].as_str().unwrap().contains("use solution::*;"));

    let main = "use solution::*;\nfn main() { println!(\"{:?}\", network_delay(&[(1, 2, 5)], 2, 1)); }\n";
    let (status, out) = call(&app, Method::POST, &format!("/api/problems/{NDT}/scratch/run"), Some(json!({ "lib": solution(), "main": main }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!((out["status"].as_str(), out["stdout"].as_str()), (Some("ok"), Some("Some(5)\n")), "{out}");

    // Both buffers were saved; Run isn't recorded as a test run.
    let (_, p) = call(&app, Method::GET, &format!("/api/problems/{NDT}"), None).await;
    assert_eq!((p["scratch"].as_str(), p["runs"].as_array().map(Vec::len)), (Some(main), Some(0)));
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn built_app_is_served_with_cache_headers(db: PgPool) {
    let dist = tempfile::tempdir().unwrap();
    std::fs::create_dir(dist.path().join("assets")).unwrap();
    std::fs::write(dist.path().join("index.html"), "<!doctype html>").unwrap();
    std::fs::write(dist.path().join("assets/index-abc123.js"), "console.log(1)").unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let app = test_app_full(db, &root.join("content"), AuthConfig::disabled(), Some(dist.path()));
    let get = |path: &str| {
        let app = app.clone();
        let req = Request::builder().uri(path).body(Body::empty()).unwrap();
        async move { app.oneshot(req).await.unwrap() }
    };
    let cache = |res: &axum::response::Response| res.headers().get("cache-control").map(|v| v.to_str().unwrap().to_owned());

    // Hashed assets never change.
    let asset = get("/assets/index-abc123.js").await;
    assert_eq!(asset.status(), StatusCode::OK);
    assert_eq!(cache(&asset).as_deref(), Some("public, max-age=31536000, immutable"));
    // A missing asset is a real 404, uncached, not the SPA's index.html.
    let missing = get("/assets/index-gone.js").await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(cache(&missing), None);
    // Pages (client-side routes) get index.html and must be revalidated.
    for path in ["/", "/rust", "/p/d1-running-sum"] {
        let page = get(path).await;
        assert_eq!(page.status(), StatusCode::OK, "{path}");
        assert_eq!(cache(&page).as_deref(), Some("no-cache"), "{path}");
    }
    // The API is untouched.
    let api = get("/api/health").await;
    assert_eq!(api.status(), StatusCode::OK);
    assert_eq!(cache(&api), None);
}

/// A one-track catalog whose only problem, d99-new-name, used to be d99-old-name.
fn renamed_catalog() -> (tempfile::TempDir, anneal_content::Catalog) {
    let dir = tempfile::tempdir().unwrap();
    let track = dir.path().join("tracks/d99-fixture");
    std::fs::create_dir_all(track.join("problems/new-name")).unwrap();
    std::fs::write(
        track.join("track.toml"),
        "code = \"D99\"\nname = \"Fixture\"\nsection = \"D\"\ntier = \"core\"\norder = 99\nsummary = \"x\"\n\n[[stages]]\nslug = \"s\"\nname = \"S\"\nband = \"easy\"\n",
    )
    .unwrap();
    std::fs::write(
        track.join("problems/new-name/problem.toml"),
        "slug = \"new-name\"\ntitle = \"Renamed\"\nmode = \"write\"\nlevel = \"easy\"\nstage = \"s\"\norder = 1\nstatus = \"draft\"\ntags = []\nrenamed_from = [\"d99-old-name\"]\n",
    )
    .unwrap();
    let loaded = Catalog::load(dir.path()).unwrap();
    assert!(loaded.issues.is_empty(), "{:?}", loaded.issues);
    (dir, loaded.catalog)
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn renaming_a_problem_keeps_its_progress(db: PgPool) {
    let (_dir, catalog) = renamed_catalog();
    // Progress under the old id: a solved attempt with a run, a draft, a review, focus time and scratch.
    let attempt: i64 = sqlx::query_scalar(
        "INSERT INTO attempts (problem_id, solved_at) VALUES ('d99-old-name', now()) RETURNING id",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO runs (attempt_id, problem_id, kind, code, status, passed, total, result) VALUES ($1, 'd99-old-name', 'submit', 'x', 'passed', 1, 1, '{}')")
        .bind(attempt)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO drafts (problem_id, code) VALUES ('d99-old-name', 'old draft')").execute(&db).await.unwrap();
    sqlx::query("INSERT INTO reviews (problem_id, step, due_at, last_result, stability, difficulty, last_review, last_grade) VALUES ('d99-old-name', 2, now(), 'unassisted', 20, 5, now(), 'good')").execute(&db).await.unwrap();
    sqlx::query("INSERT INTO scratch (problem_id, code) VALUES ('d99-old-name', 'fn main() {}')").execute(&db).await.unwrap();
    // Focus time on the same day under both ids is added up.
    sqlx::query("INSERT INTO focus_time (day, problem_id, seconds) VALUES (current_date, 'd99-old-name', 60), (current_date, 'd99-new-name', 30)")
        .execute(&db)
        .await
        .unwrap();

    // Before the move, preflight knows the old id through renamed_from, so the deploy may go ahead.
    let report = anneal_api::preflight::check(&db, &catalog, &anneal_api::MIGRATOR).await.unwrap();
    assert!(report.is_ok(), "{report:?}");

    anneal_api::preflight::apply_renames(&db, &catalog).await.unwrap();
    let count = |table: &'static str, id: &'static str| {
        let db = db.clone();
        async move {
            sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table} WHERE problem_id = $1"))
                .bind(id)
                .fetch_one(&db)
                .await
                .unwrap()
        }
    };
    for table in ["attempts", "runs", "drafts", "reviews", "scratch", "focus_time"] {
        assert_eq!(count(table, "d99-old-name").await, 0, "{table} still has the old id");
        assert_eq!(count(table, "d99-new-name").await, 1, "{table} lost the row");
    }
    let seconds: i32 = sqlx::query_scalar("SELECT seconds FROM focus_time WHERE problem_id = 'd99-new-name'")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(seconds, 90);
    // Running it again changes nothing.
    assert_eq!(anneal_api::preflight::apply_renames(&db, &catalog).await.unwrap(), 0);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn preflight_refuses_to_strand_progress(db: PgPool) {
    let (_dir, catalog) = renamed_catalog();
    sqlx::query("INSERT INTO attempts (problem_id) VALUES ('d99-deleted-without-a-trace')").execute(&db).await.unwrap();
    let report = anneal_api::preflight::check(&db, &catalog, &anneal_api::MIGRATOR).await.unwrap();
    assert_eq!(report.stranded, vec![("d99-deleted-without-a-trace".to_owned(), 1)]);
    assert!(report.migrations.is_empty());

    // An applied migration whose file changed is caught too.
    sqlx::query("UPDATE _sqlx_migrations SET checksum = '\\x00' WHERE version = 1").execute(&db).await.unwrap();
    let report = anneal_api::preflight::check(&db, &catalog, &anneal_api::MIGRATOR).await.unwrap();
    assert_eq!(report.migrations.len(), 1, "{:?}", report.migrations);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn format_runs_rustfmt_and_reports_parse_errors(db: PgPool) {
    let app = test_app(db);
    let (status, body) = call(&app, Method::POST, "/api/format", Some(json!({ "code": "pub fn  add(a:i32,b:i32)->i32{a+b}" }))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["code"], "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n");
    let (status, body) = call(&app, Method::POST, "/api/format", Some(json!({ "code": "pub fn broken( {" }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["message"].as_str().unwrap().starts_with("error"), "{body}");
}

#[test]
fn editor_settings_default_to_live_clippy_without_format_on_pause() {
    // Settings saved before these fields existed still load, with live clippy on.
    let old: anneal_api::settings::EditorSettings =
        serde_json::from_value(json!({ "font_size": 14, "font_family": "Fira Code", "vim": true })).unwrap();
    assert!(old.live_clippy);
    assert!(!old.format_on_pause);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn review_settings_default_to_the_routine_and_validate(db: PgPool) {
    let app = test_app(db);
    let s = call(&app, Method::GET, "/api/settings", None).await.1;
    assert_eq!(s["srs"]["retention"], 0.85);
    assert_eq!(s["srs"]["consolidate_on"], "sun");
    assert_eq!((s["srs"]["capacity"]["mon"].as_u64(), s["srs"]["capacity"]["sat"].as_u64(), s["srs"]["capacity"]["sun"].as_u64()), (Some(1), Some(3), Some(12)));

    let mut v = s["srs"].clone();
    v["capacity"]["sun"] = json!(8);
    v["retention"] = json!(0.8);
    assert_eq!(call(&app, Method::PUT, "/api/settings/srs", Some(v)).await.0, StatusCode::OK);
    let s = call(&app, Method::GET, "/api/settings", None).await.1;
    assert_eq!((s["srs"]["capacity"]["sun"].as_u64(), s["srs"]["retention"].as_f64()), (Some(8), Some(0.8)));

    // Importance ordering is on and Premium is left out by default; both, and the core retention, can be changed.
    assert_eq!((s["srs"]["prioritise"].as_bool(), s["srs"]["core_retention"].is_null(), s["srs"]["goal"]["free_only"].as_bool()), (Some(true), true, Some(true)));
    let mut v = s["srs"].clone();
    v["prioritise"] = json!(false);
    v["core_retention"] = json!(0.92);
    v["goal"]["free_only"] = json!(false);
    assert_eq!(call(&app, Method::PUT, "/api/settings/srs", Some(v)).await.0, StatusCode::OK);
    let s = call(&app, Method::GET, "/api/settings", None).await.1;
    assert_eq!((s["srs"]["prioritise"].as_bool(), s["srs"]["core_retention"].as_f64(), s["srs"]["goal"]["free_only"].as_bool()), (Some(false), Some(0.92), Some(false)));

    for bad in [json!({ "core_retention": 0.5 }), json!({ "retention": 0.9, "core_retention": 0.99 }), json!({ "retention": 0.4 }), json!({ "consolidate_on": "funday" }), json!({ "capacity": { "mon": 0, "tue": 0, "wed": 0, "thu": 0, "fri": 0, "sat": 0, "sun": 0 }, "consolidate_on": null })] {
        assert_eq!(call(&app, Method::PUT, "/api/settings/srs", Some(bad)).await.0, StatusCode::BAD_REQUEST);
    }
}

/// A content root with only the DSA fixture (and optionally a retired list), so its tracks are D1 and D2.
fn dsa_root(retired: Option<&str>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("tracks")).unwrap();
    copy_dir(&fixtures().join("dsa-root/dsa"), &dir.path().join("dsa"));
    copy_dir(&fixtures().join("dsa-root/tracks"), &dir.path().join("tracks"));
    if let Some(retired) = retired {
        std::fs::write(dir.path().join("retired.txt"), retired).unwrap();
    }
    dir
}

fn problem_of<'a>(overview: &'a Value, id: &str) -> &'a Value {
    overview["problems"].as_array().unwrap().iter().find(|p| p["id"] == id).unwrap_or_else(|| panic!("{id} missing"))
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn dsa_overview_lists_the_lists_with_their_techniques(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db, root.path());
    let (status, o) = call(&app, Method::GET, "/api/dsa", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(o["patterns"].as_array().unwrap().iter().map(|p| (p["code"].as_str().unwrap(), p["total"].as_u64().unwrap())).collect::<Vec<_>>(), [("D1", 2), ("D2", 3)]);
    // (D1 holds a premium problem: listed below, but not in the pattern total unless the owner chose to count premium)
    assert_eq!(o["problems"].as_array().unwrap().len(), 6);
    assert_eq!(o["techniques"].as_array().unwrap().len(), 4);
    let two_sum_ii = problem_of(&o, "lc-two-sum-ii-input-array-is-sorted");
    assert_eq!((two_sum_ii["role"].as_str(), two_sum_ii["practice_of"].as_str(), two_sum_ii["pattern"].as_str()), (Some("practice"), Some("lc-valid-palindrome"), Some("D2")));
    assert_eq!(problem_of(&o, "lc-encode-and-decode-strings")["premium"], true);
    assert_eq!(problem_of(&o, "lc-contains-duplicate")["companies"][0]["name"], "Amazon");
    // Nothing logged yet.
    assert_eq!((problem_of(&o, "lc-two-sum")["state"]["solved"].clone(), problem_of(&o, "lc-two-sum")["state"]["last_grade"].clone()), (json!(false), Value::Null));
    // The goal is the NeetCode 150 without its Premium problems: 4 of the 5 in the fixture's 150.
    assert_eq!((o["plan"]["goal_total"].as_u64(), o["plan"]["goal_done"].as_u64()), (Some(4), Some(0)));
    assert_eq!(o["plan"]["next_up"][0], "lc-contains-duplicate");
    assert_eq!(o["settings"]["new_days"], json!(["mon", "tue", "wed", "thu", "fri", "sat"]));
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn logging_a_dsa_problem_feeds_progress_reviews_and_next_up(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db.clone(), root.path());

    let (status, out) = call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/log", Some(json!({ "grade": "good" }))).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert!(out["due"].is_string());
    let (_, o) = call(&app, Method::GET, "/api/dsa", None).await;
    let two_sum = problem_of(&o, "lc-two-sum");
    assert_eq!((two_sum["state"]["solved"].clone(), two_sum["state"]["assisted"].clone(), two_sum["state"]["last_grade"].clone(), two_sum["state"]["reps"].clone()), (json!(true), json!(false), json!("good"), json!(1)));
    assert!(two_sum["state"]["due"].is_string());
    assert_eq!((o["plan"]["goal_done"].as_u64(), o["patterns"][0]["solved"].as_u64()), (Some(1), Some(1)));
    // The plan walks on from the problem just logged, skipping the done one and the Premium one outside the goal. What
    // was left above it in the same topic comes next, before the next topic, so a topic is finished before moving on.
    // lc-contains-duplicate was next and lc-two-sum is in the same topic, so it stays next and the start doesn't move.
    assert_eq!(o["plan"]["start"], Value::Null);
    assert_eq!(o["plan"]["next_up"], json!(["lc-contains-duplicate", "lc-valid-palindrome", "lc-two-sum-ii-input-array-is-sorted"]));

    // A solve through the log counts toward the streak and the Progress pages like any other.
    let (_, a) = call(&app, Method::GET, "/api/activity?sections=D", None).await;
    assert_eq!((a["streak"].as_u64(), a["week_solved"].as_u64(), a["recent"][0]["problem_id"].as_str()), (Some(1), Some(1), Some("lc-two-sum")));
    let (_, r) = call(&app, Method::GET, "/api/reviews", None).await;
    assert_eq!(r["in_rotation"], 1);

    // Needing help is "hard"; failing is "again" and doesn't count as solved.
    call(&app, Method::POST, "/api/dsa/problems/lc-valid-palindrome/log", Some(json!({ "grade": "hard" }))).await;
    call(&app, Method::POST, "/api/dsa/problems/lc-contains-duplicate/log", Some(json!({ "grade": "again" }))).await;
    let (_, o) = call(&app, Method::GET, "/api/dsa", None).await;
    assert_eq!((problem_of(&o, "lc-valid-palindrome")["state"]["solved"].clone(), problem_of(&o, "lc-valid-palindrome")["state"]["assisted"].clone()), (json!(true), json!(true)));
    let again = problem_of(&o, "lc-contains-duplicate");
    assert_eq!((again["state"]["solved"].clone(), again["state"]["last_grade"].clone(), again["state"]["lapses"].clone()), (json!(false), json!("again"), json!(1)));
    assert_eq!(o["plan"]["goal_done"], 2);

    // Logging it again is a re-solve, and it can improve the grade.
    call(&app, Method::POST, "/api/dsa/problems/lc-contains-duplicate/log", Some(json!({ "grade": "easy" }))).await;
    let kinds: Vec<String> = sqlx::query_scalar("SELECT kind FROM attempts WHERE problem_id = 'lc-contains-duplicate' ORDER BY id").fetch_all(&db).await.unwrap();
    assert_eq!(kinds, ["practice", "resolve"]);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn written_lessons_are_served_per_problem(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db, root.path());
    let (_, o) = call(&app, Method::GET, "/api/dsa", None).await;
    assert_eq!((problem_of(&o, "lc-two-sum")["has_page"].clone(), problem_of(&o, "lc-valid-palindrome")["has_page"].clone()), (json!(true), json!(false)));
    let (status, page) = call(&app, Method::GET, "/api/dsa/problems/lc-two-sum/page", None).await;
    assert_eq!((status, page["approaches"][0]["name"].as_str(), page["tips"][0].as_str()), (StatusCode::OK, Some("One-pass hash map"), Some("Check before inserting.")));
    let (status, none) = call(&app, Method::GET, "/api/dsa/problems/lc-valid-palindrome/page", None).await;
    assert_eq!((status, none), (StatusCode::OK, Value::Null));
    assert_eq!(call(&app, Method::GET, "/api/dsa/problems/lc-nothing/page", None).await.0, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn dsa_start_moves_where_the_next_problem_comes_from(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db, root.path());
    let (status, out) = call(&app, Method::POST, "/api/dsa/start", Some(json!({ "from": "d2-two-pointers" }))).await;
    assert_eq!((status, out["start"].as_str()), (StatusCode::OK, Some("lc-valid-palindrome")));
    let (_, o) = call(&app, Method::GET, "/api/dsa", None).await;
    // From Two Pointers down, then back round to the top; the Premium problem is outside the goal.
    assert_eq!(o["plan"]["next_up"], json!(["lc-valid-palindrome", "lc-two-sum-ii-input-array-is-sorted", "lc-contains-duplicate", "lc-two-sum"]));

    let (status, _) = call(&app, Method::POST, "/api/dsa/start", Some(json!({ "from": "lc-two-sum" }))).await;
    assert_eq!(status, StatusCode::OK);
    let (_, o) = call(&app, Method::GET, "/api/dsa", None).await;
    assert_eq!(o["plan"]["next_up"][0], "lc-two-sum");
    assert_eq!(call(&app, Method::POST, "/api/dsa/start", Some(json!({ "from": "nowhere" }))).await.0, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn only_dsa_problems_can_be_logged(db: PgPool) {
    let app = test_app(db);
    let (status, out) = call(&app, Method::POST, &format!("/api/dsa/problems/{NDT}/log"), Some(json!({ "grade": "good" }))).await;
    assert_eq!((status, out["error"].as_str()), (StatusCode::BAD_REQUEST, Some("bad_request")));
    assert_eq!(call(&app, Method::POST, "/api/dsa/problems/lc-nothing/log", Some(json!({ "grade": "good" }))).await.0, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn the_plan_follows_the_settings(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db, root.path());
    let (_, o) = call(&app, Method::GET, "/api/dsa", None).await;
    let mut settings = o["settings"].clone();
    // Solve only on Mondays and Wednesdays, two a day, with the goal widened to all lists.
    settings["new_days"] = json!(["mon", "wed"]);
    settings["new_per_day"] = json!(2);
    settings["goal"] = json!({ "list": "all", "free_only": false, "extra": 0, "custom_left": null });
    let (status, _) = call(&app, Method::PUT, "/api/settings/srs", Some(settings.clone())).await;
    assert_eq!(status, StatusCode::OK);
    let (_, o) = call(&app, Method::GET, "/api/dsa", None).await;
    assert_eq!(o["plan"]["goal_total"], 6);
    assert_eq!(o["settings"]["new_days"], json!(["mon", "wed"]));

    // A weekday can take its own number of new problems; 0 keeps the usual two, and more than 20 is refused.
    settings["new_by_day"] = json!({ "wed": 4 });
    assert_eq!(call(&app, Method::PUT, "/api/settings/srs", Some(settings.clone())).await.0, StatusCode::OK);
    let (_, o) = call(&app, Method::GET, "/api/dsa", None).await;
    assert_eq!((o["settings"]["new_by_day"]["wed"].as_u64(), o["settings"]["new_by_day"]["mon"].as_u64()), (Some(4), Some(0)));
    settings["new_by_day"] = json!({ "wed": 21 });
    assert_eq!(call(&app, Method::PUT, "/api/settings/srs", Some(settings.clone())).await.0, StatusCode::BAD_REQUEST);
    settings["new_by_day"] = json!({});

    settings["goal"]["list"] = json!("everything");
    assert_eq!(call(&app, Method::PUT, "/api/settings/srs", Some(settings.clone())).await.0, StatusCode::BAD_REQUEST);
    settings["goal"]["list"] = json!("all");
    settings["new_days"] = json!(["mon", "mon"]);
    assert_eq!(call(&app, Method::PUT, "/api/settings/srs", Some(settings)).await.0, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn retired_problems_lose_their_progress_without_stranding_anything(db: PgPool) {
    let root = dsa_root(Some("# removed on purpose\nd99-gone # the old one\n"));
    let loaded = Catalog::load(root.path()).unwrap();
    assert!(loaded.issues.is_empty(), "{:?}", loaded.issues);
    sqlx::query("INSERT INTO attempts (problem_id, solved_at) VALUES ('d99-gone', now()), ('lc-two-sum', now())").execute(&db).await.unwrap();
    sqlx::query("INSERT INTO focus_time (day, problem_id, seconds) VALUES (current_date, 'd99-gone', 60)").execute(&db).await.unwrap();
    // A retired id isn't "stranded": the deploy may go ahead.
    let report = anneal_api::preflight::check(&db, &loaded.catalog, &anneal_api::MIGRATOR).await.unwrap();
    assert!(report.is_ok(), "{report:?}");

    let gone = anneal_api::preflight::purge_retired(&db, &loaded.catalog).await.unwrap();
    assert_eq!(gone, 2);
    let left: Vec<String> = sqlx::query_scalar("SELECT problem_id FROM attempts").fetch_all(&db).await.unwrap();
    assert_eq!(left, ["lc-two-sum"], "only the retired problem's progress goes");
    assert_eq!(anneal_api::preflight::purge_retired(&db, &loaded.catalog).await.unwrap(), 0, "idempotent");
}

#[test]
fn a_current_problem_cannot_also_be_retired() {
    let root = dsa_root(Some("lc-two-sum\n"));
    let loaded = Catalog::load(root.path()).unwrap();
    assert!(loaded.issues.iter().any(|i| i.message.contains("lc-two-sum is retired but also a current problem")), "{:?}", loaded.issues);
}

fn practice_solution() -> String {
    std::fs::read_to_string(fixtures().join("dsa-root/tracks/p1-two-pointers-practice/problems/sorted-pair-sum/solution.py")).unwrap()
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn practice_problems_unlock_by_logging_or_by_being_a_warmup_for_what_is_next(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db.clone(), root.path());
    let (status, tracks) = call(&app, Method::GET, "/api/dsa/handwritten/D2", None).await;
    assert_eq!(status, StatusCode::OK);
    let problems = &tracks[0]["problems"];
    assert_eq!((tracks[0]["language"].as_str(), problems.as_array().unwrap().len()), (Some("python"), 2));
    // The warm-up opens because Valid Palindrome is among the next three; the other waits to be logged.
    let by_id = |id: &str| problems.as_array().unwrap().iter().find(|p| p["id"] == id).unwrap().clone();
    assert_eq!((by_id("p1-sorted-pair-sum")["open"].clone(), by_id("p1-mirror-check")["open"].clone()), (json!(true), json!(false)));
    assert_eq!(by_id("p1-mirror-check")["unlocked_by"][0]["title"], "Two Sum II - Input Array Is Sorted");
    assert!(by_id("p1-sorted-pair-sum")["blurb"].as_str().unwrap().starts_with("Given a **sorted** list"));

    let (status, locked) = call(&app, Method::GET, "/api/problems/p1-mirror-check", None).await;
    assert_eq!((status, locked["error"].as_str()), (StatusCode::LOCKED, Some("locked")));
    assert!(locked["message"].as_str().unwrap().contains("Two Sum II"), "{locked}");
    let (status, _) = call(&app, Method::POST, "/api/problems/p1-mirror-check/submit", Some(json!({ "code": "x = 1" }))).await;
    assert_eq!(status, StatusCode::LOCKED);

    // Logging the LeetCode problem, even as "not yet", opens it.
    call(&app, Method::POST, "/api/dsa/problems/lc-two-sum-ii-input-array-is-sorted/log", Some(json!({ "grade": "again" }))).await;
    let (status, p) = call(&app, Method::GET, "/api/problems/p1-mirror-check", None).await;
    assert_eq!((status, p["language"].as_str(), p["starter"].as_str()), (StatusCode::OK, Some("python"), Some("def is_mirror(items: list[int]) -> bool:\n    ...\n")));
    assert_eq!(p["unlocked_by"][0]["slug"], "two-sum-ii-input-array-is-sorted");
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn python_practice_runs_tests_and_never_schedules_reviews(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db.clone(), root.path());
    let id = "p1-sorted-pair-sum";

    // A broken program is a compile error with a span in solution.py; the editor reads the file name.
    let (_, out) = call(&app, Method::POST, &format!("/api/problems/{id}/run"), Some(json!({ "code": "def pair_sum(nums, target):\nreturn None\n" }))).await;
    assert_eq!(out["run"]["status"], "compile_error");
    assert_eq!((out["run"]["diagnostics"][0]["spans"][0]["file"].as_str(), out["run"]["diagnostics"][0]["spans"][0]["line_start"].as_u64()), (Some("solution.py"), Some(2)));

    // The starter fails its tests; Run shows visible ones and a check mismatch.
    let (_, p) = call(&app, Method::GET, &format!("/api/problems/{id}"), None).await;
    let (_, out) = call(&app, Method::POST, &format!("/api/problems/{id}/run"), Some(json!({ "code": p["starter"] }))).await;
    assert_eq!((out["run"]["status"].as_str(), out["run"]["total"].as_u64()), (Some("failed"), Some(5)));

    // The solution solves it on Submit; hidden tests are now revealed, and no review is scheduled.
    let (_, out) = call(&app, Method::POST, &format!("/api/problems/{id}/submit"), Some(json!({ "code": practice_solution() }))).await;
    assert_eq!((out["run"]["status"].as_str(), out["run"]["total"].as_u64(), out["attempt"]["solved"].clone()), (Some("passed"), Some(13), json!(true)));
    let reviews: i64 = sqlx::query_scalar("SELECT count(*) FROM reviews").fetch_one(&db).await.unwrap();
    assert_eq!(reviews, 0, "practice schedules no reviews");
    let (_, r) = call(&app, Method::GET, "/api/reviews", None).await;
    assert_eq!((r["in_rotation"].as_u64(), r["due_today"].as_u64()), (Some(0), Some(0)));
    let (_, ov) = call(&app, Method::GET, "/api/dsa", None).await;
    // No readiness from it either: the DSA lists are untouched.
    assert_eq!(ov["plan"]["goal_done"], 0);
    // There is no rust-analyzer for Python.
    let (status, _) = call(&app, Method::POST, &format!("/api/problems/{id}/scratch/run"), Some(json!({ "lib": "", "main": "" }))).await;
    assert_ne!(status, StatusCode::OK);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn leetcode_practice_is_grouped_by_technique_and_schedules_no_reviews(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db.clone(), root.path());

    // Two Pointers is the second pattern (D2) and has one technique with two practice problems.
    let (status, p) = call(&app, Method::GET, "/api/dsa/practice/D2", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!((p["pattern"].as_str(), p["techniques"].as_array().unwrap().len()), (Some("Two Pointers"), 1));
    let t = &p["techniques"][0];
    assert_eq!((t["id"].as_str(), t["must_learn"]["title"].as_str(), t["must_learn"]["solved"].clone(), t["solved"].clone()), (Some("Two Pointers:opposite"), Some("Valid Palindrome"), json!(false), json!(0)));
    let titles: Vec<&str> = t["problems"].as_array().unwrap().iter().map(|x| x["title"].as_str().unwrap()).collect();
    assert_eq!(titles, ["Valid Palindrome II", "3Sum Closest"]);
    assert_eq!(t["problems"][0]["companies"][0]["name"], "Meta");

    // The extras are not part of the NeetCode lists or the pattern counts the home uses.
    let (_, ov) = call(&app, Method::GET, "/api/dsa", None).await;
    assert_eq!(ov["problems"].as_array().unwrap().len(), 6);
    assert_eq!((ov["patterns"][1]["total"].clone(), ov["patterns"][1]["practice_total"].clone(), ov["patterns"][1]["practice_solved"].clone()), (json!(3), json!(2), json!(0)));
    assert_eq!(ov["plan"]["goal_total"], 4);

    // Logging a practice problem records the attempt, schedules no review, and doesn't move the plan.
    let (status, out) = call(&app, Method::POST, "/api/dsa/problems/lc-valid-palindrome-ii/log", Some(json!({ "grade": "good" }))).await;
    assert_eq!((status, out["due"].clone(), out["ideal_days"].clone()), (StatusCode::OK, Value::Null, Value::Null));
    let reviews: i64 = sqlx::query_scalar("SELECT count(*) FROM reviews").fetch_one(&db).await.unwrap();
    assert_eq!(reviews, 0, "practice schedules no reviews");
    call(&app, Method::POST, "/api/dsa/problems/lc-3sum-closest/log", Some(json!({ "grade": "again" }))).await;
    let (_, p) = call(&app, Method::GET, "/api/dsa/practice/D2", None).await;
    let rows = p["techniques"][0]["problems"].as_array().unwrap();
    assert_eq!((rows[0]["state"]["last_grade"].clone(), rows[0]["state"]["solved"].clone(), rows[1]["state"]["last_grade"].clone(), rows[1]["state"]["solved"].clone()), (json!("good"), json!(true), json!("again"), json!(false)));
    assert_eq!(p["techniques"][0]["solved"], 1);
    let (_, ov) = call(&app, Method::GET, "/api/dsa", None).await;
    assert_eq!((ov["patterns"][1]["practice_solved"].clone(), ov["plan"]["goal_done"].clone(), ov["plan"]["start"].clone()), (json!(1), json!(0), Value::Null));
    // They count in activity like any solve.
    let (_, a) = call(&app, Method::GET, "/api/activity?sections=D", None).await;
    assert_eq!((a["streak"].as_u64(), a["recent"][0]["problem_id"].as_str()), (Some(1), Some("lc-3sum-closest")));
    let (_, r) = call(&app, Method::GET, "/api/reviews", None).await;
    assert_eq!(r["in_rotation"], 0);
}

#[test]
fn practice_problems_must_be_free_known_and_not_already_listed() {
    let dir = dsa_root(None);
    let good = std::fs::read_to_string(dir.path().join("dsa/practice.json")).unwrap();
    let bad = good
        .replace(r#""id": "lc-3sum-closest""#, r#""id": "lc-two-sum""#)
        .replace(r#""technique": "Two Pointers:opposite",
   "order": 100001"#, r#""technique": "Nope:technique",
   "order": 100001"#);
    std::fs::write(dir.path().join("dsa/practice.json"), bad).unwrap();
    let issues: Vec<String> = Catalog::load(dir.path()).unwrap().issues.into_iter().map(|i| i.message).collect();
    assert!(issues.iter().any(|m| m.contains("lc-two-sum: is already in the NeetCode lists")), "{issues:?}");
    assert!(issues.iter().any(|m| m.contains("unknown technique Nope:technique")), "{issues:?}");
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn the_mock_page_gets_every_problem_and_keeps_its_settings_and_history(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db.clone(), root.path());
    let (status, m) = call(&app, Method::GET, "/api/dsa/mock", None).await;
    assert_eq!(status, StatusCode::OK, "{m}");
    // The six NeetCode problems and the three practice ones, each with how it stands.
    let ids: Vec<&str> = m["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), 9, "{ids:?}");
    for practice in [
        "lc-contains-duplicate-ii",
        "lc-valid-palindrome-ii",
        "lc-3sum-closest",
    ] {
        assert!(ids.contains(&practice), "{practice} missing");
    }
    assert_eq!(
        (
            m["patterns"].as_array().unwrap().len(),
            m["saved"].clone(),
            m["rounds"].as_array().unwrap().len()
        ),
        (2, Value::Null, 0)
    );

    // A practice problem logged ✓ stands as solved; a NeetCode one logged ✗ stands as attempted.
    call(
        &app,
        Method::POST,
        "/api/dsa/problems/lc-3sum-closest/log",
        Some(json!({ "grade": "good" })),
    )
    .await;
    call(
        &app,
        Method::POST,
        "/api/dsa/problems/lc-two-sum/log",
        Some(json!({ "grade": "again" })),
    )
    .await;
    let (_, m) = call(&app, Method::GET, "/api/dsa/mock", None).await;
    let by = |id: &str| {
        m["problems"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(
        (
            by("lc-3sum-closest")["state"]["solved"].clone(),
            by("lc-3sum-closest")["state"]["last_grade"].clone()
        ),
        (json!(true), json!("good"))
    );
    assert_eq!(
        (
            by("lc-two-sum")["state"]["solved"].clone(),
            by("lc-two-sum")["state"]["last_grade"].clone()
        ),
        (json!(false), json!("again"))
    );

    // The settings come back as they were saved, and only an object of reasonable size is accepted.
    let saved = json!({ "last": { "lists": ["neetcode150"], "total": 45 }, "presets": [] });
    assert_eq!(
        call(
            &app,
            Method::PUT,
            "/api/dsa/mock/config",
            Some(saved.clone())
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &app,
            Method::PUT,
            "/api/dsa/mock/config",
            Some(json!([1, 2]))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(
            &app,
            Method::PUT,
            "/api/dsa/mock/config",
            Some(json!({ "x": "y".repeat(40_000) }))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );

    // A finished round is kept, newest first.
    let round = json!({ "config": saved["last"], "seconds": 2292, "items": [{ "id": "lc-two-sum", "grade": "good", "seconds": 1300 }, { "id": "lc-valid-palindrome", "grade": null, "seconds": 992 }] });
    let (status, out) = call(
        &app,
        Method::POST,
        "/api/dsa/mock/rounds",
        Some(round.clone()),
    )
    .await;
    assert_eq!(
        (status, out["id"].is_number()),
        (StatusCode::OK, true),
        "{out}"
    );
    let mut second = round.clone();
    second["seconds"] = json!(100);
    call(&app, Method::POST, "/api/dsa/mock/rounds", Some(second)).await;
    let (_, m) = call(&app, Method::GET, "/api/dsa/mock", None).await;
    assert_eq!(
        (
            m["saved"].clone(),
            m["rounds"].as_array().unwrap().len(),
            m["rounds"][0]["seconds"].clone(),
            m["rounds"][1]["items"][0]["id"].clone()
        ),
        (saved, 2, json!(100), json!("lc-two-sum"))
    );

    // Unknown problems, a bad grade and an empty round are refused.
    let mut bad = round.clone();
    bad["items"][0]["id"] = json!("lc-nothing");
    assert_eq!(
        call(&app, Method::POST, "/api/dsa/mock/rounds", Some(bad))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let mut bad = round.clone();
    bad["items"][0]["grade"] = json!("great");
    assert_eq!(
        call(&app, Method::POST, "/api/dsa/mock/rounds", Some(bad))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let mut bad = round;
    bad["items"] = json!([]);
    assert_eq!(
        call(&app, Method::POST, "/api/dsa/mock/rounds", Some(bad))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn a_pattern_lists_its_techniques_with_lessons_and_progress(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db, root.path());
    call(&app, Method::POST, "/api/dsa/problems/lc-valid-palindrome/log", Some(json!({ "grade": "good" }))).await;
    call(&app, Method::POST, "/api/dsa/problems/lc-valid-palindrome-ii/log", Some(json!({ "grade": "good" }))).await;
    let (status, p) = call(&app, Method::GET, "/api/dsa/patterns/D2", None).await;
    assert_eq!(status, StatusCode::OK, "{p}");
    assert_eq!((p["pattern"].as_str(), p["code"].as_str(), p["total"].as_u64()), (Some("Two Pointers"), Some("D2"), Some(3)));
    assert!(p["intro"].as_str().unwrap().starts_with("Two indexes"));
    let t = &p["techniques"][0];
    assert_eq!((t["id"].as_str(), t["lesson"]["signals"].as_array().unwrap().len()), (Some("Two Pointers:opposite"), 2));
    // Must-learn first; the problems outside the lists follow, each marked by its list tag (none for them), and only the NeetCode ones
    // count in `solved`.
    let rows = t["problems"].as_array().unwrap();
    assert_eq!((rows[0]["id"].as_str(), rows.len(), t["solved"].as_u64()), (Some("lc-valid-palindrome"), 5, Some(1)));
    let outside: Vec<_> = rows.iter().filter(|r| r["list_tag"].is_null()).collect();
    assert_eq!(outside.len(), 2, "two problems of this technique are outside the lists");
    assert!(outside.iter().all(|r| r["priority"].is_string() && r["companies"].as_array().is_some_and(|c| !c.is_empty()) && !r["tags"].as_array().unwrap().is_empty()));
    assert_eq!(rows[0]["list_tag"].as_str(), Some("blind75"), "the narrowest list it is in");
    assert!(rows.iter().all(|r| r["insight"].is_string()), "every row carries its key insight (empty when the content has none)");
    // Logging one of them is tracked (it is solved) but not counted.
    let logged: Vec<_> = outside.iter().filter(|r| r["state"]["solved"].as_bool() == Some(true)).collect();
    assert_eq!(logged.len(), 1, "the logged practice problem shows as solved");
    // A technique with no lesson written still lists its problems.
    let (_, a) = call(&app, Method::GET, "/api/dsa/patterns/D1", None).await;
    assert_eq!((a["techniques"][0]["lesson"].clone(), a["intro"].clone()), (Value::Null, Value::Null));
    assert_eq!(call(&app, Method::GET, "/api/dsa/patterns/D99", None).await.0, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn the_review_queue_lists_what_is_due_with_a_preview_for_each_grade(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db.clone(), root.path());
    let (status, q) = call(&app, Method::GET, "/api/dsa/review", None).await;
    assert_eq!((status, q["items"].as_array().unwrap().len(), q["next_review"].clone()), (StatusCode::OK, 0, Value::Null), "{q}");
    assert!(q["next_new"]["slug"].is_string());

    // A problem logged ✓ and then made due: it is today's review, with a date for each grade.
    call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/log", Some(json!({ "grade": "good" }))).await;
    call(&app, Method::POST, "/api/dsa/problems/lc-contains-duplicate/log", Some(json!({ "grade": "good" }))).await;
    sqlx::query("UPDATE reviews SET due_at = now() - interval '1 day' WHERE problem_id = 'lc-two-sum'").execute(&db).await.unwrap();
    let (_, q) = call(&app, Method::GET, "/api/dsa/review", None).await;
    let items = q["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{q}");
    let item = &items[0];
    assert_eq!((item["problem"]["id"].as_str(), item["pattern"].as_str(), item["technique"].as_str()), (Some("lc-two-sum"), Some("Arrays & Hashing"), Some("Complement lookup in a hash map")));
    assert!(item["recall"].as_f64().unwrap() > 0.0 && item["stability"].as_f64().unwrap() > 0.0);
    let days = |g: &str| item["previews"][g]["days"].as_i64().unwrap();
    assert!(["again", "hard", "good", "easy"].iter().all(|g| days(g) >= 1 && item["previews"][*g]["due"].is_string()));
    assert!(days("easy") > days("again"), "{}", item["previews"]);
    // Peeking changes nothing: the problem is still due and still has one review.
    let (_, again) = call(&app, Method::GET, "/api/dsa/review", None).await;
    assert_eq!(again["items"].as_array().unwrap().len(), 1);
    // The other problem's review is later, so it is what comes next.
    assert_eq!(q["next_review"]["id"], "lc-contains-duplicate");

    // Grading it through the ordinary log takes it off today's queue.
    call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/log", Some(json!({ "grade": "good" }))).await;
    let (_, after) = call(&app, Method::GET, "/api/dsa/review", None).await;
    assert_eq!(after["items"].as_array().unwrap().len(), 0);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn a_statement_is_fetched_once_cleaned_and_kept(db: PgPool) {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static REQUESTS: AtomicUsize = AtomicUsize::new(0);
    // A stand-in for LeetCode's GraphQL endpoint.
    let fake = Router::new().route(
        "/graphql",
        axum::routing::post(|axum::Json(body): axum::Json<Value>| async move {
            REQUESTS.fetch_add(1, Ordering::SeqCst);
            let paid = body["variables"]["s"] == "encode-and-decode-strings";
            axum::Json(json!({ "data": { "question": {
                "content": if paid { Value::Null } else { json!("<p>Given <code>nums</code>.</p><script>alert(1)</script>") },
                "hints": ["<p>Use a hash map.</p>"], "isPaidOnly": paid } } }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/graphql", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, fake).await.unwrap() });
    unsafe { std::env::set_var("ANNEAL_LEETCODE_URL", &url) };

    let root = dsa_root(None);
    let app = test_app_with(db.clone(), root.path());
    let (status, st) = call(&app, Method::GET, "/api/dsa/problems/lc-two-sum/statement", None).await;
    assert_eq!(status, StatusCode::OK, "{st}");
    assert_eq!((st["locked"].clone(), st["stale"].clone(), st["hints"][0].as_str()), (json!(false), json!(false), Some("<p>Use a hash map.</p>")));
    let html = st["html"].as_str().unwrap();
    assert!(html.contains("<code>nums</code>") && !html.contains("script"), "{html}");

    // The second read comes from the table, not from LeetCode.
    call(&app, Method::GET, "/api/dsa/problems/lc-two-sum/statement", None).await;
    assert_eq!(REQUESTS.load(Ordering::SeqCst), 1);

    // A Premium problem has no public statement.
    let (_, paid) = call(&app, Method::GET, "/api/dsa/problems/lc-encode-and-decode-strings/statement", None).await;
    assert_eq!((paid["locked"].clone(), paid["html"].clone()), (json!(true), Value::Null));

    // LeetCode goes away: an old copy is still served, marked stale; a problem never fetched is an upstream error.
    unsafe { std::env::set_var("ANNEAL_LEETCODE_URL", "http://127.0.0.1:9/graphql") };
    sqlx::query("UPDATE dsa_statements SET fetched_at = now() - interval '40 days'").execute(&db).await.unwrap();
    let (status, old) = call(&app, Method::GET, "/api/dsa/problems/lc-two-sum/statement", None).await;
    assert_eq!((status, old["stale"].clone()), (StatusCode::OK, json!(true)));
    let (status, err) = call(&app, Method::GET, "/api/dsa/problems/lc-valid-palindrome/statement", None).await;
    assert_eq!((status, err["error"].as_str()), (StatusCode::BAD_GATEWAY, Some("upstream")));

    // Only DSA problems have a LeetCode statement.
    assert_eq!(call(&app, Method::GET, "/api/dsa/problems/p1-sorted-pair-sum/statement", None).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(call(&app, Method::GET, "/api/dsa/problems/lc-nothing/statement", None).await.0, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn any_problem_previews_what_each_grade_would_schedule(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db, root.path());
    // A problem never logged: the first review is held for the consolidation day, so every grade gives a date.
    let (status, p) = call(&app, Method::GET, "/api/dsa/problems/lc-two-sum/preview", None).await;
    assert_eq!(status, StatusCode::OK, "{p}");
    for g in ["again", "hard", "good", "easy"] {
        assert!(p["previews"][g]["days"].as_i64().unwrap() >= 1 && p["previews"][g]["due"].is_string(), "{g}: {p}");
    }
    // Practice problems schedule nothing; non-DSA and unknown problems are refused.
    let (_, practice) = call(&app, Method::GET, "/api/dsa/problems/lc-valid-palindrome-ii/preview", None).await;
    assert_eq!(practice["previews"], Value::Null);
    assert_eq!(call(&app, Method::GET, "/api/dsa/problems/p1-sorted-pair-sum/preview", None).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(call(&app, Method::GET, "/api/dsa/problems/lc-nothing/preview", None).await.0, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn my_solutions_are_kept_per_problem_and_can_be_edited_and_removed(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db, root.path());
    let list = |id: &'static str| call_app(&app, id);
    async fn call_app(app: &Router, id: &str) -> Value {
        call(app, Method::GET, &format!("/api/dsa/problems/{id}/solutions"), None).await.1
    }
    assert_eq!(list("lc-two-sum").await, json!([]));

    // Any number can be added, and they keep their order.
    let (status, a) = call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/solutions", Some(json!({ "label": " hash map ", "code": "class Solution:\n    pass\n", "notes": "O(n)" }))).await;
    assert_eq!(status, StatusCode::OK, "{a}");
    assert_eq!(a["label"], "hash map");
    let (_, b) = call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/solutions", Some(json!({ "code": "print(1)" }))).await;
    assert_eq!(b["label"], "");
    let all = list("lc-two-sum").await;
    assert_eq!(all.as_array().unwrap().len(), 2);
    assert_eq!(all[0]["id"], a["id"]);
    assert_eq!(list("lc-valid-palindrome-ii").await, json!([]), "another problem has none");

    // Editing keeps the id and the position.
    let url = format!("/api/dsa/solutions/{}", a["id"]);
    let (status, edited) = call(&app, Method::PUT, &url, Some(json!({ "label": "map", "code": "x = 1\n", "notes": "" }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!((edited["label"].as_str(), edited["code"].as_str()), (Some("map"), Some("x = 1\n")));
    assert_eq!(list("lc-two-sum").await[0]["code"], "x = 1\n");

    // Refused: no code, too long, unknown or non-DSA problems, unknown solutions.
    let blank = Some(json!({ "code": "   " }));
    assert_eq!(call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/solutions", blank).await.0, StatusCode::BAD_REQUEST);
    let long = Some(json!({ "code": "x".repeat(100_001) }));
    assert_eq!(call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/solutions", long).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(call(&app, Method::POST, "/api/dsa/problems/lc-nothing/solutions", Some(json!({ "code": "x" }))).await.0, StatusCode::NOT_FOUND);
    assert_eq!(call(&app, Method::POST, "/api/dsa/problems/p1-sorted-pair-sum/solutions", Some(json!({ "code": "x" }))).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(call(&app, Method::PUT, "/api/dsa/solutions/999999", Some(json!({ "code": "x" }))).await.0, StatusCode::NOT_FOUND);

    // Removing one leaves the other.
    assert_eq!(call(&app, Method::DELETE, &url, None).await.0, StatusCode::OK);
    assert_eq!(call(&app, Method::DELETE, &url, None).await.0, StatusCode::NOT_FOUND);
    let left = list("lc-two-sum").await;
    assert_eq!(left.as_array().unwrap().len(), 1);
    assert_eq!(left[0]["id"], b["id"]);
}

fn plus(day: &str, n: i64) -> String {
    (day.parse::<chrono::NaiveDate>().unwrap() + chrono::TimeDelta::days(n)).to_string()
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn the_plan_starts_from_the_routine_and_follows_the_owners_rules_and_calendar(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db, root.path());
    let (status, p) = call(&app, Method::GET, "/api/plan", None).await;
    assert_eq!(status, StatusCode::OK, "{p}");
    let today = p["today"].as_str().unwrap().to_owned();
    assert_eq!(p["active"], "default");
    assert_eq!(p["plans"].as_array().unwrap().len(), 1);
    assert_eq!(p["overrides"], json!({}));
    let topics: Vec<&str> = p["topics"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert_eq!(topics, ["D1", "D2"]);
    assert!(p["summary"]["left"].as_u64().unwrap() >= 3 && p["summary"]["finish"].is_string(), "{}", p["summary"]);
    assert!(p["days"].as_array().unwrap().len() > 100);
    // Every day has a kind; a problem day carries a problem unless the list has run out.
    let first_solve = p["days"].as_array().unwrap().iter().find(|d| d["kind"] == "solve" && d["date"].as_str().unwrap() >= today.as_str()).unwrap();
    assert!(first_solve["new"][0]["title"].is_string());

    // Making the first problem day a break moves its problem to the next problem day.
    let day = first_solve["date"].as_str().unwrap().to_owned();
    let first_problem = first_solve["new"][0]["id"].clone();
    let state = json!({ "rules": p["rules"], "overrides": { day.clone(): "break" } });
    assert_eq!(call(&app, Method::PUT, "/api/plan/state", Some(state)).await.0, StatusCode::OK);
    let (_, q) = call(&app, Method::GET, &format!("/api/plan?from={day}&to={}", plus(&day, 10)), None).await;
    assert_eq!(q["overrides"][&day], "break");
    let broken = &q["days"][0];
    assert_eq!((broken["kind"].as_str(), broken["edited"].as_bool(), broken["capacity"].as_u64()), (Some("break"), Some(true), Some(0)));
    assert_eq!(broken["new"], json!([]));
    let moved_to = q["days"].as_array().unwrap().iter().find(|d| d["new"][0]["id"] == first_problem).expect("the problem moved to another day");
    assert!(moved_to["date"].as_str().unwrap() > day.as_str());

    // A preview says what a change would do, and saves nothing.
    let (_, pv) = call(&app, Method::POST, "/api/plan/preview", Some(json!({ "overrides": { plus(&day, 1): "break", plus(&day, 2): "break" } }))).await;
    assert!(pv["diff"]["moved"].as_u64().unwrap() >= 1 && pv["diff"]["finish_days"].as_i64().unwrap() >= 1, "{pv}");
    let (_, again) = call(&app, Method::GET, "/api/plan", None).await;
    assert_eq!(again["overrides"].as_object().unwrap().len(), 1, "a preview doesn't save");

    // Rules are checked.
    let mut bad = p["rules"].clone();
    bad["targets"] = json!({ "D99": plus(&today, 20) });
    assert_eq!(call(&app, Method::PUT, "/api/plan/state", Some(json!({ "rules": bad, "overrides": {} }))).await.0, StatusCode::BAD_REQUEST);
    let mut bad = p["rules"].clone();
    bad["difficulty"] = json!([]);
    assert_eq!(call(&app, Method::PUT, "/api/plan/state", Some(json!({ "rules": bad, "overrides": {} }))).await.0, StatusCode::BAD_REQUEST);
    let mut bad = p["rules"].clone();
    bad["topic_order"] = json!(["D1"]);
    assert_eq!(call(&app, Method::PUT, "/api/plan/state", Some(json!({ "rules": bad, "overrides": {} }))).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(call(&app, Method::GET, &format!("/api/plan?from={today}&to={}", plus(&today, 400)), None).await.0, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn plans_can_be_saved_switched_and_deleted_each_with_its_own_calendar(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db, root.path());
    let (_, p) = call(&app, Method::GET, "/api/plan", None).await;
    let today = p["today"].as_str().unwrap().to_owned();
    let day = plus(&today, 3);

    // A target and a break in the default plan, then save it as another plan.
    let mut rules = p["rules"].clone();
    rules["targets"] = json!({ "D2": plus(&today, 40) });
    call(&app, Method::PUT, "/api/plan/state", Some(json!({ "rules": rules, "overrides": { day.clone(): "break" } }))).await;
    let (status, made) = call(&app, Method::POST, "/api/plans", Some(json!({ "name": "  Sprint " }))).await;
    assert_eq!(status, StatusCode::OK, "{made}");
    let id = made["id"].as_str().unwrap().to_owned();
    let (_, p) = call(&app, Method::GET, "/api/plan", None).await;
    assert_eq!(p["active"], id.as_str());
    assert_eq!(p["plans"].as_array().unwrap().len(), 2);
    assert_eq!(p["plans"][1]["name"], "Sprint");
    assert_eq!(p["overrides"][&day], "break", "the copy keeps the calendar edits");

    // Changing the new plan doesn't touch the default.
    call(&app, Method::PUT, "/api/plan/state", Some(json!({ "rules": p["rules"], "overrides": {} }))).await;
    assert_eq!(call(&app, Method::PUT, "/api/plans/active", Some(json!({ "id": "default" }))).await.0, StatusCode::OK);
    let (_, d) = call(&app, Method::GET, "/api/plan", None).await;
    assert_eq!(d["overrides"][&day], "break");
    assert_eq!(d["rules"]["targets"]["D2"], plus(&today, 40));

    // Refused: a blank name, an unknown plan, deleting the default. Deleting the other one works.
    assert_eq!(call(&app, Method::POST, "/api/plans", Some(json!({ "name": "  " }))).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(call(&app, Method::PUT, "/api/plans/active", Some(json!({ "id": "nope" }))).await.0, StatusCode::NOT_FOUND);
    assert_eq!(call(&app, Method::DELETE, "/api/plans/default", None).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(call(&app, Method::DELETE, &format!("/api/plans/{id}"), None).await.0, StatusCode::OK);
    let (_, d) = call(&app, Method::GET, "/api/plan", None).await;
    assert_eq!(d["plans"].as_array().unwrap().len(), 1);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn a_break_today_means_no_reviews_and_no_new_problem(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db, root.path());
    call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/log", Some(json!({ "grade": "again" }))).await;
    let (_, p) = call(&app, Method::GET, "/api/plan", None).await;
    let today = p["today"].as_str().unwrap().to_owned();
    let (_, o) = call(&app, Method::GET, "/api/dsa", None).await;
    assert!(o["plan"]["capacity"].as_u64().unwrap() >= 1);

    call(&app, Method::PUT, "/api/plan/state", Some(json!({ "rules": p["rules"], "overrides": { today.clone(): "break" } }))).await;
    let (_, o) = call(&app, Method::GET, "/api/dsa", None).await;
    assert_eq!((o["plan"]["capacity"].as_u64(), o["plan"]["solve_day"].as_bool()), (Some(0), Some(false)));
    assert_eq!(o["plan"]["review_ids"], json!([]));

    // And a review logged now isn't scheduled onto the break.
    let tomorrow = plus(&today, 1);
    call(&app, Method::PUT, "/api/plan/state", Some(json!({ "rules": p["rules"], "overrides": { tomorrow.clone(): "break", plus(&today, 2): "break" } }))).await;
    let (_, out) = call(&app, Method::POST, "/api/dsa/problems/lc-valid-palindrome/log", Some(json!({ "grade": "again" }))).await;
    let due = out["due"].as_str().unwrap();
    assert!(due != tomorrow && due != plus(&today, 2), "scheduled on a break: {due}");
}

// ---- courses ----------------------------------------------------------------------------------------------------

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn course_tree_stage_page_runs_and_solutions(db: PgPool) {
    let app = test_app(db);
    let (status, tree) = call(&app, Method::GET, "/api/courses/bustub", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tree["done"], 0);
    assert_eq!(tree["current"], "1a-01");
    let projects = tree["projects"].as_array().unwrap();
    assert!(!projects.is_empty() && projects.iter().all(|p| p["planned"].is_boolean()), "every project says whether it is only planned");
    // beyond BusTub: a module that ports none of BusTub's files, and every challenge; BusTub's own modules and stages are not
    let module = |code: &str| projects.iter().flat_map(|p| p["modules"].as_array().unwrap()).find(|m| m["code"] == code).cloned().unwrap_or_else(|| panic!("module {code}"));
    assert_eq!(module("4d")["beyond"], true, "the lock manager is not part of BusTub");
    assert_eq!(module("1a")["beyond"], false, "the disk manager is");
    let stages = |code: &str| module(code)["stages"].as_array().unwrap().clone();
    assert!(stages("1a").iter().filter(|x| x["kind"] == "challenge").all(|x| x["beyond"] == true), "every challenge is beyond BusTub");
    assert!(stages("1a").iter().filter(|x| x["kind"] != "challenge").all(|x| x["beyond"] == false), "BusTub's own stages are not");

    // a stage page: markdown parts, no hints written yet, no solution uploaded yet
    let (status, st) = call(&app, Method::GET, "/api/courses/bustub/stages/1a-01", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(st["state"], "todo");
    assert_eq!(st["solution"]["available"], false);
    assert!(st["stage"]["sections"].as_array().unwrap().len() >= 2);
    assert_eq!(st["next"]["id"], "1a-02");
    // concepts: the stage lists them, and each one has its own page that points back
    let k = st["concepts"][0]["id"].as_str().expect("1a-01 has a concept").to_owned();
    let (status, c) = call(&app, Method::GET, &format!("/api/courses/bustub/concepts/{k}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(c["concept"]["sections"].as_array().unwrap().len() >= 3);
    assert!(c["used_in"].as_array().unwrap().iter().any(|u| u["id"] == "1a-01"), "the concept points back to the stage that lists it");
    assert_eq!(call(&app, Method::GET, "/api/courses/bustub/concepts/nope", None).await.0, StatusCode::NOT_FOUND);
    assert!(call(&app, Method::GET, "/api/courses/bustub/stages/nope", None).await.0 == StatusCode::NOT_FOUND);
    assert!(call(&app, Method::GET, "/api/courses/nope", None).await.0 == StatusCode::NOT_FOUND);

    // a failing run leaves the stage unsolved; the solution can't be opened before it is uploaded
    let fail = json!({ "stage_id": "1a-01", "tests": [{"name": "a", "ok": true}, {"name": "b", "ok": false, "detail": "left: 1, right: 2"}] });
    let (status, r) = call(&app, Method::POST, "/api/courses/bustub/runs", Some(fail)).await;
    assert_eq!((status, r["ok"].as_bool()), (StatusCode::OK, Some(false)));
    let (_, st) = call(&app, Method::GET, "/api/courses/bustub/stages/1a-01", None).await;
    assert_eq!(st["state"], "todo");
    assert_eq!(st["last_run"]["passed"], 1);
    assert_eq!(call(&app, Method::POST, "/api/courses/bustub/stages/1a-01/solution", None).await.0, StatusCode::NOT_FOUND);

    // upload a solution; opening it before passing marks the stage assisted
    let up = json!({ "stages": { "1a-01": [{"path": "src/x.rs", "lines": [" fn f() {", "+    1", "-    todo!()", " }"]}] } });
    assert_eq!(call(&app, Method::PUT, "/api/courses/bustub/solutions", Some(up)).await.0, StatusCode::OK);
    let (_, st) = call(&app, Method::GET, "/api/courses/bustub/stages/1a-01", None).await;
    assert_eq!((st["solution"]["available"].as_bool(), st["solution"]["open"].as_bool()), (Some(true), Some(false)));
    assert!(st["solution"]["files"].is_null(), "the solution stays hidden until it is opened");
    let (_, st) = call(&app, Method::POST, "/api/courses/bustub/stages/1a-01/solution", None).await;
    assert_eq!(st["solution"]["files"][0]["path"], "src/x.rs");

    // passing now counts as solved, but assisted
    let pass = json!({ "stage_id": "1a-01", "tests": [{"name": "a", "ok": true}, {"name": "b", "ok": true}] });
    let (_, r) = call(&app, Method::POST, "/api/courses/bustub/runs", Some(pass)).await;
    assert_eq!(r["ok"], true);
    let (_, st) = call(&app, Method::GET, "/api/courses/bustub/stages/1a-01", None).await;
    assert_eq!(st["state"], "assisted");
    let (_, tree) = call(&app, Method::GET, "/api/courses/bustub", None).await;
    assert_eq!((tree["done"].as_u64(), tree["current"].as_str()), (Some(1), Some("1a-02")));
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn a_stage_keeps_its_run_history_newest_first_capped_at_ten(db: PgPool) {
    let app = test_app(db);
    let (_, st) = call(&app, Method::GET, "/api/courses/bustub/stages/1a-02", None).await;
    assert_eq!(st["runs"].as_array().unwrap().len(), 0);
    assert!(st["last_run"].is_null());
    for n in 1..=12 {
        let tests: Vec<_> = (0..n).map(|i| json!({ "name": format!("t{i}"), "ok": false, "detail": "no" })).collect();
        let (status, _) = call(&app, Method::POST, "/api/courses/bustub/runs", Some(json!({ "stage_id": "1a-02", "tests": tests }))).await;
        assert_eq!(status, StatusCode::OK);
    }
    let (_, st) = call(&app, Method::GET, "/api/courses/bustub/stages/1a-02", None).await;
    let runs = st["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 10, "only the latest ten");
    assert_eq!(runs[0]["total"], 12, "newest first");
    assert_eq!(runs[9]["total"], 3);
    assert_eq!(st["last_run"]["id"], runs[0]["id"]);
    // another stage's runs are not mixed in
    let (_, other) = call(&app, Method::GET, "/api/courses/bustub/stages/1a-03", None).await;
    assert_eq!(other["runs"].as_array().unwrap().len(), 0);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn concepts_can_be_marked_read_and_unread(db: PgPool) {
    let app = test_app(db);
    // a stage with concepts: every module is written with its concepts optional now, so `required` follows the stage's own list
    let (_, st) = call(&app, Method::GET, "/api/courses/bustub/stages/1a-01", None).await;
    let k = st["concepts"][0]["id"].as_str().unwrap().to_owned();
    assert_eq!(st["concepts"][0]["read"], false);
    assert!(st["concepts"][0]["required"].is_boolean(), "every linked concept says whether it is required");

    let url = format!("/api/courses/bustub/concepts/{k}/read");
    let (status, r) = call(&app, Method::PUT, &url, Some(json!({ "read": true }))).await;
    assert_eq!((status, r["read"].as_bool()), (StatusCode::OK, Some(true)));
    // twice is fine
    assert_eq!(call(&app, Method::PUT, &url, Some(json!({ "read": true }))).await.0, StatusCode::OK);
    let (_, st) = call(&app, Method::GET, "/api/courses/bustub/stages/1a-01", None).await;
    assert_eq!(st["concepts"][0]["read"], true);
    let (_, c) = call(&app, Method::GET, &format!("/api/courses/bustub/concepts/{k}"), None).await;
    assert_eq!(c["read"], true);

    let (_, r) = call(&app, Method::PUT, &url, Some(json!({ "read": false }))).await;
    assert_eq!(r["read"], false);
    let (_, st) = call(&app, Method::GET, "/api/courses/bustub/stages/1a-01", None).await;
    assert_eq!(st["concepts"][0]["read"], false);

    assert_eq!(call(&app, Method::PUT, "/api/courses/bustub/concepts/nope/read", Some(json!({ "read": true }))).await.0, StatusCode::NOT_FOUND);
    assert_eq!(call(&app, Method::PUT, "/api/courses/nope/concepts/x/read", Some(json!({ "read": true }))).await.0, StatusCode::NOT_FOUND);
}

/// The stage page shows a test's code, cut from `courses/<id>/template/tests`. The production image is built from the repo minus
/// `.dockerignore`: if the template is ignored there, the server has no test code and the page shows none (it happened once).
#[test]
fn the_production_image_keeps_the_template_tests() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let ignore = std::fs::read_to_string(root.join(".dockerignore")).unwrap();
    let hidden: Vec<&str> = ignore.lines().map(str::trim).filter(|l| l.starts_with("courses/") && l.contains("template")).collect();
    assert!(hidden.is_empty(), ".dockerignore hides the course template, so the server cannot show test code: {hidden:?}");
    let course = anneal_content::course::Course::load(&root.join("courses/bustub")).unwrap();
    let stage = course.stages().find(|s| s.id == "1a-01").unwrap();
    assert!(stage.test_sources.len() >= 5, "1a-01 has its tests' source: {:?}", stage.test_sources.keys().collect::<Vec<_>>());
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn a_started_run_shows_as_running_until_its_report_arrives(db: PgPool) {
    let app = test_app(db);
    let page = |app: &Router| {
        let app = app.clone();
        async move { call(&app, Method::GET, "/api/courses/bustub/stages/1a-01", None).await.1 }
    };
    assert!(page(&app).await["running"].is_null(), "nothing is running on a fresh stage");
    let (status, _) = call(&app, Method::POST, "/api/courses/bustub/runs/start", Some(json!({ "stage_id": "1a-01" }))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page(&app).await["running"].is_string(), "the page knows a run has started");
    assert!(call(&app, Method::GET, "/api/courses/bustub/stages/1a-02", None).await.1["running"].is_null(), "only that stage");
    assert_eq!(call(&app, Method::POST, "/api/courses/bustub/runs/start", Some(json!({ "stage_id": "nope" }))).await.0, StatusCode::NOT_FOUND);
    let run = json!({ "stage_id": "1a-01", "tests": [{"name": "a", "ok": true}] });
    call(&app, Method::POST, "/api/courses/bustub/runs", Some(run)).await;
    assert!(page(&app).await["running"].is_null(), "a report ends the running state");
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn progress_resets_for_a_module_a_project_or_the_whole_course(db: PgPool) {
    let app = test_app(db);
    let pass = |id: &str| json!({ "stage_id": id, "tests": [{"name": "a", "ok": true}] });
    for id in ["1a-01", "1a-02", "1b-01", "2a-01"] {
        assert_eq!(call(&app, Method::POST, "/api/courses/bustub/runs", Some(pass(id))).await.0, StatusCode::OK);
    }
    let done = |app: &Router| {
        let app = app.clone();
        async move { call(&app, Method::GET, "/api/courses/bustub", None).await.1["done"].as_i64().unwrap() }
    };
    assert_eq!(done(&app).await, 4);
    let reset = |body: Value| call(&app, Method::POST, "/api/courses/bustub/reset", Some(body));
    // one module
    let (status, r) = reset(json!({ "module": "1a" })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(r["stage_states_removed"], 2);
    assert_eq!(done(&app).await, 2);
    // one project (project 1 holds 1b, project 2 holds 2a)
    assert_eq!(reset(json!({ "project": 1 })).await.0, StatusCode::OK);
    assert_eq!(done(&app).await, 1, "only 2a-01 is left");
    // the whole course, and a body that says nothing, an unknown module and an unknown project
    assert_eq!(reset(json!({})).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(reset(json!({ "module": "zz" })).await.0, StatusCode::NOT_FOUND);
    assert_eq!(reset(json!({ "project": 99 })).await.0, StatusCode::NOT_FOUND);
    assert_eq!(done(&app).await, 1, "refused requests change nothing");
    assert_eq!(reset(json!({ "all": true })).await.0, StatusCode::OK);
    assert_eq!(done(&app).await, 0);
    let page = call(&app, Method::GET, "/api/courses/bustub/stages/2a-01", None).await.1;
    assert_eq!(page["state"], "todo");
    assert!(page["last_run"].is_null(), "the runs are gone too");
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn a_track_resets_to_nothing_solved_and_leaves_other_tracks_alone(db: PgPool) {
    let app = test_app(db);
    let (_, out) = call(&app, Method::POST, &format!("/api/problems/{NDT}/submit"), Some(json!({ "code": solution() }))).await;
    assert_eq!(out["run"]["status"], "passed", "{out}");
    assert_eq!(call(&app, Method::PUT, &format!("/api/problems/{NDT}/draft"), Some(json!({ "code": "// wip" }))).await.0, StatusCode::NO_CONTENT);
    assert_eq!(call(&app, Method::GET, "/api/tracks/d9", None).await.1["solved"], 1);

    assert_eq!(call(&app, Method::POST, "/api/tracks/nope/reset", Some(json!({}))).await.0, StatusCode::NOT_FOUND);
    assert_eq!(call(&app, Method::GET, "/api/tracks/d9", None).await.1["solved"], 1, "a refused reset changes nothing");

    let (status, r) = call(&app, Method::POST, "/api/tracks/d9/reset", Some(json!({}))).await;
    assert_eq!(status, StatusCode::OK, "{r}");
    assert_eq!(r["attempts_removed"], 1);
    assert_eq!(r["drafts_removed"], 1);
    let (_, t) = call(&app, Method::GET, "/api/tracks/d9", None).await;
    assert_eq!(t["solved"], 0, "nothing is solved any more");
    let (_, p) = call(&app, Method::GET, &format!("/api/problems/{NDT}"), None).await;
    assert_eq!(p["draft"], Value::Null, "the draft is gone too");
    assert!(p["attempt"].is_null() || p["attempt"]["solved"] != true, "{p}");
    // doing it again is harmless
    assert_eq!(call(&app, Method::POST, "/api/tracks/d9/reset", Some(json!({}))).await.0, StatusCode::OK);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn the_repository_of_a_course_is_recorded_and_read_back(db: PgPool) {
    let app = test_app(db);
    let url = "/api/courses/bustub/repo";
    assert_eq!(call(&app, Method::GET, url, None).await.1["url"], Value::Null, "nothing recorded yet");
    let (status, r) = call(&app, Method::PUT, url, Some(json!({ "url": "git@github.com:me/bustub-rs.git" }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(r["url"], "git@github.com:me/bustub-rs.git");
    assert_eq!(call(&app, Method::PUT, url, Some(json!({ "url": "https://github.com/me/other.git" }))).await.0, StatusCode::OK, "recording again replaces it");
    assert_eq!(call(&app, Method::GET, url, None).await.1["url"], "https://github.com/me/other.git");
    for bad in ["", "two words", &"x".repeat(501)] {
        assert_eq!(call(&app, Method::PUT, url, Some(json!({ "url": bad }))).await.0, StatusCode::BAD_REQUEST, "{bad:?}");
    }
    assert_eq!(call(&app, Method::GET, "/api/courses/nope/repo", None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(call(&app, Method::GET, url, None).await.1["url"], "https://github.com/me/other.git", "refused requests change nothing");
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn the_installer_names_the_address_it_was_fetched_from(db: PgPool) {
    let app = test_app(db);
    let get = |host: &'static str, proto: Option<&'static str>| {
        let app = app.clone();
        async move {
            let mut req = Request::builder().uri("/install.sh").header("host", host);
            if let Some(p) = proto {
                req = req.header("x-forwarded-proto", p);
            }
            let res = app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
            let status = res.status();
            let text = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
            (status, text)
        }
    };
    let (status, script) = get("anneal.example.dev", Some("https")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(script.starts_with("#!/bin/sh"), "a shell script");
    assert!(script.contains("base=\"https://anneal.example.dev\""), "the address it came from");
    assert!(!script.contains("__BASE__"), "nothing left to fill in");
    assert!(get("127.0.0.1:8787", None).await.1.contains("base=\"http://127.0.0.1:8787\""), "plain http for a local server");
    assert!(get("evil.example/\";rm -rf ~;\"", None).await.1.contains("base=\"http://localhost:8787\""), "a host with odd characters is not echoed into the script");
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn the_overview_names_the_last_run_the_cli_reported(db: PgPool) {
    let app = test_app(db);
    assert!(call(&app, Method::GET, "/api/courses/bustub", None).await.1["last_run"].is_null(), "nothing reported yet");
    let pass = |id: &str| json!({ "stage_id": id, "tests": [{"name": "a", "ok": true}] });
    assert_eq!(call(&app, Method::POST, "/api/courses/bustub/runs", Some(pass("1a-01"))).await.0, StatusCode::OK);
    assert_eq!(call(&app, Method::POST, "/api/courses/bustub/runs", Some(pass("1a-02"))).await.0, StatusCode::OK);
    let last = call(&app, Method::GET, "/api/courses/bustub", None).await.1["last_run"].clone();
    assert_eq!(last["stage_id"], "1a-02", "{last}");
    assert_eq!((last["ok"].as_bool(), last["passed"].as_i64(), last["total"].as_i64()), (Some(true), Some(1), Some(1)));
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn shipped_solutions_are_loaded_once_and_only_for_stages_the_course_has(db: PgPool) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../courses/bustub");
    let course = anneal_content::course::Course::load(&root).expect("courses/bustub");
    let dir = tempfile::tempdir().unwrap();
    let write = |text: &str| std::fs::write(dir.path().join("solutions.json"), text).unwrap();
    let seed = |db: &PgPool| {
        let (db, course, dir) = (db.clone(), &course, dir.path().to_owned());
        async move { anneal_api::course::seed_solutions(&db, course, &dir).await.unwrap() }
    };

    // no file: nothing happens
    assert_eq!(seed(&db).await, 0);
    // a stage of the course, one it does not have, and a challenge (which never has a published solution)
    write(r#"{"1a-01": [{"path": "src/a.rs", "lines": ["+let x = 1;"]}], "nope-99": [{"path": "src/b.rs", "lines": ["+y"]}], "1a-c1": [{"path": "src/c.rs", "lines": ["+z"]}]}"#);
    assert_eq!(seed(&db).await, 1, "only 1a-01");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM course_solutions").fetch_one(&db).await.unwrap();
    assert_eq!(n, 1);
    // unchanged: no row is rewritten, and a changed solution is
    assert_eq!(seed(&db).await, 0);
    write(r#"{"1a-01": [{"path": "src/a.rs", "lines": ["+let x = 2;"]}]}"#);
    assert_eq!(seed(&db).await, 1);
    // the app now offers it
    let app = test_app(db.clone());
    let page = call(&app, Method::GET, "/api/courses/bustub/stages/1a-01", None).await.1;
    assert_eq!(page["solution"]["available"], true, "{page}");
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn plan_lists_every_topic_even_when_it_starts_part_way(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db, root.path());
    let (status, _) = call(&app, Method::POST, "/api/dsa/start", Some(json!({ "from": "d2-two-pointers" }))).await;
    assert_eq!(status, StatusCode::OK);
    let (_, p) = call(&app, Method::GET, "/api/plan", None).await;
    let topics: Vec<&str> = p["topics"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert_eq!(topics, ["D2", "D1"], "{}", p["topics"]);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn doing_another_problem_instead_of_the_next_one(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db, root.path());
    let (_, o) = call(&app, Method::GET, "/api/dsa", None).await;
    assert_eq!(o["plan"]["next_up"][0], "lc-contains-duplicate");
    let today_of = |p: &Value, id: &str| -> Option<String> {
        p["days"].as_array().unwrap().iter().find(|d| d["new"].as_array().is_some_and(|n| n.iter().any(|x| x["id"] == id))).map(|d| d["date"].as_str().unwrap().to_owned())
    };
    let (_, before) = call(&app, Method::GET, "/api/plan", None).await;
    let two_sum_day = today_of(&before, "lc-two-sum").expect("two sum is planned");
    let next_day = today_of(&before, "lc-contains-duplicate").expect("the next problem is planned");

    // Same topic: lc-two-sum is done instead. The next problem stays next, on the first problem day, and the plan
    // closes up behind it (two sum's own day is no longer needed).
    call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/log", Some(json!({ "grade": "good" }))).await;
    let (_, o) = call(&app, Method::GET, "/api/dsa", None).await;
    assert_eq!((o["plan"]["start"].clone(), o["plan"]["next_up"][0].clone()), (Value::Null, json!("lc-contains-duplicate")));
    let (_, after) = call(&app, Method::GET, "/api/plan", None).await;
    assert_eq!(today_of(&after, "lc-contains-duplicate"), Some(next_day.clone()));
    assert_eq!(today_of(&after, "lc-two-sum"), None, "done, so no longer planned (was {two_sum_day})");

    // Another topic: the sequence restarts after the problem chosen, and the calendar follows it.
    call(&app, Method::POST, "/api/dsa/problems/lc-valid-palindrome/log", Some(json!({ "grade": "good" }))).await;
    let (_, o) = call(&app, Method::GET, "/api/dsa", None).await;
    assert_eq!(o["plan"]["start"], "lc-valid-palindrome");
    assert_eq!(o["plan"]["next_up"][0], "lc-two-sum-ii-input-array-is-sorted");
    let (_, p) = call(&app, Method::GET, "/api/plan", None).await;
    let first = p["days"].as_array().unwrap().iter().find_map(|d| d["new"].as_array().and_then(|n| n.first()).map(|x| x["id"].clone())).unwrap();
    assert_eq!(first, "lc-two-sum-ii-input-array-is-sorted");
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn a_practice_repeat_counts_but_changes_nothing_else(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db.clone(), root.path());
    let log = |id: &'static str, body: Value| {
        let app = app.clone();
        async move { call(&app, Method::POST, &format!("/api/dsa/problems/{id}/log"), Some(body)).await }
    };

    // A problem that was never logged has nothing to repeat.
    let (status, _) = log("lc-two-sum", json!({ "grade": "good", "practice": true })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // First try, on my own: schedules the first review.
    assert_eq!(log("lc-two-sum", json!({ "grade": "good" })).await.0, StatusCode::OK);
    let (_, before) = call(&app, Method::GET, "/api/dsa", None).await;
    let st = &problem_of(&before, "lc-two-sum")["state"];
    let (due, reps) = (st["due"].clone(), st["reps"].clone());

    // Two practice repeats: one on my own, one needing help.
    let (status, out) = log("lc-two-sum", json!({ "grade": "good", "practice": true })).await;
    assert_eq!((status, out["due"].clone()), (StatusCode::OK, Value::Null));
    assert_eq!(log("lc-two-sum", json!({ "grade": "hard", "practice": true })).await.0, StatusCode::OK);
    assert_eq!(log("lc-two-sum", json!({ "grade": "again", "practice": true })).await.0, StatusCode::OK);

    // The schedule, the standing and the plan are exactly as the first try left them.
    let (_, after) = call(&app, Method::GET, "/api/dsa", None).await;
    let st = &problem_of(&after, "lc-two-sum")["state"];
    assert_eq!((st["due"].clone(), st["reps"].clone(), st["solved"].clone(), st["assisted"].clone(), st["last_grade"].clone()), (due, reps, json!(true), json!(false), json!("good")));
    assert_eq!(after["plan"]["start"], before["plan"]["start"]);
    let reviews: i64 = sqlx::query_scalar("SELECT count(*) FROM reviews WHERE problem_id = 'lc-two-sum'").fetch_one(&db).await.unwrap();
    assert_eq!(reviews, 1);

    // The streak and this week count the solves (the first try and two practice solves), not the failed repeat.
    let (_, a) = call(&app, Method::GET, "/api/activity?sections=D", None).await;
    assert_eq!((a["streak"].as_u64(), a["week_solved"].as_u64()), (Some(1), Some(3)));
    assert_eq!(a["recent"][0]["detail"].as_str().map(|d| d.starts_with("practice")), Some(true));

    // The solve record keeps each kind apart.
    let (_, p) = call(&app, Method::GET, "/api/progress", None).await;
    assert_eq!(
        p["record"],
        json!({
            "first": { "own": 1, "help": 0, "not_yet": 0 },
            "later": { "own": 0, "help": 0, "not_yet": 0 },
            "practice": { "own": 1, "help": 1, "not_yet": 1 },
            "practice_count": 2, "problems_practised": 1, "practice_week": 2
        })
    );

    // The problem's own history, oldest first.
    let (_, pv) = call(&app, Method::GET, "/api/dsa/problems/lc-two-sum/preview", None).await;
    assert_eq!(
        pv["history"],
        json!([{ "kind": "first", "result": "good" }, { "kind": "practice", "result": "good" }, { "kind": "practice", "result": "hard" }, { "kind": "practice", "result": "again" }])
    );

    // A later scheduled try is counted as a later try, apart from the practice.
    assert_eq!(log("lc-two-sum", json!({ "grade": "hard" })).await.0, StatusCode::OK);
    let (_, p) = call(&app, Method::GET, "/api/progress", None).await;
    assert_eq!(p["record"]["later"], json!({ "own": 0, "help": 1, "not_yet": 0 }));
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn reset_archives_what_it_removes_and_restore_brings_it_back(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db.clone(), root.path());
    let log = |grade: &'static str| call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/log", Some(json!({ "grade": grade })));
    log("hard").await;
    log("good").await;
    let before: (i64, i64, f32) = sqlx::query_as("SELECT (SELECT count(*) FROM attempts), (SELECT count(*) FROM reviews), (SELECT stability FROM reviews)").fetch_one(&db).await.unwrap();
    assert_eq!((before.0, before.1), (2, 1));

    let (status, out) = call(&app, Method::DELETE, "/api/dsa/problems/lc-two-sum/log", None).await;
    assert_eq!((status, out["archived"].clone()), (StatusCode::OK, json!(true)));
    let gone: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM attempts), (SELECT count(*) FROM reviews), (SELECT count(*) FROM reset_archive)").fetch_one(&db).await.unwrap();
    assert_eq!(gone, (0, 0, 1), "the reset removed the log and archived it");

    // Logging again blocks the restore (it would mix two histories); nothing is lost either way.
    log("good").await;
    let (_, blocked) = call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/restore", None).await;
    assert_eq!(blocked["restored"], json!(false));
    call(&app, Method::DELETE, "/api/dsa/problems/lc-two-sum/log", None).await;
    let (_, back) = call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/restore", None).await;
    assert_eq!(back["restored"], json!(true));
    let after: (i64, i64, f32) = sqlx::query_as("SELECT (SELECT count(*) FROM attempts), (SELECT count(*) FROM reviews), (SELECT stability FROM reviews)").fetch_one(&db).await.unwrap();
    assert_eq!((after.0, after.1), (1, 1), "the second reset archived one attempt and restore brought that one back");
    let archive: (i64, i64) = sqlx::query_as("SELECT count(*), count(restored_at) FROM reset_archive").fetch_one(&db).await.unwrap();
    assert_eq!(archive, (2, 1), "archives are never deleted");
    assert_eq!(call(&app, Method::DELETE, "/api/dsa/problems/lc-nothing/log", None).await.0, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn deleting_something_the_owner_made_archives_it_first(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db.clone(), root.path());

    // A saved solution: deleting it keeps a copy of the code.
    let (status, made) = call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/solutions", Some(json!({ "label": "mine", "code": "def f(): return 42", "notes": "n" }))).await;
    assert_eq!(status, StatusCode::OK, "{made}");
    let sid = made["id"].as_i64().unwrap();
    assert_eq!(call(&app, Method::DELETE, &format!("/api/dsa/solutions/{sid}"), None).await.0, StatusCode::OK);
    let kept: (String,) = sqlx::query_as("SELECT rows::text FROM data_archive WHERE source = 'delete_solution' AND table_name = 'dsa_solutions'").fetch_one(&db).await.unwrap();
    assert!(kept.0.contains("def f(): return 42"), "{}", kept.0);

    // A track reset: the attempts and the review are in the archive before they go.
    call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/log", Some(json!({ "grade": "good" }))).await;
    let (_, o) = call(&app, Method::GET, "/api/dsa", None).await;
    let slug = o["patterns"][0]["slug"].as_str().unwrap().to_owned();
    assert_eq!(call(&app, Method::POST, &format!("/api/tracks/{slug}/reset"), None).await.0, StatusCode::OK);
    let live: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM attempts), (SELECT count(*) FROM reviews)").fetch_one(&db).await.unwrap();
    assert_eq!(live, (0, 0));
    let tables: Vec<(String,)> = sqlx::query_as("SELECT table_name FROM data_archive WHERE source = $1 ORDER BY table_name").bind(format!("reset_track:{slug}")).fetch_all(&db).await.unwrap();
    assert_eq!(tables.into_iter().map(|t| t.0).collect::<Vec<_>>(), ["attempts", "reviews"]);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn a_backup_restores_what_is_missing_and_never_overwrites_or_deletes(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db.clone(), root.path());
    call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/log", Some(json!({ "grade": "good" }))).await;
    call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/solutions", Some(json!({ "label": "mine", "code": "x = 1", "notes": "" }))).await;
    call(&app, Method::POST, "/api/prefs", Some(json!({ "key": "focus.cfg", "value": { "focus": 30 } }))).await;

    let (status, file) = call(&app, Method::GET, "/api/export", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(file["format"], "anneal-backup");
    assert_eq!(file["tables"]["attempts"].as_array().unwrap().len(), 1);
    assert_eq!(file["tables"]["dsa_solutions"].as_array().unwrap().len(), 1);

    // Importing into a database that already has it all changes nothing.
    let (_, same) = call(&app, Method::POST, "/api/import", Some(file.clone())).await;
    assert_eq!(same["added"]["attempts"], 0);
    assert_eq!(same["added"]["reviews"], 0);

    // After losing the rows (by hand, in this test only), the import brings them back and a new log still works.
    sqlx::query("DELETE FROM reviews").execute(&db).await.unwrap();
    sqlx::query("DELETE FROM attempts").execute(&db).await.unwrap();
    sqlx::query("DELETE FROM dsa_solutions").execute(&db).await.unwrap();
    let (status, back) = call(&app, Method::POST, "/api/import", Some(file)).await;
    assert_eq!(status, StatusCode::OK, "{back}");
    assert_eq!((back["added"]["attempts"].clone(), back["added"]["reviews"].clone(), back["added"]["dsa_solutions"].clone()), (json!(1), json!(1), json!(1)));
    let (_, log) = call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/log", Some(json!({ "grade": "hard" }))).await;
    assert!(log["due"].is_string(), "a new attempt after an import must not collide with the imported ids: {log}");
    let n: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM attempts), (SELECT count(*) FROM dsa_solutions)").fetch_one(&db).await.unwrap();
    assert_eq!(n, (2, 1));

    assert_eq!(call(&app, Method::POST, "/api/import", Some(json!({ "format": "other" }))).await.0, StatusCode::BAD_REQUEST);
    let (_, prefs) = call(&app, Method::GET, "/api/prefs", None).await;
    assert_eq!(prefs["focus.cfg"]["focus"], 30);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn premium_is_listed_but_counted_only_when_chosen_while_what_was_done_always_counts(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db.clone(), root.path());
    let sum = |o: &Value, key: &str| o["patterns"].as_array().unwrap().iter().map(|p| p[key].as_u64().unwrap()).sum::<u64>();

    // Default: premium is listed but not in the totals.
    let (_, o) = call(&app, Method::GET, "/api/dsa", None).await;
    assert_eq!(o["problems"].as_array().unwrap().len(), 6, "still listed");
    assert_eq!(sum(&o, "total"), 5, "the premium problem is not counted");

    // Completing the premium problem counts toward the streak and the activity, but not toward the totals.
    call(&app, Method::POST, "/api/dsa/problems/lc-encode-and-decode-strings/log", Some(json!({ "grade": "good" }))).await;
    let (_, a) = call(&app, Method::GET, "/api/activity?sections=D", None).await;
    assert_eq!((a["streak"].as_u64(), a["week_solved"].as_u64()), (Some(1), Some(1)));
    let (_, o) = call(&app, Method::GET, "/api/dsa", None).await;
    assert_eq!((sum(&o, "solved"), sum(&o, "total")), (0, 5));
    assert_eq!(problem_of(&o, "lc-encode-and-decode-strings")["state"]["solved"], true, "what was logged is kept");
    let (_, tracks) = call(&app, Method::GET, "/api/tracks", None).await;
    let dsa_solved: u64 = tracks.as_array().unwrap().iter().filter(|t| t["section"] == "D").map(|t| t["solved"].as_u64().unwrap()).sum();
    assert_eq!(dsa_solved, 0);

    // Counting premium puts it into the totals; the logged solve is there from the start.
    let (_, s) = call(&app, Method::GET, "/api/settings", None).await;
    let mut srs = s["srs"].clone();
    srs["goal"]["free_only"] = json!(false);
    assert_eq!(call(&app, Method::PUT, "/api/settings/srs", Some(srs)).await.0, StatusCode::OK);
    let (_, o) = call(&app, Method::GET, "/api/dsa", None).await;
    assert_eq!((sum(&o, "solved"), sum(&o, "total")), (1, 6));
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn the_review_queue_offers_upcoming_reviews_to_do_early(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db.clone(), root.path());
    call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/log", Some(json!({ "grade": "good" }))).await;
    call(&app, Method::POST, "/api/dsa/problems/lc-valid-palindrome/log", Some(json!({ "grade": "hard" }))).await;
    let (status, q) = call(&app, Method::GET, "/api/dsa/review", None).await;
    assert_eq!(status, StatusCode::OK);
    // Nothing is due yet, so both are offered ahead, soonest first, each with when it is due and what every grade schedules.
    let ahead = q["ahead"].as_array().unwrap();
    assert_eq!(ahead.len(), 2, "{q}");
    assert!(ahead.iter().all(|i| i["due"].is_string() && i["previews"]["good"]["days"].is_number() && i["problem"]["title"].is_string()));
    assert!(ahead[0]["due"].as_str() <= ahead[1]["due"].as_str());
    assert_eq!(q["items"].as_array().unwrap().len(), 0);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn a_log_keeps_how_many_hints_were_opened(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db.clone(), root.path());
    call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/log", Some(json!({ "grade": "hard", "hints": 3 }))).await;
    call(&app, Method::POST, "/api/dsa/problems/lc-valid-palindrome/log", Some(json!({ "grade": "good" }))).await;
    let rows: Vec<(String, i32)> = sqlx::query_as("SELECT problem_id, hints_revealed FROM attempts ORDER BY id").fetch_all(&db).await.unwrap();
    assert_eq!(rows, [("lc-two-sum".to_owned(), 3), ("lc-valid-palindrome".to_owned(), 0)]);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn a_reason_is_kept_with_the_latest_attempt_and_a_day_lists_what_was_solved(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db.clone(), root.path());
    call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/log", Some(json!({ "grade": "again" }))).await;
    let (status, _) = call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/why", Some(json!({ "why": "edge_cases" }))).await;
    assert_eq!(status, StatusCode::OK);
    let why: (Option<String>,) = sqlx::query_as("SELECT why FROM attempts WHERE problem_id = 'lc-two-sum'").fetch_one(&db).await.unwrap();
    assert_eq!(why.0.as_deref(), Some("edge_cases"));
    assert_eq!(call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/why", Some(json!({ "why": "bad mood" }))).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(call(&app, Method::POST, "/api/dsa/problems/lc-valid-palindrome/why", Some(json!({ "why": "idea" }))).await.0, StatusCode::NOT_FOUND, "no attempt to give a reason for");

    call(&app, Method::POST, "/api/dsa/problems/lc-valid-palindrome/log", Some(json!({ "grade": "good" }))).await;
    let today = chrono::Local::now().date_naive();
    let (status, day) = call(&app, Method::GET, &format!("/api/day/{today}"), None).await;
    assert_eq!(status, StatusCode::OK);
    let titles: Vec<&str> = day["solves"].as_array().unwrap().iter().map(|s| s["title"].as_str().unwrap()).collect();
    assert_eq!(titles.len(), 1, "the attempted one is not a solve: {day}");
    assert_eq!(call(&app, Method::GET, "/api/day/not-a-date", None).await.0, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn the_calendar_feed_needs_its_token_and_lists_the_coming_reviews(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db.clone(), root.path());
    call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/log", Some(json!({ "grade": "good" }))).await;
    let (_, none) = call(&app, Method::GET, "/api/feed", None).await;
    assert_eq!(none["path"], Value::Null);
    assert_eq!(call(&app, Method::GET, "/api/feed/guess.ics", None).await.0, StatusCode::NOT_FOUND, "no token yet, no feed");

    let (_, made) = call(&app, Method::POST, "/api/feed", None).await;
    let path = made["path"].as_str().unwrap().to_owned();
    let res = app.clone().oneshot(Request::builder().uri(&path).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["content-type"], "text/calendar; charset=utf-8");
    let text = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(text.starts_with("BEGIN:VCALENDAR") && text.contains("Two Sum") && text.contains("1 review due") && text.contains("RRULE:FREQ=WEEKLY"), "{text}");
    assert_eq!(call(&app, Method::GET, "/api/feed/not-the-token.ics", None).await.0, StatusCode::NOT_FOUND);

    // A new token replaces the old one: the old address stops working.
    let (_, again) = call(&app, Method::POST, "/api/feed", None).await;
    assert_ne!(again["path"], made["path"]);
    let old = app.oneshot(Request::builder().uri(&path).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(old.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn the_week_summary_describes_last_monday_to_sunday(db: PgPool) {
    use chrono::{Datelike, Days, TimeZone};
    let root = dsa_root(None);
    let app = test_app_with(db.clone(), root.path());
    let today = chrono::Local::now().date_naive();
    let monday = today - Days::new(u64::from(today.weekday().num_days_from_monday()));
    let at = |d: chrono::NaiveDate| chrono::Local.from_local_datetime(&d.and_hms_opt(12, 0, 0).unwrap()).earliest().unwrap().with_timezone(&chrono::Utc);
    let wed = at(monday - Days::new(5));
    for (problem, solved, assisted) in [("lc-two-sum", true, false), ("lc-valid-palindrome", true, true), ("lc-contains-duplicate", false, true)] {
        sqlx::query("INSERT INTO attempts (problem_id, kind, started_at, solved_at, assisted) VALUES ($1, 'practice', $2, CASE WHEN $3 THEN $2 END, $4)").bind(problem).bind(wed).bind(solved).bind(assisted).execute(&db).await.unwrap();
    }
    // this week's solve is not part of last week
    call(&app, Method::POST, "/api/dsa/problems/lc-encode-and-decode-strings/log", Some(json!({ "grade": "good" }))).await;
    let (status, w) = call(&app, Method::GET, "/api/week-summary", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!((w["solved"].clone(), w["unassisted"].clone(), w["attempted"].clone(), w["active_days"].clone()), (json!(2), json!(1), json!(1), json!(1)), "{w}");
    assert!(w["weakest"]["name"].is_string(), "{w}");
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn a_company_plan_is_made_beside_the_main_plan_and_is_not_duplicated(db: PgPool) {
    use chrono::Days;
    let root = dsa_root(None);
    let app = test_app_with(db.clone(), root.path());
    let today = chrono::Local::now().date_naive();
    async fn view(app: &Router) -> (StatusCode, Value) {
        call(app, Method::GET, "/api/plan?from=2026-01-01&to=2026-01-02", None).await
    }

    let interview = today + Days::new(30);
    let (status, made) = call(&app, Method::POST, "/api/plans/company", Some(json!({ "company": "Amazon", "interview": interview }))).await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert_eq!(made["created"], true);
    let (_, v) = view(&app).await;
    assert_eq!(v["active"], made["id"], "the new plan is switched to");
    let plans = v["plans"].as_array().unwrap();
    let company = plans.iter().find(|p| p["name"] == "Interview: Amazon").expect("the company plan");
    assert_eq!(company["rules"]["companies"], json!(["Amazon"]));
    assert_eq!((company["rules"]["order"].clone(), company["rules"]["finish_by"].clone()), (json!("company"), json!(interview.to_string())));
    assert!(plans.iter().any(|p| p["id"] == "default" && p["rules"]["companies"] == json!([])), "the main plan is untouched");

    // Asking again updates it, it does not add a second.
    let (_, again) = call(&app, Method::POST, "/api/plans/company", Some(json!({ "company": "Amazon", "recent_only": false }))).await;
    assert_eq!((again["created"].clone(), again["id"].clone()), (json!(false), made["id"].clone()));
    let (_, v) = view(&app).await;
    assert_eq!(v["plans"].as_array().unwrap().len(), 2);
    assert_eq!(v["plans"].as_array().unwrap().iter().find(|p| p["name"] == "Interview: Amazon").unwrap()["rules"]["recent_only"], false);

    assert_eq!(call(&app, Method::POST, "/api/plans/company", Some(json!({ "company": "Nonesuch Ltd" }))).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(call(&app, Method::POST, "/api/plans/company", Some(json!({ "company": "Amazon", "interview": today }))).await.0, StatusCode::BAD_REQUEST);

    // Back to the main plan.
    assert_eq!(call(&app, Method::PUT, "/api/plans/active", Some(json!({ "id": "default" }))).await.0, StatusCode::OK);
    let (_, v) = view(&app).await;
    assert_eq!(v["active"], "default");
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn the_forecast_follows_the_review_and_is_empty_without_one(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db.clone(), root.path());
    let (_, none) = call(&app, Method::GET, "/api/dsa/problems/lc-two-sum/forecast", None).await;
    assert_eq!(none["points"], Value::Null);
    call(&app, Method::POST, "/api/dsa/problems/lc-two-sum/log", Some(json!({ "grade": "good" }))).await;
    let (status, f) = call(&app, Method::GET, "/api/dsa/problems/lc-two-sum/forecast", None).await;
    assert_eq!(status, StatusCode::OK);
    let points: Vec<f64> = f["points"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert_eq!(points.len(), 61);
    assert!(points[0] > 0.9 && points.windows(2).all(|w| w[1] <= w[0] + 1e-6), "memory only fades while nothing is reviewed: {points:?}");
    assert!(f["due_in"].as_i64().unwrap() >= 1 && f["retention"].as_f64().unwrap() > 0.5);
    assert_eq!(call(&app, Method::GET, "/api/dsa/problems/lc-nothing/forecast", None).await.0, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn large_answers_are_compressed_for_a_client_that_accepts_it(db: PgPool) {
    let root = dsa_root(None);
    let app = test_app_with(db.clone(), root.path());
    let ask = |encoding: Option<&str>| {
        let mut req = Request::builder().uri("/api/dsa/mock");
        if let Some(e) = encoding {
            req = req.header("accept-encoding", e);
        }
        app.clone().oneshot(req.body(Body::empty()).unwrap())
    };
    let plain = ask(None).await.unwrap();
    assert!(plain.headers().get("content-encoding").is_none());
    let plain_len = plain.into_body().collect().await.unwrap().to_bytes().len();
    let zipped = ask(Some("gzip")).await.unwrap();
    assert_eq!(zipped.headers()["content-encoding"], "gzip");
    let zipped_len = zipped.into_body().collect().await.unwrap().to_bytes().len();
    assert!(zipped_len * 2 < plain_len, "{zipped_len} bytes compressed against {plain_len}");
}
