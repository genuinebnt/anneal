//! Tasks and comments.

use axum::Json;
use axum::extract::{Extension, Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use super::{projects, record};
use crate::auth::policy::{Action, Principal};
use crate::db::tasks::{self, Comment, Task, TaskPatch};
use crate::db::{Write, accounts};
use crate::error::{AppError, AppResult, Validator};
use crate::http::etag::{conditional, require_if_match, tagged, version_etag};
use crate::http::extract::{ApiJson, QueryPairs, double_option};
use crate::query::{TASK_STATUSES, parse_task_filter};
use crate::state::{App, AppState};

const NOT_FOUND: AppError = AppError::NotFound("task");
const MAX_PRIORITY: i64 = 5;

pub async fn load(app: &App, p: &Principal, id: &str) -> AppResult<Task> {
    let mut c = app.db.pool().acquire().await?;
    tasks::get(&mut c, &p.org_id, id).await?.ok_or(NOT_FOUND)
}

fn invalidate(app: &App, t: &Task) {
    app.stats_cache.invalidate(&projects::cache_tag(&t.org_id, &t.project_id));
}

/// A foreign key alone would let a task point at another tenant's user.
async fn check_assignee(app: &App, org_id: &str, assignee: &str) -> AppResult<()> {
    let mut c = app.db.pool().acquire().await?;
    let user = accounts::user_by_id(&mut c, assignee).await?;
    if user.is_some_and(|u| u.org_id == org_id) {
        return Ok(());
    }
    let mut v = Validator::new();
    v.fail("assignee_id", "unknown_user", "not a member of this organisation");
    v.finish()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewTask {
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    priority: i64,
    assignee_id: Option<String>,
}

pub async fn create(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
    Path(project_id): Path<String>,
    ApiJson(b): ApiJson<NewTask>,
) -> AppResult<Response> {
    projects::load(&app, &p, &project_id).await?;
    p.require(Action::Create)?;
    let mut v = Validator::new();
    v.text("title", &b.title, 1, 200).text("body", &b.body, 0, 20_000).range("priority", b.priority, 0, MAX_PRIORITY);
    v.finish()?;
    if let Some(a) = &b.assignee_id {
        check_assignee(&app, &p.org_id, a).await?;
    }
    let now = app.now_ms();
    let task = Task {
        id: app.new_id(),
        org_id: p.org_id.clone(),
        project_id,
        title: b.title.trim().to_string(),
        body: b.body,
        status: "open".into(),
        priority: b.priority,
        assignee_id: b.assignee_id,
        created_by: p.user_id.clone(),
        version: 1,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    };
    let mut tx = app.db.begin().await?;
    // Re-check under the write lock: the project may have been deleted since the load above.
    if crate::db::projects::get(&mut tx, &p.org_id, &task.project_id).await?.is_none() {
        return Err(projects::NOT_FOUND);
    }
    tasks::insert(&mut tx, &task).await?;
    let extra = json!({ "project_id": task.project_id });
    record(&mut tx, &p, "task.created", "task", &task.id, extra, now).await?;
    tx.commit().await?;
    invalidate(&app, &task);
    let mut res = tagged(StatusCode::CREATED, &version_etag(1), &task);
    if let Ok(location) = HeaderValue::from_str(&format!("/v1/tasks/{}", task.id)) {
        res.headers_mut().insert(header::LOCATION, location);
    }
    Ok(res)
}

async fn list_filtered(
    app: &App,
    p: &Principal,
    project: Option<&str>,
    query: &[(String, String)],
) -> AppResult<Response> {
    p.require(Action::Read)?;
    let filter = parse_task_filter(query)?;
    let mut c = app.db.pool().acquire().await?;
    Ok(Json(tasks::list(&mut c, &p.org_id, project, &filter).await?).into_response())
}

/// All tasks of the tenant: filtering, sorting, keyset pagination and full-text search (`q`).
pub async fn search(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
    QueryPairs(q): QueryPairs,
) -> AppResult<Response> {
    list_filtered(&app, &p, None, &q).await
}

pub async fn list_in_project(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
    Path(project_id): Path<String>,
    QueryPairs(q): QueryPairs,
) -> AppResult<Response> {
    projects::load(&app, &p, &project_id).await?;
    list_filtered(&app, &p, Some(&project_id), &q).await
}

pub async fn get(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Response> {
    p.require(Action::Read)?;
    let task = load(&app, &p, &id).await?;
    Ok(conditional(&headers, &version_etag(task.version), &task))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchTask {
    title: Option<String>,
    body: Option<String>,
    status: Option<String>,
    priority: Option<i64>,
    #[serde(default, deserialize_with = "double_option")]
    assignee_id: Option<Option<String>>,
}

impl PatchTask {
    fn validate(&self) -> AppResult<()> {
        let mut v = Validator::new();
        if let Some(t) = &self.title {
            v.text("title", t, 1, 200);
        }
        if let Some(b) = &self.body {
            v.text("body", b, 0, 20_000);
        }
        if let Some(s) = &self.status {
            v.one_of("status", s, &TASK_STATUSES);
        }
        if let Some(n) = self.priority {
            v.range("priority", n, 0, MAX_PRIORITY);
        }
        v.finish()
    }
}

pub async fn update(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
    Path(id): Path<String>,
    headers: HeaderMap,
    ApiJson(b): ApiJson<PatchTask>,
) -> AppResult<Response> {
    let current = load(&app, &p, &id).await?;
    let mut owners = vec![current.created_by.as_str()];
    owners.extend(current.assignee_id.as_deref());
    p.require(Action::Modify { owners: &owners })?;
    let expected = require_if_match(&headers)?;
    b.validate()?;
    if let Some(Some(a)) = &b.assignee_id {
        check_assignee(&app, &p.org_id, a).await?;
    }
    let patch = TaskPatch {
        title: b.title.map(|t| t.trim().to_string()),
        body: b.body,
        status: b.status,
        priority: b.priority,
        assignee: b.assignee_id,
    };
    let now = app.now_ms();
    // The permission check used `current`, so only that exact version may be written. If the row changes
    // after this line, the versioned UPDATE below refuses. Without this check, a request authorised
    // against an old assignee could be applied to a newer version.
    if current.version != expected {
        return Err(AppError::PreconditionFailed);
    }
    let mut tx = app.db.begin().await?;
    let updated = match tasks::update(&mut tx, &p.org_id, &id, expected, &patch, now).await? {
        Write::Done(t) => t,
        Write::Stale => return Err(AppError::PreconditionFailed),
        Write::NotFound => return Err(NOT_FOUND),
    };
    let extra = json!({ "version": updated.version, "status": updated.status });
    record(&mut tx, &p, "task.updated", "task", &id, extra, now).await?;
    tx.commit().await?;
    invalidate(&app, &updated);
    Ok(tagged(StatusCode::OK, &version_etag(updated.version), &updated))
}

pub async fn delete(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let current = load(&app, &p, &id).await?;
    p.require(Action::Delete { creator: &current.created_by })?;
    let now = app.now_ms();
    let mut tx = app.db.begin().await?;
    if tasks::soft_delete(&mut tx, &p.org_id, &id, now).await? != Write::Done(()) {
        return Err(NOT_FOUND);
    }
    record(&mut tx, &p, "task.deleted", "task", &id, json!({}), now).await?;
    tx.commit().await?;
    invalidate(&app, &current);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewComment {
    body: String,
}

pub async fn add_comment(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
    Path(task_id): Path<String>,
    ApiJson(b): ApiJson<NewComment>,
) -> AppResult<impl IntoResponse> {
    load(&app, &p, &task_id).await?;
    p.require(Action::Create)?;
    let mut v = Validator::new();
    v.text("body", &b.body, 1, 5_000);
    v.finish()?;
    let now = app.now_ms();
    let comment = Comment {
        id: app.new_id(),
        org_id: p.org_id.clone(),
        task_id,
        author_id: p.user_id.clone(),
        body: b.body,
        created_at: now,
    };
    let mut tx = app.db.begin().await?;
    tasks::insert_comment(&mut tx, &comment).await?;
    let extra = json!({ "task_id": comment.task_id });
    record(&mut tx, &p, "comment.created", "comment", &comment.id, extra, now).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(comment)))
}

pub async fn comments(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
    Path(task_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    p.require(Action::Read)?;
    load(&app, &p, &task_id).await?;
    let mut c = app.db.pool().acquire().await?;
    Ok(Json(json!({ "items": tasks::comments(&mut c, &p.org_id, &task_id).await? })))
}
