//! Reading to remember (DSA): a queue of problems to read on any device, the idea revealed after you try to recall it, and an
//! idea-memory schedule (FSRS) kept apart from the coding reviews, so reading never moves when you next have to write the code.

use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::{Days, NaiveDate};
use fsrs::{FSRS, MemoryState};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::AppState;
use crate::dsa::dsa_tracks;
use crate::error::{ApiError, ApiResult};
use crate::reviews::Grade;

/// The recall reading aims for: a little lower than coding reviews, since rereading is cheap.
const RETENTION: f32 = 0.85;
/// New problems offered per visit, after the due ones.
const NEW_PER_VISIT: usize = 20;

type ReadRow = (String, NaiveDate, Option<f32>, Option<f32>, Option<NaiveDate>, Option<String>, i32, i32);

#[derive(Deserialize)]
pub struct Scope {
    /// `solved` (problems you have solved, the default) or `all`.
    scope: Option<String>,
    /// A pattern code to read within, e.g. `D1`.
    pattern: Option<String>,
}

/// `GET /api/dsa/read`: what to read now: the due cards first (most overdue first), then new problems in the plan's order, within
/// the scope; and the counts.
pub async fn queue(State(s): State<AppState>, Query(q): Query<Scope>) -> ApiResult<Json<Value>> {
    let today = crate::activity::today();
    let rows: Vec<ReadRow> = sqlx::query_as("SELECT problem_id, due, stability, difficulty, last_review, last_grade, reps, lapses FROM dsa_read").fetch_all(&s.db).await?;
    let progress = crate::store::progress(&s.db).await?;
    let solved_only = q.scope.as_deref() != Some("all");
    let in_scope = |t_code: &str, id: &str| q.pattern.as_deref().is_none_or(|p| p == t_code) && (!solved_only || progress.get(id).is_some_and(|r| r.solved));
    let mut ordered: Vec<(&str, &str)> = dsa_tracks(&s.catalog).flat_map(|t| t.problems.iter().map(move |p| (t.code.as_str(), p.id.as_str()))).collect();
    ordered.retain(|(code, id)| in_scope(code, id));
    let read: std::collections::HashMap<&str, &ReadRow> = rows.iter().map(|r| (r.0.as_str(), r)).collect();
    let mut due: Vec<&ReadRow> = rows.iter().filter(|r| r.1 <= today && ordered.iter().any(|(_, id)| *id == r.0)).collect();
    due.sort_by_key(|r| r.1);
    let fresh: Vec<&str> = ordered.iter().filter(|(_, id)| !read.contains_key(id)).map(|(_, id)| *id).take(NEW_PER_VISIT).collect();
    let week = rows.iter().filter(|r| r.1 > today && r.1 <= today + Days::new(7)).count();
    Ok(Json(json!({
        "today": today,
        "due": due.iter().map(|r| json!({ "id": r.0, "due": r.1, "reps": r.6, "lapses": r.7, "last_grade": r.5 })).collect::<Vec<_>>(),
        "new": fresh,
        "read": rows.len(),
        "in_scope": ordered.len(),
        "week": week,
    })))
}

#[derive(Deserialize)]
pub struct GradeBody {
    grade: Grade,
}

/// `POST /api/dsa/read/{id}`: how well the idea came back (again: forgot, hard: fuzzy, good: remembered, easy: instantly), scheduled
/// again with FSRS. A first read makes the card.
pub async fn grade(State(s): State<AppState>, Path(id): Path<String>, Json(b): Json<GradeBody>) -> ApiResult<Json<Value>> {
    let (_, p) = s.catalog.problem(&id).ok_or_else(|| ApiError::NotFound(format!("problem {id}")))?;
    if p.dsa.is_none() {
        return Err(ApiError::NotFound(format!("DSA problem {id}")));
    }
    let today = crate::activity::today();
    let row: Option<ReadRow> = sqlx::query_as("SELECT problem_id, due, stability, difficulty, last_review, last_grade, reps, lapses FROM dsa_read WHERE problem_id = $1").bind(&id).fetch_optional(&s.db).await?;
    let prev = row.as_ref().and_then(|r| r.2.zip(r.3)).map(|(stability, difficulty)| MemoryState { stability, difficulty });
    let elapsed = row.as_ref().and_then(|r| r.4).map_or(0, |d| (today - d).num_days().max(0) as u32);
    let next = FSRS::default().next_states(prev, RETENTION, elapsed).map_err(|e| ApiError::Internal(anyhow::anyhow!("scheduling: {e}")))?;
    let (item, name) = match b.grade {
        Grade::Again => (next.again, "again"),
        Grade::Hard => (next.hard, "hard"),
        Grade::Good => (next.good, "good"),
        Grade::Easy => (next.easy, "easy"),
    };
    let days = item.interval.round().max(1.0) as u64;
    let due = today + Days::new(days);
    let lapsed = i32::from(matches!(b.grade, Grade::Again) && row.as_ref().is_some_and(|r| r.6 > 0));
    sqlx::query(
        "INSERT INTO dsa_read (problem_id, due, stability, difficulty, last_review, last_grade, reps, lapses, history)
         VALUES ($1, $2, $3, $4, $5, $6, 1, 0, jsonb_build_array(jsonb_build_object('day', $5::date, 'grade', $6::text, 'due', $2::date)))
         ON CONFLICT (problem_id) DO UPDATE SET due = $2, stability = $3, difficulty = $4, last_review = $5, last_grade = $6,
           reps = dsa_read.reps + 1, lapses = dsa_read.lapses + $7,
           history = dsa_read.history || jsonb_build_array(jsonb_build_object('day', $5::date, 'grade', $6::text, 'due', $2::date))",
    )
    .bind(&id)
    .bind(due)
    .bind(item.memory.stability)
    .bind(item.memory.difficulty)
    .bind(today)
    .bind(name)
    .bind(lapsed)
    .execute(&s.db)
    .await?;
    Ok(Json(json!({ "id": id, "due": due, "days": days })))
}
