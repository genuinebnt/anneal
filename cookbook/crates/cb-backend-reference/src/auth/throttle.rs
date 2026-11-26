//! Login throttling: at most N attempts per account per window.
//!
//! WHY the attempt is counted BEFORE the password check: "check the counter, verify, then record the
//! failure" lets 1000 parallel guesses all pass the check while the first hash is still running.
//! `try_acquire` checks and reserves under one lock. A successful login clears the counter.
//! Pitfalls: keying on the email lets an attacker lock a victim out (real systems combine account, IP and
//! device signals, and add CAPTCHA or delays instead of hard lockout); this map is per process, so N
//! replicas allow N times the budget (use Redis or the database when that matters).

use std::collections::HashMap;
use std::sync::Mutex;

/// Above this many tracked accounts, expired windows are swept so random emails cannot grow memory forever.
const SWEEP_AT: usize = 10_000;

#[derive(Debug, Clone, Copy)]
struct Window {
    started_ms: i64,
    attempts: u32,
}

#[derive(Debug)]
pub struct LoginThrottle {
    max: u32,
    window_ms: i64,
    seen: Mutex<HashMap<String, Window>>,
}

impl LoginThrottle {
    #[must_use]
    pub fn new(max: u32, window_secs: i64) -> Self {
        Self { max, window_ms: window_secs.saturating_mul(1000), seen: Mutex::default() }
    }

    /// `Err(retry_after_secs)` when the account is out of attempts.
    pub fn try_acquire(&self, account: &str, now_ms: i64) -> Result<(), u64> {
        let mut seen = self.seen.lock().unwrap();
        if seen.len() >= SWEEP_AT {
            seen.retain(|_, w| now_ms < w.started_ms + self.window_ms);
        }
        let w = seen.entry(account.to_lowercase()).or_insert(Window { started_ms: now_ms, attempts: 0 });
        if now_ms >= w.started_ms + self.window_ms {
            *w = Window { started_ms: now_ms, attempts: 0 };
        }
        if w.attempts >= self.max {
            let wait_ms = (w.started_ms + self.window_ms - now_ms).max(0) as u64;
            return Err(wait_ms.div_ceil(1000).max(1));
        }
        w.attempts += 1;
        Ok(())
    }

    pub fn reset(&self, account: &str) {
        self.seen.lock().unwrap().remove(&account.to_lowercase());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_after_max_then_recovers_after_window() {
        let t = LoginThrottle::new(3, 60);
        for _ in 0..3 {
            assert!(t.try_acquire("a@x.io", 0).is_ok());
        }
        assert_eq!(t.try_acquire("A@X.IO", 1_000), Err(59), "case variants share a budget");
        assert!(t.try_acquire("b@x.io", 1_000).is_ok(), "other accounts unaffected");
        assert!(t.try_acquire("a@x.io", 60_000).is_ok());
    }

    #[test]
    fn success_resets_the_budget() {
        let t = LoginThrottle::new(1, 60);
        assert!(t.try_acquire("a", 0).is_ok());
        t.reset("a");
        assert!(t.try_acquire("a", 0).is_ok());
    }

    #[test]
    fn parallel_attempts_cannot_exceed_the_budget() {
        let t = LoginThrottle::new(5, 60);
        let allowed: usize = std::thread::scope(|s| {
            let hs: Vec<_> = (0..64).map(|_| s.spawn(|| usize::from(t.try_acquire("a", 0).is_ok()))).collect();
            hs.into_iter().map(|h| h.join().unwrap()).sum()
        });
        assert_eq!(allowed, 5);
    }
}
