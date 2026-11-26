//! RECIPE 11 (traits): trait with generics (static dispatch) vs `dyn` (dynamic dispatch), and fakes for tests.
//! LEVEL: intermediate
//! Use in: executor stages (a `TaskRunner` trait you can fake).   Run: cargo run --example dyn_vs_generic
//! COMMON USES: swappable backends and test fakes: storage, clocks, mailers; generics in hot paths, dyn at plugin boundaries.
//!
//! Pattern: define behaviour as a trait. Use a generic `<R: Runner>` when the type is known at compile
//! time (fast, inlinable, one copy of code per type). Use `Box<dyn Runner>` when you need a heterogeneous
//! collection or runtime choice (one vtable call, smaller binary). A fake implementation makes tests easy.
#![allow(dead_code)]
trait Runner {
    fn run(&self, name: &str) -> String;
}

struct Echo;
struct Shout;
struct Fake {
    answer: &'static str,
}

impl Runner for Echo {
    fn run(&self, name: &str) -> String {
        name.to_string()
    }
}
impl Runner for Shout {
    fn run(&self, name: &str) -> String {
        name.to_uppercase()
    }
}
impl Runner for Fake {
    fn run(&self, _: &str) -> String {
        self.answer.to_string()
    }
}

// static dispatch: the compiler generates one copy per concrete R
fn run_generic<R: Runner>(r: &R, names: &[&str]) -> Vec<String> {
    names.iter().map(|n| r.run(n)).collect()
}

// dynamic dispatch: pick behaviours at runtime, store them together
fn run_dyn(runners: &[Box<dyn Runner>], name: &str) -> Vec<String> {
    runners.iter().map(|r| r.run(name)).collect()
}

fn main() {
    println!("{:?}", run_generic(&Shout, &["a", "b"]));
    let mixed: Vec<Box<dyn Runner>> = vec![
        Box::new(Echo),
        Box::new(Shout),
        Box::new(Fake { answer: "fixed" }),
    ];
    println!("{:?}", run_dyn(&mixed, "task"));
    println!(
        "object safety: a trait with generic methods or `Self` returns cannot be `dyn`; keep traits small."
    );
}
