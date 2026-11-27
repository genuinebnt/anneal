---
title: Property testing and fuzzing: choosing properties, and the tools that find the input
summary: The five properties worth checking (round trip, invariant, idempotence, agreement with a model, never panics), how shrinking and coverage guidance work, and what `proptest` and `cargo-fuzz` add to a hand-written random loop.
minutes: 9
---
An example test says *for this input the answer is that*. A **property test** says *for every input of this shape, this statement holds*, then generates hundreds of inputs and tries to break it. **Fuzzing** is the same idea aimed at *crashes*: feed a function bytes, guided by which code paths each input reaches, and report any input that panics, hangs or corrupts memory.

The *model-based testing* article shows the loop with a hand-written generator and a slow model. This one is about **what to check** and about the tools that replace the hand-written parts.

## Five properties that find real bugs

| property | the statement | finds |
|---|---|---|
| **round trip** | `decode(encode(x)) == x` | a field written at the wrong offset, a lost sign, a truncated length |
| **invariant** | after every operation, `check_structure()` passes | a split that leaves a node under-full, a tombstone without its pair |
| **idempotence** | `f(f(x)) == f(x)` (sort, normalise, a second `flush`) | state that changes on a repeat call |
| **agreement with a model** | the fast structure matches an obviously correct slow one | everything the other four miss |
| **never panics** | any bytes given to the parser return `Ok` or `Err`, never a panic or an overflow | an index out of range on a short page, `unwrap` on corrupt input |

Write the property as a plain function from an input to `bool` (or an `assert!`). The generator is the second half of the work: it must produce inputs that *reach* the interesting cases (a full node, a key equal to the split key, an empty page), not just random noise.

## Shrinking

A failing input of 4000 random operations is useless to read. **Shrinking** reruns the property on smaller inputs (drop an operation, halve a number, shorten a vector) and keeps the smallest one that still fails. `proptest` does this by building each value from a *strategy* that knows how to shrink it; a hand-written loop shrinks by deleting operations from the failing sequence one at a time.

## Coverage-guided fuzzing

A fuzzer (`cargo-fuzz` drives libFuzzer; `afl.rs` drives AFL) compiles the code with coverage counters, keeps every input that reaches a *new* branch, and mutates the kept inputs (flip a bit, splice two inputs). Because it learns the format from the code's reactions, it finds inputs a random generator never would: a valid magic number, a checksum that happens to match. Run it for minutes to hours; a crash is saved as a file you turn into a regression test.

| | property test (`proptest`) | fuzzer (`cargo-fuzz`) |
|---|---|---|
| input | structured values from strategies | raw bytes (or `arbitrary`-derived values) |
| guided by | nothing: random, with shrinking | code coverage |
| runs | in `cargo test`, a second | a separate command, minutes to hours |
| best at | logic: invariants, models | parsers and decoders: no panic, no UB |
| pair it with | the model-based loop | `miri` and the sanitizers, to turn silent memory errors into crashes |

## C++ comparison

| C / C++ | Rust |
|---|---|
| RapidCheck, `QuickCheck` ports | `proptest`, `quickcheck` |
| libFuzzer (`-fsanitize=fuzzer`), AFL++ | `cargo fuzz` (libFuzzer underneath), `afl.rs`; the `arbitrary` crate derives a generator for a struct |
| sanitizers (`-fsanitize=address,undefined`) so a bug becomes a crash | `miri`, and `RUSTFLAGS=-Zsanitizer=address` on nightly |

**Port rule:** a C++ fuzz target and a Rust one have the same shape, `fn(&[u8])`; Rust's advantage is that safe code cannot corrupt memory, so most fuzz findings are panics and logic errors, not exploits.

## In real code

### Using it: round trip and "never panics" with a std-only generator

```rust test
/// A page header: 2-byte slot count, 2-byte free-space offset, 4-byte checksum (little endian).
#[derive(Debug, PartialEq, Clone, Copy)]
struct Header {
    slots: u16,
    free: u16,
    checksum: u32,
}

fn encode(h: &Header) -> [u8; 8] {
    let mut b = [0u8; 8];
    b[0..2].copy_from_slice(&h.slots.to_le_bytes());
    b[2..4].copy_from_slice(&h.free.to_le_bytes());
    b[4..8].copy_from_slice(&h.checksum.to_le_bytes());
    b
}

/// Returns `None` for anything that is not a plausible header; it must never panic.
fn decode(bytes: &[u8]) -> Option<Header> {
    let b: &[u8; 8] = bytes.get(..8)?.try_into().ok()?;
    let h = Header {
        slots: u16::from_le_bytes([b[0], b[1]]),
        free: u16::from_le_bytes([b[2], b[3]]),
        checksum: u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
    };
    (h.free <= 4096).then_some(h)
}

/// xorshift: a fixed seed means a failure reproduces.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn round_trip_holds_for_every_valid_header() {
    let mut rng = Rng(0x9E3779B97F4A7C15);
    for _ in 0..10_000 {
        let h = Header { slots: rng.next() as u16, free: (rng.next() % 4097) as u16, checksum: rng.next() as u32 };
        assert_eq!(decode(&encode(&h)), Some(h), "{h:?}");
    }
}

#[test]
fn the_decoder_never_panics_on_arbitrary_bytes() {
    let mut rng = Rng(42);
    for _ in 0..20_000 {
        let len = (rng.next() % 20) as usize; // short inputs matter most: that is where `[..8]` would panic
        let bytes: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        let _ = decode(&bytes); // the property is simply "returns"
    }
    assert_eq!(decode(&[]), None);
    assert_eq!(decode(&[0xFF; 7]), None);
}

#[test]
fn shrinking_by_hand_finds_the_smallest_failing_sequence() {
    // Suppose a structure breaks when it sees the value 13 after any 7. Delete operations while it still fails.
    let fails = |ops: &[u32]| ops.windows(2).any(|w| w[0] == 7 && w[1] == 13);
    let mut ops: Vec<u32> = vec![1, 7, 5, 13, 2, 9, 7, 13, 4];
    assert!(fails(&ops));
    let mut i = 0;
    while i < ops.len() {
        let mut shorter = ops.clone();
        shorter.remove(i);
        if fails(&shorter) {
            ops = shorter; // still fails without it: keep the smaller one
        } else {
            i += 1;
        }
    }
    assert_eq!(ops, [7, 13]);
}
```

### The same with the tools (not run here: they need crates)

```rust
// proptest: a strategy generates the values and shrinks them for you.
proptest! {
    #[test]
    fn round_trip(slots in any::<u16>(), free in 0u16..=4096, checksum in any::<u32>()) {
        let h = Header { slots, free, checksum };
        prop_assert_eq!(decode(&encode(&h)), Some(h));
    }
}

// cargo-fuzz: `cargo fuzz run decode` mutates inputs toward new branches until something panics.
fuzz_target!(|data: &[u8]| {
    let _ = decode(data);
});
```

### In the exercises

- **2b-02 to 2b-04** and **2d-04** ask for a `verify_integrity` / `check_structure` function that is itself the oracle for a random test: a checker you write, called after every operation.
- The model-based loop in the *model-based testing* article is the *agreement with a model* row, with a hand-written generator and a manual shrink.

### Where it is used

- **SQLite** is fuzzed continuously (its own `dbsqlfuzz` plus OSS-Fuzz), and its test suite is mostly properties.
- **The Rust compiler and standard library** run fuzzers and `proptest`-style tests; **`hashbrown`**, **`regex`** and **`serde_json`** keep `cargo-fuzz` targets.
- **FoundationDB's** simulation testing runs the whole database under random faults from a seed, the extreme form of everything above.
