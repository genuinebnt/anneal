Build a regular-expression engine in the style of RE2 and Rust's `regex` crate (no backtracking). It works on `char`s.

Syntax: any character matches itself, `.` matches any one character, `*` zero or more, `+` one or more, `?` zero or one
(all postfix, applying to the single atom before them), `|` alternation, `( )` grouping, and `\` makes the next character
literal (`\.`, `\*`, `\\`). Precedence from loosest: `|`, then concatenation, then the postfix operators. The empty pattern,
an empty group `()` and an empty alternative (`a|`) are all legal and match the empty string.

`Regex::new` returns a `ParseError`: `UnbalancedParen` for a `(` with no `)` or a `)` with no `(`; `NothingToRepeat`
for a quantifier with nothing before it (at the start, after `(` or `|`) or directly after another quantifier (`a**`);
`DanglingEscape` for a trailing `\`.

`is_match` is true when the pattern matches the **entire** text. `find` returns the **leftmost** match and, among matches
starting there, the **longest**, as `(start, end)` in `char` indices (`end` exclusive). It may be an empty match.
Matching must take time proportional to `text length x pattern length`, however nasty the pattern.
