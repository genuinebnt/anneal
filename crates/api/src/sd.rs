//! System design prep (docs/SYSTEM_DESIGN.md): the overview, concept pages and problems from the content, finished drills and
//! their marks against the rubric, and review cards for what was missed, scheduled by FSRS.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use chrono::{Days, NaiveDate};
use fsrs::{FSRS, MemoryState};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::types::Json as Jsonb;

use crate::AppState;
use crate::error::{ApiError, ApiResult};
use crate::reviews::Grade;

/// The recall FSRS aims for on a system design card.
const RETENTION: f32 = 0.9;
/// When a marked rubric point or check first comes back: a miss today, a partly in two days, a got it in a week.
const FIRST_DUE: [(&str, u64); 3] = [("miss", 0), ("part", 2), ("got", 7)];
/// The largest answer kept for one step, and the largest diagram.
const MAX_STEP_BYTES: usize = 16 * 1024;
const MAX_DIAGRAM_BYTES: usize = 64 * 1024;

type AttemptRow = (i64, String, chrono::DateTime<chrono::Utc>, i32, Jsonb<Value>, Option<chrono::DateTime<chrono::Utc>>);
type CardRow = (String, String, NaiveDate, Option<f32>, Option<f32>, Option<NaiveDate>, Option<String>, i32, i32);
/// An attempt as the problem page shows it: id, when, seconds, steps, diagram, hints, marks.
type FullAttempt = (i64, chrono::DateTime<chrono::Utc>, i32, Jsonb<Value>, Jsonb<Value>, i32, Jsonb<Value>, Option<Jsonb<Value>>);

fn today() -> NaiveDate {
    crate::activity::today()
}

/// A drill's score: rubric points got, a partly counting half, over all points of the problem.
fn score(marks: &Value, points: usize) -> Option<f32> {
    let m = marks.as_object()?;
    if m.is_empty() || points == 0 {
        return None;
    }
    let got: f32 = m.values().map(|v| match v.as_str() { Some("got") => 1.0, Some("part") => 0.5, _ => 0.0 }).sum();
    Some(got / points as f32)
}

/// `GET /api/sd`: every concept and problem with where it stands, the cards due, and readiness by section.
pub async fn overview(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let sd = &s.catalog.sd;
    let attempts: Vec<AttemptRow> = sqlx::query_as("SELECT id, problem_id, finished_at, seconds, marks, marked_at FROM sd_attempts ORDER BY id").fetch_all(&s.db).await?;
    let cards: Vec<(String, NaiveDate, Option<String>)> = sqlx::query_as("SELECT card_id, due, last_grade FROM sd_cards").fetch_all(&s.db).await?;
    let t = today();
    let mut by_problem: HashMap<&str, Vec<&AttemptRow>> = HashMap::new();
    for a in &attempts {
        by_problem.entry(a.1.as_str()).or_default().push(a);
    }
    let problems: Vec<Value> = sd
        .problems
        .values()
        .map(|p| {
            let points = p.steps.iter().map(|st| st.points.len()).sum::<usize>();
            let mine = by_problem.get(p.id.as_str()).map(Vec::as_slice).unwrap_or_default();
            let best = mine.iter().filter_map(|a| score(&a.4.0, points)).fold(None, |m: Option<f32>, x| Some(m.map_or(x, |m| m.max(x))));
            let last = mine.last();
            json!({
                "id": p.id, "title": p.title, "kind": p.kind, "order": p.order, "minutes": p.minutes, "uses": p.uses, "companies": p.companies,
                "points": points, "attempts": mine.len(), "best": best, "last": last.map(|a| a.2), "last_id": last.map(|a| a.0),
                "unmarked": last.is_some_and(|a| a.5.is_none()),
            })
        })
        .collect();
    let checked: HashMap<&str, usize> = cards.iter().filter_map(|(id, _, _)| id.strip_prefix("check:")).filter_map(|r| r.rsplit_once(':')).fold(HashMap::new(), |mut m, (c, _)| {
        *m.entry(c).or_insert(0) += 1;
        m
    });
    let concepts: Vec<Value> = sd
        .concepts
        .values()
        .map(|c| json!({ "id": c.id, "title": c.title, "group": c.group, "order": c.order, "minutes": c.minutes, "lede": c.lede, "checks": c.checks.len(), "checked": checked.get(c.id.as_str()).copied().unwrap_or(0) }))
        .collect();
    let due = cards.iter().filter(|(_, d, _)| *d <= t).count();
    let week = cards.iter().filter(|(_, d, _)| *d > t && *d <= t + Days::new(7)).count();
    Ok(Json(json!({ "today": t, "concepts": concepts, "problems": problems, "due": due, "week": week, "cards": cards.len() })))
}

/// `GET /api/sd/concepts/{id}`.
pub async fn concept(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let c = s.catalog.sd.concepts.get(&id).ok_or_else(|| ApiError::NotFound(format!("concept {id}")))?;
    let marked: Vec<(String, Option<String>)> = sqlx::query_as("SELECT card_id, last_grade FROM sd_cards WHERE card_id LIKE $1").bind(format!("check:{id}:%")).fetch_all(&s.db).await?;
    let used_in: Vec<Value> = s.catalog.sd.problems.values().filter(|p| p.uses.contains(&id)).map(|p| json!({ "id": p.id, "title": p.title, "kind": p.kind })).collect();
    Ok(Json(json!({ "concept": c, "marked": marked.into_iter().map(|(k, _)| k).collect::<Vec<_>>(), "used_in": used_in })))
}

/// `GET /api/sd/problems/{id}`: the problem with its rubric and references (the page hides them until a drill is finished)
/// and the attempts made on it.
pub async fn problem(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let p = s.catalog.sd.problems.get(&id).ok_or_else(|| ApiError::NotFound(format!("problem {id}")))?;
    let attempts: Vec<FullAttempt> =
        sqlx::query_as("SELECT id, finished_at, seconds, steps, diagram, hints, marks, ai_review FROM sd_attempts WHERE problem_id = $1 ORDER BY id DESC").bind(&id).fetch_all(&s.db).await?;
    let attempts: Vec<Value> = attempts.into_iter().map(|(aid, at, secs, steps, diagram, hints, marks, ai)| json!({ "id": aid, "finished_at": at, "seconds": secs, "steps": steps.0, "diagram": diagram.0, "hints": hints, "marks": marks.0, "ai_review": ai.map(|j| j.0) })).collect();
    let uses: Vec<Value> = p.uses.iter().filter_map(|u| s.catalog.sd.concepts.get(u)).map(|c| json!({ "id": c.id, "title": c.title })).collect();
    Ok(Json(json!({ "problem": p, "uses": uses, "attempts": attempts })))
}

#[derive(Deserialize)]
pub struct NewAttempt {
    problem_id: String,
    started_at: chrono::DateTime<chrono::Utc>,
    seconds: i32,
    /// `{step name: {answer, seconds}}`.
    steps: Value,
    #[serde(default)]
    diagram: Value,
    #[serde(default)]
    hints: i32,
}

/// `POST /api/sd/attempts`: a finished drill, before it is marked.
pub async fn add_attempt(State(s): State<AppState>, Json(b): Json<NewAttempt>) -> ApiResult<Json<Value>> {
    let p = s.catalog.sd.problems.get(&b.problem_id).ok_or_else(|| ApiError::NotFound(format!("problem {}", b.problem_id)))?;
    let steps = b.steps.as_object().ok_or_else(|| ApiError::BadRequest("`steps` is an object of step name to answer".into()))?;
    if steps.keys().any(|k| !p.steps.iter().any(|st| &st.name == k)) {
        return Err(ApiError::BadRequest("a step that this problem does not have".into()));
    }
    if steps.values().any(|v| v.to_string().len() > MAX_STEP_BYTES) || b.diagram.to_string().len() > MAX_DIAGRAM_BYTES {
        return Err(ApiError::BadRequest("an answer or the diagram is too large".into()));
    }
    if b.seconds < 0 || b.hints < 0 {
        return Err(ApiError::BadRequest("seconds and hints are not negative".into()));
    }
    let diagram = if b.diagram.is_null() { json!({}) } else { b.diagram };
    let id: i64 = sqlx::query_scalar("INSERT INTO sd_attempts (problem_id, started_at, seconds, steps, diagram, hints) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id")
        .bind(&b.problem_id)
        .bind(b.started_at)
        .bind(b.seconds)
        .bind(Jsonb(&b.steps))
        .bind(Jsonb(&diagram))
        .bind(b.hints)
        .fetch_one(&s.db)
        .await?;
    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
pub struct Marks {
    /// `{point id: "got" | "part" | "miss"}`.
    marks: HashMap<String, String>,
}

/// Makes the card for a marked point or check, or brings an existing one forward when the new mark asks for it sooner. A card
/// already scheduled later keeps its memory; it is never removed.
async fn card_for(db: &sqlx::PgPool, card_id: &str, kind: &str, mark: &str) -> Result<(), sqlx::Error> {
    let Some((_, days)) = FIRST_DUE.iter().find(|(m, _)| *m == mark) else { return Ok(()) };
    let due = today() + Days::new(*days);
    sqlx::query("INSERT INTO sd_cards (card_id, kind, due) VALUES ($1, $2, $3) ON CONFLICT (card_id) DO UPDATE SET due = LEAST(sd_cards.due, EXCLUDED.due)")
        .bind(card_id)
        .bind(kind)
        .bind(due)
        .execute(db)
        .await?;
    Ok(())
}

/// `PUT /api/sd/attempts/{id}/marks`: the marks against the rubric. Every point marked becomes a card (a miss due today, a partly
/// soon, a got it in a week), so the rubric comes back in Review.
pub async fn mark_attempt(State(s): State<AppState>, Path(id): Path<i64>, Json(b): Json<Marks>) -> ApiResult<Json<Value>> {
    let problem_id: String = sqlx::query_scalar("SELECT problem_id FROM sd_attempts WHERE id = $1").bind(id).fetch_optional(&s.db).await?.ok_or_else(|| ApiError::NotFound(format!("attempt {id}")))?;
    let p = s.catalog.sd.problems.get(&problem_id).ok_or_else(|| ApiError::NotFound(format!("problem {problem_id}")))?;
    let known: Vec<&str> = p.steps.iter().flat_map(|st| st.points.iter().map(|pt| pt.id.as_str())).collect();
    if let Some((k, v)) = b.marks.iter().find(|(k, v)| !known.contains(&k.as_str()) || !FIRST_DUE.iter().any(|(m, _)| m == v)) {
        return Err(ApiError::BadRequest(format!("{k:?}: {v:?} is not a mark on a point of this problem")));
    }
    sqlx::query("UPDATE sd_attempts SET marks = $2, marked_at = now() WHERE id = $1").bind(id).bind(Jsonb(&b.marks)).execute(&s.db).await?;
    for (point, mark) in &b.marks {
        card_for(&s.db, &format!("point:{problem_id}:{point}"), "point", mark).await?;
    }
    let points = known.len();
    Ok(Json(json!({ "score": score(&serde_json::to_value(&b.marks).unwrap_or_default(), points), "cards": b.marks.len() })))
}

#[derive(Deserialize)]
pub struct CheckMark {
    concept: String,
    index: usize,
    mark: String,
}

/// `POST /api/sd/checks`: a concept check answered and marked; it becomes a card like a rubric point.
pub async fn mark_check(State(s): State<AppState>, Json(b): Json<CheckMark>) -> ApiResult<Json<Value>> {
    let c = s.catalog.sd.concepts.get(&b.concept).ok_or_else(|| ApiError::NotFound(format!("concept {}", b.concept)))?;
    if b.index >= c.checks.len() || !FIRST_DUE.iter().any(|(m, _)| *m == b.mark) {
        return Err(ApiError::BadRequest("no such check, or not a mark".into()));
    }
    card_for(&s.db, &format!("check:{}:{}", b.concept, b.index), "check", &b.mark).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Serialize)]
struct DueCard {
    card_id: String,
    kind: String,
    due: NaiveDate,
    reps: i32,
    lapses: i32,
    last_grade: Option<String>,
    /// Where it comes from: the problem or concept.
    source: Value,
    prompt: String,
    answer: String,
    /// The level a rubric point is expected at.
    level: Option<String>,
}

/// What a card asks and answers, from the content; `None` when the content no longer has it.
fn resolve(s: &AppState, card_id: &str) -> Option<(Value, String, String, Option<String>)> {
    let sd = &s.catalog.sd;
    if let Some(rest) = card_id.strip_prefix("point:") {
        let (pid, point) = rest.split_once(':')?;
        let p = sd.problems.get(pid)?;
        let (step, pt) = p.steps.iter().find_map(|st| st.points.iter().find(|x| x.id == point).map(|x| (st, x)))?;
        return Some((json!({ "type": "problem", "id": p.id, "title": p.title, "step": step.name }), pt.text.clone(), step.reference.clone(), Some(pt.level.clone())));
    }
    let rest = card_id.strip_prefix("check:")?;
    let (cid, n) = rest.rsplit_once(':')?;
    let c = sd.concepts.get(cid)?;
    let k = c.checks.get(n.parse::<usize>().ok()?)?;
    Some((json!({ "type": "concept", "id": c.id, "title": c.title }), k.prompt.clone(), k.answer.clone(), None))
}

/// `GET /api/sd/review`: the cards due today, most overdue first, and how many come in the next week.
pub async fn review(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let t = today();
    let rows: Vec<CardRow> = sqlx::query_as("SELECT card_id, kind, due, stability, difficulty, last_review, last_grade, reps, lapses FROM sd_cards WHERE due <= $1 ORDER BY due, card_id").bind(t).fetch_all(&s.db).await?;
    let due: Vec<DueCard> = rows
        .into_iter()
        .filter_map(|(card_id, kind, due, _, _, _, last_grade, reps, lapses)| {
            let (source, prompt, answer, level) = resolve(&s, &card_id)?;
            Some(DueCard { card_id, kind, due, reps, lapses, last_grade, source, prompt, answer, level })
        })
        .collect();
    let week: Vec<(NaiveDate, i64)> = sqlx::query_as("SELECT due, count(*) FROM sd_cards WHERE due > $1 AND due <= $2 GROUP BY due ORDER BY due").bind(t).bind(t + Days::new(7)).fetch_all(&s.db).await?;
    Ok(Json(json!({ "today": t, "due": due, "week": week.into_iter().map(|(d, n)| json!({ "day": d, "n": n })).collect::<Vec<_>>() })))
}

#[derive(Deserialize)]
pub struct GradeBody {
    grade: Grade,
}

/// `POST /api/sd/review/{card}`: grades a card and schedules it again with FSRS.
pub async fn grade(State(s): State<AppState>, Path(card_id): Path<String>, Json(b): Json<GradeBody>) -> ApiResult<Json<Value>> {
    let row: Option<CardRow> = sqlx::query_as("SELECT card_id, kind, due, stability, difficulty, last_review, last_grade, reps, lapses FROM sd_cards WHERE card_id = $1").bind(&card_id).fetch_optional(&s.db).await?;
    let (_, _, _, stability, difficulty, last_review, _, reps, lapses) = row.ok_or_else(|| ApiError::NotFound(format!("card {card_id}")))?;
    let t = today();
    let prev = stability.zip(difficulty).map(|(stability, difficulty)| MemoryState { stability, difficulty });
    let elapsed = last_review.map_or(0, |d| (t - d).num_days().max(0) as u32);
    let next = FSRS::default().next_states(prev, RETENTION, elapsed).map_err(|e| ApiError::Internal(anyhow::anyhow!("scheduling: {e}")))?;
    let (item, name) = match b.grade {
        Grade::Again => (next.again, "again"),
        Grade::Hard => (next.hard, "hard"),
        Grade::Good => (next.good, "good"),
        Grade::Easy => (next.easy, "easy"),
    };
    let days = item.interval.round().max(1.0) as u64;
    let due = t + Days::new(days);
    let lapsed = matches!(b.grade, Grade::Again) && reps > 0;
    sqlx::query(
        "UPDATE sd_cards SET due = $2, stability = $3, difficulty = $4, last_review = $5, last_grade = $6, reps = reps + 1, lapses = lapses + $7,
         history = history || jsonb_build_array(jsonb_build_object('day', $5::date, 'grade', $6::text, 'due', $2::date)) WHERE card_id = $1",
    )
    .bind(&card_id)
    .bind(due)
    .bind(item.memory.stability)
    .bind(item.memory.difficulty)
    .bind(t)
    .bind(name)
    .bind(i32::from(lapsed))
    .execute(&s.db)
    .await?;
    Ok(Json(json!({ "card_id": card_id, "due": due, "days": days, "reps": reps + 1, "lapses": lapses + i32::from(lapsed) })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_partly_counts_half_and_unmarked_has_no_score() {
        assert_eq!(score(&json!({ "a": "got", "b": "part", "c": "miss" }), 4), Some(0.375));
        assert_eq!(score(&json!({}), 4), None);
    }
}
