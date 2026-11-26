//! RECIPE 48 (language): modules, visibility, re-exports, and enforcing invariants with private fields.
//! LEVEL: simple
//! Use in: structuring the engine as a library (src/lib.rs) plus a thin binary; hiding internals.   Run: cargo run --example modules_visibility
//! COMMON USES: structuring a crate into modules, hiding invariants behind private fields, public API surface design.
#![allow(dead_code)]
//!
//! Everything is private by default. `pub` exposes to the world, `pub(crate)` to the whole crate, `pub(super)` to the
//! parent module. Private FIELDS + a constructor that validates = a type that can never hold an invalid value.
//! Re-export with `pub use` so users import `engine::Workflow` instead of `engine::model::workflow::Workflow`.
//!
//! Real layout for a project (files, not inline):
//!   src/lib.rs        pub mod model; pub mod graph; pub use model::Workflow;
//!   src/model.rs      (or src/model/mod.rs)       src/graph.rs
//!   src/main.rs       thin: parse args, call the library, map errors to exit codes
//!   tests/            black-box tests of the public API only
//! Study how big crates organise: https://github.com/BurntSushi/ripgrep/tree/master/crates and
//! https://rust-lang.github.io/api-guidelines/ (naming, organisation)
mod engine {
    pub mod model {
        #[derive(Debug)]
        pub struct Percent(u8); // field is PRIVATE: only this module can build it

        #[derive(Debug, PartialEq)]
        pub struct OutOfRange(pub i32);

        impl Percent {
            pub fn new(v: i32) -> Result<Percent, OutOfRange> {
                if (0..=100).contains(&v) {
                    Ok(Percent(v as u8))
                } else {
                    Err(OutOfRange(v))
                }
            }
            pub fn get(&self) -> u8 {
                self.0
            }
        }
        pub(crate) fn helper_for_crate_only() -> &'static str {
            "crate-visible"
        }
        pub(super) fn helper_for_parent() -> &'static str {
            "parent-visible"
        }
        fn private_detail() {}
    }

    pub mod graph {
        use super::model::Percent; // paths: `super` = parent, `self`, `crate`
        pub fn half(p: &Percent) -> u8 {
            p.get() / 2
        }
    }

    #[allow(unused_imports)]
    pub use model::{OutOfRange, Percent}; // re-export: flat public API

    pub fn parent_can_call() -> &'static str {
        model::helper_for_parent()
    }
}

use engine::{Percent, graph};

fn main() {
    let p = Percent::new(80).unwrap();
    println!("{} {}", p.get(), graph::half(&p));
    println!("{:?}", Percent::new(101));
    println!(
        "{} {}",
        engine::parent_can_call(),
        engine::model::helper_for_crate_only()
    );
    // Percent(500);                         // error: tuple struct constructor is private
    // engine::model::private_detail();      // error: function is private
    // engine::model::helper_for_parent();   // error: only visible inside `engine`
}
