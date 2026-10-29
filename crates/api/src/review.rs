//! The quick-recall review session (docs/DSA.md, decision 26): today's reviews one at a time.
//!
//! `GET /api/dsa/review` is the queue the plan picked for today (most forgotten first, up to the day's room), each
//! with when every grade would bring it back. The grades themselves are logged through the ordinary
//! `POST /api/dsa/problems/{id}/log`, so a review session and the ✗ ½ ✓ ⚡ buttons schedule identically.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use serde_json::{Value, json};

use crate::AppState;
use crate::dsa::{dsa_reviews, plan, row};
use crate::error::{ApiError, ApiResult};
use crate::reviews::Grade;
use crate::store::{self, ReviewRow};

/// How many upcoming reviews the page can offer to do early.
const AHEAD: usize = 15;

/// When each grade would bring a problem back, as `{again: {due, days}, ...}`.
async fn previews(s: &AppState, id: &str, settings: &crate::reviews::Settings, today: chrono::NaiveDate) -> ApiResult<serde_json::Map<String, Value>> {
    let mut out = serde_json::Map::new();
    for (grade, scheduled) in store::preview_review(&s.db, id, &crate::dsa::settings_for(&s.catalog, id, settings)).await? {
        out.insert(grade.as_str().to_owned(), json!({ "due": scheduled.due, "days": (scheduled.due - today).num_days() }));
    }
    Ok(out)
}

/// `GET /api/dsa/problems/{id}/preview`: what each grade would schedule for this problem, for the log buttons on its
/// page. Practice problems schedule nothing, so they get `null`.
pub async fn preview(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let (_, problem) = s.catalog.problem(&id).ok_or_else(|| ApiError::NotFound(format!("problem {id}")))?;
    if problem.dsa.is_none() {
        return Err(ApiError::BadRequest(format!("{id} isn't a DSA problem")));
    }
    // How this problem has been logged so far, oldest first: `first`, `later` or `practice`, and the result.
    let practice_list = crate::dsa::is_practice(problem);
    let history: Vec<Value> = store::attempt_history(&s.db, &id)
        .await?
        .into_iter()
        .map(|(kind, solved, assisted)| {
            let kind = if practice_list || kind == "extra" { "practice" } else if kind == "resolve" { "later" } else { "first" };
            json!({ "kind": kind, "result": if !solved { "again" } else if assisted { "hard" } else { "good" } })
        })
        .collect();
    if practice_list {
        return Ok(Json(json!({ "previews": null, "history": history })));
    }
    let settings = crate::settings::srs(&s.db).await?;
    Ok(Json(json!({ "previews": previews(&s, &id, &settings, crate::activity::today()).await?, "history": history })))
}

pub async fn queue(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let today = crate::activity::today();
    let settings = crate::settings::srs(&s.db).await?;
    let progress = store::progress(&s.db).await?;
    let reviews = dsa_reviews(&s.catalog, store::reviews(&s.db).await?);
    let plan = plan(&s, &settings, &progress, &reviews, today).await?;
    let by_problem: HashMap<&str, &ReviewRow> = reviews.iter().map(|r| (r.problem_id.as_str(), r)).collect();

    // One review as the page shows it: the problem, how well it is remembered and what each grade would schedule.
    let item_of = |id: String| {
        let by_problem = &by_problem;
        let (settings, progress) = (&settings, &progress);
        let s = &s;
        async move {
            let (Some((track, problem)), Some(review)) = (s.catalog.problem(&id), by_problem.get(id.as_str())) else { return Ok(None) };
            let Some(dsa) = problem.dsa.as_ref() else { return Ok(None) };
            let previews = previews(s, &id, settings, today).await?;
            let technique = s.catalog.dsa.techniques.iter().find(|t| t.id == dsa.technique).map(|t| t.name.as_str());
            let row = serde_json::to_value(row(track, problem, progress, by_problem, today)).expect("a problem row is plain data");
            ApiResult::Ok(Some(json!({
                "problem": row,
                "pattern": track.name,
                "technique": technique,
                "recall": review.retrievability(today),
                "stability": review.stability,
                "previews": previews,
                "due": review.due_at.with_timezone(&chrono::Local).date_naive(),
            })))
        }
    };
    let mut items = Vec::new();
    for id in &plan.review_ids {
        if let Some(item) = item_of(id.clone()).await? {
            items.push(item);
        }
    }

    let queued: std::collections::HashSet<&str> = plan.review_ids.iter().map(String::as_str).collect();
    // Every other review, soonest first: more that are due today than the day takes, then the ones coming up. The page
    // offers them as "review ahead"; reviewing one early still counts.
    let mut upcoming: Vec<&ReviewRow> = reviews.iter().filter(|r| !queued.contains(r.problem_id.as_str())).collect();
    upcoming.sort_by_key(|r| r.due_at);
    let mut ahead = Vec::new();
    for r in upcoming.into_iter().take(AHEAD) {
        if let Some(item) = item_of(r.problem_id.clone()).await? {
            ahead.push(item);
        }
    }
    let next_review = reviews
        .iter()
        .filter(|r| !queued.contains(r.problem_id.as_str()))
        .min_by_key(|r| r.due_at)
        .and_then(|r| s.catalog.problem(&r.problem_id).and_then(|(_, p)| p.dsa.as_ref().map(|d| (r, p, d))))
        .map(|(r, p, d)| json!({ "id": p.id, "slug": d.slug, "title": p.meta.title, "due": r.due_at.with_timezone(&chrono::Local).date_naive() }));
    let next_new = plan
        .next_up
        .first()
        .and_then(|id| s.catalog.problem(id))
        .and_then(|(_, p)| p.dsa.as_ref().map(|d| json!({ "id": p.id, "slug": d.slug, "title": p.meta.title })));

    let value = json!({
        "today": today,
        "capacity": plan.capacity,
        "due": plan.due,
        "solve_day": plan.solve_day,
        "items": items,
        "ahead": ahead,
        "next_new": next_new,
        "next_review": next_review,
        "grades": [Grade::Again.as_str(), Grade::Hard.as_str(), Grade::Good.as_str(), Grade::Easy.as_str()],
    });
    Ok(Json(value))
}
