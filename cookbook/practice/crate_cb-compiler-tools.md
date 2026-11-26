# Practice: crate cb-compiler-tools (formatter, highlighter, LSP)

How to use this page: read `crates/cb-compiler-tools/README.md` and `docs/LSP.md`, `docs/FORMATTER.md`, `docs/HIGHLIGHTING.md`, then run `cargo run -q -p cb-compiler-tools -- fmt FILE --check` on a few `.peb` files.
Each task ends with a test. Formatter tests are properties (idempotence, same AST, same output); LSP tests are transcripts in `tests/lsp/*.jsonl` plus property tests in `tests/lsp.rs`. `BLESS=1` refreshes goldens.
No solutions here. S about 1 h, M about half a day, A about 2 days. Do not attempt FlowForge stages here; attempt your stage first.

## Prove you got it
1. Why is the formatter built on a document algebra (`src/fmt/doc.rs`) instead of string building, and what does the "safety net" in `src/fmt/mod.rs` check before returning output?
2. Where do UTF-8 byte offsets become UTF-16 columns, and what happens for a column that lands inside a surrogate pair?

## formatter (`src/fmt/doc.rs`, `build.rs`, `mod.rs`)
- [S] Change the line width and observe which constructs break first; write a test that a program with a 79-column expression is untouched at width 80 and broken at 78.
- [M] Make trailing commas in multi-line struct literals mandatory and assert idempotence over all `.peb` files in `cb-compiler` (the existing property test harness shows how).
- [A] Add range formatting that preserves everything outside the range byte-for-byte, and prove it with a property: formatting a random range then the whole file equals formatting the whole file.
**Use it in:** rustfmt, prettier. Real code: [rust-lang/rustfmt](https://github.com/rust-lang/rustfmt), [astral-sh/ruff](https://github.com/astral-sh/ruff) formatter.
**Pitfall:** moving a comment across a token changes meaning in the AST-equality check; reattach comments by span.

## highlighter (`src/highlight.rs`, `src/textmate.rs`)
- [S] Add a new token class (for example constants in UPPER_CASE) and test that tokens tile the source exactly.
- [M] Add semantic highlighting for enum variants versus types and verify against `SemanticIndex` for every run program.
- [A] Generate a tree-sitter grammar skeleton from the lexer and compare token boundaries with your highlighter on 50 files.
**Use it in:** editors and docs. Real code: [tree-sitter/tree-sitter](https://github.com/tree-sitter/tree-sitter).
**Pitfall:** regex-based grammars disagree with the real lexer on corner cases (nested comments).

## language server (`src/lsp/server.rs`, `features.rs`, `completion.rs`, `snapshot.rs`, `semtok.rs`, `transport.rs`)
- [S] Add `textDocument/documentHighlight` (listed as unimplemented): same-symbol read and write ranges. Test via a `.jsonl` transcript.
- [M] Add `workspace/symbol` over open documents and a test that closed documents disappear; assert one analysis build per edit burst using `Server::analyses`.
- [A] Add multi-file projects (the docs list them as unsupported): resolve `import`-like references across documents, with a design note on cache invalidation, and a fuzz test of 500 random edits across 3 files comparing incremental and from-scratch results.
**Use it in:** every editor integration. Real code: [rust-lang/rust-analyzer](https://github.com/rust-lang/rust-analyzer) (see its architecture notes linked in the crate README).
**Pitfall:** answering a request for version N from a snapshot of version N+1 returns ranges that do not exist.

## transport and robustness (`src/lsp/transport.rs`)
- [S] Feed `Content-Length` headers with garbage and assert a JSON-RPC error and no panic.
- [M] Add `$/cancelRequest` for an in-flight (not only queued) request and test it.
- [A] Drive the server from a real editor (VS Code with the generated `editors/vscode`) and record a transcript of 20 actions as a new golden.

## Chapter projects
Other-chapter stems used: `zero_copy_lexer`, `json_parser`, `ndjson_serde` (text_and_parsing), `axum_router_state` (web), `trigram_index` (search_vectors).

### Small (about 2 h): CLI polish
Combine `fmt --check` and the highlighter with a pre-commit script; Acceptance: the script fails on an unformatted file and passes after `fmt`; stretch: HTML output with line numbers.

### Intermediate (a weekend): LSP over HTTP bridge
Combine the server's `process(Vec<Value>) -> Vec<Value>` with `axum_router_state` to expose a JSON endpoint; Acceptance: a transcript from `tests/lsp/*.jsonl` produces identical output through HTTP; stretch: a tiny web playground with diagnostics.

### Advanced (a week): workspace symbol search
Add a trigram index (`trigram_index`) over all open documents' symbols, serve `workspace/symbol`, and measure with a 5000-line file (the existing test prints timings). Acceptance: results equal a linear scan on 100 random queries; incremental updates after each edit are faster than a rebuild; stretch: fuzzy matching.
