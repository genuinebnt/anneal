use solution::*;

/// Checks the three guarantees of the statement by trying every single step.
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
fn one_culprit() {
    let fails = |v: &[i64]| v.iter().any(|&x| x >= 100);
    check!("fails when any element >= 100", shrink(vec![3, 250, 7, 900], fails), vec![100]);
}

#[test]
fn passing_input_is_returned_unchanged() {
    check!("an input that does not fail", shrink(vec![1, 2, 3], |_| false), vec![1, 2, 3]);
}

#[test]
fn everything_fails() {
    check!("a predicate that is always true", shrink(vec![5, 6, 7], |_| true), Vec::<i64>::new());
}

#[test]
fn length_matters_but_values_do_not() {
    check!("fails when len >= 3", shrink(vec![9, -4, 8, 7, 6], |v| v.len() >= 3), vec![0, 0, 0]);
}

#[test]
fn two_elements_that_must_stay_in_order() {
    let fails = |v: &[i64]| v.windows(2).any(|w| w[0] > w[1]);
    let got = shrink(vec![1, 2, 9, 4, 5, 6], fails);
    check!(format!("fails when not sorted; got {got:?}"), violation(&got, fails), None);
    check!("length", got.len(), 2);
}
