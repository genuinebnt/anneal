# Building: A language, DSL or compiler



Use this when you have a real project in front of you. For each stage: **attempt your design first**, then read the listed recipes to compare, steal what is better and note what you missed.

### Step 1: Front end

**Goal:** Syntax.

| # | Recipe | Why here |
|---|---|---|
| 1.1 | [`zero_copy_lexer`](../../examples/text_and_parsing/zero_copy_lexer.rs) · intermediate | Lexer. |
| 1.2 | [`pratt_parser`](../../examples/text_and_parsing/pratt_parser.rs) · advanced | Expressions. |
| 1.3 | [`edge_dsl_parser`](../../examples/text_and_parsing/edge_dsl_parser.rs) · advanced | A small DSL. |
| 1.4 | [`diagnostics_spans`](../../examples/errors/diagnostics_spans.rs) · intermediate | Good errors. |

### Step 2: Middle

**Goal:** Meaning.

| # | Recipe | Why here |
|---|---|---|
| 2.1 | [`hindley_milner_inference`](../../examples/compilers_runtimes/hindley_milner_inference.rs) · advanced | Types. |
| 2.2 | [`ssa_register_allocation`](../../examples/compilers_runtimes/ssa_register_allocation.rs) · advanced | IR and registers. |

### Step 3: Back end

**Goal:** Run it.

| # | Recipe | Why here |
|---|---|---|
| 3.1 | [`bytecode_vm`](../../examples/text_and_parsing/bytecode_vm.rs) · advanced | VM. |
| 3.2 | [`toy_language_interpreter`](../../examples/capstones/toy_language_interpreter.rs) · capstone | Interpreter. |
| 3.3 | [`wasm_binary_interpreter`](../../examples/compilers_runtimes/wasm_binary_interpreter.rs) · advanced | Target WASM. |

### Step 4: Tooling

**Goal:** Developer experience.

| # | Recipe | Why here |
|---|---|---|
| 4.1 | [`cb-compiler`](../../crates/cb-compiler/) · crate | Reference pipeline. |
| 4.2 | [`cb-compiler-tools`](../../crates/cb-compiler-tools/) · crate | Formatter and LSP. |
| 4.3 | [`cb-macros`](../../crates/cb-macros/) · crate | Procedural macros. |
