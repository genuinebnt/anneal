# Track: Compilers and languages

About 6-8 weeks. The flagship crate comes last.

**How to work each step:** read the header, predict the asserts, blank the file and rebuild it from memory (`./study.sh NAME`), then mutate it. Details in [HOW_TO_STUDY.md](../../HOW_TO_STUDY.md). Checkpoint = you can explain the step's goal in five lines without looking.

### Step 1: Lexing and parsing

**Goal:** Text to trees.

| # | Recipe | Why here |
|---|---|---|
| 1.1 | [`zero_copy_lexer`](../../examples/text_and_parsing/zero_copy_lexer.rs) · intermediate | Tokens without allocation. |
| 1.2 | [`expression_parsing_three_ways`](../../examples/text_and_parsing/expression_parsing_three_ways.rs) · advanced | Precedence climbing, Pratt, combinators. |
| 1.3 | [`pratt_parser`](../../examples/text_and_parsing/pratt_parser.rs) · advanced | Pratt in depth. |
| 1.4 | [`parser_combinators`](../../examples/text_and_parsing/parser_combinators.rs) · intermediate | Composable parsers. |
| 1.5 | [`json_parser`](../../examples/text_and_parsing/json_parser.rs) · advanced | A real grammar. |
| 1.6 | [`diagnostics_spans`](../../examples/errors/diagnostics_spans.rs) · intermediate | Errors that point at source. |

### Step 2: Execution

**Goal:** Run a program.

| # | Recipe | Why here |
|---|---|---|
| 2.1 | [`bytecode_vm`](../../examples/text_and_parsing/bytecode_vm.rs) · advanced | A stack VM. |
| 2.2 | [`toy_language_interpreter`](../../examples/capstones/toy_language_interpreter.rs) · capstone | End-to-end interpreter. |
| 2.3 | [`wasm_binary_interpreter`](../../examples/compilers_runtimes/wasm_binary_interpreter.rs) · advanced | A real binary format. |
| 2.4 | [`hindley_milner_inference`](../../examples/compilers_runtimes/hindley_milner_inference.rs) · advanced | Type inference. |
| 2.5 | [`ssa_register_allocation`](../../examples/compilers_runtimes/ssa_register_allocation.rs) · advanced | Middle and back end. |
| 2.6 | [`parser_generators_lr_peg_earley`](../../examples/compilers_runtimes/parser_generators_lr_peg_earley.rs) · advanced | Parser theory. |

### Step 3: The full pipeline

**Goal:** Read after building a toy.

| # | Recipe | Why here |
|---|---|---|
| 3.1 | [`cb-compiler`](../../crates/cb-compiler/) · crate | Pebble: parser, types, diagnostics, (stage 2) MIR and VM. |
| 3.2 | [`cb-compiler-tools`](../../crates/cb-compiler-tools/) · crate | Formatter, highlighter, LSP. |
