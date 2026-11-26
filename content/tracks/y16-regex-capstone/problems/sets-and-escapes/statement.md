> **Spoiler before you open the editor.** The starter of this step contains reference code for step 1. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves that step and much of the preparation before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 2 of 7.** `parse` now understands `\` and `[...]`. Everything from step 1 keeps working. Each of the new forms becomes one `Ast::Class`. The tests ask it which characters it `matches`, so the order and shape of `ranges` are yours.

**Escapes**

| Written | Means |
|---|---|
| `\d` | a digit `0`-`9` |
| `\w` | an ASCII letter, a digit or `_` |
| `\s` | space, tab, newline, carriage return, vertical tab (U+000B) or form feed (U+000C) |
| `\n`, `\t`, `\r` | newline, tab, carriage return |
| `\` before any other ASCII letter or digit | the error `BadEscape(pos)` |
| `\` before any other character | that character itself: `\.` `\*` `\[` `\\` |

A backslash as the last character is `TrailingEscape(pos)`. Both positions are the backslash's.

Outside brackets a `]` on its own is still ordinary.

**Sets.** `[...]` matches any one of the characters it describes, and `[^...]` any one character it does not describe (newlines included).

- The first `]` where an item is expected closes the set. So `[]` matches nothing, and `[^]` matches any character.
- An item is one character or one escape from the table, with the same meanings and errors. Every other character is ordinary, `[` included. `^` is ordinary except right after the opening `[`.
- If an item is followed by `-`, and the `-` is followed by something other than `]`, then the two items around it form a range, both ends included. Otherwise a `-` is an ordinary item.
- `BadRange(pos)`: a range whose low end is above its high end, or with `\d`, `\w` or `\s` as either end. `pos` is where the low item starts.
- `UnclosedClass(pos)`: the pattern ends inside a set. `pos` is its `[`.

As before, the first problem from the left is reported.
