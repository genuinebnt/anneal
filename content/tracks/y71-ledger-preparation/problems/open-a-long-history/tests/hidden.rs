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
fn a_bookmark_of_nothing() {
    let b = mark(&[], 1, &0);
    check!("on an empty log", open(&[], 1, Some(&b)), (0, 0));
    let log = log_of(&[(1, 5)]);
    check!("later", open(&log, 1, Some(&b)), (5, 1));
}

#[test]
fn a_bookmark_on_a_log_that_lost_everything() {
    let log = log_of(&[(1, 5)]);
    let b = mark(&log, 1, &5);
    check!("empty now", open(&[], 1, Some(&b)), (0, 0));
}

#[test]
fn same_amounts_with_new_ids_are_new_entries() {
    let old = log_of(&[(1, 10), (2, 20)]);
    let b = mark(&old, 1, &999);
    let new = log_of(&[(1, 10), (3, 20)]);
    check!("id 2 became id 3", open(&new, 1, Some(&b)), (30, 2));
}

#[test]
fn the_stored_state_is_what_a_good_bookmark_starts_from() {
    // the state handed to `mark` is trusted: with a good bookmark nothing before it is read again
    let log = log_of(&[(1, 10), (2, 20)]);
    let b = mark(&log[..1], 1, &1000);
    check!("from 1000", open(&log, 1, Some(&b)), (1020, 1));
}

#[test]
fn it_works_for_states_that_are_not_numbers() {
    let log = log_of(&[(5, 1), (6, 2)]);
    let b = mark(&log[..1], 3, &vec![5u64]);
    let got = load(&log, 3, Some(&b), |mut seen: Vec<u64>, e: &Entry| {
        seen.push(e.id);
        seen
    });
    check!("ids", got, vec![5, 6]);
    // a bookmark can be used more than once
    check!("again", load(&log[..1], 3, Some(&b), |s: Vec<u64>, _: &Entry| s), vec![5]);
}

#[test]
fn due_edges() {
    check!("every entry", due(7, 8, 1), true);
    check!("no growth", due(8, 8, 4), false);
    check!("nothing yet", due(0, 0, 1), false);
    check!("huge", due(0, usize::MAX, usize::MAX), true);
    check!("just short", due(0, usize::MAX - 1, usize::MAX), false);
    check!("never, huge", due(0, usize::MAX, 0), false);
    for before in 0..30 {
        for after in before..30 {
            for every in 1..9 {
                let want = (before + 1..=after).any(|n| n % every == 0);
                check!(format!("due({before}, {after}, {every})"), due(before, after, every), want);
            }
        }
    }
}

#[test]
fn random_lives_of_an_account() {
    let mut rng = anneal_prelude::Rng::new(7107);
    for _ in 0..300 {
        let mut log: Vec<Entry> = Vec::new();
        let mut next_id = 0;
        let mut rules = 1;
        // bookmarks of any age, with the entries each was made from
        let mut saved: Vec<(Bookmark<i64>, Vec<Entry>, u32)> = Vec::new();
        let mut trace = Vec::new();
        for _ in 0..40 {
            match rng.below(10) {
                0..=3 => {
                    for _ in 0..=rng.below(4) {
                        next_id += 1;
                        log.push(Entry { id: next_id, amount: rng.int(-50, 100) });
                    }
                    trace.push(format!("grow to {}", log.len()));
                }
                4 => {
                    let keep = rng.below(log.len() + 1);
                    log.truncate(keep);
                    trace.push(format!("crash, keep {keep}"));
                }
                5 => {
                    rules = 3 - rules;
                    trace.push(format!("rules {rules}"));
                }
                6 | 7 => {
                    saved.push((mark(&log, rules, &full(&log, rules)), log.clone(), rules));
                    trace.push(format!("mark #{} at {}", saved.len() - 1, log.len()));
                }
                _ if !saved.is_empty() => {
                    let i = rng.below(saved.len());
                    let (b, made_from, made_rules) = &saved[i];
                    let good = *made_rules == rules && log.starts_with(made_from);
                    let reads = if good { log.len() - made_from.len() } else { log.len() };
                    trace.push(format!("open with #{i}"));
                    check!(trace.join("; "), open(&log, rules, Some(b)), (full(&log, rules), reads));
                }
                _ => {}
            }
        }
    }
}

#[test]
fn a_large_history_reads_only_the_tail() {
    let log: Vec<Entry> = (0..100_000).map(|i| Entry { id: i, amount: i as i64 % 7 - 3 }).collect();
    let b = mark(&log[..99_990], 1, &full(&log[..99_990], 1));
    check!("ten left", open(&log, 1, Some(&b)), (full(&log, 1), 10));
}
