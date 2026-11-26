# anneal · roadmap and analysis

> **2026-10-07:** the owner dropped the Build section (B, M tracks) and AI features, and DSA became a NeetCode
> tracker; see [DSA.md](DSA.md). System design (LLD, HLD, API design) and behavioral are deferred. Plans below that
> involve B/M tracks or custom DSA content are superseded.

Written 2026-09-27 to capture requests that go beyond the current plan, so none of them get lost.
[PLAN.md](PLAN.md) holds the decisions and architecture, [CURRICULUM.md](CURRICULUM.md) the track-by-track content.
This file covers what comes next, why, and in what order.

The requests it covers:

1. Dashboard-style pages: streaks, a calendar, stats, progress over time, spaced repetition.
2. Complex Rust problems that combine lifetimes, generics, the borrow checker and traits.
3. More algorithm coverage: niche but useful graph, DP and tree algorithms, and other sections.
4. Rust design patterns and practice.
5. Microservice patterns: do they fit, and how can they be taught?
6. Everyday Rust coverage: async/futures/pinning, threads, channels, the std functions you use daily, I/O,
   strings/bytes/slices, bits, matrices and math, common traits and generics, popular crates, CLI parsing,
   event loops and sockets (§9).
7. ~~An AI assistant (Gemini API)~~: built, then removed on 2026-10-07 (§11).

---

## 0. Where things stand

**Platform:**
- Built:
  - the workspace (hybrid 1a + 1c, rust-analyzer, borrow lanes, rules, run timeline);
  - the section catalog pages (next up, streak, this week, recent);
  - the track page;
  - login with a passphrase;
  - editor settings (font, size, Vim with `jk`/`kj`, 4-space indent);
  - the Output panel;
  - difficulty colours.
  - Progress dashboards and spaced repetition; accent colour setting;
  - the workspace console, contextual Run / Run tests, layout icons (see [HANDOFF.md](HANDOFF.md) §2).
- Designed but not built: Today, Library, Mock interview, Readiness, and the project pages.
- **Handoff and pending work:** [HANDOFF.md](HANDOFF.md).

**Content:** 22 tracks, 500 verified problems (`anneal verify`, 0 failures). Every problem meets the test bar.
Finished: D1–D5, D7–D12; L1–L5 (L1–L3 raised to the senior bar); S1–S3; F2. Partly written: D6 (deferred), S4.

| Section | Written | Problems |
|---|---|---|
| L Language | L1–L5 | 20 + 37 + 22 + 22 + 16 |
| S Standard library | S1–S4 | 17 + 18 + 20 + 11 |
| D DSA | D1–D12 | 24 + 18 + 14 + 14 + 15 + 8 + 13 + 29 + 58 + 25 + 28 + 57 |
| F Performance | F2 | 14 |

**Crates:** problems can depend on a fixed crate set, done 2026-09-27 (§6.1). The set is listed in
`docker/deps/Cargo.toml`: tokio, futures, bytes, tower, axum, serde, serde_json, toml, anyhow, thiserror,
tracing, clap, rand, regex, chrono, itertools, crossbeam, parking_lot, dashmap, rayon, turmoil, proptest and
more. A problem opts in with `crates = [...]` in problem.toml. The runner image vendors the set at build time,
so sandboxed runs stay offline (`--network none`) and reproducible.

---

## 1. Dashboards and spaced repetition

### 1.1 What to build

A **Progress** area (new nav item next to Readiness) plus the already-designed **Today** and **Readiness**
pages. They share one data layer.

| Page | Widgets | Answers |
|---|---|---|
| **Today** (designed) | due re-solves · resume points · next up per section · streak · today's goal | What should I do right now? |
| **Progress** (new) | calendar heatmap (52 weeks) · current / longest streak · solved over time · readiness over time per area · time on task · weekly summary | Am I putting in the work, and is it paying off? |
| **Stats** (tab of Progress) | solved by level / section / mode · unassisted rate · first-run pass rate · runs per solve · median time to solve vs interview budget · **most frequent rustc error codes** · **most-hit clippy lints** · **rule violations** (e.g. `clone`) | Where am I weak, specifically in Rust? |
| **Reviews** (tab of Today) | due today · overdue · 14-day forecast · retention rate · per-item interval history | Am I keeping what I learned? |
| **Readiness** (designed) | per-track tiles · per-area trend · gates (SDE-2 / SDE-3) | Am I ready for the interview? |

The Rust-specific stats are what set this apart from a generic tracker. The run history already stores every
diagnostic, so "you hit E0502 31 times this month, mostly in L2 split borrows" is a query, not a feature.

### 1.2 Spaced repetition

PLAN.md fixes the ladder:
- An assisted solve is due again in 3 days.
- Each unassisted re-solve moves the interval 3 → 7 → 21 → 60 days.
- An assisted re-solve resets it to 3.
- An unassisted first solve gets one retention check at 21 days.
- Readiness credit decays 10 % per overdue week (floor 0.25).

Proposal: ship that ladder first, because it's predictable and easy to reason about. Keep the schedule behind a
trait so FSRS can replace it later without a migration. FSRS fits intervals to your own recall history, and
Anki now uses it.

| Piece | Design |
|---|---|
| `reviews` table | `problem_id` · `step` (0–4) · `due_at` · `last_result` (unassisted / assisted / failed) · `history jsonb` |
| Re-solve attempt | `attempts.kind = 'resolve'` (the column already exists) · the buffer starts from the **starter**, not your old code · hints are locked again · the solution stays unlocked but viewing it makes the re-solve assisted |
| Scheduling | on every solve: update `reviews`. Pure function `next(step, result, solved_at) -> (step, due_at)`, unit-tested |
| Readiness | credit × decay(overdue weeks), computed in `views::readiness` (the hook is already there) |
| Queue order | overdue first (oldest), then due today by track weight (core ×2), capped at a daily budget (setting, default 6) |
| Surfacing | Today page queue · a "re-solve due" box on section rails (deliberately left out of the port until this exists) · a badge in the header |

### 1.3 Data model additions

| Table / view | Purpose |
|---|---|
| `reviews` | the schedule, above |
| `readiness_snapshots` | one row per area per day (PLAN.md lists it); written on the first request of the day, or by a daily task |
| `focus_time` | seconds of active editing per attempt: the workspace sends a heartbeat every 30 s while the editor is focused and the tab visible. Wall-clock `solved_at − started_at` overstates time badly |
| view `activity_days` | solves per local day, for the heatmap and streaks (the activity endpoint computes this today; move it to SQL once the history grows) |

### 1.4 Charts

Follow the dataviz rules:
- one axis per chart;
- sequential single-hue ramps for the heatmap;
- the difficulty colours only for difficulty;
- a table view behind every chart.

Mock up first (rule D6): Progress, Stats and Reviews get screens in `anneal-screens.html`
before any code.

---

## 2. Complex Rust: problems that combine concepts

Interviews at SDE-3 rarely ask about one feature in isolation. The hard part is when lifetimes, generics,
trait bounds and the borrow checker interact. Proposal: a new section **X · Rust in combination** with two
tracks.

### X1 · Putting it together: core · 20

Each problem names the concepts it combines. Easy → hard:

| Band | Problem | Combines |
|---|---|---|
| Easy | Generic `Cache<K, V>` with `get(&Q)` where `K: Borrow<Q>` | generics · `Borrow` · `HashMap` |
| Easy | `Interner<'a>`: `&'a str` ↔ `Symbol` ids | lifetimes · newtypes · maps |
| Easy | Typestate request builder with `PhantomData` markers | generics · zero-sized types · ownership |
| Medium | Zero-copy CSV reader: `Record<'a>` with typed `get::<T: FromStr>` | lifetimes · generics · `FromStr` · iterators |
| Medium | Borrowed AST + `trait Visitor<'src>` | lifetimes on traits · enums · recursion |
| Medium | Typed arena: `alloc(&self, T) -> &'a mut T` | interior mutability · lifetimes · `unsafe`-free design |
| Medium | Event bus with `Weak` subscribers and `Box<dyn Fn(&E) + 'a>` | trait objects · lifetimes · `Rc`/`Weak` |
| Medium | Graph over generic payloads: `neighbors(&self) -> impl Iterator<Item = &N> + '_` | RPIT capture · generics · borrowing |
| Medium | Monoid-generic segment tree: `trait Monoid { const ID: Self; fn op(..) }` | associated consts · generics · operator traits |
| Medium | Fix: a generic function that needs `where for<'a> &'a C: IntoIterator` | HRTB · iterators · bounds |
| Medium | Plugin registry: `HashMap<TypeId, Box<dyn Any>>` with typed getters | `Any` · `'static` · generics |
| Hard | `LendingIterator` with GATs: `windows_mut` over a buffer | GATs · lifetimes · `&mut` |
| Hard | Object safety workaround: a trait with a generic method, used as `dyn` | object safety · `where Self: Sized` · erasure |
| Hard | Mini ECS: `Query<(&A, &mut B)>` over component storages | split borrows · traits over tuples · generics |
| Hard | Async trait `Store` with a borrowed key and `Send` futures | async fn in traits · lifetimes · `Send` bounds |
| Hard | Expression evaluator generic over a `Num` trait with a borrowed environment | operator traits · lifetimes · enums |
| Hard | Spans instead of self-reference: parser owning `Rc<str>` + `Span` tokens | `Rc<str>` · ownership design |
| Hard | `Cow`-based layered config merge, generic over sources | `Cow<'a, T>` · traits · lifetimes |
| Hard | Refactor `Rc<RefCell<_>>` spaghetti into an arena with typed ids | ownership design · generics · borrowck |
| Hard | Capstone: a small query engine (parse → plan → execute) with borrowed rows | everything above |

### X2 · API design and review: SDE-3 · 12

"Design" problems where the tests check how the API is shaped, not just what it outputs. This needs
**compile-fail tests** (§6.2): tests that must *not* compile, like trybuild. For example, "a closed connection
can't be written to", or "you can't build a `Request` without a URL".

Problems:
- Sealed traits.
- `#[non_exhaustive]`.
- Newtype vs type alias.
- Builder vs typestate.
- `impl Into<String>` parameters.
- Returning `impl Trait` vs `Box<dyn Trait>`.
- Error types for libraries.
- Semver: which changes break callers (answered by making a compile-fail test pass).

---

## 3. Algorithms: niche but useful

The D section covers the interview canon (Blind 75 and around it). Proposal: a new section
**A · Advanced algorithms**, mostly SDE-3 tier. Each problem keeps a Rust angle: generic over a `Monoid` trait,
arenas instead of pointers, `u64` arithmetic, iterators.

| Track | Problems (easy → hard) | Why it's worth it |
|---|---|---|
| **A1 Range queries** · 12 | prefix sums 2-D · Fenwick tree (point update, range sum) · Fenwick range update · sparse table (RMQ) · segment tree generic over `Monoid` · lazy propagation · sqrt decomposition · Mo's algorithm · count inversions · 2-D Fenwick | Shows up as "hard" follow-ups; the generic segment tree is also a trait-design exercise |
| **A2 String algorithms** · 12 | rolling hash (`wrapping_mul`) · KMP prefix function · Z-function · Manacher · Aho–Corasick (arena trie) · suffix array + LCP · longest repeated substring · minimal rotation (Booth) | Log scanning, search, dedup: backend-relevant, and a good test of byte-vs-char care |
| **A3 Advanced graphs** · 14 | Floyd–Warshall · Bellman–Ford with negative-cycle report · A* with an admissible heuristic · bidirectional BFS · Hierholzer (Eulerian path, "reconstruct itinerary") · LCA by binary lifting · Euler tour + subtree queries · 2-SAT via SCC · topological DP (longest path in a DAG) · articulation points · stretch: heavy-light decomposition | Completes D9; A* and LCA come up in senior loops |
| **A4 Advanced DP** · 14 | bitmask DP (TSP) · SOS DP · digit DP · tree DP with rerooting · DP over a DAG · profile DP (tilings) · convex hull trick · divide-and-conquer DP · knapsack with bitset (`u64` blocks) · probability/expected-value DP | The "hard" DP patterns interviewers pull from; bitset knapsack is a Rust perf lesson |
| **A5 Probabilistic & streaming** · 10 | reservoir sampling · Fisher–Yates with a seeded RNG · Bloom filter (+ false-positive math) · counting Bloom · count-min sketch · HyperLogLog · Misra–Gries heavy hitters · consistent hashing with virtual nodes · rendezvous hashing · t-digest (stretch) | **Directly useful in backend design**; each doubles as a system-design talking point |
| **A6 Systems data structures** · 14 | ring buffer · LRU (arena + index list) · LFU in O(1) · skip list (arena) · timing wheel · interval tree · rope · B-tree node split/merge · LSM memtable + k-way SSTable merge · Merkle tree + proof · CRDT counters (G/PN) · vector clocks · lock-free SPSC queue (cross-links C3) | The data structures inside databases, schedulers and caches; pairs with the kvlite project |

Trees specifically: D6 covers BSTs and traversals. A1 (Fenwick, segment trees), A3 (LCA, Euler tour) and
A6 (B-tree, interval tree, rope, Merkle) cover the niche tree structures, so there's no need for a separate
"advanced trees" track.

---

## 4. Rust design patterns and practice

M1 (12 idioms) already exists in the plan. Proposal: widen it into three tracks.

| Track | Content | How it's graded |
|---|---|---|
| **M1 Idioms & patterns** · 16 (was 12) | newtype · builder · typestate · RAII guard · strategy (trait vs closure) · command with undo · state machine enum · visitor · fold · extension trait · sealed trait · `mem::take`/`replace` · on-stack dynamic dispatch · interior mutability as a last resort · `Default` + struct-update · iterator as the public API | tests + rules (e.g. forbid `RefCell`) |
| **M3 Anti-patterns & refactoring** · 12 (new) | `.clone()` to appease the borrow checker · `Deref` polymorphism · `Rc<RefCell<_>>` everywhere · stringly-typed APIs · boolean parameters · `unwrap` in library code · over-generic signatures · `&String` / `&Vec<T>` parameters · needless `Box<dyn Error>` in libraries · lock held across `.await` | **fix mode**: behaviour tests stay green, and rules/lint gates prove the smell is gone (`max_changed_lines`, `forbid_methods`, clippy lints required clean) |
| **M4 Code review** · 10 (new, needs a new mode) | read a diff, find the bugs: data race, lock order, off-by-one, panic path, leaked task, unbounded channel, missing timeout, SQL injection through `format!` | a **review mode**: mark lines and pick issue types; graded against an answer key, with partial credit |

Review mode is the only new interaction here. It's also the closest thing to what SDE-3 interviews
increasingly include ("review this PR").

---

## 5. Microservice patterns: do they fit, and how to teach them

**They fit, with one split.** Anneal teaches by making you write code that tests check. Microservice
patterns come in two kinds:

- **Code-level patterns** (retry, circuit breaker, idempotency, outbox, saga state machine) are small, precise
  and testable. They're exactly what backend SDE-2/SDE-3 interviews probe in Rust, and Rust expresses them
  well: enums for saga states, `tower::Layer` for resilience, types for idempotency keys.
- **Architecture-level patterns** (service boundaries, choreography vs orchestration, data ownership, API
  gateway, service mesh) aren't unit-testable. They belong in system design (H) with a rubric.

### 5.1 What's already planned

B2 has the rate-limit and concurrency-limit layers. B3 has `SKIP LOCKED` and the transactional outbox. B5 has
retries with jitter, idempotency keys, the circuit breaker, fencing-token leases and the outbox relay. B6 has
tracing, health vs readiness and graceful shutdown. The jobq project covers leases, retries, idempotency and
the outbox. That's about half the canon.

### 5.2 New track: B7 · Distributed & microservice patterns · 16

| Band | Problems |
|---|---|
| Easy | deadline propagation (`Instant` budgets across calls) · bulkhead with a semaphore per dependency · health vs readiness vs liveness · backpressure with bounded channels |
| Medium | request hedging · singleflight / cache-stampede protection · power-of-two-choices load balancing · idempotent consumer (inbox table / dedupe window) · versioned events with serde (schema evolution) · CQRS read-model projection |
| Hard | saga orchestrator as a state-machine enum with compensations · event sourcing: fold events → state, snapshots · outbox relay with at-least-once delivery + consumer dedupe = effectively-once · distributed lock with fencing tokens under a partition · consistent-hash sharding with rebalancing · (★) end-to-end: order → payment → inventory saga with chaos tests |

### 5.3 How to make it testable in the sandbox

The sandbox has no network and no Postgres, and must be deterministic. Each pattern gets:

1. **In-process services.** A "service" is a `tower::Service` or an async trait object; the network is a
   function call. That's enough for retry, breaker, bulkhead, hedging and singleflight.
2. **Injected faults.** A `Flaky<S>` wrapper with a **seeded** RNG (fail N % of calls, add latency, drop
   replies). Seeds make failures reproducible, so a flaky test is a bug in the solution, not in the test.
3. **Deterministic time.** `tokio::time::pause()` + `advance()`: a 30-second backoff test runs in
   microseconds, and "after 3 failures within 10 s the breaker opens" is exactly checkable.
4. **Simulated networks for the hard ones.** The **turmoil** crate (Tokio's deterministic simulation):
   several hosts in one test, with partitions, latency and message loss, all reproducible from a seed.
   Sagas, fencing tokens and outbox relays under partitions become ordinary tests.
5. **Storage without Postgres.** SQLite in memory (`rusqlite`, bundled) for outbox/inbox tables, or an
   in-memory store behind a trait. Real Postgres behaviour stays in B3 and the projects, which run outside the
   sandbox's no-network rule (§6.1).
6. **Architecture patterns** go to H3 system design scenarios (e.g. "design checkout with a saga"), graded
   by a rubric once H's practice mode is decided.

**Project idea: `ordersvc`** (SDE-3, after jobq). Three in-process services (orders, payments, inventory)
over turmoil, stage by stage:
- idempotent APIs;
- outbox;
- saga with compensations;
- CQRS read model;
- chaos suite (partitions, duplicates, reordering);
- tracing across services.

This is the "microservices" portfolio piece, and every stage is testable.

---

## 6. Platform prerequisites (runner and app)

| # | Feature | Unblocks | Notes |
|---|---|---|---|
| 6.1 | ✅ **Done 2026-09-27.** **Vendored crates in the sandbox**: a fixed crate set pre-built into the runner image and used `--offline`; problem.toml lists which ones it uses. Set: tokio, futures, bytes, tower, axum, serde/serde_json/toml, thiserror, anyhow, tracing, clap, regex, rand/rand_chacha, time or chrono, itertools, crossbeam, parking_lot, dashmap, rayon, rusqlite, turmoil, proptest, uuid, hex, base64, sha2, walkdir | C1–C4, C6, B1–B7, M2, K1–K3, projects, A5 | Build the deps once in the image so first compile stays fast; pin versions per image tag |
| 6.2 | **Compile-fail tests**: `tests/compile_fail/*.rs` + expected error code, runner reports pass if it fails to compile with that code | X2, typestate problems, API design | Like trybuild, but we already parse rustc JSON |
| 6.3 | **Per-problem clippy gates**: `[rules] require_clean_lints = ["clippy::clone_on_ref_ptr", …]` | M3 anti-patterns | clippy JSON is already collected |
| 6.4 | **Review mode** (new problem mode) | M4 | new UI: diff view, line marks, issue picker |
| 6.5 | **Focus-time heartbeats** | Stats, time on task | tiny endpoint + visibility/focus listener |
| 6.6 | **Miri in the image** (nightly toolchain, optional) | Y2 unsafe, D5 unsafe list, C3 | per-problem `miri = true` |
| 6.7 | **Longer test time limits for chaos/turmoil suites** | B7 hard | per-problem override |

---

## 7. Suggested order

| Step | What | Why this order |
|---|---|---|
| 1 | **Finish the SDE-2 core content**: D6, D7, D8, D12 (DP), D10, D11, D13, D14; L4–L8; S5–S9 | The interview canon comes first; the dashboards need data to be useful |
| 2 | ✅ Spaced repetition + Progress (overview, Rust stats, reviews), built 2026-09-27 · Today and Readiness pages still to build | With ~300 problems written, keeping them matters more than adding more. Reviews make the platform "anneal" |
| 2a | **Test hardening** of all written problems: ≥5 visible (LeetCode's examples included) and ≥8 hidden tests, edge checklists, seeded randomized brute-force comparisons, scale tests, `wrong/*.rs` solutions the verifier requires to fail · **in progress:** verifier + `Rng` prelude built, D1 done 2026-09-27; D2 onward next | Every later step leans on "passes" meaning correct |
| 2b | **Q concept cards** (card mode: mockup → build) + the §10.2 additions to L4, L5, S4, S6, S7 | The spoken half of interviews; reuses the review queue (§10) |
| 2c | ~~AI assistant~~ (§11): removed by the owner on 2026-10-07 | |
| 3 | ~~6.1 vendored crates~~ done | Unblocked the concurrency/backend half of the curriculum |
| 3b | **F · Performance Rust** (CURRICULUM §6.1), agreed 2026-09-27: first the runner support (`[perf]` in problem.toml, release builds, counting allocator, timing helpers, assembly checks, new crates), then F2 and F4 (exact checks), then F1, F3, F5–F7 | Owner's request: HPC, compilers, databases, kernels |
| 4 | C1–C4, B1–B6, M1–M2 content; shortline project | The backend SDE-2/SDE-3 core |
| 4b | S12 bytes & encodings, S13 std drills (std-only, can start any time) · K1–K3 crates, grepr and chatd projects (after step 3) | Everyday fluency; §9 |
| 5 | **X1 Rust in combination** + 6.2 compile-fail tests + X2 | SDE-3 depth; the interviews' hardest Rust questions |
| 6 | B7 microservice patterns + `ordersvc`, with turmoil | Builds on 3–4 |
| 7 | A1–A6 advanced algorithms | SDE-3 tier; A5/A6 double as system-design material |
| 8 | M3 anti-patterns, M4 review mode (6.3, 6.4) | Practice-oriented polish |
| 9 | Y2, Y3, Y5 systems, S11, C3, C5 with Miri (6.6) (Y1 and Y4 moved into F) | The SDE-3 systems path |

Rough size: +20 (X1) +12 (X2) +76 (A1–A6) +16 (B7) +22 (M1 widening, M3, M4) +78 (§9: S12, S13, K1–K3, C4/C5/B4/D13
additions) ≈ 224 problems beyond CURRICULUM.md's 400-odd, plus the grepr and chatd projects.

### 7.1 Sections planned for later (owner, 2026-09-27; not scheduled yet)

Each becomes a CURRICULUM section with the owner's senior bar (HANDOFF §4) once the order above reaches it. Most are
challenge sets built on real scenarios, and several reuse F's runner support for measuring.

| Section | What it would cover |
|---|---|
| HFT / low latency | order books, market-data feed handlers (binary protocols, ITCH-style), lock-free queues between pinned threads, latency histograms, allocation-free hot paths |
| Cryptography | constant-time comparison, hashing and MACs, AEAD with the RustCrypto crates, key handling and zeroize, Merkle trees, what not to write yourself |
| CLI tools | clap, exit codes, streaming stdin/stdout, signals, progress output, config layering: ripgrep/fd-style tools |
| DBMS internals | pages and buffer pool, B+tree, LSM (memtable, SSTables, compaction), WAL and recovery, MVCC, a query executor (volcano vs vectorized); extends kvlite |
| Blockchain | hashing chains, Merkle proofs, UTXO vs account state, signatures, a toy consensus/gossip; ties to the old Web3 packet (W3-1–12) |
| Advent of Code | timed puzzle sets in idiomatic Rust: parsing with `split`/`nom`, grids, simulation, memoization; a speed-solving mode |
| `no_std` | `core` vs `alloc`, `#![no_std]` libraries, panic handlers, fixed-capacity collections (heapless), embedded-style drivers (was Y6 in §10.2) |
| Testing & benchmarking | unit/integration/doc tests, proptest, fuzzing, snapshot tests, mocking, Miri and loom (grows out of Y5); criterion / divan / iai-callgrind benchmarks, statistics of benchmark noise, CI regression gates |

**Cookbook** (owner, 2026-11-26): a reference library. First release built (guide pages and samples); meaning search, related pages and more subjects are not built. Full plan: [COOKBOOK.md](COOKBOOK.md).

---

## 8. Open questions

- **FSRS vs the fixed ladder**: start with the ladder (decided above unless you say otherwise), swap later?
- **Daily review budget**: default 6 re-solves a day?
- **Section X / A naming and tier**: X1 core (SDE-3 path) and A tracks SDE-3/light, as proposed?
- **Review mode (M4)**: worth the new UI, or keep code review as fix-mode problems?
- **H system design practice mode**: still open from CURRICULUM.md §12; B7's architecture half depends on it.

---

## 9. Coverage check: everyday Rust

The request: *async + futures + pinning, multithreading, channels, the std functions you use daily, reading
and writing, strings, bytes, slices, bit manipulation, matrices, math, common traits, generics use cases,
common crates (anyhow, thiserror, serde, tokio, rand, …), CLI parsing, event loops, network sockets.*

Most of this is already planned; this table shows where each piece lives and what's added.

| Topic | Lives in | Status | Added here |
|---|---|---|---|
| async/await, Tokio | C4 Async & Tokio (26) | planned; needs 6.1 | tokio::sync tour (Notify, watch, broadcast, oneshot) · `tokio::io` split/copy · `JoinSet` supervision |
| Futures, `Pin`, wakers | C5 Async internals (12), minirt project | planned | **`Stream` by hand** · **pin projection without macros** (then `pin-project-lite`) · `Pin<Box<dyn Future>>` recursion · **async closures** (Rust 2024) · cancel-safety of `select!` branches |
| Threads, locks | C1 Threads & shared state (22) | planned, std-only (can start now) | — |
| Channels | C2 Message passing (16) | planned, std-only | crossbeam `select!` and bounded channels (after 6.1) |
| Atomics | C3 (14) | planned | — |
| Everyday std functions | spread over S1–S11 | S1–S4 written | **S13 std drills** (below) for speed and fluency |
| Reading & writing | S9 I/O & filesystem (14) | planned, std-only | `Path`/`PathBuf` manipulation · `fs::read_dir` recursion · atomic file replace (write temp + rename) |
| Strings | S2 (18) | **written** | — |
| Bytes & binary data | partly S3, L3 (frame parser), D13 | gap | **S12 Bytes & encodings** (below) |
| Slices | S3 (20) | **written** | — |
| Bit manipulation | D13 Bits stage | planned | bitflags-style sets by hand · `u64` bitsets (links A4 bitset knapsack) |
| Matrices & math | D13 Matrix & arithmetic, number theory | planned | flat `Vec<f64>` matrix with `Index<(usize, usize)>` · matrix multiply + fast exponentiation (Fibonacci in O(log n)) · `rem_euclid` vs `%` · `f64::total_cmp`, epsilon comparisons · `u128` intermediate products |
| Common traits | S8 Core traits (16), L4 Traits (22) | planned | — |
| Generics use cases | L5 Generics (16), X1 (§2) | planned | — |
| Error crates: anyhow, thiserror | L8 Error design (16) | planned; needs 6.1 | — |
| serde, rand, regex, clap, time, itertools… | — | **gap** | **K1–K3 ecosystem crates** (below) |
| CLI parsing | S10 (`env::args`, exit codes) | planned | clap in K2 · **grepr** project |
| Event loops | C5 reactor, minirt | planned | std-only `poll`-style loop over non-blocking sockets · **chatd** project |
| Network sockets | B4 Networking (14) | planned | **std-only** `TcpListener`/`TcpStream` (blocking, timeouts, `set_nonblocking`) · **UDP** request/response with retries · line protocol over TCP |

The sandbox runs with `--network none`, but the loopback interface still exists. Tests can bind
`127.0.0.1:0` and talk to themselves, so socket problems work without opening the network.

### S12 · Bytes & encodings: core · 14 (new, std-only)

| Band | Problems |
|---|---|
| Easy | `to_be_bytes` / `from_le_bytes` round trips · hex encode/decode by hand · `&[u8]` vs `&str` vs `Vec<u8>`: converting without copying (`from_utf8`, `as_bytes`) · checked slicing with `get(..)` |
| Medium | base64 by hand · varint / LEB128 · bit-packed header fields (shifts and masks) · CRC-32 with a lookup table · reading a length-prefixed record stream from any `Read` · `Cow<[u8]>` for maybe-decoded payloads |
| Hard | zero-copy binary parser for a TLV format with lifetimes · a `BitReader` / `BitWriter` over `&[u8]` · UTF-8 decoder by hand (validate + decode, reject overlongs) |

### S13 · Std drills: light · 30 (new, std-only, a "drill" format)

Short (2–5 minute) exercises for the functions you should type without looking up. They're graded like
write-it problems but shown as a timed set of five, and a streak of fast, clean solves counts as fluency.

Groups:
- **Iterators:** `windows`, `chunks`, `zip`, `scan`, `flat_map`, `group by` with `fold`, `sum::<Option<_>>`, `max_by_key` + `Reverse`.
- **Collections:** `entry`, `retain`, `dedup_by_key`, `sort_unstable_by_key`, `binary_search_by`, `BTreeMap::range`, `VecDeque::rotate_left`, `split_off`, `drain`, `extend_from_slice`.
- **Strings:** `split_once`, `char_indices`, `trim_matches`, `parse::<T>`, `format!` specs, `strip_prefix`, `lines`.
- **Option/Result:** `ok_or_else`, `transpose`, `and_then`, `unwrap_or_default`, `?` in closures (can't: rewrite).
- **Memory:** `mem::take`, `swap`, `replace`.
- **Numbers:** `checked_*`, `saturating_*`, `abs_diff`, `div_ceil`, `ilog2`, `count_ones`, `leading_zeros`.

### K · Ecosystem crates (new section; crates available since 6.1)

Each problem is "use the crate idiomatically", with a follow-up on how it works inside.

| Track | Problems |
|---|---|
| **K1 serde & data formats** · 14 | derive + `rename_all` · `#[serde(default)]` and optional fields · enum representations (external, internal `tag`, adjacent, untagged) · `flatten` · `with`/`deserialize_with` for custom fields · borrowed deserialisation `&'a str` (zero-copy) · `serde_json::Value` vs typed · TOML config with validation · versioned structs (schema evolution) · a hand-written `Deserialize` with a `Visitor` (★) |
| **K2 CLI & utility crates** · 16 | clap derive: args, flags, subcommands, `ValueEnum`, validation · `anyhow` main with `.context()` · `thiserror` for a library + `anyhow` in its binary · `rand`: seeded `StdRng`, `shuffle`, `choose`, weighted sampling, `rand_chacha` for reproducibility · `regex`: captures, named groups, `replace_all` with a closure, `RegexSet` · `time`/`chrono`: parse, format, durations, time zones · `itertools`: `group_by`, `tuple_windows`, `kmerge`, `sorted_by_key` · `walkdir` + filtering · `uuid`, `hex`, `sha2` · `indicatif`-style progress (stretch) |
| **K3 Async & concurrency crates** · 12 | `futures`: `StreamExt`, `buffer_unordered`, `FuturesUnordered` · `bytes`: `Bytes`/`BytesMut` and cheap slicing · `tokio-util` codecs (`LengthDelimitedCodec`) · `crossbeam::channel` + `select!` · `parking_lot` vs std locks · `dashmap` vs `RwLock<HashMap>` · `rayon` custom pools · `tracing` + `tracing-subscriber` layers |

### New projects

| Project | Stages | Path | Built from |
|---|---|---|---|
| **grepr**: ripgrep-lite CLI | clap args → search one file → recursive walk (walkdir) → regex → parallel search (rayon) → `.gitignore`-ish filters → coloured output + exit codes → benchmarks | SDE-2 | S9, S10, K2, C6 |
| **chatd**: chat server | std blocking TCP, one thread per client → line protocol + commands → non-blocking sockets + a hand-written event loop → rooms + broadcast → Tokio rewrite → backpressure + slow-client eviction → graceful shutdown | SDE-2/3 | C1, C2, C4, C5, B4 |

---

## 10. Interview topic audit

Asked 2026-09-27: "did we miss any Rust topics interviewers might ask?" This checks the usual SDE-2/SDE-3 Rust
interview ground against CURRICULUM.md and this roadmap. Most of it is covered. The gaps below are now part of
the plan.

### 10.1 Already covered (no action)

| Area | Where |
|---|---|
| Ownership, moves, `Copy`/`Clone`, drop order | L1, S8 |
| Borrowing, NLL, reborrows, two-phase borrows, split borrows, Polonius cases | L2 |
| Lifetimes: elision, structs, `'static`, variance, HRTB, `impl Trait` capture | L3 |
| Traits: static vs dynamic dispatch, object safety, operators, orphan rule, blanket impls, GATs | L4 |
| Generics, associated types, typestate, `PhantomData`, monomorphization cost | L5 |
| Closures and `Fn*` traits · enums and patterns · error design (`thiserror`/`anyhow`, panics, unwinding) | L6, L7, L8 |
| Modules, workspaces, features, semver · declarative and procedural macros | L9, L10 |
| `Option`/`Result`, strings/UTF-8, `Vec`/slices, maps/sets, heaps/deques, iterators | S1–S6 |
| Smart pointers, interior mutability, `Weak`, core traits, I/O, time/processes, `mem`/`ptr`/alloc | S7–S11 |
| Threads, `Send`/`Sync`, locks, `Condvar`, channels, actors, atomics and orderings, lock-free, rayon | C1–C3, C6 |
| async/await, Tokio, `select!`, cancellation, `Pin`, wakers, executors, `Stream` | C4, C5, §9 |
| Unsafe, aliasing, Miri, FFI, testing, fuzzing, loom · layout, niches, `repr`, allocation, hashing, SIMD, performance | Y2, Y3, Y5 · F1–F7 |
| axum, tower, sqlx, networking, resilience, observability, serde, clap, rand, regex | B1–B6, K1–K3 |
| Design patterns and anti-patterns · DSA canon · niche algorithms | M1, M3, D1–D14, A1–A6 |

### 10.2 Gaps, and where they go

| Gap | Why interviewers ask | Goes into | + problems |
|---|---|---|---|
| **Conceptual questions** ("explain `Pin`", "`Rc` vs `Arc`", "why is `String` not `Copy`", "what does `Send` mean for a future", "how does `?` desugar") | Every Rust interview has a spoken part; the platform only drills code | **new section Q · Concept questions** (§10.3) | ~180 cards |
| Const generics, `const fn`, `[T; N]` generic impls, zero-sized types | Common "do you know modern Rust" check | L5 · new stage *Compile-time Rust* | +4 |
| `Sized`, `?Sized`, DSTs (`str`, `[T]`, `dyn Trait`), fat pointers, vtables | Shows you understand what `&dyn Trait` actually is | L4 · new stage *Sized & DSTs* | +3 |
| Method resolution: auto-ref/deref, fully qualified syntax `<T as Trait>::f`, ambiguous methods, turbofish | "Why does this call pick that method?" | L4 · stage *Static vs dynamic* | +2 |
| Statics and globals: `const` vs `static`, `static mut` (and why 2024 warns), `OnceLock`/`LazyLock`, `thread_local!`, a global `Mutex` | Config/singleton questions; common in backend code | S7 · new stage *Globals & lazy init* | +4 |
| Copy-on-write smart pointers: `Rc::make_mut`, `Arc::make_mut`, `Cow` + `ToOwned` | Cheap-clone data structures | S7, S8 | +2 |
| `FromIterator`/`Extend` for your own type (making `collect()` work), `Sum` | Idiomatic custom collections | S6 | +1 |
| Never type `!`, `Infallible`, diverging functions | Occasional "what's the type of `panic!()`" | L7 | +1 |
| Editions and **Rust 2024 changes** (RPIT capture, tail-expression temporary scope, `unsafe extern`, `static mut` refs, let chains) | Staying current; the tail-temporary change bites real code | L9 · new stage *Editions* | +2 |
| `build.rs`, `cfg`/`cfg_attr`, release profiles (LTO, `codegen-units`, `panic = "abort"`), `cargo tree`/`expand` | Build and deploy questions for senior roles | L9, F1 | +3 |
| Hashing: SipHash and HashDoS, `BuildHasher`, FxHash/ahash trade-offs | "Why is Rust's `HashMap` slower than C++'s?" | S4 | +1 |
| Global allocators (jemalloc/mimalloc), allocation profiling | Performance roles | F3 | +1 |
| Concurrency extras: `Barrier`, `thread::park`/`unpark`, `Once`, "shared memory vs message passing" trade-offs | Classic concurrency follow-ups | C1, C2 | +2 |
| Async extras: runtime flavours (`current_thread` vs `multi_thread`), `LocalSet` for `!Send` futures, `async fn` in `dyn` traits (`async-trait` vs native), why there's no async `Drop` | Senior async interviews | C4, C5 | +3 |
| Mocking and snapshot tests (`mockall`, `insta`) | "How do you test code with a database/HTTP dependency?" | Y5, K2 | +2 |
| `no_std`: `core` vs `alloc` vs `std`, `#![no_std]` libraries, panic handlers | Systems/embedded/infra roles | **new Y6 · no_std basics** (light) | +8 |
| gRPC: `tonic` + `prost`, streaming RPCs, deadlines, interceptors | Common in microservice shops | **new B8 · gRPC** | +6 |
| Security basics: constant-time comparison (`subtle`), secrets that zero on drop (`zeroize`), path traversal, SQL injection via `format!`, secure randomness, integer-overflow bugs | Backend interviews increasingly include these | **new B9 · Security** | +8 |
| Python interop with PyO3 | Frequent real-world Rust use (data/ML infra) | Y3 · stage *Build* | +1 |
| WebAssembly (`wasm-bindgen`) | Only for front-end/edge roles | not planned; add if a role needs it | — |

Crates to add to `docker/deps` when those tracks are written: `mockall`, `insta`, `tonic` + `prost` (plus `protoc` in
the image), `subtle`, `zeroize`, `async-trait`, `ahash`, `rustc-hash`, `bitflags`, `smallvec`, `arrayvec`, `memchr`,
`tokio-stream`, `rusqlite` (bundled), `pyo3` (host-only tests).

### 10.3 Q · Concept questions (new section)

Short, spoken-style questions with a model answer, the common wrong answer, and a follow-up. They live in the
same spaced-repetition queue as problems. After revealing the answer you grade yourself: *nailed it / partly /
missed*. Missed and partial cards come back sooner.

| Deck | Examples | Cards |
|---|---|---|
| Ownership & borrowing | move vs copy vs clone · why `String` isn't `Copy` · what NLL changed · aliasing XOR mutation in one sentence | 20 |
| Lifetimes | what `'a` means (a region, not a duration) · elision rules · `T: 'static` vs `&'static T` · variance of `&mut T` | 15 |
| Traits & generics | `dyn` vs `impl` vs generics · object safety rules · coherence/orphan rule · associated type vs parameter · what a vtable holds | 20 |
| Memory & layout | stack vs heap · fat pointers · niche optimisation · `Box<dyn Trait>` size · why `Vec` growth is amortised O(1) | 15 |
| Smart pointers & interior mutability | `Rc` vs `Arc` · `Cell` vs `RefCell` vs `Mutex` · `Weak` · when `RefCell` panics | 12 |
| Concurrency | `Send` vs `Sync` · why `Rc` isn't `Send` · `Mutex` poisoning · memory orderings in plain words · deadlock avoidance | 18 |
| Async | what a `Future` is · what `.await` compiles to · why `Pin` · wakers · cancellation at `.await` · blocking in async · `Send` futures | 20 |
| Errors & panics | `Result` vs panic · `?` desugaring · `thiserror` vs `anyhow` · unwinding vs abort · `catch_unwind` limits | 10 |
| Unsafe & FFI | what `unsafe` does and doesn't allow · common UB · `repr(C)` · who frees across FFI | 12 |
| Performance | zero-cost abstractions · monomorphization vs dynamic dispatch · inlining · allocation avoidance · profiling workflow | 12 |
| Tooling & ecosystem | Cargo features · editions · clippy/rustfmt · workspaces · semver · build scripts | 10 |
| Rust vs other languages | RAII vs `defer`/GC · move semantics vs C++ · goroutines vs async tasks · `Result` vs exceptions | 8 |
| Design & API | when to take `&str` vs `String` · `impl Into<T>` params · builder vs typestate · newtypes · sealed traits | 10 |

Format: `content/concepts/<deck>/<slug>.toml` with `question`, `answer` (markdown), `wrong_answer`,
`follow_up`, `related` (track codes). It needs a small **card mode** in the UI, mocked up before it's built, like
the other screens.

### 10.4 Effect on the order (§7)

- Concept cards go in **right after spaced repetition** (step 2). They're cheap to write, reuse the review
  queue, and cover the spoken part of every interview.
- The track additions in §10.2 are written with their tracks. L4, L5, S4, S6 and S7 are next in the content
  order anyway.
- Y6, B8 and B9 join steps 4 and 9. The extra crates go into `docker/deps` as those tracks are written.
- Total from this audit: about +55 problems, +180 concept cards, and 3 new tracks.

---

## 11. AI assistant (Gemini): removed

**Removed by the owner on 2026-10-07** after being built and deployed: anneal has no AI features. The plan below is
kept only as a record.

Requested 2026-09-27: AI help in the workspace (complexity analysis, suggestions) plus related content and
similarity search, using the user's Gemini AI Studio API key.

### 11.1 Ground rules

- **The key stays on the server.** `GEMINI_API_KEY` in the API's environment; the browser only talks to
  `/api/ai/*`. Without a key, every AI control is hidden and the rest of the app works unchanged.
- **Models are configurable** (`ANNEAL_AI_MODEL_FAST`, `ANNEAL_AI_MODEL_DEEP`): a fast tier for explanations and
  chat, a stronger tier for reviews.
- **Grounded prompts.** Each request carries the problem statement, constraints, the user's code, the failing
  test or diagnostic, and (after a solve only) the reference solution. Replies stream over SSE.
- **Honest progress.** Any AI help before a solve marks the attempt *assisted*, exactly like a hint, so spaced
  repetition (§1.2) isn't fooled. Help after a solve is free.
- **No spoilers by default.** Before a solve the tutor gives nudges, not code; asking for more is an explicit
  escalation that the attempt records.
- **Cached and logged.** Answers are cached by (problem, feature, hash of the code), so repeats cost nothing and
  the free tier's rate limits aren't a problem. Code leaves the machine for Google, which is fine for a
  personal tool; the Settings page says so.

### 11.2 Features

| # | Feature | Where | Notes |
|---|---|---|---|
| AI1 | **Explain this error** | a button on each compiler/clippy card in the console | Plain-words explanation of the diagnostic *in this code*, borrow-checker errors first. Highest value per token |
| AI2 | **Complexity check** | Tests panel, after a passing Submit | Gemini states time/space Big-O with the reasoning; the runner **measures** it too by timing the solution at n, 2n, 4n (per-problem `scale` generator) and fitting the growth. Disagreement is flagged, so the claim is checked, not trusted. Compared against the reference solution's complexity (new `complexity` field in problem.toml) |
| AI3 | **Review** | Tests panel, after a solve | Idiomatic-Rust suggestions beyond clippy: iterator use, needless clones/allocations, ownership in signatures, error handling. Rendered as line-anchored comments in the editor (reuses the lens widget) |
| AI4 | **Tutor chat** | a new left-panel tab | Socratic: asks what you tried, points at the failing case. Sees the same grounded context. Escalation levels: nudge, approach, pseudo-code |
| AI5 | **Similar problems and search** | problem page (Related tab), catalog search box | Gemini embeddings of statement + tags + teaches for every problem, stored in Postgres with **pgvector**. A tags/teaches overlap ranking ships first (no AI needed) and stays as the fallback |
| AI6 | **Targeted practice** | Progress page | Uses the embeddings plus the stats (§1): "most compile errors are E0502 in D9, try these three". Links to concept cards (§10.3) |
| AI7 | **Interview follow-ups** | after a solve; the Mock page later | "What if the input streams?", "make it thread-safe": asks, then grades the spoken-style answer against a rubric |

### 11.3 Order

1. The shared plumbing: config, `/api/ai` proxy with SSE streaming, cache table, the assisted flag. Plus AI1.
2. AI5 without AI (tag overlap), then with embeddings + pgvector (Postgres image needs the extension).
3. AI2 (needs a `scale` generator per problem, which the test-hardening pass (§7 step 2a) adds anyway) and AI3.
4. AI4 tutor chat, then AI6 and AI7.

New UI (the AI panel, review comments, search box) is mocked up before it's built, like every screen.

