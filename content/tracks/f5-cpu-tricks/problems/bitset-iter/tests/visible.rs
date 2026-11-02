use solution::*;

#[test]
fn insert_contains_len() {
    check!(r#"insert 3, 64, 3"#, { let mut s = BitSet::new(); let a = s.insert(3); let b = s.insert(64); let c = s.insert(3); (a, b, c, s.len(), s.contains(3), s.contains(4), s.contains(64)) }, (true, true, false, 2, true, false, true));
}

#[test]
fn iter_ascending() {
    check!(r#"insert 130, 5, 64, 63, 0"#, { let s: BitSet = [130, 5, 64, 63, 0].into_iter().collect(); s.iter().collect::<Vec<_>>() }, vec![0, 5, 63, 64, 130]);
}

#[test]
fn remove() {
    check!(r#"remove a member and a stranger"#, { let mut s: BitSet = [1, 70].into_iter().collect(); (s.remove(70), s.remove(70), s.remove(5000), s.iter().collect::<Vec<_>>()) }, (true, false, false, vec![1]));
}

#[test]
fn next_set_from() {
    check!(r#"members 2, 64, 200"#, { let s: BitSet = [2, 64, 200].into_iter().collect(); (s.next_set_from(0), s.next_set_from(3), s.next_set_from(64), s.next_set_from(65), s.next_set_from(201)) }, (Some(2), Some(64), Some(64), Some(200), None));
}

#[test]
fn set_algebra() {
    check!(r#"{1,2,70} with {2,3,70}"#, { let a: BitSet = [1, 2, 70].into_iter().collect(); let b: BitSet = [2, 3, 70].into_iter().collect(); let (mut u, mut i, mut d) = (a.clone(), a.clone(), a.clone()); u.union_with(&b); i.intersect_with(&b); d.difference_with(&b); (u.iter().collect::<Vec<_>>(), i.iter().collect::<Vec<_>>(), d.iter().collect::<Vec<_>>()) }, (vec![1, 2, 3, 70], vec![2, 70], vec![1]));
}
