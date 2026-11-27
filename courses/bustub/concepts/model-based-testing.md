---
title: Model-based testing: checking a clever structure against a simple one
summary: Compare your fast data structure with an obviously correct, slow one over thousands of random operations from a fixed seed, and shrink a failure to the step that caused it.
minutes: 8
---
You can test an LRU-K replacer with a dozen hand-written cases, and they will all pass while a bug waits in the thousandth operation. The reliable method for data structures is **model-based testing** (a form of property testing): run the same random operations on the real structure and on a *model*, and compare after every step.

## The model is the slow, obvious version

A model should be so simple that you can see it is right:

```rust
/// A deliberately naive LRU-K: the same rules, one linear scan per eviction.
struct NaiveLruK { k: usize, now: usize, frames: HashMap<usize, (Vec<usize>, bool)> }

fn evict(&mut self) -> Option<usize> {
    let k = self.k;
    let victim = self.frames.iter()
        .filter(|(_, (_, evictable))| *evictable)
        .min_by_key(|(&frame, (history, _))| if history.len() < k { (0, history[0], frame) } else { (1, history[0], frame) })
        .map(|(&frame, _)| frame)?;
    self.frames.remove(&victim);
    Some(victim)
}
```

It is O(n) per eviction, which does not matter: the model only ever sees a few thousand operations. What matters is that its rules are the *specification*, written the dullest possible way.

## The loop

```rust
let mut rng = Lcg(1000 + k as u64);                  // a fixed seed: the run is reproducible
for step in 0..4000 {
    let frame = (rng.next() % 48) as usize;
    match rng.next() % 10 {
        0..=4 => { fast.record_access(f(frame)); slow.record(frame); }
        5..=7 => { let on = rng.next() % 2 == 0; fast.set_evictable(f(frame), on); /* mirror on the model */ }
        _     => assert_eq!(fast.evict().map(|x| x.0), slow.evict(), "k={k}, step {step}: the two disagree about the victim"),
    }
    assert_eq!(fast.size(), slow.size(), "k={k}, step {step}: the evictable counts differ");
}
```

Four habits make this useful rather than a lottery:

- **A fixed seed, and a message that names it.** A failure prints `k=3, step 2711`, and the same seed reproduces it exactly. A random test you cannot replay is noise.
- **Compare after every step**, not at the end. The first disagreement is the bug; by the end the state has diverged beyond reading.
- **A weighted operation mix.** If evictions are rare the structure never gets deep; if they are common it stays empty. Bias towards the operations that build state (`record_access`) and keep a steady share of the destructive ones.
- **Small domains.** 48 frames and 4000 steps hit the same frame hundreds of times, which is where state-dependent bugs live. A domain of a million never revisits anything.

## A tiny random generator, no crate needed

```rust
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}
```

A linear congruential generator is statistically weak and perfect for this: deterministic, ten lines, no dependency. (The low bits of an LCG are the least random, which is why the code shifts them away.)

## Finding the smallest failing case

When the models disagree, the output is a seed and a step number. To *understand* it: print the operations leading up to the step, then **shrink**: delete operations from the front one by one and re-run until removing any more makes the failure vanish. What remains is usually five or six lines and names the bug. Crates such as `proptest` automate this; doing it by hand once teaches what they do.

## Where this course uses it

| stage | model |
|---|---|
| LRU-K, O(log n) evict | the naive scan above |
| IndexList and the replacers | a `Vec` / `VecDeque` |
| the hash table (module 2b) | a `HashMap` |
| the B+ tree (module 2c) | a `BTreeMap` (the one in `std` *is* a B-tree) |

> [!TIP] The model is also documentation
> If you cannot write the model in ten lines, you do not yet understand the specification. Writing it first, before the clever version, is a good way to find that out.

## In real code

### Using it: a hand-written ring buffer against a `Vec`, and shrinking a failure

A ring buffer is simple to describe and easy to get subtly wrong at the wrap-around: exactly the kind of structure this method is for. The model is a `Vec` with a capacity check. `run` compares both after every operation and reports the first step that differs; `shrink` deletes operations until the failure is as small as it gets.

```rust test
#[derive(Clone, Copy, Debug)]
enum Op { Push(u32), Pop }

struct Ring { buf: Vec<Option<u32>>, head: usize, len: usize, bug: bool }

impl Ring {
    fn new(cap: usize, bug: bool) -> Self { Ring { buf: vec![None; cap], head: 0, len: 0, bug } }
    fn push(&mut self, v: u32) -> bool {
        if self.len == self.buf.len() { return false; }
        let at = (self.head + self.len) % self.buf.len();
        self.buf[at] = Some(v);
        self.len += 1;
        true
    }
    fn pop(&mut self) -> Option<u32> {
        if self.len == 0 { return None; }
        let v = self.buf[self.head].take();
        let wrapped = self.head + 1 == self.buf.len();
        self.head = (self.head + 1) % self.buf.len();
        if !(self.bug && wrapped) { self.len -= 1; }                  // the planted bug: forgets to count the pop that wraps
        v
    }
}

struct Model { items: Vec<u32>, cap: usize }                          // obviously correct, slow
impl Model {
    fn push(&mut self, v: u32) -> bool { if self.items.len() == self.cap { false } else { self.items.push(v); true } }
    fn pop(&mut self) -> Option<u32> { if self.items.is_empty() { None } else { Some(self.items.remove(0)) } }
}

/// The first step at which the ring and the model disagree, if any.
fn run(ops: &[Op], cap: usize, bug: bool) -> Option<usize> {
    let (mut ring, mut model) = (Ring::new(cap, bug), Model { items: vec![], cap });
    for (step, op) in ops.iter().enumerate() {
        let same = match *op {
            Op::Push(v) => ring.push(v) == model.push(v),
            Op::Pop => ring.pop() == model.pop(),
        };
        if !same || ring.len != model.items.len() { return Some(step); }   // compare after EVERY step
    }
    None
}

/// Delete operations one at a time while the run still fails.
fn shrink(mut ops: Vec<Op>, cap: usize, bug: bool) -> Vec<Op> {
    let mut i = 0;
    while i < ops.len() {
        let mut candidate = ops.clone();
        candidate.remove(i);
        if run(&candidate, cap, bug).is_some() { ops = candidate; } else { i += 1; }
    }
    ops
}

struct Lcg(u64);
impl Lcg { fn next(&mut self) -> u64 { self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); self.0 >> 33 } }

fn random_ops(seed: u64, n: usize) -> Vec<Op> {
    let mut rng = Lcg(seed);                                           // a fixed seed: the run is reproducible
    (0..n).map(|i| if rng.next() % 2 == 0 { Op::Push(i as u32) } else { Op::Pop }).collect()
}

#[test]
fn the_correct_ring_agrees_with_the_model_over_many_seeds() {
    for seed in 0..50 { assert_eq!(run(&random_ops(seed, 400), 4, false), None, "seed {seed}"); }
}

#[test]
fn the_planted_bug_is_found_and_shrunk_to_a_readable_case() {
    let ops = random_ops(1000, 400);
    let step = run(&ops, 3, true).expect("a wrap-around bug must show up in 400 random operations");
    let small = shrink(ops[..=step].to_vec(), 3, true);
    assert!(small.len() <= 8, "shrunk to {} operations: {small:?}", small.len());
    assert!(run(&small, 3, true).is_some());
    // What is left is: fill the ring, drain it past the wrap, and the length is off. Short enough to read.
}
```

### In the exercises

- **1c-01 to 1c-03:** every stage's tests are model tests: a list against a `Vec`, LRU against a four-line model, CLOCK against a rotating queue. Read the models in `tests/stages_1c.rs` to see how small a good model is.
- **1d-02, 1d-03:** the LRU-K replacer against a brute-force model that keeps every access time; the stage tests print the step and `k` on a mismatch.
- **1e-02, 1e-03:** ARC against an exact model of its rules on four `Vec`s, with the lists printed in the failure message.
- **1f-01 to 1f-03:** a buffer pool is easy to model with a table of "what each page should contain and how often it is pinned"; whatever was last written is what a fetch must return, however many pages were evicted in between.
- **2a-02, 2a-03:** `PageArray` against a `Vec`, and a sorted bucket page against a `BTreeMap`, on random operation sequences.
- **2b-01 to 2b-04:** MurmurHash3 against vectors recorded from the C++ original; the table against a `HashMap` with a rule for when an insert may fail, `verify_integrity` after every step.

### Where it is used

- **Property-testing libraries**: `proptest` (including its state-machine testing), `quickcheck`, and Python's Hypothesis stateful tests all generate operation sequences, compare to a model and shrink the failure.
- **Databases**: SQLite's `sqllogictest` compares SQLite's answers to other engines' on millions of queries; FoundationDB and TigerBeetle run whole clusters in deterministic simulation with a fixed seed so every failure replays.
- **Concurrent code**: the same loop with a sequential specification as the model is how linearizability checkers (Jepsen's Knossos/Elle) test databases; `loom` explores thread interleavings against the model.
- **Refactoring**: keep the old slow implementation as the model while you write the fast one; delete it when the fast one has survived a million steps.
