# Practice: unsafe_ffi

**How to use this page.** Run each recipe in `--release`, then answer **Prove you got it** before you read the unsafe blocks. Every `unsafe` block you write must cite its invariant in a `// SAFETY:` comment: write it first, then the code.
Run Miri on everything you write here: `cargo +nightly miri test` (FFI calls into C are not supported by Miri, so isolate them). Finish with a Chapter project.

---

## unsafe_basics
**Prove you got it**
1. The sweep calls `my_split_at_mut(&mut a, mid)` for every `mid` in `0..=5` and then writes through `l.first_mut()` and `r.last_mut()`. Predict the final printed array after all six iterations (each iteration adds 100 to the first element of the left part, if any, and to the last element of the right part, if any); work it out on paper first.
2. `my_swap` is differentially tested against `slice::swap` with a drop counter `Counted`, asserting `drops == 0` after swaps and `drops == n` after the scope ends. Predict which assertion fails if `my_swap` is implemented with `ptr::write` of both values without `ptr::read` first (a duplicated value, so a double drop); test by mutating a copy.

**Try next**
- [S] In `checked_get`, change the bounds check to `i <= len` and predict what `catch_unwind(|| checked_get(&[1,2,3], 3))` does now (Miri reports out-of-bounds; plain run may return garbage). Run `cargo +nightly miri run --example unsafe_basics` to see the report.
- [M] Write `unsafe fn get_pair_mut<T>(s: &mut [T], i: usize, j: usize) -> (&mut T, &mut T)` plus a safe wrapper that returns `None` for `i == j` or out-of-range. Acceptance: 100,000 seeded random `(len, i, j)` agree with a safe oracle built from `split_at_mut`, run under Miri for 1,000 cases.
- [A] Audit one `unsafe` block from a small dependency in your `Cargo.lock` (pick one with fewer than 10 blocks): write a `// SAFETY:` comment in your own words naming each invariant, and produce an input that would violate it if the invariant were removed (or explain why none exists). Verification: your argument references specific lines.

**Use it in:** hot loops and std-like containers. Real: the std slice implementation <https://github.com/rust-lang/rust/blob/master/library/alloc/src/slice.rs>.

**Pitfall:** treating `unsafe` as "borrow checker off"; it only unlocks five operations and each still needs an invariant.

---

## unsafecell_cells
**Prove you got it**
1. In `main`: after `c.take()` on a `MyCell<String>`, the next `take()` returns `""`. Predict what `take` returns for `MyCell<Vec<i32>>` and for `MyCell<NonZeroU8>` (the latter will not compile: E0599, method `take` exists but its trait bounds were not satisfied); read the error.
2. The model test compares `MyRefCell` against `std::cell::RefCell` for 20,000 random steps. Predict which assertion fires first if `try_borrow_mut` forgets to check the reader count (a mutable borrow succeeds while readers exist: `a.is_ok() == b.is_ok()`).

**Try next**
- [S] Add `MyCell::update(&self, f: impl FnOnce(T) -> T) where T: Copy` and assert `n.update(|x| x*2)` on 6 gives 12.
- [M] Make `MyRefCell<T>` `!Sync` by default and prove that sharing across threads fails (E0277, `RefCell`-like). Then write `MyMutex<T>` with an `AtomicBool` spin lock around `UnsafeCell`, implement `Sync` manually with a `// SAFETY:` justification, and run 8 threads x 100,000 increments; assert the final count is exactly 800,000.
- [A] Implement `MyOnceCell<T>` (set-once, then `get`) using `UnsafeCell<Option<T>>` and a state enum; acceptance: the model test pattern from the recipe against `std::cell::OnceCell` with 20,000 random `set/get` steps, Miri clean.

**Use it in:** custom caches and locks. Real: std's `Cell` <https://github.com/rust-lang/rust/blob/master/library/core/src/cell.rs>.

**Pitfall:** returning a `&mut T` from `&self` without runtime tracking; two such references are instant UB.

---

## maybeuninit_fixedvec
**Prove you got it**
1. `FixedVec<String, 3>`: after three pushes a fourth `push("d".into())` returns `Err("d")`. Predict the drop count for that rejected `"d"` (it is returned to the caller, so it is dropped there, not leaked).
2. The panic-safety test makes the middle element's drop panic and requires the others to be dropped exactly once. Predict how many `Tracked` values are dropped if `Drop for FixedVec` calls `drop_in_place` on the slice `0..len` in one shot versus one element at a time with `catch_unwind`; read the recipe's choice and its comment.

**Try next**
- [S] Add `truncate(&mut self, n: usize)` that drops elements `n..len` exactly once; test with `Tracked` counters: truncating 8 elements to 3 drops 5.
- [M] Add `extend_from_slice(&mut self, s: &[T]) -> Result<(), usize> where T: Clone` where the error is the number that fit. Failure mode: `Clone::clone` panics on the 3rd element; assert no leak and no double drop (the live count equals what was pushed). Run under Miri.
- [A] Build `SmallVec<T, N>` (inline up to N, then spill to `Vec<T>`); acceptance: differential test against `Vec` with 100,000 seeded operations (the recipe's random loop is the template), `balanced()` drop accounting, and `cargo +nightly miri test` clean.

**Use it in:** no-alloc/embedded code and small-vector optimisation. Real: <https://github.com/rust-embedded/heapless>; see `crates/cb-no-std` in this cookbook.

**Pitfall:** reading a `MaybeUninit` slot after it has been moved out or dropped; the `len` invariant is the only guard.

---

## nonnull_linked_list
**Prove you got it**
1. The deque test asserts `d.iter().rev()` gives `[3,2,1]` after `push_back(2), push_back(3), push_front(1)`. Predict the output of `pop_front`, `pop_back`, `pop_back`, `pop_back` after `iter_mut().for_each(|x| *x *= 10)`: `Some(10), Some(30), Some(20), None`.
2. The model test compares with `VecDeque` and checks `CREATED == DROPPED`. Predict which assertion fails if `pop_front` forgets `Box::from_raw` (a leak: `CREATED > DROPPED`) versus calls it twice on a single-element list (double free: Miri error, or `DROPPED > CREATED`).

**Try next**
- [S] Add `peek_front(&self) -> Option<&T>` and `peek_back_mut(&mut self) -> Option<&mut T>` and assert them on the three-element deque.
- [M] Add `append(&mut self, other: &mut Deque<T>)` in O(1) (splice tail to head). Failure mode: `d.append(&mut d)` must be a compile error (E0499, keep it in a comment). Differential test against `VecDeque::append` over 1,000 random pairs.
- [A] Add a cursor (`move_next`, `move_prev`, `insert_after`, `remove_current`) and extend the model test to 100,000 seeded operations vs a `Vec` + index model; acceptance: Miri clean, drop counters balanced.

**Use it in:** LRU caches and intrusive lists. Real: std's `LinkedList` <https://github.com/rust-lang/rust/blob/master/library/alloc/src/collections/linked_list.rs>; also *Learn Rust With Entirely Too Many Linked Lists* <https://github.com/rust-unofficial/too-many-lists>.

**Pitfall:** `drop` recursion on a long list; the recipe drops a 200,000-element deque to prove it iterates.

---

## pin_projection
**Prove you got it**
1. `holder = vec![a]` moves the `Pin<Box<SelfRef>>`. Predict whether `&holder[0].data as *const _ as usize == a_addr` still holds (yes, only the box pointer moved) and what changes if `SelfRef` were stored by value in the Vec and the Vec reallocated.
2. `async { Timer::new(1).await + 10 }.map_with(|x| x*2).map_with(|x| x+1)` is asserted to equal 25 and `Timer::new(3)` resolves to 4. Predict the result of `Timer::new(0)` mapped with `n.to_string()` (the assertion says `"1"`): how many polls does a zero-tick timer need?

**Try next**
- [S] Add a `Join2<A, B>` combinator test: two `Timer`s of 2 and 5 ticks; assert it completes after the longer one and returns both outputs.
- [M] Implement `MapFuture` polling after completion: second `poll` must panic with a clear message (as `std` combinators do) or return `Pending` forever; choose, document and test with `catch_unwind`.
- [A] Write `Timeout<F>` with a virtual tick counter (no real clock) that returns `Err(Elapsed)` when the inner future needs more ticks; acceptance: deterministic tests where the inner completes on tick 3 with limit 3, tick 4 with limit 3, and is `ready` immediately. Compare with `async/waker_executor` for the executor side.

**Use it in:** future combinators and self-referential state machines. Real: std's `Pin` docs <https://github.com/rust-lang/rust/blob/master/library/core/src/pin.rs>.

**Pitfall:** structurally pinning a field that is `Unpin`-unsafe to move; project with `Pin::new_unchecked` only when the field is never moved out.

---

## repr_layout_ffi
**Prove you got it**
1. The recipe asserts `decode(&bytes[..5])` is `Truncated { need: 12, have: 5 }` and that flipping `bytes[0]` gives `BadMagic`. Predict the error for `bytes[6] = 99` (`BadKind(99)`) and for overwriting bytes `8..12` with `u32::MAX` (a `Truncated` because the count claims too many records).
2. `FileHeader` is `repr(packed)`. Predict the exact compiler error if you write `let r = &fh.len;` (E0793) and why `let (len, crc) = (fh.len, fh.crc)` is fine.

**Try next**
- [S] Add a `version: u8` field to the header, update `HEADER_LEN`, and see which assertion catches your layout drift first (`offset_of!`-style or `HEADER_LEN`).
- [M] Make `decode` reject `count * 8 + HEADER_LEN` overflow on 32-bit by using `checked_mul`; test with `count = u32::MAX` on both `usize` widths if you can (or simulate with `u32` arithmetic) and assert `Err`, never panic. Add a 100,000-case seeded random-bytes fuzz asserting no panic.
- [A] Write the encoded file to disk and read records through `mmap` (see `os_and_storage/mmap_files`) with unaligned-safe reads; acceptance: `parse_header_unsafe` and `parse_header_manual` agree on 10,000 random valid files, including ones starting at an odd offset (as the recipe's `shifted` test does).

**Use it in:** binary file formats and FFI structs. Real: `zerocopy` <https://github.com/google/zerocopy> and `bytemuck` <https://github.com/Lokathor/bytemuck> encode these rules in types.

**Pitfall:** `transmute` of a byte to a `repr(u8)` enum; an invalid discriminant is UB, use `TryFrom<u8>` as the recipe does.

---

## ffi_c_interop
**Prove you got it**
1. `c_qsort` sorts `[5,-3,9,0,2,2,-8,7]` and counts comparator calls with an atomic. Predict whether `COMPARISONS >= len - 1` holds and the printed count is the same on every platform (no: it depends on the libc's algorithm).
2. A callback panics (`c.push(13)` triggers it) and `sum_via_callback` returns `Err(FF_CALLBACK_PANIC)`. Predict what happens in edition 2024 if the `catch_unwind` guard is removed from the trampoline: the process aborts when unwinding crosses `extern "C"`; verify by deleting the guard in a copy.

**Try next**
- [S] Call `libc::abs` and `libc::getpid` through safe wrappers; assert `getpid() == std::process::id()`.
- [M] Wrap `getenv` for a variable containing non-UTF-8 bytes (set it from a spawned child with `OsStr` bytes on Unix) and return `Result<Option<String>, Utf8Error>`; assert the three cases (unset, valid, invalid).
- [A] Write a 30-line C library (a counter with `new/add/free`), compile it with `cc` in a `build.rs` (see `crates/cb-build-script`), and wrap it with an opaque-handle safe API and a callback; acceptance: a leak check (`leaks` on macOS or `valgrind` on Linux) reports 0 leaks and a panic in your callback is contained as in the recipe.

**Use it in:** wrapping C libraries (`-sys` crates). Real: bindgen <https://github.com/rust-lang/rust-bindgen> and cxx <https://github.com/dtolnay/cxx>.

**Pitfall:** freeing Rust-allocated memory with `libc::free`; always hand it back to the Rust `string_free` as the recipe does.

---

## cb-cdylib-c-abi (crate: `crates/cb-cdylib-c-abi`)
**Prove you got it**
1. `cb_greet` returns Rust-allocated memory that C must give back through `cb_string_free`. Predict what goes wrong (allocator mismatch, UB) if the C client in `tests/c/client.c` calls `free()` on it; add that line to a copy and run under `leaks`/ASAN if available.
2. `cb_test_panic` deliberately panics. The README says unwinding out of `extern "C"` aborts. Predict the result code the C client sees now (`CB_ERR_PANIC`, from `catch_unwind`) and the process behaviour if you remove the `catch_unwind` wrapper (abort, no exit code 0).

**Try next**
- [S] Add `cb_counter_reset(c)` to `src/lib.rs` and `include/cb_c_abi.h`, and one line in `tests/c/client.c` calling it; verify through `tests/c_client.rs` (expected stdout string is in that file, update it).
- [M] Add `cb_counter_history_len(const cb_counter*, size_t *out)`. Failure mode: a NULL handle or NULL `out` returns an error code and sets the text returned by `cb_last_error_message()`; assert with a C test that prints it and a check in `EXPECTED`.
- [A] Write a second client in Python (`ctypes`) or Swift against the same header and library, covering: version check (`cb_abi_version`), counter lifecycle, the history callback with `user_data`, and the buffer-size protocol of `cb_fill_buffer` (too-small buffer reports required size). Acceptance: the same expected output as the C client.

**Use it in:** exposing a Rust core to Python, Swift or C hosts. Real: cbindgen <https://github.com/mozilla/cbindgen> generates the header you hand-wrote here.

**Pitfall:** exporting a function that takes `&str` or returns `Result`/`Vec`; none of these have a stable C layout.

---

## cb-build-script (crate: `crates/cb-build-script`)
**Prove you got it**
1. `tests/build_script.rs` asserts `crc32(b"123456789") == 0xCBF4_3926` and `CRC32_TABLE[1] == 0x7707_3096`. Predict which of the two fails if the generated table in `build.rs` uses the wrong polynomial constant, then change it by one bit and see the failure.
2. The README says `CB_BUILD_LABEL=x cargo test ...` changes the stamp. Predict whether a second `cargo test` with the same value rebuilds the crate (no, thanks to `rerun-if-env-changed`), and what happens if you delete that directive (it reruns only when package files change, so the label can go stale).

**Try next**
- [S] Add `cargo::rustc-env=BUILD_DIR_NAME=...` stamping the last path component of `OUT_DIR` and assert the value is non-empty in a test.
- [M] Add a probe for a std API (for example `Vec::pop_if`) following the existing `has_x` pattern, with `cargo::rustc-check-cfg`; failure mode: on a compiler without it, a fallback implementation is used. Verify both paths by forcing the probe result with an env var override.
- [A] Generate a second table in `OUT_DIR` (CRC-16 or a popcount table) from a description file `data/poly.txt` with `rerun-if-changed`; acceptance: editing the file triggers a rebuild (`cargo build -v` shows the script running) and a test checks 5 known values.

**Use it in:** `-sys` crates, codegen (`prost`, `bindgen`), embedding git hashes. Real: the `cc` crate <https://github.com/rust-lang/cc-rs> and `autocfg` <https://github.com/cuviper/autocfg>.

**Pitfall:** `cfg!(target_os = ...)` inside `build.rs` describes the host, not the target; read `CARGO_CFG_TARGET_OS`.

---

## cb-no-std (crate: `crates/cb-no-std`)
**Prove you got it**
1. `tests/no_std.rs::stack_basic_and_overflow` asserts `Stack<_, 2>::push(3)` after two pushes returns `Err(3)`, and the write-macro test asserts a failed `write!` leaves `b.as_str() == "x=5 y=007"` unchanged. Predict the buffer contents if `FmtBuf::write_str` wrote the bytes that fit before failing (a partial write) and which assertion fires.
2. `bump_alignment_and_exhaustion` asserts `(y as *mut u32 as usize) % 4 == 0` and that a 32-byte allocation fails after earlier ones. Predict `a.used()` after allocating a `u8` then a `u32` (it includes 3 bytes of padding) and print it.

**Try next**
- [S] Add `Ring::peek_front(&self) -> Option<&T>` and a test that it does not remove the element, plus `peek_front` on an empty ring.
- [M] Add `FmtBuf::write_hex(u32)` using only `core`; failure mode: capacity 7 with an 8-digit value returns `fmt::Error` and leaves the buffer unchanged (as the existing test requires). Build once with `--no-default-features` to prove it needs no `std`.
- [A] Run the cross-compile check: `rustup target add thumbv7em-none-eabihf && cargo check --no-default-features --target thumbv7em-none-eabihf`. Then add a new module (a fixed-capacity `BitSet<N>`) and confirm it still compiles for that target; if you accidentally use `Vec`, keep the compiler error in a comment.

**Use it in:** firmware and kernel code. Real: heapless <https://github.com/rust-embedded/heapless> and embassy <https://github.com/embassy-rs/embassy>.

**Pitfall:** forgetting that the final binary needs `#[panic_handler]` (and an allocator if `alloc` is used); a library does not.

---

## Chapter projects

### 1. Small (~2 h): safe `split_pair` and `FixedVec` audit
Combine `unsafe_basics`, `maybeuninit_fixedvec`, plus `dev_tools/shrinking`.
- Milestones: (1) write `get_pair_mut` with SAFETY comments; (2) shrinking property test finds your deliberately injected bug; (3) fix it.
- Acceptance: Miri clean; 100k random cases agree with a safe oracle.
- Stretch: add a `loom` test for a trivial atomic.

### 2. Intermediate (~weekend): a C-callable library with a safe Rust core
Combine `ffi_c_interop`, `repr_layout_ffi`, `cb-cdylib-c-abi`, `cb-build-script`, plus `errors/error_enum`.
- Milestones: (1) safe Rust `Counter`/`Store` with typed errors; (2) C ABI wrapper with opaque handle, error codes, thread-local last error; (3) C client test compiled by `cc`; (4) layout asserts on both sides.
- Acceptance: C client runs with ASAN or `leaks` clean; panics never cross the boundary (test via `catch_unwind`).
- Stretch: generate the header with <https://github.com/mozilla/cbindgen>.

### 3. Advanced (~week): your own `LinkedHashMap` + arena with Miri
Combine `nonnull_linked_list`, `unsafecell_cells`, `pin_projection`, `maybeuninit_fixedvec`, plus `data_structures/lru_cache` and `memory/slab_generational`.
- Milestones: (1) hash map from slab handles; (2) intrusive doubly linked order list via raw pointers; (3) LRU eviction; (4) `Send`/`Sync` decisions documented.
- Acceptance: differential test against `data_structures/lru_cache` semantics with 1M seeded ops; Miri clean; no leaks (drop counter).
- Stretch: concurrent version with `loom`.
