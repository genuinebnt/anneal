//! Account flows: registration, email verification, login, refresh rotation, API keys, members.
//!
//! Each state change is one transaction that also writes its audit row.
//! Recipes: `examples/web/backend_password_accounts_totp_mfa.rs`, `examples/web/auth_basic_bearer_jwt.rs`.

use serde::Serialize;
use serde_json::json;
use sqlx::SqliteConnection;

use super::policy::{Action, Principal, Role, Via};
use super::token::{Claims, hash_secret};
use crate::db::{Actor, accounts, audit};
use crate::error::{AppError, AppResult, Validator, is_unique_violation};
use crate::jobs::queue::{self, NewJob};
use crate::jobs::worker::EMAIL_JOB;
use crate::ports::Email;
use crate::state::App;

pub const API_KEY_PREFIX: &str = "bk_";
const VERIFY_PURPOSE: &str = "verify";
/// An upper bound matters: hashing cost grows with input size.
const PASSWORD_RANGE: (usize, usize) = (10, 128);

const BAD_CREDENTIALS: AppError = AppError::Unauthenticated("invalid email or password");
const BAD_TOKEN: AppError = AppError::Unauthenticated("invalid or expired token");

pub struct Registration {
    pub org_name: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
}

fn check_password(v: &mut Validator, password: &str) {
    let n = password.chars().count();
    if !(PASSWORD_RANGE.0..=PASSWORD_RANGE.1).contains(&n) {
        v.fail(
            "password",
            "bad_length",
            format!("must have {} to {} characters", PASSWORD_RANGE.0, PASSWORD_RANGE.1),
        );
    }
}

fn map_duplicate_email(e: sqlx::Error) -> AppError {
    if is_unique_violation(&e) { AppError::Conflict("email already registered".into()) } else { e.into() }
}

async fn queue_verification_mail(app: &App, c: &mut SqliteConnection, user_id: &str, email: &str) -> AppResult<()> {
    let token = app.deps.ids.secret();
    let now = app.now_ms();
    let expires = now + app.cfg.auth.verify_ttl_secs * 1000;
    accounts::insert_email_token(c, &hash_secret(&token), user_id, VERIFY_PURPOSE, expires).await?;
    let mail = Email {
        to: email.to_string(),
        subject: "Verify your email".into(),
        body: format!("Open {}/verify?token={token} to activate your account.", app.cfg.public_url),
    };
    let payload = serde_json::to_value(&mail).map_err(AppError::internal)?;
    let job = NewJob {
        id: &app.new_id(),
        kind: EMAIL_JOB,
        payload: &payload,
        run_at: now,
        max_attempts: app.cfg.jobs.max_attempts,
        dedupe_key: None,
    };
    queue::enqueue(c, job).await?;
    Ok(())
}

/// Creates an organisation and its unverified owner. The mail is queued in the same transaction, so an
/// account can never exist without a pending verification mail.
///
/// Known trade-off: the 409 reveals that an email is registered. Hiding that means always answering 202
/// and mailing "you already have an account" instead.
pub async fn register(app: &App, r: Registration) -> AppResult<(String, String)> {
    let mut v = Validator::new();
    v.text("org_name", &r.org_name, 1, 100).email("email", &r.email);
    check_password(&mut v, &r.password);
    v.finish()?;
    // Hash BEFORE opening the transaction: it takes SQLite's single write lock.
    let hash = app.hasher.hash(r.password).await?;
    let (org_id, user_id, now) = (app.new_id(), app.new_id(), app.now_ms());
    let email = r.email.to_lowercase();
    let mut tx = app.db.begin().await?;
    accounts::insert_org(&mut tx, &org_id, r.org_name.trim(), now).await?;
    let user = accounts::NewUser {
        id: &user_id,
        org_id: &org_id,
        email: &email,
        password_hash: &hash,
        role: Role::Owner.as_str(),
        verified: false,
        now,
    };
    accounts::insert_user(&mut tx, &user).await.map_err(map_duplicate_email)?;
    queue_verification_mail(app, &mut tx, &user_id, &email).await?;
    audit(
        &mut tx,
        Actor { org_id: &org_id, user_id: &user_id },
        "user.registered",
        "user",
        &user_id,
        now,
        &json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok((org_id, user_id))
}

pub async fn verify_email(app: &App, token: &str) -> AppResult<()> {
    let now = app.now_ms();
    let mut tx = app.db.begin().await?;
    let user_id = accounts::consume_email_token(&mut tx, &hash_secret(token), VERIFY_PURPOSE, now)
        .await?
        .ok_or_else(|| AppError::BadRequest("invalid or expired verification token".into()))?;
    accounts::set_verified(&mut tx, &user_id, now).await?;
    let user = accounts::user_by_id(&mut tx, &user_id).await?.ok_or(AppError::NotFound("user"))?;
    audit(
        &mut tx,
        Actor { org_id: &user.org_id, user_id: &user_id },
        "user.verified",
        "user",
        &user_id,
        now,
        &json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Insert the next refresh token of a family and mint the access token that goes with it.
async fn issue(app: &App, c: &mut SqliteConnection, user_id: &str, family: &str) -> AppResult<TokenPair> {
    let refresh = app.deps.ids.secret();
    let now = app.now_ms();
    let refresh_expires = now + app.cfg.auth.refresh_ttl_secs * 1000;
    accounts::insert_refresh(c, &app.new_id(), family, user_id, &hash_secret(&refresh), refresh_expires).await?;
    let ttl = app.cfg.auth.access_ttl_secs;
    let claims = Claims { sub: user_id.into(), sid: family.into(), exp: now / 1000 + ttl };
    Ok(TokenPair {
        access_token: app.signer.mint(&claims),
        refresh_token: refresh,
        token_type: "Bearer",
        expires_in: ttl,
    })
}

pub async fn login(app: &App, email: &str, password: String) -> AppResult<TokenPair> {
    let email = email.to_lowercase();
    if password.chars().count() > PASSWORD_RANGE.1 {
        return Err(BAD_CREDENTIALS);
    }
    app.throttle
        .try_acquire(&email, app.now_ms())
        .map_err(|retry_after_secs| AppError::RateLimited { retry_after_secs })?;
    let user = {
        let mut c = app.db.pool().acquire().await?;
        accounts::user_by_email(&mut c, &email).await?
    };
    // Unknown email and wrong password take the same path and give the same answer.
    let stored = user.as_ref().map(|u| u.password_hash.clone());
    let ok = app.hasher.verify(password, stored).await?;
    let Some(user) = user.filter(|_| ok) else { return Err(BAD_CREDENTIALS) };
    // Reported only AFTER the password check, so it reveals nothing to someone without the password.
    if user.verified_at.is_none() {
        return Err(AppError::Forbidden);
    }
    app.throttle.reset(&email);
    let mut tx = app.db.begin().await?;
    let family = app.new_id();
    let pair = issue(app, &mut tx, &user.id, &family).await?;
    audit(
        &mut tx,
        Actor { org_id: &user.org_id, user_id: &user.id },
        "session.created",
        "session",
        &family,
        app.now_ms(),
        &json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(pair)
}

/// Rotate a refresh token. Each token works once. A second use means two parties hold it, and we cannot
/// tell which one is the thief, so the whole family is revoked and both must log in again.
pub async fn refresh(app: &App, presented: &str) -> AppResult<TokenPair> {
    let now = app.now_ms();
    // BEGIN IMMEDIATE serialises concurrent rotations of the same token.
    let mut tx = app.db.begin().await?;
    let row = accounts::refresh_by_hash(&mut tx, &hash_secret(presented)).await?.ok_or(BAD_TOKEN)?;
    if row.revoked_at.is_some() {
        return Err(BAD_TOKEN);
    }
    if row.rotated_at.is_some() {
        accounts::revoke_family(&mut tx, &row.family_id, now).await?;
        let user = accounts::user_by_id(&mut tx, &row.user_id).await?.ok_or(BAD_TOKEN)?;
        audit(
            &mut tx,
            Actor { org_id: &user.org_id, user_id: &user.id },
            "session.reuse_detected",
            "session",
            &row.family_id,
            now,
            &json!({}),
        )
        .await?;
        // Commit BEFORE returning the error: `?` or a plain return would roll the revocation back.
        tx.commit().await?;
        return Err(BAD_TOKEN);
    }
    if now >= row.expires_at || !accounts::mark_rotated(&mut tx, &row.id, now).await? {
        return Err(BAD_TOKEN);
    }
    let pair = issue(app, &mut tx, &row.user_id, &row.family_id).await?;
    tx.commit().await?;
    Ok(pair)
}

pub async fn logout(app: &App, p: &Principal) -> AppResult<()> {
    let Via::Session(family) = &p.via else {
        return Err(AppError::BadRequest("API keys have no session; delete the key instead".into()));
    };
    let mut tx = app.db.begin().await?;
    accounts::revoke_family(&mut tx, family, app.now_ms()).await?;
    audit(
        &mut tx,
        Actor { org_id: &p.org_id, user_id: &p.user_id },
        "session.revoked",
        "session",
        family,
        app.now_ms(),
        &json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Turn a bearer credential into a `Principal`.
///
/// The user row is read on every request, so role changes and logout take effect immediately. The cost
/// is one or two indexed reads; purely stateless tokens would stay valid until they expire.
pub async fn authenticate(app: &App, credential: &str) -> AppResult<Principal> {
    let mut c = app.db.pool().acquire().await?;
    let (user_id, via) = if credential.starts_with(API_KEY_PREFIX) {
        let key = accounts::api_key_by_hash(&mut c, &hash_secret(credential)).await?.ok_or(BAD_TOKEN)?;
        (key.user_id, Via::ApiKey(key.id))
    } else {
        let claims = app.signer.verify(credential, app.now_ms() / 1000).map_err(|_| BAD_TOKEN)?;
        let live: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM refresh_tokens WHERE family_id = ? AND user_id = ? AND revoked_at IS NULL LIMIT 1",
        )
        .bind(&claims.sid)
        .bind(&claims.sub)
        .fetch_optional(&mut *c)
        .await?;
        live.ok_or(BAD_TOKEN)?;
        (claims.sub, Via::Session(claims.sid))
    };
    let user = accounts::user_by_id(&mut c, &user_id).await?.ok_or(BAD_TOKEN)?;
    let role = Role::parse(&user.role).ok_or(BAD_TOKEN)?;
    Ok(Principal { user_id: user.id, org_id: user.org_id, role, via })
}

/// Returns `(id, secret)`. The secret is shown exactly once; only its hash is stored.
pub async fn create_api_key(app: &App, p: &Principal, name: &str) -> AppResult<(String, String)> {
    p.require(Action::ManageApiKeys)?;
    let mut v = Validator::new();
    v.text("name", name, 1, 100);
    v.finish()?;
    let (id, secret) = (app.new_id(), format!("{API_KEY_PREFIX}{}", app.deps.ids.secret()));
    let now = app.now_ms();
    let mut tx = app.db.begin().await?;
    accounts::insert_api_key(&mut tx, &id, &p.org_id, &p.user_id, name.trim(), &hash_secret(&secret), now).await?;
    audit(
        &mut tx,
        Actor { org_id: &p.org_id, user_id: &p.user_id },
        "api_key.created",
        "api_key",
        &id,
        now,
        &json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok((id, secret))
}

/// A user may revoke their own keys; admins may revoke any key of the org.
pub async fn revoke_api_key(app: &App, p: &Principal, id: &str) -> AppResult<()> {
    p.require(Action::ManageApiKeys)?;
    let now = app.now_ms();
    let mut tx = app.db.begin().await?;
    let owner: String = sqlx::query_scalar("SELECT user_id FROM api_keys WHERE id = ? AND org_id = ?")
        .bind(id)
        .bind(&p.org_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound("api key"))?;
    p.require(Action::Delete { creator: &owner })?;
    if !accounts::revoke_api_key(&mut tx, &p.org_id, id, now).await? {
        return Err(AppError::NotFound("api key"));
    }
    audit(
        &mut tx,
        Actor { org_id: &p.org_id, user_id: &p.user_id },
        "api_key.revoked",
        "api_key",
        id,
        now,
        &json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// An admin adds a pre-verified user. A real product would send an invitation instead.
pub async fn add_member(app: &App, p: &Principal, email: &str, password: String, role: &str) -> AppResult<String> {
    let mut v = Validator::new();
    v.email("email", email).one_of("role", role, &["viewer", "member", "admin", "owner"]);
    check_password(&mut v, &password);
    v.finish()?;
    let role = Role::parse(role).ok_or(AppError::BadRequest("unknown role".into()))?;
    p.require(Action::AddMember(role))?;
    let hash = app.hasher.hash(password).await?;
    let (id, now) = (app.new_id(), app.now_ms());
    let mut tx = app.db.begin().await?;
    let user = accounts::NewUser {
        id: &id,
        org_id: &p.org_id,
        email,
        password_hash: &hash,
        role: role.as_str(),
        verified: true,
        now,
    };
    accounts::insert_user(&mut tx, &user).await.map_err(map_duplicate_email)?;
    audit(
        &mut tx,
        Actor { org_id: &p.org_id, user_id: &p.user_id },
        "member.added",
        "user",
        &id,
        now,
        &json!({"role": role.as_str()}),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}
