---
title: Invariants: stating what must always be true, and checking it
summary: How to write a data structure's invariants down, turn them into a checker you can call from tests and debug builds, and what a violation looks like when you have not.
minutes: 8
---
Every stage from here on builds a structure with rules that must hold between operations: the free list and page table partition the slots, ARC's four lists stay within `c` and `2c`, a hash table's directory entries point at buckets whose local depth matches. Bugs in these structures are almost always **invariant violations**: one operation leaves the structure in a state no other operation expects.

## Write the invariants as a list first

For ARC, in words:

1. A frame is on exactly one of `mru`, `mfu`; a page is on at most one ghost list; no page is both live and a ghost.
2. `|mru| + |mru_ghost| <= c`, and `|mru| + |mfu| + |mru_ghost| + |mfu_ghost| <= 2c`.
3. Every handle in the maps names a node in the list the map says it is in.
4. `curr_size` equals the number of live evictable frames.

Writing them is half the work. Several of the bugs you will meet are visible the moment you try to put the rule into a sentence.

## Turn them into a function

```rust
#[cfg(debug_assertions)]
fn check(&self) {
    assert!(self.mru.len() + self.mru_ghost.len() <= self.replacer_size, "mru + mru_ghost exceeds c");
    assert!(self.mru.len() + self.mfu.len() + self.mru_ghost.len() + self.mfu_ghost.len() <= 2 * self.replacer_size, "four lists exceed 2c");
    for (frame, alive) in &self.alive {
        assert!(self.list_of(alive.status).get(alive.handle) == Some(frame), "the handle of frame {} is stale", frame.0);
    }
    let evictable = self.alive.values().filter(|a| a.evictable).count();
    assert_eq!(evictable, self.curr_size, "curr_size is out of step");
}
```

Call it at the end of every public method that changes state, under `#[cfg(debug_assertions)]` (or `debug_assert!` inside a helper) so release builds pay nothing. The assertion messages are the point: when a test fails you read *which* rule broke, and the stack shows the last operation, which is the one that broke it.

## Where the check runs

| place | what it buys |
|---|---|
| inside the structure, under `debug_assert!` | the exact operation that broke an invariant, in every test and every model run |
| a public `verify_integrity()` method | tests and the buffer pool can call it; the hash table module makes it part of the API (BusTub's `VerifyIntegrity`) |
| after each step of a model test | together with the comparison against the simple model |
| never in the hot path of release | a full check is O(n); keep it out of production |

## What a violation looks like without a checker

The failure appears *later and elsewhere*: `evict` returns a frame whose page is a ghost, `size()` disagrees with what `evict` can deliver, a handle used after `remove` returns `None` where the code expected `Some`. By then the state that caused it is gone. A checker moves the failure to the cause.

> [!TIP] Invariants, preconditions, postconditions
> A **precondition** is what the caller must guarantee (`remove` requires the frame to be evictable), a **postcondition** is what the method guarantees (the victim is no longer live), and an **invariant** is what holds between every pair of calls. `assert!` the first at the top of the method, `assert!` the second at the bottom, and `check()` the third.

## In real code

### Using it: an invariant as a `Result`, and a checker that catches a planted bug

Returning `Result<(), String>` instead of asserting inside makes the checker usable three ways: `assert!(x.verify().is_ok())` in tests, `debug_assert!` inside the structure, and a public `verify_integrity()` a test can call. This one is the extendible hash table's directory rule, the same one the 2b stages ask you to implement.

```rust test
/// Directory invariants of an extendible hash table:
///  1. the directory has exactly 2^global_depth entries,
///  2. every local depth is <= global depth,
///  3. all entries pointing at one bucket carry the same local depth,
///  4. a bucket of local depth d is pointed at by exactly 2^(global-d) entries.
fn verify_integrity(global_depth: u32, local_depths: &[u32], bucket_ids: &[u32]) -> Result<(), String> {
    use std::collections::HashMap;
    let size = 1usize << global_depth;
    if local_depths.len() != size || bucket_ids.len() != size {
        return Err(format!("directory has {} entries, expected 2^{} = {}", bucket_ids.len(), global_depth, size));
    }
    let mut seen: HashMap<u32, (u32, usize)> = HashMap::new();          // bucket -> (local depth, entries pointing at it)
    for i in 0..size {
        let (b, d) = (bucket_ids[i], local_depths[i]);
        if d > global_depth { return Err(format!("entry {i}: local depth {d} exceeds global depth {global_depth}")); }
        let e = seen.entry(b).or_insert((d, 0));
        if e.0 != d { return Err(format!("entry {i}: bucket {b} has local depth {d} here but {} elsewhere", e.0)); }
        e.1 += 1;
    }
    for (b, (d, count)) in seen {
        let want = 1usize << (global_depth - d);
        if count != want { return Err(format!("bucket {b} (local depth {d}) is pointed at by {count} entries, expected {want}")); }
    }
    Ok(())
}

#[test]
fn a_valid_directory_passes() {
    // global depth 2: buckets 10 and 11 hold two entries each (local depth 1); 12 and 13 one each (local depth 2)
    assert_eq!(verify_integrity(2, &[1, 1, 2, 2], &[10, 10, 12, 13]), Ok(()));
    assert_eq!(verify_integrity(0, &[0], &[10]), Ok(()));
}

#[test]
fn each_violation_names_the_rule_it_broke() {
    let e = |g, l: &[u32], b: &[u32]| verify_integrity(g, l, b).unwrap_err();
    assert!(e(2, &[0, 0, 0], &[1, 1, 1]).contains("expected 2^2"));                            // wrong size
    assert!(e(1, &[2, 2], &[1, 2]).contains("exceeds global depth"));                           // local > global
    assert!(e(2, &[1, 2, 1, 1], &[5, 5, 6, 6]).contains("elsewhere"));                          // one bucket, two depths
    assert!(e(2, &[2, 2, 1, 1], &[5, 5, 6, 6]).contains("pointed at by 2 entries, expected 1")); // depth says 1 entry, 2 point at it
}
```

```rust test
use std::collections::HashMap;

struct Slots { pages: HashMap<u32, usize>, free: Vec<usize>, num_slots: usize }

impl Slots {
    fn new() -> Self { Slots { pages: HashMap::new(), free: vec![], num_slots: 0 } }

    fn check(&self) {
        let mut seen = vec![false; self.num_slots];
        for &s in self.pages.values().chain(&self.free) {
            assert!(!seen[s], "slot {s} is live twice, or both live and free");
            seen[s] = true;
        }
        assert!(seen.iter().all(|&b| b), "a slot is neither live nor free: leaked");
    }

    fn insert(&mut self, page: u32) {
        let slot = self.free.pop().unwrap_or_else(|| { self.num_slots += 1; self.num_slots - 1 });
        self.pages.insert(page, slot);
        #[cfg(debug_assertions)] self.check();                       // after every mutation: the failure points at the cause
    }

    fn delete_buggy(&mut self, page: u32) {
        self.pages.remove(&page);                                    // forgot to push the slot onto the free list
        #[cfg(debug_assertions)] self.check();
    }
}

#[test]
fn a_correct_sequence_never_trips_the_check() {
    let mut s = Slots::new();
    for p in 0..10 { s.insert(p); }
    s.check();
}

#[test]
#[should_panic(expected = "leaked")]
fn the_checker_stops_at_the_operation_that_broke_the_rule() {
    let mut s = Slots::new();
    s.insert(1);
    s.delete_buggy(1);                                               // panics HERE with "leaked", not 10,000 operations later
}
```

### In the exercises

- **1c-01 and 1c-03:** a private `check()` for the list (every live node reachable from the front exactly once, `len` equal to the walk) and for the CLOCK ring (the hand in range) turns every test into a test of the structure's own rules as well.
- **1d-02, 1d-03:** "the set holds exactly the evictable frames, each under its current key" is the invariant to assert after every step of a random run.
- **1e-01, 1e-03:** ARC's bounds and the disjointness of the four lists, as in the ARC concept's `check`.
- **2a-02:** a page array's entries are sorted and `len <= capacity`; binary search is stated as a loop invariant.
- **2b-02 to 2b-04:** `verify_integrity` is written by you and called after every operation by the tests; the three rules of the first example (depths, slot counts, key placement) are the usual ones.

### Where it is used

- **Databases**: PostgreSQL's `amcheck` (`bt_index_check`) validates every B-tree invariant on a live index; SQLite has `PRAGMA integrity_check`; filesystems ship `fsck`. BusTub's own `VerifyIntegrity` and the B+ tree's `IsTreeValid` (module 2c) are the same idea.
- **Rust's ecosystem**: `debug_assert!` inside collections (`hashbrown`, `std`) and `#[cfg(debug_assertions)]` checkers in unsafe-heavy crates; `Result`-returning validators are the pattern behind `serde`'s and parsers' error positions as well.
- **Design by contract** (Eiffel, `contracts` crates): pre- and postconditions plus an invariant checked around every public method.
