use solution::*;

#[test]
fn many_readers_at_once() {
    check!(r#"three read guards together"#, { let l = RwLock::new(5); let a = l.read(); let b = l.read(); let c = l.read(); *a + *b + *c }, 15);
}

#[test]
fn writer_excludes_readers() {
    check!(r#"write guard alive: try_read and try_write fail"#, { let l = RwLock::new(0); let w = l.write(); let r = (l.try_read().is_none(), l.try_write().is_none()); drop(w); r }, (true, true));
}

#[test]
fn readers_exclude_writer() {
    check!(r#"read guard alive: try_write fails, try_read works"#, { let l = RwLock::new(0); let r = l.read(); let out = (l.try_write().is_none(), l.try_read().is_some()); drop(r); out }, (true, true));
}

#[test]
fn write_then_read() {
    check!(r#"mutation is visible"#, { let l = RwLock::new(vec![1]); l.write().push(2); let v = l.read().clone(); v }, vec![1, 2]);
}

#[test]
fn released_on_drop() {
    check!(r#"all guards dropped"#, { let l = RwLock::new(1); { let _a = l.read(); let _b = l.read(); } { let mut w = l.write(); *w += 1; } let v = *l.read(); v }, 2);
}
