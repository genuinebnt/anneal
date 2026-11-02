use solution::*;

#[test]
fn lock_and_read() {
    check!(r#"lock a 5"#, { let l = SpinLock::new(5); let g = l.lock(); let v = *g; v }, 5);
}

#[test]
fn mutate_through_guard() {
    check!(r#"add 1 twice"#, { let l = SpinLock::new(1); { *l.lock() += 1; } { *l.lock() += 1; } let v = *l.lock(); v }, 3);
}

#[test]
fn try_lock_fails_while_held() {
    check!(r#"try_lock while a guard lives"#, { let l = SpinLock::new(0); let g = l.lock(); let busy = l.try_lock().is_none(); drop(g); let again = l.try_lock().is_some(); (busy, again) }, (true, true));
}

#[test]
fn drop_releases() {
    check!(r#"lock, drop, lock again"#, { let l = SpinLock::new(String::new()); l.lock().push('a'); l.lock().push('b'); let v = l.lock().clone(); v }, "ab".to_string());
}

#[test]
fn into_inner() {
    check!(r#"value comes back"#, SpinLock::new(vec![1, 2]).into_inner(), vec![1, 2]);
}
