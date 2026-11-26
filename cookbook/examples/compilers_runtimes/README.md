# Compilers and Runtimes

WebAssembly decoding and interpretation, SSA and register allocation, type inference, parser generators, bytecode VMs.

Run any recipe from the `cookbook/` directory: `cargo run --release --example <name>` (the name is the file name without `.rs`). Each file starts with a header: what it shows, `LEVEL`, `COMMON USES` and `Read:` links.

4 examples.

| Example | What it shows | Level | Common uses | Crates |
|---|---|---|---|---|
| [`wasm_binary_interpreter`](wasm_binary_interpreter.rs) | WebAssembly (MVP subset) binary decoder + validator + interpreter, with an assembler | advanced | running untrusted plugins in a sandbox (Envoy/proxy filters, database UDFs, editors), edge and | std only |
| [`ssa_register_allocation`](ssa_register_allocation.rs) | a mini compiler backend: CFG, dominators, SSA, optimisation, liveness, register allocation, out-of-SSA | advanced | the backend shape of LLVM/Cranelift/V8/HotSpot-style compilers (SSA IR, dominators, linear-scan or graph | std only |
| [`hindley_milner_inference`](hindley_milner_inference.rs) | Hindley-Milner type inference with let-polymorphism, level-based generalisation and source-span errors | advanced | ML, OCaml, Haskell (as the core of a richer system), Elm and PureScript inference; typed configuration and query | std only |
| [`parser_generators_lr_peg_earley`](parser_generators_lr_peg_earley.rs) | three general parsing techniques on the same grammars: packrat PEG, SLR(1) LR generator, Earley | advanced | PEG: Python's CPython parser since 3.9, many hand-written DSL and Markdown/TOML parsers; LR: yacc/bison, most | std only |
