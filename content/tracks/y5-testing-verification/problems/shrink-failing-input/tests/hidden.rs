use solution::*;

#[allow(dead_code)]
fn violation(result: &[i64], mut fails: impl FnMut(&[i64]) -> bool) -> Option<String> {
    if !fails(result) {
        return Some("the result no longer fails".to_string());
    }
    for i in 0..result.len() {
        let mut shorter = result.to_vec();
        shorter.remove(i);
        if fails(&shorter) {
            return Some(format!("element {i} can still be removed"));
        }
        let x = result[i];
        for y in [0, x / 2, x - x.signum()] {
            let mut simpler = result.to_vec();
            simpler[i] = y;
            if y != x && fails(&simpler) {
                return Some(format!("element {i} can still become {y}"));
            }
        }
    }
    None
}

#[test]
fn empty_input() {
    check!("empty and failing", shrink(Vec::new(), |_| true), Vec::<i64>::new());
}

#[test]
fn negative_values_move_towards_zero() {
    let fails = |v: &[i64]| v.iter().any(|&x| x <= -37);
    check!("fails when any element <= -37", shrink(vec![5, -1000, 12], fails), vec![-37]);
}

#[test]
fn extremes_do_not_overflow() {
    let fails = |v: &[i64]| v.iter().any(|&x| x < -1_000_000_000_000_000_000);
    let mut calls = 0usize;
    let got = shrink(vec![i64::MIN, i64::MAX], |v| {
        calls += 1;
        assert!(calls <= 20_000, "more than 20 000 calls: moving one value takes too many steps");
        fails(v)
    });
    check!("i64::MIN, within 20 000 calls", got, vec![-1_000_000_000_000_000_001]);
}

#[test]
fn a_value_change_unlocks_a_removal() {
    // Fails with a lone 0..=3, or with any pair that sums to at least 50.
    let fails = |v: &[i64]| (v.len() == 2 && v[0] + v[1] >= 50) || (v.len() == 1 && v[0] > 0 && v[0] <= 60 && v[0] % 7 == 0);
    let got = shrink(vec![0, 56], fails);
    check!(format!("one step enables another; got {got:?}"), violation(&got, fails), None);
}

#[test]
fn removal_after_simplification() {
    // Two elements are needed until one of them reaches 0.
    let fails = |v: &[i64]| match v {
        [a, b] => *a == 0 && *b >= 5 || *a >= 1 && *b >= 5,
        [b] => *b == 5,
        _ => false,
    };
    check!("[8, 90]", shrink(vec![8, 90], fails), vec![5]);
}

#[test]
fn order_is_kept() {
    let needle = [7i64, -3, 7];
    let fails = |v: &[i64]| {
        let mut want = needle.iter();
        let mut next = want.next();
        for x in v {
            if Some(x) == next {
                next = want.next();
            }
        }
        next.is_none()
    };
    check!("fails when 7, -3, 7 appear in this order", shrink(vec![1, 7, 7, 2, -3, -3, 5, 7, 7], fails), vec![7, -3, 7]);
}

#[test]
fn large_input_stays_within_the_call_budget() {
    let mut input = vec![1i64; 10_000];
    input[1234] = 41;
    input[8765] = -17;
    let mut calls = 0usize;
    let got = shrink(input, |v| {
        calls += 1;
        assert!(calls <= 500, "more than 500 calls to fails");
        v.contains(&41) && v.contains(&-17)
    });
    check!("10 000 elements, two matter", got, vec![41, -17]);
}

#[test]
fn needles_at_both_ends() {
    let mut input = vec![5i64; 4096];
    input[0] = 1001;
    input[4095] = 1002;
    let mut calls = 0usize;
    let got = shrink(input, |v| {
        calls += 1;
        assert!(calls <= 500, "more than 500 calls to fails");
        v.first() == Some(&1001) && v.last() == Some(&1002)
    });
    check!("first and last of 4096", got, vec![1001, 1002]);
}

#[test]
fn never_asks_about_an_invented_list() {
    let input = vec![4i64, -9, 6, 0, 3];
    let bound: i64 = input.iter().map(|x| x.abs()).max().unwrap();
    let mut ok = true;
    let _ = shrink(input.clone(), |v| {
        ok &= v.len() <= input.len() && v.iter().all(|x| x.abs() <= bound);
        v.len() >= 2
    });
    check!("every candidate is no longer and no larger than the input", ok, true);
}

#[test]
fn random_predicates_reach_a_local_minimum() {
    let mut rng = anneal_prelude::Rng::new(8111);
    for _ in 0..400 {
        let len = rng.below(14);
        let input: Vec<i64> = rng.vec(len, -40, 40);
        let kind = rng.below(5);
        let k = rng.int(1, 30);
        let fails = move |v: &[i64]| match kind {
            0 => v.iter().sum::<i64>() >= k,
            1 => v.iter().filter(|x| **x < 0).count() as i64 >= k % 4,
            2 => v.windows(2).any(|w| (w[0] - w[1]).abs() >= k),
            3 => v.len() % 2 == 1 && v.iter().any(|x| x.abs() >= k % 9),
            _ => v.iter().map(|x| x.abs()).sum::<i64>() >= k && v.len() >= (k % 3) as usize,
        };
        let got = shrink(input.clone(), fails);
        let shown = format!("predicate {kind} with k={k} on {input:?} gave {got:?}");
        if fails(&input) {
            check!(shown, violation(&got, fails), None);
        } else {
            check!(shown, got, input);
        }
    }
}
