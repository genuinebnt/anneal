---
title: Testing concurrent code: stress tests, loom and miri
summary: Why a passing thread test proves little, what loom (every interleaving of a small model) and miri (undefined behaviour and data races in unsafe code) each check, and the bugs they would find in this course's own protocols.
minutes: 9
---
A concurrency test that passes ten thousand times can still hide a race that needs one rare interleaving. Three tools, each with a different job:

| tool | what it does | cost | finds |
|---|---|---|---|
| **stress test** (many threads, many iterations) | runs the real code under the OS scheduler | cheap | frequent races, deadlocks, lost updates that happen often |
| **loom** | replaces `std::sync`/`std::thread` with versions that **explore every interleaving** (up to a bound) of a small model | exponential in steps: keep models tiny | races, lost updates, ordering mistakes, deadlocks in the *protocol* |
| **miri** | an interpreter that runs your tests and **detects undefined behaviour**: out-of-bounds, use after free, invalid values, data races on non-atomic memory, with `-Zmiri-many-seeds` to vary thread scheduling | 10 to 100 times slower | UB in `unsafe`, data races in code that bypasses locks |

Miri needs the nightly toolchain: `rustup +nightly component add miri`, then `cargo +nightly miri test`. Loom is a crate: add `loom` under `[target.'cfg(loom)'.dependencies]` and run with `RUSTFLAGS="--cfg loom" cargo test --release`.

**Floats under miri.** Miri deliberately adds a tiny random error to results of floating-point functions such as `ln`, `exp` and `powf`, because different CPUs differ in the last bits. A test that compares such a result with `assert_eq!` can fail under miri and pass natively; that is the interpreter, not undefined behaviour. Either compare with a tolerance or run with `MIRIFLAGS="-Zmiri-no-extra-rounding-error"`. (The HyperLogLog tests of this course hit exactly this.)

## What loom needs

Loom explores your code only if the code uses *loom's* types, so you write the protocol against a small facade (`#[cfg(loom)] use loom::sync::{Arc, Mutex}; #[cfg(not(loom))] use std::sync::{Arc, Mutex};`) and wrap the test in `loom::model(|| { ... })`. Keep the model to two or three threads and a handful of operations; extract the part with the concurrency bug and model *that*.

## Four protocols from this course, modelled

Each was run under loom (the version that is wrong fails in some interleaving; the shipped version passes in all of them):

1. **Raising a HyperLogLog register** is read, compare, write. With the read and the write under *separate* locks, one thread can store 20 and the other then store 5 over it. The first draft of the Presto sketch had exactly this bug; stress tests did not show it; loom found it in the first model. Fix: hold the lock across the compare-and-write.
2. **A persistent store's writers** must serialise: without the write lock, two writers clone the same version and one update is lost.
3. **Commit publication order** (stamp the tuples, then publish the commit under the watermark's lock): publishing first lets a transaction that begins in between read a tuple that is not stamped yet.
4. **The write-write check** is only a check if it runs under the page latch together with the write: checked outside, both writers pass it.

## What miri finds and doesn't

Run on the primer and MVCC test suites (a few seeds each), the reference code is clean: it contains no `unsafe`, so there is no UB to find, and all sharing goes through `Mutex`, `RwLock` and atomics. That is the useful result: **miri is the safety net for the day someone adds `unsafe`** (a `Send` impl, a `transmute`, a raw pointer), and it also catches data races if a test bypasses the locks. It does not find logical races (the lost update above is perfectly defined behaviour): that is loom's job.

## C++ comparison

| C / C++ | Rust |
|---|---|
| ThreadSanitizer (`-fsanitize=thread`) | miri's data-race detection; `RUSTFLAGS=-Zsanitizer=thread` on nightly |
| AddressSanitizer / UBSan | miri (slower, but exact for Rust's rules) |
| model checkers (CDSChecker, Relacy) | loom |
| `rr`, `helgrind` | rarely needed for safe Rust |

**Port rule:** if the bug is a missing lock, a stress test may catch it; if it is a missing *ordering* (publish before stamp), model it with loom.

## In real code

### Using it: the lost-update pattern as a deterministic std test

Loom is not available in the course's std-only crate, so this test forces the bad interleaving by hand (a barrier between the read and the write) to show what loom would explore on its own:

```rust test
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

#[test]
fn separate_read_and_write_locks_lose_the_larger_value() {
    let reg = Arc::new(Mutex::new(0u32));
    let both_read = Arc::new(Barrier::new(2)); // both threads have read 0
    let big_wrote = Arc::new(Barrier::new(2)); // the 20 is stored before the 5 writes
    let hs: Vec<_> = [5u32, 20]
        .into_iter()
        .map(|v| {
            let (reg, both_read, big_wrote) = (reg.clone(), both_read.clone(), big_wrote.clone());
            thread::spawn(move || {
                let seen = *reg.lock().unwrap(); // the lock is released here
                both_read.wait();
                if v == 5 {
                    big_wrote.wait(); // the smaller writer goes last
                }
                if v > seen {
                    *reg.lock().unwrap() = v; // compare used the stale read
                }
                if v == 20 {
                    big_wrote.wait();
                }
            })
        })
        .collect();
    hs.into_iter().for_each(|h| h.join().unwrap());
    assert_eq!(*reg.lock().unwrap(), 5, "the 5 overwrote the 20: a lost update");
}

#[test]
fn holding_one_lock_across_the_compare_keeps_the_maximum() {
    let reg = Arc::new(Mutex::new(0u32));
    let hs: Vec<_> = [5u32, 20, 7, 13]
        .into_iter()
        .map(|v| {
            let reg = reg.clone();
            thread::spawn(move || {
                let mut r = reg.lock().unwrap();
                if v > *r {
                    *r = v;
                }
            })
        })
        .collect();
    hs.into_iter().for_each(|h| h.join().unwrap());
    assert_eq!(*reg.lock().unwrap(), 20);
}
```

### Using it: what a loom model looks like

This block is shown for reading; it needs the `loom` crate and `--cfg loom`, so it is not compiled by the course's snippet runner:

```rust
#[test]
fn hll_register_never_loses_the_larger_run() {
    loom::model(|| {
        let reg = loom::sync::Arc::new(loom::sync::Mutex::new(0u32));
        let hs: Vec<_> = [5u32, 20]
            .into_iter()
            .map(|v| {
                let r = reg.clone();
                loom::thread::spawn(move || {
                    let mut g = r.lock().unwrap();
                    if v > *g { *g = v; }
                })
            })
            .collect();
        hs.into_iter().for_each(|h| h.join().unwrap());
        assert_eq!(*reg.lock().unwrap(), 20); // checked in every interleaving
    });
}
```

### In the exercises

- **0d-03:** the Presto register update (hold both locks across the compare and the write).
- **0a-03:** why the store has a write lock.
- **4a-02:** the commit publication order.
- **4b-02:** the conflict check under the page latch.
- **2e-01 to 2e-04:** the concurrent B+ tree: tests that hold a page from the test thread to stop a writer in the middle, threaded workloads checked against models, and BusTub's concurrent tests.
- **0b-03 / 0c-04:** the concurrent stress tests; run them under `cargo +nightly miri test` with a few seeds to see them pass a stricter check.

### Where it is used

- **tokio**, **crossbeam**, **bytes** and **once_cell** test their lock-free pieces with loom; the standard library and many crates run their `unsafe` tests under miri in CI.
- The `rust-skills` concurrency guide asks the same question: does the value need to cross threads, and is the lock scope as small as it can be and no smaller?
