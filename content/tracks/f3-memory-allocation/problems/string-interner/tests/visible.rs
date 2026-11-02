use solution::*;

#[test]
fn same_string_same_symbol() {
    check!(r#"intern twice"#, { let mut i = Interner::new(); let a = i.intern("foo"); let b = i.intern("foo"); (a == b, i.len()) }, (true, 1));
}

#[test]
fn different_strings_differ() {
    check!(r#"foo and bar"#, { let mut i = Interner::new(); let a = i.intern("foo"); let b = i.intern("bar"); (a == b, i.len()) }, (false, 2));
}

#[test]
fn dense_first_seen() {
    check!(r#"symbols 0, 1, 0, 2"#, { let mut i = Interner::new(); [i.intern("a").0, i.intern("b").0, i.intern("a").0, i.intern("c").0] }, [0, 1, 0, 2]);
}

#[test]
fn round_trip() {
    check!(r#"resolve gives the text back"#, { let mut i = Interner::new(); let s = i.intern("hello"); let t = i.intern("world"); (i.resolve(s).to_string(), i.resolve(t).to_string()) }, ("hello".to_string(), "world".to_string()));
}

#[test]
fn get_never_creates() {
    check!(r#"get on a stranger"#, { let mut i = Interner::new(); let a = i.intern("x"); (i.get("x") == Some(a), i.get("y"), i.len()) }, (true, None, 1));
}
