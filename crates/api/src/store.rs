//! Progress data: attempts, runs and drafts.

use std::collections::HashMap;

use anneal_rules::Violation;
use anneal_runner::RunResult;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use sqlx::types::Json;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Attempt {
    pub id: i64,
    pub started_at: DateTime<Utc>,
    pub solved_at: Option<DateTime<Utc>>,
    pub hints_revealed: i32,
    pub solution_revealed: bool,
    pub assisted: bool,
    /// `practice` (first time through) or `resolve` (a scheduled review).
    pub kind: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RunRow {
    pub id: i64,
    pub kind: String,
    pub code: String,
    pub result: Json<RunResult>,
    pub violations: Json<Vec<Violation>>,
    pub created_at: DateTime<Utc>,
}

const ATTEMPT_COLS: &str = "id, started_at, solved_at, hints_revealed, solution_revealed, assisted, kind";

pub async fn latest_attempt(db: &PgPool, problem_id: &str) -> sqlx::Result<Option<Attempt>> {
    sqlx::query_as(&format!("SELECT {ATTEMPT_COLS} FROM attempts WHERE problem_id = $1 ORDER BY started_at DESC, id DESC LIMIT 1"))
        .bind(problem_id)
        .fetch_optional(db)
        .await
}

/// The latest attempt, or a new one if the problem was never attempted.
pub async fn current_attempt(db: &PgPool, problem_id: &str) -> sqlx::Result<Attempt> {
    if let Some(a) = latest_attempt(db, problem_id).await? {
        return Ok(a);
    }
    sqlx::query_as(&format!(
        "INSERT INTO attempts (problem_id) VALUES ($1) RETURNING {ATTEMPT_COLS}"
    ))
    .bind(problem_id)
    .fetch_one(db)
    .await
}

/// Reveals one more hint, up to `max`. Hints opened before solving make the attempt assisted.
pub async fn reveal_hint(db: &PgPool, attempt_id: i64, max: i32) -> sqlx::Result<Attempt> {
    sqlx::query_as(&format!(
        "UPDATE attempts SET hints_revealed = LEAST(hints_revealed + 1, $2), assisted = assisted OR solved_at IS NULL
         WHERE id = $1 RETURNING {ATTEMPT_COLS}"
    ))
    .bind(attempt_id)
    .bind(max)
    .fetch_one(db)
    .await
}

/// "Reveal anyway": assisted unless the problem was already solved.
pub async fn reveal_solution(db: &PgPool, attempt_id: i64) -> sqlx::Result<Attempt> {
    sqlx::query_as(&format!(
        "UPDATE attempts SET solution_revealed = true, assisted = assisted OR solved_at IS NULL
         WHERE id = $1 RETURNING {ATTEMPT_COLS}"
    ))
    .bind(attempt_id)
    .fetch_one(db)
    .await
}

pub async fn mark_solved(db: &PgPool, attempt_id: i64) -> sqlx::Result<Attempt> {
    sqlx::query_as(&format!("UPDATE attempts SET solved_at = COALESCE(solved_at, now()) WHERE id = $1 RETURNING {ATTEMPT_COLS}"))
        .bind(attempt_id)
        .fetch_one(db)
        .await
}

pub async fn insert_run(
    db: &PgPool,
    attempt_id: i64,
    problem_id: &str,
    kind: &str,
    code: &str,
    result: &RunResult,
    violations: &[Violation],
) -> sqlx::Result<RunRow> {
    let status = serde_json::to_value(result.status)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    sqlx::query_as(
        "INSERT INTO runs (attempt_id, problem_id, kind, code, status, passed, total, result, violations)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id, kind, code, result, violations, created_at",
    )
    .bind(attempt_id)
    .bind(problem_id)
    .bind(kind)
    .bind(code)
    .bind(status)
    .bind(result.passed as i32)
    .bind(result.total as i32)
    .bind(Json(result))
    .bind(Json(violations))
    .fetch_one(db)
    .await
}

/// Runs in an attempt, oldest first.
pub async fn runs(db: &PgPool, attempt_id: i64) -> sqlx::Result<Vec<RunRow>> {
    sqlx::query_as(
        "SELECT id, kind, code, result, violations, created_at FROM runs WHERE attempt_id = $1 ORDER BY id",
    )
    .bind(attempt_id)
    .fetch_all(db)
    .await
}

pub async fn draft(db: &PgPool, problem_id: &str) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("SELECT code FROM drafts WHERE problem_id = $1")
        .bind(problem_id)
        .fetch_optional(db)
        .await
}

pub async fn save_draft(db: &PgPool, problem_id: &str, code: &str) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO drafts (problem_id, code) VALUES ($1, $2)
         ON CONFLICT (problem_id) DO UPDATE SET code = EXCLUDED.code, updated_at = now()",
    )
    .bind(problem_id)
    .bind(code)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn delete_draft(db: &PgPool, problem_id: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM drafts WHERE problem_id = $1")
        .bind(problem_id)
        .execute(db)
        .await?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub struct ProgressRow {
    pub solved: bool,
    pub assisted: bool,
    /// Readiness credit from the chance of still recalling it (1.0 with no review on record); see [`crate::reviews::credit`].
    pub decay: f64,
}

/// Per problem: the latest solved attempt if there is one (a re-solve in progress keeps the earlier
/// credit), otherwise the latest attempt.
pub async fn progress(db: &PgPool) -> sqlx::Result<HashMap<String, ProgressRow>> {
    /// problem id, solved, assisted, and the review's stability, difficulty and last review (none before a first solve).
    type Row = (String, bool, bool, Option<f32>, Option<f32>, Option<DateTime<Utc>>);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT DISTINCT ON (a.problem_id) a.problem_id, a.solved_at IS NOT NULL, a.assisted, r.stability, r.difficulty, r.last_review
         FROM attempts a LEFT JOIN reviews r ON r.problem_id = a.problem_id
         ORDER BY a.problem_id, (a.solved_at IS NOT NULL) DESC, (a.kind <> 'extra') DESC, a.started_at DESC, a.id DESC",
    )
    .fetch_all(db)
    .await?;
    let today = crate::activity::today();
    Ok(rows
        .into_iter()
        .map(|(id, solved, assisted, stability, difficulty, last)| {
            let decay = match (stability, difficulty, last) {
                (Some(stability), Some(difficulty), Some(last)) => {
                    let elapsed = (today - last.with_timezone(&chrono::Local).date_naive()).num_days() as f32;
                    crate::reviews::credit(fsrs::MemoryState { stability, difficulty }, elapsed)
                }
                _ => 1.0,
            };
            (id, ProgressRow { solved, assisted, decay })
        })
        .collect())
}

/// Starts a scheduled re-solve: a new attempt (hints locked again) and the editor back to the starter.
pub async fn start_resolve(db: &PgPool, problem_id: &str) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query("INSERT INTO attempts (problem_id, kind) VALUES ($1, 'resolve')")
        .bind(problem_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM drafts WHERE problem_id = $1").bind(problem_id).execute(&mut *tx).await?;
    tx.commit().await
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReviewRow {
    pub problem_id: String,
    pub step: i32,
    pub due_at: DateTime<Utc>,
    pub last_result: String,
    pub history: Json<Vec<serde_json::Value>>,
    pub stability: f32,
    pub difficulty: f32,
    pub last_review: DateTime<Utc>,
    pub last_grade: String,
    pub reps: i32,
    pub lapses: i32,
}

impl ReviewRow {
    pub fn memory(&self) -> fsrs::MemoryState {
        fsrs::MemoryState { stability: self.stability, difficulty: self.difficulty }
    }

    /// Chance of recalling the problem on `today` (local date).
    pub fn retrievability(&self, today: chrono::NaiveDate) -> f32 {
        let last = self.last_review.with_timezone(&chrono::Local).date_naive();
        crate::reviews::retrievability(self.memory(), (today - last).num_days() as f32)
    }
}

pub async fn reviews(db: &PgPool) -> sqlx::Result<Vec<ReviewRow>> {
    sqlx::query_as(
        "SELECT problem_id, step, due_at, last_result, history, stability, difficulty, last_review, last_grade, reps, lapses
         FROM reviews ORDER BY due_at",
    )
    .fetch_all(db)
    .await
}

/// Grades a review (or a first solve) and schedules the next one: the FSRS memory state is updated, the due date is
/// snapped to the owner's review days and kept off busy ones, and the result is recorded in the problem's history.
/// What scheduling a review needs: the problem's memory state and last review date (none before its first), and how
/// many reviews every other problem has planned for each day.
async fn review_context(db: &PgPool, problem_id: &str) -> sqlx::Result<(Option<(fsrs::MemoryState, chrono::NaiveDate)>, HashMap<chrono::NaiveDate, u32>)> {
    let local = |t: DateTime<Utc>| t.with_timezone(&chrono::Local).date_naive();
    let prev: Option<(f32, f32, DateTime<Utc>)> = sqlx::query_as("SELECT stability, difficulty, last_review FROM reviews WHERE problem_id = $1").bind(problem_id).fetch_optional(db).await?;
    let planned: Vec<DateTime<Utc>> = sqlx::query_scalar("SELECT due_at FROM reviews WHERE problem_id <> $1").bind(problem_id).fetch_all(db).await?;
    let mut load: HashMap<chrono::NaiveDate, u32> = HashMap::new();
    for due in planned {
        *load.entry(local(due)).or_default() += 1;
    }
    // A day the owner made a break takes no reviews: count it as full many times over, so scheduling goes round it.
    for day in crate::calendar::active_overrides(db).await?.into_iter().filter(|(_, k)| *k == crate::planner::Kind::Break).map(|(d, _)| d) {
        *load.entry(day).or_default() += 1000;
    }
    Ok((prev.map(|(stability, difficulty, last)| (fsrs::MemoryState { stability, difficulty }, local(last))), load))
}

/// When each grade would bring the problem back, without logging anything.
pub async fn preview_review(db: &PgPool, problem_id: &str, settings: &crate::reviews::Settings) -> sqlx::Result<Vec<(crate::reviews::Grade, crate::reviews::Scheduled)>> {
    use crate::reviews::Grade;
    let (before, load) = review_context(db, problem_id).await?;
    let today = crate::activity::today();
    [Grade::Again, Grade::Hard, Grade::Good, Grade::Easy]
        .into_iter()
        .map(|g| {
            crate::reviews::schedule(before, g, today, settings, &|day| load.get(&day).copied().unwrap_or(0))
                .map(|s| (g, s))
                .map_err(|e| sqlx::Error::Protocol(e.to_string()))
        })
        .collect()
}

pub async fn record_review(
    db: &PgPool,
    problem_id: &str,
    grade: crate::reviews::Grade,
    resolve: bool,
    settings: &crate::reviews::Settings,
) -> sqlx::Result<crate::reviews::Scheduled> {
    let (before, load) = review_context(db, problem_id).await?;
    let today = crate::activity::today();
    let scheduled = crate::reviews::schedule(before, grade, today, settings, &|day| load.get(&day).copied().unwrap_or(0))
        .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
    let recall_before = before.map(|(m, last)| crate::reviews::retrievability(m, (today - last).num_days() as f32));
    let due_at = chrono::TimeZone::from_local_datetime(&chrono::Local, &scheduled.due.and_hms_opt(6, 0, 0).expect("06:00 exists"))
        .earliest()
        .map_or_else(|| Utc::now() + chrono::TimeDelta::days(1), |t| t.with_timezone(&Utc));
    let step = crate::reviews::level(scheduled.memory.stability);
    let entry = serde_json::json!({
        "at": Utc::now(), "grade": grade.as_str(), "result": grade.legacy_result(), "resolve": resolve, "step": step,
        "recall_before": recall_before, "stability": scheduled.memory.stability, "difficulty": scheduled.memory.difficulty,
        "ideal_days": scheduled.ideal_days, "due": scheduled.due,
    });
    let lapse = i32::from(grade == crate::reviews::Grade::Again);
    sqlx::query(
        "INSERT INTO reviews (problem_id, step, due_at, last_result, history, stability, difficulty, last_review, last_grade, reps, lapses)
         VALUES ($1, $2, $3, $4, jsonb_build_array($5::jsonb), $6, $7, now(), $8, 1, $9)
         ON CONFLICT (problem_id) DO UPDATE SET step = EXCLUDED.step, due_at = EXCLUDED.due_at, last_result = EXCLUDED.last_result,
             history = reviews.history || $5::jsonb, stability = EXCLUDED.stability, difficulty = EXCLUDED.difficulty,
             last_review = now(), last_grade = EXCLUDED.last_grade, reps = reviews.reps + 1,
             lapses = reviews.lapses + $9, updated_at = now()",
    )
    .bind(problem_id)
    .bind(step)
    .bind(due_at)
    .bind(grade.legacy_result())
    .bind(Json(entry))
    .bind(scheduled.memory.stability)
    .bind(scheduled.memory.difficulty)
    .bind(grade.as_str())
    .bind(lapse)
    .execute(db)
    .await?;
    Ok(scheduled)
}

/// Logs a LeetCode problem done elsewhere: an attempt (solved unless the grade is `again`, assisted on `hard` and
/// `again`) and, when `schedule` is set, the graded review that schedules the next one. A problem that already has a
/// review is a re-solve. Practice problems are logged without a review (decision 24).
pub async fn log_attempt(
    db: &PgPool,
    problem_id: &str,
    grade: crate::reviews::Grade,
    settings: &crate::reviews::Settings,
    schedule: bool,
    extra: bool,
) -> sqlx::Result<Option<crate::reviews::Scheduled>> {
    use crate::reviews::Grade;
    let resolve: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM reviews WHERE problem_id = $1)").bind(problem_id).fetch_one(db).await?;
    let solved = grade != Grade::Again;
    let assisted = matches!(grade, Grade::Again | Grade::Hard);
    sqlx::query(
        "INSERT INTO attempts (problem_id, kind, solved_at, assisted)
         VALUES ($1, $2, CASE WHEN $3 THEN now() END, $4)",
    )
    .bind(problem_id)
    .bind(if extra { "extra" } else if resolve { "resolve" } else { "practice" })
    .bind(solved)
    .bind(assisted)
    .execute(db)
    .await?;
    if !schedule || extra {
        return Ok(None);
    }
    record_review(db, problem_id, grade, resolve, settings).await.map(Some)
}

/// A problem's logged attempts, oldest first: the kind (`practice` is the first try, `resolve` a later one, `extra` a
/// repeat for practice), whether it was solved and whether it needed help.
pub async fn attempt_history(db: &PgPool, problem_id: &str) -> sqlx::Result<Vec<(String, bool, bool)>> {
    sqlx::query_as("SELECT kind, solved_at IS NOT NULL, assisted FROM attempts WHERE problem_id = $1 AND kind <> 'mock' ORDER BY started_at, id")
        .bind(problem_id)
        .fetch_all(db)
        .await
}

/// The DSA problems (`lc-…`) that have any attempt logged, with any grade.
pub async fn attempted_dsa(db: &PgPool) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar("SELECT DISTINCT problem_id FROM attempts WHERE problem_id LIKE 'lc-%'").fetch_all(db).await
}

/// Where "next problem" starts from (`settings.dsa_start`): a problem id, the last one logged or the first of a
/// track the owner picked.
pub async fn dsa_start(db: &PgPool) -> sqlx::Result<Option<String>> {
    let stored: Option<Json<String>> = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'dsa_start'").fetch_optional(db).await?;
    Ok(stored.map(|j| j.0))
}

pub async fn set_dsa_start(db: &PgPool, problem_id: &str) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES ('dsa_start', $1)
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .bind(Json(problem_id))
    .execute(db)
    .await?;
    Ok(())
}

/// Adds active-editing seconds for a problem on a (local) day.
pub async fn add_focus(db: &PgPool, day: chrono::NaiveDate, problem_id: &str, seconds: i32) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO focus_time (day, problem_id, seconds) VALUES ($1, $2, $3)
         ON CONFLICT (day, problem_id) DO UPDATE SET seconds = focus_time.seconds + EXCLUDED.seconds",
    )
    .bind(day)
    .bind(problem_id)
    .bind(seconds)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn focus(db: &PgPool) -> sqlx::Result<Vec<(chrono::NaiveDate, String, i32)>> {
    sqlx::query_as("SELECT day, problem_id, seconds FROM focus_time").fetch_all(db).await
}

/// Every run, oldest first, with just what the stats need.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RunStat {
    pub attempt_id: i64,
    pub problem_id: String,
    pub status: String,
    pub diagnostics: Json<Vec<anneal_runner::Diagnostic>>,
    pub violations: Json<Vec<Violation>>,
    pub created_at: DateTime<Utc>,
}

pub async fn run_stats(db: &PgPool) -> sqlx::Result<Vec<RunStat>> {
    sqlx::query_as(
        "SELECT attempt_id, problem_id, status, result -> 'diagnostics' AS diagnostics, violations, created_at
         FROM runs ORDER BY id",
    )
    .fetch_all(db)
    .await
}

/// One attempt with its run count and latest activity, for the activity rail.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ActivityRow {
    pub attempt_id: i64,
    pub kind: String,
    pub problem_id: String,
    pub started_at: DateTime<Utc>,
    pub solved_at: Option<DateTime<Utc>>,
    pub assisted: bool,
    pub hints_revealed: i32,
    pub solution_revealed: bool,
    pub runs: i64,
    pub last_at: DateTime<Utc>,
}

/// Every attempt, most recently active first.
pub async fn activity(db: &PgPool) -> sqlx::Result<Vec<ActivityRow>> {
    sqlx::query_as(
        "SELECT a.id AS attempt_id, a.kind, a.problem_id, a.started_at, a.solved_at, a.assisted, a.hints_revealed, a.solution_revealed,
                count(r.id) AS runs,
                GREATEST(a.started_at, COALESCE(a.solved_at, a.started_at), COALESCE(max(r.created_at), a.started_at)) AS last_at
         FROM attempts a LEFT JOIN runs r ON r.attempt_id = a.id
         GROUP BY a.id
         ORDER BY last_at DESC, a.id DESC",
    )
    .fetch_all(db)
    .await
}

pub async fn scratch(db: &PgPool, problem_id: &str) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("SELECT code FROM scratch WHERE problem_id = $1").bind(problem_id).fetch_optional(db).await
}

pub async fn save_scratch(db: &PgPool, problem_id: &str, code: &str) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO scratch (problem_id, code) VALUES ($1, $2)
         ON CONFLICT (problem_id) DO UPDATE SET code = EXCLUDED.code, updated_at = now()",
    )
    .bind(problem_id)
    .bind(code)
    .execute(db)
    .await?;
    Ok(())
}

/// Whether any attempt at the problem has been solved; hidden tests open up after that.
pub async fn ever_solved(db: &PgPool, problem_id: &str) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM attempts WHERE problem_id = $1 AND solved_at IS NOT NULL)")
        .bind(problem_id)
        .fetch_one(db)
        .await
}
