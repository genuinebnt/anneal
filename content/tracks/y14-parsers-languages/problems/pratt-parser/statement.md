Write `parse(source)`: parse one expression and return it as an **S-expression** that shows exactly how it was grouped.

**Atoms** are unsigned integers (`42`) and identifiers (`[A-Za-z_][A-Za-z0-9_]*`), printed as written. Spaces, tabs and
newlines separate tokens and are otherwise ignored.

**Operators**, loosest first:

| Operators | Kind |
|---|---|
| `=` | right associative. The left side must be an identifier or an index (`a[0]`) |
| `\|\|` | left |
| `&&` | left |
| `==` `!=` `<` `<=` `>` `>=` | **non-associative**: `a < b < c` is an error, `(a < b) < c` is fine |
| `+` `-` | left |
| `*` `/` `%` | left |
| prefix `-` `!` | |
| `^` | right. Binds tighter than a prefix operator on its left: `-2^2` is `-(2^2)`. Its right side may start with one: `2^-1` |
| postfix call `f(a, b)` and index `a[i]` | tightest, and they chain: `f(1)[2](3)` |

**Output.** A binary operator is `(op left right)`, a prefix one `(op operand)`, a call `(call callee arg ...)`, an
index `(index base i)`. Parentheses in the source only group; they leave no mark. So `a + f(b, 2) * -c` becomes
`(+ a (* (call f b 2) (- c)))`. A call with no arguments is `(call f)`. Trailing commas are not allowed.

**Errors** carry the byte offset of the offending token:

- `UnexpectedChar(pos)`: a character that starts no token (a lone `&` or `|` included).
- `UnexpectedToken(pos)`: a token that cannot appear there, including anything left over after the expression.
- `UnexpectedEnd(len)`: the input stops too early (empty input too); the position is the input's length.
- `ChainedComparison(pos)`: at the second comparison operator.
- `BadAssignTarget(pos)`: at the `=`, when its left side is not an identifier or an index.
- `TooDeep(pos)`: nesting deeper than the parser allows. **200** levels of parentheses must parse; 100 000 must give
  `TooDeep` and must not overflow the stack.

The whole input is split into tokens first, so an `UnexpectedChar` is reported before any other error.
