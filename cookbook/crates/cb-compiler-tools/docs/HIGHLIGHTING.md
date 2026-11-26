# Syntax highlighting

## Lexical layer (`highlight::lexical`)

Every byte of the file belongs to exactly one `HlToken` (tokens, comments, and `Whitespace` for gaps; string tokens are split into `String` and `Escape` pieces). Classes: keyword, control keyword (`if else match while loop break continue return`), type, function, variable, parameter, constant, enum variant, property, string, escape, number, operator, punctuation, comment, doc comment, attribute (the modifier `mut`; PEBBLE has no attribute syntax), error (lexer error tokens). Identifiers are classified by neighbours only: `fn name` and `name(` are functions, `X::name` is a variant, `int bool str` and capitalised names are types. It works on any text, including files that do not parse.

## Semantic layer (`highlight::semantic`)

Identifiers found in the `SemanticIndex` (through a sorted `NameTable` for O(log n) lookups) get their class from the definition kind (function, struct/enum/type parameter, variant, field, constant, local, parameter). Modifiers: `mutable` (also on uses), `declaration`, `unused` (the checker's W0501/W0502/W0507/W0508 on the defining name; rendered faded), `deprecated` (the definition's doc comment starts with `Deprecated` or contains `@deprecated`; rendered faded), `builtin`.

## Output formats

`pebble-tools highlight FILE [--semantic] [--format=ansi|html|json]`. ANSI uses 256-colour codes (italic comments, underline for `mut`, dim for faded, bold red for errors). HTML is a standalone page with inline CSS, light and dark palettes via `prefers-color-scheme`, every text node escaped. JSON lists `{start, end, class, modifiers, text}` for non-whitespace tokens.

## TextMate grammar and VS Code extension

`pebble-tools gen-editors` writes `editors/pebble.tmLanguage.json` (copied to `editors/vscode/syntaxes/`), `editors/vscode/{package.json, language-configuration.json, extension.js}`. All of it is generated from `src/textmate.rs`; the keyword list comes from the compiler's token table, and `tests/highlight.rs` fails when the files on disk are stale or the list drifts (the exhaustive `match` in `textmate::kind_guard` also stops compilation when the lexer gains a token kind). The language configuration defines comments, brackets, auto-closing pairs, surrounding pairs, indentation rules and `///` continuation. The extension starts `pebble-tools lsp` through `vscode-languageclient` (listed in `package.json`; run `npm install` in `editors/vscode` to use it; nothing is installed here).

## Tree-sitter notes (description only, no grammar is shipped)

A tree-sitter grammar would map one-to-one onto `docs/LANGUAGE.md`: `source_file: repeat(item)`; items `function_item | struct_item | enum_item | const_item`; precedence levels from the parser's binding powers (assignment 1, `||` 2, `&&` 4, comparison 6 non-associative, additive 8, multiplicative 10, `as` 12, unary, postfix); struct literals disallowed in `if`/`while`/`match` heads via a conflict or an external flag. Comments are `extras`. A `highlights.scm` would capture `(function_item name: (identifier) @function)`, `(parameter name: (identifier) @variable.parameter)`, `(type_identifier) @type`, `(enum_variant_path) @constructor`, string escapes as `@string.escape`, and keywords by listing the same 17 words. Tree-sitter's incremental, error-tolerant parsing would replace the TextMate layer, while the semantic layer here would stay as the LSP semantic tokens.
