A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`line_col` and `render_error` in `src/sql/error_render.rs`: `line_col(src, offset)` turns a byte offset into a 1-based `(line, column)` where the column counts **characters**; `render_error(src, offset, len, msg)` prints the message, the offending source line, and a line of spaces and `^` marks under the error.

## Why

`syntax error at byte 47` is a debugging session; ```ERROR: unknown column 'nmae'\nLINE 1: SELECT nmae FROM t\n               ^``` is a fix. Every SQL front end does this, and the traps are the usual ones: lines end with `\r\n` as well as `\n`, columns count characters not bytes, and an error at the very end of the input has a position too.

## The contract

- Lines end at `\n` (a preceding `\r` is not part of the line). Line and column are 1-based; a tab counts as one column.
- `line_col(src, offset)` for `offset == src.len()` is the position just after the last character.
- `render_error` returns `msg`, then `LINE <n>: <line text>`, then a caret line aligned under the error (the `LINE <n>: ` prefix counted), with `max(len, 1)` carets, cut at the end of the line.

## Invariants

These must hold after every step, whatever the input:

- `line_col` is monotone: a later offset never has an earlier (line, column).
- The caret column equals the error column plus the prefix width.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Splitting the source differently (`\n` vs `\r\n`) gives the same line and column numbers.
- Moving the error one character right moves the caret one place right (within the line).
- An offset at a line start has column 1.

## Examples

Worked cases (the tests include them):

```text
"SELECT a\nFROM t" offset 12 -> (2, 4)
"héllo" offset 3 -> column 3 (the bytes of é count once)
```

## What the tests check

- Positions on several lines, with CRLF and multi-byte characters.
- The end of input.
- Rendering with one and several carets.

## Done when

All the `s3d_c4` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Compiler-style diagnostics](/cookbook/p/diagnostics-spans): turning a byte span into line and column, with a caret under the problem.
- [Rust-quality error messages](/cookbook/p/rust-quality-error-messages): rendering rustc-style diagnostics, including display width.
