# How to learn Rust from this cookbook (without just running code)

Running an example teaches you almost nothing: you watch someone else's program succeed. Learning happens when you **predict, struggle,
break things, and rebuild from memory**. This guide turns the cookbook into a training gym. The examples are built for it: every one has a
header (the spec), `assert!`s (hidden tests) and a differential oracle (an answer key you can check yourself against).

## The 7-step loop for one recipe (45-90 minutes)

1. **Pick by need, not by number.** Start from `INDEX.md` "I want to..." or the chapter you are stuck in. If you are doing a FlowForge stage, attempt the
   stage FIRST, then read the recipes its page lists to compare with how others solve the same sub-problem.
2. **Read only the header.** `//! RECIPE`, `LEVEL`, `COMMON USES`, the design notes. Close the file. Write 3 sentences: what problem is this, what is the
   trick, where would I use it in FlowForge?
3. **Predict.** Open `main()` only. Read the `assert!`s: they are the specification. Write down what each one forces to be true and what the printed output
   will be. Wrong predictions are the valuable ones: note them in your notebook.
4. **Attempt it blind.** `./study.sh <name>` makes a private copy in `examples/scratch/<name>_mine.rs`. Delete one or two function bodies (or the whole
   implementation) and make the original asserts pass again WITHOUT looking at the answer. Typing it yourself is the point; never paste.
   Stuck for 20 minutes? Look at ONE function of the original, close it, and continue.
5. **Run, then read the original side by side with yours.** Where do they differ? Which is simpler? Which handles an edge case you missed? Write one
   improvement you would steal and one thing you would do differently.
6. **Break it on purpose (mutation).** Change one thing and predict the result BEFORE running: remove a `move`, swap `Ordering::Release` for `Relaxed`, change
   `<` to `<=` in a heap, return the wrong lifetime, drop a `Weak`, widen a lock scope. Read the compiler error with `rustc --explain E0xxx`. If the asserts still
   pass, your mutation is invisible to the tests: write the assert that would catch it (that is how you learn what tests are for).
7. **Rebuild from memory tomorrow (and in a week).** Spaced retrieval beats re-reading. Blank the file again and rewrite it. Finish by explaining it aloud
   in 5 lines, or ask your mentor to review your version (correctness, edge cases, idiom, performance).

## Five habits that make it stick

- **Asserts are specs.** Before reading an implementation, ask "what must be true for these asserts to pass?" This is how real engineers read unfamiliar code.
- **Differential thinking.** Every algorithm recipe compares a clever version with a naive oracle. When you write your own code, write the naive version FIRST and
  test the clever one against it on random inputs (seeded RNG, 2,000 cases). It will find bugs in your head before users find them in production.
- **One question per recipe.** End each session by writing a question you could not answer. Look it up in the `Read:` links, the std docs, or the real crate it
  names (`ripgrep`, `tokio`, `std`). Reading real code after you have built a toy version is when it finally makes sense.
- **Make it smaller, then bigger.** Shrink the example to 20 lines that still show the idea (minimal reproduction), then extend it with one real requirement
  (a timeout, a limit, a metric). Both directions teach.
- **Measure, do not believe.** For anything about performance (`perf/`, `hpc/`), predict the ranking of variants first, then run `--release`, then explain
  any surprise. Your intuition improves only when it is checked.

## Which recipes in which order

Full ordered curricula, project-based guides and per-folder reading orders are generated in [`paths/`](paths/README.md): **tracks** (learn a subject from zero), **building guides** (you are building X right now) and **folder guides** (teaching order inside one chapter). The table below is the short version.

| Goal | Path |
|---|---|
| Solid Rust foundations | `language/` (lifetimes, strings, closures, smart pointers, pattern matching) -> `errors/` -> `patterns/` -> `interview/rust_*_qa` as a self-test |
| Think in ownership and memory | `language/borrowck_patterns` -> `memory/` -> `unsafe_ffi/` (only after the above) |
| Concurrency | `concurrency/` atomics and locks -> `async/futures_by_hand` -> `async/` patterns -> `capstones/mini_async_runtime` |
| Data structures and algorithms | `data_structures/` -> `algorithms/` (graphs, dp, strings) -> `interview/` question files as timed practice |
| Systems and databases | `os_and_storage/` -> `database/` -> `capstones/` (kv store, filesystem, SQL engine) |
| Distributed systems | `distributed/` (consistent hashing, CRDT, clocks, SWIM, quorums, Raft) -> `capstones/lease_fencing_sim`, `event_sourced_ledger` |
| Web and services | `web/` (requests and responses) -> `networking/` -> `microservices/` |
| Compilers | `text_and_parsing/` -> `compilers_runtimes/` -> `crates/cb-compiler` (the full pipeline, read in the order of its ARCHITECTURE.md) |

## Using the interview chapter as a retrieval-practice engine

Each file lists its questions as `//! Qn.` lines (see `examples/interview/QUESTIONS.md`). Cover the code, take a question, set a 25-minute timer, solve it in
`examples/scratch/` with a brute-force oracle plus random tests, THEN compare. Track which questions you solved without hints; redo the rest next week.

## Tools to use while studying

`cargo test` / `cargo run --release --example X` (release matters for anything about speed) - `rustc --explain E0502` - `cargo expand` (see what macros
generate) - `cargo asm` (see what the compiler did) - `RUST_BACKTRACE=1` - `dbg!` - `cargo clippy` (every warning is a mini-lesson) - Miri for unsafe code
(`rustup +nightly component add miri`, `cargo +nightly miri run --example X`) - see `TOOLING.md`.

## A realistic 4-week plan (about 6 hours/week)

- **Week 1: ownership and errors.** 6 recipes from `language/` + `errors/`; interview `rust_ownership_borrowing_qa` as a closed-book quiz. Finish FlowForge stages 1-3.
- **Week 2: data structures.** Heaps, tries, LRU, graphs; implement each yourself from the Q-lines, compare with the cookbook. Finish stages 4-6.
- **Week 3: concurrency and performance.** `atomics`, `locks_deadlocks`, `bounded_channel`, `worker_pool`, then measure `cache_locality` and `false_sharing`; stage 7-8 plus your own benchmark.
- **Week 4: build something.** Pick a capstone, read its header, build the smallest slice of it yourself (a WAL; a regex engine; a rate limiter), then compare.

If you only do one thing differently: **close the file and write it yourself before you read the answer.**
