# Build a DBMS: BusTub in Rust (a CodeCrafters-style course)

Started 2026-10-08 at the owner's request. **Status: design done; the CLI and modules 1a-1g (Project 1), 2a and 2b (49 stages, regrouped from 115 tiny ones) are built and verified; everything else is on the board.** Progress and who-does-what: [BUSTUB_TASKS.md](BUSTUB_TASKS.md). Agent prompt for
writing a module: [authoring/write-course-module.md](authoring/write-course-module.md). The C/C++ → Rust reference every stage leans
on: [PORTING.md](PORTING.md). This course replaces the BusTub parts of SYSTEMS.md (K17–K20, P6); the general systems tracks come after.

## 0. What the owner asked for (the requirements log)

In order, kept close to their words so nobody has to re-ask:

1. *"fully read bustub and understand how to port things"*, for **low-level systems patterns from OS kernels, databases and compilers**, and to
   learn to **translate C and C++ to Rust**. (Context in [SYSTEMS.md](SYSTEMS.md).)
2. *"also add bustub curriculum in isolation that can be done one problem at a time. it should not match the exact bustub solution, but I will
   need foundations to build each module"*. Example: **the disk manager**: "go through basics with small problems; you give me the syntax and
   methods needed and ask me to solve something with my brain using those; then the next one builds on it. Think of it like **CodeCrafters**:
   *build your own relational DBMS* is our curriculum and it closely mirrors BusTub. **Templates to fill, enough syntax and extra reading, and
   test cases to pass. One module at a time.** Build the BusTub curriculum first, then move on to general Rust things."
3. *"can we build the curriculum like the CodeCrafters CLI where I clone a repo and submit. **Git hooks and all.**"*
4. *"**this mirrors bustub exactly. Same tests, same module structure etc., but rusty.**"*
5. *"use this CodeCrafters challenge as an example* (https://app.codecrafters.io/courses/shell/overview): it has useful methods, functions and
   things, resources for learning, and tests. **Each module is split into many parts based on difficulty, and multiple modules form one project.**"*
6. *"in case of bustub you can **link the lectures** too."* And: *"you can link to documentation for crates, std, core libs and other Rust docs,
   white papers, blogs, content in the Rust book, lectures, videos, CMU lectures, book chapters, parts of other open-source code repos etc.
   **The more comprehensive the better.**"*
7. *"**Deploy chunk by chunk, one by one when done, so I can practise while you build the rest.**"*
8. *"**Each stage should be tested, along with the whole project; make sure there are no bugs.** Regression, smoke, sanity, unit and integration
   tests if needed. We already have tests from BusTub, so at the least they should pass. Other tests only if relevant, since we are porting to
   another language, but don't spend a lot of time on extra tests; they can be deferred. **These tests ensure the solutions *you* provide are
   correct, and the BusTub tests ensure the solutions *I* provide are correct.**"*
9. *"you can also **mention the C/C++ way of the things we are learning**, so it helps me convert C/C++ code to Rust in the future. Come up
   with the design first and **document all of this and the progress too**."*
10. (2026-10-08, web pages) *"the hints and the content should not be trivial or basic things … bustub is a serious exercise"* and *"i am not
    looking for one or two line change exercise in bustub"*. So: a stage is a real piece of work (tens of lines, a design decision), the first
    cut of 115 tiny stages was merged into 36 (`tools/course_merge.py`), and hints are written at BusTub's level (design, then the trap, then the
    invariant to check), never "which shift keeps the top bits". The stage page follows the layout the owner picked (Learn C++ Through Projects'
    walkthrough, CodeCrafters' stage flow, anneal's own design); the mockup is `docs/design_handoff_anneal/designs/course-page.html`.

**The standing rules for stages and pages live in [COURSE_STANDARDS.md](COURSE_STANDARDS.md). Read it before changing the course.**

## 1. The shape, copied from CodeCrafters

CodeCrafters' *Build your own Shell* is one **course** made of **extensions** (Navigation, Quoting, Redirection, Command Completion, Background
Jobs, Pipelines, History, ...), each a run of small **stages** labelled **Very easy / Easy / Medium / Hard** with a time estimate for "a proficient
developer" (under 5 min, 5-10 min, 30 min to 1 hour, over 1 hour). The learner clones a repo with stubs, edits, pushes, and a tester reports.

| CodeCrafters | here |
|---|---|
| Course ("Build your own Shell") | **Course** `bustub`: "Build a DBMS: BusTub in Rust" (`courses/bustub/`) |
| Extension (a themed group of stages) | **Module**, named by its BusTub component (1a Disk manager, 1b Disk scheduler, ...). Its last stage is **BusTub's own test** |
| Stage with a difficulty label and a time | **Stage** `1a-07`: `very-easy` · `easy` · `medium` · `hard` (same time buckets); kind `learn` / `build` / `boss` |
| The repo you clone | `bustub-rs`, made by `anneal course init bustub`, **mirroring BusTub's tree** with every body `todo!()`; only the modules reached so far are in it (the next one arrives when you pass the last stage of the current one) |
| `git push` runs the tester | a **pre-push git hook** runs `anneal course test` locally and records the result |
| Stage page: task, tests, notes | `anneal course show`: see §3 |

A stage is **small** (a few lines to ~40; most are "easy"): the CodeCrafters rhythm of many quick wins. The component's real behaviour appears in
the medium/hard stages and the boss.

## 2. Mirroring BusTub exactly, but rusty

Same files, same names, same tests; Rust spelling (`LRUKReplacer` → `LruKReplacer`, `GetPinCount` → `get_pin_count`), so the C++ and the Rust can be
opened side by side. The tests are BusTub's, ported test for test with the same names in snake_case. Where BusTub's tests assume a C++ idiom (fixed file
names, exceptions, `DISABLED_` prefixes) the port changes the idiom and keeps the assertion.

| BusTub | `bustub-rs` | Boss tests |
|---|---|---|
| `src/include/common/{config,rid,rwlatch,channel}.h` | `src/common/*.rs` | `rwlatch_test` |
| `src/storage/disk/disk_manager{,_memory}.cpp` | same path, `.rs` | `disk_manager_test` ×4 **(built)** |
| `src/storage/disk/disk_scheduler.cpp` | same | `disk_scheduler_test` |
| `src/buffer/{lru,clock,lru_k,arc}_replacer.cpp` | same | `lru_replacer_test`, `clock_replacer_test`, `lru_k_replacer_test`, `arc_replacer_test`, `arc_replacer_performance_test` |
| `src/buffer/buffer_pool_manager.cpp`, `storage/page/page_guard.cpp` | same | `buffer_pool_manager_test` ×7, `page_guard_test` ×2 |
| `src/storage/page/extendible_htable_*`, `container/disk/hash/*` | same | `extendible_htable_page_test`, `…_test`, `…_concurrent_test` |
| `src/storage/page/b_plus_tree_*`, `storage/index/*` | same | `b_plus_tree_{insert,delete,sequential_scale,concurrent,tombstone}_test` |
| `src/type/*`, `catalog/*`, `storage/table/*`, `table_page.cpp` | same | `type_test`, `tuple_test`, `tmp_tuple_page_test` |
| `src/execution/*`, `optimizer/*` | same | `test/sql/*.slt` via a Rust sqllogictest runner |
| `src/concurrency/*` | same | `txn_*_test`, lock manager and deadlock tests |
| `src/recovery/*` | same | `recovery_test` |
| `src/primer/*` | same | `trie_test`, `skiplist_test`, `count_min_sketch_test`, `hyperloglog_test`, `orset_test`, `robin_hood_hash_set_test`, ... |

BusTub's list of test cases was read from the 2026-09 `cmu-db/bustub` master (`/tmp/bustub-full`, not kept).
**Given code**, as in BusTub: `binder/` and `planner/` (SQL → plan; FRONT-1 on the board), `table_generator`, `mock_scan`, the shell, the sqllogictest runner.

**"Rusty", defined once** (PORTING.md has the long form):
- `page_id_t` → a `PageId(i32)` newtype; `INVALID_PAGE_ID` survives only inside on-disk formats; APIs use `Option<PageId>`. Same for frame, txn, lsn.
- Exceptions → `Result`; asserts that guard programmer errors stay panics.
- `(char *p, size_t n)` → a slice; a fixed buffer → `&[u8; N]`; `reinterpret_cast` of page bytes → a typed view, no `unsafe` unless a stage says so.
- `shared_ptr` → `Arc` only where ownership really is shared, else `Box`/borrow; `mutex` + fields → `Mutex<Inner>`; RAII guards stay RAII (`Drop`).
- Abstract base classes → traits; templates → generics with the same parameter roles.

**Creative freedom (owner, 2026-10-08: "just like CodeCrafters I should have the creative freedom to write my own way, and just the tests have to pass, but still show the necessary skills and tools").**
The contract of a stage is its **public API and its tests**, nothing else. Consequences for authors:
- Stubs are plain `todo!()` bodies; they never declare typed local variables or dictate the shape of the code (`@begin`/`@end` regions may nest, so a stage that extends a function written by an earlier stage sits *inside* that function's region).
- Given structs, helper functions and fields are a *starting point*; the stage text says so. Tests never reach into private state. Where a test needs to observe something, it uses a public accessor that is part of the stage's stated API (e.g. `slot_of`), and the stage says so.
- The stage page teaches the tools (types, methods, patterns, the C/C++ equivalents) but prescribes no algorithm beyond what the behaviour requires.
- `anneal course show` ends every stage with "Your way: ...".

## 3. Anatomy of a stage (what `anneal course show` prints)

Every stage page has, in this order (CodeCrafters' task / tests / notes, plus the two things the owner added):

1. **Where this fits**, one line.
2. **The task**: what to implement and where (file, function); precise behaviour.
3. **Tests**: what the stage's tests check, in words (the learner can also read them).
4. **Syntax and methods**: exactly the std items the task needs, with signatures and a one-line example. The owner's "useful methods, functions and things".
5. **Notes**: the idea, the trap, the why.
6. **In BusTub**: the C++ lines this replaces, quoted with the path.
7. **The C/C++ way**: a table mapping the C (POSIX), C++ and Rust spellings of what the stage does, the pitfalls the C/C++ version has that Rust removes, and a
   *port rule* ("`(char*, size_t)` pairs become one slice"). This is the owner's "so it helps me convert C/C++ to Rust in future"; each stage's rule is a
   candidate **idiom card** ([SYSTEMS.md](SYSTEMS.md) §8).
8. **Learn more**: links specific to the stage.
9. **Module resources** (appended by the CLI from `module.toml`): the CMU lecture (slides, notes, video), BusTub's own files, then docs, books, man pages,
   papers, blogs, videos and other projects' code. **Policy: comprehensive, and every URL checked** (`/tmp/linkcheck.py`-style: HTTP 200, YouTube via oEmbed).

Resources come from: std/core/crate docs (docs.rs), the Rust Book, Rust by Example, the Nomicon, *Rust Atomics and Locks*, man pages, CMU 15-445 slides/notes/videos
(`courses/bustub/lectures.toml`, copied from the course's schedule pages), database papers (e.g. *Architecture of a Database System*, the ARC and LRU-K papers,
the mmap-in-a-DBMS paper), engineering blogs, and real code (SQLite, PostgreSQL, redb, Turso, sled, tokio, ...).

## 4. How a learner works (the CLI and hooks)

```
anneal course init bustub ~/code/bustub-rs   # copy the template, git init, install the pre-push hook, first commit
anneal course status                         # modules and stages, ✓ done, → current
anneal course show [stage]                   # the stage page
anneal course tests [stage]                  # the test cases to pass (readable names), with ✓/✗ from your last run
anneal course guide                          # the one-screen guide: the loop and the commands around it
anneal course test [stage] [--all]           # run the stage's tests, then everything done so far (regression)
anneal course next                           # the next stage
git commit -am work && git push              # hook: runs the tests, records the result, prints the next stage
```

State is in the learner's repo: `.anneal/course/` (the stage definitions, copied at init, so it works offline), `.anneal/progress.json` (passed stages
with commit and time), `.anneal/course.toml` (`block_on_fail = false` by default: the hook reports but never blocks a push). The hook script embeds
the absolute path of the `anneal` that installed it. Tests run under `cargo test` with a time limit (a deadlocked solution is killed, not hung).
**Server and web:** `POST /api/course/submissions` + a Course page showing modules, stages and history are *not built*; they need a mockup and the
owner's go-ahead (CLAUDE.md rule), plus a server-side token (`ANNEAL_CLI_TOKEN`, never committed).

## 5. How a module is authored (single source = the reference tree)

```
courses/bustub/
  course.toml          lectures.toml (CMU lecture catalogue)
  modules/<NN>-<slug>/module.toml          code, title, summary, lectures = [..], bustub = [..], [[resources]] kind/title/url
  modules/<NN>-<slug>/stages/<NN>-<slug>/stage.toml   id, title, kind, difficulty, tests = ["bin::filter", ...], retest?
  modules/<NN>-<slug>/stages/<NN>-<slug>/stage.md
  template/            generated: the repo as it is before stage 1 (committed; what `init` copies)
  reference/           the complete solution with stage markers       <- committed (§8): never embedded in the CLI, never in the image
```

The reference tree is the whole learner repo with the solution written out. What a learner writes is marked:

```rust
    // @begin 1a-07
    let n = read_full_at(file, buf, slot_offset(slot))?;
    buf[n..].fill(0);
    Ok(())
    //~ todo!("1a-07: read_full_at, then zero-fill the rest of the buffer")
    // @end
```

`anneal course build [--stage S | --full]` renders the repo after any stage (later stages' regions become their `//~` stub lines; stubs declare the variables the
rest of the function needs, so every state compiles). `anneal course template` writes `template/`. Nobody edits `template/` by hand.

## 6. How it is tested (the owner's item 8)

| Level | What | How | Status |
|---|---|---|---|
| **Stage unit tests** | each stage has tests named `s<module>_<NN>_*`; they must fail with the stage's stub and pass with its solution | `anneal course verify` | 1a: 19 stages ✓ |
| **Regression** | every earlier stage's tests still pass after each stage | `verify`, and `anneal course test` for the learner | ✓ |
| **Compiles-at-every-state** | the repo compiles with any prefix of the stages done | `verify` (a "doesn't compile" state fails it) | ✓ |
| **BusTub's tests** | each module's last stage is BusTub's own tests, ported; the reference passes them | boss stage | 1a: `disk_manager_test` ×4 ✓ |
| **Integration** | the full solution passes `cargo test` for the whole repo and builds in release | `tools/course-smoke.sh` | ✓ |
| **Smoke** | `init` → `status` → `test` (stub fails) → solve stage 1 → commit → the pre-push hook records the pass | `tools/course-smoke.sh` | ✓ |
| **Sanity** | the learner's tests are the same files `verify` ran; no hidden tests | by construction | ✓ |
| **Extra tests** (property tests, model checks, loom, fuzz against a `HashMap` model) | only where a port could differ from C++ in ways BusTub's tests don't see; **deferred** | later, per module | open |

Two directions, as the owner put it: **the reference passes the stage tests and BusTub's tests** (so the answers anneal ships are right), and **the learner's code
passes the same tests** (so theirs are right). `tools/course-smoke.sh` is the one command that checks all of it; the CI job doesn't run it (it needs a long build per stage); run it before shipping a module.

## 7. Delivery: chunk by chunk

A module ships when its `verify` is green: commit (without `reference/`), push to `master`; `git pull` in the anneal checkout and `anneal course init bustub` (new learner) or `anneal course update` (existing repo: adds new stages, tests and stubs;
given files you haven't touched are refreshed; a file you edited is never overwritten, the new version lands beside it as `<file>.new`) is all the learner needs. The CI deploys the web app as usual; the course is CLI-first until the web page exists.
Order of modules: BusTub's (1a → 1e → 2a → 2c → 3 → 4 → 5), with the optional primer (P0) last.

## 8. Decisions and risks

1. **Where the reference lives.** In this repo, at `courses/bustub/reference/` (owner's decision, 2026-10-10). It is original Rust, not CMU's C++; the
   repo is public, so the solutions are public too. Two things keep it from leaking into products: `crates/cli/build.rs` embeds only `course.toml`,
   `lectures.toml`, `modules/` and `template/`, and `.dockerignore` excludes `courses/*/reference` from the app image. The server learns the solutions
   from `courses/bustub/solutions.json`, which CI generates from the reference at deploy time and the API loads at start-up (no manual upload).
   `tools/push-solutions.sh` still works against a running app.
2. **Licence.** BusTub is MIT; ported tests and short C++ excerpts keep the CMU copyright line (in the test files and the template README).
3. **Defaults chosen:** std only (no new crates, no image rebuild) until a stage names a crate; `BUSTUB_PAGE_SIZE` 8192 as in BusTub; macOS/Linux only (positional I/O via `FileExt`);
   tests visible; hooks never block; progress local.
4. **SQL front end** (BusTub gives students a parser/binder/planner) must be written once (FRONT-1) before the `.slt` bosses of module 3 can run.
5. **Scale:** ~400 stages. 1a took 19 stages / ~100 tests; expect the same density. Modules ship independently.
6. **Windows:** not supported by the disk stages (they use `std::os::unix`). A `cfg(windows)` shim is a deferred task.

## 9. The modules and their stages (what exists, what is planned)

Module codes follow BusTub's projects (`1x` = Project 1, ...). A module is a run of stages ending in BusTub's own test. Lecture ids are from
`courses/bustub/lectures.toml`. ✓ = built, verified (`anneal course verify`) and shipped.

### P0 · Primer (optional; planned, ~40 stages, authored last)
Copy-on-write trie and trie store (`Arc`, `make_mut`, `Box<dyn Any>`) · skip list · count-min sketch · HyperLogLog · ORSet (a CRDT) · Robin Hood hash set.
Boss: `trie_test` ×14, `trie_store_test`, `skiplist_test`, `count_min_sketch_test` ×13, `hyperloglog_test`, `orset_test`, `robin_hood_hash_set_test`.

### P1 · Storage (80 stages, complete ✓)
- **1a Disk manager (19) ✓:** slot offsets → open/create → `set_len` → positional write/read (short reads, zero-fill) → fresh slots → page table
  (`write_page`/`read_page`) → growth by doubling → `delete_page` + free list → atomic counters → log append/read → `trait DiskIo` → memory disks → boss `disk_manager_test` ×4.
- **1b Disk scheduler (15) ✓:** `Channel` (`Mutex` + `Condvar`) → the `None` stop signal → one-shot promise/future (broken promises) → `DiskRequest` and buffer
  ownership → `execute` → worker thread + `schedule` → `Drop` shuts down → a panicking disk doesn't kill the worker → `ReaderWriterLatch` → sharded workers with
  per-page order → boss `disk_scheduler_test`, `rwlatch_test`.
- **1c Simple replacers (11) ✓:** a generational index-linked list (`push_back`, `pop_front`, `remove`, `move_to_back`) → LRU → CLOCK → boss `lru_replacer_test`, `clock_replacer_test`.
- **1d LRU-K (9) ✓:** bounded history → backward k-distance → `record_access`/`set_evictable` → `evict` (below k, then the k-th access) → `remove` → O(log n) eviction
  with a `BTreeSet` → boss `lru_k_replacer_test`.
- **1e ARC (9) ✓:** four lists → new pages → `evict` and ghosts → hits promote to MFU → ghost hits adapt the target → trimming ghost lists → `remove` → boss
  `arc_replacer_test` ×3 + the 256K-frame performance test.
- **1f Buffer pool (10) ✓:** frames and free list → `new_page` → `fetch_page` (miss) → hits and pin counts → `unpin_page` → eviction → dirty write-back → flush →
  `delete_page` → boss: BusTub's classic tests + a threaded stress test.
- **1g Page guards (7) ✓:** `ReadPageGuard`/`WritePageGuard` with `Deref` and `Drop` (unlatch, then unpin) → moves → flush → latch order and the deadlock test →
  contention and evictable tests → boss `buffer_pool_manager_test` ×7, `page_guard_test` ×2.

### P2 · Indexes (planned, ~90 stages; 2a built)
- **2a Typed pages (13) ✓:** little-endian ints at offsets, header views, fixed arrays in a page (`const fn` capacity), keys and comparators, sorted-array insert/remove with `copy_within`.
- **2b Extendible hash table (22) ✓:** hash and bit masks, bucket/directory/header pages, insert with split, remove with merge and shrink, concurrency with guards.
  Boss: `extendible_htable_page_test`, `…_test`, `…_concurrent_test`.
- **2c B+ tree (~48):** pages, search, insert/split, delete/borrow/merge, the iterator, latch crabbing, tombstones. Boss: `b_plus_tree_{insert,delete,sequential_scale,concurrent,tombstone}_test`.

### P3 · Query execution (planned, ~85 stages)
- **3a Types, tuples, table pages and heap, catalog (~25):** `Value`/`Type`, `Tuple` (de)serialisation, `TablePage`, `TableHeap`, iterator. Boss: `type_test`, `tuple_test`, `tmp_tuple_page_test`.
- **3b Executors (~45):** expressions, seq scan, insert/update/delete, index scan, aggregation, nested-loop/hash/index joins, sort, limit, top-N, window functions, external merge sort. Boss: `test/sql/p3.*.slt` (needs FRONT-1).
- **3c Optimizer (~14):** rewrite rules (NLJ → hash join, sort+limit → top-N, seq scan → index scan, ...). Boss: optimizer `.slt` files.

### P4 · Concurrency control (planned, ~55 stages)
- **4a MVCC (~30):** `Watermark`, timestamps and `TransactionManager`, undo logs and version chains, snapshot reads, write-write conflicts, GC, serializable validation. Boss: `txn_*_test`.
- **4b Lock manager (~25):** lock modes and the compatibility matrix, request queues with `Condvar`, upgrades, 2PL, deadlock detection. Boss: lock manager and deadlock tests.

### P5 · Logging and recovery (planned, ~15 stages)
`LogRecord` encode/decode → `LogManager` buffer and flush → write-ahead rule in the buffer pool → checkpoints → recovery (analysis, redo, undo). Boss: `recovery_test`.
