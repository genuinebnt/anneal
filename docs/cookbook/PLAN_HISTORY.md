# Cookbook: planning record

The notes written while the cookbook was being designed, kept for the reasons behind decisions. Later sections override
earlier ones, and much of it was never built. **For how the cookbook works now, read [../COOKBOOK.md](../COOKBOOK.md).**

---

# anneal · Cookbook section (plan)

**Status: planned, not built (written 2026-11-26).** No app code, content or migrations exist for it; a mockup awaits approval (section 11). The owner agreed with
the direction and asked for it to be added to the plan, not implemented. Each phase needs the owner's go; new screens need
approved mockups first (CLAUDE.md).

## 1. What it is

A shared library of Rust and systems reference material that other areas link into. It is **not** another course to finish.

**Reference only (owner, 2026-11-26).** No Learn or Practice views, graded exercises, progress tracking, follow-ups or scheduled
reviews. The effort goes into additional context, easy viewing, extra information, search and discovery. (Earlier drafts had
later phases for learning and practice; they are removed, not deferred.)

## 2. Source material and its state

- Source: the cookbook crate in the FlowForge repo, `~/projects/flowforge/cookbook`: several hundred `.rs` files (about 245k
  lines in `examples/` when counted on 2026-11-26, and it is still growing).
- It has runnable, self-checking code, headers with level and common uses, study guides, reading paths and practice pages.
  Not every file has every header.
- Agents wrote much of it. Passing its own asserts does not prove an explanation or claim is right. Some of it is unfinished or
  unchecked: read `cookbook/handoff/README.md` before copying anything.
- **FlowForge stays the source for now.** Copy the pilot into anneal, then decide which copy is maintained before adding more, so
  the two do not drift.
- Compared with COURSE_STANDARDS ("researched, measured, never invented"), it has not been audited.

The source is strongest as reference already. What is missing is fast lookup and easy viewing in the app.

## 3. Pages

`/cookbook/<recipe>`: one reference page, with two densities (section 13). Browse by "I want to ...", by reading order, or from
the command palette. Not by a folder tree.

## 4. Content model

```
content/cookbook/<slug>/
  recipe.toml    # level, tags, prerequisites, uses, spoils, status
  page.md        # the reference text
  recipe.rs      # the one source of truth; the page shows marked sections of it
```

One idea per page. Splitting large files is editing work, not an import script.

**Status** per recipe:

| Status | Meaning | Shown to readers |
|---|---|---|
| `draft` | copied, unchecked | no |
| `verified` | builds and passes; links and claims checked | yes, labelled |
| `reviewed` | the owner has read and approved it | yes, labelled |

## 5. Spoilers

The cookbook likely overlaps the BusTub and DSA exercises (database structures, caches, tries; check per recipe).

- Each recipe lists what it `spoils`; it stays locked until that exercise is done, with a deliberate override.
- Filtering happens **on the server**: pages, search results, snippets and related lists.
- **Exception (owner, 2026-11-26):** the BusTub and Lox solutions from the two guides are kept and are not locked (section 14).
- This is new work. `web/src/spoilers.ts` only hides a problem's key idea in the browser, by a preference.

## 6. (Removed: practice)

Dropped by the reference-only decision (section 1). The section number is kept so other references stay valid. Anything stored
(notes, pins, reports) is still additive and backed up (CLAUDE.md data rule).

## 7. Search

### 7.1 What exists

Read on 2026-11-26: `crates/api/src/meaning.rs` up to its tests. `bge-small-en-v1.5` (384 dims, pinned revision) on CPU through
candle; unit vectors; exact cosine over all rows; query embedding on a blocking thread; DSA problems only, with a small bonus for
shared rare topics. `content/dsa/meaning.bin` (2.6 MB) carries one fingerprint over all texts; a stale or missing file is rebuilt
in memory at start-up when the model is there.

Seen by text search only, not read: the palette's `~` meaning search and similar-problems view (`Palette.tsx`); the graph page's
similarity modes.

(ROADMAP §11 and migration 0007 removed the AI assistant and its embeddings table. Local meaning search is separate and stays.)

### 7.2 What changes

1. **Index sections, not files.** The embedder truncates at 512 tokens. A unit is one explanation section or important code item,
   carrying its recipe and heading.
2. **Add exact text search.** Meaning search can miss `get_or_insert_with` or `E0502`. Words, identifiers (split snake_case and
   CamelCase, keep the whole name too) and partial matches. In memory; Tantivy is a candidate (not evaluated).
3. **Describe code for the model.** Assumption to test: this general model handles raw Rust poorly. Embed title, purpose, doc
   comment and signature; text search covers the code.
4. **Incremental vectors.** A hash per section; embed only what changed. Build in CI and ship with the release instead of
   committing a large binary that changes often, and do not rely on a start-up rebuild. Check first that CI can fetch the model.

### 7.3 Pipeline

```
query -+- text search ----+- merge by rank -> filter -> group by recipe
       +- meaning search -+
```

- Merge with reciprocal rank fusion (the two scores are not comparable). Exact name matches go first.
- Text results appear while typing; meaning results join after a pause. The selected result must not move.
- A result opens the matching section, with a snippet and why it matched.
- Server-side filters: level, tags, status, spoilers.

### 7.4 Related recipes

Each group says why an item is there:

| Group | Source |
|---|---|
| Read first / next | written links (similarity cannot give order) |
| Similar | vectors of a short per-recipe summary |
| Shares concepts | tags, libraries and methods used |
| Applied in | links to DSA problems and course stages |

Limit repeats from one recipe. Show "how these were found" (the existing idea from the Learn redesign). Add recipes to the graph
page later. High similarity also flags possible duplicates for the owner to review.

### 7.5 Context search

Help based on what the reader is doing:

| Context | Query built from |
|---|---|
| compiler error | code and message |
| problem or course stage | its concepts, after spoiler filtering |
| recipe section | the section itself (related sections) |
| selected text | the selection |

Shown in a side panel **only when asked**, so it never volunteers hints. **Open question:** does "fast context search" also mean
supplying context to an AI assistant? If so it would use the same search with a size limit.

### 7.6 Scale

No vector database for now. Estimate: a few thousand sections (not measured). At 10,000, raw vectors are about 15 MB
(10,000 x 384 x 4 bytes); exact comparison should be fast enough. Query embedding is the likelier slow part: **measure it on the
production server**, along with memory use. Add approximate search or pgvector only if measurements call for it.

### 7.7 Evaluation (before any tuning)

About 50 searches the owner would really type, each with its expected sections: exact names, vague descriptions, compiler errors,
and queries that should return nothing. Compare text, meaning and combined: recall in the top five, the rank of the right section,
latency. Try a code-focused model or a reranker only if this shows a need.

Eight pilot recipes can test the plumbing but not ranking quality at size. Before choosing a model or ranking method, also index a
larger private set, hidden from readers.

## 8. Phases

| # | Phase | Notes |
|---|---|---|
| 0 | Mockups: home, recipe page (Brief and Full), search results, related panel | owner approves before building |
| 1 | Pilot: about 8 recipes from error handling and the std tours, verified and split into sections; recipe pages; text search; spoiler and status filtering | write the 50 test searches here |
| 2 | Meaning search (existing model), incremental vectors, rank merging; run the evaluation | measure latency and memory on production |
| 3 | Related recipes with reasons; duplicate report | |
| 4 | Context search | settle the open question first |
| 5 | Judge after two weeks of use (were answers found quickly? which searches failed? which pages were revisited?), then expand one chapter at a time | only what gets used |

**Main risk:** importing everything at once and getting a large library that is not trusted or opened.

## 9. Open decisions for the owner

- Approve or change the mockup (section 11).
- Context search: for the reader only, or also for an AI assistant?
- Which eight pilot recipes.
- Which copy of the cookbook is maintained after the pilot.

## 10. Grouping, viewing and context (owner asked 2026-11-26 to add all of the brainstorm)

All of it is in the plan. "First" and "Later" below are sequencing, not scope cuts.

### 10.1 Grouping

Each recipe has **one home topic**; every other way of browsing comes from tags, not from a second folder tree.

| Browse by | Question it answers | Examples |
|---|---|---|
| Task (front page) | what am I trying to do? | retry safely, parse input, limit memory |
| Topic (the home) | what is this about? | language, errors, concurrency, databases |
| What you are building | what does my project need? | job queue, storage engine, language server |
| Problem you are seeing | why is this failing? | `E0502`, deadlock, slow allocation |
| Reading path | what do I read in order? | ownership, then smart pointers, then arenas |

**Page kinds** (`kind` in recipe.toml): concept (one idea), API guide (methods and when to use them), pattern (a recurring design
problem), comparison (choosing between options, e.g. `Mutex` / `RwLock` / channels), build (several ideas working together).

### 10.2 Viewing

- Home: search first, then task cards, recently viewed and pinned pages.
- Recipe page in the three-panel frame: left "on this page"; centre short answer, code, diagram, pitfalls, full source
  (collapsed); right read first, alternatives, applied in, similar (with why), your notes.
- Start with the answer: what to use, when, and its main drawback.
- Code in steps: smallest useful example, a realistic one, then the full source.
- Preview a related idea without leaving the page, on request.
- Compare two recipes side by side on wide screens.

### 10.3 Additional context (little by default, the rest on request)

| Group | Content |
|---|---|
| Choosing | when not to use it, simpler alternatives, what would change the choice |
| Cost | speed, memory, complexity; each claim says whether it was measured |
| Failures | likely errors, symptoms, how to diagnose |
| Real code | checked links to specific files, and why they are worth reading |
| Version | Rust and library versions used; when the page was checked |
| Trust | what was run, which sources were checked, what the owner reviewed |
| Connections | prerequisites, next topics, where it is applied, with spoilers hidden |

### 10.4 Ideas

| Idea | What it is | When |
|---|---|---|
| Decision guides | a few questions about the need, then options with why they fit and their drawbacks | first |
| Error lookup | paste a compiler error, get explanations and fixes | first |
| Cheat sheets | one compact page per topic, generated from reviewed recipes | first |
| Broken and fixed code | side by side, the important change highlighted | later |
| Step-through diagrams | ownership moving, tasks waiting, data changing | later |
| Measurement lab | recorded benchmarks with the machine and inputs that produced them | later |
| Symptom guide | symptom, what to check, likely causes | later |
| Idea map | follow connections; for exploring, search and lists stay primary | later |

**Smallest set for the pilot** (recommended): task browsing, the short answer at the top, related ideas with reasons, one
comparison guide, section notes. Notes and pins store new data: additive migration and backup (CLAUDE.md data rule).
Diagrams only where they explain better than text.

## 11. Mockup

`docs/mockups/cookbook.html` (published: https://claude.ai/artifact/GimwEU6WbtUNpdus1V3F1i): **awaiting the owner's approval; nothing is built from it.** Open decisions it raises: Cookbook as
a fifth header area (as drawn) or a tab inside Rust; its area colour (drawn with `--mac`). Counts, scores and notes in it are
sample values, and nothing typed into it is saved. It inlines `design.css`, `layout.css` and `ui.css` as they were on 2026-11-26;
new styles are prefixed `ck-` (port them with the usual mockup rule). Screens: **two pages in the reading view built from real guide sections** (Join algorithms; Rust techniques, one added step-through
and hand-written terms, summary line and resource notes), home, recipe (Brief and Full), search
and its states, error lookup, comparison guide, context help, pinned and notes, cheat sheet, and rough shapes for the later ideas.
Updated for section 12: the best search result shows the answer and snippet; `core`/`alloc`/`std` labels; claim labels; a
"Wrong or unclear?" control per section and a reports list; a recipe right column with only Read first, Similar and notes open;
trust and freshness fields. Its snippets are still hand-written.

## 12. Improvements (owner asked 2026-11-26 to add them)

Order of importance. The first two build trust; answers inside search make it quicker to use. **Start with:** compiled snippets,
tests that can fail, answers inside search. The idea map and measurement lab wait.

### 12.1 Make "verified" mean something (extends the status table in section 4)

Why: while repairing the source cookbook on 2026-11-26, some checks passed while testing very little. A loop-optimiser test never
ran a trapping program although it was meant to check traps; an agent reported that a CRDT merge which dropped deletes would
still pass the merge-law trials. "It builds and runs" is not enough.

| Requirement | Meaning |
|---|---|
| Compiled snippets | every snippet on a page is cut from source that CI builds; no hand-written copies |
| Tests that can fail | break the key behaviour on purpose (a planted bug) and require a check to fail; keep the planted bugs runnable |
| Labelled claims | each claim says: tested here, from a linked source, or advice |
| Freshness | record Rust and library versions; flag a page for rechecking when they change |

A convergence-only check cannot catch a bug that makes every replica wrong in the same way: it needs an oracle too.

### 12.2 Faster answers

- A search result shows the short answer and smallest snippet without opening the page.
- Method-level anchors: searching `get_or_insert_with` opens that section.
- **API home labels** on every std-related page and hit: `core`, `alloc` or `std` (where the API is defined; `std` re-exports
  much of the other two). It tells whether the item works with `no_std` (`alloc` also needs an allocator). Check each label against
  the docs; do not write separate core and std copies.
- Recipe page right column: "Read first", "Similar" and notes stay open; the rest is collapsed.

### 12.3 Small and focused

One idea per page with longer source linked underneath; merge duplicates after the owner reviews what similarity flags; add pages
for real needs (searches, compiler errors, current projects).

### 12.4 Learn from use

A **"wrong or unclear"** button on each section. Reports sit with the private gap list. After two weeks check: were answers found
quickly, which searches failed, which pages were revisited or never opened. Reports are new stored data (additive, backed up).

### 12.5 Source coverage gap

On 2026-11-26 the source had 16 std look-up files (`examples/std_tour/`). The interrupted briefs name about 30 more that do not
exist, and there is no dedicated `core` reference. Method coverage of the 16 has not been checked against the docs.

## 13. Reference viewing and extra information (owner, 2026-11-26)

| Feature | What it does |
|---|---|
| Brief / Full | Brief: short answer, at a glance, smallest snippet, pitfalls. Full adds signatures, common tasks, choosing and more detail |
| At a glance | API home, stable since, when it allocates, trait bounds, crates needed. Each fact checked against the docs |
| Key signatures | the few signatures worth seeing, each with one line on what it does; link to the official docs |
| Common tasks | short "how do I" snippets on the same topic, each copyable |
| Jump list and expand all | "On this page" jumps to a section; one control opens or closes all folded detail |
| See also | official docs and checked real-code links, with why each is worth opening |
| Symptom guide | reference form of the incident idea: symptom, what to check, likely causes (not a lesson) |

Reading order (paths) is navigation only: no read or done states. Comparison guides stay: they help choose, they do not test.
Where earlier sections say "incident lessons", use the symptom guide.

## 14. Second source: the owner's two guides (added 2026-11-26)

`~/dbms.html` ("Cairn: Databases, Layer by Layer") and `~/compiler.html` ("Lox in Rust, Slice by Slice").

### 14.1 Reuse rules

- **Snippets in the HTML are compiled and tested (confirmed by the owner).** Take them directly from the HTML, unchanged, with
  the guide and section recorded, marked "compiled and tested (confirmed by the owner)". They have not been rerun for this plan.
- Once in anneal they join the CI build like other snippets (section 12.1). Bring along any code they depend on.
- **Keep the BusTub and Lox solutions (owner's decision).** No spoiler lock for them. The rest of section 5 still applies.
- The decision covers the snippets. Claims and "Measured" figures in the surrounding text were not checked here; label them with
  their source and machine. Check reuse rights for any text or code that came from someone else.

### 14.2 Presentation to take

| In the guides | In anneal |
|---|---|
| strip of database layers a section touches | "Where it sits" in the system |
| callouts: Rust, performance, testing, pitfall, in production | the same typed boxes for extra information |
| "How production systems pick" | "In production": what real projects do, with sources |
| comparison of rustc, swc, oxc and rlox | one concern compared across real codebases |
| pipeline diagram and test-oracle tables | each stage and how to check it |
| references grouped by kind, reading maps, glossary | grouped sources, browsing by goal, term definitions |

Not taken: progress checklists and exercises (reference only, section 1).

### 14.3 Topics the cookbook did not appear to have

Found by comparing the guides' headings with file headers and code searches. The guides were only sampled, searches can miss
different wording, and a heading match elsewhere does not show depth.

- **Databases:** the update-during-scan (Halloween) problem, larger-than-memory databases, the PostgreSQL wire protocol, SCRAM
  authentication, row-level security, HTAP, running a database engine in WebAssembly.
- **Compilers:** JIT inline caches and deoptimization, tensor compilers, Tree-sitter grammars, a vector-clock race detector,
  compiler hardening and sanitizers, variance computation, incremental reparsing.

Related basics already exist (heap files, B+tree deletion, vector clocks, a Wasm interpreter). Start imports with these gaps;
where both cover a topic, compare and keep the clearer, better-tested version.

## 15. Page format and the whole-cookbook outline (owner, 2026-11-26)

**Outline:** [cookbook/OUTLINE.md](OUTLINE.md) lists every module, chapter and section; links are in
[cookbook/resources.tsv](resources.tsv). The grouping is a proposal for the owner to review.

**Owner:** pages should be like the two guides (descriptions, tables, tests, callouts and asides, tips and tricks, explanations),
use what anneal already renders, and look good. **BusTub Rust solutions can be made public** (owner's statement; the owner is not a
student). This and section 14 replace the earlier spoiler concern for the BusTub and Lox solutions.

**Use the System design reading view** (`SdRead.tsx`, `SdMd.tsx`, `styles/sd.css`) as the cookbook page:

| Page part | Existing piece | New work |
|---|---|---|
| contents with progress, width / text / spacing, focus mode | `rd-toc`, `rd-sets` | none expected |
| quick summary | `rd-sum` | "Key ideas" from the guides goes here |
| numbered sections with a copyable link | `rd-sec`, `rd-h` | |
| callouts | `sd-call`: NOTE, TIP, WARNING, ASIDE | add RUST, TESTING, PRODUCTION kinds (renderer, content checker) |
| code, file name, highlighted lines, line links, tabs, copy | ` ```rust title= hl= group= ` | wrapping long lines instead of hidden scroll (as mocked) |
| collapsed code and tests | `:::deeper title \| meta` | label reads "optional"; decide for reference pages |
| step-throughs, maths, margin notes, tables | `:::steps`, `$$`, `^[...]`, `sd-tbl` | |
| resources | `sd-srcs` / `rd-srcs` cards | resource model below; filter by type |
| glossary and related | `rd-terms`, `ly-cards` | |
| layer strip ("Where it sits") | none | new |

`courses/` has more (` ```svg ` figures with captions, ` ```rust test `, WHY and PORT callouts in `courseMd.ts`); reuse them if the
renderers are merged. INTERVIEW and CHECK are left out of reference pages (section 1).

**Page template** (use what the topic needs, never pad): description, where it sits, quick summary, explanation, comparison table,
code with its tests, callouts (pitfall, in Rust, testing, in production), tips and tricks, step-through or figure, terms,
related, Go deeper, source and verification line.

**Resources** (owner: blogs, writeups, papers, GitHub code, books, videos, links, articles, series). Each has: type, title, one
line on why to open it, URL, status (loads / unconfirmed / broken), date checked, and the exact file or chapter for code and
books. A loading link is not a read one: only say what a resource shows after reading it.

**Guide conversion:** the mockup converts two real sections by script (text unchanged; exercises and "Done when" lists dropped;
a word-level comparison found no other words missing). It is a mockup converter, not a production importer.

## 16. Code viewer (owner, 2026-11-26)

**Owner:** code blocks large, runnable and readable like VS Code, and **not editable**. References given: the owner's blog post
`blog.genuinebasil.dev/writing/crafting-a-parser-lexer` and the problem page `/p/l1-fix-use-after-move` (read from source, since it
needs a login).

| Part | Design (in the mockup) | In anneal |
|---|---|---|
| look | blog style: large Fira Code, numbered lines, click a number to highlight the line | CodeMirror with `EditorState.readOnly` and the theme in `workspace/Editor.tsx`, or the static `sd-code` lines |
| files | workspace file tabs (`ftabs`, `ftab`) | existing styles |
| actions | Run, Test, Copy, text size, wrap (on by default), expand to full screen | |
| results | the workspace console (Compiler, Output, Tests) and terminal card; hidden until used | existing `console` and `term` pieces |
| fragments | marked "not runnable alone", no Run | needs a per-block `runnable` flag in content |

**Running:** the runner already has `run_scratch(id, Scratch { lib_rs, main_rs, crates })`, but its route is tied to a problem
(`/problems/{id}/scratch/run`) and saves drafts. The cookbook needs its own route that runs **only the page's stored files**, chosen
by page and block id, never code sent by the browser (the viewer is read-only, so no free-form execution is needed). Tests need a
test run, which `run_scratch` does not do. Results can be cached by file hash and toolchain. Multi-file layouts and dev
dependencies (the join tests use `proptest`) must fit the runner's fixed crate set.

**Mockup limits:** it cannot compile; Run and Test replay output recorded on 2026-11-26 with rustc 1.98.1. The join library and its
tests are the guide's code unchanged (3 tests passed); `main.rs` and the `Cow` program were written for the mockup.

## 17. Code-first layout and visual design (owner, 2026-11-26). Replaces layout points in sections 13, 15 and 16

- **Code first.** Left panel toggles with `[`, right context panel with `]`; both remembered. Context starts closed.
- **One width** for the whole article (text, callouts, tables, step-throughs, code). The width setting changes all of it; Full is the default.
- **GitHub-like code:** its light and dark syntax colours, file header (name, lines, size), a symbols list that jumps to a line,
  click and Shift-click line ranges. 16px code, wrapping on by default. The page scrolls; file tabs and the console stay in view.
- **Visual language:** hero with page facts counted from the page, "Where it sits" as a pipeline, quick summary as cards,
  numbered section badges, icon-tile callouts, framed tables, resource cards. No divider lines, underlines or visible scrollbars.
- Implementation notes: the mockup overrides several `sd.css` widths (including the reading view's full-width table rule); avoid
  the short class name `.cl` or namespace it.

## 18. Testing and benchmarking module (owner, 2026-11-26)

A module of its own (outline module 4) with **12 method guides** (test design and TDD; property; fuzzing; mutation; integration,
smoke and black-box; API; async; concurrency; memory-ordering bugs; unsafe and unsoundness; security and vulnerability
checking; benchmarking and profiling) and **3 system guides** (distributed, databases, compilers). Snapshot, golden and
regression tests, coverage, flaky tests, test doubles, load, soak and chaos testing are planned pages inside them. Services and
APIs are covered by the API guide and the by-system table.

Every guide answers: what can go wrong; the oracle; how to find bugs; how to show the check can fail; what to benchmark; how
to report it. The module's home page starts from "what are you testing" (a table), then method cards, when each kind of test
runs, and tests that can fail. **Only the plan and a mockup home page exist; no guide pages are written.** The table text is a
draft. Curated tool links loaded on 2026-11-26 but were not read; check each tool's current limits before writing.

## 19. Order of work (owner, 2026-11-26). Replaces the order in section 8

**Owner:** once all recipes are done and the additional information is generated, move the cookbook into anneal and implement it
there. **No anneal implementation before that.** FlowForge stays the source until the move.

Proposed definition of "ready to move" (not yet confirmed by the owner):

1. **Recipes:** every kept recipe builds, runs and passes; known gaps are closed or listed as accepted; full `./check.sh` green.
2. **Pages:** each outline section has its page written (see the format below) or is explicitly dropped; duplicates merged.
3. **Additional information:** what the topic needs of description, tables, callouts, tips, tests, terms and related links.
4. **Resources:** broken links fixed or removed; each cited resource read for what the page says it shows.
5. **Checks:** snippets come from compiled files; claims labelled tested, sourced or advice; planted-bug checks where they matter.

**Write pages in anneal's content format from the start** (`SdMd` Markdown: callouts, code fences with title, `:::deeper`,
`:::steps`, tables, margin notes), so the move needs no rewrite. Settle the format and the page metadata, and get one finished
page per page kind approved, before generating pages in bulk.

## 20. The mockup is built from anneal's own parts (2026-11-26)

After "design looks good but missing what makes anneal anneal", the mockup was rebuilt from markup captured from a disposable
local instance (temporary Postgres container; the dev and production databases were not used) plus the built app stylesheet:
the real header (Cookbook as a fifth area, sliding thumb and tab pill, clock, focus timer, accent picker, theme, bell, avatar),
the ticker, the catalogue page structure (`ly-*`, `s-next`, `s-trk`, `d-opt`, keys panel), animated module-map diagrams and the
command palette with its preview pane. Reference only: no streaks, solved counts or progress bars. The "verified pages"
percentages and "last viewed" are samples. **When implementing, use the real components, not the mockup's copies.**

## 21. Book navigation: GitBook's structure, anneal's look (owner agreed 2026-11-26). Replaces panel contents in section 17

| Part | Behaviour |
|---|---|
| Left panel | the whole book: modules, chapters, pages. Current page highlighted, its branches open and scrolled into view; filter box and collapse/expand stay at the top. Unwritten pages are dimmed and not links |
| Right panel | this page: contents with progress through the page, symbols in the code, then terms, callout filters, related; reading settings collapsed at the bottom |
| Foot of page | previous and next page in book order |
| Panels | both open by default (changes "context starts closed"); `[` and `]` hide them, remembered. Mockup at 1500px: code 886px with both, about 1140px with one hidden, 1402px with neither |
| Links | a permanent address per page and per heading (heading `#` buttons) |
| Search | the command palette, scoped to the whole book or the current module |
| Home | unchanged: browse by task; the tree is for reading the book |

Not taken from GitBook: its theme, a block editor (pages are files), public comments.

The mockup's tree holds 765 entries in 15 modules and 82 chapters from the outline; titles from file names ("Drop raii") are
placeholders, so every page needs a written title. The same title appears twice in a chapter in 2 places (to merge). Palette
scope and page links are partly mocked.

## 22. No stored user features; subject tabs; index pages (owner, 2026-11-26). Overrides anything earlier that conflicts

**Owner:** no additional features such as user notes. **Removed:** notes, pins and pinned collections, "wrong or unclear"
reports, the gap list, recently viewed and "pick up where you left off". This drops section 12.4, the notes and pins in
sections 3, 10.2, 10.4 and 13, and the related migrations. The cookbook stores nothing about the reader beyond existing anneal
preferences (theme, accent, panel and reading settings).

**Tabs are subjects** (owner agreed): Overview, Rust, Algorithms, Systems, Data, Distributed & web, Compilers, Testing. Page
names are never tabs. Compare is a page kind and errors are found through search. Projects has no tab (small, spans subjects);
it stays in the whole-book tree. While reading, the matching tab is lit and the tree shows that subject, with a switch to the
whole book.

**A subject page is a one-page index:** every entry as a row (name, one-line description, state: draft page, guide text, code
only, planned), grouped by chapter, with a filter. Descriptions come from file headers and guide text and are unchecked.
**Condition for shipping:** a subject should not be a list of mostly unopenable rows; ship subjects as their pages are written.


## 23. Queued after the first release (owner, 2026-11-26)

- Ideas from books: *Rust in Action*, *Rust for Rustaceans*, *Rust Atomics and Locks*, *Zero To Production in Rust* (others
  of the same kind welcome).
- **Separate sections** drawn from https://www.40tude.fr/rust and
  https://nextechtide.blogspot.com/2025/03/going-in-with-rust-interview-prep-guide.html.
- Rules: read each source before using it; use it for topics, structure ideas and further reading; write our own text and
  examples, never copy; link and credit it. Say which parts were read.

## 24. First release (2026-11-26; overrides the "order of work" in section 19)

The owner asked to commit and deploy now, ahead of finishing all content.

- **Shipped:** Data (104 pages) and Compilers (62 pages), converted from the two guides, plus six draft samples. All guide
  material is public, including the Lox project review pages (owner: keep them). Exercises and "Done when" lists are left out;
  reading links became sources. Four code blocks that were malformed in the database guide are repaired by the converter
  (the guide itself is unchanged), as are unescaped generics such as `Res<String>`.
- **Checked:** content validation; a browser test opens all 172 pages and looks for leftover page syntax and sideways
  overflow. The guide pages' code was **not** recompiled here (the owner states the guides are compiled and tested), and
  their prose has not been reviewed in the new layout page by page.
- **Not built:** running code from a page, meaning (embedding) search, related pages, compare and error look-up pages,
  subjects other than Data and Compilers.
- **Research for what comes next** (in the FlowForge repo, `cookbook/handoff/ideas/`): books.md, 40tude.md,
  interview-guide.md. 40tude states copyright and no article licence was found: link, never copy.
