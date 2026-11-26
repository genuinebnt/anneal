//! Repositories for the identity aggregate: orgs, users, one-time tokens, refresh tokens, API keys, webhooks.

use serde::Serialize;
use sqlx::SqliteConnection;

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct User {
    pub id: String,
    pub org_id: String,
    pub email: String,
    #[serde(skip)]
    pub password_hash: String,
    pub role: String,
    pub verified_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RefreshRow {
    pub id: String,
    pub family_id: String,
    pub user_id: String,
    pub expires_at: i64,
    pub rotated_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct ApiKeyRow {
    pub id: String,
    pub org_id: String,
    pub user_id: String,
    pub name: String,
    pub created_at: i64,
    pub revoked_at: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct Webhook {
    pub id: String,
    pub org_id: String,
    pub url: String,
    #[serde(skip)]
    pub secret: String,
    pub created_at: i64,
}

pub async fn insert_org(c: &mut SqliteConnection, id: &str, name: &str, now: i64) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO orgs (id, name, created_at) VALUES (?,?,?)")
        .bind(id)
        .bind(name)
        .bind(now)
        .execute(c)
        .await
        .map(|_| ())
}

pub struct NewUser<'a> {
    pub id: &'a str,
    pub org_id: &'a str,
    pub email: &'a str,
    pub password_hash: &'a str,
    pub role: &'a str,
    pub verified: bool,
    pub now: i64,
}

/// Fails with a unique violation when the email exists; the caller maps that to 409.
pub async fn insert_user(c: &mut SqliteConnection, u: &NewUser<'_>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO users (id, org_id, email, password_hash, role, verified_at, created_at) VALUES (?,?,?,?,?,?,?)",
    )
    .bind(u.id)
    .bind(u.org_id)
    .bind(u.email.to_lowercase())
    .bind(u.password_hash)
    .bind(u.role)
    .bind(u.verified.then_some(u.now))
    .bind(u.now)
    .execute(c)
    .await
    .map(|_| ())
}

pub async fn user_by_email(c: &mut SqliteConnection, email: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM users WHERE email = ?").bind(email.to_lowercase()).fetch_optional(c).await
}

pub async fn user_by_id(c: &mut SqliteConnection, id: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM users WHERE id = ?").bind(id).fetch_optional(c).await
}

pub async fn set_verified(c: &mut SqliteConnection, user_id: &str, now: i64) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET verified_at = COALESCE(verified_at, ?) WHERE id = ?")
        .bind(now)
        .bind(user_id)
        .execute(c)
        .await
        .map(|_| ())
}

pub async fn insert_email_token(
    c: &mut SqliteConnection,
    hash: &str,
    user_id: &str,
    purpose: &str,
    expires_at: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO email_tokens (token_hash, user_id, purpose, expires_at) VALUES (?,?,?,?)")
        .bind(hash)
        .bind(user_id)
        .bind(purpose)
        .bind(expires_at)
        .execute(c)
        .await
        .map(|_| ())
}

/// Single-use: the UPDATE both checks and consumes, so two concurrent clicks cannot both succeed.
pub async fn consume_email_token(
    c: &mut SqliteConnection,
    hash: &str,
    purpose: &str,
    now: i64,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "UPDATE email_tokens SET used_at = ? WHERE token_hash = ? AND purpose = ? AND used_at IS NULL AND expires_at > ? RETURNING user_id",
    )
    .bind(now)
    .bind(hash)
    .bind(purpose)
    .bind(now)
    .fetch_optional(c)
    .await
}

pub async fn insert_refresh(
    c: &mut SqliteConnection,
    id: &str,
    family: &str,
    user_id: &str,
    hash: &str,
    expires_at: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO refresh_tokens (id, family_id, user_id, token_hash, expires_at) VALUES (?,?,?,?,?)")
        .bind(id)
        .bind(family)
        .bind(user_id)
        .bind(hash)
        .bind(expires_at)
        .execute(c)
        .await
        .map(|_| ())
}

pub async fn refresh_by_hash(c: &mut SqliteConnection, hash: &str) -> Result<Option<RefreshRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, family_id, user_id, expires_at, rotated_at, revoked_at FROM refresh_tokens WHERE token_hash = ?",
    )
    .bind(hash)
    .fetch_optional(c)
    .await
}

/// Atomic claim of a refresh token: exactly one concurrent caller gets `true`.
pub async fn mark_rotated(c: &mut SqliteConnection, id: &str, now: i64) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE refresh_tokens SET rotated_at = ? WHERE id = ? AND rotated_at IS NULL AND revoked_at IS NULL",
    )
    .bind(now)
    .bind(id)
    .execute(c)
    .await?;
    Ok(r.rows_affected() == 1)
}

pub async fn revoke_family(c: &mut SqliteConnection, family: &str, now: i64) -> Result<u64, sqlx::Error> {
    sqlx::query("UPDATE refresh_tokens SET revoked_at = COALESCE(revoked_at, ?) WHERE family_id = ?")
        .bind(now)
        .bind(family)
        .execute(c)
        .await
        .map(|r| r.rows_affected())
}

pub async fn insert_api_key(
    c: &mut SqliteConnection,
    id: &str,
    org_id: &str,
    user_id: &str,
    name: &str,
    hash: &str,
    now: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO api_keys (id, org_id, user_id, name, key_hash, created_at) VALUES (?,?,?,?,?,?)")
        .bind(id)
        .bind(org_id)
        .bind(user_id)
        .bind(name)
        .bind(hash)
        .bind(now)
        .execute(c)
        .await
        .map(|_| ())
}

pub async fn api_key_by_hash(c: &mut SqliteConnection, hash: &str) -> Result<Option<ApiKeyRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, org_id, user_id, name, created_at, revoked_at FROM api_keys WHERE key_hash = ? AND revoked_at IS NULL",
    )
    .bind(hash)
    .fetch_optional(c)
    .await
}

pub async fn revoke_api_key(c: &mut SqliteConnection, org_id: &str, id: &str, now: i64) -> Result<bool, sqlx::Error> {
    let r = sqlx::query("UPDATE api_keys SET revoked_at = ? WHERE id = ? AND org_id = ? AND revoked_at IS NULL")
        .bind(now)
        .bind(id)
        .bind(org_id)
        .execute(c)
        .await?;
    Ok(r.rows_affected() == 1)
}

pub async fn insert_webhook(
    c: &mut SqliteConnection,
    id: &str,
    org_id: &str,
    url: &str,
    secret: &str,
    now: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO webhooks (id, org_id, url, secret, created_at) VALUES (?,?,?,?,?)")
        .bind(id)
        .bind(org_id)
        .bind(url)
        .bind(secret)
        .bind(now)
        .execute(c)
        .await
        .map(|_| ())
}

pub async fn webhooks_for_org(c: &mut SqliteConnection, org_id: &str) -> Result<Vec<Webhook>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM webhooks WHERE org_id = ? ORDER BY created_at, id").bind(org_id).fetch_all(c).await
}

pub async fn webhook_by_id(c: &mut SqliteConnection, id: &str) -> Result<Option<Webhook>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM webhooks WHERE id = ?").bind(id).fetch_optional(c).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::error::is_unique_violation;

    async fn seeded() -> Db {
        let db = Db::memory().await.unwrap();
        let mut c = db.pool().acquire().await.unwrap();
        insert_org(&mut c, "o1", "Acme", 1).await.unwrap();
        insert_user(
            &mut c,
            &NewUser {
                id: "u1",
                org_id: "o1",
                email: "A@X.io",
                password_hash: "h",
                role: "owner",
                verified: false,
                now: 5,
            },
        )
        .await
        .unwrap();
        drop(c);
        db
    }

    #[tokio::test]
    async fn emails_are_case_insensitive_unique() {
        let db = seeded().await;
        let mut c = db.pool().acquire().await.unwrap();
        let dup = insert_user(
            &mut c,
            &NewUser {
                id: "u2",
                org_id: "o1",
                email: "a@x.IO",
                password_hash: "h",
                role: "member",
                verified: false,
                now: 6,
            },
        )
        .await
        .unwrap_err();
        assert!(is_unique_violation(&dup));
        assert!(user_by_email(&mut c, "a@X.io").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn email_token_is_single_use_and_expires() {
        let db = seeded().await;
        let mut c = db.pool().acquire().await.unwrap();
        insert_email_token(&mut c, "h1", "u1", "verify", 100).await.unwrap();
        assert_eq!(consume_email_token(&mut c, "h1", "verify", 50).await.unwrap().as_deref(), Some("u1"));
        assert!(consume_email_token(&mut c, "h1", "verify", 51).await.unwrap().is_none(), "second use");
        insert_email_token(&mut c, "h2", "u1", "verify", 100).await.unwrap();
        assert!(consume_email_token(&mut c, "h2", "verify", 100).await.unwrap().is_none(), "expired at the boundary");
        assert!(consume_email_token(&mut c, "h2", "reset", 1).await.unwrap().is_none(), "wrong purpose");
    }

    #[tokio::test]
    async fn refresh_rotation_claim_is_exclusive() {
        let db = seeded().await;
        let mut c = db.pool().acquire().await.unwrap();
        insert_refresh(&mut c, "r1", "f1", "u1", "hash1", 1000).await.unwrap();
        assert!(mark_rotated(&mut c, "r1", 10).await.unwrap());
        assert!(!mark_rotated(&mut c, "r1", 11).await.unwrap(), "already rotated");
        insert_refresh(&mut c, "r2", "f1", "u1", "hash2", 1000).await.unwrap();
        assert_eq!(revoke_family(&mut c, "f1", 20).await.unwrap(), 2);
        assert!(!mark_rotated(&mut c, "r2", 21).await.unwrap(), "revoked family cannot rotate");
    }

    #[tokio::test]
    async fn revoked_api_keys_stop_resolving() {
        let db = seeded().await;
        let mut c = db.pool().acquire().await.unwrap();
        insert_api_key(&mut c, "k1", "o1", "u1", "ci", "kh", 1).await.unwrap();
        assert!(api_key_by_hash(&mut c, "kh").await.unwrap().is_some());
        assert!(!revoke_api_key(&mut c, "other-org", "k1", 2).await.unwrap(), "tenant scoped");
        assert!(revoke_api_key(&mut c, "o1", "k1", 2).await.unwrap());
        assert!(api_key_by_hash(&mut c, "kh").await.unwrap().is_none());
    }
}
