//! RECIPE 36 (language): the borrow checker: 8 common errors and how experienced Rust avoids them.
//! LEVEL: intermediate
//! Use in: everywhere, especially structs with several fields and collections you mutate while reading.   Run: cargo run --example borrowck_patterns
//! COMMON USES: day-to-day Rust: mutate collections while reading, move out of &mut, avoid clone-everything.
//!
//! Core rule: at any moment you may have EITHER many `&T` OR exactly one `&mut T`, never both, to the same data.
//! The fixes are mostly about *structuring* code so the two uses do not overlap.
//! Read: Rust Book ch. 4 https://doc.rust-lang.org/book/ch04-00-understanding-ownership.html
#![allow(dead_code)]
use std::collections::HashMap;
use std::mem;

#[derive(Default, Debug)]
struct Engine {
    queue: Vec<u32>,
    total: u32,
    log: Vec<String>,
    current: Option<String>,
}

impl Engine {
    // 1. Borrow DIFFERENT FIELDS at once: the compiler tracks fields separately inside one method.
    fn drain_into_total(&mut self) {
        for x in &self.queue {
            self.total += x; // &self.queue and &mut self.total are disjoint: fine
        }
        // BUT calling a `&mut self` METHOD while `&self.queue` is borrowed would fail: methods borrow all of self.
        self.queue.clear();
    }

    // 2. Move a value OUT of a `&mut` with `take`/`replace` instead of fighting the checker.
    fn finish_current(&mut self) -> Option<String> {
        let done = self.current.take(); // leaves None behind
        if let Some(name) = &done {
            self.log.push(format!("finished {name}"));
        }
        done
    }
    fn swap_log(&mut self) -> Vec<String> {
        mem::take(&mut self.log) // leaves an empty Vec, returns the old one
    }
}

fn main() {
    let mut e = Engine {
        queue: vec![1, 2, 3],
        current: Some("a".into()),
        ..Default::default()
    };
    e.drain_into_total();
    println!("{:?}", e.finish_current());
    println!("{:?} {}", e.swap_log(), e.total);

    // 3. get-then-insert on a map: use the ENTRY API (one lookup, no overlapping borrows)
    let mut counts: HashMap<&str, u32> = HashMap::new();
    for w in ["a", "b", "a"] {
        *counts.entry(w).or_insert(0) += 1;
    }
    let mut sorted: Vec<_> = counts.iter().collect();
    sorted.sort();
    println!("{sorted:?}");

    // 4. Mutate two elements of one Vec: `split_at_mut` (or swap) instead of two `&mut v[i]`.
    let mut v = vec![1, 2, 3, 4];
    let (left, right) = v.split_at_mut(2);
    mem::swap(&mut left[0], &mut right[1]);
    println!("{v:?}");

    // 5. Remove while iterating: `retain` (never mutate the Vec you are looping over).
    let mut nums = vec![1, 2, 3, 4, 5, 6];
    nums.retain(|n| n % 2 == 0);
    println!("{nums:?}");

    // 6. Hold an index, not a reference, when you will mutate the collection later.
    let mut items = vec![10, 20, 30];
    let idx = items.iter().position(|&x| x == 20).unwrap(); // usize is Copy; no borrow remains
    items[idx] += 1;
    items.push(40);
    println!("{items:?}");

    // 7. Two-phase borrow: `v.push(v.len())` compiles because the &mut starts after the argument is evaluated.
    let mut w = vec![0usize];
    w.push(w.len());
    println!("{w:?}");

    // 8. Clone ON PURPOSE for cheap/small data when restructuring would hurt readability.
    let name = String::from("shared");
    let owned_for_closure = name.clone();
    let f = move || owned_for_closure.len();
    println!("{} {}", f(), name);

    // What fails (uncomment to read the errors):
    // let first = &v[0]; v.push(9); println!("{first}");   // E0502: cannot borrow `v` as mutable (first still used)
    // let s = String::from("x"); let t = s; println!("{s}"); // E0382: borrow of moved value
}
