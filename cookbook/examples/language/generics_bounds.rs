//! RECIPE 33 (language): generics, trait bounds, `impl Trait`, associated types, const generics, blanket impls.
//! LEVEL: intermediate
//! Use in: reusable engine code (`Scheduler<S: Store>`, `fn run(tasks: impl Iterator<..>)`).   Run: cargo run --example generics_bounds
//! COMMON USES: reusable library code: generic containers, trait-bounded algorithms, extension methods on foreign types.
//!
//! Generics are monomorphised: the compiler stamps out one specialised copy per concrete type (fast, bigger binary).
//! Read: Rust Book ch. 10 https://doc.rust-lang.org/book/ch10-00-generics.html and ch. 17/19 on advanced traits.
#![allow(dead_code)]
use std::collections::HashMap;
use std::fmt::Display;

// bound with `T: A + B`, and the same thing with a `where` clause (use it when bounds get long)
fn largest<T: PartialOrd + Copy>(xs: &[T]) -> Option<T> {
    let mut it = xs.iter().copied();
    let first = it.next()?;
    Some(it.fold(first, |m, x| if x > m { x } else { m }))
}
fn show_all<I, T>(items: I) -> String
where
    I: IntoIterator<Item = T>,
    T: Display,
{
    items
        .into_iter()
        .map(|t| t.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

// `impl Trait` in argument position = anonymous generic; in return position = "some iterator, I won't name it"
fn evens_up_to(n: u32) -> impl Iterator<Item = u32> {
    (0..=n).filter(|x| x % 2 == 0)
}
fn total(xs: impl Iterator<Item = u32>) -> u32 {
    xs.sum()
}

// ASSOCIATED TYPE: one implementation fixes the type (vs a generic param where one type could impl many times)
trait Store {
    type Key;
    fn get(&self, k: &Self::Key) -> Option<&String>;
}
struct ByName(HashMap<String, String>);
struct ByIndex(Vec<String>);
impl Store for ByName {
    type Key = String;
    fn get(&self, k: &String) -> Option<&String> {
        self.0.get(k)
    }
}
impl Store for ByIndex {
    type Key = usize;
    fn get(&self, k: &usize) -> Option<&String> {
        self.0.get(*k)
    }
}
fn lookup<S: Store>(s: &S, k: S::Key) -> Option<&String> {
    s.get(&k)
}

// a generic struct, with an extra method only available when T: Display
struct Stack<T>(Vec<T>);
impl<T> Stack<T> {
    fn push(&mut self, t: T) {
        self.0.push(t)
    }
}
impl<T: Display> Stack<T> {
    fn render(&self) -> String {
        show_all(self.0.iter())
    }
}

// CONST GENERICS: array length is part of the type
fn sum_array<const N: usize>(a: [i32; N]) -> i32 {
    a.iter().sum()
}

// BLANKET IMPL: give every Display type a new method
trait Shout {
    fn shout(&self) -> String;
}
impl<T: Display> Shout for T {
    fn shout(&self) -> String {
        self.to_string().to_uppercase() + "!"
    }
}

fn main() {
    println!("{:?} {:?}", largest(&[3, 9, 2]), largest::<f64>(&[])); // turbofish ::<f64> picks T
    println!("{}", show_all(vec![1, 2, 3]));
    println!("{}", total(evens_up_to(10)));
    let by_name = ByName(HashMap::from([("a".to_string(), "alpha".to_string())]));
    println!(
        "{:?} {:?}",
        lookup(&by_name, "a".to_string()),
        lookup(&ByIndex(vec!["x".into()]), 0)
    );
    let mut s = Stack(vec![]);
    s.push(1.5);
    s.push(2.5);
    println!("{}", s.render());
    println!("{} {}", sum_array([1, 2, 3]), sum_array([10; 5]));
    println!("{} {}", "hi".shout(), 42.shout());
}
