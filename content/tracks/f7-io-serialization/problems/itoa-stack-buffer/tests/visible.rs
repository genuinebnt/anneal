use solution::*;

#[test]
fn small_numbers() {
    check!(r#"0, 7, 10, 99, 100"#, { let mut b = [0u8; 20]; let mut out = Vec::new(); for n in [0u64, 7, 10, 99, 100] { out.push(fmt_u64(n, &mut b).to_string()); } out }, vec!["0", "7", "10", "99", "100"].into_iter().map(String::from).collect::<Vec<_>>());
}

#[test]
fn the_largest_u64() {
    check!(r#"20 digits fill the buffer"#, { let mut b = [0u8; 20]; fmt_u64(u64::MAX, &mut b).to_string() }, "18446744073709551615".to_string());
}

#[test]
fn negative_numbers() {
    check!(r#"a leading minus"#, { let mut b = [0u8; 20]; (fmt_i64(-1, &mut b).to_string(), fmt_i64(-4050, &mut b).to_string(), fmt_i64(42, &mut b).to_string()) }, ("-1".to_string(), "-4050".to_string(), "42".to_string()));
}

#[test]
fn i64_min_and_max() {
    check!(r#"the extremes"#, { let mut b = [0u8; 20]; (fmt_i64(i64::MIN, &mut b).to_string(), fmt_i64(i64::MAX, &mut b).to_string()) }, ("-9223372036854775808".to_string(), "9223372036854775807".to_string()));
}

#[test]
fn grouping() {
    check!(r#"commas every three digits"#, { let mut b = [0u8; 26]; (fmt_grouped(0, &mut b).to_string(), fmt_grouped(999, &mut b).to_string(), fmt_grouped(1000, &mut b).to_string(), fmt_grouped(1234567, &mut b).to_string()) }, ("0".to_string(), "999".to_string(), "1,000".to_string(), "1,234,567".to_string()));
}
