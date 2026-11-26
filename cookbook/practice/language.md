# Practice: language

**How to use this page.** Run the recipe first (`cargo run --release --example <name>`), then do **Prove you got it** before reading further: write your prediction on paper, change the line, run, compare.
Do at least the [S] and one [M] task per recipe; the [A] tasks are for when you want the idea to stick for good. Never look up a solution: the proof is a failing assertion turning green, or a compiler error you provoked on purpose.
Put your attempts in a scratch crate (not in the recipe files) so the originals stay clean; finish with one of the Chapter projects at the bottom.

---

## borrowck_patterns
**Prove you got it**
1. In `main`, the `split_at_mut` call splits `v` at 2. Predict the output if you split at 0 and then swap `left[0]` with `right[0]`. What does the compiler say if you replace the split with `let a = &mut v[0]; let b = &mut v[3]; swap(a, b);`? Check.
2. `Engine::finish_current` moves out of a `&mut self` field. Predict which error you get if you replace the `take`-style call with `self.current.unwrap()`, then verify the error code.

**Try next**
- [S] Add a method `bump_all(&mut self)` that increments `total` once per queue item while iterating `self.queue` (borrowing two fields). Verify it compiles without clone, then rewrite it as `for i in 0..self.queue.len() { self.helper() }` where `helper(&mut self)` touches `queue`: provoke the error, then fix it by restructuring only.
- [M] Write a `Inventory { items: Vec<Item>, index: HashMap<String, usize> }` with `rename(&mut self, from, to)` that keeps `index` consistent. Property: after 1000 random renames (seeded), every `index` entry points to an item with that name. Make the failure mode: renaming to an existing name must return `Err`, not corrupt state.
- [A] Port a real graph-ish routine you wrote before (e.g. a union-find or an LRU list) so it has zero `clone()` calls and zero `Rc<RefCell>`. Verifiable: `grep -c clone` is 0 and your old test-suite passes.

**Use it in:** an in-memory cache/session store where one method must read a config field while mutating a map field. Same idea appears throughout ripgrep (<https://github.com/BurntSushi/ripgrep>), whose builders split state into several fields precisely so methods can borrow them separately.

**Pitfall:** reaching for `.clone()` or `Rc<RefCell>` as soon as the compiler complains, instead of splitting the struct, reordering statements or using the entry API.

---

## bytes
**Prove you got it**
1. `n = 0x0102_0304`. Predict the two arrays printed for `to_le_bytes()` and `to_be_bytes()`, then predict `back` (`0x01020304`) for `buf = [0x04,0x03,0x02,0x01,0xff]` read with `from_le_bytes(buf[..4])`. Change the slice to `buf[1..5]` and predict the new value before running.
2. The final line prints `decode_varint(&[0x80, 0x80])`. Predict what it returns (input ends with the continuation bit set, so an error/`None`, not a panic), then predict how many bytes the six numbers `[0,1,127,128,300,1<<40]` take in `enc` (1+1+1+2+2+6 = 13).

**Try next**
- [S] Add `zigzag(i64) -> u64` and `unzigzag`, then assert `zigzag(-1) == 1`, `zigzag(1) == 2`, `unzigzag(zigzag(i64::MIN)) == i64::MIN`.
- [M] Make `decode_varint` reject overlong encodings: `[0x80, 0x00]` (a padded zero) and an 11-byte run of `0x80` followed by `0x01` (overflows `u64`). Assert both return the error value, and that a 100,000-iteration seeded random-bytes loop never panics.
- [A] Define a record file: magic `b"FFLG"`, version byte, then records of `varint(len) + payload + u32 CRC (big-endian)`. Write `write_records(impl Write, &[&[u8]])` and `read_records(&[u8]) -> Result<Vec<Vec<u8>>, Error>`; flip every single bit of a 3-record file and assert the reader reports an error for each flip (document which bits, if any, are not covered).

**Use it in:** a write-ahead log or index file for a job runner. Protobuf and SQLite use this varint; for fast scanning see <https://github.com/BurntSushi/memchr>.

**Pitfall:** trusting a decoded length and calling `Vec::with_capacity(len)` before comparing it with the bytes that remain (memory-exhaustion bug).

---

## closures_hof
**Prove you got it**
1. `call_n(|| count += 1, 5)` compiles because the parameter is `impl FnMut()`. Predict the error (and the trait named in it) if you change `call_n`'s bound to `impl Fn()`; then predict what `consume(move || owned + "!")` would do if `consume` took `impl Fn() -> String` instead (the closure moves `owned` out, so it is only `FnOnce`).
2. `compose(|s: &str| s.len() as i32, double)("four")` prints 8. Predict the result of `compose(double, |x| x + 1)(5)` and of `compose(|x| x + 1, double)(5)` (11 and 12).

**Try next**
- [S] Add `fn twice_with<F: Fn(i32) -> i32>(f: F) -> impl Fn(i32) -> i32` returning `f` applied twice, and assert `twice_with(make_adder(5))(1) == 11`.
- [M] Extend `Registry` with `off(&mut self, name)` and make `fire` return `Result<String, FireError>` with `UnknownHandler(String)`; failure mode: a handler registered under the same name twice replaces the first (assert) or errors (your choice, documented). Test with `"greet"` registered twice.
- [A] Write `retry<T,E>(n: usize, mut backoff: impl FnMut(usize) -> Duration, mut sleep: impl FnMut(Duration), mut op: impl FnMut() -> Result<T,E>) -> Result<T,E>`. Verify with a recording `sleep`: an op failing twice then succeeding produces exactly two recorded delays equal to `backoff(0)` and `backoff(1)`, and an op that never succeeds produces `n - 1` sleeps.

**Use it in:** middleware and retry wrappers in a service client. Real: tower's layers wrap services the same way <https://github.com/tower-rs/tower>; compare `patterns/tower_service_layer`.

**Pitfall:** storing a closure that captures a local by reference in a `Box<dyn Fn>` field (as `Registry` does) without `move`; the `'static` bound on `on` rejects it.

---

## collections_tour
**Prove you got it**
1. `v` is `[5,3,8,3,1,8,9]`. Predict the printed `binary_search(&8)` and `binary_search(&4)` results after `sort_unstable(); dedup();` (hint: `Ok(index)` vs `Err(insertion point)`). Then move `dedup()` before `sort_unstable()` and predict the new vector.
2. The `events` BTreeMap holds keys 10,20,30,45. Predict `events.range(..25).next_back()` and `events.range(15..=35)` before running; then change `..25` to `..=20` and predict again.

**Try next**
- [S] In the `wc` word-count section, replace `entry(..).or_insert(0)` with `get` + `insert` and provoke the borrow error; record the error code (E0502) in a comment.
- [M] Add a sliding "last 3 events" view on the `BTreeMap`: write `fn window(events: &BTreeMap<u64,&str>, t: u64, width: u64) -> Vec<&str>` using `range(t-width..=t)`. Failure mode: `t < width` must not underflow (use `saturating_sub`); assert on `t = 5, width = 10`.
- [A] Reimplement `groups.entry(len).or_default().push(word)` as a generic `group_by<K: Hash+Eq, V>(iter, key_fn) -> HashMap<K, Vec<V>>`; verify against a naive nested-loop version on 1000 seeded random words, comparing after sorting keys.

**Use it in:** a log-analysis CLI that counts words, groups lines by length and queries time ranges (`BTreeMap::range`). The sorted-map + range idea is the core of LSM memtables (see `database/lsm_compaction_strategies`).

**Pitfall:** `dedup()` removes only consecutive duplicates, so calling it on an unsorted Vec leaves duplicates (see step 1 above).

---

## common_traits
**Prove you got it**
1. `jobs` is a `BTreeSet<Job>` containing ids/priorities `(2,1)`, `(1,9)`, `(3,9)` and `Job::cmp` is `o.priority.cmp(&self.priority).then(self.id.cmp(&o.id))`. The comment says `[1, 3, 2]`. Re-derive it, then flip `o.priority.cmp(&self.priority)` to `self.priority.cmp(&o.priority)` and predict the new order (`[2, 1, 3]`).
2. `"7".parse::<JobId>()` prints an `Err`. Predict the exact string (`` `7` must start with `job-` ``) and what `"job-x".parse::<JobId>()` returns (the `ParseIntError` text, `invalid digit found in string`).

**Try next**
- [S] Implement `From<JobId> for u32` and `TryFrom<u32> for JobId` rejecting 0; assert `JobId::try_from(0).is_err()`.
- [M] Add `Hash` consistency: write `impl PartialEq for Job` comparing only `id` (not priority) while keeping derived `Hash` over both fields; build a `HashSet` showing the law violation (two "equal" jobs landing in different buckets), then fix the `Hash` impl and assert the set now dedups.
- [A] Write `Money(i64 cents)` with `Display` (`-12.05`), `FromStr`, `Add`, `Sum`, `Default`, `Ord`; property over 100,000 seeded values: `parse(format(x)) == x` and `sum(parts) == whole`, including negatives and zero.

**Use it in:** domain keys and ids in any service; derive the obvious, hand-write the order. See <https://rust-lang.github.io/api-guidelines/interoperability.html> and `jiff`'s types <https://github.com/BurntSushi/jiff>.

**Pitfall:** `PartialOrd` that disagrees with `Ord` (the recipe delegates `partial_cmp` to `cmp` for that reason).

---

## const_and_generics_advanced
**Prove you got it**
1. `a = Matrix::<2,3>`, `b = a.transpose()` and `p: Matrix<2,2> = a.mul(&b)` asserts `[[14,32],[32,77]]`. Predict the compile error if you annotate `p: Matrix<3,3>` or call `a.mul(&a)` (dimension mismatch in the const parameters) and read which type the compiler expected.
2. `rb.push(100)` after nine pushes into `RingBuf<u32>` (capacity 8) returns `Some(1)`. Predict what `RingBuf::<char, 2>` returns for three pushes (`(None, None, Some('a'))`) and the value returned for a fourth.

**Try next**
- [S] Add `count_true(&PRIMES)` equivalent for a `const` table of squares up to 100 (`const SQUARES: [bool; 101]`) and assert the count is 10 (1, 4, ..., 100).
- [M] Add `Matrix::identity::<N>()` and `pow(self, k)` for square matrices only (`impl<const N: usize> Matrix<N, N>`); failure mode: calling `pow` on a `Matrix<2,3>` is a compile error (keep its code). Assert `pow(identity, 5) == identity` and Fibonacci via `[[1,1],[1,0]].pow(10)[0][1] == 55`.
- [A] Build `Quantity<U, const EXP: i32>`-free units: `Length<Meters>`, `Time<Seconds>`, `Speed` = `Length/Time` with `Div`; assert `size_of::<Length<Meters>>() == 8`, `Length<Meters> + Length<Feet>` is rejected until `.into()` (compile-fail comment), and `(Length::<Meters>::new(10.0) / Time::<Seconds>::new(2.0)).0 == 5.0`.

**Use it in:** embedded buffers and protocol tables (`const fn` CRC tables as in `crc32`). Real: <https://github.com/rust-embedded/heapless> and the `cb-no-std` crate here.

**Pitfall:** const-generic expressions like `[T; N + 1]` are not stable; pass both sizes as parameters (as `concat::<2,3,5>` does).

---

## conversions
**Prove you got it**
1. Predict the two lines: `Percent::try_from(50)` (`Ok(Percent(50))`) and `Percent::try_from(500)` (`Err("500 is not a percentage")`), then what `7i64.try_into()` gives and why `TryInto` needs no impl of its own.
2. Predict `big as u32` for `5_000_000_000u64` (`705032704`), `-1i32 as u32` (`4294967295`), `300i32 as u8` (`44`) and `3.99f64 as i32` (`3`); compute 5e9 mod 2^32 by hand first.

**Try next**
- [S] Add `impl From<Percent> for f64` returning `v as f64 / 100.0` and assert `f64::from(Percent(25)) == 0.25`.
- [M] Add a second error source to `MyErr` (`Range(String)` from your `Percent::try_from`) and make `parse_double` return `Result<Percent, MyErr>` after parsing; each failure keeps the original text. Table test: `"21"` ok, `"x"` -> `Parse`, `"500"` -> `Range`.
- [A] Parse unit strings: `"5MB"`, `"1.5GiB"`, `"512"` into `Bytes(u64)` with `FromStr` and `Display`; overflow (`"20000000000000EiB"`) is an error. Property: `format(parse(s)) parses to the same Bytes` for 10,000 seeded values.

**Use it in:** accept-anything APIs (`impl Into<String>`, `impl AsRef<Path>`) and `?` error plumbing. Real: <https://github.com/dtolnay/anyhow> leans on `From` for `?`.

**Pitfall:** narrowing with `as` silently truncates; use `try_from`.

---

## cow
**Prove you got it**
1. `normalize` is called on `"upload"`, `"Upload Video"`, `"thumbnail-small"`. Predict `Borrowed`/`Owned` for each (borrowed, owned, borrowed) and the owned output (`upload-video`).
2. Predict what `normalize("")` returns (`Borrowed`, empty), and whether `normalize("é")` allocates (no uppercase, no space: borrowed). Add the cases with `matches!(n, Cow::Borrowed(_))` assertions.

**Try next**
- [S] Add the character `_` to the rewrite rules (`snake_case` -> `snake-case`) and assert `matches!(normalize("a_b"), Cow::Owned(_))`.
- [M] Write `fn dedup_spaces(s: &str) -> Cow<'_, str>` that collapses runs of spaces, borrowing when no run exists. Property over 10,000 seeded strings: result equals a naive `split(' ').filter(non-empty).join(" ")` only when the input had no leading/trailing space (document and test the exact rule) and `Borrowed` iff the output equals the input.
- [A] Count allocations with a counting global allocator (`perf/counting_allocator`) over 1,000,000 mostly-clean ids (99% clean): assert the `Cow` version allocates about 10,000 times and an always-`String` version about 1,000,000.

**Use it in:** id normalisation and HTML escaping where most input needs no change. Real: `Cow` in std <https://github.com/rust-lang/rust/blob/master/library/alloc/src/borrow.rs> and serde's borrowed deserialization <https://github.com/serde-rs/serde>.

**Pitfall:** calling `.into_owned()` or `.to_mut()` eagerly on the hot path, defeating the point.

---

## drop_raii
**Prove you got it**
1. Predict the printed drop order for `_a, _b, _c` in one block (`c`, `b`, `a`) and the sequence "after early drop" relative to `drop x`. Then predict the final `leases returned:` vector after `risky(.., true)` and `risky(.., false)` (`[7, 7]`: both exit paths release the lease).
2. The panic test prints `drop guard-during-panic` then `panic caught: true`. Predict whether the `drop` line appears if you build with `panic = "abort"` in the profile (no: no unwinding), and what `mem::forget(Noisy("x"))` prints (nothing).

**Try next**
- [S] Add a field-drop-order test: `struct Pair { a: Noisy, b: Noisy }` declared `a, b`; predict the order (fields drop in declaration order `a, b`) and compare with locals (reverse).
- [M] Add `Defer::cancel(self)` that disarms the closure (use `Option<F>` inside); assert it runs exactly once, or never after `cancel`, with a `Cell<u32>` counter. Failure mode: a panic inside the deferred closure during unwinding aborts the process; document that.
- [A] Build a connection pool of 3 connections shared by 4 threads where `PooledConn` returns itself on `Drop` (`Mutex<Vec<Conn>>` + `Condvar`); after 10,000 checkouts assert `pool.idle() == 3`, that no checkout ever saw more than 3 connections in use (atomic high-water mark), and no thread deadlocks (join with a timeout).

**Use it in:** locks, permits, temp files, transaction rollback. Real: `MutexGuard` in std and tokio's semaphore permits <https://github.com/tokio-rs/tokio>.

**Pitfall:** doing fallible or blocking work in `Drop` and ignoring the error.

---

## dyn_vs_generic
**Prove you got it**
1. `run_generic(&Shout, &["a","b"])` prints `["A","B"]` and `run_dyn(&mixed, "task")` prints `["task","TASK","fixed"]`. Predict what happens if you try `run_generic(&mixed[0], ..)` (type `&Box<dyn Runner>` does not implement `Runner`: add `impl Runner for Box<dyn Runner>` to make it compile, or read the error).
2. Predict `size_of::<&Shout>()` versus `size_of::<&dyn Runner>()` (8 vs 16) and verify with `println!`.

**Try next**
- [S] Add a `Counting` runner holding `Cell<u32>` that counts calls; assert `run_generic(&counting, &["a","b","c"])` leaves the count at 3.
- [M] Make `Runner::run` return `Result<String, RunError>` and add `Retrying<R: Runner>` that retries `n` times; failure mode: a `Fake` scripted to fail twice then succeed is called exactly 3 times (count with `Cell`), and with `n = 1` the error is returned after 2 calls.
- [A] Add a method `fn run_all<I: IntoIterator<Item = String>>(&self, names: I) -> Vec<String>` to the trait and observe why `Box<dyn Runner>` can no longer be created (the error mentions that the trait is not dyn compatible); fix it with `where Self: Sized` and show the dyn call to `run` still works.

**Use it in:** swappable storage backends and test fakes. Real: tower's `Service` trait <https://github.com/tower-rs/tower>.

**Pitfall:** generic methods or `Self` returns make a trait non-object-safe, discovered only when you add `dyn` later.

---

## edition_2024_changes
**Prove you got it**
1. `if_let_scope` asserts `log == ["scrut", "else"]`: in edition 2024 the scrutinee temporary of `if let` is dropped before the `else` block runs. Predict the order that edition 2021 gave (`["else", "scrut"]`, per the recorded comment) and why `["scrut","else"]` matters for guards held across `else`.
2. `tail_scope` asserts `log == ["body-end","temp","local"]`: in 2024 the temporary in the tail expression drops before locals. Predict the 2021 order, then confirm by compiling the function alone with `rustc --edition 2021`.

**Try next**
- [S] Run `cargo fix --edition` on a 2021 scratch crate containing an `impl Trait` return capturing a reference and a `static mut`; list which rewrites it makes and which it only warns about.
- [M] Write a function `fn first_word<'a>(s: &'a str, scratch: &str) -> impl Iterator<Item = char> + use<'a>` and a caller that drops `scratch` before using the iterator (mirrors `rpit_capture`'s `make_adder(10, &temp)`); remove `use<'a>` and keep the compiler's error text (borrowed value does not live long enough).
- [A] Write a script (shell or Rust) that scans a crate for the 2024 hazards covered here (`static mut`, `extern "C"` blocks without `unsafe`, `#[no_mangle]`, `std::env::set_var` outside `unsafe`) and prints counts; run it on three crates you have locally and fix one.

**Use it in:** migrating a 2021 crate (`cargo fix --edition`) and reviewing 2024 code. Real: rust-analyzer migrated <https://github.com/rust-lang/rust-analyzer>.

**Pitfall:** assuming drop order of temporaries is unchanged across editions; guards and `RefCell` borrows in `if let` scrutinees behave differently.

---

## extension_traits_ops
**Prove you got it**
1. `[1,1,2,2,2,3,1,1].into_iter().dedup_adjacent()` prints `[1,2,3,1]` (the last two `1`s are adjacent so they collapse, but the first `1` run is separate). Predict `(1..=7).chunked(3)` (`[[1,2,3],[4,5,6],[7]]`) and the output of `"héllo wörld".truncate_chars(7)` (7 chars, not 7 bytes).
2. `c = a + b * 2.0 - V2{x:0.25,y:0.0}` with `a = (1,2)`, `b = (0.5,0.5)`, then `c += -a`. Compute `c` by hand ((1+1-0.25, 2+1-0) then minus a gives (0.75, 1.0)) and check `c[0]`, `c[1]`.

**Try next**
- [S] Add `IterExt::every_nth(n)` returning an adapter; assert `(0..10).every_nth(3).collect() == [0,3,6,9]` and that `every_nth(0)` panics with a clear message (or returns an error: choose).
- [M] Add `impl Mul<V2<f64>> for f64` (scalar on the left). Provoke the orphan-rule error first for a generic `impl<T> Mul<V2<T>> for T` (keep the error code), then solve it for concrete `f64` and assert `2.0 * V2{x:1.0,y:2.0} == V2{x:2.0,y:4.0}`.
- [A] Port five itertools helpers (`interleave`, `unique`, `group_by_key`, `intersperse`, `sliding(k)`) as an extension trait; each checked against a naive `Vec`-based implementation on 1,000 seeded random inputs including empty and single-element iterators.

**Use it in:** helper crates and fluent APIs. Real: itertools <https://github.com/rust-itertools/itertools>.

**Pitfall:** your extension method name colliding with a std method added later (compile warning `unstable_name_collisions` or silent shadowing).

---

## generics_bounds
**Prove you got it**
1. `largest(&[3, 9, 2])` works and `largest::<f64>(&[])` is called with a turbofish. Predict which bound makes `largest(&[1.5, 2.5])` fail to compile, then try it and read the error (`f64: Ord` not satisfied).
2. `sum_array([1,2,3])` and `sum_array([10;5])` use a const generic `N`. Predict what `sum_array([])` infers for `T` and `N`; run it and read the inference error.

**Try next**
- [S] Add a second impl of `Shout` for `Vec<String>` and check `"hi".shout()` still resolves (no ambiguity).
- [M] Make `Stack<T>::render` return an error-free join but require `T: Display`; then add `fn peek_max(&self) -> Option<&T> where T: PartialOrd` as a separate `impl` block with its own bound; verify that `Stack<Vec<u8>>` still has `push` but not `peek_max` (provoke the error).
- [A] Add a `Store` impl backed by a `BTreeMap` (`ByKey`), then write a generic `fn lookup_all<S: Store>(s: &S, ids: impl IntoIterator<Item = S::Key>) -> Vec<Option<S::Value>>` (adapt to the actual associated type names in the recipe); assert the same output across `ByName`, `ByIndex` and `ByKey` for equivalent data.

**Use it in:** a `Scheduler<S: Store>` that you test with an in-memory store and run with a file store. Real: `HashMap<K, V, S>` in std/hashbrown is generic over the hasher <https://github.com/rust-lang/hashbrown>.

**Pitfall:** putting `T: Display` on the struct definition instead of the `impl` that needs it, which forces the bound on every user of the type.

---

## generics_lifetimes_structs
**Prove you got it**
1. `Parsed::<i32>::new("1, 2, x, 4")` yields four results. Predict which is an `Err` and that the iterator does not stop at it (collected into a `Vec<Result<..>>`); then predict what `collect::<Result<Vec<f64>,_>>()` does for `"1.5,2.5"` (`Ok([1.5, 2.5])`).
2. `changes = vec![Change{ before: &old_text, after: &new_text }]` and `longest_before(&changes)` is printed inside the block where `new_text` still lives, but `old_text` lives longer. Predict whether `longest_before` may be used after `new_text` is dropped (yes if it returns `&'a str` tied only to `before`'s lifetime); try moving the print outside the block.

**Try next**
- [S] Add `View::last_two` returning borrowed data that outlives the `View` itself and prove it by dropping the view before printing.
- [M] Add `Memo::get_or_insert_with` style eviction: cap at 2 entries and assert `computed` after the sequence `7,8,9,7` is 4 (the first 7 was evicted). Failure mode: capacity 0 must compute every time.
- [A] Write a zero-copy `CsvRow<'a>` iterator yielding `&'a str` fields, handling quoted fields with commas; test 6 tricky lines (empty fields, trailing comma, quoted comma, quote at the end) and assert the yielded `&str` points into the original line (`as_ptr` range check).

**Use it in:** zero-copy parsers and borrowed iterators. Real: ripgrep's searcher borrows haystacks <https://github.com/BurntSushi/ripgrep>.

**Pitfall:** returning data tied to `&self` when it actually belongs to the source `'a`, so callers cannot keep results after the view is gone.

---

## hrtb_gats
**Prove you got it**
1. Remove the `bind` helper and write `let trim = |s| s.trim();`, then call `trim(&owned)` and `trim("  x")`. Predict which line fails and with a `lifetime may not live long enough` error (no E-code; with an un-annotated `|s|` you instead get E0282 type annotations needed); then restore `bind`.
2. The Fibonacci loop over `WindowsMut` writes `win[2] = win[0] + win[1]`. Predict the final `v` if the window `size` is 4 and you write `win[3] = win[2] + win[1]` (start `[1,1,0,0,0,0,0,0]`); check by running.

**Try next**
- [S] Add a `digit1` parser with `take_while(|c| c.is_ascii_digit())` and assert `seq(tag("n="), digit1).parse("n=42;")` gives `Some((";", "n=42"))`.
- [M] Add `separated_list(item, sep)` built from `seq` and `many0`; failure mode: input `"a,"` (trailing separator) must leave `","` unconsumed. Add that exact assertion plus `"a,b,c;"`.
- [A] Try to implement `collect()` for `WindowsMut` (a regular `Iterator` that returns the windows) and keep the compiler error that stops you; explain in a comment why a lending iterator cannot be a std `Iterator`. Then write `for_each_window(&mut self, f: impl FnMut(&mut [T]))` as the workable alternative and assert it produces the same Fibonacci result.

**Use it in:** a zero-copy tokenizer API where the token text borrows from the input. Lending iterators are the motivation for GATs; see the std `Iterator::windows` limitation and the `lending-iterator` discussions in <https://github.com/rust-lang/rust>.

**Pitfall:** sprinkling `for<'a>` everywhere; it is only required when elision cannot express "works for every lifetime", e.g. in `Box<dyn for<'a> Fn(&'a str) -> &'a str>`.

---

## implementing_std_traits
**Prove you got it**
1. `hash_demo` asserts `found_good == 100` but `found_bad < 100`. Predict why `BadCi` (case-insensitive `Eq` with a case-sensitive derived `Hash`) fails to find equal keys, and the fraction you would expect (hash buckets differ for `"KEY1"` vs `"key1"`).
2. `index_demo` asserts `m.get(0, 3) == None` ("column 3 must not alias row 1 col 0") for a 3-column matrix. Predict what a naive `data[r*cols + c]` bounds check would return for `(0, 3)` and why the explicit column check is needed.

**Try next**
- [S] Add `DoubleEndedIterator` to the `SortedVec` iterator and assert `iter().rev()` equals the reversed collect.
- [M] Implement `Index<Range<usize>>` for `SortedVec` returning `&[T]` and a `Borrow<[T]>`-based lookup; failure mode: slicing past the end panics with a message containing the length (test with `catch_unwind`).
- [A] Write `SmallVec<T, N>` with `Deref<Target=[T]>`, `Extend`, `FromIterator`, three `IntoIterator` impls and `Drop`; differential test against `Vec` with 100,000 seeded operations and assert drop counts.

**Use it in:** library collection types and newtypes. Real: <https://github.com/rust-lang/rust/blob/master/library/alloc/src/vec/mod.rs>.

**Pitfall:** `Hash`/`Eq`/`Borrow` disagreement, or `PartialOrd` not matching `Ord` (floats need `total_cmp`, see `ordering_demo`).

---

## io_files_paths
**Prove you got it**
1. `write_report(&mut sink, &[("a",1),("b",2)])` writes into a `Vec<u8>`. Predict the printed lossy string (`"a\t1\nb\t2\n"`), and `count_lines(&b"one\ntwo\nthree"[..])` (3: the last line has no trailing newline, `lines()` still counts it).
2. After two `atomic_write` calls the file holds `{"version":2}`. Predict what remains on disk if the process dies after `f.sync_all()` but before `fs::rename` (the old `state.json` intact plus a stray `state.tmp`), and what a reader sees.

**Try next**
- [S] After `fs::rename`, also `sync_all` the parent directory (open it as a `File` on Unix) and explain in a comment which crash it protects against.
- [M] Make `atomic_write` use a unique temp name (`state.json.<pid>.tmp`) so two processes writing the same target cannot clobber each other's temp file; test with two child processes (re-exec `std::env::current_exe`) each writing 500 times, asserting the final file parses and no `.tmp` is left.
- [A] Build a crash test: a child writes 1,000 versions and is killed with `SIGKILL` at random points; after each kill the parent asserts `state.json` parses and holds some complete version (never empty, never truncated). Compare with `os_and_storage/file_locking_atomic_replace`.

**Use it in:** saving job state or config safely. Real: <https://github.com/BurntSushi/walkdir> for robust directory traversal.

**Pitfall:** `std::fs::write(target, ..)` directly over the target: a crash mid-write leaves a truncated file.

---

## iterator_adapters
**Prove you got it**
1. Predict `Fib{a:0,b:1}.skip(5).step_by(2).take(5)` (Fibonacci: 0,1,1,2,3,5,8,13,...). Write the five numbers on paper first, then run.
2. `[100u8,100,100].iter().try_fold(0u8, ..checked_add)` prints `None`. Put a counter in the closure and predict how many times it runs (3: the third call overflows and stops the fold), then change the input to `[100,100]` and predict the output.

**Try next**
- [S] Change the `peekable` run-length example to group by case-insensitive level (`"info"`/`"INFO"`); assert `runs` for `["info","INFO","WARN"]` is `[("info",2),("WARN",1)]`-style as you define it.
- [M] Write your own `struct RunLength<I: Iterator>` adapter (not using `peekable`) that yields `(item, count)`; failure mode: empty iterator yields `None` immediately, a single item yields `(x, 1)`. Property: expanding the runs reproduces the input for 1000 seeded random vectors.
- [A] Replace the `counts` fold at the bottom with `iter::from_fn` + `BTreeMap` to compute word frequencies from a 1 MB generated text read via `BufRead::lines`, never collecting all lines; check peak line count held at once is 1 by incrementing a live-counter in a wrapper struct with `Drop`.

**Use it in:** a log summarizer (run-length of levels, top-N tokens). Real: ripgrep's `grep-searcher` streams lines lazily <https://github.com/BurntSushi/ripgrep>.

**Pitfall:** `rev()` after `step_by` or `chain` of mixed iterators needing `DoubleEndedIterator`; and forgetting that `by_ref()` is required to keep using an iterator after `take`.

---

## iterators
**Prove you got it**
1. `bad = ["a=1","b=oops","c"]`: predict which error `collect::<Result<Vec<_>,_>>()` returns (the first one, `b=oops`'s parse error). Put a `println!` inside `parse_line` and predict how many lines are parsed (2, not 3).
2. Predict `kept` (from `filter_map(ok)`) vs `err`; then change `bad` order to `["c","a=1","b=oops"]` and predict what `err` becomes.

**Try next**
- [S] Change `parse_line` to also reject keys that are empty (`"=5"`), with message including the line; add that input to `bad` and assert the exact message.
- [M] Return all errors, not only the first: write `fn parse_all(lines:&[&str]) -> Result<Vec<(String,u32)>, Vec<String>>` using `partition` on `Result`s; assert on `bad` that the error vector has 2 messages.
- [A] Compute `diffs`/`paired` for 100k random increasing numbers using `windows(2)` and verify against a plain index loop; then bench both with `perf/bench_harness` and write down whether the iterator version is slower (it should not be).

**Use it in:** loading `key=value` config files and CSV rows. See the Book chapter on iterators <https://doc.rust-lang.org/book/ch13-02-iterators.html>.

**Pitfall:** `unwrap()` inside the `map` closure instead of letting `collect` build a `Result`.

---

## library_api_polish
**Prove you got it**
1. `Shelf::builder().level(7)` returns an error whose `to_string()` is `"invalid level"` while `.source()` is `"level 7 is out of range 0..=2"`. Predict what `{:#}`-style single-line printers lose if you merged these into one message; then print `e.source()` chain yourself.
2. In `main`, delete the `_ =>` arm in the `match e` block. Predict the compile result *inside this single crate* (it still compiles because `#[non_exhaustive]` only bites downstream). Then explain how you would prove the downstream behaviour (hint: a second crate or a doctest in the `shelf` module is not enough).

**Try next**
- [S] Add a fourth tag limit test: `.tag` x4 returns the exact message `"at most 3 tags are allowed"` (already there); add a test that tags are lower-cased (`"Rust"` -> `"rust"`) and that duplicate tags are rejected with a new `Error::DuplicateTag(String)` variant.
- [M] Add `impl TryFrom<&str> for Level` accepting `"low"|"mid"|"high"`; failure mode: `"HIGH"` must be rejected (or accepted: decide and document). Add the case to the `lvl` assertions.
- [A] Create a second scratch crate depending on this module copied as a library; prove with a real downstream `match` that adding a new variant to `Error` does not break it, and that adding a `Storage` impl outside the crate fails (sealed trait). Keep both compile outputs in comments.

**Use it in:** any crate you publish. Real: the API Guidelines checklist <https://rust-lang.github.io/api-guidelines/checklist.html> and `thiserror`-based error types <https://github.com/dtolnay/thiserror>.

**Pitfall:** `#[non_exhaustive]` inside the defining crate has no effect, so tests there give false confidence.

---

## lifetimes
**Prove you got it**
1. Uncomment the `dangling` block in `main`. Predict the error code (E0597 `temp does not live long enough`) and which line the compiler blames. Then move `println!("{dangling}")` inside the braces and predict whether it compiles.
2. Change `prefer_first<'a>(a: &'a str, _b: &str) -> &'a str` to return `_b`. Predict the error; then add `'a` to `_b` too and predict what `prefer_first("a", &String::from("b"))` still allows.

**Try next**
- [S] Add `fn last_dep<'a>(e: &Entry<'a>) -> Option<&'a str>` and prove the returned `&str` outlives `e` by dropping `e` before printing it.
- [M] Extend `parse_entry` so a line with no colon yields `Entry` with empty deps, and with two colons returns `Err`; change the return type to `Result<Entry<'_>, String>` and assert 3 inputs.
- [A] Change `Entry` to own its data (`String`, `Vec<String>`) and benchmark parse of 1M lines versus the borrowed version with a counting allocator (`perf/counting_allocator`); write down the allocation counts per line.

**Use it in:** a zero-copy line parser for a manifest file. Real: the `regex-automata` / ripgrep matchers borrow haystacks the same way <https://github.com/BurntSushi/ripgrep>.

**Pitfall:** adding `'static` to make an error go away, which only works for literals and leaks the real problem.

---

## macro_rules
**Prove you got it**
1. `count!(1 (2 3) [4])` asserts `3`. Predict `count!(a, b, c)` (commas are tokens!) and run it; explain the number.
2. `using_x!(x)` with caller `x = 1` returns `101`. Predict the result of `using_x!(x * 2)` and explain which `x` each occurrence refers to (hygiene), then run.

**Try next**
- [S] Add `Yellow => "yellow"` to the `string_enum!` call and extend the round-trip assertion; then write the macro call with a missing comma and keep the exact error.
- [M] Add a `@sum` internal rule to `max_of!` style: write `min_of!(3, 9, 4,)` with an accumulator and an empty-input compile error (use `compile_error!`). Assertion: `min_of!(8)==8`, `min_of!(5,2,9)==2`.
- [A] Write `bitflags_lite!(Perm { Read = 1, Write = 2, Exec = 4 })` generating a struct with `const` values, `contains`, `|` via `BitOr`, and `Debug` listing set flags; test 8 combinations against a hand-written struct.

**Use it in:** removing repetitive enum/FromStr boilerplate (string-enum in a CLI). Proc-macro counterpart: <https://github.com/dtolnay/syn>; see `crates/cb-macros` in this cookbook.

**Pitfall:** forgetting that a `$e:expr` is evaluated each time it is pasted, so side effects run twice (try `max_of!(next(), 3)` with a counter).

---

## mem_functions
**Prove you got it**
1. After `c.advance()` twice on `Conn::Idle(Tracked::new(1))`, `live()` goes 1 then 0. Predict `live()` if you remove `drop(id);` in the `Active` arm (it is dropped at the end of the match arm instead): is it still 0? Verify and explain where `id` dies.
2. `mem::forget(Tracked::new(9))` leaves `live() == 1`. Predict `live()` if you wrap it in `ManuallyDrop` and never drop it vs forget; check.

**Try next**
- [S] Add a `Conn::reset(&mut self)` using `mem::take` on a `Default` variant; assert `live()` stays balanced.
- [M] Make `advance` panic in the `Active` arm (e.g. `assert_eq!(buf.len(), 4)`); catch with `catch_unwind` and assert the state is `Poisoned` and `live()` did not double-drop.
- [A] Make `Stack`'s `Drop` iterative (pop in a loop) and prove it with a 1,000,000-node push followed by drop; first show the default recursive drop overflowing in a thread with a 256 KiB stack (`thread::Builder::stack_size`).

**Use it in:** consuming-state `&mut self` methods in a connection or job state machine (see `patterns/state_machine`). Real: <https://github.com/rust-lang/rust/blob/master/library/core/src/mem/mod.rs>.

**Pitfall:** a `Poisoned` placeholder left observable after a panic; make every method check or documented-unreachable.

---

## modules_visibility
**Prove you got it**
1. Predict which of the three commented-out lines in `main` gives which error code (`Percent(500)` -> E0603 private tuple struct constructor; `private_detail()` -> E0603; `helper_for_parent()` -> E0603). Uncomment each and record the text.
2. Change `helper_for_parent` to `pub(crate)`; predict whether `engine::model::helper_for_parent()` now compiles from `main` (yes: same crate).

**Try next**
- [S] Make `OutOfRange(pub i32)` private-field and add a getter; update the assertion on `Percent::new(101)`.
- [M] Add `engine::graph::double(p: &Percent) -> Result<Percent, OutOfRange>` that cannot bypass the invariant; prove `double(&Percent::new(60)?)` returns `Err(OutOfRange(120))`.
- [A] Turn the module into a library `src/lib.rs` + `src/main.rs` and a `tests/api.rs` integration test; assert that `tests/api.rs` cannot see `pub(crate)` items (compile error E0603 kept in a comment).

**Use it in:** hiding invariants of a value type (percentages, validated ids) in an engine library. Real: ripgrep's crate layout <https://github.com/BurntSushi/ripgrep>.

**Pitfall:** making fields `pub` for test convenience, which permanently allows invalid values.

---

## newtype_ids
**Prove you got it**
1. Uncomment `names.get(line)`: predict the error text (`expected StopId, found LineId`, E0308). Then predict `size_of::<StopId>()` printed by the last line.
2. `StopId(1)` and `LineId(1)` hold the same number. Predict whether `stop == line` compiles; try it.

**Try next**
- [S] Add `Display` for `StopId` printing `S#1`; assert `format!("{stop}")`.
- [M] Make `Names::get` return `Option<&str>` (out-of-range id is `None`, not a panic); assert `get(StopId(9)) == None`.
- [A] Make `Names` generic: `IdVec<I: From<usize> + Into<usize>, T>`, and use it for both `StopId -> String` and `LineId -> Vec<StopId>`; compile-fail check: indexing the line table with a `StopId` is rejected (keep the error).

**Use it in:** typed indices into node/edge tables in any graph code. Real: petgraph's `NodeIndex` <https://github.com/petgraph/petgraph>.

**Pitfall:** deriving `From<u32>` for the newtype "for convenience", which re-opens the door to mixing ids.

---

## pattern_matching
**Prove you got it**
1. Predict the output list for `[-5, 0, 2, 10, 11].map(describe)` (hint: `i64::MIN..=-1`, then literals, then a guard). Run and compare.
2. For `Msg::Pair(1, 15)`, predict which arm fires (`b @ 10..=20`). Then change the second arm range to `10..20` and predict `Msg::Pair(1, 20)`.

**Try next**
- [S] Add `Shape::Triangle {a,b,c}` and observe which functions fail to compile; compile error E0004 (non-exhaustive) goes in a comment.
- [M] Make `port` return a typed error enum and reject ports `0`; add inputs `"h:0"` and `"h:70000"` to the assertions (the latter overflows `u16`).
- [A] Write `fn simplify(e: Expr) -> Expr` over `Add/Mul/Num` with patterns `Mul(Num(1), x)`, `Add(Num(0), x)`; property: `eval(simplify(e)) == eval(e)` for 10k seeded random trees.

**Use it in:** command/message dispatch in a server and AST walkers. Real: <https://github.com/oxc-project/oxc>.

**Pitfall:** a lazy `_ =>` arm on your own enum silences exhaustiveness exactly when you need it.

---

## phantomdata_typed
**Prove you got it**
1. `assert_send_sync::<NotSendTag<u8>>()` is commented out. Predict the error (a `*const u8` is not `Send`); uncomment and check. Then predict `SendTag<*const u8>` (uses `fn() -> T`) compiles.
2. `Id<T>` has manual `Clone/Copy`. Predict what happens to `let b = a; assert_eq!(a, b);` if you `#[derive(Clone, Copy)]` instead (because `User: !Clone`, derive adds `T: Clone`); run it.

**Try next**
- [S] Add a `Seconds` unit and show `Length<Meters> + Length<Seconds>` is rejected (E0308 comment).
- [M] Add `impl Mul<Length<U>> for Length<U>`? Decide whether it makes sense dimensionally and write what type it should produce (area), implementing `Area<U>`; assert `Length::<Meters>::new(2.0) * Length::new(3.0)` is `Area<Meters>` with value 6.0.
- [A] Wrap a file-descriptor-like `u32` into `Handle<Open>` / `Handle<Closed>` with `close(self) -> Handle<Closed>`; compile-fail checks: using a `Closed` handle's `read` and closing twice.

**Use it in:** typed ids (`Id<User>`), units of measure and permission tags. Real: <https://github.com/rust-lang/rust/blob/master/library/core/src/marker.rs>.

**Pitfall:** deriving traits on the tag-carrying type which adds bounds on the tag (`User: Clone`).

---

## recent_stable_language_features
**Prove you got it**
1. In `effective_port`, the config has `("port", " 8080 ")`. Predict the return; then with `"80"` (below 1024) and with `"abc"`. Which `let` in the chain fails each time?
2. `lengths(&names, &tmp)` returns `impl Iterator + use<'a>`, and `tmp` is dropped before the iterator is used. Remove `+ use<'a>` and predict the edition-2024 error (the iterator captures the `_scratch` lifetime, E0597); verify.

**Try next**
- [S] Add a fourth case to `classify` (`"a=+5"`) and predict then assert its classification; then rewrite the `if let` guard arm without guards to see the extra nesting.
- [M] Write `retry` usage where the async closure borrows a local `Vec` mutably (as `calls` does) but also stops after `max` failures: assert `retry(.., 2)` with an op that needs 3 attempts returns `Err("fail 2")`.
- [A] Port your own project function with 3 nested `if let` to a let-chain and a `let else` version; show that a test covering all 4 branches passes on both.

**Use it in:** config parsing and retry helpers. Release notes: <https://doc.rust-lang.org/stable/releases.html>.

**Pitfall:** let-chains and `if let` guards need a recent toolchain/edition 2024; an older CI image fails with a confusing syntax error.

---

## recent_std_apis
**Prove you got it**
1. `balances.get_disjoint_mut([0,2])` works and `[1,1]` errors. Predict the returned error variant for an out-of-range index like `[0,9]` (look at `GetDisjointMutError`) and verify.
2. `250u8.midpoint(254)` is `252`. Predict `i8::MIN.midpoint(i8::MAX)` and `5usize.checked_sub_signed(-3)`; run and compare.

**Try next**
- [S] Replace a manual `while let Some(x) = v.last() { if cond { v.pop(); } else { break } }` loop with `pop_if` and assert both give the same vector for `[1,5,10]` with `> 7`.
- [M] In `extract_if` on `HashMap`, add a case where the closure mutates the value (`*ttl += 1`) before deciding; assert the surviving values are bumped and the extracted ones are not.
- [A] Parse a binary packet with `split_first_chunk` and `from_be_bytes` in a function `parse_packet(&[u8]) -> Result<(u16,u32,&[u8]), PacketError>`; fuzz 100k random byte slices (seeded) without panic, and truncated input must return `Err`.

**Use it in:** replacing small helper crates and hand-written loops. Notes: <https://doc.rust-lang.org/stable/releases.html>.

**Pitfall:** adopting APIs that raise your MSRV without updating `rust-version` in `Cargo.toml`.

---

## ref_patterns
**Prove you got it**
1. After `let Person { nick, ref name, age } = p;`, the code asserts `p.name` and `p.age` still work. Predict the exact error if you add `println!("{:?}", p);` (E0382 borrow of partially moved value) and verify.
2. `o` goes `Some(10)` then increments three different ways, ending `Some(13)`. Predict `o` if one of the three used `Some(mut x)` (a copy) instead; check.

**Try next**
- [S] Use `as_deref()` on `Option<String>` in a function taking `Option<&str>` and assert it with `Some("ann")`.
- [M] Write `fn rename(p: &mut Person, new: &str)` using a pattern `Person { name, nick: Some(n), .. } = p` that updates both `name` and the nick when present; assert on a person with and without nick.
- [A] Write `find_mut(list: &mut [Person], name: &str) -> Option<&mut Person>` and use it to bump ages; acceptance: the borrow of `list` ends before a second call (two sequential calls compile), and returning two overlapping `&mut` is rejected (keep that error).

**Use it in:** reading older code with `ref`, and matching on borrowed structs in handlers. Reference: <https://doc.rust-lang.org/reference/patterns.html>.

**Pitfall:** accidental moves out of an `Option<String>` field in a `match`, making the whole struct partially moved.

---

## smart_pointers
**Prove you got it**
1. Predict the three numbers printed for `strong_count` (before, after dropping `a` and `b`), and `Rc::weak_count(&root)` after `*leaf.parent.borrow_mut() = Rc::downgrade(&root)`.
2. After `drop(root)`, predict `leaf.parent.borrow().upgrade()` and why `root` was freed even though `leaf` still points at it.

**Try next**
- [S] Print `Rc::strong_count(&leaf)` before and after `root` is dropped; predict and explain 2 -> 1.
- [M] Add `fn add_child(parent: &Rc<Node>, child: Rc<Node>)` that sets the parent link and pushes; failure mode: adding the same child twice must be rejected (compare with `Rc::ptr_eq`). Test it.
- [A] Replace `Weak` with `Rc` in the parent link, build the same tree, drop `root` and show a leak with a `Drop` counter that never reaches 0; then fix and show the counter reaching 0.

**Use it in:** DOM-like trees and graphs with parent links. Real: <https://github.com/rust-lang/rust/blob/master/library/core/src/cell.rs>.

**Pitfall:** `Rc` cycles leak silently.

---

## static_const_lazy
**Prove you got it**
1. `EXIT_NAMES` prints `(building table once)`. Predict how many times it prints across the two `.get` calls and across a second thread that touches it; add a thread to check.
2. `CALLS` is `thread_local`. Predict the pair printed for this thread and the spawned thread (1 and 0).

**Try next**
- [S] Change `NEXT_ID` ordering to `SeqCst` and explain why `Relaxed` is enough for a unique-id counter.
- [M] Call `config()` before `CONFIG.set`; predict the panic message and replace `expect` with a `Result<&Config, ConfigError>`; assert the error.
- [A] Spawn 8 threads x 10,000 `next_id()` calls, collect all ids and assert they are unique and form exactly `1..=80000` plus the earlier two ids.

**Use it in:** global config set once in `main`, lookup tables, id generators. Real: <https://github.com/rust-lang/rust/blob/master/library/std/src/sync/once_lock.rs>.

**Pitfall:** `static mut` and lazy init racing; use `OnceLock`/`LazyLock`.

---

## strings
**Prove you got it**
1. `"héllo"`: predict `len`, `chars().count()` and the output of `s.get(0..2)` vs `s.get(0..3)` (`None` vs `Some("hé")`); then write `&s[0..2]` and read the panic message.
2. Predict `"ß".to_uppercase()` and `"a-b-c".replace('-', "_")`; run.

**Try next**
- [S] In the `name` section, predict the three printed values after `retain`, `drain(..4)`, `pop` from `"flow-forge"`; then change `..4` to `..5` and predict.
- [M] Write `truncate_chars(s: &str, n: usize) -> &str` returning a slice on a char boundary with `char_indices`; assert `truncate_chars("héllo", 2) == "hé"` and `n` larger than length returns the whole string.
- [A] Write a `parse_kv_line` for `"  task-42 : transcode ; retries=3  "` returning `(id, name, retries)` with `split_once`/`strip_prefix`; fuzz with 10k random strings (including multi-byte chars) asserting no panic.

**Use it in:** parsing ids, paths and log lines. Real: <https://github.com/BurntSushi/ripgrep>.

**Pitfall:** slicing at a byte index in the middle of a multi-byte char (panics).

---

## Chapter projects

### 1. Small (~2 h): typed ids and a validated config
Combine `newtype_ids`, `conversions`, `common_traits`, plus `errors/error_enum`.
- Milestones: (1) `JobId`/`QueueId` newtypes with `Display`/`FromStr`; (2) `Config::from_str` that returns an error enum; (3) CLI printing errors with exit code 1.
- Acceptance: a table test of 10 malformed inputs; round-trip property over 10k seeded ids; passing a `QueueId` where `JobId` is expected fails to compile (keep as comment).
- Stretch: `serde` support via `encoding_compression/serde_custom`.

### 2. Intermediate (~weekend): zero-copy log analyser
Combine `lifetimes`, `generics_lifetimes_structs`, `iterator_adapters`, `cow`, `bytes`, plus `text_and_parsing/zero_copy_lexer`.
- Milestones: (1) borrowed `LogLine<'a>` parser; (2) streaming iterator that never allocates per line; (3) top-N and per-minute histogram; (4) `Cow` for normalised ids.
- Acceptance: allocation counter shows 0 allocations per clean line (use `perf/counting_allocator`); differential test vs a naive `String`-based parser on 100k generated lines.
- Stretch: lending iterator via `hrtb_gats`.

### 3. Advanced (~week): own `std`-style collection crate
Combine `implementing_std_traits`, `generics_bounds`, `phantomdata_typed`, `mem_functions`, `const_and_generics_advanced`, plus `unsafe_ffi/maybeuninit_fixedvec` and `dev_tools/shrinking`.
- Milestones: (1) `SortedVec`, `SmallVec<T,N>`, `IdVec<I,T>`; (2) full trait surface; (3) differential test vs `Vec`/`BTreeMap` with seeded ops and shrinking; (4) docs with doctests and `#[non_exhaustive]` where appropriate.
- Acceptance: 1M random ops agree with the model; `cargo +nightly miri test` clean for unsafe parts; `cargo doc` has no warnings.
- Stretch: benchmark vs the std types; publish as a local crate and consume it from a second crate.
