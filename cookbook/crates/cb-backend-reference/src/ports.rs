//! BLOCK 2: the "ports" of the app. Anything non-deterministic or external (time, randomness, email,
//! blob storage) is reached through a small trait so tests can substitute a fake.
//!
//! WHY traits instead of generics everywhere: `Arc<dyn Trait>` keeps `AppState` a single concrete type
//! (no generic parameter infecting every handler signature). The vtable call is irrelevant next to a DB hit.
//! Repositories are intentionally NOT traits here: the faithful fake for SQL is in-memory SQLite
//! (see `db::Db::memory`), because a hand-written fake repository cannot reproduce constraint,
//! collation or transaction behaviour, which is where repository bugs actually live.

use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

use async_trait::async_trait;

/// Source of "now". Never call `SystemTime::now()` in business code: tests could not control it.
pub trait Clock: Send + Sync + 'static {
    fn now_ms(&self) -> i64;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        chrono::Utc::now().timestamp_millis()
    }
}

/// A virtual clock: time only moves when the test says so (rate-limit refill, token expiry, leases).
#[derive(Debug)]
pub struct ManualClock(AtomicI64);

impl ManualClock {
    #[must_use]
    pub fn new(start_ms: i64) -> Self {
        Self(AtomicI64::new(start_ms))
    }
    pub fn advance_ms(&self, ms: i64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
    pub fn advance_secs(&self, s: i64) {
        self.advance_ms(s * 1000);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// Id and secret-token generation.
pub trait IdGen: Send + Sync + 'static {
    /// Time-ordered unique id (UUIDv7 in production: index-friendly, roughly sortable).
    fn new_id(&self) -> String;
    /// Unguessable secret (>= 244 bits). Used for refresh/verification tokens and API keys.
    fn secret(&self) -> String;
}

/// Production generator. `Uuid::new_v4` draws from the OS CSPRNG, two of them give 244 random bits.
#[derive(Debug, Default, Clone, Copy)]
pub struct UuidIds;

impl IdGen for UuidIds {
    fn new_id(&self) -> String {
        uuid::Uuid::now_v7().to_string()
    }
    fn secret(&self) -> String {
        format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
    }
}

/// Deterministic generator for tests: `id-000001`, `secret-000002`...
#[derive(Debug, Default)]
pub struct SeqIds(AtomicU64);

impl IdGen for SeqIds {
    fn new_id(&self) -> String {
        format!("id-{:06}", self.0.fetch_add(1, Ordering::SeqCst))
    }
    fn secret(&self) -> String {
        format!("secret-{:06}-{}", self.0.fetch_add(1, Ordering::SeqCst), "x".repeat(32))
    }
}

/// An email ready to send.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Email {
    pub to: String,
    pub subject: String,
    pub body: String,
}

#[derive(Debug, thiserror::Error)]
#[error("mailer unavailable: {0}")]
pub struct MailError(pub String);

#[async_trait]
pub trait Mailer: Send + Sync + 'static {
    async fn send(&self, email: &Email) -> Result<(), MailError>;
}

/// Fake mailer: records mail, and can be told to fail the next N sends (fault injection).
#[derive(Debug, Default)]
pub struct MemoryMailer {
    sent: Mutex<Vec<Email>>,
    fail_next: AtomicU64,
}

impl MemoryMailer {
    pub fn fail_next(&self, n: u64) {
        self.fail_next.store(n, Ordering::SeqCst);
    }
    #[must_use]
    pub fn sent(&self) -> Vec<Email> {
        self.sent.lock().unwrap().clone()
    }
    /// Pull the last token (`token=<...>`) out of the latest mail to `to`, like a user clicking a link.
    #[must_use]
    pub fn last_token_for(&self, to: &str) -> Option<String> {
        let sent = self.sent.lock().unwrap();
        let mail = sent.iter().rev().find(|m| m.to == to)?;
        let (_, rest) = mail.body.split_once("token=")?;
        Some(rest.split_whitespace().next()?.to_string())
    }
}

#[async_trait]
impl Mailer for MemoryMailer {
    async fn send(&self, email: &Email) -> Result<(), MailError> {
        let failing = self.fail_next.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1)).is_ok();
        if failing {
            return Err(MailError("injected failure".into()));
        }
        self.sent.lock().unwrap().push(email.clone());
        Ok(())
    }
}

/// Production stand-in that just logs. A real deployment would implement `Mailer` over SMTP/SES.
#[derive(Debug, Default)]
pub struct LogMailer;

#[async_trait]
impl Mailer for LogMailer {
    async fn send(&self, email: &Email) -> Result<(), MailError> {
        tracing::info!(to = %email.to, subject = %email.subject, "mail (log only)");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_clock_only_moves_when_told() {
        let c = ManualClock::new(1_000);
        assert_eq!(c.now_ms(), 1_000);
        c.advance_secs(2);
        assert_eq!(c.now_ms(), 3_000);
    }

    #[test]
    fn seq_ids_are_unique_and_ordered() {
        let g = SeqIds::default();
        let (a, b) = (g.new_id(), g.new_id());
        assert!(a < b);
        assert_ne!(g.secret(), g.secret());
    }

    #[test]
    fn uuid_v7_ids_sort_by_time() {
        let g = UuidIds;
        let a = g.new_id();
        std::thread::sleep(std::time::Duration::from_millis(2));
        assert!(a < g.new_id());
        assert!(g.secret().len() >= 64);
    }

    #[tokio::test]
    async fn mailer_fails_exactly_n_times() {
        let m = MemoryMailer::default();
        m.fail_next(2);
        let e = Email { to: "a@x.io".into(), subject: "s".into(), body: "go token=abc now".into() };
        assert!(m.send(&e).await.is_err());
        assert!(m.send(&e).await.is_err());
        assert!(m.send(&e).await.is_ok());
        assert_eq!(m.last_token_for("a@x.io").as_deref(), Some("abc"));
    }
}
