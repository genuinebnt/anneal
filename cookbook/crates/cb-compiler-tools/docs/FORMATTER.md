# The PEBBLE formatter

`pebble_tools::fmt::format(&str) -> Result<String, FormatError>`; CLI `pebble-tools fmt FILE [--check]`. Line width 100, indent 4 spaces.

## Pipeline

1. `pebble::syntax::parse` gives the AST and the comment side table (comments are not tokens).
2. `fmt/build.rs` walks the AST **in source order** and builds a `Doc`.
3. `fmt/doc.rs` prints the `Doc` at the target width.
4. Safety net: the output is parsed again; if it has errors or a different comment count, `FormatError::Internal` is returned instead of damaged text.

## Broken input

`format` (and `pebble-tools fmt`, `fmt --check`) is **strict**: any lexer or parser error returns `FormatError::Syntax(diagnostics)` and nothing is written.

**Best effort** (`format_best_effort`, `pebble-tools fmt --best-effort`, and the language server for a file that does not parse) works one top-level item at a time. An item is re-printed only if no error diagnostic touches its span, its own text parses without errors when taken alone, and the formatted item parses to the same AST. Everything else (error items, stray tokens, and the gaps between items with their comments and blank lines) stays byte for byte.

## Line endings

`\n`, `\r\n` and a lone `\r` are accepted. The formatter works on LF text and writes the file's dominant terminator.

## The document algebra

```text
Doc = Nil | Text(s) | Line | SoftLine | HardLine | BreakParent
    | Concat([Doc]) | Nest(n, Doc) | Group(Doc) | IfBreak(broken, flat)
```

`Group` is the only choice. The printer tries the group flat; if everything up to the next possible line break (including text after the group) fits in the remaining columns it stays flat, otherwise its direct `Line`/`SoftLine` become newlines, and nested groups decide again. `Line` is a space when flat, `SoftLine` nothing. `HardLine` is always a newline and `BreakParent` is invisible but makes every enclosing group break; both make `fits` fail, which is how a `//` comment forces its list onto separate lines. `IfBreak(",", "")` gives trailing commas only in the broken layout. Indentation is written lazily so blank lines never contain spaces.

Layout rules: calls, arrays, tuples, struct literals, parameter lists and struct/enum bodies are groups with `SoftLine`/`Line` inside `Nest(4)`; operator chains of one precedence level form one group that breaks before every operator; `let x = value` keeps the value on the `=` line when it can break itself (call, struct literal, `if`, `match`, ...) and otherwise breaks after `=`; blocks are always multi-line except `if`/`else` and match-arm blocks holding only a tail expression (`{ x }`) that fit; `match` arms are one per line; a block-bodied arm has no trailing comma. Parentheses are never added or removed (the AST keeps `Paren` nodes), so precedence cannot change. Literals are copied from the source (`1_000`, escapes). Top-level items are separated by exactly one blank line (consecutive `const`s keep the source spacing).

## Comment attachment

The builder holds a cursor into the comment table that only moves forward.

| Kind | Rule |
|------|------|
| leading | Before an item, statement, field, variant, parameter, argument or arm: every not-yet-printed comment that ends before it goes on its own line(s) above it. |
| trailing | A comment that starts on the line where an element ends, before the next element or the closing delimiter, stays on that line (`let x = 1; // why`). A line comment forces the surrounding group to break. |
| dangling | Comments left before a closing `}`, `)` or `]` (including inside an empty block) are printed last, inside the delimiters. |
| leftover | A comment in a position the builder does not visit (inside a type, between `}` and `else`) is printed by the next leading/dangling flush, i.e. it moves down to the next sensible place. |

Blank lines between statements, fields, variants and arms are preserved but collapsed to at most one; blank lines after `{` or before `}` are dropped. Because the cursor never moves backwards and everything left is flushed at the end of the module, comments cannot be lost or reordered. `//` comment trailing whitespace is trimmed; block comment text (even multi-line) is verbatim.

## Idempotence

Layout depends only on the AST, comment positions relative to elements, and blank-line counts; all three are reproduced by the output. The one trap found by the property tests: choosing the compact `{ tail }` block form must not depend on whether a *leftover* comment gets flushed inside the block, so the builder tries the compact form on a scratch cursor and falls back when it consumed a comment.

## Range formatting

`format_range` re-prints only the top-level items that overlap the range, using the best-effort rule above; text outside those items is untouched, and each edit is reduced to its common-prefix/suffix minimum. A range that touches no item (for example a comment between items) produces no edit. `format_on_type` (after `}` or `;`) applies the same to the item just typed in.

**Splice safety.** An item that is correct on its own can still be wrong in place: the builder may move a comment from inside an item to after its last token (`const K: int = (1 // why\n);` becomes `const K: int = (1);` followed by `// why`). If a `//` comment then ends the replacement and other text follows on that line, a line break is added so the comment cannot swallow it. As a last guard the whole spliced file must have the same AST and the same number of comments as the input, otherwise no edit is returned.
