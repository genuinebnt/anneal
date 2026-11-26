# Practice: crate cb-compiler (PEBBLE)

How to use this page: read `crates/cb-compiler/README.md` and `docs/ARCHITECTURE.md` (reading order), run `cargo run -q -p cb-compiler --bin pebblec -- run crates/cb-compiler/examples/showcase.peb`, then climb the ladder in order.
Every rung ends in a test you write FIRST: a UI golden (`tests/ui/*.peb` + `.stderr`), a MIR golden (`tests/mir/*.peb` + `.mir`), a run test (`tests/run/*.peb` + `.stdout`), or a differential check in `tests/backend.rs`. `BLESS=1 cargo test -q --release -p cb-compiler` regenerates goldens; read the diff before accepting it.
Work on a git branch. No solutions here, only where to look. The interpreter (`src/interp.rs`) is the oracle for everything the VM does.

## Prove you got it (before the ladder)
1. Run `pebblec dump-mir` at `--opt-level=0` and `--opt-level=3` on `examples/fib.peb`; which passes changed the output (`--dump-after=all`) and which invariant (trap-preserving rule in `docs/OPTIMISATIONS.md`) did they respect?
2. Change one `.peb` run test so that an overflow occurs; explain from the code why the interpreter, VM -O0 and VM -O3 must all trap with the same message.

## Rung 1: add an operator (small)
Add a binary operator that PEBBLE lacks, for example `**` (power) or the bitwise `&`, `|`, `^`, `<<`, `>>` on `int` (current `BinOp` has only Add..Rem, comparisons, And, Or).
Touch in order: `src/syntax/token.rs` (token and its text), `src/syntax/lexer.rs`, `src/syntax/parser.rs` (`infix` binding-power table), `src/syntax/ast.rs` (`BinOp` and its `symbol`), `src/sema/check.rs` (typing rule beside `Sub | Mul | Div | Rem`), `src/interp.rs` (semantics and traps), `src/mir/verify.rs` and `src/mir/mod.rs` (`may_trap_or_effect`), `src/opt/prop.rs` (folding must never fold a trapping case), `src/bc/mod.rs` (instruction macro) and `src/bc/codegen.rs`, `src/vm/mod.rs`.
Verify: a `tests/run/` program exercising edge values (overflow, negative shift counts); the differential tests in `tests/backend.rs` pass at -O0..-O3; `fmt` (crate cb-compiler-tools) round-trips the new syntax.
Question to answer: where does precedence get decided, and what breaks if the binding power is wrong by one?

## Rung 2: add a type (small-medium)
Add `char` or `float`. Start from `Ty` in `src/sema/types.rs`, literal lexing in `lexer.rs`, `src/sema/resolve.rs` (type names), `check.rs` (literals, operators on the new type, casts), `unify.rs` (nothing special should be needed; explain why), `interp.rs` values, `src/mir/` lowering and `verify.rs` (types of registers), `bc/` instruction selection (typed instruction selection is why the VM is fast).
Verify: UI golden for a type mismatch involving the new type; run tests with edge cases (NaN comparison semantics if float, Unicode scalar bounds if char); `tests/props.rs` type-soundness property still holds with the generator extended (`tests/common/progen.rs`).
Question: how do the pattern checker (`sema/pattern.rs`, `exhaustive.rs`) and literals of the new type interact?

## Rung 3: add a diagnostic with a UI test (small)
Pick a real mistake PEBBLE does not report, such as a comparison of a value with itself (`x == x`), an assignment to an unused `mut` (extra warning), or `if` with a constant condition. Add its code in `src/diag/codes.rs` (follow the `E0301` / `W0503` style), emit it from `src/sema/check.rs` with primary and secondary labels, a `help` and, if possible, a machine-applicable suggestion (`diag::apply_suggestions`).
Verify: add `tests/ui/<code>_<name>.peb`, run `BLESS=1 cargo test -q --release -p cb-compiler --test ui`, inspect the `.stderr`, and confirm `docs/ERRORS.md` regenerated (the test fails if it is stale). Add one UI case where your diagnostic must NOT fire (a false-positive guard).
Question: does the warning need a `CheckOptions` flag like `warn_shadow`, and who decides?

## Rung 4: add an optimisation pass with a golden MIR test (medium)
Write a new pass such as jump-threading on equal conditions, redundant-load elimination for `Len` of an unchanged array, or loop-invariant code motion. Register it in `src/opt/mod.rs` (`PASS_NAMES`, `run_pass`, `pipeline`), implement it in a new `src/opt/<name>.rs`, using the `cfg.rs` / `live.rs` helpers.
Verify: (a) a `tests/mir/<name>.peb` with `// passes: <name>` and its `.mir` golden showing before/after (`BLESS=1`); (b) a negative case where the pass must NOT fire because it would drop a trap; (c) `tests/backend.rs` runs each pass alone over all run tests and examples with the verifier after every pass; (d) the `bench` table (`pebblec bench crates/cb-compiler/examples/bench`) shows instruction counts, and you report before and after for at least two programs.
Question: what happens to a pass if it removes a computation that can overflow? Show a test that would catch it.

## Rung 5: add an LSP feature (medium-advanced; crate cb-compiler-tools)
Implement `textDocument/documentHighlight` or `workspace/symbol` (both listed as not implemented) or `selectionRange`. Modules: `crates/cb-compiler-tools/src/lsp/server.rs` (advertise capability and dispatch near the `request` match), `src/lsp/features.rs` (handler built on `Snapshot` and the `SemanticIndex` queries `references`, `symbols`), `src/lsp/snapshot.rs` (UTF-16 ranges).
Verify: a JSON-RPC transcript in `tests/lsp/*.jsonl` driven by `tests/lsp_golden.rs`, plus a property test in `tests/lsp.rs` (for example highlight ranges equal the references of the symbol on every identifier of every run program), and a UTF-16 case with an emoji before the symbol.
Question: why must the handler answer from the snapshot of the document version the request carries?

## Rung 6: stretch (advanced, optional)
Add closures (the architecture notes say there are none) or let-polymorphism; list every module that assumes their absence (`docs/ARCHITECTURE.md` "Design decisions"), write the failing tests first, and keep the interpreter-versus-VM differential tests green.

## Chapter projects
Other-chapter stems used: `hindley_milner_inference`, `ssa_register_allocation` (compilers_runtimes), `pratt_parser` (text_and_parsing), `wasm_binary_interpreter` (compilers_runtimes), `binary_format_parsing` (text_and_parsing).

### Small (about 2 h)
Combine this crate with `pratt_parser`. Add Rung 1, then explain the difference between the crate's parser and the recipe in 5 sentences.
Acceptance: run test and UI test pass; `pebblec fmt` equivalent passes.

### Intermediate (a weekend)
Do Rungs 3 and 4, and compare your pass with the SSA optimisations in `ssa_register_allocation`.
Acceptance: golden tests, `bench` before/after table, no new differential failures in `tests/backend.rs`.
Stretch: a pass that uses dominators.

### Advanced (a week)
Write a second back end that emits WebAssembly bytes for a subset, and run it through `wasm_binary_interpreter`.
Acceptance: 20 run tests produce identical stdout through VM and wasm interpreter; `binary_format_parsing` style tests reject truncated modules.
Stretch: a source-map style diagnostic from wasm trap to PEBBLE span.
