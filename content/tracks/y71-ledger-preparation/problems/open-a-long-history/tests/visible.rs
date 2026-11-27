use solution::*;

/// An account whose state depends on the order of its entries: a negative amount takes a tenth as a fee first.
#[allow(dead_code)]
fn apply_v1(total: i64, e: &Entry) -> i64 {
    let total = if e.amount < 0 { total - total / 10 } else { total };
    total.wrapping_add(e.amount)
}

#[allow(dead_code)]
fn apply_v2(total: i64, e: &Entry) -> i64 {
    total.wrapping_add(e.amount.wrapping_mul(2))
}

#[allow(dead_code)]
fn log_of(items: &[(u64, i64)]) -> Vec<Entry> {
    items.iter().map(|&(id, amount)| Entry { id, amount }).collect()
}

/// The state, and how many times `apply` was called.
#[allow(dead_code)]
fn open(log: &[Entry], rules: u32, bookmark: Option<&Bookmark<i64>>) -> (i64, usize) {
    let mut calls = 0;
    let state = load(log, rules, bookmark, |s, e| {
        calls += 1;
        if rules == 1 { apply_v1(s, e) } else { apply_v2(s, e) }
    });
    (state, calls)
}

/// What reading everything gives.
#[allow(dead_code)]
fn full(log: &[Entry], rules: u32) -> i64 {
    log.iter().fold(0, if rules == 1 { apply_v1 } else { apply_v2 })
}

#[test]
fn without_a_bookmark_everything_is_read() {
    let log = log_of(&[(1, 100), (2, -30), (3, 5)]);
    check!("three entries", open(&log, 1, None), (full(&log, 1), 3));
}

#[test]
fn with_a_good_one_only_the_rest_is_read() {
    let log = log_of(&[(1, 100), (2, -30), (3, 5), (4, 5)]);
    let b = mark(&log[..3], 1, &full(&log[..3], 1));
    check!("bookmark at 3", open(&log, 1, Some(&b)), (full(&log, 1), 1));
    let whole = mark(&log, 1, &full(&log, 1));
    check!("nothing after it", open(&log, 1, Some(&whole)), (full(&log, 1), 0));
}

#[test]
fn new_rules_ignore_old_bookmarks() {
    let log = log_of(&[(1, 100), (2, 7)]);
    let b = mark(&log, 1, &full(&log, 1));
    check!("rules 2", open(&log, 2, Some(&b)), (214, 2));
}

#[test]
fn a_bookmark_beyond_the_end_of_the_log() {
    let log = log_of(&[(1, 100), (2, 7), (3, 9)]);
    let b = mark(&log, 1, &full(&log, 1));
    check!("the last entry was lost", open(&log[..2], 1, Some(&b)), (107, 2));
}

#[test]
fn a_log_that_regrew() {
    let old = log_of(&[(1, 100), (2, 7)]);
    let b = mark(&old, 1, &full(&old, 1));
    let new = log_of(&[(1, 100), (9, 50), (10, 1)]);
    check!("entry 2 is gone", open(&new, 1, Some(&b)), (151, 3));
}

#[test]
fn when_a_new_one_is_due() {
    check!("2 to 3 by 4", due(2, 3, 4), false);
    check!("3 to 4 by 4", due(3, 4, 4), true);
    check!("3 to 9 by 4", due(3, 9, 4), true);
    check!("4 to 5 by 4", due(4, 5, 4), false);
    check!("never", due(0, 100, 0), false);
}
