# Cookbook content plan

Status (2026-11-27): the owner approved building the new components and the pilot chapter with agents, as local
pages. Later the same day the owner asked for these changes to be pushed in batches once checked, without waiting.
Current run: phases 0-2. Phases 3-5 wait for the owner to read the results.

## Goal

Help the reader understand a topic and apply it, especially when stuck. It stays a reference: no quizzes, progress,
notes or anything stored about the reader.

- Cookbook: understand a topic, read working code, see how ideas connect.
- Rust exercises and series: practise.
- FlowForge: combine them in one project.

## Where it stands (measured 2026-11-27)

Words are counted outside code blocks; explanations written as code comments are not counted. Word count is not a
quality measure. Only parts of a few pages have been read for quality.

| Page type | Pages | Median words | Note |
|---|---|---|---|
| Guide chapters | 210 | about 920 | 100 over 1,000; 48 under 400 |
| Code example pages | 481 | about 320 | 98 under 150 |
| Project pages | 10 | about 370 | mostly brief |
| Draft samples | 6 | about 2,360 | not reviewed by the owner |

154 of the 701 non-draft pages have under 200 words. Data is strongest (99 of 178 pages have 800 or more). Rust is
thin (2 of 155 reach 800; 50 are under 200).

## Page types and length

Rough guides, not targets. The full code file and optional folds don't count.

| Type | Words | Must have |
|---|---|---|
| Quick lookup | 150–400 | the answer, a short example, limits, related links |
| Focused lesson | 600–1,200 | one concept: how it works, example, when to use it, mistakes, connections |
| Large topic | 1,200–2,500 | connected ideas, comparisons, diagrams, deeper examples |
| Project guide | 1,000–2,000 | what is built, how the parts connect, key decisions, links to lessons |

Split a page when it teaches two ideas, when one part must be learned before the rest, when a section is worth looking
up on its own, or when optional detail buries the main explanation. A lookup page may stay short.

## Page layout

One page, no tabs for explanation, code and notes.

1. **In short** (50–100 words): what, when, main trade-off. **Jump to code** button.
2. The problem: why it exists.
3. How it works: plain explanation, diagram if something changes over time.
4. Worked example: short code, explained step by step.
5. Full code: the existing read-only viewer.
6. Choosing it: comparison table, when not to use it.
7. When it goes wrong: mistakes, symptoms, tests.
8. Connections: know first, practise, reused in projects.
9. Learn more: sources, each with a reason to open it.

Leave out sections that don't help. Lists of method names become a table saying what each is for. Exercise links are
labelled related, similar or exact; full solutions stay closed until asked.

## Writing rules

Write like a good engineering blog article that teaches (owner, 2026-11-27): open with a concrete problem, connect
each section to the next, talk to the reader as "you", and show an example soon after each idea. Look at well-regarded
articles and style guides when unsure; learn from them, never copy.

- No jumbled blocks: paragraphs at most 4 sentences and about 70 words; sentences mostly under 20 words, none over 30.
- No chains of terms joined by semicolons, colons or arrows; no list of method names without what each is for; no
  bullet that is a paragraph. Old compressed notes are rewritten, not carried over.
- Don't overcorrect into one-line fragments or a wall of boxes.
- Check lengths with a script, then reread the whole page as a reader. A length check can't judge clarity.

- Headings say what the section answers ("What happens when the queue is full?").
- One idea per paragraph, usually 2–4 sentences, main point first.
- Explain a term when first used.
- Little bold. Numbered lists for ordered steps, bullets for the rest, tables to compare the same details.
- The example follows the idea it shows.
- The basic explanation is never inside a fold.
- Test: after one read, can you state the main point and when you'd use it?

## UI features

Use three or four inside a lesson, not all of them.

| Feature | Use | Exists? |
|---|---|---|
| In short box, Jump to code | quick lookup | callouts exist; the jump needs checking |
| Highlighted code lines, margin notes, "does not compile" fences, "go deeper" folds, step sections, tables, callouts | explanation | supported by the page format |
| Full code viewer, On this page, reading controls | reading | built |
| Step-by-step diagram | change over time | check whether current diagram tools can show it |
| Connections box | links between topics, exercises, projects | needs adding or adapting |

Reading: about 16.5px text with open line spacing. Pages stay full width; keep explanation lines short enough to
follow, give code, tables and diagrams more room, and put side notes in the spare space so it doesn't look empty.
On phones, side notes and panels move into the page or fold. The Cookbook cannot run code.

## Learning and reading features

Build "first" features with the pilot chapter. All must work with the keyboard and on a phone, never move on their
own, and leave the lesson understandable if never touched. Nothing is stored about the reader.

| Feature | What it does | When |
|---|---|---|
| In short + Jump to | straight to the answer, the code or common mistakes | first |
| Step-through diagram | Back and Next through a changing state (a queue filling, a move, waiting threads) | first |
| Code walkthrough | pick a step; its lines are highlighted beside the explanation | first |
| Wrong and fixed examples | the mistake, its error or wrong result, then what changed and the trade-off | first |
| Connections box | know first, practise, where the idea returns, real project use | first |
| Choosing guide | compare options by what the reader needs | next |
| Term previews | hover or tap a new word for a short definition | next |
| Section links and previews | see a related page before opening it | next |
| Better search | jump to a section or code symbol, not only a page (check what exists) | next |
| Large diagram view | open a detailed diagram at a readable size (check what exists) | if needed |
| Print or reading layout | the lesson without side panels | later |
| Predict, then reveal | think before opening an answer; nothing scored or saved | **ask the owner first**: it feels like practice |

Not planned: running code (until it can be done safely), scores, saved notes, progress in the Cookbook, a large
concept map.

## Keeping hand-written lessons safe

`tools/cookbook/convert.py` deletes and regenerates every page outside `content/cookbook/samples/`, from code comments
and two guide files outside the repository. A rewrite made in a generated file would be lost.

Before rewriting any page (phase 0):

1. Decide where written lessons live (their own folder, or the full format used by the samples).
2. Make the converter leave them alone **and** keep the same address, subject, chapter and order. It currently
   reserves sample names, so check how a lesson replacing a generated page behaves.
3. Add a test: run the converter, confirm the written lessons and all page addresses are unchanged.
4. Keep every existing page address; exercises and the series link to them.

## UI review loop

Every change to what the reader sees: a builder makes it; a separate reviewer that changes no code takes screenshots
(about 1500, 1100 and 390 wide, dark and light, every state of each new block), looks at them and lists problems; the
builder fixes; the reviewer checks fresh screenshots. Repeat until no important issue is left, then the mentor looks at
the final screens before release. This covers new lesson pages too, not only components.

## How each lesson is written

1. Read the whole code file and the current page.
2. Check claims against the code and primary documentation.
3. Open every source link; for "exact", read the linked material and confirm it solves the task.
4. Compile and run every new example (`cookbook/check.sh`); error examples must fail with the stated error.
5. Reproduce any behaviour claim that looks doubtful; report code bugs rather than write around them.
6. Check spoilers against the exercises that link to the page.
7. Validate content, check the page at wide and phone widths in dark and light.

## Phases

Each ends with a review. The next does not start until the owner says so.

### Phase 0: mockup and safety

- **Bounded channels** as a working local page with real content (about 700–1,000 words), wide and phone, with the
  "first" features (in place of a static mockup, at the owner's request to build).
- The converter change and its test.
- **Gate:** the owner approves the layout and the writing.

### Phase 1: one pilot chapter

- Build the approved layout; write the chapter around queues and workers: `bounded-channel`,
  `mutex-condvar-channel`, `worker-pool`, `thread-pool-work-stealing`, `graceful-shutdown`.
  (Check these against the current chapter list; they may sit in more than one chapter.)
- **Gate:** the owner reads it while doing series parts 1–3 and says what was missing.

### Phase 2: the pages the web server series uses

The series links to 19 pages; 14 have under 400 words. Order follows the series.

| Series parts | Page | Words now |
|---|---|---|
| 1–3 | mutex-condvar-channel, thread-pool-work-stealing, worker-pool | 347, 275, 153 |
| 4–21 | mini-http-server-thread-pool (project guide; also all 10 capstone steps) | 634 |
| 5–9 | tcp-framed-server, http1-parser | 357, 324 |
| 10–11 | axum-router-state, tower-service-layer, std-panic-process-exit-catch | 84, 269, 423 |
| 12–13 | security-pitfalls-secure-coding, http-caching-conditional | 301, 252 |
| 15–16 | concurrent-counters-and-statistics, metrics-prometheus-histograms-load-testing | 341, 349 |
| 17–19 | std-net-tcp-udp-ip-addr, sans-io-protocol, deterministic-simulation-testing | 564, 339, 641 |
| 20–21 | graceful-shutdown, testing-patterns, deterministic-simulation | 68, 78, 2,107 |

Long pages are reviewed for clarity, not lengthened. Check whether any link points at a topic the series doesn't
need (for example `axum-router-state` for a server built without that library).

### Phase 3: the rest of Rust

(Order changed: see "Order for the next phases" below.)

Chapter by chapter in learning order. First sort each page: lesson, lookup, merge with another, or already good.

### Phase 4: Compilers, Systems, Algorithms, Distributed, Testing

Start with the broken compiler pages listed in "Known issues to fix in the later phases" below.

Same sorting. Order follows the next series and the current FlowForge stage.

### Phase 5: Data and projects

Mostly editing for readability: break up compressed notes, add project guides.

## Order for the next phases (approved by the owner, 2026-11-27)

**This replaces the order of phases 3 to 5 above:** parsing and compiler pages move ahead of the rest of the Rust
pages. The work still waits for the owner's go-ahead. Page sorting, writing rules and the builder plus reviewer loop
are unchanged. Coverage below comes from a keyword search and page word counts (not counting code). Not every matching
page was read, so check what exists before writing a new page, and reuse it.

**Order:** broken pages → parsing and diffs → safe collection → asset graphs and workflows → intelligence and measuring
results → everything else (the rest of Rust, databases, other subjects).

**Step 0, first.** Repair the broken diagrams and blocks (see "Known issues"). Check the example code for bugs,
starting with pages the series link to.

**1 · Parsing and diffs.**
- Expand: `dp-lcs-diff` (90 words), `dp-edit-distance` (69), `diagnostics-spans` (81), `edge-dsl-parser` (58).
- New or expanded: reading HTML safely; reading JavaScript with a parser library; source maps (generated code back to
  original files); comparing syntax trees, not only lines.
- Fix the broken compiler pages in this step.

**2 · Safe collection.**
- New or expanded: controlling what a program may fetch (web addresses, redirects, private addresses; reuse
  `std-net-tcp-udp-ip-addr`); polite collection (rate limits, retries, robots.txt); running outside tools safely.

**3 · Asset graphs and workflows.**
- Expand: `union-find` (72), `tarjan-scc` (118).
- New or expanded: matching the same thing from different sources; tracking where evidence came from.
- Workflow pages must not give away a FlowForge or series solution.

**4 · Intelligence and measuring results.**
- Expand: `minhash-simhash-lsh` (227).
- Measuring ranking quality: check the existing evaluation and vector pages first.

**5 · Everything else.**
- Also review the short pages on signed webhooks, tamper-evident records (`merkle-tree`) and queues between threads
  (`spsc-ring-buffer`); move one earlier if a series starts to need it.
- Database topics already have substantial pages: fix layout and readability first, and add content only for a gap.
- If a shell course is added, add terminal and job control material.

**The Cookbook is public.** Teach general ideas with new examples. Never put anything about the owner's private
projects in a page, a title, a link or a commit message.

## Known issues to fix in the later phases (found 2026-11-27, after phase 2)

Reported by the owner on `the-pipeline-and-where-tests-attach`: its diagram shows as stacked lines of text, with
labels glued to their descriptions ("Source&str"). The owner asked to fix this with the planned compiler work,
not now. Nothing below has been fixed.

**Cause.** Pages converted from the guides keep HTML blocks with `cb-g-*` classes. `web/src/styles` has no rule for
many of them. `tools/cookbook/convert.py` adds the `cb-g-` prefix; the guides' own styling did not come across.
Only the reported page was seen on screen. The counts come from a scan that matches class names in page files against
CSS rules, so screenshot each page before and after fixing it.

**Compiler pages with unstyled diagrams, lists or link blocks (phase 4, do these first): 8**
backends-hir-mir-llvm-and-cranelift, clox-tooling, fix-these-first-in-rlox, from-jlox-to-clox, recoverable-parsing-and-advanced-compiler-theory, the-pipeline-and-where-tests-attach, the-slice-loop, tooling-around-a-new-language

**Compiler pages where only table classes have no styling (check on screen; may only need table styles): 32**
abis-object-files-linking-and-unwinding, arithmetic, bidirectional-typing-and-dependent-types, blocks-and-scope, calls-and-functions, classes-and-inheritance, classes-methods-and-super, closures-and-upvalues, control-flow, dataflow-analysis-and-abstract-interpretation, designing-a-language, formal-verification-and-mechanized-semantics, functions-and-closures, globals-locals-and-statements, gpu-and-accelerator-compilation, hindley-milner-type-inference, how-rustc-swc-and-oxc-handle-the-same-problems, implementing-rust-s-hardest-features, instruction-selection-scheduling-and-emission, jumps-and-control-flow, loop-optimization-and-vectorization, metaprogramming-macros-and-staging, reading-rustc-and-contributing-to-it, resolver, scalar-optimizations, security-hardening-and-sanitizers, statements-and-globals, test-harness, types-strings-and-interning, values-and-comparison, vm-and-the-pratt-compiler, webassembly-as-a-compilation-target

**Compiler pages under 200 words, not counting code: 20**
tooling-in-code (13), project-layout (22), zero-copy-lexer (58), edge-dsl-parser (66), pratt-parser (70), parser-combinators (84), bytecode-vm (97), json-parser (107), csv-state-machine (112), the-slice-loop (116), the-pipeline-and-where-tests-attach (120), optimization (131), chunks-and-the-disassembler (136), clox-tooling (138), workspace-and-shared-tests (138), blocks-and-scope (151), vm-and-the-pratt-compiler (156), from-jlox-to-clox (184), arithmetic (198), jumps-and-control-flow (198)

**Other subjects with unstyled diagrams or blocks (fix in their own phase; shared styles may fix them early):**
- Appendices: glossary-compilers, how-to-use-this-guide, reading-map-from-front-end-to-back-end, university-courses, what-to-build-next
- Data structures and algorithms: dp-trees-and-specialized-algorithms, hash-tables
- Databases: arc-replacer, b-trees, buffer-pool-manager, buffer-pools, dsls-and-sql-parsing, log-structured-storage, pages-and-tuples, parallel-execution, relational-model-algebra, search-and-insert, sorting-and-aggregation, sql-front-end, storage-models-and-compression, the-dbms-stack
- Performance and systems: performance-data-layout-simd-threads-and-lock-free-structure
- Rust language: graph-algorithms-gc-and-life-without-gc, language-features-from-rust
- Testing and benchmarking: tooling

A further 58 pages outside compilers have only the unstyled table classes.

**Whole Cookbook (701 non-draft pages in this scan).**
- 123 pages have at least one HTML block with no matching style; 33 of them have more than plain tables.
  Drawn figures (inline SVG) on database pages are the likely worst case.
- Under 200 words, not counting code: 139 pages (62 under 100). 122 of the 139 have attached code, so some may be
  fine as short code pages. 121 are generated code pages, 17 are guides, 1 is a project.
- 167 pages start with a lowercase fragment as their opening line (a note, not a sentence).
- 264 pages have at least one section under 8 words (some may be code-only sections).
- Generated guide pages say "From the ... guide. Its exercises are left out." (`CookbookRead.tsx`): reader-facing filler
  to remove or reword.

**Done means, for these:** a check in `anneal validate` or the content tests that fails when a page uses a class with
no style, screenshots of every repaired page type at wide and phone widths, and the usual builder plus reviewer loop.

## Done means

- The reader can state the main idea, follow the example, say when to use it and when not, recognise common problems
  and see where it connects.
- Every claim, example and link was checked as above.
- Addresses unchanged; content check and browser tests pass.

## Running it

Parallel agents may draft separate chapters after phase 1 sets the standard. Each batch is reviewed against this
plan before release; drafts in bulk without review are how pages become thin or wrong.

## Open questions

- Should existing links from exercises to Cookbook pages be rechecked as pages change? (Recommended: yes.)
- Do the six draft samples meet this standard? They need the owner's review.
