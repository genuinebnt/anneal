A lab tool must know that `J / kg K` and `J / (K kg)` are the same unit. Write `parse(text)`. It returns each unit name with its power, leaving out names whose power is 0.

**Tokens.** A name is a longest run of ASCII letters. The other tokens are `1`, `/`, `.`, `^`, `(`, `)` and a power number. Spaces separate tokens and are otherwise ignored.

**Meaning**, from the tightest binding to the loosest:

1. A **factor** is a name, the digit `1` (no unit at all) or a bracketed expression. It may be followed by **one** `^` and a power number, which multiplies every power inside it.
2. A **product** is factors written side by side or with `.` between them: `N m` and `N.m` are the same. `kg(m)` is a product too.
3. A **ratio** is products with `/` between them, read from the left. `/` subtracts the powers of the product on its right. So `J / kg K` is J divided by (kg times K), and `a / b / c` is `(a / b) / c`.

A **power number** is digits with an optional `-` straight before them: `2`, `-1`, `0`. It may only follow `^`. Spaces may come between `^` and the number.

A run of digits is one token, read by its value. So `01` is the factor `1`, and `^02` is the power 2.

**Errors** carry a position in characters from 0.

- `BadChar(pos)`: a character that begins no token. A `-` not followed by a digit is one. The whole text is split into tokens first, so this error comes before any other.
- `Unexpected(pos)`: a token that cannot be there. This covers a number other than `1` where a factor is needed, a missing power after `^` (at the token found instead), a second `^`, and anything left over.
- `UnexpectedEnd`: the text ends where more is needed. Empty text is this.
- `TooDeep(pos)`: more than `MAX_OPEN` (40) brackets are open at once. `pos` is the `(` that goes over.
- `TooBig`: a power, or a written power number, does not fit in an `i32` at any step. A written number that does not fit is found while splitting into tokens, in text order with `BadChar`.

After splitting, the first problem met reading from the left is reported. A power overflows at the step that combines it.

**Size.** A text may have hundreds of thousands of tokens, and no text may crash your function.
