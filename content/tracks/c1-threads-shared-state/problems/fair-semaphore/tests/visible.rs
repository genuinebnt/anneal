use solution::*;

#[test]
fn acquire_and_release() {
    check!(r#"total 2: take 1, take 1, drop"#, { let s = Semaphore::new(2); let a = s.acquire(1); let b = s.acquire(1); let none = s.available(); drop(a); drop(b); (none, s.available()) }, (0, 2));
}

#[test]
fn try_acquire_fails_when_short() {
    check!(r#"total 2: hold 2, try 1"#, { let s = Semaphore::new(2); let _a = s.acquire(2); let r = s.try_acquire(1).is_none(); r }, true);
}

#[test]
fn try_acquire_ok() {
    check!(r#"total 3: try 2 then 1 then 1"#, { let s = Semaphore::new(3); let a = s.try_acquire(2); let b = s.try_acquire(1); let c = s.try_acquire(1); (a.is_some(), b.is_some(), c.is_some()) }, (true, true, false));
}

#[test]
fn permit_returns_on_drop() {
    check!(r#"scope ends"#, { let s = Semaphore::new(1); { let _p = s.acquire(1); } s.available() }, 1);
}

#[test]
fn bad_request_panics() {
    check!(r#"0 and total+1"#, { let s = Semaphore::new(2); (std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { s.acquire(0); })).is_err(), std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { s.acquire(3); })).is_err()) }, (true, true));
}
