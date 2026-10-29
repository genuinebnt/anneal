//! Courses (courses/<id>): the tree with the owner's progress, one stage's page, hints and the stage solution (both marked as
//! assistance), and the CLI's reports: `anneal course test` posts each run, `anneal course solutions push` uploads the diffs.

use std::collections::HashMap;

use anneal_content::course::{Course, Stage};
use axum::Json;
use axum::extract::{Path, State};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::AppState;
use crate::error::{ApiError, ApiResult};

const MAX_SOLUTION: usize = 400_000;

fn find<'a>(s: &'a AppState, id: &str) -> ApiResult<&'a Course> {
    s.courses.iter().find(|c| c.id == id).ok_or_else(|| ApiError::NotFound(format!("course {id}")))
}

fn find_stage<'a>(c: &'a Course, id: &str) -> ApiResult<&'a Stage> {
    c.stage(id).ok_or_else(|| ApiError::NotFound(format!("stage {id}")))
}

#[derive(sqlx::FromRow)]
struct StateRow {
    stage_id: String,
    hints_revealed: i32,
    solution_revealed: bool,
    solved_at: Option<DateTime<Utc>>,
    assisted: bool,
}

async fn states(s: &AppState, course: &str) -> ApiResult<HashMap<String, StateRow>> {
    let rows: Vec<StateRow> = sqlx::query_as(
        "SELECT stage_id, hints_revealed, solution_revealed, solved_at, assisted FROM course_stage_state WHERE course = $1",
    )
    .bind(course)
    .fetch_all(&s.db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.stage_id.clone(), r)).collect())
}

fn status(row: Option<&StateRow>) -> &'static str {
    match row {
        Some(r) if r.solved_at.is_some() && r.assisted => "assisted",
        Some(r) if r.solved_at.is_some() => "solved",
        _ => "todo",
    }
}

/// `GET /api/courses/{course}`: projects → modules → stages with the owner's progress.
pub async fn overview(State(s): State<AppState>, Path(course): Path<String>) -> ApiResult<Json<Value>> {
    let c = find(&s, &course)?;
    let st = states(&s, &c.id).await?;
    let runs: Vec<(String, i64)> = sqlx::query_as("SELECT stage_id, count(*) FROM course_runs WHERE course = $1 GROUP BY stage_id")
        .bind(&c.id)
        .fetch_all(&s.db)
        .await?;
    let runs: HashMap<String, i64> = runs.into_iter().collect();
    let mut done = 0;
    let mut challenges = 0;
    let mut challenges_done = 0;
    let projects: Vec<Value> = c
        .projects
        .iter()
        .map(|p| {
            let modules: Vec<Value> = c
                .modules
                .iter()
                .filter(|m| m.project == p.number)
                .map(|m| {
                    let stages: Vec<Value> = m
                        .stages
                        .iter()
                        .map(|x| {
                            let state = status(st.get(&x.id));
                            // challenges are extra practice: their own tally, not the course's
                            if x.kind == "challenge" {
                                challenges += 1;
                                if state != "todo" {
                                    challenges_done += 1;
                                }
                            } else if state != "todo" {
                                done += 1;
                            }
                            json!({ "id": x.id, "title": x.title, "kind": x.kind, "difficulty": x.difficulty, "rank": x.rank, "beyond": x.beyond,
                                    "state": state, "runs": runs.get(&x.id).copied().unwrap_or(0) })
                        })
                        .collect();
                    json!({ "code": m.code, "title": m.title, "summary": m.summary, "planned": m.planned, "optional": m.optional, "beyond": m.beyond, "stages": stages })
                })
                .collect();
            json!({ "number": p.number, "title": p.title, "planned": p.planned, "modules": modules })
        })
        .collect();
    let total = c.stages().filter(|x| x.kind != "challenge").count();
    // the next stage is the first undone one in the course's own order (the optional primer, project 0, comes last in it)
    let current: Option<&str> = c.modules.iter().filter(|m| !m.planned && !m.optional).flat_map(|m| m.stages.iter()).filter(|x| x.kind != "challenge").find(|x| status(st.get(&x.id)) == "todo").map(|x| x.id.as_str());
    // The last run the CLI reported, from any laptop: tells you whether the CLI you are using is signed in to this app.
    let last_run: Option<(String, bool, i32, i32, DateTime<Utc>)> =
        sqlx::query_as("SELECT stage_id, ok, passed, total, at FROM course_runs WHERE course = $1 ORDER BY at DESC, id DESC LIMIT 1").bind(&c.id).fetch_optional(&s.db).await?;
    let last_run = last_run.map(|(stage, ok, passed, total, at)| json!({ "stage_id": stage, "ok": ok, "passed": passed, "total": total, "at": at }));
    Ok(Json(json!({ "id": c.id, "title": c.title, "total": total, "done": done, "challenges": challenges, "challenges_done": challenges_done, "current": current, "last_run": last_run, "projects": projects })))
}

#[derive(Serialize, sqlx::FromRow)]
struct RunView {
    id: i64,
    ok: bool,
    passed: i32,
    total: i32,
    tests: Value,
    problem: Option<String>,
    commit_sha: Option<String>,
    duration_ms: i32,
    at: DateTime<Utc>,
}

/// `GET /api/courses/{course}/stages/{id}`: everything the stage page shows. Hints and the solution come only once opened.
pub async fn stage(State(s): State<AppState>, Path((course, id)): Path<(String, String)>) -> ApiResult<Json<Value>> {
    let c = find(&s, &course)?;
    let x = find_stage(c, &id)?;
    let m = c.module(&x.module).ok_or_else(|| ApiError::NotFound(format!("module {}", x.module)))?;
    let all: Vec<&Stage> = c.stages().collect();
    let at = all.iter().position(|y| y.id == id).unwrap_or(0);
    let st = states(&s, &c.id).await?;
    let me = st.get(&id);
    let solved = me.is_some_and(|r| r.solved_at.is_some());
    let last: Option<RunView> = sqlx::query_as(
        "SELECT id, ok, passed, total, tests, problem, commit_sha, duration_ms, at FROM course_runs
         WHERE course = $1 AND stage_id = $2 ORDER BY at DESC, id DESC LIMIT 1",
    )
    .bind(&c.id)
    .bind(&id)
    .fetch_optional(&s.db)
    .await?;
    let stored: Option<(Value,)> = sqlx::query_as("SELECT files FROM course_solutions WHERE course = $1 AND stage_id = $2")
        .bind(&c.id)
        .bind(&id)
        .fetch_optional(&s.db)
        .await?;
    // a run the CLI started and has not reported yet (ten minutes at most: a run that died leaves no report)
    let running: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT started_at FROM course_run_starts WHERE course = $1 AND stage_id = $2 AND started_at > now() - interval '10 minutes'",
    )
    .bind(&c.id)
    .bind(&id)
    .fetch_optional(&s.db)
    .await?;
    let read: std::collections::HashSet<String> = read_concepts(&s, &c.id).await?;
    // The most recent runs, newest first, for the Last run tab's history.
    let runs: Vec<RunView> = sqlx::query_as(
        "SELECT id, ok, passed, total, tests, problem, commit_sha, duration_ms, at FROM course_runs
         WHERE course = $1 AND stage_id = $2 ORDER BY at DESC, id DESC LIMIT 10",
    )
    .bind(&c.id)
    .bind(&id)
    .fetch_all(&s.db)
    .await?;
    let revealed_hints = me.map_or(0, |r| r.hints_revealed.max(0) as usize).min(x.hints.len());
    let solution_open = me.is_some_and(|r| r.solution_revealed) || solved;
    let pn = |i: Option<usize>| i.and_then(|i| all.get(i)).map(|y| json!({ "id": y.id, "title": y.title, "rank": y.rank }));
    let module_stages: Vec<Value> = m
        .stages
        .iter()
        .map(|y| json!({ "id": y.id, "title": y.title, "kind": y.kind, "difficulty": y.difficulty, "rank": y.rank, "state": status(st.get(&y.id)) }))
        .collect();
    Ok(Json(json!({
        "course": { "id": c.id, "title": c.title, "total": all.len() },
        "stage": { "id": x.id, "title": x.title, "learn": x.learn, "kind": x.kind, "difficulty": x.difficulty, "tests": x.tests, "rank": x.rank,
                   "intro": x.intro, "sections": x.sections, "test_sources": x.test_sources },
        "module": { "code": m.code, "title": m.title, "summary": m.summary, "project": m.project, "stages": module_stages,
                    "lectures": m.lectures, "bustub": m.bustub, "resources": m.resources },
        "concepts": x.concepts.iter().map(|id| (id, true)).chain(x.concepts_optional.iter().map(|id| (id, false)))
            .filter_map(|(id, required)| c.concept(id).map(|k| json!({ "id": k.id, "title": k.title, "summary": k.summary, "minutes": k.minutes, "required": required, "read": read.contains(&k.id) })))
            .collect::<Vec<_>>(),
        "running": running,
        "prev": pn(at.checked_sub(1)),
        "next": pn(Some(at + 1)),
        "state": status(me),
        "hints": { "total": x.hints.len(), "revealed": x.hints.iter().take(revealed_hints).collect::<Vec<_>>(),
                   "titles": x.hints.iter().map(|h| &h.title).collect::<Vec<_>>() },
        "solution": { "available": stored.is_some() && x.kind != "challenge", "open": solution_open && stored.is_some() && x.kind != "challenge",
                      "files": if solution_open { stored.map(|r| r.0) } else { None } },
        "last_run": last,
        "runs": runs,
    })))
}

/// `GET /api/courses/{course}/concepts/{id}`: one concept page, with the stages that point to it.
pub async fn concept(State(s): State<AppState>, Path((course, id)): Path<(String, String)>) -> ApiResult<Json<Value>> {
    let c = find(&s, &course)?;
    let k = c.concept(&id).ok_or_else(|| ApiError::NotFound(format!("concept {id}")))?;
    let used_in: Vec<Value> = c
        .stages()
        .filter(|x| x.concepts.contains(&id) || x.concepts_optional.contains(&id))
        .map(|x| json!({ "id": x.id, "title": x.title, "rank": x.rank, "module": x.module }))
        .collect();
    let read = read_concepts(&s, &c.id).await?.contains(&k.id);
    Ok(Json(json!({ "course": { "id": c.id, "title": c.title }, "concept": k, "used_in": used_in, "read": read })))
}

/// The ids of the concepts marked as read.
async fn read_concepts(s: &AppState, course: &str) -> ApiResult<std::collections::HashSet<String>> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT concept FROM course_concept_state WHERE course = $1").bind(course).fetch_all(&s.db).await?;
    Ok(rows.into_iter().map(|(c,)| c).collect())
}

#[derive(Deserialize)]
pub struct ReadBody {
    read: bool,
}

/// `PUT …/concepts/{id}/read`: marks a concept article as read, or as unread again.
pub async fn set_concept_read(State(s): State<AppState>, Path((course, id)): Path<(String, String)>, Json(body): Json<ReadBody>) -> ApiResult<Json<Value>> {
    let c = find(&s, &course)?;
    c.concept(&id).ok_or_else(|| ApiError::NotFound(format!("concept {id}")))?;
    if body.read {
        sqlx::query("INSERT INTO course_concept_state (course, concept) VALUES ($1, $2) ON CONFLICT DO NOTHING").bind(&c.id).bind(&id).execute(&s.db).await?;
    } else {
        sqlx::query("DELETE FROM course_concept_state WHERE course = $1 AND concept = $2").bind(&c.id).bind(&id).execute(&s.db).await?;
    }
    Ok(Json(json!({ "read": body.read })))
}

/// `POST …/hints`: opens the next hint (marks the stage assisted unless it has already passed).
pub async fn reveal_hint(State(s): State<AppState>, Path((course, id)): Path<(String, String)>) -> ApiResult<Json<Value>> {
    let c = find(&s, &course)?;
    let x = find_stage(c, &id)?;
    if x.hints.is_empty() {
        return Err(ApiError::BadRequest(format!("{id} has no hints written yet")));
    }
    sqlx::query(
        "INSERT INTO course_stage_state (course, stage_id, hints_revealed, assisted) VALUES ($1, $2, 1, true)
         ON CONFLICT (course, stage_id) DO UPDATE SET
             hints_revealed = LEAST(course_stage_state.hints_revealed + 1, $3),
             assisted = course_stage_state.assisted OR course_stage_state.solved_at IS NULL",
    )
    .bind(&c.id)
    .bind(&id)
    .bind(x.hints.len() as i32)
    .execute(&s.db)
    .await?;
    stage(State(s), Path((course, id))).await
}

/// `POST …/solution`: shows the stage's solution (marks the stage assisted unless it has already passed).
pub async fn reveal_solution(State(s): State<AppState>, Path((course, id)): Path<(String, String)>) -> ApiResult<Json<Value>> {
    let c = find(&s, &course)?;
    find_stage(c, &id)?;
    let has: Option<i32> = sqlx::query_scalar("SELECT 1 FROM course_solutions WHERE course = $1 AND stage_id = $2")
        .bind(&c.id)
        .bind(&id)
        .fetch_optional(&s.db)
        .await?;
    if has.is_none() {
        return Err(ApiError::NotFound(format!("a solution for {id}; upload them with `anneal course solutions push`")));
    }
    sqlx::query(
        "INSERT INTO course_stage_state (course, stage_id, solution_revealed, assisted) VALUES ($1, $2, true, true)
         ON CONFLICT (course, stage_id) DO UPDATE SET solution_revealed = true,
             assisted = course_stage_state.assisted OR course_stage_state.solved_at IS NULL",
    )
    .bind(&c.id)
    .bind(&id)
    .execute(&s.db)
    .await?;
    stage(State(s), Path((course, id))).await
}

#[derive(Deserialize)]
pub struct TestReport {
    name: String,
    ok: bool,
    #[serde(default)]
    detail: String,
}

#[derive(Deserialize)]
pub struct RunBody {
    stage_id: String,
    #[serde(default)]
    tests: Vec<TestReport>,
    /// Compiler errors or a timeout: no test ran.
    #[serde(default)]
    problem: Option<String>,
    #[serde(default)]
    commit: Option<String>,
    #[serde(default)]
    duration_ms: u32,
}

/// `POST …/runs`: one test run of a stage, reported by the CLI. A passing run marks the stage solved the first time.
pub async fn post_run(State(s): State<AppState>, Path(course): Path<String>, Json(b): Json<RunBody>) -> ApiResult<Json<Value>> {
    let c = find(&s, &course)?;
    find_stage(c, &b.stage_id)?;
    if b.tests.len() > 2_000 || b.tests.iter().any(|t| t.name.len() > 300 || t.detail.len() > 4_000) {
        return Err(ApiError::BadRequest("that run report is too large".into()));
    }
    let passed = b.tests.iter().filter(|t| t.ok).count() as i32;
    let total = b.tests.len() as i32;
    let ok = b.problem.is_none() && total > 0 && passed == total;
    let tests: Vec<Value> = b.tests.iter().map(|t| json!({ "name": t.name, "ok": t.ok, "detail": t.detail })).collect();
    sqlx::query(
        "INSERT INTO course_runs (course, stage_id, ok, passed, total, tests, problem, commit_sha, duration_ms)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(&c.id)
    .bind(&b.stage_id)
    .bind(ok)
    .bind(passed)
    .bind(total)
    .bind(Value::Array(tests))
    .bind(&b.problem)
    .bind(&b.commit)
    .bind(b.duration_ms as i32)
    .execute(&s.db)
    .await?;
    sqlx::query("DELETE FROM course_run_starts WHERE course = $1 AND stage_id = $2").bind(&c.id).bind(&b.stage_id).execute(&s.db).await?;
    if ok {
        sqlx::query(
            "INSERT INTO course_stage_state (course, stage_id, solved_at) VALUES ($1, $2, now())
             ON CONFLICT (course, stage_id) DO UPDATE SET solved_at = COALESCE(course_stage_state.solved_at, now())",
        )
        .bind(&c.id)
        .bind(&b.stage_id)
        .execute(&s.db)
        .await?;
    }
    Ok(Json(json!({ "ok": ok, "passed": passed, "total": total })))
}

#[derive(Deserialize)]
pub struct SolutionFile {
    path: String,
    /// Diff lines with a leading ' ', '+' or '-'.
    lines: Vec<String>,
}

#[derive(Deserialize)]
pub struct SolutionsBody {
    stages: HashMap<String, Vec<SolutionFile>>,
}

/// `PUT …/solutions`: replaces the stored solutions of the listed stages (sent by `anneal course solutions push`).
pub async fn put_solutions(State(s): State<AppState>, Path(course): Path<String>, Json(b): Json<SolutionsBody>) -> ApiResult<Json<Value>> {
    let c = find(&s, &course)?;
    let mut n = 0;
    for (id, files) in &b.stages {
        find_stage(c, id)?;
        let size: usize = files.iter().map(|f| f.path.len() + f.lines.iter().map(|l| l.len() + 1).sum::<usize>()).sum();
        if size > MAX_SOLUTION {
            return Err(ApiError::BadRequest(format!("{id}: the solution is too large")));
        }
        let files: Vec<Value> = files.iter().map(|f| json!({ "path": f.path, "lines": f.lines })).collect();
        sqlx::query(
            "INSERT INTO course_solutions (course, stage_id, files) VALUES ($1, $2, $3)
             ON CONFLICT (course, stage_id) DO UPDATE SET files = $3, updated_at = now()",
        )
        .bind(&c.id)
        .bind(id)
        .bind(Value::Array(files))
        .execute(&s.db)
        .await?;
        n += 1;
    }
    Ok(Json(json!({ "stored": n })))
}

#[derive(Deserialize)]
pub struct StartBody {
    stage_id: String,
}

/// `POST …/runs/start`: the CLI is about to run a stage's tests; the stage page shows "Running…" until the report arrives.
pub async fn start_run(State(s): State<AppState>, Path(course): Path<String>, Json(b): Json<StartBody>) -> ApiResult<Json<Value>> {
    let c = find(&s, &course)?;
    find_stage(c, &b.stage_id)?;
    sqlx::query(
        "INSERT INTO course_run_starts (course, stage_id) VALUES ($1, $2)
         ON CONFLICT (course, stage_id) DO UPDATE SET started_at = now()",
    )
    .bind(&c.id)
    .bind(&b.stage_id)
    .execute(&s.db)
    .await?;
    Ok(Json(json!({ "started": true })))
}

#[derive(Deserialize)]
pub struct ResetBody {
    /// The whole course.
    #[serde(default)]
    all: bool,
    /// One module, by code (`1a`).
    #[serde(default)]
    module: Option<String>,
    /// One project, by number (`1` is the buffer pool).
    #[serde(default)]
    project: Option<u32>,
}

/// `POST …/reset`: forgets the progress of the whole course, of one project or of one module: solved marks, runs, opened hints and solutions.
/// Resetting the whole course also marks every concept article unread. Code in the learner's repo is not touched.
pub async fn reset_progress(State(s): State<AppState>, Path(course): Path<String>, Json(b): Json<ResetBody>) -> ApiResult<Json<Value>> {
    let c = find(&s, &course)?;
    let ids: Vec<String> = match (b.all, &b.module, b.project) {
        (true, None, None) => c.stages().map(|x| x.id.clone()).collect(),
        (false, Some(code), None) => {
            let m = c.module(code).ok_or_else(|| ApiError::NotFound(format!("module {code}")))?;
            m.stages.iter().map(|x| x.id.clone()).collect()
        }
        (false, None, Some(n)) => {
            if !c.projects.iter().any(|p| p.number == n) {
                return Err(ApiError::NotFound(format!("project {n}")));
            }
            c.modules.iter().filter(|m| m.project == n).flat_map(|m| m.stages.iter().map(|x| x.id.clone())).collect()
        }
        _ => return Err(ApiError::BadRequest("say what to reset: {\"all\": true}, {\"module\": \"1a\"} or {\"project\": 1}".into())),
    };
    let mut tx = s.db.begin().await?;
    let source = format!("reset_course:{}", c.id);
    for table in ["course_runs", "course_stage_state", "course_run_starts"] {
        crate::store::archive_course_rows(&mut tx, &source, table, &c.id, Some(&ids)).await?;
    }
    if b.all {
        crate::store::archive_course_rows(&mut tx, &source, "course_concept_state", &c.id, None).await?;
    }
    let runs = sqlx::query("DELETE FROM course_runs WHERE course = $1 AND stage_id = ANY($2)").bind(&c.id).bind(&ids).execute(&mut *tx).await?.rows_affected();
    let stages = sqlx::query("DELETE FROM course_stage_state WHERE course = $1 AND stage_id = ANY($2)").bind(&c.id).bind(&ids).execute(&mut *tx).await?.rows_affected();
    sqlx::query("DELETE FROM course_run_starts WHERE course = $1 AND stage_id = ANY($2)").bind(&c.id).bind(&ids).execute(&mut *tx).await?;
    if b.all {
        sqlx::query("DELETE FROM course_concept_state WHERE course = $1").bind(&c.id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "stages": ids.len(), "stage_states_removed": stages, "runs_removed": runs })))
}

/// `GET …/repo`: the repository that holds the learner's work, if `anneal course remote` recorded one.
pub async fn get_repo(State(s): State<AppState>, Path(course): Path<String>) -> ApiResult<Json<Value>> {
    let c = find(&s, &course)?;
    let url: Option<String> = sqlx::query_scalar("SELECT url FROM course_repos WHERE course = $1").bind(&c.id).fetch_optional(&s.db).await?;
    Ok(Json(json!({ "url": url })))
}

#[derive(Deserialize)]
pub struct RepoBody {
    url: String,
}

/// `PUT …/repo`: records the repository (sent by `anneal course remote`).
pub async fn put_repo(State(s): State<AppState>, Path(course): Path<String>, Json(b): Json<RepoBody>) -> ApiResult<Json<Value>> {
    let c = find(&s, &course)?;
    let url = b.url.trim();
    if url.is_empty() || url.len() > 500 || url.chars().any(char::is_whitespace) {
        return Err(ApiError::BadRequest("a repository URL has no spaces and is at most 500 characters".into()));
    }
    sqlx::query("INSERT INTO course_repos (course, url) VALUES ($1, $2) ON CONFLICT (course) DO UPDATE SET url = $2, updated_at = now()")
        .bind(&c.id)
        .bind(url)
        .execute(&s.db)
        .await?;
    Ok(Json(json!({ "url": url })))
}

/// The solutions a course ships with: `courses/<id>/solutions.json`, written by `anneal course solutions --out` (CI does this at deploy time
/// from the reference, which is not in the image). Loaded at start-up so nobody has to upload them. Stages the course does not have are
/// skipped, a row is only rewritten when it changed, and a missing or unreadable file changes nothing. Returns how many rows were written.
pub async fn seed_solutions(db: &sqlx::PgPool, course: &Course, dir: &std::path::Path) -> anyhow::Result<usize> {
    let path = dir.join("solutions.json");
    let Ok(text) = std::fs::read_to_string(&path) else { return Ok(0) };
    let stages: HashMap<String, Vec<SolutionFileOut>> = serde_json::from_str(&text).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
    let mut written = 0;
    for (id, files) in stages {
        if course.stage(&id).is_none() || files.is_empty() || course.stage(&id).is_some_and(|x| x.kind == "challenge") {
            continue;
        }
        let size: usize = files.iter().map(|f| f.path.len() + f.lines.iter().map(|l| l.len() + 1).sum::<usize>()).sum();
        if size > MAX_SOLUTION {
            tracing::warn!(stage = %id, "a shipped solution is too large and was skipped");
            continue;
        }
        let value = serde_json::to_value(&files)?;
        let r = sqlx::query(
            "INSERT INTO course_solutions (course, stage_id, files) VALUES ($1, $2, $3)
             ON CONFLICT (course, stage_id) DO UPDATE SET files = $3, updated_at = now() WHERE course_solutions.files IS DISTINCT FROM $3",
        )
        .bind(&course.id)
        .bind(&id)
        .bind(value)
        .execute(db)
        .await?;
        written += r.rows_affected() as usize;
    }
    Ok(written)
}

#[derive(Deserialize, Serialize)]
struct SolutionFileOut {
    path: String,
    lines: Vec<String>,
}
