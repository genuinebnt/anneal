**Capstone, step 1 of 7.** Over seven steps you build a regular-expression engine whose matching time cannot blow up, and then a way to catch its mistakes. This step turns pattern text into a tree.

Write `parse(pattern)`.

| Pattern | Tree |
|---|---|
| a character with no special meaning, `c` | `Class(Class::literal(c))` |
| `.` | `Any` |
| `^`, `$` | `Start`, `End` |
| `x*`, `x+`, `x?` | `Star`, `Plus`, `Opt` of `x` |
| `xyz` (two or more items) | `Cat([x, y, z])` |
| `x\|y\|z` (two or more branches) | `Alt([x, y, z])` |
| `(x)` | the tree of `x`; brackets add no node |
| nothing: the empty pattern, `()`, an empty branch | `Empty` |

- `*`, `+` and `?` bind tightest and apply to the one item before them. Next comes writing items side by side. `|` binds loosest.
- A list of one is never wrapped. A group stays one item: `(ab)c` is `Cat([Cat([a, b]), c])`, and `(a|b)|c` is `Alt([Alt([a, b]), c])`.
- Quantifiers may be stacked, `a+?` meaning `Opt(Plus(a))`. They may follow any item, anchors and groups included.
- Only `. ^ $ * + ? | ( ) [ \` are special. Every other character is ordinary, `]`, `{` and `-` included.
- `[` and `\` get a meaning in step 2. This step's tests do not use them.

**Errors** carry a position, counted in characters from 0. The pattern is read from left to right, and the first problem found is reported. The step-2 variants of `ParseError` are already declared.

- `NothingToRepeat(pos)`: a `*`, `+` or `?` with no item before it (at the start, after `(` or after `|`).
- `UnmatchedClose(pos)`: a `)` with no `(`.
- `UnclosedGroup(pos)`: the pattern ends inside a group. `pos` is the `(` of the innermost group still open.
- `TooDeep(pos)`: more than `MAX_OPEN_GROUPS` (100) groups open at once, or more than `MAX_STACKED` (3) quantifiers in a row. `pos` is the `(` or the quantifier that goes over. Patterns can come from strangers, and this must never crash.

A pattern may be hundreds of thousands of characters long, with as many items or branches.
