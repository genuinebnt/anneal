use solution::*;

#[test]
fn snapshot_does_not_reset() {
    check!(r#"record 3, snapshot twice"#, { let s = Stats::new(); s.record(3); (s.snapshot(), s.snapshot()) }, (Some(Snapshot { count: 1, sum: 3, min: 3, max: 3 }), Some(Snapshot { count: 1, sum: 3, min: 3, max: 3 })));
}

#[test]
fn sum_saturates() {
    check!(r#"record u64::MAX, 5"#, { let s = Stats::new(); s.record(u64::MAX); s.record(5); s.snapshot() }, Some(Snapshot { count: 2, sum: u64::MAX, min: 5, max: u64::MAX }));
}

#[test]
fn sum_saturates_in_steps() {
    check!(r#"record MAX-1, 1, 1"#, { let s = Stats::new(); s.record(u64::MAX - 1); s.record(1); s.record(1); s.snapshot().map(|x| x.sum) }, Some(u64::MAX));
}

#[test]
fn min_can_be_max_value() {
    check!(r#"record u64::MAX only"#, { let s = Stats::new(); s.record(u64::MAX); s.snapshot() }, Some(Snapshot { count: 1, sum: u64::MAX, min: u64::MAX, max: u64::MAX }));
}

#[test]
fn reusable_after_take() {
    check!(r#"record 9, take, record 2"#, { let s = Stats::new(); s.record(9); s.take(); s.record(2); s.snapshot() }, Some(Snapshot { count: 1, sum: 2, min: 2, max: 2 }));
}

#[test]
fn eight_threads_lose_nothing() {
    let s = std::sync::Arc::new(Stats::new());
    let mut hs = Vec::new();
    for t in 0..8u64 {
        let s = s.clone();
        hs.push(std::thread::spawn(move || {
            for i in 0..50_000u64 {
                s.record(t * 1_000_000 + i + 1);
            }
        }));
    }
    for h in hs {
        h.join().unwrap();
    }
    let want_sum: u64 = (0..8u64).map(|t| (1..=50_000u64).map(|i| t * 1_000_000 + i).sum::<u64>()).sum();
    check!("8 threads x 50000 records", s.snapshot(), Some(Snapshot { count: 400_000, sum: want_sum, min: 1, max: 7_050_000 }));
}

#[test]
fn racing_saturation_never_wraps() {
    let s = std::sync::Arc::new(Stats::new());
    let mut hs = Vec::new();
    for _ in 0..4 {
        let s = s.clone();
        hs.push(std::thread::spawn(move || {
            for _ in 0..20_000 {
                s.record(u64::MAX / 3);
            }
        }));
    }
    for h in hs {
        h.join().unwrap();
    }
    let snap = s.snapshot().unwrap();
    check!("4 threads x 20000 records of u64::MAX/3", (snap.count, snap.sum), (80_000, u64::MAX));
}

#[test]
fn extremes_found_under_contention() {
    let s = std::sync::Arc::new(Stats::new());
    let hs: Vec<_> = (0..6u64)
        .map(|t| {
            let s = s.clone();
            std::thread::spawn(move || {
                for i in 0..20_000u64 {
                    // thread 3 alone records the smallest value, thread 5 alone the largest, late in its run
                    let v = if t == 3 && i == 10_000 { 1 } else if t == 5 && i == 19_999 { 999_999 } else { 1_000 + (i % 100) };
                    s.record(v);
                }
            })
        })
        .collect();
    for h in hs {
        h.join().unwrap();
    }
    let snap = s.snapshot().unwrap();
    check!("6 threads, one tiny and one huge value hidden among 120000", (snap.count, snap.min, snap.max), (120_000, 1, 999_999));
}

#[test]
fn min_and_max_survive_racing_updates() {
    // every thread lowers the minimum and raises the maximum again and again: a check-then-store update
    // lets a stale write overwrite a better value
    for round in 0..5u64 {
        let s = std::sync::Arc::new(Stats::new());
        let hs: Vec<_> = (0..8u64)
            .map(|t| {
                let s = s.clone();
                std::thread::spawn(move || {
                    for i in 0..20_000u64 {
                        s.record(100_000 - i * 5 + t);
                        s.record(i * 5 + t + 10);
                    }
                })
            })
            .collect();
        for h in hs {
            h.join().unwrap();
        }
        let snap = s.snapshot().unwrap();
        check!(format!("round {round}: 8 threads recording descending and ascending values"), (snap.min, snap.max), (5, 100_012));
    }
}

#[test]
fn random_vs_model() {
    let mut rng = anneal_prelude::Rng::new(4101);
    for _ in 0..300 {
        let s = Stats::new();
        let n = rng.below(12);
        let mut vals = Vec::new();
        for _ in 0..n {
            let v = if rng.below(6) == 0 { u64::MAX - rng.below(3) as u64 } else { rng.below(1000) as u64 };
            vals.push(v);
            s.record(v);
        }
        let want = if vals.is_empty() {
            None
        } else {
            Some(Snapshot {
                count: vals.len() as u64,
                sum: vals.iter().fold(0u64, |a, &b| a.saturating_add(b)),
                min: *vals.iter().min().unwrap(),
                max: *vals.iter().max().unwrap(),
            })
        };
        check!(format!("record {vals:?}"), s.snapshot(), want);
    }
}
