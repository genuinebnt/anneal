# cb-compiler-tools: formatter, highlighter and language server for PEBBLE

Stage 3 of the PEBBLE compiler project (stage 1: `cb-compiler`, the front end). Library `pebble_tools`, binary `pebble-tools`. Dependencies: `cb-compiler` and `serde_json` only.

```text
cd cookbook
cargo run -q -p cb-compiler-tools -- fmt FILE [--check] [--best-effort]
cargo run -q -p cb-compiler-tools -- highlight FILE [--format=ansi|html|json] [--semantic]
cargo run -q -p cb-compiler-tools -- lsp [--sync=full|incremental]      # JSON-RPC on stdin/stdout
cargo run -q -p cb-compiler-tools -- gen-editors                        # regenerate editors/
cargo test -q --release -p cb-compiler-tools                            # BLESS=1 refreshes goldens and editor files
```

Exit codes: 0 success; 1 input problem (`fmt` of a file with syntax errors, `fmt --check` on unformatted text); 2 usage or I/O error. `lsp` exits 0 after `shutdown` + `exit`, 1 otherwise.

## Layout

| Path | What |
|------|------|
| `src/fmt/` | formatter: `doc.rs` (Wadler document algebra + printer), `build.rs` (AST to doc, comment re-attachment), `mod.rs` (API, safety net) |
| `src/highlight.rs` | lexical and semantic highlighter; ANSI, HTML, JSON renderers |
| `src/textmate.rs` | generates the TextMate grammar and VS Code extension files |
| `src/lsp/` | `transport` (framing), `server` (lifecycle, sync, dispatch), `snapshot` (analysis + UTF-16), `features`, `completion`, `semtok`, `nav` (highlight, selection, call hierarchy), `workspace` + `fuzzy` (workspace symbols) |
| `src/analysis.rs`, `src/eol.rs` | the only module that calls the compiler; line-ending policy |
| `scripts/` | `lsp_smoke.py`, `nvim_smoke.lua`, `helix_smoke.py` |
| `editors/` | generated `pebble.tmLanguage.json` and the `vscode/` extension scaffold (no `node_modules` needed to read it) |
| `tests/` | `fmt.rs`, `fmt_range.rs`, `highlight.rs`, `editors.rs` (+ `vscode/mock.js`; needs `node` for the mock run, skips it with a message otherwise), `lsp.rs`, `lsp_workspace.rs`, `lsp_stdio.rs`, `rename_safety.rs`, `lsp_golden.rs` + `lsp/*.jsonl` |
| `docs/` | `LSP.md`, `FORMATTER.md`, `HIGHLIGHTING.md` |

## What is tested

* Formatter: idempotence, spans-erased AST equality, identical interpreter output, comment multiset and order, over every `.peb` in `cb-compiler` plus up to 8000 generated programs; 5000 mutated broken inputs never panic.
* Highlighter: tokens tile the source exactly (also on broken input); HTML is balanced and escaped; the TextMate keyword list is derived from the lexer and checked against it exhaustively for short words.
* Server: lifecycle and error codes, cancellation, coalescing of edit bursts, incremental versus full sync over 500 random edits, rename on every kind of definition across all run programs (still type checks, same output), completion at random offsets, hover types versus the HIR, 5000 random JSON-RPC messages, a 5000-line file (timings printed with `--nocapture`).

## Further reading

* LSP specification: <https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/>
* Wadler, "A prettier printer": <https://homepages.inf.ed.ac.uk/wadler/papers/prettier/prettier.pdf>
* rust-analyzer architecture notes: <https://rust-analyzer.github.io/book/contributing/architecture.html>
