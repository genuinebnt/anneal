# cb-compiler: PEBBLE, a small compiler with professional diagnostics

A statically typed language (generics, enums with payloads, pattern matching with exhaustiveness checking, first-class functions) implemented with zero dependencies: lexer, recoverable parser, name resolution, Hindley-Milner style inference, rustc-style diagnostics, a tree-walking interpreter (the test oracle), and, since stage 2, a **middle and back end**: monomorphising MIR with a verifier, an optimiser (scalar passes, inlining, and SSA with SCCP, GVN and loop-invariant code motion), a register bytecode with assembler/verifier, and a VM. Stage 3 (formatter, highlighter, LSP) lives in a separate crate.

```text
cd cookbook
cargo run -q -p cb-compiler --bin pebblec -- run crates/cb-compiler/examples/showcase.peb
cargo run -q -p cb-compiler --bin pebblec -- check --color FILE
cargo test -q --release -p cb-compiler          # BLESS=1 regenerates golden files and docs/ERRORS.md
```

`pebblec check|run|dump-tokens|dump-ast|dump-hir|dump-mir|dump-cfg|dump-bc FILE [--color] [--error-format=json|human] [--warn-shadow]`. Exit codes: 0 ok, 1 compile errors, 2 usage/I-O error, 3 run-time failure.

### Stage 2 commands

```text
pebblec run FILE [--backend=interp|vm] [--opt-level=0..3] [--dump-after=PASS] [--time]
pebblec dump-mir FILE [--opt-level=N]     # monomorphic MIR (default level 0 = as lowered)
pebblec dump-cfg FILE | dot -Tsvg -o cfg.svg
pebblec dump-bc FILE [--opt-level=N]      # round-trippable bytecode listing
pebblec bench crates/cb-compiler/examples/bench   # interpreter vs VM -O0 vs VM -O3
```

`--backend` defaults to `vm`, `--opt-level` to 2. Passes for `--dump-after`: `simplify-cfg const-prop copy-prop cse strength dce inline tco bce ssa sccp gvn licm out-of-ssa` (or `all`).

### Measured on the benchmark programs (release build, one run each, your machine will differ)

```text
program               interp ms   vm-O0 ms   vm-O3 ms   speedup      instrs O0      instrs O3  agree
fib30                     260.8       64.4       50.4      5.2x       16155225       12116420  yes
loop_invariant             71.1       31.3        9.7      7.3x        7950177        4362167  yes
nbody_int                  39.4       29.5       19.9      2.0x        3764096        3172081  yes
nested_loops              123.9       26.9       25.8      4.8x       10913257       10913257  yes
sieve_1m                  286.0       67.8       74.2      3.9x       21617414       20694913  yes
sort_100k                 233.3       59.0       46.4      5.0x       14447297       13256142  yes
string_build                3.2        1.5        1.4      2.3x          66320          57320  yes
tree_traversal            135.7       54.5       43.8      3.1x        7035121        4414883  yes
```

(`cargo test -p cb-compiler --release -- --nocapture bench_programs` prints the same table. Instruction counts are deterministic; single-run times are noisy.) Most of the speed-up over the interpreter comes from the register machine and typed instruction selection.

## Example session

```text
$ pebblec check demo.peb
error[E0301]: `if` and `else` have incompatible types
 --> demo.peb:5:12
  |
3 |       let x = if c {
  |                    - this is of type `int`
4 |           1
5 |       } else {
  |  ____________^
6 | |         "one"
7 | |     };
  | |_____^ expected `int`, found `str`

error[E0203]: no field `xx` on type `Point`
 --> demo.peb:12:22
   |
 1 | struct Point { x: int, y: int }
   |        ----- `Point` defined here
...
12 |     println(to_str(p.xx));
   |                      ^^ unknown field
   |
   = help: a field with a similar name exists: `x`

error: aborting due to 2 previous errors
```

## Layout

* `src/` - library `pebble` and binary `pebblec` (reading order in `docs/ARCHITECTURE.md`).
* `src/mir/` (lowering + monomorphisation, verifier, printer), `src/opt/` (passes), `src/bc/` (bytecode, codegen, assembler, verifier), `src/vm/`, `src/driver.rs` (the whole back-end pipeline, benchmarks).
* `docs/LANGUAGE.md` - grammar, types, semantics. `docs/ERRORS.md` - every diagnostic code with an example (generated). `docs/ARCHITECTURE.md` - pipeline, API contract, reading order. Stage 2: `docs/MIR.md`, `docs/OPTIMISATIONS.md`, `docs/BYTECODE.md`.
* `examples/*.peb` - sample programs; `examples/bench/*.peb` - benchmarks. `tests/ui` golden diagnostics (80+), `tests/run` programs with expected output (50+), `tests/fuzz.rs`, `tests/props.rs` (type-soundness property tests), `tests/units.rs`.
* Stage 2 tests: `tests/backend.rs` (differential: interpreter vs VM -O0..-O3 on all run tests, examples, 1500 generated programs with traps and 500 imperative ones; each pass alone; monomorphisation; benchmarks), `tests/ssa.rs` (SSA, SCCP, GVN, LICM), `tests/mir.rs` + `tests/mir/*.peb|mir` (golden before/after MIR per pass, `BLESS=1`), `tests/verify.rs` (verifier negatives, assembler round trip, bytecode fuzzing).

## Further reading

* Crafting Interpreters: <https://craftinginterpreters.com/>
* Rustc dev guide (diagnostics, type checking): <https://rustc-dev-guide.rust-lang.org/>
* Resilient parsing: <https://matklad.github.io/2023/05/21/resilient-ll-parsing-tutorial.html> (blog: <https://matklad.github.io/>)
* Pattern-match usefulness: Maranget, "Warnings for pattern matching".
