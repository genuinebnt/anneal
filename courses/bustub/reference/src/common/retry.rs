//! Retrying a transaction that lost a conflict.

use crate::common::db_error::{DbError, DbResult};

/// Is this error a serialization failure (code `40001`), which the client answers by running the transaction again?
pub fn is_retryable(e: &DbError) -> bool {
    // @begin 4e-c1
    e.code() == "40001"
    //~ todo!("4e-c1: serialization failures only, whatever their message says")
    // @end
}

/// Milliseconds to wait before attempt `attempt + 1`.
pub fn backoff_ms(attempt: usize, seed: u64) -> u64 {
    // @begin 4e-c1
    let base: u64 = 1u64 << attempt.saturating_sub(1).min(6);
    let base = base.min(64);
    // a cheap deterministic mix of (attempt, seed)
    let mut x = seed ^ (attempt as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 29;
    base / 2 + x % (base / 2 + 1)
    //~ todo!("4e-c1: min(2^(attempt-1), 64), then a jitter in [base/2, base] that depends only on (attempt, seed)")
    // @end
}

/// Runs `f(1)`, `f(2)`, ... until it succeeds, fails with something that is not retryable, or `max_attempts` calls were made.
pub fn run_with_retry<T>(max_attempts: usize, mut f: impl FnMut(usize) -> DbResult<T>) -> DbResult<(T, usize)> {
    // @begin 4e-c1
    let max = max_attempts.max(1);
    let mut attempt = 1;
    loop {
        match f(attempt) {
            Ok(v) => return Ok((v, attempt)),
            Err(e) if attempt < max && is_retryable(&e) => attempt += 1,
            Err(e) => return Err(e),
        }
    }
    //~ let _ = (max_attempts, &mut f);
    //~ todo!("4e-c1: call f with the attempt number; retry only retryable errors while attempts remain")
    // @end
}
