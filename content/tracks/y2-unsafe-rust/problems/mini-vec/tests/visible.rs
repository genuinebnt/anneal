use solution::*;

#[test]
fn push_pop() {
    check!(r#"three values"#, { let mut v = MyVec::new(); v.push(1); v.push(2); v.push(3); (v.pop(), v.pop(), v.len(), v.pop(), v.pop()) }, (Some(3), Some(2), 1, Some(1), None));
}

#[test]
fn derefs_to_a_slice() {
    check!(r#"index, iterate, sort"#, { let mut v = MyVec::new(); for x in [3, 1, 2] { v.push(x); } v.sort(); (v[0], v.iter().sum::<i32>(), v.to_vec()) }, (1, 6, vec![1, 2, 3]));
}

#[test]
fn insert_and_remove() {
    check!(r#"shift right then left"#, { let mut v = MyVec::new(); for x in [1, 2, 4] { v.push(x); } v.insert(2, 3); v.insert(0, 0); v.insert(5, 5); let r = v.remove(1); (r, v.to_vec()) }, (1, vec![0, 2, 3, 4, 5]));
}

#[test]
fn capacity_sequence() {
    check!(r#"0, 4, 8, 16"#, { let mut v = MyVec::new(); let mut caps = vec![v.capacity()]; for i in 0..9 { v.push(i); if i == 0 || i == 4 || i == 8 { caps.push(v.capacity()); } } caps }, vec![0, 4, 8, 16]);
}

#[test]
fn empty_vec() {
    check!(r#"no allocation yet"#, { let mut v: MyVec<String> = MyVec::new(); (v.len(), v.is_empty(), v.capacity(), v.pop(), v.iter().count()) }, (0, true, 0, None, 0));
}
