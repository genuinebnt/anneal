# Architecture and API contract (stages 1 and 2)

## Reading order

1. `src/source.rs` - spans, `SourceFile`, `LineIndex` (UTF-8 / char / UTF-16 columns).
2. `src/diag/mod.rs`, `codes.rs`, `render.rs` - the diagnostic model, the code catalogue, the renderer and JSON.
3. `src/syntax/token.rs`, `lexer.rs` - never-failing lexer, comment side table.
4. `src/syntax/ast.rs` - the tree and the `Node` walker; then `parser.rs` (recursive descent + Pratt, recovery).
5. `src/sema/types.rs`, `unify.rs` - types and unification.
6. `src/sema/resolve.rs` - declaration collection and type lowering.
7. `src/sema/check.rs`, `pattern.rs` - body checking, locals, inference, typed HIR (`hir.rs`).
8. `src/sema/exhaustive.rs` - Maranget usefulness.
9. `src/sema/index.rs` - editor queries.
10. `src/interp.rs` - the reference interpreter (the oracle); `src/main.rs` - the driver.

Stage 2 reading order (back end):

11. `docs/MIR.md`, then `src/mir/mod.rs` (data types), `src/mir/lower.rs` (HIR to CFG, monomorphisation), `src/mir/verify.rs`.
12. `docs/OPTIMISATIONS.md`, then `src/opt/mod.rs` (pass manager), `cfg.rs`, `prop.rs`, `live.rs`, `dce.rs`, `cse.rs`, `strength.rs`, `inline.rs`, `loops.rs` (tail calls, bounds checks).
13. `docs/BYTECODE.md`, then `src/bc/mod.rs` (instruction set macro), `codegen.rs` (register allocation, selection), `text.rs`, `verify.rs`.
14. `src/vm/mod.rs` (the interpreter loop), `src/driver.rs` (pipeline + benchmarks), `tests/backend.rs`.

## Pipeline

```text
 text --lex--> tokens + comments + diags
      --parse--> ast::Module (Error nodes allowed) + diags
      --collect/lower--> DefIds, signatures, struct/enum layouts      (sema/resolve)
      --check bodies--> typed HirExpr per fn/const, locals resolved   (sema/check, pattern)
      --zonk--> no inference vars; E0310 if any remain
      --exhaustive--> E0401/E0402/W0504 on the zonked tree
      --> CheckResult { hir: Program, diagnostics, index: SemanticIndex }
      --interp::run--> output / RuntimeError                          (oracle for the VM)
 diagnostics --diag::render_all / to_json--> text / JSON
```

## Stable public API

* `pebble::source::{Span, SourceFile, LineIndex, LineCol}`. `Span` is a byte range (`u32`). `LineIndex::{line_col_utf8, line_col_chars, line_col_utf16, offset_from_utf16}` take the text explicitly; `SourceFile` wraps them.
* `pebble::syntax::{lex(&str) -> (Vec<Token>, Vec<Comment>, Vec<Diagnostic>), parse(&str) -> ParseResult { ast, comments, diagnostics, fuel_exhausted }}`. Every AST node has `span` and a unique `NodeId` (ids are `0..module.node_count`). `ast::Node` gives a uniform read-only walk (`children()`, `innermost_at(offset)`).
* `pebble::sema::{check(&Module), check_with(&Module, &CheckOptions)} -> CheckResult { hir: Program, diagnostics, index: SemanticIndex }`. `Program` holds `defs`, `structs`, `enums`, `fns: Vec<HirFn>`, `consts`, `main`. `HirExpr { id, span, ty, kind }`; every path is a `DefId`; method sugar is lowered to `Builtin`. `SemanticIndex`: `node_at`, `type_of`, `type_at`, `def_at`, `definition_at`, `references`, `symbols`, `completions_at`, `fields_of`.
* `pebble::diag::{Diagnostic, render, render_all, to_json, to_json_array, apply_suggestions, closest_match, codes::CODES}`.
* `pebble::interp::{run, run_with, RunOptions, RuntimeError, RuntimeErrorKind}`.
* `pebble::analyze(src, &CheckOptions) -> Analysis` runs parse and check and merges diagnostics.

Guarantees: nothing panics on any input; a `Program` is returned even with errors; with zero error diagnostics the HIR contains no `Ty::Var`, `Ty::Error` or `HirExprKind::Error`; local `DefId`s of one function/const are contiguous (`locals_start..locals_start + locals_len`, parameters first), which lets a VM index frames directly.

## Design decisions and trade-offs

* **Error nodes everywhere, one diagnostic per mistake.** The parser drops errors at the same token and right after lexer errors; the checker uses `Ty::Error` which unifies with anything. Cost: a few real follow-on errors are hidden until the first is fixed.
* **Local names are resolved while checking**, sharing one scope stack with inference. This avoids a separate resolved-AST pass; the `SemanticIndex` still records every reference.
* **No let-polymorphism, no closures**: generics live on items only, so inference is plain unification per body and monomorphisation is a simple substitution later.
* **`!` is coerced, not unified**: `demand` accepts it, `join` skips it. Simple and enough for `return`/`break`.
* **Pattern analysis after zonking**, because constructors of a column need the final scrutinee type.
* **Value vs reference semantics**: arrays alias (cheap VM heap object), aggregates copy-on-write. `mut` is permission on the *binding root*, so aliasing never bypasses it for the owner.
* **Overflow traps** instead of wrapping: bugs surface, and a VM can use checked ops.
* **Interpreter on a big-stack thread** with buffered output, so deep recursion is a clean error. HIR is `Send + Sync` (no `Rc`) for this reason.
* **Zero dependencies**; JSON is hand-written; width tables for rendering are approximations.
* **Limits**: parser nesting 100, operator chains 300; checker recursion follows AST depth, so run it on a large stack for hostile input (`pebblec` does).

## Stage 2: middle and back end

```text
 CheckResult.hir --mir::lower::lower--> MirProgram (monomorphic CFG; instance counts)   --mir::verify
      --opt::optimize (-O0..-O3, verify after every pass, --dump-after)-->  MirProgram
      --bc::codegen::generate--> Chunk (register bytecode)                               --bc::verify
      --vm::run--> output / RuntimeError (same messages, kinds and traces as interp::run)
```

* **Entry points:** `pebble::driver::{compile, compile_passes, run, run_chunk, bench_program}`; `pebble::mir::lower::lower`, `pebble::opt::{optimize, run_passes, PASS_NAMES}`, `pebble::bc::{codegen::generate, text::{disassemble, assemble}, verify::verify}`, `pebble::vm::run`.
* **Runtime errors have locations.** MIR statements carry spans, codegen emits a per-instruction span table (`BcFunc::pc_span`), and the VM builds the same `RuntimeError { span, trace }` as the interpreter; `RuntimeError::render(&SourceFile)` prints both back ends identically (`--> file:line:col`, plus a `note: f was inlined from ...` line per inlined frame at `-O3`). Fuel is counted in different units (tree nodes vs instructions), so for fuel exhaustion only the kind and message are compared; the VM reports the span of the instruction that would have run.
* **The interpreter stays the oracle.** `tests/backend.rs` requires identical stdout, error message, error kind, stack trace, `-->` span and per-frame call sites on the interpreter and the VM at every optimisation level (the `Outcome` struct in `tests/common`; `main`'s own call site is not compared, and fuel exhaustion compares only the kind), for all run tests, all examples, 1500 type-directed generated programs (a third of them forced to trap) and 500 imperative ones.
* **Design decisions.** Typed, non-SSA MIR (mutable locals, dataflow passes) keeps lowering and the inliner small; calls are terminators; match lowering is a tag `Switch` plus ordered arm tests; generics are removed by instantiation, so the VM is untyped but the bytecode is chosen by static type (`add` vs `concat`, `lti` vs `lts`); values are reference counted (see `docs/BYTECODE.md` for the NaN-boxing trade-off and cycle caveat); per-statement *scopes* keep traces exact under inlining.
* **Known deviation.** Self tail calls become loops at -O2/-O3, so the recursion-depth limit no longer applies to them (documented and tested). Physical frames of inlined non-recursive helpers no longer count against the limit.
* **SSA at -O3.** `opt/dom.rs`, `ssa.rs`, `sccp.rs`, `gvn.rs`, `licm.rs` (read in that order); see `docs/OPTIMISATIONS.md`.
* **Not done:** a module system (one file is one program), unqualified enum variants, loop rotation, a MIR interpreter for the whole language (the VM at -O0 plays that role).

## Stage 3 (separate crate)

A formatter (re-attach `ParseResult::comments` by span; `Node` is the walker), a syntax highlighter on the token stream plus `SemanticIndex`, and an LSP server: diagnostics via `to_json` fields (`utf16_*` positions), hover = `type_at`, go-to-definition = `definition_at`, references = `references`, outline = `symbols`, completion = `completions_at`/`fields_of`, quick fixes from `Diagnostic::suggestions`.
