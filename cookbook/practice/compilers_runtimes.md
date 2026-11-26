# Practice: compilers_runtimes

How to use this page: run each recipe (`cargo run --release --example <name>`), answer Prove you got it, then do Try next in order.
These recipes self-check by running programs, so extend them the same way: add a program, predict its output, then run.
No solutions here. S about 30 min, M about 2 h, A about a day. Recipe files are in `examples/compilers_runtimes/`.

## hindley_milner_inference
**Prove you got it**
1. Why does `let id = \x. x in (id 1, id true)` type check but the lambda-bound version does not?
2. What does the occurs check prevent, with an example term?

**Try next**
- [S] Add a `fix`/recursion-free `if` form and write 5 well-typed and 5 ill-typed terms asserting the printed principal type.
- [M] Add a type error that reports two spans (the use and the conflicting declaration), and test the message text.
- [A] Add type classes or row-polymorphic records (pick one) and write a property test that every inferred type is stable under renaming of bound variables.

**Use it in:** inference in ML-family languages. Real code: [rust-lang/rust](https://github.com/rust-lang/rust) (`rustc_infer`), and the `cb-compiler` crate in this repo.
**Pitfall:** generalising variables that appear in the environment makes the system unsound.

## parser_generators_lr_peg_earley
**Prove you got it**
1. Show a grammar where SLR(1) reports a conflict that LR(1) would not.
2. How does packrat memoisation make PEG linear and what does it cost?

**Try next**
- [S] Add the dangling-else grammar and show each engine's result.
- [M] Make the three engines agree on 500 random strings from random grammars, and print the first disagreement.
- [A] Add LALR(1) lookahead merging and count states against canonical LR(1).

**Use it in:** language tooling. Real code: [lalrpop/lalrpop](https://github.com/lalrpop/lalrpop), [pest-parser/pest](https://github.com/pest-parser/pest).
**Pitfall:** PEG ordered choice silently hides alternatives that can never match.

## ssa_register_allocation
**Prove you got it**
1. Where do phi nodes go and why the iterated dominance frontier?
2. Why do critical edges need splitting before out-of-SSA?

**Try next**
- [S] Add a program with a loop and print the SSA form before and after optimisation.
- [M] Reduce registers to 3 and force spills; check by running the program at each register count.
- [A] Compare against the allocator of `regalloc2` ([bytecodealliance/regalloc2](https://github.com/bytecodealliance/regalloc2)) on the same live ranges conceptually: write a table of what each does differently from reading its docs.

**Use it in:** backends. Real code: [bytecodealliance/regalloc2](https://github.com/bytecodealliance/regalloc2).
**Pitfall:** the lost-copy and swap problems when sequentialising parallel copies.

## wasm_binary_interpreter
**Prove you got it**
1. What does the validator check that the interpreter would otherwise have to?
2. Why is a structured control flow stack needed for `br`?

**Try next**
- [S] Assemble a recursive factorial and a loop-summing program and assert results.
- [M] Add a fuel limit and a trap for exceeding it; test an infinite loop is stopped.
- [A] Add f32/f64 support, and test with a handful of spec-style cases against known results from the [WebAssembly/spec](https://github.com/WebAssembly/spec).

**Use it in:** sandboxed plugins. Real code: [bytecodealliance/wasmtime](https://github.com/bytecodealliance/wasmtime), [bytecodealliance/wasm-tools](https://github.com/bytecodealliance/wasm-tools).
**Pitfall:** skipping validation turns hostile bytes into memory-safety bugs.

## Chapter projects

Other-chapter stems used: `pratt_parser`, `bytecode_vm` (text_and_parsing), `toy_language_interpreter` (capstones), `edge_dsl_parser` (text_and_parsing). The `cb-compiler` crate is also a model; see `practice/crate_cb-compiler.md`.

### Small (about 2 h): typed calculator
Combine `pratt_parser`, `hindley_milner_inference`.
Milestones: (1) parse expressions with let and lambda; (2) infer types; (3) print errors with spans.
Acceptance: 30 programs with expected types or errors pass; no panics on random token soup.
Stretch: add pairs and lists.

### Intermediate (a weekend): optimising backend for a tiny language
Combine `bytecode_vm`, `ssa_register_allocation`, `toy_language_interpreter`.
Milestones: (1) lower the AST to a three-address IR; (2) constant folding and dead code elimination; (3) run on the VM; (4) differential test against the tree-walker.
Acceptance: 500 generated programs give identical output on both paths; instruction count drops on at least 3 benchmarks.
Stretch: SSA construction.

### Advanced (a week): sandboxed plugin host
Combine `wasm_binary_interpreter`, `binary_format_parsing` (text_and_parsing), `hmac_hkdf_pbkdf2` (cryptography), `hindley_milner_inference`.
Milestones: (1) load modules signed with an HMAC; (2) validate and run with fuel; (3) host calls with typed signatures; (4) a tiny text format to assemble modules.
Acceptance: tampered modules are rejected; infinite loops stopped; fuzzing 10k mutated modules yields only clean errors.
Stretch: compile your tiny language to this wasm subset.
