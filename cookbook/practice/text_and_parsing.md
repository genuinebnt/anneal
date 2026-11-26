# Practice: text_and_parsing

How to use this page: run the recipe (`cargo run --release --example <name>`), answer Prove you got it, then do Try next in order.
For parsers the best check is a round trip or a second implementation; write that oracle first.
No solutions here. S about 30 min, M about 2 h, A about a day. Recipe files are in `examples/text_and_parsing/`.

## binary_format_parsing
**Prove you got it**
1. Why must lengths from the file never be used to preallocate before checking `remaining()`?
2. What does an error carrying the byte offset give you over a panic?

**Try next**
- [S] Truncate a valid file at every offset and assert each gives a typed error, never a panic.
- [M] Add a chunk with a CRC and a nested chunk container; fuzz with 10k mutated inputs, asserting bounded memory.
- [A] Write a reader for a real small format (e.g. WAV or a PNG header and chunks) and compare to a known file's fields.

**Use it in:** image and network codecs. Real code: [tokio-rs/prost](https://github.com/tokio-rs/prost) for protobuf decoding.
**Pitfall:** integer overflow in `offset + len` wrapping past the bounds check.

## bytecode_vm
**Prove you got it**
1. Why does post-order traversal produce valid stack code?
2. What does constant folding change in the bytecode for `2*3+x`?

**Try next**
- [S] Add `%` and unary minus and a test per op.
- [M] Add variables and `if` with jumps, checking jump targets are validated before running.
- [A] Differential test: evaluate 1000 random expressions with a tree walker and the VM and assert equal results and equal overflow errors.

**Use it in:** embedded scripting. Real code: the stage 2 back end in `crates/cb-compiler` and [WebAssembly/spec](https://github.com/WebAssembly/spec) for stack machine semantics.
**Pitfall:** stack underflow on malformed bytecode if not verified.

## csv_state_machine
**Prove you got it**
1. List the states and say what a quote means in each.
2. Why does the round-trip property catch writer bugs a parser test misses?

**Try next**
- [S] Add CRLF handling and test with a field containing a newline.
- [M] Make it streaming: feed 1-byte chunks and assert the same rows as a single chunk.
- [A] Compare with [BurntSushi/rust-csv](https://github.com/BurntSushi/rust-csv) on 1000 generated tables for equality and speed.

**Use it in:** ETL. Real code: [BurntSushi/rust-csv](https://github.com/BurntSushi/rust-csv).
**Pitfall:** a quote in the middle of an unquoted field.

## edge_dsl_parser
**Prove you got it**
1. Why do tokens keep spans, and what does the error show?
2. What does recursive descent do per grammar rule?

**Try next**
- [S] Add a comment syntax and test that spans stay correct after it.
- [M] Add error recovery to report 3 errors in one input.
- [A] Port to `parser_combinators` style and compare errors for 20 bad inputs.

**Use it in:** config languages. Real code: [oxc-project/oxc](https://github.com/oxc-project/oxc) parser.
**Pitfall:** spans counted in chars vs bytes.

## expression_parsing_three_ways
**Prove you got it**
1. How does each algorithm encode associativity?
2. Why are comparisons non-associative here?

**Try next**
- [S] Add a new operator in all four with one table change if possible.
- [M] Generate 5000 random expressions and assert all four ASTs equal; print the first diff.
- [A] Add error messages and compare them across the four for 20 malformed inputs.

**Use it in:** language front ends. Real code: [oxc-project/oxc](https://github.com/oxc-project/oxc), [rust-lang/rust-analyzer](https://github.com/rust-lang/rust-analyzer).
**Pitfall:** the ternary and unary minus binding with `**`.

## json_parser
**Prove you got it**
1. What does the depth limit prevent?
2. How are surrogate pairs decoded?

**Try next**
- [S] Test the JSONTestSuite style cases: leading zeros, trailing commas, lone surrogates.
- [M] Make numbers preserve big integers losslessly and floats round trip.
- [A] Benchmark against [serde-rs/json](https://github.com/serde-rs/json) on a 50 MB file and explain the gap.

**Use it in:** every API. Real code: [serde-rs/json](https://github.com/serde-rs/json).
**Pitfall:** duplicate keys: decide and test.

## ndjson_serde
**Prove you got it**
1. What happens to the rest of the stream after one bad line?
2. What does `tag = "type"` do on the wire?

**Try next**
- [S] Add an unknown event type and decide: error, skip, or catch-all variant.
- [M] Stream a 1 GB generated file and assert flat memory.
- [A] Add line offsets in error reports and resume parsing after a truncated last line.

**Use it in:** log pipelines. Real code: [vectordotdev/vector](https://github.com/vectordotdev/vector).
**Pitfall:** `\r\n` and a final line without newline.

## parser_combinators
**Prove you got it**
1. Why do combinators return remaining input?
2. When does `many` loop forever?

**Try next**
- [S] Add `separated_list` and a test with trailing separator.
- [M] Add error positions and "expected one of" sets.
- [A] Re-implement one grammar with [winnow-rs/winnow](https://github.com/winnow-rs/winnow) and compare code size and speed.

**Use it in:** protocol parsing. Real code: [rust-bakery/nom](https://github.com/rust-bakery/nom), [winnow-rs/winnow](https://github.com/winnow-rs/winnow).
**Pitfall:** left recursion and backtracking cost.

## pratt_parser
**Prove you got it**
1. Why right power < left power for right associativity?
2. Where do prefix operators fit?

**Try next**
- [S] Add `!` postfix factorial and test.
- [M] Add a ternary and assert it matches `expression_parsing_three_ways`.
- [A] Make the operator table runtime-configurable and fuzz-compare with a precedence-climbing build.

**Use it in:** expression parsers. Real code: [rust-lang/rust-analyzer](https://github.com/rust-lang/rust-analyzer) expression parser.
**Pitfall:** equal binding powers make associativity undefined.

## zero_copy_lexer
**Prove you got it**
1. What does the lifetime on `Token<'a>` forbid you from doing?
2. Why is the lexer an Iterator?

**Try next**
- [S] Count allocations during lexing with a counting allocator and assert zero.
- [M] Add Unicode identifiers and test byte vs char spans.
- [A] Compare speed against a version that allocates `String`s and report the ratio on a 10 MB file.

**Use it in:** compilers. Real code: [oxc-project/oxc](https://github.com/oxc-project/oxc) `oxc_parser` lexer.
**Pitfall:** slicing a `&str` in the middle of a UTF-8 char panics.

## Chapter projects

Other-chapter stems used: `serde_custom` (encoding_compression), `tiny_sql_engine` and `regex_nfa_engine` (capstones), `hindley_milner_inference` (compilers_runtimes), `trigram_index` (search_vectors).

### Small (about 2 h): log tool
Combine `ndjson_serde`, `csv_state_machine`, `json_parser`.
Milestones: (1) read NDJSON or CSV; (2) filter by field; (3) export in the other format.
Acceptance: round trip of 1000 random records equals; bad lines reported with line numbers, not panics.
Stretch: streaming with flat memory.

### Intermediate (a weekend): expression language with diagnostics
Combine `zero_copy_lexer`, `pratt_parser`, `edge_dsl_parser`, `bytecode_vm`.
Milestones: (1) spans on everything; (2) parse with 3 errors recovered; (3) compile and run.
Acceptance: golden tests for 20 bad programs; 500 random good programs match a tree-walking evaluator.
Stretch: type check with `hindley_milner_inference`.

### Advanced (a week): binary log format with query
Combine `binary_format_parsing`, `trigram_index`, `regex_nfa_engine`, `compression_lz77_huffman` (encoding_compression).
Milestones: (1) chunked, checksummed, compressed log file; (2) reader with bounds checks; (3) trigram index over decompressed text; (4) regex search with verify.
Acceptance: 10k mutated files never panic; search results equal a linear scan; index narrows candidates by 10x on a test corpus.
Stretch: incremental append.
