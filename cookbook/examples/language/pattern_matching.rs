//! RECIPE 46 (language): pattern matching: destructuring, guards, bindings, slice patterns, let-else.
//! LEVEL: simple
//! Use in: parsing tokens, state transitions, handling Option/Result, walking ASTs.   Run: cargo run --example pattern_matching
//! COMMON USES: parsers, state machines, error handling, walking ASTs: exhaustive matching finds every place to update.
#![allow(dead_code)]
//!
//! `match` must be EXHAUSTIVE: add a variant to an enum and every `match` that lacks a wildcard stops compiling.
//! That is a feature: it finds every place you must update.  Avoid a lazy `_ =>` arm on enums you own.
//! Read: https://doc.rust-lang.org/book/ch18-00-patterns.html
#[derive(Debug)]
enum Shape {
    Circle { r: f64 },
    Rect { w: f64, h: f64 },
    Point,
}
#[derive(Debug)]
enum Msg {
    Quit,
    Move { x: i32, y: i32 },
    Text(String),
    Pair(i32, i32),
}

fn area(s: &Shape) -> f64 {
    match s {
        Shape::Circle { r } => std::f64::consts::PI * r * r,
        Shape::Rect { w, h } if w == h => w * w, // guard
        Shape::Rect { w, h } => w * h,
        Shape::Point => 0.0,
    }
}
fn describe(n: i64) -> &'static str {
    match n {
        i64::MIN..=-1 => "negative",
        0 => "zero",
        1 | 2 | 3 => "small",
        x if x % 2 == 0 => "big even",
        _ => "big odd",
    }
}
fn handle(m: &Msg) -> String {
    match m {
        Msg::Quit => "quit".into(),
        Msg::Move { x: 0, y } => format!("vertical to {y}"),
        Msg::Move { x, y: 0 } => format!("horizontal to {x}"),
        Msg::Move { x, y } => format!("to {x},{y}"),
        Msg::Text(t) if t.is_empty() => "empty text".into(),
        Msg::Text(t) => format!("text {}", t.len()),
        Msg::Pair(a, b @ 10..=20) => format!("{a} and mid {b}"), // @ binds while testing a range
        Msg::Pair(a, b) => format!("{a} and {b}"),
    }
}
fn slice_pat(xs: &[i32]) -> String {
    match xs {
        [] => "empty".into(),
        [one] => format!("one: {one}"),
        [first, .., last] if first == last => format!("bookends {first}"),
        [first, rest @ ..] => format!("first {first}, then {} more", rest.len()),
    }
}
// let-else: bind or bail out early; keeps the happy path unindented
fn port(s: &str) -> Result<u16, String> {
    let Some((_, p)) = s.rsplit_once(':') else {
        return Err(format!("no port in `{s}`"));
    };
    let Ok(n) = p.parse::<u16>() else {
        return Err(format!("bad port `{p}`"));
    };
    Ok(n)
}

fn main() {
    for s in [
        Shape::Circle { r: 1.0 },
        Shape::Rect { w: 2.0, h: 2.0 },
        Shape::Rect { w: 2.0, h: 3.0 },
        Shape::Point,
    ] {
        println!("{:.2}", area(&s));
    }
    println!("{:?}", [-5, 0, 2, 10, 11].map(describe));
    for m in [
        Msg::Quit,
        Msg::Move { x: 0, y: 5 },
        Msg::Move { x: 3, y: 0 },
        Msg::Move { x: 1, y: 2 },
        Msg::Text(String::new()),
        Msg::Text("abc".into()),
        Msg::Pair(1, 15),
        Msg::Pair(1, 99),
    ] {
        println!("{}", handle(&m));
    }
    println!(
        "{:?}",
        [&[][..], &[7], &[4, 5, 4], &[1, 2, 3]].map(slice_pat)
    );
    println!(
        "{:?} {:?} {:?}",
        port("host:8080"),
        port("host"),
        port("h:xx")
    );

    // tuples, nesting, matches!, while-let, if-let chains of Option/Result
    let pair = (3, "x");
    if let (n @ 1..=5, "x") = pair {
        println!("matched {n}");
    }
    let nested: Option<Result<i32, String>> = Some(Ok(4));
    match nested {
        Some(Ok(n)) if n > 3 => println!("big ok {n}"),
        Some(Ok(_)) => println!("ok"),
        Some(Err(e)) => println!("err {e}"),
        None => println!("none"),
    }
    println!("{}", matches!(Msg::Quit, Msg::Quit | Msg::Text(_)));
    let mut stack = vec![1, 2, 3];
    while let Some(top) = stack.pop() {
        print!("{top} ");
    }
    println!();
    // ref patterns: borrow inside a pattern instead of moving
    let name = Some(String::from("owned"));
    if let Some(ref n) = name {
        println!("{n}");
    }
    println!("{name:?} still usable");
}
