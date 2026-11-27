---
title: Optimistic latching: assume, do it cheaply, check, and fall back
summary: The pattern behind the fast path of a concurrent index: try the cheap version assuming nothing structural will change, detect when the assumption was false, and redo the work the safe way; with a seqlock and a version-checked retry as tested examples.
minutes: 9
---
Take an insert into a 681-way leaf. Almost always the leaf has room: no split, no parent change, nothing above the leaf needs to be written. A pessimistic writer pays for the rare case on every operation by exclusively latching the whole path. An **optimistic** writer bets on the common case: it takes the cheap latches (shared on the way down, exclusive only on the leaf), does the work, and only if the bet fails does it give everything back and redo the operation the slow way.

The pattern has three parts every time:

1. **Assume** something about the common case (the leaf has room).
2. **Do** the work with the cheapest synchronisation that is correct *if the assumption holds*.
3. **Check** the assumption at the point where it can be checked safely, and **fall back** to the pessimistic path if it is false. Nothing may have changed in the meantime that the fallback cannot cope with.

```svg
caption: The optimistic insert. Most operations take the left path: shared latches down, one exclusive latch on the leaf, done. The rare ones (the leaf would split) release everything and take the right path, exactly the pessimistic insert of the earlier stages.
<svg viewBox="0 0 760 240" role="img" aria-label="A flow chart: read-latch down, write-latch the leaf, then either insert (fast path) or release and retry pessimistically">
<defs><marker id="ol-a" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--dim)"/></marker></defs>
<rect class="box" x="20" y="90" width="150" height="46" rx="4"/><text class="mid fg sm" x="95" y="110">read-latch down</text><text class="mid dim sm" x="95" y="126">(crab to the leaf)</text>
<rect class="box" x="210" y="90" width="150" height="46" rx="4"/><text class="mid fg sm" x="285" y="110">write-latch the leaf</text><text class="mid dim sm" x="285" y="126">(parent still held)</text>
<path class="ln" d="M170 113 H208" marker-end="url(#ol-a)"/>
<rect class="blue" x="400" y="90" width="130" height="46" rx="4"/><text class="mid t-b sm" x="465" y="110">leaf has room?</text>
<path class="ln" d="M360 113 H398" marker-end="url(#ol-a)"/>
<rect class="live" x="590" y="30" width="150" height="46" rx="4"/><text class="mid fg sm" x="665" y="50">insert, release</text><text class="mid t-g sm" x="665" y="66">fast path · ≈99%</text>
<rect class="hot" x="590" y="150" width="150" height="46" rx="4"/><text class="mid fg sm" x="665" y="170">release all, redo</text><text class="mid t-a sm" x="665" y="186">pessimistic path</text>
<path class="ln-g" d="M530 100 L588 62" marker-end="url(#ol-a)"/><text class="t-g sm" x="545" y="76">yes</text>
<path class="ln" d="M530 128 L588 168" marker-end="url(#ol-a)"/><text class="t-a sm" x="545" y="160">no</text>
</svg>
```

## Why it is correct

The fast path is correct only if the assumption cannot be invalidated between the check and the action. In the index the bet is protected by a latch: the leaf's **write** latch makes the "room?" check and the insert one atomic step, and the **parent's read latch**, held while the leaf is re-latched for writing, makes sure nobody splits or merges that leaf in between (a restructure needs the parent exclusively). Two requirements follow:

- the optimistic path must make **no change** before it knows the assumption holds (or it must be able to undo it), or the fallback would start from a half-done state;
- a *complete* answer is complete in both paths: a duplicate key or a missing key finishes the operation immediately, not by falling back.

## Optimism without locks: version numbers

The same idea works with no lock at all for readers. Give the data a **version** that a writer increments to an odd number before changing it and to the next even number afterwards. A reader copies the data, then checks the version: if it is odd, or it changed since the reader started, the copy may be torn, so retry. This is a **seqlock**. Readers never block writers and never write to shared memory (so they scale), at the price of retrying under write contention. Modern in-memory B-trees put a version in every node and traverse with it ("optimistic lock coupling").

| | pessimistic latching | optimistic |
|---|---|---|
| readers write shared memory? | yes (the latch count) | no, with versions |
| writers on different leaves | wait if they share an exclusive ancestor | run in parallel |
| cost when the assumption fails | none | the wasted fast attempt, then the slow path |
| where it pays | write-heavy, small trees | large trees, mostly non-structural writes |

## C++ comparison

| C / C++ | Rust |
|---|---|
| `std::atomic<uint64_t> version` with `memory_order_acquire/release` | `AtomicU64` with `Ordering::Acquire` / `Release` |
| a seqlock written with `memcpy` of a plain struct (a data race the memory model forbids unless the type is atomic) | keep the data in atomics, or use the `seqlock` crate; plain non-atomic reads of data being written are a data race in Rust too |
| `try_lock` / `try_lock_shared` | `RwLock::try_read` / `try_write` |

## In real code

### Using it: assume, check, fall back, and version-checked retries

```rust test
use std::sync::Mutex;

/// Runs the optimistic attempt; if it declines (None), runs the pessimistic one. Counts which was taken.
struct Stats { fast: usize, slow: usize }

fn optimistic_then_pessimistic<T>(stats: &mut Stats, optimistic: impl FnOnce() -> Option<T>, pessimistic: impl FnOnce() -> T) -> T {
    match optimistic() {
        Some(done) => { stats.fast += 1; done }
        None => { stats.slow += 1; pessimistic() }
    }
}

/// A leaf of `cap` slots. The fast path inserts only if it stays below `cap`; the slow path "splits" (a stand-in for the full algorithm).
struct Leaf { keys: Mutex<Vec<u32>>, cap: usize }

impl Leaf {
    fn insert_fast(&self, key: u32) -> Option<bool> {
        let mut keys = self.keys.lock().unwrap();
        if keys.contains(&key) { return Some(false); }            // a complete answer: no fallback for a duplicate
        if keys.len() + 1 < self.cap { keys.push(key); return Some(true); }
        None                                                       // nothing was changed: the fallback starts clean
    }
    fn insert_slow(&self, key: u32) -> bool {
        let mut keys = self.keys.lock().unwrap();
        keys.push(key);
        let half = keys.len() / 2;
        keys.drain(..half);                                        // "split": drop the lower half (the real code moves it to a new leaf)
        true
    }
}

#[test]
fn most_inserts_take_the_fast_path_and_the_fallback_starts_from_a_clean_state() {
    let leaf = Leaf { keys: Mutex::new(vec![]), cap: 8 };
    let mut stats = Stats { fast: 0, slow: 0 };
    for key in 0..100 {
        optimistic_then_pessimistic(&mut stats, || leaf.insert_fast(key), || leaf.insert_slow(key));
    }
    assert!(stats.fast > 3 * stats.slow, "the common case is fast: {} fast, {} slow", stats.fast, stats.slow);
    assert_eq!(stats.fast + stats.slow, 100);
    assert!(leaf.keys.lock().unwrap().len() < leaf.cap, "the leaf never holds `cap` keys at rest");
    let mut dup_stats = Stats { fast: 0, slow: 0 };
    let again = optimistic_then_pessimistic(&mut dup_stats, || leaf.insert_fast(99), || leaf.insert_slow(99));
    assert!(!again && dup_stats.slow == 0, "a duplicate is a finished answer on the fast path");
}

#[test]
fn check_a_version_and_retry_if_it_moved() {
    use std::sync::atomic::{AtomicU64, Ordering};
    // Optimistic concurrency control: read a value and its version, compute outside the lock, then apply only if the version is unchanged.
    struct Cell { value: Mutex<i64>, version: AtomicU64 }
    impl Cell {
        fn read(&self) -> (i64, u64) { (*self.value.lock().unwrap(), self.version.load(Ordering::Acquire)) }
        fn try_apply(&self, seen_version: u64, new_value: i64) -> bool {
            let mut v = self.value.lock().unwrap();
            if self.version.load(Ordering::Acquire) != seen_version { return false; }     // someone changed it since we looked: redo
            *v = new_value;
            self.version.fetch_add(1, Ordering::Release);
            true
        }
    }
    let cell = Cell { value: Mutex::new(10), version: AtomicU64::new(0) };
    let (seen, version) = cell.read();
    // an interfering writer gets in between our read and our apply
    assert!(cell.try_apply(version, 50));
    assert!(!cell.try_apply(version, seen + 1), "our version is stale: the apply is refused");
    let (seen, version) = cell.read();
    assert!(cell.try_apply(version, seen + 1), "retried with a fresh read: succeeds");
    assert_eq!(*cell.value.lock().unwrap(), 51);
}
```

```rust test
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

/// A seqlock holding two numbers that must always be seen together (a, a + 1). The data is in atomics, so reading it while it is
/// being written is not a data race; the version tells the reader whether the pair it assembled is consistent.
struct SeqPair { version: AtomicU64, a: AtomicU64, b: AtomicU64 }

impl SeqPair {
    fn write(&self, a: u64) {                                  // a single writer at a time (here: one writer thread)
        let v = self.version.load(Ordering::Relaxed);
        self.version.store(v + 1, Ordering::Relaxed);          // odd: a write is in progress
        std::sync::atomic::fence(Ordering::Release);           // ...and no data store below may be reordered before it
        self.a.store(a, Ordering::Relaxed);
        self.b.store(a + 1, Ordering::Relaxed);
        self.version.store(v + 2, Ordering::Release);          // even: done
    }
    fn read(&self) -> (u64, u64, usize) {
        let mut retries = 0;
        loop {
            let v1 = self.version.load(Ordering::Acquire);
            let (a, b) = (self.a.load(Ordering::Relaxed), self.b.load(Ordering::Relaxed));
            std::sync::atomic::fence(Ordering::Acquire);
            let v2 = self.version.load(Ordering::Relaxed);
            if v1 == v2 && v1 % 2 == 0 { return (a, b, retries); }      // no write overlapped: the pair is consistent
            retries += 1;
        }
    }
}

#[test]
fn readers_never_see_a_torn_pair_and_never_block_the_writer() {
    let cell = Arc::new(SeqPair { version: AtomicU64::new(0), a: AtomicU64::new(0), b: AtomicU64::new(1) });
    let writer = { let c = Arc::clone(&cell); thread::spawn(move || for i in 1..=20_000 { c.write(i * 10); }) };
    let readers: Vec<_> = (0..3).map(|_| {
        let c = Arc::clone(&cell);
        thread::spawn(move || {
            for _ in 0..20_000 {
                let (a, b, _) = c.read();
                assert_eq!(b, a + 1, "a torn read: a = {a}, b = {b}");
            }
        })
    }).collect();
    writer.join().unwrap();
    for r in readers { r.join().unwrap(); }
    assert_eq!(cell.read().0 % 10, 0);
}
```

### In the exercises

- **2e-02:** the fast paths of insert and remove read-latch down, re-latch the leaf for writing under the parent's read latch and check it again (a duplicate or a missing key is simply done); anything else falls back to the pessimistic path of 2e-01, unchanged.
- **2e-03:** a scan that holds no latch between calls checks each pair against the last key it returned: the same "act, then validate" idea applied to a position instead of a page.
- **Version-checked retry:** the second test is the validation step in "optimistic lock coupling"; the course's index gets the same safety from the parent's read latch instead of a version number.

### Where it is used

- **Linux's `seqlock_t`/`seqcount_t`** protect kernel timekeeping readers the way `SeqPair` protects a pair; the `seqlock` crate does it for Rust.
- **Optimistic lock coupling** (Leis et al., "The ART of Practical Synchronization", DaMoN 2016) in-memory B-trees and tries, and the buffer manager of **LeanStore**/**Umbra**, version-check instead of latching on reads.
- **Databases**: InnoDB's optimistic descent (`BTR_MODIFY_LEAF` before falling back to `BTR_MODIFY_TREE`), PostgreSQL's optimistic fast path for inserting into the rightmost leaf; **MVCC** and OCC transactions validate at commit that their reads are unchanged.
- **Concurrent data structures in general**: compare-and-swap loops (`AtomicU64::compare_exchange`) are optimism at the instruction level.
