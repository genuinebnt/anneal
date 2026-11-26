# Cookbook page format

One folder per page. It moves into anneal later, so it follows anneal's System design content
(`learn-rust/crates/content/src/sd.rs`: TOML fields plus `SdMd` Markdown). Reference only: no quizzes, checks, exercises or progress.

```
pages/<slug>/
  page.toml
  code/            # a standalone cargo project, if the page has Rust code
    Cargo.toml     # must contain an empty [workspace] table
    src/ ... tests/ ...
```

Check with `python3 pages/check.py <slug> --links --strict` (`--strict` adds rustfmt, clippy with warnings denied, and debug tests; no slug checks every page). A page is not finished until it passes with both flags.

## page.toml

| Field | Required | Meaning |
|---|---|---|
| `title` | yes | name of the page |
| `module`, `chapter` | yes | as in `learn-rust/docs/cookbook/OUTLINE.md` |
| `order` | yes | position in the chapter |
| `kind` | yes | `concept`, `api`, `pattern`, `comparison`, `build` or `testing` |
| `level` | yes | `simple`, `intermediate` or `advanced` |
| `status` | yes | `draft` until the owner reviews it (`verified` and `reviewed` are set later, not by the writer) |
| `lede` | yes | one or two sentences: what it is |
| `summary` | yes | "In 60 seconds": 3 to 7 sentences, each one point |
| `layers`, `layers_on` | no | "Where it sits": ordered names, and those this page is about |
| `[[glance]]` `label`, `value` | no | short facts (defined in, stable since, allocates, crates) |
| `[[sections]]` `heading`, `body` | yes | `body` is SdMd Markdown |
| `[[terms]]` `term`, `means`, `section` | no | one short sentence; `section` is an existing heading |
| `related` | no | slugs; a slug that does not exist yet is reported as a warning |
| `[[sources]]` `title`, `url`, `kind`, `note`, `read` | yes (at least 3) | see below |
| `[[labs]]` `title`, `after`, `files`, `run`, `test` | no | a runnable, read-only viewer placed after the section `after` |
| `illustrative` | no | titles of Rust fences that are deliberately not compiled (for example code that must NOT compile) |
| `[verified]` `toolchain`, `date`, `platform`, `not_checked` | yes | what was run, and what was not |
| `credit` | no | attribution for adapted text |
| `[[methods]]` | no (expected on API pages) | see Methods below |

**Sources.** `kind` is one of anneal's `paper`, `docs`, `guide`, `book`, `standard`, `article`, or the **new** `video`, `code`,
`blog`, `series`, `slides`. `note` says why to open it. `read = true` only if you actually read the part the page relies on (`scope`, optional, says which part);
a claim marked as sourced must rest on a `read = true` source. A link that only loads is `read = false`.

**Labs.** `files` are paths inside `code/` in tab order. `run = true` runs `cargo run`, `test = true` runs `cargo test`.

## Methods (API pages)

`[[methods]]` `type`, `name`, `signature` (must contain `fn name`), `since`, `use_when`, and optionally `common = true`,
`unstable = true`, `anchor` (a section heading or `###` id). The page renders **common methods first and the full list
collapsed**, and search matches method names to their anchors. Do not also hand-write signature tables in the body.
The method list renders **directly after the first section**, so it is the quick look-up. For an unstable method set
`unstable = true` and `since = "unstable (feature_name)"`. Write a bound that std puts on the impl block into the signature
as a `where` clause. The checker does **not** yet compare signatures, `since` values or completeness with the std source;
`pages/tools/sigs.py` is a starting point (its impl patterns are specific to the two sample pages).

## Decisions (made 2026-11-26)

- List **every** method of the type the page is about, as `[[methods]]` data. Mark the everyday ones `common = true`.
- **Internals** ("how it works", "how it is built") go in `:::deeper` blocks, collapsed.

## Body Markdown (anneal's grammar)

- Fences: ` ```rust title=src/lib.rs hl=3,5-7 group=name `. Languages: rust, rs, python, py, sql, http, json, redis, sh, bash, text.
- **Every `rust` fence must be a contiguous excerpt of a file in `code/`** (indentation may be removed), unless its `title` is
  listed in `illustrative`. Give illustrative fences a title and say in the prose why they are not compiled.
- **Code that must not compile:** ` ```rust compile_fail=E0502 title=name.rs `. The checker compiles it alone (std only) and
  requires that error code. Use `illustrative` only when a snippet needs a dependency or cannot be compiled for another reason,
  and say why; every unchecked snippet is reported as a warning. (`compile_fail` is **new** to anneal.)
- Linkable subsections: `### Title {#id}` (**new** to anneal), unique on the page.
- A title that equals a file path in `code/` must be an excerpt of **that** file. `hl=` lines must exist in the snippet.
- `illustrative` must come **before the first `[[...]]` table**, or TOML puts it inside that table.
- Write a `|` inside a table cell as `\|`.
- Callouts at the start of a quote: `> [!NOTE]`, `> [!TIP]`, `> [!WARNING]`, `> [!ASIDE]`, and the **new** `> [!RUST]`,
  `> [!TESTING]`, `> [!PRODUCTION]`. `INTERVIEW` is not used.
- `:::deeper Title | meta` ... `:::` (collapsed), `:::steps Title` with `- who | what | narration` lines (at least two). No nesting.
- Tables, `$$` maths, `\( … \)` inline maths, margin notes `^[…]`.
- No `#` headings in a body (a section's `heading` is the heading); `###` is allowed.

## Writing rules

- Answer first. Say when to use it, when not to, and the main cost, before the detail.
- Use what the topic needs, never pad: explanation, comparison table, code with tests, callouts (pitfall, in Rust, testing, in
  production), tips and tricks, a step-through, terms, related pages, sources.
- Plain English, short sentences, no filler.
- Label the strength of claims in the text: "tested here", a linked source, or advice. Never state a performance number you did
  not measure; if you measured it, give machine, inputs, toolchain and date.
- Check library and language claims against the local docs or source (`rustc --print sysroot`, `~/.cargo/registry/src`).
- **Planted bugs need a build folder each.** Copies of one crate that share a target directory can silently reuse another
  copy's binary and report a false result (seen with Miri on 2026-11-26).
- Tests must be able to fail: for the key claims, break the code in a scratch copy and see the test fail. Say what you tried in
  `[verified].not_checked` or in your report.
- Do not give away FlowForge stage exercises (topological sort, cycle detection, critical path, task executor).
  BusTub and Lox solutions are allowed (owner's decision).
