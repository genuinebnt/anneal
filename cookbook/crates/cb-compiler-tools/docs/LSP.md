# The PEBBLE language server

`pebble-tools lsp` speaks JSON-RPC 2.0 over stdio with `Content-Length` framing, implemented on std + `serde_json` (`src/lsp/`). `lsp::Server` is a pure message processor (`process(Vec<Value>) -> Vec<Value>`) so it can be driven in-process; `lsp::serve` adds the stdio loop.

## Supported features

| Feature | Method | Notes |
|---------|--------|-------|
| lifecycle | `initialize`, `initialized`, `shutdown`, `exit` | requests before `initialize`: -32002; after `shutdown`: -32600; `exit` code 0 after `shutdown`, else 1 |
| errors | any | unknown method -32601, malformed JSON -32700 (id null), bad shape -32600, bad params -32602, cancelled -32800, handler panic -32603 |
| cancellation | `$/cancelRequest` | applies to requests queued in the same batch |
| document sync | `textDocument/didOpen`, `didChange`, `didClose`, `didSave` | FULL and INCREMENTAL (range edits in UTF-16 units, several per notification); `--sync=` picks what is advertised, both are always accepted; stale versions are ignored |
| diagnostics | `textDocument/publishDiagnostics` | UTF-16 ranges, severity, code, `source: pebble`, related information for secondary labels, notes/help appended, Unnecessary tag for unused/unreachable |
| hover | `textDocument/hover` | declaration and doc comment of the definition, `here:` instantiated type of generic calls, expression type, built-in docs |
| navigation | `definition`, `references` | via a sorted name table; `context.includeDeclaration` is honoured |
| rename | `prepareRename`, `rename` | edits every reference; understands shorthand `Point { x }` patterns; refuses keywords, bad identifiers, tuple-field names and clashes (see "Rename safety"); `prepareRename` with an extra `newName` validates the name too |
| workspace symbols | `workspace/symbol` | fuzzy ranking (exact, prefix, camelCase/underscore humps, subsequence; `src/lsp/fuzzy.rs`) over open documents and the `*.peb` files of the workspace folders, at most 200 results |
| workspace folders | `didChangeWorkspaceFolders`, `didChangeWatchedFiles` | capped scan (2000 files, depth 12, 2 MiB per file; hidden directories, `target`, `node_modules` skipped); file watching is registered dynamically when the client supports it |
| highlight, selection | `documentHighlight`, `selectionRange` | read/write kinds; strictly nested ranges |
| call hierarchy, type definition | `prepareCallHierarchy`, `callHierarchy/*`, `typeDefinition` | one document only |
| outline | `documentSymbol` | hierarchical: struct fields, enum variants and their fields |
| completion | `completion`, `completionItem/resolve` | scope-aware locals/params/items/built-ins; fields and built-in methods after `.`; variants after `Enum::`; keyword snippets; resolve adds signature and doc comment; triggers `.` `:` |
| signature help | `signatureHelp` | user functions and built-ins, active parameter from commas at depth 0; triggers `(` `,` |
| semantic tokens | `semanticTokens/full`, `full/delta`, `range` | the last 8 result ids per document are kept; an unknown previous id gets a full result; legend: function, variable, parameter, type, enumMember, property, keyword, string, number, operator, comment; modifiers declaration, readonly, mutable, defaultLibrary, deprecated, unused; delta encoded, split per line |
| formatting | `formatting`, `rangeFormatting`, `onTypeFormatting` (`}` `;`) | a clean file gets one minimal edit; a file with syntax errors is formatted best effort, item by item; range and on-type format only the items touched (see `FORMATTER.md`) |
| code actions | `codeAction` | quick fixes from compiler suggestions ("did you mean", parenthesise); `isPreferred` for machine-applicable ones |
| inlay hints | `inlayHint` | inferred `let` types, parameter names at call sites |
| folding | `foldingRange` | blocks, struct/enum bodies, `match`, comment runs |
| configuration | `workspace/didChangeConfiguration` | accepted with any payload and stored |

Not implemented: multi-file semantic analysis (one file is one program: definition, references, rename and call hierarchy do not cross files), pull diagnostics, `$/progress`.

Line endings: `\n`, `\r\n` and a lone `\r` all end a line, as the LSP specification requires; formatting keeps the file's dominant terminator (`src/eol.rs`).

## Rename safety

Cheap checks name the obvious clash (another item, field or variant of the same owner, a built-in function, a local or parameter in scope). The guarantee comes from a post-condition: the renamed text is analysed again and the rename is refused if it adds an error, changes the number of definitions or resolved uses, or makes any use resolve to a different definition. `tests/rename_safety.rs` renames random definitions of the run corpus to names the same program already uses and runs the program before and after every accepted rename.

## Design notes

* **Document store.** `HashMap<uri, Document { text, version, generation, cache }>`. Edits apply to the text, bump the generation and drop the cached snapshot; the cache key is (version, generation), so a client that reuses a version number cannot be served a stale analysis.
* **One adapter.** Every call into the compiler goes through `src/analysis.rs`.
* **Analysis cache keyed by version.** A `Snapshot` (text, `LineIndex`, `pebble::Analysis`, sorted `NameTable`) is built lazily by the first request or diagnostics flush after an edit and reused by every later request until the next edit (`Server::analyses` counts builds; tests assert one build per burst).
* **Debounce / latest version wins.** Edits only mark a document dirty. `Server::process` handles a whole batch and publishes diagnostics once per dirty document at the end, for its newest text. `serve` forms batches from whatever the reader thread has queued and, when the batch holds only edits, waits up to 15 ms for more (the 15 ms window is a constant in `main.rs`). So 100 keystrokes in one batch cost one analysis and one `publishDiagnostics`.
* **UTF-16.** LSP columns count UTF-16 code units while the compiler uses byte offsets. Every conversion goes through `LineIndex::{offset_from_utf16, line_col_utf16}` in `snapshot.rs`; a column inside a surrogate pair rounds down, a column past the line end clamps to it.
* **Broken code.** Everything runs on the recoverable parser's tree. Completion after `.` falls back to the declared type of a variable receiver when the checker skipped the half-typed field access.
* **Robustness.** Each message runs under `catch_unwind`; `Server::internal_errors` stays 0 in the 5000-message fuzz test. Requests answer from the same thread, no locks.
* **Stack.** The checker recurses deeply on nested programs; `main` runs everything on a 256 MiB thread and tests use the same.

## Connecting an editor

Build once: `cargo build --release -p cb-compiler-tools`. Use the absolute path of the `pebble-tools` binary below, or put it on `PATH`.

### What was really exercised (2026-11-26, macOS)

| Client | Exercised? | How |
|--------|------------|-----|
| raw stdio | yes | `tests/lsp_stdio.rs` (part of `cargo test`): the built binary as a child process; split and coalesced frames, huge payloads, malformed headers, exit codes, timeouts |
| Neovim 0.12.0-dev | yes, headless | `scripts/nvim_smoke.lua`, Neovim's built-in LSP client |
| Helix 25.07.1 | yes, on a pseudo-terminal | `scripts/helix_smoke.py`: `hx --health pebble`, open, `:format`, `:write`, `:quit!`. Hover, completion and rename were NOT driven in Helix |
| VS Code | **no** (not installed) | `tests/editors.rs` only: static checks of `package.json`, referenced files and grammar scopes; `node --check`; `extension.js` run under node 22 against a hand-written mock of `vscode` and `vscode-languageclient` (the mock client spawns the real server and gets an `initialize` reply). The real `vscode-languageclient` package was never installed or run |
| Vim, Emacs, others | no | |

Neovim output:

```text
$ PEBBLE_TOOLS=$BIN nvim --headless -u NONE -i NONE -l scripts/nvim_smoke.lua
ok   initialize: pebble-tools 0.1.0
ok   diagnostics: line 4 E0301 mismatched types
ok   hover: ```pebble | fn add(a: int, b: int) -> int | ``` |  | --- |  | Adds two numbers.
ok   definition: line 2
ok   workspace/symbol: add
ok   rename: both occurrences rewritten by the client
ok   formatting: 1 edit(s) applied by the client
ok   shutdown: client stopped
all steps passed
```

(Neovim appends to its own `lsp.log`; set `XDG_STATE_HOME` to a temporary directory to avoid that.)

Helix output (`python3 scripts/helix_smoke.py $BIN`, after the `hx --health` report, which lists the server with a check mark and no tree-sitter grammar or queries, so there is no highlighting from Helix itself):

```text
ok   hx quit by itself with status 0
ok   initialize answered
ok   diagnostics published with E0301
ok   formatting requested
ok   formatting edit applied and saved
ok   shutdown and exit sent
all steps passed
```

All three scripts were also run against `/usr/bin/false` and reported failure.

To check your own setup without any editor: `python3 scripts/lsp_smoke.py /path/to/pebble-tools` (standard library only).

**VS Code** (untested in VS Code itself): open `crates/cb-compiler-tools/editors/vscode` as an extension folder, run `npm install` (fetches `vscode-languageclient`), then "Run Extension". The setting `pebble.server.path` points at the binary.

**Neovim** (0.10+; `scripts/nvim_smoke.lua` calls `vim.lsp.start` directly, the autocommand below was not run):

```lua
vim.filetype.add({ extension = { peb = 'pebble' } })
vim.api.nvim_create_autocmd('FileType', {
  pattern = 'pebble',
  callback = function(args)
    vim.lsp.start({ name = 'pebble', cmd = { 'pebble-tools', 'lsp' },
                    root_dir = vim.fs.dirname(args.file) })
  end,
})
```

**Helix** (`languages.toml`; `auto-format` was not part of the tested configuration):

```toml
[language-server.pebble]
command = "pebble-tools"
args = ["lsp"]

[[language]]
name = "pebble"
scope = "source.pebble"
file-types = ["peb"]
comment-tokens = ["//", "///"]
block-comment-tokens = { start = "/*", end = "*/" }
indent = { tab-width = 4, unit = "    " }
language-servers = ["pebble"]
auto-format = true
```

## Tests

`tests/lsp_golden.rs` replays `tests/lsp/*.jsonl` transcripts (`{"->": request}` then the expected `{"<-": response}` lines, ids normalised; `BLESS=1` rewrites them; review the diff). `tests/lsp.rs` holds the protocol property tests, `tests/lsp_workspace.rs` the multi-document and navigation tests (fuzzy-match oracles, delta-apply equals full recompute over 500 random edits), `tests/rename_safety.rs`, `tests/lsp_stdio.rs` and `tests/editors.rs` as described above.
