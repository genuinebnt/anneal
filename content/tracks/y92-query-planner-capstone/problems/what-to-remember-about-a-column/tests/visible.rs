use solution::*;

#[allow(dead_code)]
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[allow(dead_code)]
fn even_numbers() -> Vec<Option<i64>> {
    // 0, 2, 4, ..., 128: 65 values, so every cut lands on every second one
    (0..65).map(|i| Some(i * 2)).collect()
}

#[test]
fn counts_and_cuts() {
    let s = summarise(&even_numbers());
    check!("65 even numbers: rows, missing, distinct", (s.rows, s.missing, s.distinct), (65, 0, 65));
    check!("cuts", s.cuts.clone(), (0..=32).map(|i| i * 4).collect::<Vec<i64>>());
    check!("nothing is unusually frequent", s.common.len(), 0);
}

#[test]
fn frequent_values_are_listed_with_exact_counts() {
    let mut values = vec![Some(7); 10];
    values.extend([Some(1), Some(2), Some(3), None, None]);
    let s = summarise(&values);
    check!("rows, missing, distinct", (s.rows, s.missing, s.distinct), (15, 2, 4));
    check!("common", s.common.clone(), vec![(7, 10)]);
    check!("equal_to(7)", s.equal_to(7), 10.0);
    check!("equal_to(2): the 3 other rows over 3 other values", s.equal_to(2), 1.0);
    check!("equal_to(8): above the largest value", s.equal_to(8), 0.0);
}

#[test]
fn a_range_is_read_off_the_cuts() {
    let s = summarise(&even_numbers());
    // 9 is in the slice from 8 to 12: two whole slices, then (9 - 8 + 1) / (12 - 8 + 1) of the third
    let at_most_9 = (2.0 + 2.0 / 5.0) / 32.0 * 65.0;
    check!("between(MIN, 9) is close to the formula", close(s.between(i64::MIN, 9), at_most_9), true);
    check!("everything", close(s.between(0, 128), 65.0), true);
    check!("an empty range", s.between(9, 8), 0.0);
    check!("a range of one value is equal_to", s.between(10, 10), s.equal_to(10));
}

#[test]
fn a_column_with_nothing_in_it() {
    let s = summarise(&[None, None]);
    check!("rows, missing, distinct", (s.rows, s.missing, s.distinct), (2, 2, 0));
    check!("no cuts", s.cuts.len(), 0);
    check!("nothing matches", (s.equal_to(0), s.between(i64::MIN, i64::MAX)), (0.0, 0.0));
}

#[test]
fn present_leaves_out_the_missing() {
    let s = summarise(&[Some(4), None, Some(4), None, None]);
    check!("present", s.present(), 2);
    check!("equal_to(4)", s.equal_to(4), 2.0);
}
