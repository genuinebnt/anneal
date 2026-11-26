//! Projects: the template aggregate. Create / list / read (ETag) / update (If-Match) / soft delete, plus
//! a cached aggregate endpoint.

use axum::Json;
use axum::extract::{Extension, Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::record;
use crate::auth::policy::{Action, Principal};
use crate::db::projects::{self, Project};
use crate::db::{Write, tasks};
use crate::error::{AppError, AppResult, Validator};
use crate::http::etag::{conditional, require_if_match, tagged, version_etag};
use crate::http::extract::{ApiJson, QueryPairs};
use crate::query::{Cursor, DEFAULT_LIMIT, MAX_LIMIT};
use crate::state::{App, AppState};

pub const NOT_FOUND: AppError = AppError::NotFound("project");

/// Tag under which everything derived from one project is cached. Includes the tenant.
#[must_use]
pub fn cache_tag(org_id: &str, project_id: &str) -> String {
    format!("project:{org_id}:{project_id}")
}

/// Fetch within the caller's tenant. Another tenant's id is indistinguishable from a missing one.
pub async fn load(app: &App, p: &Principal, id: &str) -> AppResult<Project> {
    let mut c = app.db.pool().acquire().await?;
    projects::get(&mut c, &p.org_id, id).await?.ok_or(NOT_FOUND)
}

fn written<T>(w: Write<T>) -> AppResult<T> {
    match w {
        Write::Done(v) => Ok(v),
        Write::Stale => Err(AppError::PreconditionFailed),
        Write::NotFound => Err(NOT_FOUND),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewProject {
    name: String,
    #[serde(default)]
    description: String,
}

pub async fn create(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
    ApiJson(b): ApiJson<NewProject>,
) -> AppResult<Response> {
    p.require(Action::Create)?;
    let mut v = Validator::new();
    v.text("name", &b.name, 1, 100).text("description", &b.description, 0, 5_000);
    v.finish()?;
    let now = app.now_ms();
    let project = Project {
        id: app.new_id(),
        org_id: p.org_id.clone(),
        name: b.name.trim().to_string(),
        description: b.description,
        created_by: p.user_id.clone(),
        version: 1,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    };
    let mut tx = app.db.begin().await?;
    projects::insert(&mut tx, &project).await?;
    record(&mut tx, &p, "project.created", "project", &project.id, json!({}), now).await?;
    tx.commit().await?;
    let mut res = tagged(StatusCode::CREATED, &version_etag(1), &project);
    if let Ok(location) = HeaderValue::from_str(&format!("/v1/projects/{}", project.id)) {
        res.headers_mut().insert(header::LOCATION, location);
    }
    Ok(res)
}

fn page_params(pairs: &[(String, String)]) -> AppResult<(Option<Cursor>, u32)> {
    let (mut cursor, mut limit) = (None, DEFAULT_LIMIT);
    let mut v = Validator::new();
    for (k, val) in pairs {
        match k.as_str() {
            "cursor" => cursor = Some(Cursor::decode(val, "-created_at")?),
            "limit" => match val.parse::<u32>() {
                Ok(n) if (1..=MAX_LIMIT).contains(&n) => limit = n,
                _ => v.fail("limit", "out_of_range", format!("must be between 1 and {MAX_LIMIT}")),
            },
            other => v.fail(other, "unknown_parameter", "unknown query parameter"),
        }
    }
    v.finish()?;
    Ok((cursor, limit))
}

pub async fn list(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
    QueryPairs(q): QueryPairs,
) -> AppResult<impl IntoResponse> {
    p.require(Action::Read)?;
    let (cursor, limit) = page_params(&q)?;
    let mut c = app.db.pool().acquire().await?;
    Ok(Json(projects::list(&mut c, &p.org_id, cursor.as_ref(), limit).await?))
}

pub async fn get(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Response> {
    p.require(Action::Read)?;
    let project = load(&app, &p, &id).await?;
    Ok(conditional(&headers, &version_etag(project.version), &project))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchProject {
    name: Option<String>,
    description: Option<String>,
}

pub async fn update(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
    Path(id): Path<String>,
    headers: HeaderMap,
    ApiJson(b): ApiJson<PatchProject>,
) -> AppResult<Response> {
    // Order matters: existence (404), then permission (403), then precondition (428/412).
    let current = load(&app, &p, &id).await?;
    p.require(Action::Modify { owners: &[&current.created_by] })?;
    let expected = require_if_match(&headers)?;
    let mut v = Validator::new();
    if let Some(name) = &b.name {
        v.text("name", name, 1, 100);
    }
    if let Some(d) = &b.description {
        v.text("description", d, 0, 5_000);
    }
    v.finish()?;
    let now = app.now_ms();
    let mut tx = app.db.begin().await?;
    // The version check happens INSIDE the UPDATE, so it cannot race with the read above.
    let name = b.name.as_deref().map(str::trim);
    let updated =
        written(projects::update(&mut tx, &p.org_id, &id, expected, name, b.description.as_deref(), now).await?)?;
    record(&mut tx, &p, "project.updated", "project", &id, json!({ "version": updated.version }), now).await?;
    tx.commit().await?;
    app.stats_cache.invalidate(&cache_tag(&p.org_id, &id));
    Ok(tagged(StatusCode::OK, &version_etag(updated.version), &updated))
}

pub async fn delete(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<StatusCode> {
    let current = load(&app, &p, &id).await?;
    p.require(Action::Delete { creator: &current.created_by })?;
    let expected = require_if_match(&headers)?;
    let now = app.now_ms();
    let mut tx = app.db.begin().await?;
    written(projects::soft_delete(&mut tx, &p.org_id, &id, Some(expected), now).await?)?;
    let removed = tasks::soft_delete_by_project(&mut tx, &p.org_id, &id, now).await?;
    record(&mut tx, &p, "project.deleted", "project", &id, json!({ "tasks_deleted": removed }), now).await?;
    tx.commit().await?;
    app.stats_cache.invalidate(&cache_tag(&p.org_id, &id));
    Ok(StatusCode::NO_CONTENT)
}

/// Task counts per status, served through the singleflight cache.
pub async fn stats(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Response> {
    p.require(Action::Read)?;
    // Authorization and existence are checked on EVERY request, cached or not.
    load(&app, &p, &id).await?;
    let tag = cache_tag(&p.org_id, &id);
    let body: Value = app
        .stats_cache
        .get_or_load(&format!("stats:{tag}"), &[&tag], app.now_ms(), || async {
            let mut c = app.db.pool().acquire().await?;
            let rows = tasks::status_counts(&mut c, &p.org_id, &id).await?;
            let by_status: serde_json::Map<String, Value> =
                rows.into_iter().map(|(status, n)| (status, json!(n))).collect();
            Ok::<_, AppError>(json!({ "project_id": id, "by_status": by_status }))
        })
        .await?;
    let etag = format!("\"{}\"", &hex::encode(Sha256::digest(body.to_string()))[..32]);
    Ok(conditional(&headers, &etag, &body))
}
