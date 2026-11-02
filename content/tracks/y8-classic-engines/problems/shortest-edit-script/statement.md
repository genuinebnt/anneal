Implement `diff` for sequences of lines: return a **shortest edit script** turning `a` into `b`, as a list of `Op`s
that walks both sequences from the start. `Keep(i, j)` is a line present in both (`a[i] == b[j]`), `Delete(i)` drops `a[i]`,
`Insert(j)` adds `b[j]`. The number of `Delete`s plus `Insert`s must be the smallest possible, which is
`a.len() + b.len() - 2 * LCS(a, b)`.

When deleting and inserting are equally good at some point, **delete first** (so a replaced line shows as a `Delete`
followed by an `Insert`, as in `git diff`). Inputs can be thousands of lines: an `O(a.len() * b.len())` table is fine.
