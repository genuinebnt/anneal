use solution::*;

#[test]
fn sorts_integers() {
    check!(r#"ascending"#, { let mut v = vec![5, 1, 4, 2, 3]; sort_by_c(&mut v, |a, b| a.cmp(b)); v }, vec![1, 2, 3, 4, 5]);
}

#[test]
fn sorts_descending_with_a_closure() {
    check!(r#"reverse order"#, { let mut v = vec![5, 1, 4, 2, 3]; sort_by_c(&mut v, |a, b| b.cmp(a)); v }, vec![5, 4, 3, 2, 1]);
}

#[test]
fn sorts_by_a_key() {
    check!(r#"tuples by the second field"#, { let mut v = vec![(1, 'c'), (2, 'a'), (3, 'b')]; sort_by_c(&mut v, |a, b| a.1.cmp(&b.1)); v }, vec![(2, 'a'), (3, 'b'), (1, 'c')]);
}

#[test]
fn empty_and_single() {
    check!(r#"nothing to do"#, { let mut e: Vec<i32> = vec![]; let mut one = vec![9]; sort_by_c(&mut e, |a, b| a.cmp(b)); sort_by_c(&mut one, |a, b| a.cmp(b)); (e, one) }, (vec![], vec![9]));
}

#[test]
fn the_closure_may_capture_and_mutate() {
    check!(r#"counts its own calls"#, { let mut calls = 0; let mut v = vec![3, 2, 1]; sort_by_c(&mut v, |a, b| { calls += 1; a.cmp(b) }); (v, calls > 0) }, (vec![1, 2, 3], true));
}
