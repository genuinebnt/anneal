use solution::*;

#[test]
fn push_pop_both_ends() {
    check!(r#"mix of ends"#, { let mut l = List::new(); l.push_back(2); l.push_back(3); l.push_front(1); (l.pop_front(), l.pop_back(), l.pop_back(), l.pop_back(), l.len()) }, (Some(1), Some(3), Some(2), None, 0));
}

#[test]
fn iter_in_order() {
    check!(r#"front to back"#, { let mut l = List::new(); for x in [1, 2, 3] { l.push_back(x); } l.push_front(0); l.iter().copied().collect::<Vec<_>>() }, vec![0, 1, 2, 3]);
}

#[test]
fn iter_reversed() {
    check!(r#"back to front"#, { let mut l = List::new(); for x in [1, 2, 3] { l.push_back(x); } l.iter().rev().copied().collect::<Vec<_>>() }, vec![3, 2, 1]);
}

#[test]
fn front_and_back() {
    check!(r#"peek"#, { let mut l = List::new(); l.push_back('a'); l.push_back('b'); (l.front().copied(), l.back().copied()) }, (Some('a'), Some('b')));
}

#[test]
fn append_moves_everything() {
    check!(r#"other ends up empty"#, { let mut a = List::new(); let mut b = List::new(); a.push_back(1); b.push_back(2); b.push_back(3); a.append(&mut b); (a.iter().copied().collect::<Vec<_>>(), a.len(), b.len(), b.pop_front()) }, (vec![1, 2, 3], 3, 0, None));
}

#[test]
fn retain_keeps_matches() {
    check!(r#"keep even numbers"#, { let mut l = List::new(); for x in 1..=8 { l.push_back(x); } l.retain(|x| x % 2 == 0); (l.iter().copied().collect::<Vec<_>>(), l.len()) }, (vec![2, 4, 6, 8], 4));
}
