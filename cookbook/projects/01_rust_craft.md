# 01. Rust craft: tools, APIs and idioms

Projects that make ownership, errors, traits and API design second nature. Recipe stems are written `folder/stem` and live in `examples/folder/stem.rs`. A 🔜 means the recipe is planned but not written yet: use the linked docs instead.

Rules for every project: write the naive/oracle version first, keep a `NOTES.md` with your write-up answers, and never paste from the recipes (see `HOW_TO_STUDY.md`).

---

## 1.1 `lsx`: a directory lister with excellent errors  `S`

**Goal.** Build a CLI `lsx [PATH...] [--sort size|name|mtime] [--json] [--depth N]` that lists files, never panics, and reports every failure (missing path, permission denied, invalid UTF-8 name, broken symlink) with context and a distinct exit code.
**Learn.** `clap` derive, `anyhow` vs a typed error enum, `Result` plumbing with `?`, exit codes, `OsStr` vs `str`, stdout/stderr separation.
**Read before.** `dev_tools/clap_cli`, `errors/error_enum`, `errors/anyhow_context`. **During.** `errors/option_result_toolbox`, `language/io_files_paths`.
**Milestones.**
1. Lists one directory, sorted by name, plain text. Exit 0.
2. Nonexistent path prints `lsx: <path>: No such file or directory (os error 2)` to stderr, exit 2; other paths still listed (partial failure exits 1).
3. `--sort size|mtime` with a stable tie-break by name.
4. `--json` emits one NDJSON object per entry (`path`, `size`, `kind`), non-UTF-8 names escaped, never lossy-dropped.
5. `--depth N` recursion that detects symlink loops.
**Acceptance.** Integration tests via `assert_cmd` (or `std::process::Command`) for: missing path, unreadable dir (`chmod 000`), a dir of 10,000 files (under 200 ms release), a file named with bytes `0xFF`, a symlink cycle `a -> b -> a`. `grep -rn "unwrap()\|expect(" src/` returns nothing outside tests.
**Data.** Generate the fixture tree from a seeded script in `tests/` (depth 4, fanout 6, 1,296 leaves, random sizes 0..1 MiB sparse).
**Write-up.** Which errors are recoverable per-entry vs fatal? Why `thiserror` in the lib part and `anyhow` only in `main`? How did you pick the exit codes?
**Stretch.** `--color` with `NO_COLOR` support; parallel walking with a bounded thread count and deterministic output order.
**Resembles.** [fd](https://github.com/sharkdp/fd), [uutils coreutils](https://github.com/uutils/coreutils).

## 1.2 `ids`: a newtype-ID and typed-handle library  `S`

**Goal.** A small crate providing `Id<T>` (generic over a phantom marker), a `Slab<T>` arena that returns generational handles, and `Display`/`FromStr`/`serde` support, so `UserId` can never be passed where `OrderId` is expected.
**Learn.** `PhantomData`, zero-cost newtypes, `Copy`/`Eq`/`Hash` derive pitfalls with phantom types, generational indices, `compile_fail` doctests.
**Read before.** `language/newtype_ids`, `language/phantomdata_typed`, `memory/slab_generational`. **During.** `memory/arena_ids`, `language/implementing_std_traits`.
**Milestones.**
1. `Id<T>` is `Copy` and 4 bytes even if `T` is not `Copy` (manual impls, not derives).
2. `Slab<T>::insert/get/remove` with generation counters; removed handle returns `None`, reused slot rejects the old handle.
3. `FromStr` + `Display` round-trip (`"user:42"`).
4. Doctests with `compile_fail` proving two ID kinds do not mix.
5. Optional `serde` feature gated in `Cargo.toml`.
**Acceptance.** Property test (seeded, 5,000 ops: insert/remove/get) against `HashMap<u64, T>` oracle; `assert_eq!(size_of::<Id<String>>(), 4)`; `cargo doc` has zero warnings.
**Data.** Op sequences from a seeded PRNG; include the ABA case (remove, insert into same slot, old handle must miss).
**Write-up.** Why can `#[derive(Clone)]` on `Id<T>` wrongly require `T: Clone`? What is the cost of a generation check?
**Stretch.** `Slab::iter` that is stable under removal; a `Drop`-checking debug mode that panics on leaked handles.
**Resembles.** the `slotmap` and `generational-arena` crates (search docs.rs; not linked here), and index-based graphs in [rust-analyzer](https://github.com/rust-lang/rust-analyzer).

## 1.3 `confcheck`: layered config with span diagnostics  `M`

**Goal.** Load config from defaults < file (TOML/JSON) < env (`APP_*`) < CLI flags, validate it, and report problems as compiler-style diagnostics with line/column and a caret.
**Learn.** `serde` defaults and `deny_unknown_fields`, error enums with source chains, span tracking, `Cow<str>`, builder pattern.
**Read before.** `encoding_compression/serde_defaults`, `errors/diagnostics_spans`, `errors/error_chain_retryable`. **During.** `language/cow`, `patterns/typestate_builder`.
**Milestones.**
1. Parse a JSON/TOML file into a typed struct with defaults.
2. Unknown key `retries_max` suggests `retries` (edit distance <= 2).
3. Env and CLI overrides with a provenance map: every field remembers where it came from.
4. Range/regex/duration validation (`"30s"`, `"5m"`) collecting all errors, not the first.
5. `--explain` prints the provenance of every field.
**Acceptance.** 20 golden files in `tests/golden/*.toml` with expected stderr snapshots; fuzz 1,000 random byte mutations of a valid file: no panic, exit code 1 or 0 only.
**Data.** A 40-key sample config; mutate with a seeded generator (delete key, swap type, truncate).
**Write-up.** Why collect errors instead of failing fast? Where do spans live (owned vs borrowed)?
**Stretch.** Live reload on SIGHUP with a diff of changed keys.
**Resembles.** the `figment` crate's layering (docs.rs) and [clap](https://github.com/clap-rs/clap) error output.

## 1.4 `wirefmt`: a typestate protocol client  `M`

**Goal.** Encode a connection lifecycle (`Disconnected -> Connected -> Authenticated -> InTransaction`) in the type system for a fake text protocol, so illegal call orders do not compile.
**Learn.** Typestate, sealed traits, consuming `self`, `#[must_use]`, `compile_fail` tests, sans-I/O design.
**Read before.** `patterns/typestate_protocol`, `patterns/typestate_builder`, `patterns/sans_io_protocol`. **During.** `patterns/generic_state_machine`.
**Milestones.**
1. Core state machine as pure functions `(State, Event) -> (State, Vec<Output>)` with no I/O.
2. Typestate wrapper `Client<S>`; `begin()` exists only on `Client<Authenticated>`.
3. Drive it over an in-memory byte buffer and over `TcpStream` with the same core.
4. `compile_fail` doctests for 5 illegal sequences.
5. A runtime variant (`enum` state) for dynamic use, converted with `TryFrom`.
**Acceptance.** A random sequence generator (seeded, 10,000 sequences): legal sequences never error, illegal ones return `Err` in the runtime variant; both agree with the typestate model.
**Data.** Define 12 verbs (`LOGIN`, `BEGIN`, `COMMIT`...) in a table and generate sequences from it.
**Write-up.** When is typestate worth the ergonomic cost? How does it interact with trait objects and storing clients in a `Vec`?
**Stretch.** Pipelining: queue 100 commands, match replies by order.
**Resembles.** [hyper](https://github.com/hyperium/hyper) connection state, [tokio-rs/mini-redis](https://github.com/tokio-rs/mini-redis) client.

## 1.5 `evbus`: a typed plugin/event bus  `M`

**Goal.** An in-process bus where plugins register handlers for event types (`TypeId`-keyed), with priorities, middleware layers and panic isolation.
**Learn.** `dyn Any` downcasting, type erasure, object safety, `Box<dyn Fn>`, `Send + Sync` bounds, `catch_unwind`, tower-style layers.
**Read before.** `patterns/any_event_bus`, `patterns/type_erasure_vtable`, `language/dyn_vs_generic`. **During.** `patterns/tower_service_layer`, `language/closures_hof`.
**Milestones.**
1. `subscribe::<E>(handler)` and `publish(E)`; handlers run in priority then registration order.
2. Middleware wraps every handler (timing, logging) without handlers knowing.
3. A panicking handler is isolated, reported, and does not stop others.
4. Unsubscribe via a drop guard (`Subscription` RAII).
5. Async variant: handlers return futures, bounded concurrency 8.
**Acceptance.** 1,000 random subscribe/publish/unsubscribe ops vs a `Vec<(TypeId, id)>` oracle; a handler that unsubscribes itself mid-publish does not deadlock or skip others; 100k events/s single thread in release.
**Data.** Three event types, 20 handlers with seeded priorities.
**Write-up.** Why is `publish` on `&self` and how do you avoid holding a lock while calling handlers?
**Stretch.** Typed request/response (`Query<R>`) built on the bus.
**Resembles.** `tokio::sync::broadcast` in [tokio](https://github.com/tokio-rs/tokio) and the `tower` middleware stack.

## 1.6 `minstd`: rebuild `Vec`, `Rc`/`Arc`, `HashMap`  `L`

**Goal.** Implement your own `MyVec<T>` (unsafe, raw alloc), `MyArc<T>` (atomic refcount, `Weak`) and `MyMap<K,V>` (open addressing), each verified under Miri and against `std`.
**Learn.** Unsafe invariants, drop check, `NonNull`, `MaybeUninit`, variance, atomics ordering, amortized growth, hashing.
**Read before.** `interview/implement_std_from_scratch`, `unsafe_ffi/unsafe_basics`, `concurrency/own_arc_refcount`. **During.** `unsafe_ffi/maybeuninit_fixedvec`, `unsafe_ffi/nonnull_linked_list`, `data_structures/open_addressing_map`, `concurrency/atomics_ordering`.
**Milestones.**
1. `MyVec`: push/pop/insert/remove/drain, ZST support, growth 0 -> 4 -> 8 ....
2. `Drop` correctness with a counting type: every element dropped exactly once, also on panic in `clone`.
3. `MyArc` + `Weak`: strong/weak counts, `try_unwrap`, `Send`/`Sync` impls justified in comments.
4. `MyMap` with linear probing and tombstones; resize at load 0.75.
5. Run everything under `cargo +nightly miri test` and a loom model of Arc clone/drop (2 threads).
**Acceptance.** Differential test vs `std` for 5,000 random ops each; Miri clean; zero leaks (counting allocator, see `perf/counting_allocator`).
**Data.** A `DropCounter` type, zero-sized `()` and a 4 KiB struct to hit alignment and ZST paths.
**Write-up.** For each `unsafe` block, state the invariant it relies on. Where would a `Weak` cycle leak?
**Stretch.** `MyVec::drain` that is panic-safe (leak amplification discussion); small-vector optimization with inline capacity 4.
**Resembles.** [rust-lang/rust library/alloc](https://github.com/rust-lang/rust/tree/master/library/alloc), [hashbrown](https://github.com/rust-lang/hashbrown).

## 1.7 `crate-polish`: make a library publishable  `L`

**Goal.** Take any earlier project (for example `ids` or `evbus`) and turn it into a crate you would publish: public API review, docs with runnable examples, feature flags, benchmarks, CI, MSRV, and semver discipline.
**Learn.** API Guidelines checklist, `#[non_exhaustive]`, sealed traits, `cargo-semver-checks`, `cargo deny`, criterion, doc-tests, workspace layout.
**Read before.** `language/library_api_polish`, `language/modules_visibility`, `dev_tools/testing_patterns`. **During.** `perf/bench_harness`, `dev_tools/shrinking`, `language/edition_2024_changes`.
**Milestones.**
1. Run the Rust API Guidelines checklist; file 10 issues against yourself and fix them.
2. Every public item has a doc line, at least 12 have runnable examples, `#![deny(missing_docs)]`.
3. Two cargo features (`serde`, `std`), default off for `serde`; test the full feature matrix.
4. Criterion benchmark for the hot path with a stored baseline; CI fails on >10% regression (script using `critcmp` or your own compare).
5. GitHub Actions: fmt, clippy `-D warnings`, test on stable + MSRV, Miri, `cargo doc` with `-D warnings`, `cargo deny`.
**Acceptance.** `cargo semver-checks` passes between v0.1.0 and v0.2.0 where you intentionally add (not break) API; a deliberate breaking change is caught.
**Data.** Your own crate; a `examples/` dir with 3 realistic usage programs.
**Write-up.** Which 3 API decisions would be breaking to change later? How do you document panics and errors?
**Stretch.** Publish to a private registry or `cargo publish --dry-run`; add a `no_std` mode.
**Resembles.** [serde](https://github.com/serde-rs/serde), [thiserror](https://github.com/dtolnay/thiserror).

---

## Katas (30 minutes each)

1. Make `Id<T>` `Hash` without `T: Hash`; prove with a test that uses a non-`Hash` marker.
2. Convert one `unwrap()` chain in an old project to a typed error with `#[from]`; assert the `source()` chain has length 2.
3. Write `fn parse_duration(&str) -> Result<Duration, E>` for `"1h30m15s"`; reject `"1h 30m"` and `"-5s"` with distinct variants (6 cases).
4. Implement `Display` for a table type so `{:>8}` width flags from the caller are respected (use `f.pad`).
5. Add a `compile_fail` doctest proving a `Typestate<Open>` cannot call `close_twice()`.
6. Replace a `Vec<String>` return with `impl Iterator<Item = &str> + '_` and show with `size_of`/benchmark that one allocation vanished.
7. Write `impl FromIterator<(K,V)> for MyMap` and `Extend`; test with `collect()` on 1,000 pairs vs `HashMap`.
8. Make a function generic over `AsRef<Path>`; call it with `&str`, `String`, `PathBuf`, `&OsStr` in one test.
9. Use `Cow<'_, str>` to normalize line endings only when `\r\n` is present; assert via `matches!(.., Cow::Borrowed(_))` for clean input.
10. Add `#[must_use]` to a builder and write the test that shows the warning (use `trybuild` or a doctest with `#![deny(unused_must_use)]`).
