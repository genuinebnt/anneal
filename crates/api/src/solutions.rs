//! The owner's own solutions to DSA problems: any number per problem, each with a label, the code and notes.
//!
//! `GET /api/dsa/problems/{id}/solutions` lists them oldest first, `POST` adds one, and
//! `PUT`/`DELETE /api/dsa/solutions/{id}` change or remove one.

use axum::Json;
use axum::extract::{Path, State};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::error::{ApiError, ApiResult};

const MAX_CODE: usize = 100_000;
const MAX_NOTES: usize = 20_000;
const MAX_LABEL: usize = 80;

#[derive(Serialize, sqlx::FromRow)]
pub struct Solution {
    id: i64,
    problem_id: String,
    label: String,
    code: String,
    notes: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct Body {
    #[serde(default)]
    label: String,
    code: String,
    #[serde(default)]
    notes: String,
}

impl Body {
    fn check(&self) -> ApiResult<()> {
        if self.code.trim().is_empty() {
            return Err(ApiError::BadRequest("a solution needs some code".into()));
        }
        if self.code.len() > MAX_CODE || self.notes.len() > MAX_NOTES || self.label.chars().count() > MAX_LABEL {
            return Err(ApiError::BadRequest(format!("keep the label under {MAX_LABEL} characters, the notes under {MAX_NOTES} and the code under {MAX_CODE}")));
        }
        Ok(())
    }
}

fn dsa_problem(s: &AppState, id: &str) -> ApiResult<()> {
    let (_, p) = s.catalog.problem(id).ok_or_else(|| ApiError::NotFound(format!("problem {id}")))?;
    if p.dsa.is_none() {
        return Err(ApiError::BadRequest(format!("{id} isn't a DSA problem; solve it in the workspace")));
    }
    Ok(())
}

pub async fn list(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Vec<Solution>>> {
    dsa_problem(&s, &id)?;
    let rows = sqlx::query_as("SELECT id, problem_id, label, code, notes, created_at, updated_at FROM dsa_solutions WHERE problem_id = $1 ORDER BY created_at, id")
        .bind(&id)
        .fetch_all(&s.db)
        .await?;
    Ok(Json(rows))
}

pub async fn create(State(s): State<AppState>, Path(id): Path<String>, Json(body): Json<Body>) -> ApiResult<Json<Solution>> {
    dsa_problem(&s, &id)?;
    body.check()?;
    let row = sqlx::query_as(
        "INSERT INTO dsa_solutions (problem_id, label, code, notes) VALUES ($1, $2, $3, $4)
         RETURNING id, problem_id, label, code, notes, created_at, updated_at",
    )
    .bind(&id)
    .bind(body.label.trim())
    .bind(&body.code)
    .bind(&body.notes)
    .fetch_one(&s.db)
    .await?;
    Ok(Json(row))
}

pub async fn update(State(s): State<AppState>, Path(sid): Path<i64>, Json(body): Json<Body>) -> ApiResult<Json<Solution>> {
    body.check()?;
    let row = sqlx::query_as(
        "UPDATE dsa_solutions SET label = $2, code = $3, notes = $4, updated_at = now() WHERE id = $1
         RETURNING id, problem_id, label, code, notes, created_at, updated_at",
    )
    .bind(sid)
    .bind(body.label.trim())
    .bind(&body.code)
    .bind(&body.notes)
    .fetch_optional(&s.db)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("solution {sid}")))?;
    Ok(Json(row))
}

pub async fn delete(State(s): State<AppState>, Path(sid): Path<i64>) -> ApiResult<Json<serde_json::Value>> {
    let mut tx = s.db.begin().await?;
    crate::store::archive_rows(&mut tx, "delete_solution", "dsa_solutions", "id", &[sid.to_string()]).await?;
    let gone = sqlx::query("DELETE FROM dsa_solutions WHERE id = $1").bind(sid).execute(&mut *tx).await?.rows_affected();
    tx.commit().await?;
    if gone == 0 {
        return Err(ApiError::NotFound(format!("solution {sid}")));
    }
    Ok(Json(serde_json::json!({ "deleted": sid })))
}
