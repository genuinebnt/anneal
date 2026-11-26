# Chapter `compilers_runtimes`: suggested reading order

Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. Cross-chapter order is in [the tracks](../tracks/).

| # | Recipe | Level | What it is |
|---|---|---|---|
| 1 | [`hindley_milner_inference`](../../examples/compilers_runtimes/hindley_milner_inference.rs) | advanced | Hindley-Milner type inference with let-polymorphism, level-based generalisation and source-span errors. |
| 2 | [`parser_generators_lr_peg_earley`](../../examples/compilers_runtimes/parser_generators_lr_peg_earley.rs) | advanced | three general parsing techniques on the same grammars: packrat PEG, SLR(1) LR generator, Earley. |
| 3 | [`ssa_register_allocation`](../../examples/compilers_runtimes/ssa_register_allocation.rs) | advanced | a mini compiler backend: CFG, dominators, SSA, optimisation, liveness, register allocation, out-of-SSA. |
| 4 | [`wasm_binary_interpreter`](../../examples/compilers_runtimes/wasm_binary_interpreter.rs) | advanced | WebAssembly (MVP subset) binary decoder + validator + interpreter, with an assembler. |
