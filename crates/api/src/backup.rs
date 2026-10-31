//! A copy of everything the owner has made, as one JSON file, and the way back in.
//!
//! `GET /api/export` writes every progress table (attempts and runs, reviews, saved solutions, settings and preferences,
//! plans, course runs, the archives of what was reset or deleted). `POST /api/import` reads such a file and only
//! *adds*: a row whose key already exists is left alone, nothing is overwritten and nothing is deleted, so importing an
//! old backup into a database that has moved on cannot lose anything.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use sqlx::types::Json as Jsonb;

use crate::AppState;
use crate::error::{ApiError, ApiResult};

/// Parents before children (`attempts` before `runs`). Sessions and the cached LeetCode statements are not the owner's.
const TABLES: &[&str] = &[
    "settings",
    "attempts",
    "runs",
    "drafts",
    "scratch",
    "reviews",
    "focus_time",
    "dsa_solutions",
    "plan_overrides",
    "mock_rounds",
    "course_stage_state",
    "course_runs",
    "course_solutions",
    "course_concept_state",
    "course_run_starts",
    "course_repos",
    "reset_archive",
    "data_archive",
    "day_plan",
    "review_events",
    "streak_marks",
    "sd_attempts",
    "sd_cards",
    "dsa_read",
];

const FORMAT: &str = "anneal-backup";

/// `GET /api/export`: the backup file.
pub async fn export(State(s): State<AppState>) -> ApiResult<Response> {
    let mut tables = serde_json::Map::new();
    for table in TABLES {
        let rows: Jsonb<Value> = sqlx::query_scalar(&format!("SELECT COALESCE(jsonb_agg(to_jsonb(t)), '[]'::jsonb) FROM {table} t")).fetch_one(&s.db).await?;
        tables.insert((*table).to_owned(), rows.0);
    }
    let now = chrono::Utc::now();
    let body = serde_json::to_vec(&json!({ "format": FORMAT, "version": 1, "exported_at": now, "tables": tables })).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let name = format!("attachment; filename=\"anneal-backup-{}.json\"", now.format("%Y-%m-%d"));
    let mut res = body.into_response();
    res.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    res.headers_mut().insert(header::CONTENT_DISPOSITION, HeaderValue::from_str(&name).expect("an ASCII file name"));
    Ok(res)
}

/// `POST /api/import`: adds what a backup holds and the database lacks. Returns how many rows each table gained.
pub async fn import(State(s): State<AppState>, Json(file): Json<Value>) -> ApiResult<Json<Value>> {
    if file.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(ApiError::BadRequest("this is not an anneal backup file".into()));
    }
    let tables = file.get("tables").and_then(Value::as_object).ok_or_else(|| ApiError::BadRequest("the backup has no tables".into()))?;
    let mut tx = s.db.begin().await?;
    let mut added = serde_json::Map::new();
    for table in TABLES {
        let Some(rows) = tables.get(*table).filter(|r| r.as_array().is_some_and(|a| !a.is_empty())) else { continue };
        let sql = format!("INSERT INTO {table} OVERRIDING SYSTEM VALUE SELECT * FROM jsonb_populate_recordset(NULL::{table}, $1) ON CONFLICT DO NOTHING");
        let n = sqlx::query(&sql).bind(Jsonb(rows)).execute(&mut *tx).await?.rows_affected();
        added.insert((*table).to_owned(), json!(n));
        // Imported rows carry their own ids; the next row the app makes must not collide with them.
        let has_id: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = $1 AND column_name = 'id')").bind(table).fetch_one(&mut *tx).await?;
        if has_id {
            sqlx::query(&format!("SELECT setval(pg_get_serial_sequence('{table}', 'id'), (SELECT GREATEST(COALESCE(max(id), 1), 1) FROM {table}))")).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    Ok(Json(json!({ "added": added })))
}
