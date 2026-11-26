//! `cargo run`: the sound shelf under the three kinds of hostile caller. Only safe code, only the sound version.

use shelf::script::Liar;
use shelf::Shelf;
use std::cell::Cell;
use std::panic::{self, AssertUnwindSafe};

fn contents(shelf: &Shelf<String, 4>) -> Vec<&String> {
    shelf.iter().collect()
}

fn main() {
    println!("1. an iterator that claims len() == 0 and yields 7 items");
    let shelf = Shelf::<String, 4>::new();
    let result = shelf.extend_exact(Liar { claimed: 0, inner: (0..7).map(|i| i.to_string()) });
    println!("   extend_exact -> {result:?}, contents {:?}", contents(&shelf));

    println!("2. a closure that pushes to the shelf that is calling it");
    let shelf = Shelf::<String, 4>::new();
    let kept: Cell<Option<&String>> = Cell::new(None);
    shelf.fill_with(1, |_| {
        kept.set(shelf.push("from inside".into()).ok());
        "from outside".into()
    });
    println!("   kept reference still reads {:?}, contents {:?}", kept.get(), contents(&shelf));

    println!("3. a closure that panics on its third call (the panic message below is expected)");
    let shelf = Shelf::<String, 4>::new();
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        shelf.fill_with(4, |k| {
            assert!(k < 2, "third call");
            format!("v{k}")
        })
    }));
    println!("   unwound: {}, contents {:?}", result.is_err(), contents(&shelf));
}
