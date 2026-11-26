# Cookbook

A reference to look things up in: how something works, the code for it, and what it costs. It lives at `/cookbook`.

## Rules the owner set

- **Reference only.** No exercises, practice, progress or follow-ups, and **nothing about the reader is stored**: no notes,
  pins, problem reports, reading history or "pick up where you left off". Only view settings (side panels, wrapping, code
  size) are kept, like other anneal preferences. No database tables.
- **Tabs are subjects**, never page names. A subject gets a tab only when it has enough written pages.
- Pages read like the owner's two guides: explanations, tables, tests, callouts, tips, with links to further reading.
- Code is wide, read-only and highlighted. It is not editable.
- Guide material is public, including the BusTub and Lox solutions and the Lox project review.
- Writing from books and websites: read it first, use it for topics, write our own words and code, link and credit it.

## What is there

707 pages in eight subjects: Rust, Algorithms, Systems, Data, Distributed & web, Compilers, Testing, Projects.

| Part | Where |
|---|---|
| 210 pages from the two guides, 481 pages for code examples, 10 for multi-file projects | `content/cookbook/<subject>/*.toml`, **generated**: do not edit by hand |
| The code itself (the examples and projects), its own Cargo workspace | `cookbook/` (`./check.sh` builds and runs it) |
| Six draft sample pages in the full format | `content/cookbook/samples/<id>/` (`page.toml`, `code/`, `recorded.json`) |
| Loader and its checks | `crates/content/src/cookbook.rs` |
| API (`/api/cookbook`, `/api/cookbook/pages/{id}`) | `crates/api/src/cookbook.rs` |
| Web pages: front page, subject, reading view, code viewer | `web/src/pages/cookbook/`, `web/src/styles/cookbook.css` |
| Page renderer, shared with System design | `web/src/components/SdMd.tsx` |
| Browser tests (every page is opened) | `web/e2e/cookbook.spec.ts` |

A page about code names its files (`code = [...]`, relative to `cookbook/`); the code is not copied into the page. The
Docker image copies `cookbook/` for this.

The layouts follow the owner's blog (genuine-folio: home, wiki shelf and series), which shares anneal's palette. On a code
page the code comes first, in a read-only viewer: file tree, symbols, text size, wrapping, full window; the file's opening
`//!` notes are folded there because the page shows them as text. `[` and `]` hide the side panels, `J` and `K` move between
parts, `/` moves to the search box, `⌘K` finds any page. A draft page with six or more sections opens two and folds the rest.

## Changing pages

The two guides are HTML files that are not in this repository. After editing a guide or the code in `cookbook/`:

```sh
python3 tools/cookbook/convert.py ~/dbms.html ~/compiler.html
cargo run -q -p anneal-cli -- validate
```

`tools/cookbook/outline.json` says which guide section, example file and project goes in which subject, module and chapter.
**A new section or file appears only once it is listed there.** The guide pages leave out exercises and "Done when" lists,
turn reading links into sources, and repair the guide's malformed code blocks and unescaped generics. A code page's text is
the file's header comment (`tools/cookbook/code_pages.py`), laid out by `tools/cookbook/notes.py`: bullets that open with
a term in capitals become term-and-explanation rows, capitals used for emphasis become bold (abbreviations stay, told apart
with `words.txt`), long paragraphs are split between sentences, and lists of uses and reading become lists. The words are
kept; if a page reads wrong, fix the comment or that script, not the generated file. A page's address comes from its guide title or file name, so
renaming either changes the address.

## Adding a subject

Add it to `SUBJECTS` in `crates/content/src/cookbook.rs`, give it a colour in `web/src/pages/cookbook/shared.tsx`, and add
its header tab in `web/src/components/Header.tsx`.

## Page format

A page is TOML: `title`, `module`, `chapter`, `chapter_order`, `order`, `kind`, `lede`, `[[sections]]` (`heading`,
`body`) and `[[sources]]`. Full-format pages may add `status`, `summary`, `glance`, `terms`, `methods` and `labs`.

A body is the Markdown System design uses (fences with `title="…"` and `hl=`, `$$` math, `:::steps`, `:::deeper`, margin
notes), plus:

- callouts `> [!NOTE]`, `TIP`, `WARNING`, `ASIDE`, `RUST`, `TESTING`, `PRODUCTION`;
- `<aside data-call="…">` for a panel whose text is already HTML (also `performance`, `why`, `idea`);
- `compile_fail=E0502` on a fence, shown as "does not compile";
- `### Title {#anchor}` to name a heading.

## What has and has not been checked

- Every page loads, shows its code if it has any, renders without leftover syntax and does not scroll sideways.
- `cookbook/check.sh` last ran in the FlowForge copy this was copied from: 474 passed, 0 failed (2026-11-26). It is not part
  of anneal's CI.
- The guide pages' code is **not compiled here**; the owner states the guides' snippets were compiled and tested.
- Text made from file header comments, and descriptions in lists, have not been reviewed page by page.
- The six samples are **drafts the owner has not reviewed**.
- A sample shows output recorded when it was written. **Nothing can be run from the cookbook.**

## Next

1. Run and test buttons (not built).
2. **Testing is thin:** most of its planned guides are not written ([cookbook/OUTLINE.md](cookbook/OUTLINE.md), module 4).
3. The owner reviews the six samples.
4. Ideas from books (*Rust in Action*, *Rust for Rustaceans*, *Rust Atomics and Locks*, *Zero To Production*) and two
   separate sections drawn from 40tude.fr/rust and the nextechtide interview guide. Research: FlowForge
   `cookbook/handoff/ideas/`. 40tude shows a copyright notice and no article licence was found: link, never copy.
5. The sample author reported bugs in `cookbook/examples/capstones/mini_kv_store.rs` that nobody has reproduced yet.
6. Not built: meaning search, compare and error look-up pages.

Earlier design notes: [cookbook/PLAN_HISTORY.md](cookbook/PLAN_HISTORY.md). The proposed full outline:
[cookbook/OUTLINE.md](cookbook/OUTLINE.md). Mockup used during design: [mockups/cookbook.html](mockups/cookbook.html).
