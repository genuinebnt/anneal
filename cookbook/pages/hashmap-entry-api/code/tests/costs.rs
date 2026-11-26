//! The two cost claims, measured by counting (no timing).

use hashmap_entry_api::{bump, bump_with_entry};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::HashMap;
use std::hash::{BuildHasher, DefaultHasher};
use std::hint::black_box;
use std::rc::Rc;

struct Counting;
thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
// SAFETY: every call is forwarded unchanged to `System`; we only add a counter.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.with(|n| n.set(n.get() + 1));
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

fn allocations(f: impl FnOnce()) -> usize {
    let before = ALLOCATIONS.with(Cell::get);
    f();
    ALLOCATIONS.with(Cell::get) - before
}

#[test]
fn entry_with_a_borrowed_str_allocates_even_on_a_hit() {
    let mut m = HashMap::from([("word".to_string(), 1)]);
    let with_entry = allocations(|| {
        for _ in 0..100 {
            bump_with_entry(black_box(&mut m), black_box("word"));
        }
    });
    let with_get_mut = allocations(|| {
        for _ in 0..100 {
            bump(black_box(&mut m), black_box("word"));
        }
    });
    assert_eq!((with_entry, with_get_mut), (100, 0));
    assert_eq!(m["word"], 201);
}

/// Counts how many times the map starts a hash.
#[derive(Clone, Default)]
struct CountHashes(Rc<Cell<usize>>);
impl BuildHasher for CountHashes {
    type Hasher = DefaultHasher;
    fn build_hasher(&self) -> DefaultHasher {
        self.0.set(self.0.get() + 1);
        DefaultHasher::new()
    }
}

/// Hashes used by `op` on a map that has spare room and one other key.
/// The other key matters: an empty map can answer a look-up without hashing.
fn hashes(op: impl FnOnce(&mut HashMap<u32, u32, CountHashes>)) -> usize {
    let counter = CountHashes::default();
    let mut m = HashMap::with_capacity_and_hasher(64, counter.clone());
    m.insert(1, 1);
    counter.0.set(0);
    op(&mut m);
    counter.0.get()
}

#[test]
#[allow(clippy::map_entry)] // the longer form is what we are measuring
fn entry_hashes_once_where_check_then_insert_hashes_more() {
    let entry_miss = hashes(|m| *m.entry(2).or_insert(0) += 1);
    let entry_hit = hashes(|m| *m.entry(1).or_insert(0) += 1);
    let three_calls = hashes(|m| {
        if !m.contains_key(&2) {
            m.insert(2, 0);
        }
        *m.get_mut(&2).unwrap() += 1;
    });
    let get_mut_hit = hashes(|m| *m.get_mut(&1).unwrap() += 1);
    let get_mut_miss = hashes(|m| {
        if m.get_mut(&2).is_none() {
            m.insert(2, 1);
        }
    });
    assert_eq!(
        (
            entry_miss,
            entry_hit,
            three_calls,
            get_mut_hit,
            get_mut_miss
        ),
        (1, 1, 3, 1, 2)
    );
}
