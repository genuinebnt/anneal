//! BLOCK 1: layered, typed, validated configuration with redacted secrets.
//!
//! Precedence (low to high): compiled defaults < TOML file < environment (`APP__SECTION__KEY=value`).
//! Mechanism: every layer is a `serde_json::Value` tree merged key by key, then deserialized ONCE into
//! the typed `Config` with `deny_unknown_fields`, so a typo in a file or env var is a startup error
//! instead of a silently ignored setting. Production alternatives: `figment`, `config`, `clap` env support.
//! Pitfall: env values are strings; we parse them as JSON scalars first (`8080` -> number, `true` -> bool)
//! and fall back to string, otherwise `APP__SERVER__BODY_LIMIT_BYTES=1024` would fail to deserialize.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A string that never prints itself. `Debug`/`Serialize` redact so a stray `{:?}` or a
/// `/debug/config` dump cannot leak it. Only `expose()` reveals it, which makes leaks greppable.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl Serialize for Secret {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str("***")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Dev,
    Prod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ServerConfig {
    pub bind: String,
    pub request_timeout_ms: u64,
    pub body_limit_bytes: usize,
    pub upload_limit_bytes: usize,
    pub cors_origins: Vec<String>,
    pub shutdown_grace_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct DbConfig {
    /// Path of the SQLite file. Empty = temporary database (dev/demo).
    pub path: String,
    pub max_connections: u32,
    pub busy_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct AuthConfig {
    pub token_secret: Secret,
    pub access_ttl_secs: i64,
    pub refresh_ttl_secs: i64,
    pub verify_ttl_secs: i64,
    /// Argon2id memory in KiB / iterations. OWASP baseline is 19 MiB, t=2; tests lower it for speed.
    pub argon2_m_kib: u32,
    pub argon2_t: u32,
    pub login_max_failures: u32,
    pub login_window_secs: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct RateLimitConfig {
    pub capacity: u32,
    pub refill_per_sec: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct JobsConfig {
    pub workers: usize,
    pub poll_ms: u64,
    pub lease_secs: i64,
    pub max_attempts: i64,
    pub backoff_base_secs: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct StorageConfig {
    /// Directory for the local-disk object store. Empty = in-memory store.
    pub dir: String,
    pub signing_secret: Secret,
    pub download_ttl_secs: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub environment: Environment,
    pub public_url: String,
    pub debug_routes: bool,
    pub server: ServerConfig,
    pub db: DbConfig,
    pub auth: AuthConfig,
    pub rate_limit: RateLimitConfig,
    pub jobs: JobsConfig,
    pub storage: StorageConfig,
}

const DEV_SECRET: &str = "dev-only-secret-change-me-dev-only-secret";

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8080".into(),
            request_timeout_ms: 10_000,
            body_limit_bytes: 64 * 1024,
            upload_limit_bytes: 5 * 1024 * 1024,
            cors_origins: vec![],
            shutdown_grace_ms: 10_000,
        }
    }
}
impl Default for DbConfig {
    fn default() -> Self {
        Self { path: String::new(), max_connections: 8, busy_timeout_ms: 5_000 }
    }
}
impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            token_secret: Secret::new(DEV_SECRET),
            access_ttl_secs: 900,
            refresh_ttl_secs: 14 * 24 * 3600,
            verify_ttl_secs: 24 * 3600,
            argon2_m_kib: 19 * 1024,
            argon2_t: 2,
            login_max_failures: 5,
            login_window_secs: 900,
        }
    }
}
impl Default for RateLimitConfig {
    fn default() -> Self {
        Self { capacity: 60, refill_per_sec: 10.0 }
    }
}
impl Default for JobsConfig {
    fn default() -> Self {
        Self { workers: 2, poll_ms: 200, lease_secs: 30, max_attempts: 5, backoff_base_secs: 2 }
    }
}
impl Default for StorageConfig {
    fn default() -> Self {
        Self { dir: String::new(), signing_secret: Secret::new(DEV_SECRET), download_ttl_secs: 300 }
    }
}
impl Default for Config {
    fn default() -> Self {
        Self {
            environment: Environment::Dev,
            public_url: "http://127.0.0.1:8080".into(),
            debug_routes: false,
            server: ServerConfig::default(),
            db: DbConfig::default(),
            auth: AuthConfig::default(),
            rate_limit: RateLimitConfig::default(),
            jobs: JobsConfig::default(),
            storage: StorageConfig::default(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    #[error("cannot read config file {0}: {1}")]
    File(String, String),
    #[error("invalid config: {0}")]
    Parse(String),
    #[error("config validation failed: {}", .0.join("; "))]
    Invalid(Vec<String>),
}

impl Config {
    /// Build the config from all three layers. `env` is injected so tests do not touch the process env.
    pub fn load(file: Option<&Path>, env: impl Iterator<Item = (String, String)>) -> Result<Self, ConfigError> {
        let mut tree = serde_json::to_value(Config::default()).map_err(|e| ConfigError::Parse(e.to_string()))?;
        // `Secret` serializes as "***": restore real defaults so they survive the merge.
        let defaults = Config::default();
        tree["auth"]["token_secret"] = Value::String(defaults.auth.token_secret.expose().into());
        tree["storage"]["signing_secret"] = Value::String(defaults.storage.signing_secret.expose().into());
        if let Some(path) = file {
            let text = std::fs::read_to_string(path)
                .map_err(|e| ConfigError::File(path.display().to_string(), e.to_string()))?;
            let layer: Value = toml::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string()))?;
            merge(&mut tree, layer);
        }
        apply_env(&mut tree, env);
        let cfg: Config = serde_json::from_value(tree).map_err(|e| ConfigError::Parse(e.to_string()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Cross-field rules that types cannot express. Collects ALL problems so the operator fixes them in one go.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let mut errs = vec![];
        if self.server.bind.parse::<std::net::SocketAddr>().is_err() {
            errs.push(format!("server.bind {:?} is not host:port", self.server.bind));
        }
        if self.server.body_limit_bytes == 0 {
            errs.push("server.body_limit_bytes must be > 0".into());
        }
        if self.jobs.workers == 0 {
            errs.push("jobs.workers must be >= 1".into());
        }
        if self.rate_limit.capacity == 0 || self.rate_limit.refill_per_sec <= 0.0 {
            errs.push("rate_limit needs capacity > 0 and refill_per_sec > 0".into());
        }
        if self.auth.token_secret.expose().len() < 32 {
            errs.push("auth.token_secret must be at least 32 bytes".into());
        }
        if self.storage.signing_secret.expose().len() < 32 {
            errs.push("storage.signing_secret must be at least 32 bytes".into());
        }
        if self.jobs.lease_secs < 1 || self.jobs.max_attempts < 1 || self.jobs.backoff_base_secs < 0 {
            errs.push("jobs needs lease_secs >= 1, max_attempts >= 1, backoff_base_secs >= 0".into());
        }
        if self.db.max_connections == 0 {
            errs.push("db.max_connections must be >= 1".into());
        }
        if self.server.cors_origins.iter().any(|o| o == "*") {
            errs.push("server.cors_origins must list explicit origins, not *".into());
        }
        if self.environment == Environment::Prod {
            if self.auth.token_secret.expose() == DEV_SECRET || self.storage.signing_secret.expose() == DEV_SECRET {
                errs.push("prod must not use the built-in development secrets".into());
            }
            if self.debug_routes {
                errs.push("debug_routes must be off in prod".into());
            }
            if self.db.path.is_empty() {
                errs.push("db.path must be set in prod (temporary DB would lose data)".into());
            }
        }
        if errs.is_empty() { Ok(()) } else { Err(ConfigError::Invalid(errs)) }
    }

    #[must_use]
    pub fn request_timeout(&self) -> Duration {
        Duration::from_millis(self.server.request_timeout_ms)
    }

    /// Redacted JSON view, safe to log at startup.
    #[must_use]
    pub fn redacted(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

/// Deep merge: objects merge key by key, everything else is replaced by the higher layer.
fn merge(base: &mut Value, over: Value) {
    match (base, over) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                merge(b.entry(k).or_insert(Value::Null), v);
            }
        }
        (b, o) => *b = o,
    }
}

/// `APP__DB__MAX_CONNECTIONS=4` sets `db.max_connections` to the number 4.
///
/// The type already in the tree decides how the string is read: a field that is a string stays a string
/// even if it looks like a number. Without this, an all-digit secret would become a number, fail to
/// deserialize, and the serde error would print the secret in the startup log.
fn apply_env(tree: &mut Value, env: impl Iterator<Item = (String, String)>) {
    for (k, v) in env {
        let Some(path) = k.strip_prefix("APP__") else { continue };
        let parts: Vec<String> = path.split("__").map(str::to_lowercase).collect();
        set_path(tree, &parts, &v);
    }
}

fn set_path(node: &mut Value, parts: &[String], raw: &str) {
    let Some((head, rest)) = parts.split_first() else {
        *node = if node.is_string() { Value::String(raw.to_string()) } else { parse_scalar(raw) };
        return;
    };
    // Unknown keys are inserted on purpose: `deny_unknown_fields` then rejects the typo.
    if let Value::Object(m) = node {
        set_path(m.entry(head.clone()).or_insert(Value::Null), rest, raw);
    }
}

/// Numbers, bools and JSON arrays are parsed; everything else stays a string.
fn parse_scalar(s: &str) -> Value {
    match serde_json::from_str::<Value>(s) {
        Ok(v @ (Value::Number(_) | Value::Bool(_) | Value::Array(_))) => v,
        _ => Value::String(s.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> impl Iterator<Item = (String, String)> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<Vec<_>>().into_iter()
    }

    #[test]
    fn defaults_are_valid() {
        let c = Config::load(None, env(&[])).unwrap();
        assert_eq!(c.server.body_limit_bytes, 64 * 1024);
        assert_eq!(c.environment, Environment::Dev);
    }

    #[test]
    fn precedence_default_then_file_then_env() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("app.toml");
        std::fs::write(&f, "[server]\nbody_limit_bytes = 1000\nrequest_timeout_ms = 77\n").unwrap();
        let c = Config::load(Some(&f), env(&[("APP__SERVER__BODY_LIMIT_BYTES", "2000")])).unwrap();
        assert_eq!(c.server.body_limit_bytes, 2000, "env beats file");
        assert_eq!(c.server.request_timeout_ms, 77, "file beats default");
        assert_eq!(c.jobs.workers, 2, "untouched default survives the merge");
    }

    #[test]
    fn unknown_keys_are_errors_not_silently_ignored() {
        let e = Config::load(None, env(&[("APP__SERVER__BODY_LIMIT", "1")])).unwrap_err();
        assert!(matches!(e, ConfigError::Parse(_)), "{e}");
    }

    #[test]
    fn validation_collects_every_problem() {
        let e = Config::load(
            None,
            env(&[("APP__ENVIRONMENT", "prod"), ("APP__JOBS__WORKERS", "0"), ("APP__SERVER__BIND", "nonsense")]),
        )
        .unwrap_err();
        let ConfigError::Invalid(list) = e else { panic!("wrong error") };
        assert!(list.len() >= 4, "{list:?}");
    }

    #[test]
    fn secrets_are_redacted_everywhere() {
        let c = Config::default();
        let dbg = format!("{c:?}");
        assert!(!dbg.contains(DEV_SECRET), "Debug leaked a secret");
        assert!(!c.redacted().to_string().contains(DEV_SECRET), "Serialize leaked a secret");
        assert_eq!(c.auth.token_secret.expose(), DEV_SECRET);
    }

    #[test]
    fn numeric_looking_env_secret_stays_a_string() {
        let digits = "1234567890".repeat(4);
        let c = Config::load(None, env(&[("APP__AUTH__TOKEN_SECRET", &digits)])).unwrap();
        assert_eq!(c.auth.token_secret.expose(), digits);
    }

    #[test]
    fn short_signing_secret_and_wildcard_cors_are_rejected() {
        let e = Config::load(
            None,
            env(&[("APP__STORAGE__SIGNING_SECRET", "short"), ("APP__SERVER__CORS_ORIGINS", "[\"*\"]")]),
        )
        .unwrap_err();
        let ConfigError::Invalid(list) = e else { panic!("wrong error: {e}") };
        assert_eq!(list.len(), 2, "{list:?}");
    }

    #[test]
    fn env_secret_is_loaded_but_not_printed() {
        let long = "z".repeat(40);
        let c = Config::load(None, env(&[("APP__AUTH__TOKEN_SECRET", &long)])).unwrap();
        assert_eq!(c.auth.token_secret.expose(), long);
        assert!(!format!("{c:?}").contains(&long));
    }
}
