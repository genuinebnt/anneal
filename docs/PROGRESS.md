# anneal · progress (the pick-up file)

**Read this first, then [HANDOFF.md](HANDOFF.md) for background.** This file is the single live record of what is done, what is in
flight and what is next, so an agent can continue without the chat history. HANDOFF.md explains the project and how the owner works;
this file says where the work stands today.

**Keep it current.** At the end of every batch: add a line to "Done" (newest first, with the commit), update "Now" and "Next", and set
the date below. Do not let it drift: a wrong line here costs the next agent more than a missing one.

Last updated: **2026-10-11** (after the 3k commit; see section 2).

## 1. Now (unfinished or waiting)

| What | State |
|---|---|
| Deploy of the latest pushes (3k, figures, interview removal) | pushed to master; CI was still running at the last look (`gh run list`). When "Deploy to production" is green the owner runs `anneal self-update` to get the new CLI screens (the installed build predates them). |
| Learner journey | **done 2026-10-11**: `tools/course-journey.sh` (already existed) passed, 42 steps, 0 failures, against a throwaway app. |
| Full `anneal course verify` | **no complete green run since the 2c-05 fix** (the last was killed by an IDE crash at 4c-08). Passed alone: 3d-07, 3h-04, 2c-05. Module 3k was checked per stage (pass counts 0, 7, 13, 21, 32) and by `verify --stage unlock` (317 stages, 0 problems), **not** by the official full verify. Run it in the background, one build at a time. |
| Mutation experiment on the stage tests | **paused** after 10 mutants (the run, and the IDE, were killed by memory pressure). Harness: `/tmp/c2/mut/mutate.py` (ephemeral). Rerun later with one build at a time. |
| Owner decisions waiting | see section 4 |

## 2. Done (newest first)

| Commit | What |
|---|---|
| 3k commit (this session) | **BusTub module 3k, "Errors worth returning"** (`courses/bustub/modules/15b-errors-worth-returning`, code `3k`, placed after 3d, mid-course): one `DbError` for the whole database; the learner designs it, 32 tests check only the contract (From, `?`, Display, `source()`, `context`, `report()`, SQLSTATE-style `code()`, `client_message()`, Send+Sync, threads, a proptest model). Reference `src/common/db_error.rs`, tests `tests/stages_3k.rs`, concept `errors-that-carry-their-cause`. Pass counts after each stage: 0, 7, 13, 21, 32 (checked with `/tmp/c2/work_3k/check.sh`, one render folder). Also new animated figures and Invariants sections for two-phase locking, waits-for graphs, logical clocks and MVCC. Later modules needed no change. |
| `fc510812` | Animated figures and Invariants sections for ARIES, write-ahead logging, undo logs and fsync. |
| `9e395d38` | `docs/PROGRESS.md` created. |
| `8b663d66`, `0a7fc900` | BusTub 2c-06 and 4c-08: an "Explain it aloud" round and a design-note task. **Removed 2026-10-11 at the owner's request** ("remove interview materials and skip that"); the sections no longer exist and the interview layer is dropped from the plan. |
| `6c5605e4` | CLI screens (`crates/cli/src/ui.rs`): stage banner, `course tests` (test cases by readable name, ✓/✗ from the last run in `.git/anneal-last-run.json`), "next to fix" nudge on failure, completion/module/course boxes, roadmap strip in `status`, `course guide`. The stage count in the test header now matches `status` (challenges are not counted). |
| `243e7dfc` | `anneal self-update [--from] [--check]`; `anneal course update` fetches the course files from the app (`/courses-bundle/<id>`, ETag = fingerprint, never `reference/`) with the built-in files as fallback; a once-a-day update hint after `status`/`test`. Shared std-only format `crates/content/src/bundle.rs`; app side `crates/api/src/cli_updates.rs`; CLI side `crates/cli/src/update.rs`. Docs: CLI_INSTALL.md. |
| `1ccea220` | BusTub 2c-05: the scan-during-writes test checked more than the stage teaches (strictly sorted, no lost key) and failed about 4% of runs under load. It now checks that the scan finishes and invents no key, and that the tree is sound and unpinned after the writers join. Reference and template. |
| `f9af778f` | A key insight on every problem: all 1,669 in `content/dsa/insights.json`; check V8 in the content loader; shown as the second line of a row and on the problem page. Spec: DSA_LEARN_PAGE_SPEC.md section 8. |
| `f7e68e62` and the commits before it (2026-10-10) | DSA Learn page: technique cards for every topic (456 cards, 818 tested templates, 1,456 names only; status in DSA_LEARN_PAGE_STATUS.md), the UI polish pass (shared `polish.css`, centred search, width toggle, state icons, stats bar, course map), the status generator `tools/neetcode/learn_status.py`. |

The owner's own CLI was reinstalled from production on 2026-10-11 (the `243e7dfc` build, which has `self-update`).

## 3. Where the course stands (measured 2026-10-11)

Numbers from `courses/bustub`: 32 modules, **169 core stages** (124 build, 31 boss, 14 learn) and 148 optional challenges, 161 concept
articles. `anneal course lint`: 286 stages checked, 0 problems. `anneal validate`: 0 issues.

What is good: every core stage has "Your freedom", "Performance" and "Experiment"; 136 of 164 core stages carry an open `[!CHECK]`
question; 132 of 160 concepts have runnable "In real code" examples; the shape is contract + freedom, not recipes.

Gaps, most costly first (the owner agreed spaced return does **not** fit a cumulative project; regression runs already re-exercise
earlier code):

1. ~~Interview/career layer~~ **Dropped by the owner 2026-10-11.** Do not add "explain it aloud", interview questions or portfolio write-ups to stages. (The existing open `[!CHECK]` questions stay: they are the course's own checks.)
2. **Visuals are thin and uneven** (partly fixed 2026-10-11: ARIES, write-ahead logging, undo logs, fsync, 2PL, waits-for graphs and logical clocks now have animated figures, and those plus MVCC have Invariants sections; the counts below are from before). 66 of 160 concepts have an SVG, 39 only a text diagram, 55 no figure; only 5 are animated. No core
   stage page has an SVG, and stages link 849 concepts as *optional* against 52 as required. Weakest by diagram share: LRU-K (1 of 10
   concepts), values and types (1/12), optimiser rules (1/9), ACID logging and recovery (6/17, 10 with no figure; ARIES has none),
   lock manager (5/15), skip list (3/8), Robin Hood (3/9). The B+ tree module is the best covered, not typical.
3. **Invariants are not systematic.** About 38 of 160 concepts state them (search is approximate). The B+ tree concept has a good
   "rules that always hold" section; most structures do not. Tests already have shape checkers, so an "Invariants" box per structure,
   tied to them, would make it consistent.
4. **Thin boss/late-stage openings.** Shortest opening paragraphs 34 to 49 words (skip list, Robin Hood, lock-manager bosses) against a
   median of 261; the lock manager is the thinnest module (median about 720 words).
5. **No learning signal** (time to first hint, hints per stage are not recorded). Low priority for one learner.
6. **Raft scope** is undecided (single-node course so far).

Measure again with the same definitions: figures = fenced ```svg blocks per `concepts/*.md`; animated = `@keyframes` in the file;
check questions = `> [!CHECK]` callouts in `modules/*/stages/*/stage.md`; concept links = `concepts` and `concepts_optional` in each
`stage.toml`.

## 4. Next, in order (owner decisions marked)

1. **Push module 3k** once `anneal course verify --stage unlock` is green; watch CI; update this file.
2. **Visual and invariants pass** (the owner said "fill all thin areas"). Done: ARIES, WAL, undo logs, fsync, 2PL, waits-for, logical clocks (figures + Invariants), MVCC (Invariants). Still to do, database-core concepts with no figure: outer joins and null padding, access paths (index vs seq scan), hash functions, LIKE matching, conjunctive predicates; with a figure but no Invariants: the catalog, HyperLogLog, maintaining indexes (external merge sort, join algorithms with a new hash-join animation, LRU-K and the clock algorithm got theirs 2026-10-11). Then the Rust-language concepts without a figure, lower priority. Format: animated SVG whose base state shows everything (see `concepts/aries-recovery.md`); check by restarting the API and screenshotting `/courses/bustub/concept/<slug>`.
3. ~~Interview layer~~ dropped (owner, 2026-10-11).
4. **Rewrite the dozen thinnest boss openings.** Also proposed, waiting for the owner: a short "Fixed by the tests" line beside "Your freedom" on every stage (tests pin exact tree shapes, depths, latch counts and pin counts in several modules); an optional 4d challenge making `LockError` a real `Error` with lock codes (`40P01`, `55P03`) for `DbError`; a 4e stage where the SQL session returns `DbError` and the client sees `client_message()`.
5. CLI ideas the owner liked but not built: `course tests --explain <test>`, a "Next:" line after every command, `doctor` reporting a
   stale CLI or stale course files. Already exist (do not rebuild): `test --watch`, `test --filter`, `status --json`, `show --hint N`.
6. DSA: only write cards where free LeetCode problems exist (the 1,456 names-only techniques stay names). Dead CSS to remove when
   asked: `.l-listed`, `.l-ltoggle`, `.l-lchip`, `.l-lpanel`, `.l-tags`.
7. The owner's own steps for BusTub: CLI install on other laptops, the private solutions repo (`anneal course remote`).

## 5. Recipes and gotchas (things that cost time)

**Checks before a commit** (what CI runs): `cargo clippy --workspace --all-targets --locked -- -D warnings`;
`cargo test -q -p anneal-content -p anneal-api -p anneal-cli` (the API tests need Postgres on 5435); `cargo run -q -p anneal-cli --
validate` (0 issues); `cargo run -q -p anneal-cli -- course lint` (0 problems). Web: `cd web && pnpm exec tsc --noEmit -p .`; e2e
`E2E_CHROME=1 pnpm exec playwright test` (about 2.5 minutes; **the e2e server serves `web/dist`, so run `pnpm build` after a web
change**). The e2e API is on 127.0.0.1:8791 against `postgres://anneal:anneal@localhost:5435/anneal_e2e` (a throwaway DB: never point
experiments at the dev DB).

**Course checks.** `anneal course verify [--stage ID]` renders the repo before/after each stage in a scratch directory and runs the
tests. A full run takes well over an hour: run it in the background and poll the log. Never run it while editing `courses/bustub/reference`
or `modules`. A single stage run also runs the regression of the stages before it.

**Learner simulation recipe** (not run yet). Use the dev binary with `ANNEAL_EMBEDDED_ONLY=1`, and an isolated `XDG_CONFIG_HOME` and
`XDG_CACHE_HOME` so there is no login session and **nothing reaches the real app or the owner's progress**. In a scratch directory:
`anneal course init bustub learner`; for each core stage in order, render the reference after it
(`anneal course build --stage ID out`) and copy into the learner repo every rendered file that already exists there **or** is not in
`.anneal/template` at all (files of locked modules must not appear early); then `anneal course test --only` (skips the regression, which
would make the whole run quadratic) and check the stage records as passed and the next module unlocks. Finish with one
`anneal course test --all`, a `course update`, and `anneal course reset --all --yes`; assert progress is empty. Set
`CARGO_TARGET_DIR` to a shared scratch directory. The reference solution of one stage was used this way on 2026-10-11 (1a-01) and the
completion screen rendered correctly.

**Shell and machine quirks (macOS, zsh).** `>` into an existing file fails (noclobber): use `>|`. `ls` is aliased to eza and `du` to
dust: use `command ls`. There is no `timeout` command. `pkill -f anneal-api` also kills the e2e API on 8791; restart it with
`/tmp/c2/verify.sh`-style commands (`/tmp` is ephemeral: it may be gone). Do not use `echo =====` in zsh. A background launcher's
completion notice means the launcher exited, not that the job finished: wait for the job's own marker line.

**Local end-to-end of the update commands.** Start `target/debug/anneal-api` on another port with `ANNEAL_COURSES` pointing at a copy of
`courses/bustub` (needs `concepts/` too) and `ANNEAL_DOWNLOADS` at a directory holding `anneal-<os>-<arch>`; a differently code-signed copy
of the binary (`codesign -f -s - -i other.id`) gives a build with a different hash. Appending bytes to a signed binary kills it on macOS.

**Content rules that bite.** Course stage pages state what and why, never implementation details (owner rule). Check questions are
open, never multiple choice. DSA Learn pages follow DSA_LEARN_PAGE_SPEC.md exactly. A new catalog problem needs its insight in
`content/dsa/insights.json` or validation fails.

## 6. Where things live

| Area | Place |
|---|---|
| DSA Learn page rules, status, insights | `docs/DSA_LEARN_PAGE_SPEC.md`, `docs/DSA_LEARN_PAGE_STATUS.md`, `content/dsa/insights.json`, `content/dsa/lessons/*.toml`, `tools/neetcode/` |
| Course standards and plans | `docs/COURSE_STANDARDS.md`, `docs/CURRICULUM_REASSESSMENT.md`, `docs/BUSTUB.md`, `docs/BUSTUB_RESTRUCTURE.md`, `docs/COURSE_ROADMAP.md` |
| The course itself | `courses/bustub/{modules,concepts,template,reference}` (reference is committed; `anneal course template` regenerates the template) |
| CLI | `crates/cli/src/{course.rs,ui.rs,update.rs,course_sync.rs,embedded.rs}`; install and update docs in `docs/CLI_INSTALL.md` |
| App side of CLI updates | `crates/api/src/cli_updates.rs` (public: `/cli/<target>/version`, `/courses-bundle/<id>[/version]`), `/install.sh`, `/downloads/*` |
