> **Spoiler before you open the editor.** The starter holds `search_key` and its table. They are the reference answer to **Light up what the user typed**. Solve that first.
>
> You may change the code; keep the public names.

**Capstone, step 1 of 4.** A programming site suggests language and library names while you type. Names that people choose often, and **recently**, come first.

A **term** is known by its **key**, `search_key(text)`. `Café` and `cafe` are one term. A text whose key is empty is no term.

`Terms` has:

- `new()`, `len()` (the number of terms) and `is_empty()` (given).
- `add(batch, now)` takes pairs `(text, count)` and applies them **in order**: each records `count` choices of that term at time `now`. A new term is created even when `count` is 0, and is **shown** as `text.trim()` of its **first** pair. A pair with an empty key is skipped. `Added` says how many pairs created a term (`added`) and how many were for a term already there (`existing`).
- `choose(text, now)` records one choice of an existing term. It returns `false`, and changes nothing, when there is none.
- `suggest(typed, limit, now)` returns the best terms whose key starts with `search_key(typed)`. It returns at most `limit` of them, and never more than `MOST` (20). A `limit` of 0 gives nothing, and an empty `typed` fits every term.

**Popularity fades.** `count` choices made at time `at` are worth `count * 0.5 ^ ((now - at) / HALF_LIFE)` at time `now`. The division is not rounded, and `HALF_LIFE` is 3 600 seconds. A term's `score` is the sum of these.

Times are seconds from 0 to 1 000 000 000. In a question, `now` is never before any recorded choice. Writes may arrive with a time **earlier** than an earlier write's.

**Accuracy.** A score is within one part in 100 000 (relative) of the exact sum. It is never infinite or NaN. For an exact value below `1e-300`, any answer from 0 to `1e-300` will do.

One test records at most 2 000 choices of one term.

**Order.** Higher score first. If two scores differ by more than one part in 100 000, the higher comes first, however small both are.

Terms **tie** when they got the same non-zero counts at the same times, in the same order; terms never chosen tie too. Tied terms are ordered by key. Other closer scores may come in either order.

**Speed is not graded here**, because these tests cannot count your work. Still, one keystroke must not look at every term. **The best players for any beginning of a name** graded this idea against a remote store. Check it yourself with 100 000 terms and an empty `typed`.
