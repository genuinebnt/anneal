use solution::*;

#[allow(dead_code)]
fn n(x: &str) -> Term {
    Term::Name(x.into())
}

#[allow(dead_code)]
fn f(x: &str, body: Term) -> Term {
    Term::Fn(x.into(), Box::new(body))
}

#[allow(dead_code)]
fn ap(a: Term, b: Term) -> Term {
    Term::Apply(Box::new(a), Box::new(b))
}

#[allow(dead_code)]
fn le(x: &str, value: Term, body: Term) -> Term {
    Term::Let(x.into(), Box::new(value), Box::new(body))
}

#[test]
fn two_parameters() {
    let t = f("x", f("y", ap(n("x"), n("y"))));
    check!(
        "fn x. fn y. x y",
        nameless(&t),
        "(fn (fn (#1 #0)))".to_string()
    );
}

#[test]
fn renaming_changes_nothing() {
    let a = f("x", ap(n("x"), n("z")));
    let b = f("q", ap(n("q"), n("z")));
    check!("fn x. x z  and  fn q. q z", same_meaning(&a, &b), true);
}

#[test]
fn outside_names_matter() {
    check!(
        "fn x. y  and  fn x. z",
        same_meaning(&f("x", n("y")), &f("x", n("z"))),
        false
    );
    check!(
        "fn x. y  and  fn y. y",
        same_meaning(&f("x", n("y")), &f("y", n("y"))),
        false
    );
}

#[test]
fn the_value_of_a_let_is_outside_it() {
    let t = le("x", n("x"), n("x"));
    check!("let x = x in x", nameless(&t), "(let x #0)".to_string());
    check!("let x = x in x", outside_names(&t), vec!["x".to_string()]);
}

#[test]
fn the_nearest_one_wins() {
    let t = f("a", f("a", n("a")));
    check!("fn a. fn a. a", nameless(&t), "(fn (fn #0))".to_string());
}
