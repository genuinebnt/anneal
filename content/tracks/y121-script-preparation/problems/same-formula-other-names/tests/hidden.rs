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
fn a_lone_name() {
    check!(
        "y",
        (nameless(&n("y")), outside_names(&n("y"))),
        ("y".to_string(), vec!["y".to_string()])
    );
}

#[test]
fn a_name_stops_counting_after_its_body() {
    let t = ap(f("x", n("x")), n("x"));
    check!("(fn x. x) x", nameless(&t), "((fn #0) x)".to_string());
    check!("(fn x. x) x", outside_names(&t), vec!["x".to_string()]);
    let t = f("a", ap(f("b", n("b")), n("a")));
    check!(
        "fn a. (fn b. b) a",
        nameless(&t),
        "(fn ((fn #0) #0))".to_string()
    );
}

#[test]
fn surrounding_places_count_even_with_other_names() {
    let t = f("a", f("b", f("c", n("a"))));
    check!("fn a b c. a", nameless(&t), "(fn (fn (fn #2)))".to_string());
}

#[test]
fn a_hidden_place_still_counts() {
    let t = f("a", f("b", f("b", n("a"))));
    check!(
        "fn a. fn b. fn b. a",
        nameless(&t),
        "(fn (fn (fn #2)))".to_string()
    );
}

#[test]
fn a_let_value_does_not_count_the_let() {
    let t = f("y", le("x", n("y"), ap(n("x"), n("y"))));
    check!(
        "fn y. let x = y in x y",
        nameless(&t),
        "(fn (let #0 (#0 #1)))".to_string()
    );
}

#[test]
fn a_let_hides_in_its_body_only() {
    let a = f("x", le("x", ap(n("x"), n("x")), n("x")));
    let b = f("p", le("q", ap(n("p"), n("p")), n("q")));
    check!("shadowing by let", same_meaning(&a, &b), true);
}

#[test]
fn swapping_two_parameters_changes_the_meaning() {
    let a = f("x", f("y", n("x")));
    let b = f("x", f("y", n("y")));
    check!("first or second", same_meaning(&a, &b), false);
}

#[test]
fn a_renaming_that_captures_changes_the_meaning() {
    // Renaming y to x inside `fn y. x y` would make the outside x refer to the parameter.
    check!(
        "capture",
        same_meaning(&f("y", ap(n("x"), n("y"))), &f("x", ap(n("x"), n("x")))),
        false
    );
}

#[test]
fn a_let_is_not_an_application() {
    check!(
        "let and apply",
        same_meaning(&le("x", n("a"), n("x")), &ap(f("x", n("x")), n("a"))),
        false
    );
}

#[test]
fn outside_names_are_sorted_and_unique() {
    let t = ap(ap(n("zz"), n("b")), ap(n("b"), f("zz", n("a1"))));
    check!(
        "zz b b a1",
        outside_names(&t),
        vec!["a1".to_string(), "b".to_string(), "zz".to_string()]
    );
}

#[test]
fn three_hundred_levels() {
    let mut t = n("deepest");
    for i in 0..299 {
        t = f(&format!("v{i}"), t);
    }
    check!(
        "299 functions",
        nameless(&t).starts_with("(fn (fn (fn "),
        true
    );
    check!(
        "299 functions",
        outside_names(&t),
        vec!["deepest".to_string()]
    );
}

// Random terms are made from a skeleton in which every use already says what it means. The skeleton is then given
// names twice: once from a tiny pool, so that names hide each other, and once with all names different.

#[allow(dead_code)]
enum Skeleton {
    Bound(usize),
    Outside(&'static str),
    Fn(Box<Skeleton>),
    Apply(Box<Skeleton>, Box<Skeleton>),
    Let(Box<Skeleton>, Box<Skeleton>),
}

#[allow(dead_code)]
fn skeleton(rng: &mut anneal_prelude::Rng, around: usize, depth: usize) -> Skeleton {
    if depth == 0 || rng.below(4) == 0 {
        return if around > 0 && rng.below(4) > 0 {
            Skeleton::Bound(rng.below(around))
        } else {
            Skeleton::Outside(rng.pick(&["free1", "free2"]))
        };
    }
    let d = depth - 1;
    match rng.below(3) {
        0 => Skeleton::Fn(Box::new(skeleton(rng, around + 1, d))),
        1 => {
            let a = Box::new(skeleton(rng, around, d));
            Skeleton::Apply(a, Box::new(skeleton(rng, around, d)))
        }
        _ => {
            let value = Box::new(skeleton(rng, around, d));
            Skeleton::Let(value, Box::new(skeleton(rng, around + 1, d)))
        }
    }
}

#[allow(dead_code)]
fn expected(s: &Skeleton) -> String {
    match s {
        Skeleton::Bound(i) => format!("#{i}"),
        Skeleton::Outside(x) => x.to_string(),
        Skeleton::Fn(b) => format!("(fn {})", expected(b)),
        Skeleton::Apply(a, b) => format!("({} {})", expected(a), expected(b)),
        Skeleton::Let(v, b) => format!("(let {} {})", expected(v), expected(b)),
    }
}

/// All names different: the n-th place that introduces a name calls it `u<n>`.
#[allow(dead_code)]
fn named(s: &Skeleton, around: &mut Vec<String>, next: &mut usize, prefix: &str) -> Term {
    match s {
        Skeleton::Bound(i) => n(&around[around.len() - 1 - i]),
        Skeleton::Outside(x) => n(x),
        Skeleton::Fn(b) => {
            *next += 1;
            let name = format!("{prefix}{next}");
            around.push(name.clone());
            let body = named(b, around, next, prefix);
            around.pop();
            f(&name, body)
        }
        Skeleton::Apply(a, b) => {
            let a = named(a, around, next, prefix);
            ap(a, named(b, around, next, prefix))
        }
        Skeleton::Let(v, b) => {
            let value = named(v, around, next, prefix);
            *next += 1;
            let name = format!("{prefix}{next}");
            around.push(name.clone());
            let body = named(b, around, next, prefix);
            around.pop();
            le(&name, value, body)
        }
    }
}

#[test]
fn random_formulas() {
    let mut rng = anneal_prelude::Rng::new(6201);
    for _ in 0..2000 {
        let s = skeleton(&mut rng, 0, 6);
        let a = named(&s, &mut Vec::new(), &mut 0, "u");
        let b = named(&s, &mut Vec::new(), &mut 0, "w");
        check!(format!("{a:?}"), nameless(&a), expected(&s));
        check!(format!("{a:?} and {b:?}"), same_meaning(&a, &b), true);
        let other = skeleton(&mut rng, 0, 3);
        let c = named(&other, &mut Vec::new(), &mut 0, "u");
        check!(
            format!("{a:?} and {c:?}"),
            same_meaning(&a, &c),
            expected(&s) == expected(&other)
        );
    }
}
