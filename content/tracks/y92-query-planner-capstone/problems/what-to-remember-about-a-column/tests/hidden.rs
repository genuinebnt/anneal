use solution::*;
use anneal_prelude::Rng;

#[allow(dead_code)]
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn the_empty_column() {
    let s = summarise(&[]);
    check!("rows, missing, distinct", (s.rows, s.missing, s.distinct), (0, 0, 0));
    check!("cuts and common", (s.cuts.len(), s.common.len()), (0, 0));
    check!("estimates", (s.equal_to(3), s.between(1, 5), s.between(4, 4)), (0.0, 0.0, 0.0));
}

#[test]
fn one_value_many_times() {
    let s = summarise(&vec![Some(5); 100]);
    check!("33 cuts, all 5", (s.cuts.len(), s.cuts.iter().all(|&c| c == 5)), (33, true));
    check!("one value is never unusual", s.common.len(), 0);
    check!("equal_to", (s.equal_to(5), s.equal_to(4), s.equal_to(6)), (100.0, 0.0, 0.0));
    check!("ranges", (s.between(0, 9), s.between(6, 9), s.between(0, 4)), (100.0, 0.0, 0.0));
}

#[test]
fn a_single_row() {
    let s = summarise(&[Some(-3)]);
    check!("cuts", s.cuts.clone(), vec![-3; 33]);
    check!("estimates", (s.equal_to(-3), s.between(-10, 0)), (1.0, 1.0));
}

#[test]
fn the_threshold_for_common_is_strictly_more_than_a_quarter_above_average() {
    // 4 values, 16 rows, average 4: a count of 5 is exactly 1.25 times the average and does not count
    let mut at = vec![Some(1); 5];
    for v in [2, 3] {
        at.extend(vec![Some(v); 4]);
    }
    at.extend(vec![Some(4); 3]);
    check!("count 5 against average 4", summarise(&at).common.clone(), Vec::<(i64, usize)>::new());
    let mut over = vec![Some(1); 6];
    over.extend(vec![Some(2); 4]);
    over.extend(vec![Some(3); 3]);
    over.extend(vec![Some(4); 3]);
    check!("count 6 against average 4", summarise(&over).common.clone(), vec![(1, 6)]);
}

#[test]
fn only_eight_are_listed_most_frequent_first_and_ties_by_value() {
    let mut values = Vec::new();
    // values 100..110 appear 50 times each, value 5 appears 60 times, and 200 values appear once
    for v in (100..110).rev() {
        values.extend(vec![Some(v); 50]);
    }
    values.extend(vec![Some(5); 60]);
    values.extend((1000..1200).map(Some));
    let want: Vec<(i64, usize)> = std::iter::once((5, 60)).chain((100..107).map(|v| (v, 50))).collect();
    let s = summarise(&values);
    check!("common", s.common.clone(), want);
    let listed = 60 + 7 * 50;
    let others = (values.len() - listed) as f64 / (s.distinct - 8) as f64;
    check!("a value left off the list shares the rest", close(s.equal_to(108), others), true);
}

#[test]
fn when_every_value_is_listed_an_unlisted_one_matches_nothing() {
    let mut values = vec![Some(1); 10];
    values.extend(vec![Some(9); 10]);
    values.push(Some(5));
    let s = summarise(&values);
    check!("distinct", s.distinct, 3);
    check!("1 and 9 are common", s.common.clone(), vec![(1, 10), (9, 10)]);
    check!("the one other value", close(s.equal_to(5), 1.0), true);
    check!("4 is inside the range, so it is guessed like any unlisted value", close(s.equal_to(4), 1.0), true);
}

#[test]
fn the_ends_of_the_number_line() {
    let s = summarise(&[Some(i64::MIN), Some(0), Some(i64::MAX), None]);
    check!("cuts start and end at the ends", (s.cuts[0], s.cuts[32]), (i64::MIN, i64::MAX));
    check!("everything", close(s.between(i64::MIN, i64::MAX), 3.0), true);
    let half = s.between(i64::MIN, 0);
    check!("a range up to 0 is between 0 and 3", (0.0..=3.0).contains(&half), true);
    let upper = s.between(1, i64::MAX);
    check!("the rest is between 0 and 3", (0.0..=3.0).contains(&upper), true);
    check!("the two halves add up", close(half + upper, 3.0), true);
    check!("MAX alone", s.between(i64::MAX, i64::MAX), s.equal_to(i64::MAX));
}

/// The definition again, written the long way.
#[allow(dead_code)]
fn slow_at_most(s: &ColumnStats, v: i64) -> f64 {
    let present = (s.rows - s.missing) as f64;
    if s.cuts.is_empty() || v < s.cuts[0] {
        return 0.0;
    }
    if v >= s.cuts[32] {
        return present;
    }
    let mut slice = 0;
    for i in 0..32 {
        if s.cuts[i] <= v {
            slice = i;
        }
    }
    let (a, b) = (s.cuts[slice] as f64, s.cuts[slice + 1] as f64);
    (slice as f64 + (v as f64 - a + 1.0) / (b - a + 1.0)) / 32.0 * present
}

#[allow(dead_code)]
fn random_column(rng: &mut Rng) -> Vec<Option<i64>> {
    let n = rng.below(300);
    let span = [3, 40, 5000][rng.below(3)];
    let hot = rng.int(0, span);
    (0..n)
        .map(|_| match rng.below(10) {
            0 => None,
            1..=3 => Some(hot),
            _ => Some(rng.int(0, span)),
        })
        .collect()
}

#[test]
fn random_columns_match_the_definition() {
    let mut rng = Rng::new(9101);
    for case in 0..300 {
        let values = random_column(&mut rng);
        let s = summarise(&values);
        let mut sorted: Vec<i64> = values.iter().flatten().copied().collect();
        sorted.sort();
        let cuts: Vec<i64> = if sorted.is_empty() { vec![] } else { (0..33).map(|i| sorted[i * (sorted.len() - 1) / 32]).collect() };
        check!(format!("case {case}: cuts of {values:?}"), s.cuts.clone(), cuts);
        let mut distinct = sorted.clone();
        distinct.dedup();
        check!(format!("case {case}: distinct of {values:?}"), s.distinct, distinct.len());
        let mut counts: Vec<(i64, usize)> = distinct.iter().map(|&v| (v, sorted.iter().filter(|&&x| x == v).count())).collect();
        counts.sort_by_key(|&(v, c)| (std::cmp::Reverse(c), v));
        let common: Vec<(i64, usize)> = counts.into_iter().take(8).filter(|&(_, c)| c as f64 > 1.25 * sorted.len() as f64 / distinct.len() as f64).collect();
        check!(format!("case {case}: common of {values:?}"), s.common.clone(), common);
        for _ in 0..20 {
            let (low, high) = (rng.int(-5, 5100), rng.int(-5, 5100));
            let want = if low > high {
                0.0
            } else if low == high {
                s.equal_to(low)
            } else {
                (slow_at_most(&s, high) - slow_at_most(&s, low - 1)).max(0.0)
            };
            let got = s.between(low, high);
            check!(format!("case {case}: between({low}, {high}) = {got}, wanted {want}, for {values:?}"), close(got, want), true);
            let listed: usize = s.common.iter().map(|c| c.1).sum();
            let equal = if s.cuts.is_empty() || low < s.cuts[0] || low > s.cuts[32] {
                0.0
            } else if let Some(c) = s.common.iter().find(|c| c.0 == low) {
                c.1 as f64
            } else if s.distinct == s.common.len() {
                0.0
            } else {
                (sorted.len() - listed) as f64 / (s.distinct - s.common.len()) as f64
            };
            let got = s.equal_to(low);
            check!(format!("case {case}: equal_to({low}) = {got}, wanted {equal}, for {values:?}"), close(got, equal), true);
        }
    }
}

#[test]
fn on_evenly_spread_values_a_range_is_within_two_slices_and_two_rows() {
    let mut rng = Rng::new(9102);
    let values: Vec<Option<i64>> = (0..20_000).map(|_| Some(rng.int(0, 999_999))).collect();
    let s = summarise(&values);
    for _ in 0..300 {
        let (a, b) = (rng.int(0, 999_999), rng.int(0, 999_999));
        let (low, high) = (a.min(b), a.max(b) + 1);
        let truth = values.iter().flatten().filter(|&&v| v >= low && v <= high).count() as f64;
        let got = s.between(low, high);
        check!(format!("between({low}, {high}): estimate {got}, truth {truth}"), (got - truth).abs() <= 2.0 * 20_000.0 / 32.0 + 2.0, true);
    }
}

#[test]
fn a_large_column() {
    let values: Vec<Option<i64>> = (0..200_000).map(|i| Some((i * 7919) % 200_000)).collect();
    let s = summarise(&values);
    check!("200 000 distinct values", (s.distinct, s.common.len(), s.cuts[32]), (200_000, 0, 199_999));
}
