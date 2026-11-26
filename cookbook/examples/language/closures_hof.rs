//! RECIPE 39 (language): closures and higher-order functions: Fn / FnMut / FnOnce, returning closures, callbacks.
//! LEVEL: simple
//! Use in: task handlers, retry wrappers, middleware, event callbacks, sort keys.   Run: cargo run --example closures_hof
//! COMMON USES: callbacks, middleware, retry wrappers, sort keys, event handlers, lazy defaults.
//!
//! A closure captures variables from its environment. The compiler picks the weakest trait that fits:
//!   Fn      only reads captured data     -> can be called many times, even concurrently
//!   FnMut   mutates captured data        -> needs `mut`, many times, not concurrently
//!   FnOnce  consumes captured data       -> can be called once
//! `move` forces capture by value (needed when a closure outlives its scope, e.g. `thread::spawn`).
//! Read: https://doc.rust-lang.org/book/ch13-01-closures.html
#![allow(dead_code)]
use std::collections::HashMap;

fn apply_twice(f: impl Fn(i32) -> i32, x: i32) -> i32 {
    f(f(x))
}
fn call_n(mut f: impl FnMut(), n: usize) {
    for _ in 0..n {
        f()
    }
}
fn consume(f: impl FnOnce() -> String) -> String {
    f()
}

// returning a closure: `impl Fn` (one concrete type) ...
fn make_adder(n: i32) -> impl Fn(i32) -> i32 {
    move |x| x + n
}
// ... or `Box<dyn Fn>` when different branches return different closures
fn pick(op: &str) -> Box<dyn Fn(i32, i32) -> i32> {
    match op {
        "add" => Box::new(|a, b| a + b),
        "mul" => Box::new(|a, b| a * b),
        _ => Box::new(|_, _| 0),
    }
}
// compose two functions into a new one
fn compose<A, B, C>(f: impl Fn(A) -> B, g: impl Fn(B) -> C) -> impl Fn(A) -> C {
    move |x| g(f(x))
}

// A registry of callbacks keyed by name: the shape of a handler table / plugin system.
struct Registry {
    handlers: HashMap<String, Box<dyn Fn(&str) -> String>>,
}
impl Registry {
    fn on(&mut self, name: &str, h: impl Fn(&str) -> String + 'static) {
        self.handlers.insert(name.to_string(), Box::new(h));
    }
    fn fire(&self, name: &str, arg: &str) -> Option<String> {
        self.handlers.get(name).map(|h| h(arg))
    }
}

fn double(x: i32) -> i32 {
    x * 2
}

fn main() {
    println!("{}", apply_twice(|x| x + 3, 10));
    let mut count = 0;
    call_n(|| count += 1, 5); // FnMut mutates `count`
    println!("count = {count}");
    let owned = String::from("moved in");
    println!("{}", consume(move || owned + "!")); // FnOnce consumes `owned`

    let add5 = make_adder(5);
    println!("{} {} {}", add5(1), pick("mul")(3, 4), pick("zzz")(3, 4));
    let len_then_double = compose(|s: &str| s.len() as i32, double);
    println!("{}", len_then_double("four"));

    // function items and method paths are values too
    let fp: fn(i32) -> i32 = double;
    println!("{:?}", [1, 2, 3].map(fp));
    println!(
        "{:?}",
        ["a", "bb"].iter().map(|s| s.len()).collect::<Vec<_>>()
    );
    println!("{:?}", ["a", "bb"].map(str::to_uppercase));

    let mut reg = Registry {
        handlers: HashMap::new(),
    };
    let prefix = String::from(">> ");
    reg.on("greet", move |n| format!("{prefix}hello {n}"));
    reg.on("len", |n| n.len().to_string());
    println!(
        "{:?} {:?} {:?}",
        reg.fire("greet", "ana"),
        reg.fire("len", "ana"),
        reg.fire("nope", "")
    );

    // closures as sort keys / predicates / defaults
    let mut words = vec!["pear", "fig", "banana"];
    words.sort_by_key(|w| std::cmp::Reverse(w.len()));
    println!(
        "{words:?} {}",
        None::<i32>.map_or_else(|| "lazy default".to_string(), |v| v.to_string())
    );
}
