//! BLOCK 2 (part): application state and dependency injection.
//!
//! Everything non-deterministic or external arrives through `Deps` as a trait object; production wires
//! the real ones (`Deps::production`), tests wire fakes. Handlers only ever see `AppState`.

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::auth::password::Hasher;
use crate::auth::throttle::LoginThrottle;
use crate::auth::token::TokenSigner;
use crate::cache::TagCache;
use crate::config::Config;
use crate::db::Db;
use crate::jobs::worker::JobContext;
use crate::jobs::{relay, webhook};
use crate::metrics::Metrics;
use crate::objstore::{LocalDiskStore, MemoryStore, ObjectStore, Presigner};
use crate::ports::{Clock, IdGen, LogMailer, Mailer, SystemClock, UuidIds};
use crate::ratelimit::RateLimiter;

const STATS_TTL_MS: i64 = 30_000;
const STATS_CAPACITY: usize = 1_000;
const WEBHOOK_TIMEOUT: Duration = Duration::from_secs(10);

pub struct Deps {
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdGen>,
    pub mailer: Arc<dyn Mailer>,
    pub store: Arc<dyn ObjectStore>,
}

impl Deps {
    /// Real clock and ids. The mailer only logs: plug an SMTP/SES implementation in here.
    #[must_use]
    pub fn production(cfg: &Config) -> Self {
        let store: Arc<dyn ObjectStore> = if cfg.storage.dir.is_empty() {
            Arc::new(MemoryStore::default())
        } else {
            Arc::new(LocalDiskStore::new(&cfg.storage.dir))
        };
        Self { clock: Arc::new(SystemClock), ids: Arc::new(UuidIds), mailer: Arc::new(LogMailer), store }
    }
}

pub struct App {
    pub cfg: Config,
    pub db: Db,
    pub deps: Deps,
    pub hasher: Hasher,
    pub signer: TokenSigner,
    pub presigner: Presigner,
    pub throttle: LoginThrottle,
    pub limiter: RateLimiter,
    pub stats_cache: TagCache<Value>,
    pub metrics: Metrics,
    /// "New events were published" wake-up for SSE streams. Carries no data: streams read the database.
    pub events: broadcast::Sender<()>,
    /// Cancelled once, when the process starts shutting down.
    pub shutdown: CancellationToken,
    http: reqwest::Client,
}

pub type AppState = Arc<App>;

impl App {
    pub fn build(cfg: Config, db: Db, deps: Deps) -> anyhow::Result<AppState> {
        Ok(Arc::new(Self {
            hasher: Hasher::new(&cfg.auth)?,
            signer: TokenSigner::new(cfg.auth.token_secret.expose()),
            presigner: Presigner::new(cfg.storage.signing_secret.expose()),
            throttle: LoginThrottle::new(cfg.auth.login_max_failures, cfg.auth.login_window_secs),
            limiter: RateLimiter::new(cfg.rate_limit.capacity, cfg.rate_limit.refill_per_sec),
            stats_cache: TagCache::new(STATS_TTL_MS, STATS_CAPACITY),
            metrics: Metrics::default(),
            events: broadcast::channel(16).0,
            shutdown: CancellationToken::new(),
            http: webhook::client(WEBHOOK_TIMEOUT)?,
            cfg,
            db,
            deps,
        }))
    }

    #[must_use]
    pub fn now_ms(&self) -> i64 {
        self.deps.clock.now_ms()
    }

    #[must_use]
    pub fn new_id(&self) -> String {
        self.deps.ids.new_id()
    }

    #[must_use]
    pub fn job_context(&self) -> JobContext {
        JobContext {
            db: self.db.clone(),
            clock: self.deps.clock.clone(),
            mailer: self.deps.mailer.clone(),
            http: self.http.clone(),
            cfg: self.cfg.jobs.clone(),
        }
    }

    /// One outbox relay step, waking SSE streams if anything was published.
    pub async fn relay_events(&self) -> Result<usize, sqlx::Error> {
        let n =
            relay::relay_once(&self.db, self.deps.ids.as_ref(), self.now_ms(), self.cfg.jobs.max_attempts).await?;
        if n > 0 {
            // An error only means nobody is listening.
            let _ = self.events.send(());
        }
        Ok(n)
    }
}
