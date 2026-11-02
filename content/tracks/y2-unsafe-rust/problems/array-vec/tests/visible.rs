use solution::*;

#[test]
fn push_until_full() {
    check!(r#"capacity 3"#, { let mut v: ArrayVec<i32, 3> = ArrayVec::new(); let r: Vec<Result<(), i32>> = (1..=4).map(|x| v.try_push(x)).collect(); (r, v.len(), v.is_full(), v.to_vec()) }, (vec![Ok(()), Ok(()), Ok(()), Err(4)], 3, true, vec![1, 2, 3]));
}

#[test]
fn pop_order() {
    check!(r#"last in, first out"#, { let mut v: ArrayVec<_, 4> = ArrayVec::new(); v.try_push('a').unwrap(); v.try_push('b').unwrap(); (v.pop(), v.pop(), v.pop(), v.len()) }, (Some('b'), Some('a'), None, 0));
}

#[test]
fn insert_and_remove_keep_order() {
    check!(r#"middle edits"#, { let mut v: ArrayVec<i32, 6> = ArrayVec::new(); for x in [1, 2, 4] { v.try_push(x).unwrap(); } v.try_insert(2, 3).unwrap(); v.try_insert(0, 0).unwrap(); let r = v.remove(1); (r, v.to_vec()) }, (1, vec![0, 2, 3, 4]));
}

#[test]
fn swap_remove_is_unordered() {
    check!(r#"last element fills the hole"#, { let mut v: ArrayVec<i32, 5> = ArrayVec::new(); for x in 1..=5 { v.try_push(x).unwrap(); } let r = v.swap_remove(1); (r, v.to_vec()) }, (2, vec![1, 5, 3, 4]));
}

#[test]
fn derefs_to_a_slice() {
    check!(r#"slice methods"#, { let mut v: ArrayVec<i32, 4> = ArrayVec::new(); for x in [3, 1, 2] { v.try_push(x).unwrap(); } v.sort(); (v.iter().sum::<i32>(), v.contains(&2), v.to_vec()) }, (6, true, vec![1, 2, 3]));
}
