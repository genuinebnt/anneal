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
fn empty() {
    let t = Timings::new(&[10]);
    check!("nothing recorded", (t.count(), t.sum(), t.max(), t.quantile(0.5)), (0, 0, 0, None));
}

#[test]
fn small_values_are_exact() {
    let t = filled(&[], &[3, 1, 4, 1, 5]);
    check!("3 1 4 1 5", (t.count(), t.sum(), t.max(), t.quantile(0.5), t.quantile(1.0)), (5, 14, 5, Some(3), Some(5)));
}

#[test]
fn percentiles_of_many_values_are_close() {
    let values: Vec<u64> = (1..=10_000).collect();
    let t = filled(&[], &values);
    check_quantile(&t, &values, 0.5);
    check_quantile(&t, &values, 0.99);
}

#[test]
fn the_report() {
    let t = filled(&[10, 100], &[5, 10, 11, 500]);
    let want = "d_bucket{le=\"10\"} 2\nd_bucket{le=\"100\"} 3\nd_bucket{le=\"+Inf\"} 4\nd_sum 526\nd_count 4\n";
    check!("values 5 10 11 500", t.render("d"), want.to_string());
}

#[test]
fn recording_allocates_nothing() {
    let t = Timings::new(&[100, 1000]);
    t.record(1);
    let (_, used) = anneal_prelude::allocs(|| {
        for v in 0..50_000u64 {
            t.record(v * 37);
        }
    });
    check!("allocations during 50000 records", used.count, 0);
}
