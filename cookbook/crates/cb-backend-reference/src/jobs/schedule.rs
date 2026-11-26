//! Scheduled jobs: fixed intervals and a "cron-lite" (`minute hour * * *`, UTC).
//!
//! The scheduler never runs work itself: it ENQUEUES a job whose dedupe key names the time slot
//! (`sched:<name>:<slot>`). Any number of replicas can tick; the unique index lets one insert win.
//! Missed slots (process down) collapse into one catch-up run, never a burst.
//! Production alternatives: `tokio-cron-scheduler`, `cron` parser crate, Kubernetes CronJob.

use serde_json::json;

use super::queue::{self, NewJob};
use crate::db::Db;
use crate::ports::IdGen;

const MINUTE_MS: i64 = 60_000;
const DAY_MINUTES: i64 = 24 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CronLite {
    minutes: u64,
    hours: u64,
}

/// One field: `*`, `*/n`, `a`, or a comma list of those. Returns a bit per allowed value.
fn field(spec: &str, count: u32) -> Result<u64, String> {
    let mut bits = 0_u64;
    for part in spec.split(',') {
        if let Some(step) = part.strip_prefix("*/") {
            let step: u32 = step.parse().ok().filter(|s| *s > 0).ok_or_else(|| format!("bad step {part:?}"))?;
            (0..count).step_by(step as usize).for_each(|v| bits |= 1 << v);
        } else if part == "*" {
            (0..count).for_each(|v| bits |= 1 << v);
        } else {
            let v: u32 = part.parse().ok().filter(|v| *v < count).ok_or_else(|| format!("bad value {part:?}"))?;
            bits |= 1 << v;
        }
    }
    Ok(bits)
}

impl CronLite {
    pub fn parse(expr: &str) -> Result<Self, String> {
        let f: Vec<&str> = expr.split_whitespace().collect();
        let [min, hour, "*", "*", "*"] = f.as_slice() else {
            return Err("expected 'minute hour * * *' (day fields are not supported)".into());
        };
        Ok(Self { minutes: field(min, 60)?, hours: field(hour, 24)? })
    }

    #[must_use]
    pub fn matches(&self, epoch_minute: i64) -> bool {
        let of_day = epoch_minute.rem_euclid(DAY_MINUTES);
        self.minutes & (1 << (of_day % 60)) != 0 && self.hours & (1 << (of_day / 60)) != 0
    }

    /// Start (ms) of the most recent matching minute at or before `now_ms`.
    #[must_use]
    pub fn latest_slot(&self, now_ms: i64) -> Option<i64> {
        let now_min = now_ms.div_euclid(MINUTE_MS);
        // Any valid expression matches at least once per day.
        (0..DAY_MINUTES).map(|back| now_min - back).find(|m| self.matches(*m)).map(|m| m * MINUTE_MS)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    EverySecs(i64),
    Cron(CronLite),
}

impl When {
    #[must_use]
    pub fn latest_slot(&self, now_ms: i64) -> Option<i64> {
        match self {
            Self::EverySecs(s) => {
                let period = s.saturating_mul(1000).max(1);
                Some(now_ms.div_euclid(period) * period)
            }
            Self::Cron(c) => c.latest_slot(now_ms),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Schedule {
    pub name: &'static str,
    pub job_kind: &'static str,
    pub when: When,
}

/// Enqueue every schedule whose current slot has not run yet. Returns the names that fired.
pub async fn tick(
    db: &Db,
    ids: &dyn IdGen,
    schedules: &[Schedule],
    now: i64,
) -> Result<Vec<&'static str>, sqlx::Error> {
    let mut fired = vec![];
    for s in schedules {
        let Some(slot) = s.when.latest_slot(now) else { continue };
        let mut tx = db.begin().await?;
        let last: Option<i64> = sqlx::query_scalar("SELECT last_run FROM schedule_state WHERE name = ?")
            .bind(s.name)
            .fetch_optional(&mut *tx)
            .await?;
        if last.is_some_and(|l| l >= slot) {
            continue;
        }
        // First sight of a cron schedule: do not run for a time that passed before we ever existed.
        let stale_first_run = last.is_none() && matches!(s.when, When::Cron(_)) && now - slot >= MINUTE_MS;
        if !stale_first_run {
            let key = format!("sched:{}:{slot}", s.name);
            let payload = json!({});
            let job = NewJob {
                id: &ids.new_id(),
                kind: s.job_kind,
                payload: &payload,
                run_at: now,
                max_attempts: 3,
                dedupe_key: Some(&key),
            };
            if queue::enqueue(&mut tx, job).await? {
                fired.push(s.name);
            }
        }
        sqlx::query(
            "INSERT INTO schedule_state (name, last_run) VALUES (?, ?) \
             ON CONFLICT(name) DO UPDATE SET last_run = excluded.last_run",
        )
        .bind(s.name)
        .bind(slot)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
    }
    Ok(fired)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::SeqIds;

    const HOUR_MS: i64 = 60 * MINUTE_MS;

    #[test]
    fn cron_lite_parses_and_matches() {
        let c = CronLite::parse("*/15 3 * * *").unwrap();
        let hits: Vec<i64> = (0..DAY_MINUTES).filter(|m| c.matches(*m)).collect();
        assert_eq!(hits, [180, 195, 210, 225]);
        assert!(c.matches(DAY_MINUTES + 180), "repeats daily");
        let c = CronLite::parse("5,35 * * * *").unwrap();
        assert_eq!((0..DAY_MINUTES).filter(|m| c.matches(*m)).count(), 48);
        for bad in ["", "* *", "60 * * * *", "* 24 * * *", "*/0 * * * *", "a * * * *", "* * 1 * *"] {
            assert!(CronLite::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn latest_slot_looks_backwards() {
        let c = CronLite::parse("30 2 * * *").unwrap();
        let day = 24 * HOUR_MS;
        assert_eq!(c.latest_slot(day + 3 * HOUR_MS), Some(day + 2 * HOUR_MS + 30 * MINUTE_MS));
        assert_eq!(c.latest_slot(day + HOUR_MS), Some(2 * HOUR_MS + 30 * MINUTE_MS), "yesterday");
    }

    #[tokio::test]
    async fn each_slot_fires_once_even_with_two_schedulers_and_gaps_collapse() {
        let db = Db::memory().await.unwrap();
        let ids = SeqIds::default();
        let s = [Schedule { name: "purge", job_kind: "purge", when: When::EverySecs(60) }];
        assert_eq!(tick(&db, &ids, &s, 61_000).await.unwrap(), ["purge"]);
        assert!(tick(&db, &ids, &s, 62_000).await.unwrap().is_empty(), "same slot");
        assert_eq!(tick(&db, &ids, &s, 3_600_000).await.unwrap(), ["purge"], "one catch-up run");
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs").fetch_one(db.pool()).await.unwrap();
        assert_eq!(n, 2);
    }

    #[tokio::test]
    async fn new_cron_schedule_waits_for_its_next_time() {
        let db = Db::memory().await.unwrap();
        let ids = SeqIds::default();
        let when = When::Cron(CronLite::parse("0 3 * * *").unwrap());
        let s = [Schedule { name: "nightly", job_kind: "purge", when }];
        assert!(tick(&db, &ids, &s, 12 * HOUR_MS).await.unwrap().is_empty());
        assert_eq!(tick(&db, &ids, &s, 27 * HOUR_MS + 5).await.unwrap(), ["nightly"]);
    }
}
