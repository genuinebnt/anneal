**Capstone, step 1 of 9.** Over nine steps you build a small scripting language. It gets two ways of running a program: by following its tree, and by running compiled instructions. This step cuts source text into tokens and reports what is wrong with it.

Write `line_col`, `scan`, `Diag::short` and `Diag::render`.

**Tokens.** `scan(src)` returns the tokens and the diagnostics. The last token is always `Eof`, with an empty span at the end of the source.

- Punctuation and operators: `( ) { } , . - + ; / * % ! != = == > >= < <=`. Always take two characters when they form an operator.
- A name starts with an ASCII letter or `_` and goes on with ASCII letters, digits and `_`. These words are keywords instead: `and break class continue else false for fun if nil or return super this true var while`.
- A number is ASCII digits, then optionally a `.` followed by at least one more digit. So `1.` is a number and a `Dot`.
- A string starts and ends with `"` and may run over several lines. The escapes are `\n`, `\t`, `\\` and `\"`. The token holds the text with escapes replaced.
- Spaces, tabs, `\r` and `\n` separate tokens. `//` starts a comment that ends before the next `\n`.
- `/*` starts a comment that ends with the matching `*/`. These comments **nest**: `/* a /* b */ c */` is one comment.

A span holds byte offsets `start..end` and the line on which it starts, counted from 1. Every `\n` counts, inside strings and comments too.

**Mistakes do not stop the scan.** Each one adds a diagnostic with `at: At::Nowhere`.

| Mistake | Message | Span | What happens next |
|---|---|---|---|
| a character that starts no token | `Unexpected character 'X'.` | that character | skip it |
| `\` and then any other character in a string | `Invalid escape sequence.` | the `\` and that character | leave both out; the string goes on |
| a string with no closing `"` | `Unterminated string.` | the opening `"` | no token for it |
| a comment with no matching `*/` | `Unterminated block comment.` | the opening `/*` | nothing is left |

A string whose last character is a lone `\` is just unterminated. Any Unicode text may appear anywhere; nothing may panic.

**Positions for people.** `line_col(src, pos)` gives the line and the column of a byte offset, both from 1. The column counts **characters**, not bytes. `pos` is on a character boundary and at most `src.len()`.

`short` gives one line: `[line L:C] Error at 'text': message`, from the span's start.

- With `At::Token`, `text` is the source under the span.
- With `At::End` it is `[line L:C] Error at end: message`. With `At::Nowhere` it is `[line L:C] Error: message`.
- When `warn` is set, the word `Warning` replaces `Error`.

`render` gives three lines joined by `\n`:

```text
[line 2:7] Error at 'abc': Something is wrong.
   2 | print(abc);
     |       ^^^
```

- The first line is `short`.
- The second is the line number right-aligned in 4 columns, then ` | `, then that source line without its line end (`\n` or `\r\n`).
- The third is 5 spaces, `| `, then C−1 spaces and one `^` for each **character** of the span on that line. There is at least one `^`.
