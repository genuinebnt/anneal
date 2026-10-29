use axum::Json;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde_json::{Value, json};

use anneal_content::{Problem, Status, Track};
use anneal_runner::{RunStatus, Submission};

use crate::AppState;
use crate::activity::{self, Activity};
use crate::progress::{self, Overview, Reviews, Stats};
use crate::reviews::Outcome;
use crate::error::{ApiError, ApiResult};
use crate::lsp::{self, SessionFiles};
use crate::store;
use crate::views::{self, CodeBody, ProblemDetail, RunOutcome, ScratchBody, TrackDetail, TrackSummary};

/// Longest editor buffer accepted, in bytes.
const MAX_CODE: usize = 256 * 1024;

pub async fn health(State(s): State<AppState>) -> Json<Value> {
    let problems: usize = s.catalog.tracks.iter().map(|t| t.problems.len()).sum();
    Json(json!({ "ok": true, "tracks": s.catalog.tracks.len(), "problems": problems }))
}

pub async fn tracks(State(s): State<AppState>) -> ApiResult<Json<Vec<TrackSummary>>> {
    let progress = store::progress(&s.db).await?;
    Ok(Json(
        s.catalog
            .tracks
            .iter()
            .map(|t| views::track_summary(t, &progress))
            .collect(),
    ))
}

#[derive(serde::Deserialize)]
pub struct ActivityQuery {
    /// Section letters, e.g. `D` or `L,S,C,Y`. All sections when absent.
    sections: Option<String>,
}

pub async fn activity(
    State(s): State<AppState>,
    Query(q): Query<ActivityQuery>,
) -> ApiResult<Json<Activity>> {
    let sections = match q.sections.as_deref() {
        Some(list) => activity::parse_sections(list),
        None => activity::parse_sections("D,L,S,C,Y,B,M"),
    };
    let progress = store::progress(&s.db).await?;
    let rows = store::activity(&s.db).await?;
    Ok(Json(activity::build(&s.catalog, &sections, &progress, &rows, activity::today())))
}

pub async fn progress(State(s): State<AppState>) -> ApiResult<Json<Overview>> {
    let rows = store::activity(&s.db).await?;
    let focus = store::focus(&s.db).await?;
    let progress = store::progress(&s.db).await?;
    Ok(Json(progress::overview(&s.catalog, &rows, &focus, &progress, activity::today())))
}

pub async fn stats(State(s): State<AppState>) -> ApiResult<Json<Stats>> {
    let rows = store::activity(&s.db).await?;
    let runs = store::run_stats(&s.db).await?;
    let focus = store::focus(&s.db).await?;
    Ok(Json(progress::stats(&s.catalog, &rows, &runs, &focus, chrono::Utc::now())))
}

pub async fn reviews(State(s): State<AppState>) -> ApiResult<Json<Reviews>> {
    let rows = store::reviews(&s.db).await?;
    Ok(Json(progress::reviews(&s.catalog, &rows, chrono::Utc::now())))
}

/// Starts a scheduled re-solve of a problem you've solved before.
pub async fn resolve(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<ProblemDetail>> {
    let (_, p) = find_open(&s, &id).await?;
    if p.meta.status != Status::Ready {
        return Err(ApiError::NotReady(id));
    }
    store::start_resolve(&s.db, &id).await?;
    Ok(Json(detail(&s, &id).await?))
}

#[derive(serde::Deserialize)]
pub struct FocusBody {
    seconds: u32,
}

/// Workspace heartbeat: seconds of active editing since the last one (capped at two minutes).
pub async fn focus(State(s): State<AppState>, Path(id): Path<String>, Json(body): Json<FocusBody>) -> ApiResult<StatusCode> {
    find(&s, &id)?;
    let seconds = body.seconds.min(120) as i32;
    if seconds > 0 {
        store::add_focus(&s.db, activity::today(), &id, seconds).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn track(
    State(s): State<AppState>,
    Path(slug): Path<String>,
) -> ApiResult<Json<TrackDetail>> {
    let t = s
        .catalog
        .track(&slug)
        .ok_or_else(|| ApiError::NotFound(format!("track {slug}")))?;
    let progress = store::progress(&s.db).await?;
    Ok(Json(views::track_detail(t, &progress)))
}

/// `POST /api/tracks/{track}/reset`: forgets the progress on every problem of one track: attempts and their runs, drafts, scratch files,
/// review schedule and focus time. The learner's written solutions (`dsa_solutions`) and the plan overrides are kept.
pub async fn reset_track(State(s): State<AppState>, Path(slug): Path<String>) -> ApiResult<Json<serde_json::Value>> {
    let t = s.catalog.track(&slug).ok_or_else(|| ApiError::NotFound(format!("track {slug}")))?;
    let ids: Vec<String> = t.problems.iter().map(|p| p.id.clone()).collect();
    let mut tx = s.db.begin().await?;
    let source = format!("reset_track:{slug}");
    for table in ["runs", "attempts", "drafts", "scratch", "reviews", "focus_time"] {
        store::archive_rows(&mut tx, &source, table, "problem_id", &ids).await?;
    }
    let attempts = sqlx::query("DELETE FROM attempts WHERE problem_id = ANY($1)").bind(&ids).execute(&mut *tx).await?.rows_affected();
    let drafts = sqlx::query("DELETE FROM drafts WHERE problem_id = ANY($1)").bind(&ids).execute(&mut *tx).await?.rows_affected();
    sqlx::query("DELETE FROM scratch WHERE problem_id = ANY($1)").bind(&ids).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM reviews WHERE problem_id = ANY($1)").bind(&ids).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM focus_time WHERE problem_id = ANY($1)").bind(&ids).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(serde_json::json!({ "problems": ids.len(), "attempts_removed": attempts, "drafts_removed": drafts })))
}

fn find<'a>(s: &'a AppState, id: &str) -> ApiResult<(&'a Track, &'a Problem)> {
    s.catalog
        .problem(id)
        .ok_or_else(|| ApiError::NotFound(format!("problem {id}")))
}

/// `find`, then refuse a practice problem that isn't open yet.
async fn find_open<'a>(s: &'a AppState, id: &str) -> ApiResult<(&'a Track, &'a Problem)> {
    let found = find(s, id)?;
    crate::practice::ensure_open(s, found.1).await?;
    Ok(found)
}

async fn detail(s: &AppState, id: &str) -> ApiResult<ProblemDetail> {
    let (t, p) = find_open(s, id).await?;
    let attempt = store::latest_attempt(&s.db, id).await?;
    let runs = match &attempt {
        Some(a) => store::runs(&s.db, a.id).await?,
        None => Vec::new(),
    };
    let draft = store::draft(&s.db, id).await?;
    let scratch = store::scratch(&s.db, id).await?;
    let solved_ever = store::ever_solved(&s.db, id).await?;
    let mut view = views::problem_detail(t, p, attempt.as_ref(), draft, scratch, runs, solved_ever);
    view.unlocked_by = crate::practice::unlock_refs(s, p);
    Ok(view)
}

pub async fn problem(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<ProblemDetail>> {
    Ok(Json(detail(&s, &id).await?))
}

fn check_code(code: &str) -> ApiResult<()> {
    if code.len() > MAX_CODE {
        return Err(ApiError::BadRequest(format!(
            "code is {} bytes; the limit is {MAX_CODE}",
            code.len()
        )));
    }
    Ok(())
}

pub async fn save_draft(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CodeBody>,
) -> ApiResult<StatusCode> {
    find_open(&s, &id).await?;
    check_code(&body.code)?;
    store::save_draft(&s.db, &id, &body.code).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Reset: drop the draft so the editor goes back to the starter.
pub async fn reset(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<ProblemDetail>> {
    find_open(&s, &id).await?;
    store::delete_draft(&s.db, &id).await?;
    Ok(Json(detail(&s, &id).await?))
}

pub async fn save_scratch(State(s): State<AppState>, Path(id): Path<String>, Json(body): Json<CodeBody>) -> ApiResult<StatusCode> {
    find_open(&s, &id).await?;
    check_code(&body.code)?;
    store::save_scratch(&s.db, &id, &body.code).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Run: builds the scratch `main.rs` against the lib.rs buffer and runs it. Not recorded as a run.
pub async fn run_scratch(State(s): State<AppState>, Path(id): Path<String>, Json(body): Json<ScratchBody>) -> ApiResult<Json<anneal_runner::ScratchResult>> {
    let (_, p) = find_open(&s, &id).await?;
    if p.meta.language != anneal_content::Language::Rust {
        return Err(ApiError::BadRequest(format!("{id} has no scratch file: Run runs its tests")));
    }
    check_code(&body.lib)?;
    check_code(&body.main)?;
    if p.meta.status != Status::Ready {
        return Err(ApiError::NotReady(id));
    }
    store::save_draft(&s.db, &id, &body.lib).await?;
    store::save_scratch(&s.db, &id, &body.main).await?;
    let result = s
        .runner
        .run_scratch(&id, &anneal_runner::Scratch { lib_rs: &body.lib, main_rs: &body.main, crates: &p.meta.crates })
        .await?;
    Ok(Json(result))
}

pub async fn run(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CodeBody>,
) -> ApiResult<Json<RunOutcome>> {
    execute(&s, &id, &body.code, false).await.map(Json)
}

pub async fn submit(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CodeBody>,
) -> ApiResult<Json<RunOutcome>> {
    execute(&s, &id, &body.code, true).await.map(Json)
}

/// Run: visible tests only. Submit: visible and hidden; all passing, with no rule broken, solves the problem.
async fn execute(s: &AppState, id: &str, code: &str, with_hidden: bool) -> ApiResult<RunOutcome> {
    let (track, p) = find_open(s, id).await?;
    check_code(code)?;
    if p.meta.status != Status::Ready {
        return Err(ApiError::NotReady(id.to_owned()));
    }
    let visible = p.files.visible_tests.as_deref().unwrap_or_default();
    let hidden = with_hidden.then(|| p.files.hidden_tests.as_deref().unwrap_or_default());
    store::save_draft(&s.db, id, code).await?;
    let result = match p.meta.language {
        anneal_content::Language::Python => s.runner.run_python(&anneal_runner::PySubmission { code, visible_tests: visible, hidden_tests: hidden }).await?,
        anneal_content::Language::Rust => {
            s.runner
                .run(
                    id,
                    &Submission {
                        lib_rs: code,
                        visible_tests: visible,
                        hidden_tests: hidden,
                        crates: &p.meta.crates,
                        perf: runner_perf(p.meta.perf),
                    },
                )
                .await?
        }
    };

    // Fix-this rules are checked against the source; the tests still run so you see both.
    let violations = match (&p.meta.rules, &p.files.starter) {
        (Some(rules), Some(starter)) => anneal_rules::check(code, starter, rules),
        _ => Vec::new(),
    };

    let mut attempt = store::current_attempt(&s.db, id).await?;
    let kind = if with_hidden { "submit" } else { "run" };
    let row = store::insert_run(&s.db, attempt.id, id, kind, code, &result, &violations).await?;
    if with_hidden && result.status == RunStatus::Passed && violations.is_empty() {
        let newly = attempt.solved_at.is_none();
        attempt = store::mark_solved(&s.db, attempt.id).await?;
        // Practice never schedules reviews (decision 23).
        if newly && track.section != anneal_content::Section::Practice {
            let outcome = if attempt.assisted { Outcome::Assisted } else { Outcome::Unassisted };
            let srs = crate::settings::srs(&s.db).await?;
            store::record_review(&s.db, id, outcome.grade(), attempt.kind == "resolve", &srs).await?;
        }
    }
    let solved_ever = store::ever_solved(&s.db, id).await?;
    Ok(RunOutcome {
        run: views::RunView::new(row, solved_ever),
        attempt: Some(&attempt).into(),
        solution: views::solution_view(p, Some(&attempt)),
    })
}

pub async fn reveal_hint(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<ProblemDetail>> {
    let (_, p) = find_open(&s, &id).await?;
    if p.meta.hints.is_empty() {
        return Err(ApiError::BadRequest(format!("{id} has no hints")));
    }
    let attempt = store::current_attempt(&s.db, &id).await?;
    if (attempt.hints_revealed as usize) < p.meta.hints.len() {
        store::reveal_hint(&s.db, attempt.id, p.meta.hints.len() as i32).await?;
    }
    Ok(Json(detail(&s, &id).await?))
}

pub async fn reveal_solution(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<ProblemDetail>> {
    let (_, p) = find_open(&s, &id).await?;
    if p.files.solution.is_none() {
        return Err(ApiError::NotReady(id));
    }
    let attempt = store::current_attempt(&s.db, &id).await?;
    store::reveal_solution(&s.db, attempt.id).await?;
    Ok(Json(detail(&s, &id).await?))
}

/// Upgrades to a WebSocket running rust-analyzer on this problem. See [`crate::lsp`].
#[derive(serde::Deserialize)]
pub struct LspParams {
    /// Check with clippy; defaults to the saved "Live clippy" setting. The web app sends it so a toggle takes effect
    /// on the reconnect without racing the settings save.
    clippy: Option<bool>,
}

pub async fn lsp(
    ws: WebSocketUpgrade,
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<LspParams>,
) -> ApiResult<Response> {
    let (_, p) = find_open(&s, &id).await?;
    if p.meta.language != anneal_content::Language::Rust {
        return Err(ApiError::BadRequest(format!("{id} isn't a Rust problem: there is no language server for it")));
    }
    let starter = p
        .files
        .starter
        .clone()
        .ok_or_else(|| ApiError::NotReady(id.clone()))?;
    let lib_rs = store::draft(&s.db, &id).await?.unwrap_or(starter);
    let visible_tests = p.files.visible_tests.clone().unwrap_or_default();
    let crates = p.meta.crates.clone();
    let perf = runner_perf(p.meta.perf);
    let clippy = match params.clippy {
        Some(c) => c,
        None => crate::settings::editor(&s.db).await?.live_clippy,
    };
    let permit = s.lsp.slots.clone().try_acquire_owned().map_err(|_| {
        ApiError::Busy(
            "rust-analyzer is already running for 3 editors; close one and try again".into(),
        )
    })?;
    let cfg = s.lsp.clone();
    Ok(ws.on_upgrade(move |socket| async move {
        if let Err(e) = lsp::session(
            socket,
            cfg,
            SessionFiles {
                lib_rs,
                visible_tests,
                crates,
                perf,
                clippy,
            },
        )
        .await
        {
            tracing::warn!(error = %e, problem = %id, "rust-analyzer session ended with an error");
        }
        drop(permit);
    }))
}

/// A problem's `[perf]` as the runner takes it; all off when there's none.
fn runner_perf(perf: Option<anneal_content::Perf>) -> anneal_runner::Perf {
    let p = perf.unwrap_or_default();
    anneal_runner::Perf { release: p.release, asm: p.asm, count_allocs: p.count_allocs }
}
