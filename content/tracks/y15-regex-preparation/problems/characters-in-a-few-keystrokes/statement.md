Command-line tools such as `tr` let you write a set of characters briefly: `a-zA-Z0-9_`. Write `CharSet::parse(spec)`, plus `ranges()` and `contains(c)`.

**The notation**

- If the text starts with `^`, the set is every character that the rest does **not** describe. Anywhere else `^` is ordinary. So `^` alone is every character, and the empty text is the empty set.
- An **item** is one character or one escape. Escapes are `\\`, `\-`, `\^`, and `\n`, `\t`, `\r` (newline, tab, carriage return).
- Read from left to right. Take an item. If a `-` comes next **and something follows that dash**, then the item, the dash and the next item are a range, both ends included. Otherwise the item stands alone.
- So a dash at the start or the end is ordinary. In `a-c-e` the set is `a` to `c`, then `-`, then `e`.

**`ranges()`** returns the set in its one standard form: pairs `(low, high)`, sorted, which neither overlap nor touch. Two pairs touch when no `char` lies between them. There is no `char` from U+D800 to U+DFFF, so a pair ending at U+D7FF touches one starting at U+E000.

**Errors.** The first one from the left is reported; positions count characters from 0.

- `UnknownEscape(pos)`: a backslash followed by any other character. `pos` is the backslash.
- `TrailingEscape(pos)`: a backslash is the last character.
- `Reversed(pos)`: a range whose low end is above its high end. `pos` is where the low item starts.

Two sets with the same members must be equal under `==`, however they were written.

Sets can have tens of thousands of ranges, and `contains` is called hundreds of thousands of times.
