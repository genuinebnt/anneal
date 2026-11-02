use solution::*;

#[test]
fn powers_of_ten() {
    check!(r#"every length from 1 to 20"#, { let mut b = [0u8; 20]; let mut n = 1u64; let mut ok = true; for _ in 0..20 { ok &= fmt_u64(n, &mut b) == n.to_string(); ok &= fmt_u64(n - 1, &mut b) == (n - 1).to_string(); n = n.saturating_mul(10); } ok }, true);
}

#[test]
fn the_result_lives_in_the_buffer() {
    check!(r#"the &str points into buf at the end"#, { let mut b = [b'#'; 20]; let (p, l) = { let s = fmt_u64(305, &mut b); (s.as_ptr() as usize, s.len()) }; (l, p + l == b.as_ptr() as usize + 20, b[..17].iter().all(|&c| c == b'#')) }, (3, true, true));
}

#[test]
fn i64_boundaries() {
    check!(r#"around zero and the extremes"#, { let mut b = [0u8; 20]; [i64::MIN, i64::MIN + 1, -10, -9, 0, 9, 10, i64::MAX - 1, i64::MAX].iter().all(|&n| fmt_i64(n, &mut b) == n.to_string()) }, true);
}

#[test]
fn minus_zero_does_not_exist() {
    check!(r#"no negative zero"#, { let mut b = [0u8; 20]; fmt_i64(0, &mut b).to_string() }, "0".to_string());
}

#[test]
fn grouping_boundaries() {
    check!(r#"lengths 1 to 20 digits"#, { let mut b = [0u8; 26]; (fmt_grouped(100, &mut b).to_string(), fmt_grouped(1_000_000, &mut b).to_string(), fmt_grouped(12_345, &mut b).to_string(), fmt_grouped(u64::MAX, &mut b).to_string()) }, ("100".to_string(), "1,000,000".to_string(), "12,345".to_string(), "18,446,744,073,709,551,615".to_string()));
}

#[test]
fn negative_after_positive_in_one_buffer() {
    check!(r#"no stale minus or digits"#, { let mut b = [0u8; 20]; let a = fmt_i64(-123456789, &mut b).to_string(); let c = fmt_i64(7, &mut b).to_string(); (a, c) }, ("-123456789".to_string(), "7".to_string()));
}

#[test]
fn buffer_reuse_leaves_no_stale_digits() {
    check!(r#"short after long"#, { let mut b = [0u8; 20]; let long = fmt_u64(u64::MAX, &mut b).len(); let short = fmt_u64(5, &mut b).to_string(); (long, short) }, (20, "5".to_string()));
}

#[test]
fn matches_to_string_on_random_values() {
    let mut rng = anneal_prelude::Rng::new(808);
    let mut b = [0u8; 20];
    let mut g = [0u8; 26];
    for _ in 0..20_000 {
        let bits = rng.below(64) + 1;
        let u = (rng.below(1 << 20) as u64) << 32 ^ (rng.below(1 << 20) as u64) << 16 ^ rng.below(1 << 16) as u64;
        let u = u >> (64 - bits as u32).min(63);
        let i = u as i64 * if rng.bool() { -1 } else { 1 };
        check!(format!("u64 {u}"), fmt_u64(u, &mut b).to_string(), u.to_string());
        check!(format!("i64 {i}"), fmt_i64(i, &mut b).to_string(), i.to_string());
        let digits = u.to_string();
        let mut want = String::new();
        for (k, c) in digits.chars().enumerate() {
            if k > 0 && (digits.len() - k) % 3 == 0 {
                want.push(',');
            }
            want.push(c);
        }
        check!(format!("grouped {u}"), fmt_grouped(u, &mut g).to_string(), want);
    }
}
