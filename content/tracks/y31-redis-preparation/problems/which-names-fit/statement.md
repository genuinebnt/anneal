An operator searches a store with millions of key names: `session:*`, `user:?:cart`, `log-2026-1[0-2]`. Write the test that decides whether one name fits such a pattern.

**`matches(pattern, name)`** is true when the **whole** name fits the whole pattern. Both are bytes.

| In the pattern | Matches |
|---|---|
| `*` | any number of bytes, none included |
| `?` | exactly one byte |
| `[abc]` | one of the listed bytes |
| `[a-z]` | one byte in the range; `[z-a]` means the same |
| `[^...]` | one byte **not** listed |
| `\x` | the byte `x` itself, also inside `[...]` |
| anything else | itself |

Inside brackets:

- `*`, `?` and `[` are ordinary bytes. `^` is special only straight after `[`.
- A `-` that comes first or last is itself. So is an escaped one.
- The first `]` that is not escaped closes the set, even straight after `[`. So `[]` matches nothing, and `[^]` any one byte.
- A range is `x-y`, where the `y` is the byte straight after the `-`. (The tests do not escape the end of a range.)

Nothing is ever an error. A `[` with no closing `]` lists everything up to the end of the pattern. A single `\` at the very end is an ordinary backslash.

The name may be 100000 bytes long and the pattern may hold a thousand `*`. Every answer must come quickly, whatever the pattern.
