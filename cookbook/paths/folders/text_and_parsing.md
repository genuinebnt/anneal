# Chapter `text_and_parsing`: suggested reading order

Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. Cross-chapter order is in [the tracks](../tracks/).

| # | Recipe | Level | What it is |
|---|---|---|---|
| 1 | [`ndjson_serde`](../../examples/text_and_parsing/ndjson_serde.rs) | simple | serde beyond structs: tagged enums, renames, streaming NDJSON, tolerant line-by-line parsing. |
| 2 | [`csv_state_machine`](../../examples/text_and_parsing/csv_state_machine.rs) | intermediate | a CSV parser as an explicit character state machine, plus a round-trip property test. |
| 3 | [`parser_combinators`](../../examples/text_and_parsing/parser_combinators.rs) | intermediate | hand-written parser combinators: small functions that compose into a parser. |
| 4 | [`zero_copy_lexer`](../../examples/text_and_parsing/zero_copy_lexer.rs) | intermediate | a zero-copy lexer: tokens borrow from the input via a lifetime. |
| 5 | [`binary_format_parsing`](../../examples/text_and_parsing/binary_format_parsing.rs) | advanced | safe binary format parsing: a bounds-checked Reader, TLV, a PNG-like chunk container, bit reading, zero-copy. |
| 6 | [`bytecode_vm`](../../examples/text_and_parsing/bytecode_vm.rs) | advanced | a mini compiler pipeline: lex -> Pratt parse -> AST -> optimise -> bytecode -> stack VM. |
| 7 | [`edge_dsl_parser`](../../examples/text_and_parsing/edge_dsl_parser.rs) | advanced | a tiny language: tokenizer + recursive-descent parser + errors with spans. |
| 8 | [`expression_parsing_three_ways`](../../examples/text_and_parsing/expression_parsing_three_ways.rs) | advanced | one expression language, four parsers (recursive descent, precedence climbing, Pratt, shunting-yard) that must agree. |
| 9 | [`json_parser`](../../examples/text_and_parsing/json_parser.rs) | advanced | a complete recursive-descent JSON parser with error positions, escapes, and a depth limit. |
| 10 | [`pratt_parser`](../../examples/text_and_parsing/pratt_parser.rs) | advanced | Pratt parser: precedence and associativity with "binding power". |
