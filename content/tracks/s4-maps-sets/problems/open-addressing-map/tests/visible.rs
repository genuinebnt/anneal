use solution::*;

#[test]
fn insert_get() {
    check!(r#"insert 1 → "a", 2 → "b""#, { let mut m = OpenMap::new(); m.insert(1, "a"); m.insert(2, "b"); (m.get(1).copied(), m.get(3).copied(), m.len()) }, (Some("a"), None, 2));
}

#[test]
fn replace() {
    check!(r#"insert 7 → 1 then 7 → 2"#, { let mut m = OpenMap::new(); let first = m.insert(7, 1); let second = m.insert(7, 2); (first, second, m.get(7).copied(), m.len()) }, (None, Some(1), Some(2), 1));
}

#[test]
fn remove() {
    check!(r#"insert 5, remove 5"#, { let mut m = OpenMap::new(); m.insert(5, 'x'); (m.remove(5), m.get(5).copied(), m.remove(5), m.len()) }, (Some('x'), None, None, 0));
}

#[test]
fn remove_missing() {
    check!(r#"remove 9 from an empty map"#, { let mut m: OpenMap<u8> = OpenMap::new(); (m.remove(9), m.len(), m.get(9).copied()) }, (None, 0, None));
}

#[test]
fn reinsert_after_remove() {
    check!(r#"insert 3; remove 3; insert 3 again"#, { let mut m = OpenMap::new(); m.insert(3, "a"); m.remove(3); (m.insert(3, "b"), m.get(3).copied(), m.len()) }, (None, Some("b"), 1));
}

#[test]
fn a_quarter_of_the_slots_stay_empty() {
    let mut m = OpenMap::new();
    let mut always = true;
    for k in 0..3u64 {
        m.insert(k, k);
    }
    for k in 3..200u64 {
        m.insert(k, k);
        always &= m.empty_slots() * 4 >= m.capacity();
        m.remove(k);
        always &= m.empty_slots() * 4 >= m.capacity();
    }
    check!("keep 3 keys, insert and remove 197 more: empty_slots() * 4 >= capacity() after every call", (always, m.len()), (true, 3));
}
