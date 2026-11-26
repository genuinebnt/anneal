//! Auth, members, API keys, webhooks and the audit log.

use axum::Json;
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Deserialize;
use serde_json::json;

use super::record;
use crate::auth::policy::{Action, Principal};
use crate::auth::service::{self, Registration};
use crate::config::Environment;
use crate::db::{accounts, audit_list};
use crate::error::{AppResult, Validator};
use crate::http::extract::ApiJson;
use crate::jobs::webhook::validate_url;
use crate::state::AppState;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterBody {
    org_name: String,
    email: String,
    password: String,
}

pub async fn register(
    State(app): State<AppState>,
    ApiJson(b): ApiJson<RegisterBody>,
) -> AppResult<impl IntoResponse> {
    let r = Registration { org_name: b.org_name, email: b.email, password: b.password };
    let (org_id, user_id) = service::register(&app, r).await?;
    Ok((StatusCode::CREATED, Json(json!({ "org_id": org_id, "user_id": user_id }))))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenBody {
    token: String,
}

pub async fn verify(State(app): State<AppState>, ApiJson(b): ApiJson<TokenBody>) -> AppResult<StatusCode> {
    service::verify_email(&app, &b.token).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginBody {
    email: String,
    password: String,
}

pub async fn login(State(app): State<AppState>, ApiJson(b): ApiJson<LoginBody>) -> AppResult<impl IntoResponse> {
    Ok(Json(service::login(&app, &b.email, b.password).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshBody {
    refresh_token: String,
}

pub async fn refresh(State(app): State<AppState>, ApiJson(b): ApiJson<RefreshBody>) -> AppResult<impl IntoResponse> {
    Ok(Json(service::refresh(&app, &b.refresh_token).await?))
}

pub async fn logout(State(app): State<AppState>, Extension(p): Extension<Principal>) -> AppResult<StatusCode> {
    service::logout(&app, &p).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn me(Extension(p): Extension<Principal>) -> impl IntoResponse {
    Json(json!({ "user_id": p.user_id, "org_id": p.org_id, "role": p.role.as_str() }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberBody {
    email: String,
    password: String,
    role: String,
}

pub async fn add_member(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
    ApiJson(b): ApiJson<MemberBody>,
) -> AppResult<impl IntoResponse> {
    let id = service::add_member(&app, &p, &b.email, b.password, &b.role).await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiKeyBody {
    name: String,
}

pub async fn create_api_key(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
    ApiJson(b): ApiJson<ApiKeyBody>,
) -> AppResult<impl IntoResponse> {
    let (id, key) = service::create_api_key(&app, &p, &b.name).await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id, "key": key }))))
}

pub async fn revoke_api_key(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    service::revoke_api_key(&app, &p, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebhookBody {
    url: String,
}

/// The signing secret is returned once. It has to be stored readable, because we sign with it: encrypt
/// it at rest with a KMS key in production.
pub async fn create_webhook(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
    ApiJson(b): ApiJson<WebhookBody>,
) -> AppResult<impl IntoResponse> {
    p.require(Action::ManageWebhooks)?;
    let mut v = Validator::new();
    if let Err(why) = validate_url(&b.url, app.cfg.environment == Environment::Prod) {
        v.fail("url", "invalid_url", why);
    }
    v.finish()?;
    let (id, secret, now) = (app.new_id(), app.deps.ids.secret(), app.now_ms());
    let mut tx = app.db.begin().await?;
    accounts::insert_webhook(&mut tx, &id, &p.org_id, &b.url, &secret, now).await?;
    record(&mut tx, &p, "webhook.created", "webhook", &id, json!({}), now).await?;
    tx.commit().await?;
    // NOTE: this route is NOT idempotency-wrapped, because the stored replay would hold the secret.
    Ok((StatusCode::CREATED, Json(json!({ "id": id, "url": b.url, "secret": secret }))))
}

pub async fn list_webhooks(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
) -> AppResult<impl IntoResponse> {
    p.require(Action::ManageWebhooks)?;
    let mut c = app.db.pool().acquire().await?;
    let items = accounts::webhooks_for_org(&mut c, &p.org_id).await?;
    Ok(Json(json!({ "items": items })))
}

pub async fn audit_log(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
) -> AppResult<impl IntoResponse> {
    p.require(Action::ReadAudit)?;
    let mut c = app.db.pool().acquire().await?;
    // Unpaginated to keep the example short: see docs/EXTEND.md.
    let items = audit_list(&mut c, &p.org_id).await?;
    Ok(Json(json!({ "items": items })))
}
