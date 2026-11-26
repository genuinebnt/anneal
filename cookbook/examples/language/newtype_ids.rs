//! RECIPE 03 (types): newtype ids so the compiler stops you mixing things up.
//! LEVEL: simple
//! Use in: stages 3 and 5.   Run: cargo run --example newtype_ids
//! COMMON USES: database ids, API ids, units: stop mixing a UserId with an OrderId at compile time at zero runtime cost.
//!
//! Pattern: `struct StopId(u32)` costs nothing at runtime but a `LineId` can no longer be passed where a
//! `StopId` is expected. Typed indices into a `Vec` are how Rust graphs usually work.
//! Real code: petgraph's `NodeIndex` -> https://docs.rs/petgraph/latest/petgraph/graph/struct.NodeIndex.html
//!            API Guidelines C-NEWTYPE -> https://rust-lang.github.io/api-guidelines/type-safety.html
#![allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct StopId(u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct LineId(u32);

struct Names(Vec<String>);

impl Names {
    fn get(&self, id: StopId) -> &str {
        &self.0[id.0 as usize]
    }
}

fn main() {
    let names = Names(vec!["central".into(), "airport".into()]);
    let stop = StopId(1);
    let line = LineId(1);
    println!("stop {:?} is {}", stop, names.get(stop));
    println!("line {:?} exists but cannot index Names", line);
    // names.get(line); // <- does not compile: expected `StopId`, found `LineId`. Try uncommenting.
    println!(
        "size_of::<StopId>() = {} (same as u32)",
        std::mem::size_of::<StopId>()
    );
}
