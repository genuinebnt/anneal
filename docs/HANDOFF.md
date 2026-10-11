# anneal · handoff

> **The current state, the log of what was done and the next steps live in [PROGRESS.md](PROGRESS.md).** This file is background (how to run it, how the owner works, content authoring) and its "where things stand" sections date from 2026-09-27.

Last updated 2026-09-27 (evening). Read this first if you're picking the project up, whether you're working locally or
in a cloud session. It covers where things stand, how to run and check the work, how the owner likes to work,
and what's next, in order.

## 1. What this is

**anneal** is a personal Rust interview-prep platform for SDE-2/SDE-3 backend interviews, with one user, the owner. It has:

- an axum + sqlx API (`crates/api`) on Postgres;
- a sandboxed Rust runner (`crates/runner`);
- file-based content (`content/tracks`);
- a Vite + React SPA (`web`).

| Doc | What it holds |
|---|---|
| [PLAN.md](PLAN.md) | Decisions and architecture (D1–D8), phases |
| [CURRICULUM.md](CURRICULUM.md) | Every section, track and stage, and what each problem teaches |
| [ROADMAP.md](ROADMAP.md) | Everything beyond the curriculum, and **§7, the agreed order of work** |
| [design_handoff_anneal/designs/](design_handoff_anneal/designs/) | HTML mockups. `anneal-screens.html` is every screen; `workspace-layout.html` is the approved layout controls |
| [README.md](../README.md) | Run, login, crates, content CLI, tests |

## 2. Where things stand

**Content:** 22 tracks, 500 problems. Every ready problem meets the test bar (≥5 visible, ≥8 hidden, ≥1 `wrong/`), which
`anneal verify` enforces, and every track passed `verify`.

| Track | Problems | Planned |
|---|---|---|
| D1 Arrays & hashing | 24 | 24 |
| D2 Two pointers & windows | 18 | 18 |
| D3 Stacks & queues | 14 | 14 |
| D4 Binary search | 14 | 14 |
| D5 Linked lists | 15 | 15 |
| D6 Trees & BSTs | 8 | 44 |
| D7 Heaps & priority queues | 13 | 13 |
| D8 Intervals & greedy | 29 | 29 |
| D9 Graphs | 58 | 58 |
| D10 Tries & strings | 25 | 25 |
| D11 Recursion & backtracking | 28 | 28 |
| D12 Dynamic programming | 57 | 57 |
| L1 Ownership & moves | 20 | 20 |
| L2 Borrowing | 37 | 37 |
| L3 Lifetimes | 22 | 22 |
| L4 Traits & dispatch | 22 | 22 |
| L5 Generics & associated types | 16 | 16 |
| S1 Option & Result | 17 | 17 |
| S2 Strings & text | 18 | 18 |
| S3 Vec & slices | 20 | 20 |
| S4 Maps & sets | 11 | 18 |
| F2 Data layout | 14 | 14 |

**Built:**
- **Catalog:** the section catalog pages (Rustfinity-style) and the track page.
- **Workspace:**
  - three panels, with the console below the editor;
  - Run (scratch `main.rs`), Run tests and Submit;
  - rust-analyzer, borrow lanes and rule checks;
  - hidden tests revealed once solved.
- **Progress dashboards:** overview, Rust stats, reviews.
- **Spaced repetition.**
- **Login:** a single-user passphrase.
- **Settings:** editor (font, size, Vim with `jk`/`kj`), accent colour, and the workspace toggles (below).
- **Crates in problems:** vendored into the runner image.
- **Company tags:** `companies = [...]` in `problem.toml`, names from `COMPANIES` in `crates/content/src/model.rs` (grouped
  FAANG / Big tech / Databases / Rust shops; `validate` rejects others). The track page has a COMPANIES column (three
  chips, FAANG first and tinted, `+n`) and GROUP / COMPANY filter chips. Tags are approximate (commonly reported
  questions), and the page says so.
- **Verifier:** `anneal verify` runs each problem's `wrong/*.rs` and fails if one passes or doesn't compile; the test
  prelude has a seeded `anneal_prelude::Rng` for randomized brute-force comparisons.

**Designed but not built** (all in `anneal-screens.html`): Today, Library, Readiness, the project pages. The DSA mock interview is built (docs/DSA.md decision 25); its Rust round is not.

**Workspace layout as of the last session** (each point was a separate request from the owner; keep them):
- Left panel: Problem, Hints, Solution, Related. Centre: the editor, with the console under it. Right panel: Tests only, with Submit at the bottom.
- The editor tab bar has a single run button that depends on the tab. On `main.rs SCRATCH` it's **▷ Run** (⌘'). On every other tab it's **Run tests** (⌘↵).
  - Both buttons show a spinner while busy.
  - Run tests and Submit reopen the right panel if it's hidden.
- Layout icons at the **top right of the problem bar** show or hide the left panel (⌘B), the console (⌘J) and the right panel (⌥⌘B). Hidden panels disappear completely: no rails, no strips.
- The status bar holds clickable toggles: rust-analyzer, autocomplete, and borrow lanes (lanes only on problems that have them).
  They are saved editor settings (`autocomplete`, `rust_analyzer`, `borrow_lanes` in `crates/api/src/settings.rs`), so
  they carry across problems and reloads. **Borrow lanes are off by default** (owner's request).
- The problem bar never scrolls. Tag pills drop out first, then the breadcrumb truncates.
- The console hides until the next run, then opens on whichever tab has something to show.
- The Borrows tab was **removed** on request. Borrow lanes live in the editor only.
- Formatting: ⌘S (or Vim's `:w`) and ⇧⌥F run rustfmt through `POST /api/format`, applied as a minimal edit so the
  cursor and undo stay put; "Format on pause" (Aa, off by default) formats 1.5 s after typing stops. The shortcuts
  are caught in the capture phase so Vim can't swallow them. "Live clippy" (Aa, on by default) makes rust-analyzer
  check with clippy; ⌘S also triggers that check at once.
- Hovering a rust-analyzer error shows the same card as the inline lens after a run (`web/src/workspace/diagcard.ts`,
  used by the lens and by `richDiagnostics()` in `lsp.ts`, which replaces lsp-client's `serverDiagnostics`). The type/docs
  hover, completion docs and signature help share its look: a tinted header with the signature, then the docs ("editor
  popups" in app.css).

## 3. Running it

### Locally (macOS, OrbStack)

```sh
docker compose up -d                                                    # Postgres 18 on :5435
docker build -t anneal-runner:1.98 -f docker/runner.Dockerfile docker   # sandbox image
(cd web && pnpm install && pnpm build)
cargo run -p anneal-api                                                 # http://127.0.0.1:8787
```

`.cargo/config.toml` supplies `ANNEAL_DATABASE_URL` and `DATABASE_URL` for local runs. The full list of
environment variables is in the doc comment at the top of `crates/api/src/main.rs`. The main ones:

| Variable | Purpose |
|---|---|
| `ANNEAL_SANDBOX=host` | Runs code without Docker |
| `ANNEAL_DOCKER_CONTEXT` | Docker context; defaults to `orbstack` |
| `ANNEAL_PASSPHRASE_HASH` | Turns on login |

Day to day, the owner runs **`./scripts/restart.sh`** (stop the dev server, `git pull --rebase --autostash`, start
it again) or **`./scripts/dev.sh`**, and uses http://127.0.0.1:5180. If another program holds :8787 or :5180, dev.sh
picks random free ports, prints them, and points Vite's `/api` proxy at the API's port (`ANNEAL_API_PORT` /
`ANNEAL_WEB_PORT`). In dev mode:
- `cargo watch` restarts the API when `crates/`, `content/` or the Cargo files change;
- Vite hot-reloads the web app;
- `.githooks` reinstalls web dependencies after a pull that changes them.

So when you push, the owner's `git pull` puts your change live without a manual restart. Without dev mode, the
API on :8787 serves `web/dist`, so rebuild the web app after frontend changes.

### In a cloud session (no Docker, maybe no Postgres)

| What | Needs | Command |
|---|---|---|
| Content authoring and checking | a Rust toolchain only (edition 2024, so rustc ≥ 1.85; the image uses 1.98) | `cargo run -q -p anneal-cli -- validate`, then `cargo run -q -p anneal-cli -- verify <track>` |
| Unit tests (content, runner, rules) | Rust | `cargo test -p anneal-content -p anneal-runner -p anneal-rules` |
| API tests | Postgres | `cargo test -p anneal-api` (`DATABASE_URL` is forced to localhost:5435 in `.cargo/config.toml`, so start Postgres there or edit the config) |
| Web type-check and build | Node + pnpm/npm | `cd web && npx tsc -b --noEmit && npm run build` |

Notes on the cloud setup:
- `anneal verify` always uses the **host** runner, so it doesn't need Docker.
- Problems that list `crates = [...]` fetch them from crates.io when run on the host, so they need network access.
- In a cloud session, content work and backend code are the safest things to do. UI work needs the owner's
  review anyway (§4).

Cloud-session setup that worked on 2026-09-27 (Ubuntu container, running as root):

```sh
# Postgres 16 is installed but stopped, on 5432. The app and tests expect 5435 (moved from 5434 on 2026-09-27;
# if yours already runs on 5434, change `port = 5434` to 5435 in postgresql.conf and restart it).
sed -i "s/^port = 5432/port = 5435/" /etc/postgresql/16/main/postgresql.conf && pg_ctlcluster 16 main start
su postgres -c "psql -p 5435 -c \"CREATE ROLE anneal LOGIN PASSWORD 'anneal' SUPERUSER\"; psql -p 5435 -c 'CREATE DATABASE anneal OWNER anneal'"
rustup component add rust-analyzer          # the API's LSP test and the workspace need it
ANNEAL_SANDBOX=host cargo run -p anneal-api  # after `cd web && npm run build`; delete web/package-lock.json (the repo uses pnpm)
export CHROME=/opt/pw-browsers/chromium-*/chrome-linux/chrome   # for tools/ui-check.mjs
```

### Production

**Every push to `master` is deployed to https://anneal.genuinebasil.dev** once CI passes (clippy with
`-D warnings`, `cargo test`, `anneal validate`, the web build). Keep master green: run those locally before
pushing, and don't push half-finished work to master.

**Never lose the owner's progress.** It's stored by problem id (`<track code>-<slug>`):
- To rename a problem, change its track, or change its slug, add the old id to the problem's
  `renamed_from = ["old-id"]`. The API moves the stored progress to the new id when it starts. Don't delete problems.
- Migrations in `crates/api/migrations` are append-only. Add a new one; never edit or delete an applied one.
- CI (`tools/check-progress-safety.py`) and the deploy's `anneal-api preflight` both enforce this. A deploy that
  would strand progress stops before replacing the running version. The pipeline, the server layout, rollbacks and the secrets are
in [DEPLOY.md](DEPLOY.md). Never commit secrets: the repo is public.

### Checking UI without screenshots

`node tools/ui-check.mjs <url> "<js expression>" [width]` loads a page in headless Chrome and prints the
expression's value. Use it to check widths, overflow and which elements exist, at 1280px and 1440px. Two
things to watch:
- **1280px is the tight case.** The centre column is about 580px there.
- **Clean up test data.** Checks that run or submit code create attempts in the dev database. Clear them with:

```sh
psql postgres://anneal:anneal@127.0.0.1:5435/anneal -c "TRUNCATE attempts, reviews, focus_time, scratch CASCADE"
```

## 4. How the owner works (follow these)

- **Do exactly what's asked.** Put extra ideas in a proposal with a recommendation and wait for an answer. Unrequested
  changes were reverted twice. Never drop an existing control while moving things around. "do recommended" means go
  ahead with your recommendations.
- **Rust tracks are senior-level interview material** (owner, 2026-09-27): the owner is a senior Rust developer. Every
  problem, the Easy band included, turns on a real trap, trade-off or compiler reason; no tutorial drills, no one-token
  fixes. Each track must still cover its whole surface: every construct in the topic is written by hand at least once
  (Easy = several constructs from memory in a non-trivial task), with a "Syntax to remember" line in the notes where
  one is easy to forget. L4 and L5 put recall problems in fix mode, because `verify` needs a write-mode starter to
  compile, which would give away the signatures.
- **Mockups first for new screens and big UI changes.** Publish an HTML mockup and wait for approval. Small
  requested tweaks can be built directly.
- **The approved mockups in [mockups/](mockups/) are the UI.** The pages use the mockups' own markup and CSS, not
  an approximation: `tools/port_mockup_css.py <mockup> <out.css> <prefix> [accent]` turns a mockup's stylesheet into
  an app one (classes prefixed, every rule scoped under `.<prefix>root`). `course-motion.html` → `mock-course.css`
  (`k-`: course page, stage page, `/kit`, popups, toasts), `section-page.html` → `mock-section.css` (`s-`: `/rust`
  and the DSA home). Never edit those two files; change the mockup and re-run the tool. Pieces the mockup does not
  draw go in `mock-extra.css`. Pages mount their markup under `MockRoot` (`components/mock.tsx`).
- **No visible scrollbars anywhere** (app.css hides them). Content must fit without them.
- Editor indent is **4 spaces**. The editor font, size and Vim mode are user settings.
- **Difficulty colours:** easy = `--grn`, medium = `--warn`, hard = `--bad`. **Area colours:** DSA = `--acc`,
  Rust = `--vio`, Build = `--grn`.
- **Commits:**
  - go on `master` (or the session's branch, fast-forwarded into `master`);
  - **push every commit to `origin/master`** (github.com/genuinebnt/learn-rust), owner's standing request;
  - end each message with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>` (or your model's line);
  - update the status in this file and in ROADMAP §0 when something lands.
- **No prettier config exists.** Don't run prettier over files; it reformats everything to 80 columns. Match the
  surrounding style (about 160 columns, double quotes).

## 5. Content authoring

`content/tracks/<track>/problems/<slug>/` is the source of truth. Each problem folder holds:
- `problem.toml`
- `statement.md`
- `starter.rs`
- `solution.rs`
- `tests/visible.rs`
- `tests/hidden.rs`

`tools/author/` holds the Python specs the 13 tracks were generated from: `author.py` has the helpers, and there's
one script per track. They **reproduce the committed content exactly**, which was checked on 2026-09-27 by
regenerating into a copy and diffing.

To change a track:

```sh
cd tools/author && python3 d9.py                       # rewrites content/tracks/d9-graphs
cargo run -q -p anneal-cli -- verify d9                # from the repo root
```

The script and the folder must stay in sync. Either edit the spec and rerun it, or hand-edit the files and stop
using that script. Don't mix the two. `ANNEAL_CONTENT_ROOT=/tmp/x python3 d9.py` writes into a scratch copy instead.

The test conventions:
- Tests use the `check!(input_description, got, expected)` macro. The runner injects it (`crates/runner/src/project.rs`).
- The workspace parses `check!` calls to show input and expected values per test (`web/src/workspace/testcases.ts`).
- Problem crates use edition 2021, and the package is named `solution`.

Performance problems (section F, CURRICULUM §6.1) add `[perf]` to problem.toml. `validate` rejects tests that use a
helper their `[perf]` doesn't turn on.

| `[perf]` key | Runner does | Tests get (`anneal_prelude::…`) |
|---|---|---|
| `release = true` | `--release` build and run, `--test-threads=1` | `assert_faster(what, factor, runs, baseline, yours)`: medians, interleaved; prints the ratio and fails with a `check!`-style report. `median_time(runs, f)` works in any build |
| `asm = true` (needs `release`) | compiles the library alone first (`cargo rustc --release -- --emit asm -C codegen-units=1 -Z cross-crate-inline-threshold=never`) into `tests/anneal/solution.s` | `asm::function("total")` or `asm::function("Matrix::mul")`: that function's instructions; `asm::calls(body, "panic_bounds_check")`, `asm::uses_simd(body)` (x86_64 and aarch64), `asm::instructions(body)`, `asm::functions()`. Handles legacy and v0 mangling (v0 is what the runner gets, because `RUSTC_BOOTSTRAP` is set). Only non-generic functions can be inspected |
| `count_allocs = true` | installs a counting `#[global_allocator]` in the test binaries | `allocs(\|\| f())` → `(result, Allocs { count, bytes })`, this thread only. Leave it off when the solution defines its own global allocator |

The performance crates (bumpalo, smallvec, hashbrown, rustc-hash, ahash, memchr, bytemuck, arc-swap) were added to
`docker/deps` on 2026-09-27: **rebuild the runner image** (`docker build -t anneal-runner:1.98 -f docker/runner.Dockerfile docker`)
before a problem that uses them runs in the Docker sandbox. Host runs (and `verify`) fetch them from crates.io.

## 6. What's next, in order (ROADMAP §7)

### 6.0a The BusTub course (owner's priority from 2026-10-08)

A CodeCrafters-style course: **clone a repo, finish stages, `git push` runs the tests** ("Build a DBMS: BusTub in Rust"). Design and the owner's
requirements: [BUSTUB.md](BUSTUB.md). Board and pick-up steps: [BUSTUB_TASKS.md](BUSTUB_TASKS.md). **State (2026-10-10): the whole course is restructured and shipped**: the optional Rust
on-ramp `r`, modules 1a to 4c (4c, ACID and recovery, is new), and the optional primers 0a to 0d. Read
[BUSTUB_RESTRUCTURE.md](BUSTUB_RESTRUCTURE.md) §8 for what each module became and [COURSE_STANDARDS.md](COURSE_STANDARDS.md) before touching a stage.
Tools: `anneal course verify [--stage S | --stage unlock]` (never while editing `reference/`), `anneal course lint`, `anneal course solutions`
(the CLI signed in to the live app uploads the solution diffs; re-run after a module ships), `tools/rust_prereqs.py --pages`.
An untied given file that needs a later module's file goes in that module's `files = [...]`. Also built: the lock manager 4d, the "errors you will meet" pages, `anneal course adopt`, `anneal course reset`, the listening run strip, and
**148 optional challenges** (`kind = "challenge"`; generator and packs in `tools/course_challenges/`, see [COURSE_ROADMAP.md](COURSE_ROADMAP.md) §1).
Also built since: module **3j** (outer joins and set operations, 7 stages and 5 challenges, BEYOND BusTub), the CLI install and restore flow ([CLI_INSTALL.md](CLI_INSTALL.md)), module **3i** (the SQL surface) and **4e** (SQL transactions), both BEYOND BusTub and tagged so on the Courses page. Still open: rewriting the older concept articles, the proposed modules in [COURSE_ROADMAP.md](COURSE_ROADMAP.md), and [ADVANCED_DB_COURSE.md](ADVANCED_DB_COURSE.md). The reference solution **is** in the repo (`courses/bustub/reference`, owner's decision 2026-10-10; BUSTUB.md §8); CI turns it into `courses/bustub/solutions.json` at deploy time and the app loads that at start-up. Check with `tools/course-smoke.sh`.

### 6.0 Pending work: start here

Updated 2026-09-27 after D9, D12, L4, L5 and S1 were finished. Everything that passed `anneal verify` is committed on
`master`, one commit per track or stage.
Anything uncommitted in a checkout was unverified; delete it (`git status`, `git checkout -- <path>`, remove new folders)
or verify and commit it. `cargo run -q -p anneal-cli -- list` shows each track's written count.

**Order the owner set: Graphs → DP (both done 2026-09-27) → Rust exercises (L4, L5, S1 done) → Tries → the rest; Trees deferred by the owner (2026-09-27). Since 2026-09-27 the owner wants **one agent at a time**, in this order: D11, D7, F2, and the L1–L3 senior pass are done → S2–S4 senior pass → F4 and the rest of F.** Hand
[authoring/write-track.md](authoring/write-track.md) to an agent per track (fill `{TRACK}`, `{SPEC}`, `{FOLDER}`,
`{CODE}`, `{ORDER}`, `{SEEDS}`); it continues from the spec as it stands. Agents sharing a checkout commit only their own
`tools/author/<track>.py` + `content/tracks/<folder>`, verify with `--jobs 1` (4 CPUs), and retry a rejected push
without pulling. Each new problem needs ≥5 visible (LeetCode's examples), ≥8 hidden, a seeded random check, a scale
test, `wrong/` solutions and `companies` (`verify` enforces the counts and `wrong/`).

| # | Track | Done | Next (in CURRICULUM order) | Notes |
|---|---|---|---|---|
| 1 | D6 Trees (d6.py, order 6) — **deferred** | 8/44: Basics | Traversals, Levels & recursion, BSTs, Ownership-shaped trees; seeds from 609 | reuse `TREE` / `HELP` in d6.py; deep trees run in `big_stack`; count-nodes scale test uses a shared-`Rc` complete tree |
| 2 | D10 Tries (d10.py, order 11) | 11/25: First tries, Tries at work | String algorithms, Hard tries & strings; seeds from 1012 | Autocomplete with hot counts: visible tests type LeetCode's example keystroke by keystroke, incl. `#`; D10's zero-copy tokenizer must be harder than L3's |
| 3 | D11 Recursion & backtracking (d11.py, order 12) | 15/28: Recursion, First backtracking, Choices & grids 6/12 | generate-parentheses, different-ways-to-add-parentheses, word-search, palindrome-partitioning, restore-ip-addresses, Fix: recursive closure can't borrow the grid (flood fill → inner `fn`); then Constraints & pruning; seeds from 1127 | |
| 4 | D8 Intervals & greedy (d8.py, order 9) | 13/29: First greedy, Intervals | Greedy choices (9), Hard greedy (7); seeds 814–829 | |
| 5 | **F · Performance Rust** (new section, CURRICULUM §6.1) | 0/92; runner support built 2026-09-27 (§5) | runner `[perf]` support first, then F2, F4 | replaces Y1 and Y4; grading order: size → counters → asm → relative timing |
| 6 | Not started | | D7 Heaps, D13, D14; L6–L8; S5–S9 (ROADMAP §7 step 1) | write each track's CURRICULUM table first if it still needs the LeetCode 250 pass |

#### Every problem not written yet (checklist)

Tick against `cargo run -q -p anneal-cli -- list <track>`. Stage lists are in each track's CURRICULUM table.

- **D6 Trees (36; seeds from 609):** Traversals: Preorder, Inorder, Postorder, Leaf-similar trees, Count nodes · Levels
  & recursion: Average of levels, Level order, Zigzag level order, Right side view, Iterative inorder, Diameter, Balanced
  tree, Subtree of another, Count good nodes, Path sum II, Sum root-to-leaf numbers, Max width, Flatten to linked list,
  Vertical order traversal · BSTs: Search in a BST, Insert into a BST, Validate BST, Kth smallest, LCA of BST, LCA of
  binary tree, Build from preorder / inorder, Delete node in a BST, BST iterator · Ownership-shaped trees: Max path sum,
  Binary tree cameras, Recover BST, Serialize / deserialize, Insert & delete in an `Option<Box<Node>>` BST, In-order
  iterator with lifetimes (W43), Arena tree with typed indices (W55), Parent pointers with `Weak`.
- **D10 Tries (14; seeds from 1012):** String algorithms: Find the first occurrence (KMP), Repeated substring pattern,
  Longest palindromic substring, Palindromic substrings, String to integer, Repeated DNA sequences · Hard tries &
  strings: Word search II, Autocomplete system with hot counts (W31), Max XOR (bit trie), Stream of characters,
  Concatenated words, Palindrome pairs, Shortest palindrome (KMP), Zero-copy tokenizer (W36).
- **D11 Recursion & backtracking (13; seeds from 1127):** Choices & grids: Generate parentheses, Different ways to add
  parentheses, Word search, Palindrome partitioning, Restore IP addresses, Fix: recursive closure can't borrow the grid
  mutably · Constraints & pruning: N-Queens (bitmasks), N-Queens II, Sudoku solver, Matchsticks to square, Partition to K
  equal subsets, Word break II, Expression add operators.
- **D8 Intervals & greedy (16; seeds 814–829):** Greedy choices: Jump game, Jump game II, Gas station, Partition labels,
  Boats to save people, Two city scheduling, Min add to make parentheses valid, Valid parenthesis string, Queue
  reconstruction by height · Hard greedy: Hand of straights, Remove K digits, Candy, Min interval to include each query,
  Employee free time, Min refueling stops, Course schedule III.
- **Written tracks below their planned size:** S4 Maps & sets has 11 of 18 (the rest are variations of the stage
  anchors; see its CURRICULUM row).
- **Tracks with nothing written** (full lists in CURRICULUM): D7 Heaps 13, D13 Matrix/bits/math 16, D14 Data-structure
  design 20; L6 Closures 14, L7 Enums 14, L8 Error design 16, L9 Modules 10, L10 Macros 12;
  S5 Queues & heaps 14, S6 Iterators 22, S7 Smart pointers 20, S8 Core traits 16, S9 I/O 14, S10 Time & processes 8, S11
  mem/ptr/alloc 12; and sections C (concurrency), Y (systems), B (backend), M, P, H.

Known gaps and proposals from 2026-09-27:
- **S1 API not yet written in a solution:** `and`, `or_else`, `is_some_and`/`is_ok_and`, `inspect`, `unwrap_or_default`,
  `iter()` on `Option`, `let else`, `matches!`, `cloned`, `Option<Box<T>>` niche reasoning.
- **Proposal awaiting the owner:** a pass over L1–L3 and S1–S4 to fill syntax-coverage gaps and raise the easiest problems
  to the senior bar in §4.
- `combination-sum-iii` doesn't catch a permutations-then-dedup solution (~10 s, under the cap); skipped as out of scope.
- D12's recursive-memo fix is E0500 (what rustc reports), not the E0499 CURRICULUM lists.

Also pending:
- **Company tags** on the L/S tracks' LeetCode-style problems (S2 `reverse-each-word`, `word-frequency`; S3
  `rotate-in-place`, `remove-duplicates-sorted`, `chunks-and-windows`): add a `COMPANIES` map + `tag_companies(P, COMPANIES)`
  to the spec, as in d1.py. DSA tracks get tags as they are written.
- **Proposal awaiting the owner:** track cards show only written problems ("8 problems"); show "8 of 44 written" and
  don't mark a track done while CURRICULUM plans more (`web/src/pages/SectionPage.tsx`, `stateOf`).
- **Browser check** of the workspace toggles (autocomplete / rust-analyzer / borrow lanes now saved as editor settings,
  lanes off by default): not clicked through yet.
- **Final pass** once the tracks are written: `anneal verify` on every track, `cargo test -p anneal-content -p
  anneal-runner -p anneal-rules -p anneal-api` (the API tests need Postgres on :5435 and the `rust-analyzer` component),
  update §2's counts and ROADMAP §0.

Done this session (for reference): verifier `wrong/` solutions + prelude `Rng` + host-kill fix; test hardening of all 13
original tracks and the stricter `verify` minimums; CURRICULUM extended for D6/D8/D9/D10/D11/D12; company tags (model,
validation, API, track-page column and filter); saved workspace toggles.

### 6.1 Test hardening (step 2a): done 2026-09-27

**Status (2026-09-27):** plan steps 1 and 2 are built, and step 3 is done for **D1** (every problem has ≥5 visible, ≥8 hidden, a
seeded random comparison, a scale test where one makes sense, and 1–3 `wrong/*.rs`). **Next: D2**, then down the table in §2.

How the D1 pass was written, to repeat per track (see `tools/author/d1.py`):
- Add hidden `T(...)` cases for the edge checklist, then one free-form string holding `random_vs_brute_force` (an
  `anneal_prelude::Rng::new(<seed>)` loop over small inputs, the brute force written inline) and `scale_*`.
- Size scale tests so O(n²) runs past the 15 s test limit in a debug build (n ≈ 10⁵–2·10⁵), and put the work where a
  naive loop can't exit early (e.g. the answer at the end). Raise the problem's `constraints` if the test needs it.
- Build big inputs on the heap (`vec![x; n]`, not `[x; n]`): a large array on a test thread's 2 MB stack aborts the
  binary, which shows up as every later test "timed out".
- `Rng` calls can't nest (`rng.vec(rng.below(n), ..)` is E0499); bind the length first. `rng.pick` in a `&str`
  position needs a `*`.
- Put wrong solutions in the spec's `wrong=dict(name=code)`: the quadratic version, the classic bug, the misread
  statement. `verify` reports any that compile and pass; that means a test is missing, so add one rather than drop it.
- `verify d1` takes about 1.5 minutes, because every quadratic wrong solution runs to the timeout.

The owner asked for real unit tests, not a handful of trivial cases. Today's audit:
- the median problem has 4 tests, and the median hidden count is 2;
- 62 problems have only 3 tests;
- 62 have a large-input test;
- only 5 have randomized comparisons.

Plan (approved):

1. **Verifier support for wrong solutions.** Add an optional `wrong/<name>.rs` in a problem folder: a plausible but incorrect
   solution (off-by-one, wrong complexity, missed edge case). `anneal verify` must fail if any wrong solution passes
   Submit. The code is `verify` in `crates/cli/src/main.rs`, which today checks the reference solution passes and the
   starter fails. Also teach `tools/author/author.py` to write `wrong` entries.
2. **A seeded RNG in the test prelude**, next to `check!` in `crates/runner/src/project.rs`: a tiny splitmix/xorshift with
   no crate dependency. Hidden tests use it for randomized comparisons against a brute-force reference written
   inside the test.
3. **Raise the bar, track by track.** The targets:
   - ≥ 5 visible and ≥ 8 hidden tests, for **every problem, existing and new**. The visible ones must explain the
     problem before a Submit: the main case, the empty or minimal input, and each rule that's easy to misread (order,
     duplicates, direction, ties, the no-answer value). For a LeetCode question, include LeetCode's own examples,
     adapted to the signature;
   - an edge-case checklist per problem type (empty, single, duplicates, negatives, bounds/overflow, max size,
     Unicode for strings);
   - one randomized brute-force comparison where there's a simple reference;
   - one scale test sized so a wrong-complexity solution times out.

   Start with D1 and work down the table in §2. Rerun `anneal verify <track>` after each track.
4. ✅ The verifier now requires ≥5 visible, ≥8 hidden and at least one `wrong/` solution per ready problem.

### 6.2 Then, per ROADMAP §7

- **Step 1 content:**
  - D6 Trees, D7, D8, D12 DP, D10, D11, D13, D14;
  - L6–L8 (L4, L5 done);
  - S5–S9.

  Track outlines are in CURRICULUM.md. Include the §10.2 additions for L4, L5, S4, S6 and S7.
- **Step 2b, Q concept cards** (ROADMAP §10.3): needs a card-mode mockup first.
- ~~Step 2c, AI assistant~~: built, then removed by the owner on 2026-10-07. Don't re-add AI features.
- Designed screens still to build: Today, Readiness, Library, project pages (a Rust mock round later; the DSA mock exists).

## 7. Known rough edges

- Track cards count only written problems ("8 problems" for D6's 8 of 44) and would call a partly written track done once
  those are solved. Proposed fix (awaiting the owner): show "8 of 44 written" and use CURRICULUM's size in `stateOf`
  (`web/src/pages/SectionPage.tsx`; sizes in `PLANNED`, `web/src/curriculum.ts`).
- The saved workspace toggles were type-checked and API-tested but not clicked through in a browser.
- The Run tests button changes width by about 3px between idle and busy.
- With five file tabs open at 1280px, the tab list scrolls sideways (the scrollbar is hidden). It works, but
  isn't obvious.
- `cargo test -p anneal-runner -- --ignored` (the Docker sandbox test) only runs where the runner image exists.
- The web bundle is over 500 kB (Vite warns). Code-splitting CodeMirror would fix it; it's low priority.
