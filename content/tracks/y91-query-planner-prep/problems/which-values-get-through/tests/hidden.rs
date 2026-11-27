use solution::*;
use anneal_prelude::Rng;
use std::collections::BTreeMap;

#[allow(dead_code)]
fn rule(field: u32, test: Test) -> Rule {
    Rule { field, test }
}

#[allow(dead_code)]
fn passes(test: Test, v: i64) -> bool {
    match test {
        Test::Above(x) => v > x,
        Test::AtLeast(x) => v >= x,
        Test::Below(x) => v < x,
        Test::AtMost(x) => v <= x,
        Test::Exactly(x) => v == x,
    }
}

#[test]
fn nothing_is_above_the_largest_or_below_the_smallest() {
    check!("above MAX", allowed(&[rule(0, Test::Above(i64::MAX))]), None);
    check!("below MIN", allowed(&[rule(0, Test::Below(i64::MIN))]), None);
    check!("above MAX and at most 3", allowed(&[rule(0, Test::Above(i64::MAX)), rule(0, Test::AtMost(3))]), None);
    check!("at least MAX", allowed(&[rule(0, Test::AtLeast(i64::MAX))]), Some(BTreeMap::from([(0, (i64::MAX, i64::MAX))])));
    check!("at most MIN", allowed(&[rule(0, Test::AtMost(i64::MIN))]), Some(BTreeMap::from([(0, (i64::MIN, i64::MIN))])));
    check!("above MIN", allowed(&[rule(0, Test::Above(i64::MIN))]), Some(BTreeMap::from([(0, (i64::MIN + 1, i64::MAX))])));
    check!("below MAX", allowed(&[rule(0, Test::Below(i64::MAX))]), Some(BTreeMap::from([(0, (i64::MIN, i64::MAX - 1))])));
}

#[test]
fn one_impossible_field_spoils_the_whole_answer() {
    let rules = [rule(5, Test::AtLeast(0)), rule(6, Test::Exactly(1)), rule(6, Test::Exactly(2)), rule(7, Test::AtMost(9))];
    check!("field 6 cannot be 1 and 2", allowed(&rules), None);
}

#[test]
fn random_rules_against_trying_every_value() {
    let mut rng = Rng::new(9121);
    for case in 0..2000 {
        let n = rng.below(7);
        let rules: Vec<Rule> = (0..n)
            .map(|_| {
                let v = rng.int(-4, 4);
                let test = [Test::Above(v), Test::AtLeast(v), Test::Below(v), Test::AtMost(v), Test::Exactly(v)][rng.below(5)];
                rule(rng.below(3) as u32, test)
            })
            .collect();
        let got = allowed(&rules);
        // values from -10 to 10 are enough to see every bound; the two ends stand for "no bound"
        let mut want = Some(BTreeMap::new());
        for field in 0..3 {
            let mine: Vec<Test> = rules.iter().filter(|r| r.field == field).map(|r| r.test).collect();
            if mine.is_empty() {
                continue;
            }
            let good: Vec<i64> = (-10..=10).filter(|&v| mine.iter().all(|&t| passes(t, v))).collect();
            match (good.first(), good.last(), want.as_mut()) {
                (Some(&low), Some(&high), Some(map)) => {
                    let low = if low == -10 { i64::MIN } else { low };
                    let high = if high == 10 { i64::MAX } else { high };
                    map.insert(field, (low, high));
                }
                _ => want = None,
            }
        }
        check!(format!("case {case}: {rules:?}"), got, want);
    }
}

#[test]
fn ranges_that_are_backwards_or_at_the_ends() {
    let entries = [(i64::MIN, 0), (-1, 1), (5, 2), (5, 3), (i64::MAX, 4)];
    check!("low above high, inside the data", between(&entries, 6, 4).len(), 0);
    check!("far backwards", between(&entries, i64::MAX, i64::MIN).len(), 0);
    check!("the smallest only", between(&entries, i64::MIN, i64::MIN), &entries[..1]);
    check!("the largest only", between(&entries, i64::MAX, i64::MAX), &entries[4..]);
    check!("both fives", between(&entries, 5, 5), &entries[2..4]);
    check!("all", between(&entries, i64::MIN, i64::MAX), &entries[..]);
    check!("an empty list", between(&[], 1, 2).len(), 0);
}

#[test]
fn random_ranges_against_a_filter() {
    let mut rng = Rng::new(9122);
    for case in 0..1000 {
        let n = rng.below(30);
        let mut entries: Vec<(i64, u32)> = (0..n).map(|i| (rng.int(0, 8), i as u32)).collect();
        entries.sort();
        let (low, high) = (rng.int(-1, 9), rng.int(-1, 9));
        let want: Vec<(i64, u32)> = entries.iter().copied().filter(|e| e.0 >= low && e.0 <= high).collect();
        check!(format!("case {case}: {entries:?} between {low} and {high}"), between(&entries, low, high).to_vec(), want);
    }
}

#[test]
fn many_rules() {
    let rules: Vec<Rule> = (0..100_000).map(|i| rule(i % 100, Test::AtLeast((i / 100) as i64))).collect();
    let got = allowed(&rules).map(|m| (m.len(), m[&42]));
    check!("100 000 rules", got, Some((100, (999, i64::MAX))));
}

#[test]
fn the_order_of_the_rules_does_not_matter() {
    let mut rng = Rng::new(9123);
    for _ in 0..300 {
        let mut rules: Vec<Rule> = (0..6).map(|i| rule(i % 2, [Test::AtLeast(rng.int(-3, 1)), Test::AtMost(rng.int(0, 4)), Test::Above(rng.int(-3, 0))][rng.below(3)])).collect();
        let before = allowed(&rules);
        let shown = rules.clone();
        rng.shuffle(&mut rules);
        check!(format!("{shown:?} shuffled to {rules:?}"), allowed(&rules), before);
    }
}

#[test]
fn a_long_list_of_entries() {
    let entries: Vec<(i64, u32)> = (0..200_000).map(|i| ((i / 3) as i64, i)).collect();
    check!("values 1000 to 1001", between(&entries, 1000, 1001), &entries[3000..3006]);
}
