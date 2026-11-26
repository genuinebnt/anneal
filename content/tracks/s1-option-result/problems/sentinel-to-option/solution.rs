use std::time::Duration;

/// Legacy: the index of `x` in `v`, or -1. Leave it as it is.
fn legacy_find(v: &[i32], x: i32) -> i32 {
    v.iter().position(|&y| y == x).map_or(-1, |i| i as i32)
}

/// Legacy: the byte offset of the last `c` in `s`, or `usize::MAX` (C++'s `npos`). Leave it as it is.
fn legacy_rfind(s: &str, c: char) -> usize {
    s.rfind(c).unwrap_or(usize::MAX)
}

/// The index of `x` in `v`, or `None`.
pub fn find(v: &[i32], x: i32) -> Option<usize> {
    usize::try_from(legacy_find(v, x)).ok()
}

/// The byte offset of the last `c` in `s`, or `None`.
pub fn rfind(s: &str, c: char) -> Option<usize> {
    Some(legacy_rfind(s, c)).filter(|&i| i != usize::MAX)
}

/// The `ms` argument for the legacy `wait(ms)`: -1 waits forever, 0 polls, n > 0 waits up to n ms.
pub fn timeout_ms(t: Option<Duration>) -> i64 {
    t.map_or(-1, |d| {
        i64::try_from(d.as_nanos().div_ceil(1_000_000)).unwrap_or(i64::MAX)
    })
}
