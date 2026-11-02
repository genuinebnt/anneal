use solution::*;

#[test]
fn empty_get() {
    check!(r#"fresh cell"#, Once::<u32>::new().get().copied(), None);
}

#[test]
fn init_then_get() {
    check!(r#"get_or_init(|| 5), then get"#, { let c = Once::new(); let a = *c.get_or_init(|| 5); (a, c.get().copied()) }, (5, Some(5)));
}

#[test]
fn second_init_ignored() {
    check!(r#"get_or_init(|| 1), get_or_init(|| 2)"#, { let c = Once::new(); c.get_or_init(|| 1); *c.get_or_init(|| 2) }, 1);
}

#[test]
fn same_reference() {
    check!(r#"two calls return the same address"#, { let c = Once::new(); let a = c.get_or_init(|| String::from("x")) as *const String; let b = c.get_or_init(|| String::from("y")) as *const String; a == b }, true);
}

#[test]
fn into_inner() {
    check!(r#"init, into_inner; and an empty one"#, { let c = Once::new(); c.get_or_init(|| "v".to_string()); (c.into_inner(), Once::<u8>::new().into_inner()) }, (Some("v".to_string()), None));
}
