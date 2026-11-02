use solution::*;

#[test]
fn deref() {
    check!(r#"new(5)"#, *MiniArc::new(5), 5);
}

#[test]
fn clone_shares() {
    check!(r#"two handles, one allocation"#, { let a = MiniArc::new(String::from("x")); let b = a.clone(); (MiniArc::ptr_eq(&a, &b), MiniArc::strong_count(&a), b.len()) }, (true, 2, 1));
}

#[test]
fn count_goes_down() {
    check!(r#"clone, clone, drop one"#, { let a = MiniArc::new(1); let b = a.clone(); let c = a.clone(); drop(b); let n = MiniArc::strong_count(&a); drop(c); (n, MiniArc::strong_count(&a)) }, (2, 1));
}

#[test]
fn get_mut_only_when_unique() {
    check!(r#"get_mut with and without a clone"#, { let mut a = MiniArc::new(1); let b = a.clone(); let shared = MiniArc::get_mut(&mut a).is_none(); drop(b); let unique = MiniArc::get_mut(&mut a).is_some(); (shared, unique) }, (true, true));
}

#[test]
fn try_unwrap_unique_and_shared() {
    check!(r#"unique, then shared"#, { let a = MiniArc::new(7); let ok = MiniArc::try_unwrap(a).ok(); let b = MiniArc::new(8); let b2 = b.clone(); let err = MiniArc::try_unwrap(b).err().map(|x| *x); drop(b2); (ok, err) }, (Some(7), Some(8)));
}
