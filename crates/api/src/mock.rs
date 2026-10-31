//! The mock interview (docs/DSA.md, decision 25): a configurable timed round of random LeetCode problems.
//!
//! The web app filters and draws from `GET /api/dsa/mock` (every DSA problem, the practice ones too, with how each
//! stands) and logs each problem through the ordinary `POST /api/dsa/problems/{id}/log`. This module keeps the
//! settings the owner last used, and the history of finished rounds.

use std::collections::HashMap;

use axum::Json;
use axum::response::IntoResponse;
use axum::extract::State;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::types::Json as Jsonb;

use crate::AppState;
use crate::dsa::{dsa_reviews, dsa_tracks, practice_state, row};
use crate::error::{ApiError, ApiResult};
use crate::store::{self, ReviewRow};

/// Rounds kept in the history shown on the page.
const HISTORY: i64 = 20;
const MAX_CONFIG_BYTES: usize = 32 * 1024;
const MAX_ITEMS: usize = 12;

/// A stored round: id, when it finished, its settings, its items and the seconds it ran.
type RoundRow = (i64, DateTime<Utc>, Jsonb<Value>, Jsonb<Value>, i32);

pub async fn get(State(s): State<AppState>) -> ApiResult<axum::response::Response> {
    let today = crate::activity::today();
    let progress = store::progress(&s.db).await?;
    let reviews = dsa_reviews(&s.catalog, store::reviews(&s.db).await?);
    let by_problem: HashMap<&str, &ReviewRow> =
        reviews.iter().map(|r| (r.problem_id.as_str(), r)).collect();
    let mut problems: Vec<_> = dsa_tracks(&s.catalog)
        .flat_map(|t| t.problems.iter().map(move |p| (t, p)))
        .map(|(t, p)| row(t, p, &progress, &by_problem, today))
        .collect();
    // Practice problems stand by their latest attempt: they have no review.
    let none: HashMap<&str, &ReviewRow> = HashMap::new();
    for t in &s.catalog.practice_tracks {
        for p in &t.problems {
            let mut r = row(t, p, &progress, &none, today);
            r.state = practice_state(progress.get(&p.id));
            problems.push(r);
        }
    }
    let patterns: Vec<Value> = dsa_tracks(&s.catalog)
        .map(|t| json!({ "code": t.code, "name": t.name }))
        .collect();
    let saved: Option<Jsonb<Value>> =
        sqlx::query_scalar("SELECT value FROM settings WHERE key = 'mock'")
            .fetch_optional(&s.db)
            .await?;
    let rounds: Vec<RoundRow> =
        sqlx::query_as("SELECT id, finished_at, config, items, seconds FROM mock_rounds ORDER BY finished_at DESC, id DESC LIMIT $1").bind(HISTORY).fetch_all(&s.db).await?;
    let rounds: Vec<Value> = rounds.into_iter().map(|(id, at, config, items, seconds)| json!({ "id": id, "finished_at": at, "config": config.0, "items": items.0, "seconds": seconds })).collect();
    // Serialized straight from the rows, never through a `Value`: that would widen each company's f32 frequency to f64 and
    // print it as 92.5999984741211 instead of 92.6, on every company of all 1,669 problems.
    let body = serde_json::to_string(&MockView {
        today,
        patterns,
        company_groups: &s.catalog.dsa.company_groups,
        problems,
        saved: saved.map(|j| j.0),
        rounds,
    })
    .expect("the mock view is plain data");
    Ok(([(axum::http::header::CONTENT_TYPE, "application/json")], body).into_response())
}

#[derive(Serialize)]
struct MockView<'a> {
    today: chrono::NaiveDate,
    patterns: Vec<Value>,
    company_groups: &'a [anneal_content::CompanyGroup],
    problems: Vec<crate::dsa::ProblemRow<'a>>,
    saved: Option<Value>,
    rounds: Vec<Value>,
}

/// Remembers the settings last used and the presets the owner saved. The page owns the shape; this only bounds it.
pub async fn put_config(State(s): State<AppState>, Json(v): Json<Value>) -> ApiResult<Json<Value>> {
    if !v.is_object() {
        return Err(ApiError::BadRequest(
            "mock settings must be an object".into(),
        ));
    }
    if v.to_string().len() > MAX_CONFIG_BYTES {
        return Err(ApiError::BadRequest("mock settings are too large".into()));
    }
    crate::settings::store(&s, "mock", &v).await?;
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct RoundBody {
    /// The settings the round was drawn with.
    config: Value,
    items: Vec<Item>,
    /// Time used, in seconds.
    seconds: u32,
}

#[derive(Deserialize, Serialize)]
struct Item {
    id: String,
    /// `again`, `hard` or `good`; none when the problem was never logged.
    grade: Option<String>,
    seconds: u32,
}

pub async fn post_round(
    State(s): State<AppState>,
    Json(body): Json<RoundBody>,
) -> ApiResult<Json<Value>> {
    if body.items.is_empty() || body.items.len() > MAX_ITEMS {
        return Err(ApiError::BadRequest(format!(
            "a round has 1 to {MAX_ITEMS} problems"
        )));
    }
    if !body.config.is_object() || body.config.to_string().len() > MAX_CONFIG_BYTES {
        return Err(ApiError::BadRequest(
            "the round's settings must be a small object".into(),
        ));
    }
    for item in &body.items {
        if !s
            .catalog
            .problem(&item.id)
            .is_some_and(|(_, p)| p.dsa.is_some())
        {
            return Err(ApiError::NotFound(format!("problem {}", item.id)));
        }
        if item
            .grade
            .as_deref()
            .is_some_and(|g| !["again", "hard", "good", "easy"].contains(&g))
        {
            return Err(ApiError::BadRequest(format!(
                "{}: grade must be again, hard, good or easy",
                item.id
            )));
        }
    }
    let seconds = i32::try_from(body.seconds)
        .map_err(|_| ApiError::BadRequest("seconds is too large".into()))?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO mock_rounds (config, items, seconds) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(Jsonb(&body.config))
    .bind(Jsonb(&body.items))
    .bind(seconds)
    .fetch_one(&s.db)
    .await?;
    Ok(Json(json!({ "id": id })))
}
