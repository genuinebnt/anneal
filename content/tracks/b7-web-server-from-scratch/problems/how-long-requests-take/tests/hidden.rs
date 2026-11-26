use solution::*;

#[allow(dead_code, unused_imports)]
fn filled(bounds: &[u64], values: &[u64]) -> Timings {
    let t = Timings::new(bounds);
    for &v in values {
        t.record(v);
    }
    t
}

#[allow(dead_code, unused_imports)]
fn exact(sorted: &[u64], q: f64) -> u64 {
    sorted[((q * sorted.len() as f64).ceil() as usize).max(1) - 1]
}

#[allow(dead_code, unused_imports)]
fn check_quantile(t: &Timings, sorted: &[u64], q: f64) {
    let want = exact(sorted, q);
    let got = t.quantile(q);
    let top = want.saturating_add(want / 8).min(sorted[sorted.len() - 1]);
    check!(format!("quantile({q}) of {} values: exact {want}, got {got:?}", sorted.len()), got.is_some_and(|g| g >= want && g <= top), true);
}

#[test]
fn memory_stays_small() {
    let (t, used) = anneal_prelude::allocs(|| {
        let t = Timings::new(&[1, 2, 3]);
        for v in 0..200_000u64 {
            t.record(v);
        }
        t
    });
    check!(format!("{} bytes allocated in all", used.bytes), used.bytes < 64 * 1024, true);
    check!("count", t.count(), 200_000);
}

#[test]
fn random_values_at_every_size() {
    let mut rng = anneal_prelude::Rng::new(71501);
    for _ in 0..200 {
        let n = 1 + rng.below(50);
        let shift = rng.below(60) as u32;
        let mut values: Vec<u64> = (0..n).map(|_| rng.next_u64() >> (shift + rng.below(4) as u32)).collect();
        let t = filled(&[], &values);
        values.sort_unstable();
        for q in [0.0, 0.1, 0.5, 0.9, 0.99, 1.0] {
            check_quantile(&t, &values, q);
        }
    }
}

#[test]
fn values_just_around_powers_of_two() {
    for p in [3u32, 4, 5, 10, 20, 39, 40, 41, 62, 63] {
        for v in [(1u64 << p) - 1, 1 << p, (1 << p) + 1] {
            // a larger second value, so that the answer is not simply the maximum
            let values = [v, u64::MAX];
            let t = filled(&[], &values);
            check_quantile(&t, &values, 0.5);
        }
    }
}

#[test]
fn the_extremes() {
    let t = filled(&[0, u64::MAX], &[0, u64::MAX, u64::MAX]);
    check!("0 and twice MAX", (t.quantile(0.0), t.quantile(1.0), t.sum(), t.max()), (Some(0), Some(u64::MAX), u64::MAX, u64::MAX));
    check!("report", t.render("x"), format!("x_bucket{{le=\"0\"}} 1\nx_bucket{{le=\"{m}\"}} 3\nx_bucket{{le=\"+Inf\"}} 3\nx_sum {m}\nx_count 3\n", m = u64::MAX));
}

#[test]
fn bounds_count_exactly_including_the_bound_itself() {
    let t = filled(&[100, 101, 1000, 1001], &[99, 100, 101, 102, 999, 1000, 1001, 1002]);
    let counts: Vec<String> = t.render("n").lines().take(4).map(|l| l.rsplit(' ').next().unwrap().to_string()).collect();
    check!("le 100 101 1000 1001", counts, ["2", "3", "6", "7"].map(String::from).to_vec());
}

#[test]
fn no_bounds() {
    check!("render", filled(&[], &[7]).render("z"), "z_bucket{le=\"+Inf\"} 1\nz_sum 7\nz_count 1\n".to_string());
}

#[test]
fn an_empty_report() {
    check!("render", Timings::new(&[5]).render("e"), "e_bucket{le=\"5\"} 0\ne_bucket{le=\"+Inf\"} 0\ne_sum 0\ne_count 0\n".to_string());
}

#[test]
fn bounds_must_increase() {
    for bounds in [vec![5u64, 5], vec![5, 4], vec![1, 3, 2]] {
        let got = std::panic::catch_unwind(|| drop(Timings::new(&bounds)));
        check!(format!("{bounds:?}"), got.is_err(), true);
    }
}

#[test]
fn random_reports_against_a_model() {
    let mut rng = anneal_prelude::Rng::new(71502);
    for _ in 0..200 {
        let mut bounds: Vec<u64> = Vec::new();
        let mut at = 0;
        for _ in 0..rng.below(6) {
            at += 1 + rng.below(200) as u64;
            bounds.push(at);
        }
        let n = rng.below(40);
        let values: Vec<u64> = rng.vec(n, 0, 1000);
        let t = filled(&bounds, &values);
        let mut want = String::new();
        for b in &bounds {
            want.push_str(&format!("m_bucket{{le=\"{b}\"}} {}\n", values.iter().filter(|v| **v <= *b).count()));
        }
        want.push_str(&format!("m_bucket{{le=\"+Inf\"}} {n}\nm_sum {}\nm_count {n}\n", values.iter().sum::<u64>()));
        check!(format!("bounds {bounds:?} values {values:?}"), t.render("m"), want);
    }
}

#[test]
fn many_threads_lose_nothing() {
    for round in 0..4 {
        let t = Timings::new(&[10]);
        std::thread::scope(|s| {
            for k in 0..4u64 {
                let t = &t;
                s.spawn(move || {
                    for i in 0..30_000u64 {
                        t.record((i * 7 + k) % 1000);
                    }
                    t.record(u64::MAX / 3);
                });
            }
        });
        check!(format!("round {round}: count, max, saturated sum"), (t.count(), t.max(), t.sum()), (120_004, u64::MAX / 3, u64::MAX));
    }
}
