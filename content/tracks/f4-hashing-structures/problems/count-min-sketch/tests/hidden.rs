use solution::*;

#[test]
fn fresh_sketch_is_empty() {
    check!(r#"nothing seen"#, { let c = CountMin::new(50, 3); (c.estimate("x"), c.total()) }, (0, 0));
}

#[test]
fn zero_count_adds_nothing() {
    check!(r#"add(_, 0)"#, { let mut c = CountMin::new(50, 3); c.add("a", 0); (c.estimate("a"), c.total()) }, (0, 0));
}

#[test]
fn saturates_instead_of_overflowing() {
    check!(r#"u64::MAX twice"#, { let mut c = CountMin::new(10, 2); c.add("a", u64::MAX); c.add("a", 5); (c.estimate("a"), c.total()) }, (u64::MAX, u64::MAX));
}

#[test]
fn str_and_string_agree() {
    check!(r#"same hashing"#, { let mut c = CountMin::new(100, 3); c.add("k", 2); c.add(&String::from("k"), 3); c.estimate("k") }, 5);
}

#[test]
fn degenerate_shapes_are_clamped() {
    check!(r#"zero width and depth"#, { let mut c = CountMin::new(0, 0); c.add("a", 1); (c.width(), c.depth(), c.estimate("a")) }, (1, 1, 1));
}

#[test]
fn count_is_added_not_one() {
    check!(r#"weighted adds"#, { let mut c = CountMin::new(500, 4); c.add(&7u32, 1000); c.estimate(&7u32) }, 1000);
}

#[test]
fn heavy_hitters_do_not_spoil_light_keys() {
    // 20 heavy keys (3000 each) and 20,000 light keys (2 each): total 100,000, so eps * total = 1000.
    let mut c = CountMin::with_error(0.01, 0.01);
    for h in 0..20u32 {
        c.add(&format!("heavy-{h}"), 3000);
    }
    for l in 0..20_000u32 {
        c.add(&format!("light-{l}"), 2);
    }
    let total = c.total();
    check!("total", total, 100_000);
    for h in 0..20u32 {
        assert!(c.estimate(&format!("heavy-{h}")) >= 3000, "undercounted a heavy key");
    }
    let bound = (0.01 * total as f64) as u64;
    let within = (0..20_000u32).filter(|l| c.estimate(&format!("light-{l}")) <= 2 + bound).count();
    let frac = within as f64 / 20_000.0;
    assert!(frac >= 0.99, "only {frac} of light keys were within eps * total of the truth: rows are not independent, or the estimate is not the minimum");
    let top = (0..20u32).map(|h| c.estimate(&format!("heavy-{h}"))).min().unwrap();
    let loudest_light = (0..20_000u32).map(|l| c.estimate(&format!("light-{l}"))).filter(|&e| e < 3000).max().unwrap();
    assert!(top >= 3000 && loudest_light < top + 1, "heavy hitters should stand out");
}

#[test]
fn random_stream_never_undercounts() {
    let mut rng = anneal_prelude::Rng::new(6901);
    let mut c = CountMin::new(64, 4);
    let mut truth = std::collections::HashMap::new();
    for _ in 0..30_000 {
        let key = rng.below(500) as u32;
        let n = 1 + rng.below(5) as u64;
        c.add(&key, n);
        *truth.entry(key).or_insert(0u64) += n;
    }
    for (k, v) in &truth {
        assert!(c.estimate(k) >= *v, "key {k}: estimate {} below the true {v}", c.estimate(k));
    }
    check!("total matches", c.total(), truth.values().sum::<u64>());
}
