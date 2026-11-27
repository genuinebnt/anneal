# Text Processing and Parsing

Lexers, parser combinators, recursive descent, Pratt parsing, JSON/CSV, bytecode compilers.

Run any recipe from the `cookbook/` directory: `cargo run --release --example <name>` (the name is the file name without `.rs`). Each file starts with a header: what it shows, `LEVEL`, `COMMON USES` and `Read:` links.

14 examples.

| Example | What it shows | Level | Common uses | Crates |
|---|---|---|---|---|
| [`zero_copy_lexer`](zero_copy_lexer.rs) | a zero-copy lexer: tokens borrow from the input via a lifetime | intermediate | DSL/config lexers, query parsers, log parsers: tokens that borrow from the input allocate nothing. | std only |
| [`pratt_parser`](pratt_parser.rs) | Pratt parser: precedence and associativity with "binding power" | advanced | expression languages: calculators, filter expressions in query languages, spreadsheet formulas. | std only |
| [`parser_combinators`](parser_combinators.rs) | hand-written parser combinators: small functions that compose into a parser | intermediate | durations/sizes/versions in configs, small DSL fragments, anything regex handles badly. | std only |
| [`edge_dsl_parser`](edge_dsl_parser.rs) | a tiny language: tokenizer + recursive-descent parser + errors with spans | advanced | workflow/config DSLs, graph description languages (DOT-like), query languages. | std only |
| [`ndjson_serde`](ndjson_serde.rs) | serde beyond structs: tagged enums, renames, streaming NDJSON, tolerant line-by-line parsing | simple | event logs, webhooks, queue messages: tagged enums, tolerant NDJSON ingestion. | serde, serde-json |
| [`json_parser`](json_parser.rs) | a complete recursive-descent JSON parser with error positions, escapes, and a depth limit | advanced | config/IO boundaries, understanding serde_json internals, hardening recursive parsers against hostile input. | std only |
| [`csv_state_machine`](csv_state_machine.rs) | a CSV parser as an explicit character state machine, plus a round-trip property test | intermediate | CSV/TSV ingestion, any format where quoting/escaping changes the meaning of characters. | std only |
| [`bytecode_vm`](bytecode_vm.rs) | a mini compiler pipeline: lex -> Pratt parse -> AST -> optimise -> bytecode -> stack VM | advanced | scripting/DSL engines, query evaluators, config languages, WASM-like VMs; differential testing against an oracle. | std only |
| [`binary_format_parsing`](binary_format_parsing.rs) | safe binary format parsing: a bounds-checked Reader, TLV, a PNG-like chunk container, bit reading, zero-copy | advanced | image/audio/video container parsers, protobuf-like wire formats, network packet dissectors, database page and | std only |
| [`expression_parsing_three_ways`](expression_parsing_three_ways.rs) | one expression language, four parsers (recursive descent, precedence climbing, Pratt, shunting-yard) that must agree | advanced | query-language WHERE clauses, spreadsheet formulas, feature-flag/rule expressions, calculators, template engines, | std only |
| [`reading_html_safely`](reading_html_safely.rs) | Reading HTML safely: pull links and text out of a page you don't trust | intermediate | crawlers, link previews in chat and mail, feed readers, site audits. | std only |
| [`javascript_syntax_tree`](javascript_syntax_tree.rs) | Reading JavaScript as a syntax tree: find the addresses a script fetches, without running it | advanced | linters, bundlers, code search, finding the API calls in a script. | std only |
| [`source_maps`](source_maps.rs) | Source maps: from a place in generated code back to the file a person wrote | advanced | minified JavaScript and CSS, TypeScript and Sass output, crash report services. | serde, serde-json |
| [`syntax_tree_diff`](syntax_tree_diff.rs) | Comparing syntax trees: show what changed in code, not which lines moved | advanced | review tools, ignoring formatting-only commits, finding moved functions, checking generated code. | std only |
