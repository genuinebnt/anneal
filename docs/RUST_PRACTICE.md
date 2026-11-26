# The Rust tab as learn-by-doing: plan

> **Owner's decision (2026-11-26): spoilers from the Cookbook are not a concern.** It is fine that a Cookbook page solves a
> practice problem: the owner practises first and opens the Cookbook when stuck, "to get exact or similar or related
> information". Cookbook spoiler handling is dropped from this plan. What replaces it is in section 6: references in three
> levels, closed until the learner opens them.
>
> **Progress:** the gap list is in [rust-practice/GAPS.md](rust-practice/GAPS.md). A first batch of 20 problems is written
> and verified outside the repository, in the FlowForge folder `anneal-staging/rust-practice/` (with `cookbook-refs.toml`).
> It is **not** in `content/` and waits for the owner's review. Integration steps are in that folder's `README.md`.
> The catalogue of everything to build, organised as series with a roadmap and capstones, is
> [rust-practice/BUILD_YOUR_OWN.md](rust-practice/BUILD_YOUR_OWN.md). Six builds from it are staged in `anneal-staging/build-your-own/`.

Plan only. Nothing here is built (apart from the staged problems above). Mockup: [mockups/rust-practice.html](mockups/rust-practice.html) (all progress numbers in it are samples).

The goal, in the owner's words: the Cookbook is reference, so the Rust section should become "a fully practical and learn by doing
section separate from cookbook", using the Cookbook's concepts. Later additions: compiler-error explanations, compare after
solving, build-a-project paths, a code editor that behaves "like a mini version of vscode", and an inventory of what a full IDE has.

Marks used: **[code]** read in the code or content on 2026-11-26 (another session was changing the web app and Cookbook, so line
details may move). **[general]** from general knowledge, not checked here. **[proposal]** a suggestion.

---

## 1. Decisions the owner must make

Each has a recommendation. Nothing starts until the mockup is approved (CLAUDE.md).

| # | Decision | Recommendation |
|---|---|---|
| 1 | "What this teaches", the follow-up and the topic tags show **before** solving, and some state the fix. | Keep them, but closed by default until solved, with "Open anyway (may give the answer away)". Nothing is removed. Decide whether opening it marks the attempt assisted (recommend no). |
| 2 | ~~How to treat Cookbook pages that contain an exercise's solution.~~ | **Decided by the owner: not a concern.** No notice, no folding, no overlap check, no rule against repeating page code. |
| 3 | New exercises in new tracks, or added to existing ones. | New tracks (`L8`, `C4`, `Y5`, `F1`, ...). Adding to an existing track lowers its readiness percentage, because readiness divides by every problem in the track **[code]**. Where an exercise belongs in an existing track (`C1`, `C2`, `Y2`), accept the drop and say so on the track page. |
| 4 | Is the first release below the right size? | Yes: trust fixes, compare after solving, Cookbook links, 6 exercises, small editor items. |
| 5 | Project paths: in the browser, or in the Courses engine (clone and `git push`, as BusTub). | Browser first: one `lib.rs`, 4 stages, no new tables. Use the Courses engine for anything needing several files or real files. |
| 6 | Editor: keep CodeMirror or move to Monaco. | Keep CodeMirror (section 10.4). |
| 7 | rust-analyzer quick fixes (code actions) in exercises. | Off while an exercise is unsolved; on in `main.rs` scratch and after a solve. |
| 8 | Miri (needs a nightly toolchain in the runner image) and compile-fail grading. | Defer both to release 4. Release 1 and 2 need neither. |
| 9 | Show `B4 Networking & protocols` on the Rust tab. | Yes. Today it is not listed there. |

---

## 2. The Rust section today (measured)

Counted with a read-only script over `content/tracks` on 2026-11-26. "Rust problems" means every track except the three
Python practice tracks `P1` to `P3` (34 problems), which belong to DSA.

| Measure | Value |
|---|---|
| Rust tracks | 26 (`L1`-`L5`, `S1`-`S4`, `C1`-`C3`, `C5`, `Y2`, `Y3`, `Y6`-`Y10`, `F2`-`F5`, `F7`, `B4`) |
| Rust problems, all `ready` | **248** |
| By area | Language 117 (L1 20, L2 37, L3 22, L4 22, L5 16), Std 66, Concurrency 15, Systems 19, Networking 3, Performance 28 |
| Mode | write 145, fix 103, stage **0** |
| Level | easy 61, medium 115, hard 72 |
| Problems with fix rules | 66 (forbid methods 54, max changed lines 26, forbid unsafe 8, forbid types 7) |
| Tests per problem | visible 5 to 8 (median 5), hidden 8 to 21 (median 10); 224 use the seeded `Rng` |
| `wrong/` solutions | 847 |
| Hints | 1: 18 problems, 2: 67, 3: 163 |
| Solution notes, follow-up, teaches | all 248 |
| Problems that use a vendored crate | **1** (`f2-inline-edge-lists`, `smallvec`), out of 39 crates in `docker/deps/Cargo.toml` |
| `[perf]` problems | 9 (allocation counting 7, timed release 2, assembly 0) |
| Company tags | 0 on Rust problems |
| Statement length | median 58 words |
| Reference solution length | median 48 lines, 90th percentile 111, longest 327 |

### How a problem is built and graded **[code]**

- Files: `problem.toml`, `statement.md`, `starter.rs`, `solution.rs`, `tests/visible.rs`, `tests/hidden.rs`, `wrong/*.rs`.
- The runner writes a one-library cargo package. It runs clippy, then `cargo test` (visible; Submit adds hidden). `[perf]` adds a
  release build, allocation counting or an assembly file. Limits: compile 90 s, tests 15 s; in Docker 1 GB, 2 CPUs, no network,
  read-only root, 256 processes. Doctests are off. The submission is a single `lib.rs`.
- A fix rule is a syntax check (`crates/rules`). A submit solves only if all tests pass and no rule is broken.
- Any hint or early reveal marks the attempt assisted. Solving records a spaced-repetition review.
- `anneal verify` demands at least 5 visible and 8 hidden tests and 1 wrong solution; the solution passes, the starter does not,
  each wrong solution is caught by a test (not by the compiler), and a write-mode starter compiles.
- `Run` builds a scratch `main.rs` against `lib.rs`.

### The pages **[code]**

- `/rust` (`SectionPage.tsx`): Today steps, status and area filters, Next up card, three goal cards, a roadmap, track cards with
  a bar per stage, and a side column (week, recent, re-solves, readiness).
- `/t/$track` (`TrackPage.tsx`): Learn (path, then for every problem what it teaches and its follow-ups) and Problems
  (filters, sort, keys `j` `k` `/` `[` `]`), reset progress.
- `/p/$id` (`Workspace.tsx`): section 10.
- Header: the Rust area has one tab, "Tracks", and "Library" listed as later.
- Progress page: Rust statistics include the most frequent compiler error codes, lints and broken rules, with where each
  happens most.

---

## 3. Weaknesses, with evidence

| # | Weakness | Evidence **[code]** |
|---|---|---|
| W1 | **The page gives answers away before an attempt.** | The Problem tab always renders `teaches` and `follow_up`; `views.rs` always sends them; the track's Learn tab shows both for every problem. `l2-fix-get-or-insert` teaches "Decide with a `bool`, mutate, then borrow once for the return." `c3-mini-arc` teaches "A relaxed increment on clone, a Release decrement on drop, and an Acquire fence before freeing." |
| W2 | **Two kinds of exercise only.** | 145 write, 103 fix, 0 stage. Nothing asks the learner to write a test, review unsound code, or profile. |
| W3 | **Almost no ecosystem practice.** | 1 of 248 problems uses a crate. No problem uses tokio, serde, thiserror, anyhow, axum, clap, tracing or proptest, all vendored. |
| W4 | **Large gaps.** | No async problem beyond three building runtime pieces (`C5`); no cancellation or `select!`. Error design is a handful of problems across `S1`, `L3`, `L5`; `L8` is planned and unwritten. No problem asks for tests, fuzzing or a benchmark. |
| W5 | **Thin tracks.** | `C2`, `Y6`, `Y8`, `Y10` and `F7` have 2 problems each; six more have 3. |
| W6 | **`B4` is not on the Rust tab.** | `NAV_SECTIONS.rust` is `L,S,C,Y,F`; `PLANNED` has no `B4`; the Progress `rust` area also leaves out `Backend`. Its 3 problems show in ⌘K but not on `/rust`. |
| W7 | **A new track would be invisible.** | `/rust` builds its cards only from the hand-kept list `PLANNED` in `curriculum.ts`. That list also holds written counts as "planned", so unwritten tracks (`L6`-`L10`, `S5`-`S11`, `C4`, `F1`) never show. |
| W8 | **Related is a dead end.** | The Related tab prints a code and the section name, with no link. 164 of 487 `related` references point to unwritten tracks (126 problems; `S7` 21, `S8` 17, `S6` 16, `L8` 10). |
| W9 | **No way from a problem to the concept.** | No problem field names a Cookbook page, and no link from `content/cookbook` to `/p/` was found. |
| W10 | **After a solve there is little to learn from.** | The reference opens as a read-only tab with notes and complexity. It is never set beside the learner's code. |
| W11 | **Stale records.** | `HANDOFF.md` section 2 still counts the retired `D1`-`D12` tracks (listed in `content/retired.txt`). |
| W12 | **Editor gaps.** | Section 10. |

---

## 4. Cookbook concepts against practice

**How thorough:** I read the opening of all 248 Rust statements (first 450 to 700 characters; the median statement is 58 words,
but a good number are longer and were cut) and matched them by hand to the chapters in `tools/cookbook/outline.json`. I did not
read all 248 solutions or all Cookbook pages. A mapping means "a learner writes code that uses this idea", not "covers the
chapter". The Cookbook had about 700 page files and was still changing.

Depth: **Good** (many problems, easy to hard), **Part** (some), **Thin** (one to three, or only by building internals),
**None**. Counts such as "L1 (20)" are whole tracks.

### Rust, standard library, crates

| Chapter | Practised by | Depth | Missing |
|---|---|---|---|
| Ownership and borrowing | `L1` (20), `L2` (37), `L3` (22) | Good | |
| Types, traits and generics | `L4` (22), `L5` (16) | Good | |
| Everyday language | closure fixes in `L1`-`L4`; `l4-iterator-extension`, `s3-pairs-iterator`, `l2-stride-iter-mut`, `s2-zero-copy-split-iterator`; `s1-my-result-and-try` (one macro) | Thin | macros, modules and visibility, pattern matching, edition features |
| Errors and diagnostics | `l3-box-dyn-error-downcast`, `l5-fix-generic-parse-list`, `s1-fix-question-mark-from`, `s1-collect-into-result`, `s1-result-of-option`, `s1-fix-unwrap-on-input` | Thin | error design for a library and an app, context, spans |
| Design patterns | `l5-typestate-http-builder`, `l5-newtype-typestate`, `l1-mem-replace-state`, `l4-where-self-sized`, `l4-any-downcasting`, `l4-plugin-logger`; sans-IO style in `B4` and `Y7` | Part | tower layer, event bus, undo and redo, choosing between patterns |
| Memory and allocators | `F3` (4), `y2-array-vec` | Part | garbage collection, size classes |
| Unsafe and FFI | `Y2` (3), `Y3` (4), `s3-minivec-*`, `c3-*` | Part | pin projection, auditing someone else's unsafe code |
| Option, Result, conversions, ownership types | `S1` (17), `l1-cow-normalizer`, `s2-cow-str` | Good for Option; Thin for conversions and smart pointers | `Rc`/`Weak`/`Arc` in use (`S7` unwritten) |
| Text | `S2` (18) | Good | |
| Iterators and collections | `S4` (11) and the adapters above | Part | adapters and consumers (`S6` unwritten) |
| Vectors, slices and collections | `S3` (20) | Good | `VecDeque` and `BinaryHeap` in use |
| Numbers, traits and formatting | `s2-format-width-precision`, `s2-display-vs-debug`, `l4-impl-display`, `l4-vec2-operators`, `l4-matrix-operators`, `s1-order-total`, `f7-itoa-stack-buffer` | Part | floats and total order |
| Memory and low level | `s3-minivec-core`, `y2-mini-vec`, `s3-ring-buffer` | Part | global allocator |
| Threads, synchronisation and time | `C1` (3, building primitives), `l1-move-into-threads`, `l3-fix-thread-spawn-static` | Thin | using the std primitives, time |
| Processes, signals and networking | none (`B4` and `y9-shell-line-parser` parse bytes and text only) | None | needs a runner that allows processes and loopback |
| I/O, files and paths | `s2-word-frequency`, `l5-inner-fn-monomorphization` (generic `Read`) | Thin | readers, writers, files |
| Data, text and encoding crates | only `smallvec` | None | serde, bytes, regex, itertools |
| Application crates | none | None | anyhow, thiserror, tokio, axum, clap, tracing |
| Multi-file projects (macros, features, build script, no_std) | none | None | needs multi-file grading |
| Worked questions | overlap with `L`, `S`, `C` and the DSA area | n/a | these pages hold answers |

### Testing

| Chapter | Practised by | Depth |
|---|---|---|
| Test design and TDD; property testing; fuzzing; concurrency testing; memory-ordering bugs; testing databases, compilers, distributed systems; benchmarking and profiling | none asks the learner to write a test or a benchmark. `F2` has 2 timed problems. `proptest` and `turmoil` are vendored and unused | **None** |

### Algorithms

| Chapter | Practised by | Depth |
|---|---|---|
| Hashing and probabilistic structures | `F4` (4), `s4-open-addressing-map` | Part (no HyperLogLog, cuckoo, MinHash) |
| Trees and ordered structures | `s4-interval-map`; `f2-slotted-page` (one leaf page) | Thin |
| Caches, queues, heaps, bit sets | `f5-bitset-iter`, `s3-ring-buffer` | Thin (no LRU) |
| Graph algorithms | `l5-associated-type-vs-parameter` (reachability) | Thin here; the DSA area has graphs, with Python practice |
| Dynamic programming, greedy, search | `y8-shortest-edit-script` | Thin here; DSA area |
| Sorting, strings, bits, theory | `f5-swar-memchr`, `f5-varint-codec` | Thin |
| Mathematics and numerics | `l5-const-fn-crc-table` | Thin |

### Systems

| Chapter | Practised by | Depth |
|---|---|---|
| Threads, locks, channels | `C1`, `C2` (5) | Thin (no deadlock, actor, rayon, sharded map) |
| Atomics and memory ordering | `C3` (7) | Good (no false sharing) |
| Concurrent data structures | `c3-spsc-ring`, `c3-bounded-mpmc`, `c3-index-stack`, `c3-atomic-stats` | Part |
| Async Rust | `C5` (3, runtime internals) | **Thin**: no cancellation, `select!`, streams, structured concurrency |
| Measuring and CPU performance | 2 timed `F2` problems; `f5-swar-memchr` | Thin (`F1` unwritten) |
| Memory layout and allocation | `F2` (14), `F3` | Good |
| Files, processes, OS | `y9-kv-store-wal` (a `Vec` stands for the file) | Thin |
| High-performance computing | none | None |

### Data, distributed, compilers, projects

| Chapter | Practised by on the Rust tab | Depth |
|---|---|---|
| Storage and buffer management; indexes; execution; optimization; transactions; logging and recovery; building engines | `f2-slotted-page`, `y9-kv-store-wal`, `y10-trigram-index`. The BusTub course (Courses area) practises buffer pool, indexes, execution, concurrency control and recovery | Thin here, covered by Courses |
| Foundations and SQL | none | None |
| Analytics, time series, streaming | `f2-validity-bitmap-column`, `f5-bit-packed-vec`, `f7-posting-list-codec` | Thin |
| Text search | `Y10` (2) | Thin |
| Vector search; running a vector database | none | None |
| Time, partitioning, replication | `f4-consistent-hash-ring` | Thin |
| Consensus and distributed transactions | `Y7` (3) | Thin |
| Collaboration and CRDTs | `y9-ot-collaborative-text` | Thin |
| Streams, messaging, reliability; microservice patterns | none | None |
| Web services; authentication; cryptography | `b4-http1-request-parser` only | None to Thin |
| Networking and protocols | `B4` (3) | Part (hidden, W6) |
| Encodings and compression | `f5-varint-codec`, `f7-posting-list-codec` | Thin |
| Lexing and parsing | `l3-cursor-over-str`, `l3-zero-copy-tokenizer`, `y9-shell-line-parser`, `y8-regex-nfa-matcher` | Thin |
| Tree-walking interpreter; types and semantics; back ends; tooling | none | None |
| Bytecode and VMs; middle end | `Y6` (2), `f2-eight-byte-instructions`, `f2-inline-edge-lists` | Thin |
| Complete projects | `Y9` (3 single-step builds) | Thin |
| Appendices | reading lists | n/a |

**Priority from the map:** testing, async, application crates, error design, unsafe review and performance hunting. These are
Rust skills with none or thin practice. Database, distributed and compiler subjects stay with Courses and later project paths.

---

## 5. The learn-by-doing experience **[proposal]**

### 5.1 What a learner does on the tab

1. **Home** answers "what do I do now": re-solves due, then the unfinished exercise, then the next one on the chosen path.
2. **Choose by path, track or kind.** Paths are ordered lists of existing problem ids ("Senior interview", "Async in
   production", "Unsafe and proof", "Make it fast"). A path is a view: it never moves a problem. Tracks stay as they are.
3. **Attempt** in the workspace. When stuck, open Cookbook references one level at a time (section 6).
4. **Submit.** Then: compare with the reference, open "what this teaches", answer the follow-up.
5. **Come back** on the review schedule, from the starter.
6. **Build** a project path when a group of ideas is solid.

Progression in each track keeps the owner's bands: use it, understand it, build it. Nothing is locked.

### 5.2 Kinds of exercise

`mode` (`write`, `fix`, `stage`) stays, because time budgets and statistics use it **[code]**. Add an optional `kind` field
used only for labels and filters. Without it the label comes from `mode`. The loader rejects unknown fields
(`deny_unknown_fields`), so the field must be added to the model first. Content only; no database change.

| Kind | What the learner does | Graded today? | How, or what is missing |
|---|---|---|---|
| Implement / make the tests pass | writes code to a signature | **Yes** | `write` |
| Fix the compile error | repairs code under rules | **Yes** | `fix` with rules |
| Fix the bug | code compiles, a test fails | **Yes** | `fix`; the starter may compile |
| Refactor | same behaviour, better shape | **Mostly** | behaviour tests plus rules (4 rule types), allocation budgets, `size_of` checks. No rule can require something ("must use `thiserror`") |
| Make it faster | meets a budget | **Yes** | `[perf]`: allocations, relative timing, assembly. Timing on a busy host can flake |
| Predict | fills in constants without running | **Yes** | used by `l1-predict-drop-order`. `Run` can reveal it, so honour system |
| Review unsound code | finds and fixes a soundness hole | **Part** | observable effects (double drop counted, panic safety, drop order). A missing `Send`/`Sync` bound needs a runtime trait probe in the test **[general]**, because a wrong solution may not be rejected by the compiler. Data races and aliasing need Miri (nightly; not in the image) |
| Write the tests | catches broken implementations | **Yes, with a convention** | the learner writes `pub fn check(subject: ...)` in `lib.rs` that panics on a wrong subject. Hidden tests call it with one correct and several broken implementations. The trait and the correct version live in the test files. A `wrong/` checker that accepts everything must fail |
| Misuse must not compile | typestate, lifetimes, auto traits | **No** | needs runner support: extra files built one by one, each expected to fail with a named error code |
| Build a project in stages | carries code forward | **Part** | each stage is gradable today; carrying code needs section 7.3 |
| Async exercises | any kind, with tokio | **Yes, with a convention** | `verify` counts the literal `#[test]`, and the web test list matches the same, so `#[tokio::test]` would not count or be listed **[code]**. Write `#[test]` functions that build a runtime (`start_paused` for time) |
| Programs, files, sockets, several files | | **No** | tests exercise one library; multi-file and binary grading need runner work |

---

## 6. Cookbook references

The owner's use: "i can use my brain to practice rust myself and if i am stuck i can refer cookbook to get exact or similar
or related information". Where a page solves a problem, that is now a useful **exact** reference. Sections 6.1 and 6.2
are kept as a record of which pages do.

### 6.1 What was checked

The Cookbook attaches complete source files to pages (`code = [...]`, shown as a read-only project; 491 pages in the snapshot).
Two checks:

1. **Line matching (automatic, weak lower bound).** Lines the learner must write (solution minus starter, comments and spacing
   removed) searched in each page's code. Of 246 comparable problems, 3 have at least half their lines on one page, 9 at least
   30 %, 14 at least 20 %. Renaming a variable defeats it.
2. **By hand and by name**, for 90 topic-matched exercises (159 exercise and page pairs): shared type and function names for
   all pairs, and page code read for the first table below.

### 6.2 Pages that solve an existing problem (candidate "exact" references)

**Confirmed by reading the page's code** (against the reference solution, or against the statement where noted):

| Exercise | Page | What the page gives |
|---|---|---|
| `c3-spsc-ring` | `spsc-ring-buffer` | the whole design with the same orderings and drop |
| `y2-mini-vec` | `implement-std-from-scratch` | `MyVec` push, pop, insert, remove, growth, close to line by line (no zero-sized case) |
| `s2-cow-str` | `cow-borrow-until-you-must-own` (sample) | `escape_html`, nearly the reference solution |
| `c3-bounded-mpmc` | `mpmc-bounded-queue` | full push, pop, drop (compared with the statement and names) |
| `c3-mini-arc` | `own-arc-refcount` | a larger `MyArc` with orderings explained (compared with the statement and names) |
| `f3-string-interner` | `design-patterns-low` | the interner, nearly the reference solution |
| `l3-static-bound-vs-static-ref` | `std-any-error-marker-traits` | `Extensions` insert, get, get_mut (not remove) |
| `l4-lending-iterator` | `hrtb-gats` | the GAT trait and `WindowsMut` (compared with the statement and names) |
| `l5-typestate-http-builder` | `typestate-builder` | the same typestate shape and names, no validation (compared with the statement and names) |
| `y7-raft-leader-commit` | `raft-consensus` | `advance_commit` with the same term rule |
| `y8-shortest-edit-script` | `dp-lcs-diff` | LCS table and backtrack (other tie rule) |
| `y10-trigram-index` | `trigram-index` | candidate intersection and verify (no replace or remove) |
| `l4-iterator-extension` | `extension-traits-ops` | `dedup_adjacent` identical (1 of 3 methods) |

**Likely (names and domain match; code not read side by side):** `c3-spinlock`, `c3-once-cell`, `c1-rwlock-writer-preferring`,
`s1-my-option`, `s3-minivec-core` (`implement-std-from-scratch`, 2,366 lines); `c5-block-on`, `c5-oneshot-channel`,
`c5-mini-executor` (`waker-executor`, `mini-async-runtime`); `c3-index-stack` (`lockfree-stack-aba`); `y2-unsafe-linked-list`
(`nonnull-linked-list`); `y2-array-vec` (`maybeuninit-fixedvec`); `f3-generational-slab` (`slab-generational`);
`f3-typed-arena` (`bump-arena-allocator`); `y8-regex-nfa-matcher` (`regex-nfa-engine`); `b4-http1-request-parser`
(`http1-parser`); `y9-kv-store-wal` (`mini-kv-store`, `mini-kv-store-code`); `y10-bm25-ranking` (`search-engine`);
`s3-ring-buffer` (`ring-buffer-zoo`); `l5-phantom-units-of-measure` and `l1-fix-small-struct-copy` (`phantomdata-typed`);
`l5-stack-plugs-into-std` (`std-iterator-sources-custom`); `l4-where-self-sized` (`type-erasure-vtable`).

**Different design, low risk:** the two open-addressing maps (the page uses Robin Hood with backward shift; the exercises use
other removal rules), `s2-zero-copy-split-iterator`, the `F2` layout problems.

Not checked pair by pair: the other fix-mode problems, and the `Worked questions` pages, which are answers by design. Concept
pages that explain a borrow-checker technique help a fix problem without containing it; treat those as safe "read first" only
after a person has looked.

### 6.3 Showing references **[proposal]**

- Three levels, least revealing first: **related** (explains the idea), **similar** (the technique on a different
  problem), **exact** (solves the task or a named part of it). Call a page exact only after reading it; leave a level
  empty rather than stretch.
- All three start **closed**. The learner opens one level at a time, from the Hints tab or a Cookbook list beside it.
- Content: `problem.toml` has no field for this and `ProblemFile` has `deny_unknown_fields` **[code]**, so a new field
  must be added to the model first: `[[cookbook]]` entries with `level`, `page`, optional `section` (the anchor
  `cb-<heading slug>`) and `note`. The batch-one data is a sidecar, `cookbook-refs.toml`, in the staging folder.
- Loader: parse the field; `anneal validate` checks that the page id exists and that the section is a heading on it.
- API and web: send the entries with the problem; render three closed disclosures linking to `/cookbook/p/<page>#<section>`.
- Open question for the owner: should opening a level mark the attempt assisted, as a hint does? Recommendation: only
  **exact** does. It would be recorded on the attempt (the server's), so the Cookbook still stores nothing about the
  reader. Opening a page from the Cookbook tab itself marks nothing.

---

## 7. Three added ideas: what exists, what each needs

### 7.1 Compiler-error explanations

**Today [code]:** the console's diagnostic card links a code to Rust's error index and a clippy lint to its list
(`DiagCard` in `Workspace.tsx`). The inline lens and hover card (`diagcard.ts`) have no such link. rustc's rendered text,
with its `help:` lines, is shown. The Progress page counts error codes. The BusTub course has 30 `errors-*.md` concept files.
There is no explanation inside the Rust workspace. 60 Rust problems name an error code in their title, statement or tags
(22 distinct codes; `E0502` 16, `E0499` 14, `E0382` 7, `E0507` 5). The Cookbook mentions 17 distinct codes and has 5
`compile_fail` examples.

**Plan:** a side sheet opened from the code in the console card, the lens, the hover card and the statistics list.
Content `content/errors/E0382.toml`: what the compiler is protecting, a small failing example in a neutral domain, **several**
ways out with when each fits, Cookbook page ids, and the official link (kept). It is chosen by the code only, never by the
exercise, and never quotes the learner's or an exercise's code. Because it lists several remedies and no exercise code, it does
not state which one a rule allows.

**Needs:** a loader and one read endpoint; a verify step that compiles each example and checks it raises that code; 22 written
pages for the codes the problems already name; a spoiler read of each against the problems that name the code. No database.
**Place:** release 2.

### 7.2 Compare after solving

**Today [code]:** the solution unlocks on a solve **or** a reveal, as a read-only `solution.rs` tab plus notes (explanation,
time, space). `workspace/diff.ts` has a line diff, used only between runs in the Timeline. Every run's code is stored
(`runs.code`) and the page already receives the current attempt's runs. No side-by-side view and no "why it differs".

**Plan:** a "Compare" editor tab, shown only when the current attempt has a **passing submit** with no rule broken (a reveal
alone does not open it). Left: that run's code. Right: the reference. Changed lines marked, panes scroll together; one column
with a Mine/Reference switch on phones. Above it, authored notes: optional `[[compare]]` entries in `problem.toml`
(`point`, `why`, optional line in the reference) saying why the reference chose what it did and when another choice is as
good. Facts computed without judging: lines changed, clippy warnings on the passing run. "What this teaches" and the
The Cookbook references (6.3) sit under it. No generated or AI text. A passing solution that differs is not called wrong.

**Needs:** the first version is web only (runs and solution are already sent); `[[compare]]` needs a model field. After a
re-solve starts a new attempt, Compare waits for a new pass; showing older passes would need an API read of earlier runs.
**Place:** release 1 (without notes), notes added track by track.

### 7.3 Build-a-project paths

**Today [code]:** `Mode::Stage` exists in the model, labels and time budgets, with **0** problems. `Y9` has 3 one-step
builds, one of them a key-value store with a log (`y9-kv-store-wal`). The BusTub course already does staged building, through
the CLI and `git push`. `CURRICULUM.md` plans five projects (38 stages, including `kvlite`) and `PLAN.md` lists project pages
and a `project_repos` table that no migration creates. The planned letter `P` cannot be used: the loader treats section `P`
as Python practice and requires `unlocked_by`. The Cookbook's project and capstone pages publish whole builds.

**Plan:** a track in section `Y` (say `Y11 Build: a key-value store`), mode `stage`, in four stages: command parser and
in-memory store; append-only log and recovery; segments and compaction; reads during writes. Each stage's tests include the
earlier ones. The learner writes all the code. One `lib.rs` with inline modules.

**Carrying code:** on stage N with no draft, offer "Start from my passing stage N-1" or "Start from the given starter". It
writes only the new stage's draft. If a draft exists, it is first copied to `data_archive` (the existing table) and the
button asks for confirmation. The given starter is always a complete stage N-1, so a stage can be taken alone.

**Needs:** a project path page; an endpoint returning the last passing submit of the earlier stage's id; verify that each
stage's solution passes that stage's cumulative tests. No migration. The `mini-kv-store` page is a Cookbook reference
for it (section 6). The format and API should differ from `y9-kv-store-wal` so that the stages are new work. **Place:** release 3 (stages 1 and 2),
stages 3 and 4 later. Multi-file projects and real files stay with the Courses engine until the runner grows.

---

## 8. Backlog

Sizes: S under a day, M a few days, L a week or more. Rough, not measured.

### Release 1 (small): trustworthy practice

| # | Item | Size |
|---|---|---|
| R1.1 | "What this teaches", follow-up and tags closed until solved, on the problem page and the track's Learn tab (decision 1) | S |
| R1.2 | `B4` on the Rust tab and in Rust progress (`NAV_SECTIONS`, `PLANNED`, `progress.rs` areas); a test that every Rust track is listed, or derive the list from the API (fixes W6, W7) | S |
| R1.3 | Cookbook references in three closed levels (6.3): model field, `validate`, API, UI; fill it from `cookbook-refs.toml` and 6.2; make Related link to written tracks and plain text for unwritten ones | M |
| R1.4 | Compare after solving, without authored notes | S |
| R1.5 | Six exercises needing no runner change: numbers 1, 4, 8, 10, 12, 14 below | L |
| R1.6 | Editor step E1 (section 10.6) | M |

The home redesign (paths, kinds) waits until there are enough new kinds to filter: release 2.

### Release 2
Home and path pages as in the mockup; the `kind` label and filter; compiler-error pages for 22 codes; exercises 2, 3, 5-7, 9,
11, 13, 15-18; compare notes for new exercises; editor steps E2 and E3.

### Release 3
Project path (exercises 19, 20, then two more stages); finish `C4`, `L8`, `Y5`, `F1` tracks; editor step E4.

### Release 4 (runner work)
Compile-fail grading; running one test; assembly view; Miri if wanted; multi-file submissions. Then exercises that need
files, sockets or several files, and subjects such as loom (not vendored; needs an image rebuild).

### The first 20 new exercises

**Superseded** by [rust-practice/GAPS.md](rust-practice/GAPS.md) (the full list) and the staged first batch. The table is
kept for its notes on grading. Its Cookbook page column and "`after`" remarks predate the owner's decision.

Ids are `<track code>-<slug>`. They are new; none appears in `content/retired.txt`, whose retired ids all start `d` or `lc-`
(check again when written, because a retired id's progress is deleted at start-up). Slugs are fixed once shipped. Every one
needs the usual bar (5 visible, 8 hidden, a `wrong/`, hints, notes, follow-up). "Page" is `first` unless marked `after`.
**The new pages' code has not been compared with these exercises, which do not exist yet; run the overlap check when writing.**

| # | Id and title | Concept | Kind | How it is graded | Crates | Cookbook page |
|---|---|---|---|---|---|---|
| 1 | `l8-import-errors`: A library error that keeps its cause | error enum, `source`, `From` | implement | match variants, walk `source()`, exact `Display` | thiserror | `anyhow-thiserror-eyre-error-design`; `after`: `error-enum` |
| 2 | `l8-fix-stringly-errors`: Replace `Result<_, String>` | typed errors, retryable | refactor | tests match variants; rules forbid `to_string` where it hides a cause | thiserror | `error-chain-retryable` (`after`) |
| 3 | `l8-context-at-the-edge`: Say what was being done | `anyhow` context | refactor | the `{:#}` chain equals the expected text | anyhow | `anyhow-context` (`after`) |
| 4 | `c4-cancel-safe-reader`: `select!` is losing bytes | cancellation safety | fix the bug | paused time, chunked input, every frame arrives once | tokio | `async-cancel-safety` (`after` unless checked) |
| 5 | `c4-bounded-fan-out`: At most N in flight | `JoinSet`, semaphore | implement | counter of the highest in flight, results, error stops the rest | tokio | `joinset-semaphore` (`after`) |
| 6 | `c4-fix-stalled-runtime`: Who blocked the executor? | blocking and guards across `.await` | fix the bug | single-thread runtime; a heartbeat task must keep ticking | tokio | `tokio-runtime-tasks-channels-timers` |
| 7 | `c4-drain-with-deadline`: Shut down without losing work | graceful shutdown | implement | paused time: accepted work finishes or is returned | tokio, tokio-util | `graceful-shutdown` (`after`) |
| 8 | `y5-catch-the-broken-queue`: Write the test that catches six queues | test design, model checking by hand | write the tests | the checker passes one correct queue and panics on each broken one | none | `testing-patterns` |
| 9 | `y5-codec-properties`: Round trips are not enough | property tests, shrinking | write the tests | same convention, broken codecs | proptest | `criterion-proptest-insta-testing`, `shrinking` |
| 10 | `y2-review-panic-safety`: This `unsafe` drops twice | panic safety | review unsound code | drop counters with a closure that panics at each position | none | `drop-raii`, `std-panic-process-exit-catch`; `after`: `testing-unsafe-code` |
| 11 | `y2-review-auto-traits`: Who said it was `Sync`? | `Send`/`Sync` bounds | review unsound code | runtime trait probe on chosen types; behaviour tests | none | `unsafecell-cells` |
| 12 | `c1-fix-transfer-deadlock`: Two locks, two orders | lock ordering | fix the bug | opposing transfers under a watchdog; totals preserved | none | `locks-deadlocks` (`after`) |
| 13 | `c2-fix-lost-wakeup`: The worker that sleeps forever | condition variables | fix the bug | a forced interleaving through test hooks, plus a watchdog; must not rely on luck | none | `mutex-condvar-channel` (`after`) |
| 14 | `f1-allocation-hunt`: Fourteen allocations a line (sample figure; set by measuring) | allocation profiling | make it faster | `count_allocs` budget plus behaviour tests | none | `counting-allocator`, `perf-checklist` |
| 15 | `f1-hidden-quadratic`: Fast on ten, stuck on a million | complexity in real code | make it faster | release; `assert_faster` against the starter's copy in the tests, and a size that times out | none | `perf-checklist`, `benchmarking` |
| 16 | `s6-merge-sorted-streams`: A lazy merge | custom iterator | implement | infinite inputs, pull counts, `size_hint` | none | `std-iterator-adapters-lazy`; `after`: `std-iterator-sources-custom` |
| 17 | `s7-fix-observer-leak`: Dropped, but still alive | `Rc` cycles, `Weak` | fix the bug | drop counters; callbacks still fire while alive | none | `smart-pointers`, `std-smart-pointers-use-cases` |
| 18 | `s12-match-the-wire-format`: JSON you do not control | serde attributes | make the tests pass | fixed documents in and out, unknown fields | serde, serde_json | `serde-attributes-enum-representations` (`after`) |
| 19 | `y11-store-commands`: Store, stage 1 | parsing, ownership, API shape | build stage | command scripts against expected replies | none | `std-str-search-split-parse` |
| 20 | `y11-store-log`: Store, stage 2 | append-only log, torn writes | build stage | cumulative tests; the log cut at every byte | none | `wal-append-log` (`after`), `mini-kv-store` (`after`) |

Also check the editor's rust-analyzer with crates: its session resolves them from crates.io on the host **[code]**, so confirm
it works in production before shipping crate exercises.

---

## 9. Keeping progress safe

Progress is keyed by `problem_id` in `attempts`, `runs`, `drafts`, `reviews`, `focus_time` and `scratch` **[code]**.

| Change | Why progress is kept |
|---|---|
| Every item in this plan | no id is renamed, removed or renumbered; nothing is added to `retired.txt`; no migration is needed |
| Paths | new content that lists existing ids; deleting a path touches no row |
| New tracks and exercises | new ids only. A new `order` value never forms part of an id; append rather than reorder |
| `B4` on the tab | list and area changes only; its ids stay `b4-*` |
| Closing "teaches" until solved | display only |
| `kind`, `[[cookbook]]`, `[[compare]]`, error pages | content fields; nothing stored (marking an attempt assisted uses the existing flag) |
| Compare | reads existing runs |
| Project carry-forward | writes a draft only for the new stage; an existing draft is archived to `data_archive` first, with confirmation |
| Restoring an old run into the editor (later) | same rule: archive the current draft, ask first |
| New editor settings | new fields with defaults inside the `settings` JSON; old rows still load |
| Editor layout state | browser storage, as the pane sizes are now |
| Readiness | adding problems to a track lowers its percentage. No data changes; decision 3 |

If a later change did need a new table, it would be additive, and `tools/check-progress-safety.py` and the preflight apply.

---

## 10. The code editor window (`/p/$id`)

**Design goal (owner):** the editor behaves like a small VS Code. "Mini" matters: one exercise, a fixed set of files, a
sandbox that runs tests. It must not pretend to have a terminal or a file system. All existing controls stay.

### 10.1 What exists **[code]**

- **Editing** (`Editor.tsx`): CodeMirror 6; Rust and Python highlighting; line numbers, active line, bracket match and
  closing, auto indent (4 spaces), undo history, default keymap (includes toggle comment, move and delete line), Tab indents.
  Vim mode (`jk`, `kj`, `:w`). rustfmt on ⌘S and ⇧⌥F and optionally on pause, applied as a minimal edit.
- **rust-analyzer** (`lsp.ts`, `inlay.ts`): completion (buffer words until ready), hover, signature help, jump to definition,
  diagnostics with a rustc-style card, inlay type hints, Live clippy. For `src/lib.rs` only.
- **Run results:** inline lens under error lines (dismissable), borrow lanes (off by default), console with Compiler
  (codes link to docs, locations jump), Output (including `dbg!`), Timeline with diffs between runs.
- **Window:** tabs `src/lib.rs` (dot when changed since the last test run), `main.rs` scratch, `tests`, `hidden` (after a
  solve), `solution` (after unlock); Run or Run tests, Aa settings (size, Vim, format on pause, live clippy, ligatures,
  font), Reset with confirmation; status bar (rust-analyzer state and counts, toggled by click; autocomplete; lanes; Ln, Col);
  left tabs Problem, Hints, Solution, Related; right panel Tests with Submit and the focus timer; panel toggles ⌘B, ⌘J, ⌥⌘B;
  draggable widths and console height; problem bar with breadcrumb, previous and next, interview clock; on narrow screens
  Problem, Code, Tests tabs.
- The same `Editor` component serves the DSA problem page (Python), so editor changes must not break it.

### 10.2 Missing or weak **[code]**

- No find or replace outside Vim: `@codemirror/search` is in `package.json` and nothing in `web/src` imports it.
- No folding, line wrapping, multiple selections (`allowMultipleSelections` is not set), go to line, outline or symbol jump.
- Rename and references are exported by the installed `@codemirror/lsp-client` 6.3.0, but not bound. It has no code actions.
- No key for jumping to the next or previous diagnostic (the lint keymap is not added).
- Long lines run past the right edge: no wrapping, and scrollbars are hidden app-wide.
- `main.rs` gets no rust-analyzer: the editor is given no session, and the bridge writes only `src/lib.rs` on save. The
  session's package is written with `lib.rs` and the visible tests; no scratch file.
- Panel resize grips are `aria-hidden`, so widths cannot be changed from the keyboard.
- The focus timer sits at a fixed height under the test list. The coordinator's 1500x950 screenshot shows it taking much of
  the panel; by its CSS it is roughly 220 px closed (my estimate, not measured), and I saw no way to collapse it.

### 10.3 The bridge **[code]** (`crates/api/src/lsp.rs`)

- `from_client` handles `anneal/save` (writes `src/lib.rs`, sends `didSave`) and overrides `initialize` options and roots.
  It relays every other message, rewriting the virtual root. So rename, references, document symbols, folding ranges,
  semantic tokens, code actions and macro expansion are **not blocked**. None was tested.
- `from_server` answers every server-to-client request itself: `workspace/configuration` with the settings, everything else
  with `null`. A flow that relies on `workspace/applyEdit` would not work: the edit never reaches the browser. Edits returned
  in a response (rename; code actions that carry their edit) can be applied by the client.
- Only the workspace root is rewritten, so a definition inside std keeps a host path that the browser cannot open.
- `settings()` turns off proc macros, build scripts, lenses, cache priming, and parameter, chaining and closing-brace hints.
  The check runs on the host, not in the sandbox. The file's comment gives "no dependencies, build scripts or proc macros"
  as the reason this is safe, though sessions can now include crates. **Do not enable proc macros or build scripts in this
  bridge**, and review that comment.
- At most 3 sessions.

### 10.4 CodeMirror or Monaco

**Recommendation: keep CodeMirror.**

| Point | Evidence |
|---|---|
| Work that would be rewritten **[code]** | `Editor.tsx` (454 lines: theme, lens block widgets, dismissal, borrow-lane drawing, format and save keys, Vim `:w`), `lsp.ts` (199: bridge transport and diagnostic cards), `inlay.ts` (83), Vim through `@replit/codemirror-vim`, Python editing rules (`pythonEditing.ts`, 97) also used by the DSA page. `lanes.ts` (81, the lane model) and most of `diagcard.ts` (84) are not tied to CodeMirror in the same way and could be reused |
| What is missing now **[code]** | mostly unbound features: search, folding and wrapping have ready packages or built-ins; rename and references are already in the installed LSP client |
| Size **[code]** | the `web/dist` found on disk has a 2,033,024-byte main script (the whole app; KaTeX is separate). I did not rebuild it and did not measure CodeMirror's share. Monaco is not installed |
| Monaco's advantages **[general]** | closer to VS Code out of the box: minimap, command palette, symbol widgets, peek, multi-cursor, built-in diff editor |
| Monaco's costs **[general]** | considerably larger download, uses web workers, weak support on phones, theming outside the app's CSS variables, no built-in Rust language service (the bridge would need a Monaco LSP client), Vim through a different third-party package |

Measure before overturning this: build a throwaway page with each and compare compressed size and behaviour on a phone.

### 10.5 The mini VS Code layout

How each part would be built. "Local" means CodeMirror or React only. "Bridge ok" means a rust-analyzer request that
`lsp.rs` already relays (untested).

| Part | Today | Build |
|---|---|---|
| Activity bar: Exercise, Explorer, Search, Outline, Cookbook, Settings | no | React. "Exercise" holds the existing Problem, Hints, Solution and Related tabs |
| Explorer | no (tabs only) | React over a **fixed** list: `src/lib.rs`, `main.rs` (scratch), `tests/visible.rs` (read-only), `tests/hidden.rs` and `solution.rs` (shown locked until unlocked), `compare` (after a pass). No new, rename or delete: there is no file system |
| Tabs with unsaved mark and close | tabs and a dot | React. The dot keeps its meaning, "changed since the last test run" (drafts autosave). Closing hides a tab; `lib.rs` stays |
| Breadcrumbs | page breadcrumb only | file, then symbol path from document symbols (bridge ok; custom request) |
| Outline and go to symbol | no | `textDocument/documentSymbol` (bridge ok; custom, since the client lacks it) |
| Find and replace | Vim only | local: `@codemirror/search` |
| Search across the exercise's files | no | React over the text already in the page; never hidden or locked files |
| Command palette | ⌘K is anneal's app palette | a second, editor-scoped list on ⇧⌘P (and F1). ⌘K stays as it is |
| Go to line | no | local (`@codemirror/search` `gotoLine`) |
| Folding, wrapping | no | local: `foldGutter` (the Rust grammar has fold data), `EditorView.lineWrapping`. With borrow lanes on, check wrapping against the lanes' 260 px right padding |
| Minimap | no | a third-party CodeMirror package **[general]**; later and off by default |
| Split view | no | a second editor, mainly code beside tests or the reference; later |
| Bottom panel | Compiler, Output, Timeline | add **Problems** (live rust-analyzer list; rows jump) and **Test results** (a text view of the last run's cases). Compiler, Output, Timeline stay. **No terminal tab** |
| Right panel | Tests, Submit, focus timer | stays. Timer gains a one-row form (time, start, expand); expanded it is today's card with every control |
| Status bar | rust-analyzer, autocomplete, lanes, Ln/Col, format error | keep all; add error and warning counts that open Problems, "edition 2021" (now in the bar above the editor), Wrap, Vim state |
| Resizable, collapsible panels | drag and ⌘B ⌘J ⌥⌘B | keep; make grips focusable with arrow keys |
| Settings | Aa popover and status toggles | add wrap, minimap, relative line numbers to the same popover |
| Phones | Problem, Code, Tests | keep those three. No activity bar, split or minimap. Explorer and the command list open as sheets; wrapping on by default |

**Shortcuts.** On Windows and Linux read ⌘ as Ctrl. Browser behaviour below is **[general]** and must be tried in the
owner's browsers.

| Action | Key | Note |
|---|---|---|
| App palette (anneal) | ⌘K | unchanged, also inside the editor. The listener also accepts Ctrl+K, so on a Mac it takes Ctrl+K from the editor **[code]**; consider ⌘ only on Mac. VS Code's ⌘K chords are not used |
| Run tests, Submit, scratch run | ⌘↵, ⇧⌘↵, ⌘' | unchanged |
| Save and format; format | ⌘S; ⇧⌥F | unchanged. Do not also bind the LSP client's format keymap, which uses the same keys |
| Panels: side, bottom, tests | ⌘B, ⌘J, ⌥⌘B | unchanged |
| Editor commands | ⇧⌘P, F1 | Firefox uses Ctrl/⌘+Shift+P for a private window and may not give it up |
| Quick open a file | ⌘P | replaces Print inside the workspace |
| Find; replace; across files | ⌘F; ⌥⌘F (Ctrl+H); ⇧⌘F | browser find is overridden only while the editor has focus |
| Symbol; line | ⇧⌘O; Ctrl+G | ⇧⌘O is the bookmark manager in some browsers |
| Next and previous diagnostic | F8, ⇧F8 | |
| Definition; references; rename | F12; ⇧F12; F2 | F12 can open developer tools; offer them in the command list too |
| Close tab | ⌥W | ⌘W, ⌘T, ⌘N and Ctrl+Tab belong to the browser; do not try to take them. ⌥W types a character on a Mac, so match the physical key as ⇧⌥F does |
| Split; wrap | ⌘\; ⌥Z | match physical keys |

Every shortcut also has a button or a command-list entry, and is added to the `?` help.

### 10.6 Editor backlog, small and safe first

| Step | Items | Needs | Size |
|---|---|---|---|
| **E1** | find and replace, go to line; wrap toggle (on for phones); folding; F8; bind rename and references for `lib.rs`; focus timer one-row form; keyboard resize; `?` help entries | local, bridge ok. Test with Vim on, on the DSA page, and with lanes on | M |
| **E2** | the shell: activity bar, explorer, closable tabs, Problems panel, status additions, editor command list and quick open | React; no server change | L |
| **E3** | outline, symbol jump, breadcrumbs; search across files; `main.rs` in rust-analyzer; Compare tab polish | custom requests; the bridge must also write `src/bin/scratch.rs` on save, and the browser must open that file in the session | M-L |
| **E4** | split view; run one test; restore code from a run; quick fixes where allowed; semantic colouring; minimap | runner filter argument; custom LSP work | L |

---

## 11. Inventory: a full Rust IDE against anneal

Effort: S, M, L. "Answers" says whether the feature helps practice or hands over solutions. Verdict: **first** (E1 or
release 1), **next** (E2, E3, release 2), **later**, **out** (with reason). Claims about how rust-analyzer or browsers behave
in rows not covered by section 10 are **[general]** and untested.

| Feature | Today | How it would be built | Effort | Practice or answers | Verdict |
|---|---|---|---|---|---|
| **Editing** | | | | | |
| Highlighting, brackets, auto indent, undo | yes | keep | | fine | keep |
| Vim mode | yes | keep | | fine | keep |
| Format (key, on pause) | yes | keep | | fine | keep |
| Toggle comment, move or delete line | yes (default keymap) | add to help | S | fine | first |
| Find and replace | Vim only | `@codemirror/search` | S | fine | first |
| Multiple cursors, select next match | no | search keymap plus `allowMultipleSelections` | S | fine; check Vim | first |
| Folding | no | `foldGutter` | S | fine | first |
| Line wrapping | no | `EditorView.lineWrapping` | S | fine | first |
| Rulers, whitespace marks, indent guides | no | small extensions | S | fine | later |
| Snippets | no | `snippet` from `@codemirror/autocomplete` | S | writes `impl` skeletons that drills ask from memory | out in exercises |
| Emmet-style or AI completion | no | | | writes the answer | out. The app has optional AI help on some DSA and system design pages (`ai.rs`, `AskPanel`); the Rust workspace has none, and none is proposed (HANDOFF also says not to re-add AI features, which the code has moved past: confirm the rule) |
| Persistent undo across reloads | no | store history locally | M | fine | later |
| **Navigation** | | | | | |
| Go to definition | yes (`lib.rs`) | keep | | fine | keep |
| Definition into std | no | a read-only std source endpoint and URI mapping (bridge change) | M | reading std is good practice, and is also the answer for "build Vec" drills | later, hidden until solved |
| Type definition, implementation | no | exported by the LSP client; bind | S | fine for own code | next |
| References | no | client's keymap; bridge ok | S | fine | first |
| Go to line | no | `gotoLine` | S | fine | first |
| Symbol jump, outline | no | document symbols | M | fine | next |
| Workspace symbol search | no | one file matters; covered by outline | S | fine | out: no value |
| Breadcrumbs | page only | document symbols | S | fine | next |
| Explorer, tabs, close | tabs | section 10.5 | M | locked files stay locked | next |
| Search across files | no | over text in the page | M | must skip hidden and solution until unlocked | next |
| Peek definition, back and forward | no | custom panel; history of jumps | M | fine | later |
| Call or type hierarchy | no | custom views | L | little value at this size | out |
| Split view | no | second editor | M | fine | later |
| Minimap | no | third-party package | S | fine | later |
| **Language features (rust-analyzer)** | | | | | |
| Diagnostics, Live clippy | yes | keep | | clippy often names the fix; it is a setting already | keep |
| Completion, hover, signature help | yes | keep | | method lists help recall drills; the toggle exists | keep |
| Inlay type hints | yes | keep; parameter and chaining hints could be settings | S | weakens "predict the type" drills a little | later |
| Rename | no | `renameKeymap`; response edits; one file | S | fine | first |
| Quick fixes (code actions) | no | custom request and client capabilities; edits in the response only, since `applyEdit` is dropped | M | **can fix a fix-mode exercise in one click** (such as adding a clone or `'a`) | later; off while unsolved (decision 7) |
| Refactorings (extract, inline, fill match arms, generate impl) | no | same machinery | M | writes required code | later; same rule |
| Auto import on completion | unverified | check what the client does with extra edits | S | fine | next |
| Semantic colouring | no | custom semantic tokens and theme | M | fine | later |
| Highlight uses of the symbol | no | document highlight, custom; or `highlightSelectionMatches` for text only | S | shows a borrow's uses; mild | next |
| Folding ranges from the server | no | the grammar's folding is enough | S | fine | out |
| Code lenses (run, references count) | off | keep off | | little value | out |
| Expand macro | no | custom `rust-analyzer/expandMacro`; derives will not expand with proc macros off | S | fine for `macro_rules!` | later |
| Proc macros, build scripts | off | | | would run code on the host through the bridge | **out: unsafe** |
| View MIR, HIR, syntax tree | no | custom requests | M | niche | out |
| Error explanations | docs link in console | section 7.1 | M | written to avoid answers | next |
| **Running and testing** | | | | | |
| Run tests, Submit, scratch run, clippy | yes | keep | | | keep |
| Per-test results, panic, stdout | yes | add the text Test results panel | S | hidden details stay withheld | next |
| Run one test | no | a filter argument through API and runner; visible only | M | fine | later |
| Gutter run buttons beside tests | no | depends on run one test | S | fine | later |
| Watch mode (run on save) | no | | M | encourages guess and check; loads the sandbox | out |
| Coverage | no | needs coverage tooling in the image | L | useful for "write the tests", though mutants already grade it | out for now |
| Doctests | off in the manifest | enable per problem | S | useful for documentation exercises | later |
| Benchmarks | relative timing inside tests | keep; no free benchmark UI | M | timings on a shared host mislead | out |
| Real terminal, shell | no | | | would be fake or a remote shell | **out: do not fake it** |
| Real file system, new files, `Cargo.toml` editing | no | fixed files; multi-file needs runner work | L | | out here; projects use Courses |
| Add any dependency | no | vendored set, per problem | | the sandbox has no network | out |
| **Debugging** | | | | | |
| `println!`, `dbg!` with locations | yes (Output) | keep; add to help | S | | keep |
| Breakpoints, stepping, variables | no | debug adapter in a sandbox that drops all capabilities | L | | **out**: large work and a weaker sandbox |
| Backtraces | unverified (panic text shown) | set `RUST_BACKTRACE` for visible tests | S | hidden must stay withheld | later |
| Miri | no | nightly in the image, a second run kind | L | the right tool for unsafe review | later (decision 8) |
| Sanitizers, loom | no | nightly or new crates | L | | out for now |
| **Profiling** | | | | | |
| Allocation counts, relative timing | tests only | show the numbers in Test results | S | good practice | next |
| Assembly view | runner can emit it; no UI | return it for `[perf] asm` problems | M | fine after your own code | later |
| Flame graph, perf counters | no | needs privileges in the sandbox **[general]** | L | | out |
| Type size and layout viewer | no | custom; or print `size_of` in scratch | M | answers the layout quizzes | out |
| **History** | | | | | |
| Runs and diffs between them | yes (Timeline) | keep | | | keep |
| Restore a run's code | no | archive the draft, confirm, write | S | fine | later |
| Compare with the reference | no | section 7.2 | S | only after a passing submit | first |
| Git, blame, branches | no | | L | | out: one draft per exercise |
| **Settings and workspace** | | | | | |
| Font, size, ligatures, Vim, format, clippy, autocomplete, rust-analyzer, lanes, theme | yes | keep | | | keep |
| Wrap, minimap, relative numbers | no | same popover and JSON | S | | first (wrap), later |
| Editable key bindings, user keymaps, extensions | no | | L | | out: one user, fixed set |
| Command list and quick open | app palette only | editor list | M | | next |
| Remembered layout | sizes and panels | add open tabs and view | S | | next |
| **Accessibility** | | | | | |
| Leaving the editor by keyboard | Tab indents; the default keymap has a tab-focus toggle **[general]** | document the key in help; verify | S | | first |
| Keyboard panel resize | no (grips are hidden from assistive tech) | focusable separators with arrow keys | S | | first |
| Announce run results and diagnostics | unverified | a polite live region | S | | next |
| Colour is not the only signal | test cards use tick, cross, dashed ring | check lanes and squiggles | S | | next |
| Contrast, focus rings, reduced motion | tokens and rules exist; not audited | audit light and dark | M | | next |
| Screen reader use of the editor | not tested | test with VoiceOver | M | | next |
| Phone editing | one pane at a time | wrap on; a key row for `{ } & ' | ; ::` | M | | next |

---

## 12. Not verified

- Nothing was built or run: no cargo, pnpm, Docker, database or server. I did not run `anneal validate` or `verify`.
- The app was not seen running. The one screenshot observation is the coordinator's.
- No rust-analyzer request was tried through the bridge.
- Pages that solve a problem (6.2): line matching over all comparable problems, name matching over 159 chosen pairs, page code read only for the
  rows described as confirmed. Pages outside the chosen pairs may also give answers.
- Cookbook mapping used statement openings, not solutions, against a Cookbook that was changing.
- Cookbook pages linked from the 20 new exercises were chosen by title and chapter; most were not read.
- The two grading conventions (write-the-tests, `#[test]` plus a tokio runtime) follow from the runner's code but have not
  been tried through `verify`. The runtime `Send`/`Sync` probe is general knowledge.
- Browser shortcut behaviour, Monaco's properties, and third-party packages are general knowledge.
- Sizes of work are estimates.
