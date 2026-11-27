This is capstone step 1 of a **query planner**. A database must decide how to run a query before it runs it. To do that it guesses how many rows each part will produce. It guesses from a small summary of every column.

`summarise(values)` takes one column. `None` is a missing value. The summary holds:

- `rows`: all rows. `missing`: how many are `None`. `present()` is the difference.
- `distinct`: how many different values are present.
- `cuts`: let `s` be the present values in sorted order, `p` of them. Then `cuts[i] = s[i * (p - 1) / 32]` for `i` from 0 to 32 (whole-number division). With no present value, `cuts` is empty.
- `common`: take the values by count, highest first, equal counts by smaller value. Keep the first 8. Of those, keep only values whose count is **more than** 1.25 times the average count `p / distinct`.

**`equal_to(v)`** estimates the rows whose value is `v`:

1. 0 if there are no cuts, or `v` is below the first cut or above the last.
2. Else, if `v` is in `common`, its count.
3. Else the rows not counted in `common`, divided by the number of values not listed there. If every value is listed, 0.

**`between(low, high)`** estimates the rows with a value in `low..=high`. It is 0 if `low > high`, and `equal_to(low)` if they are equal. Otherwise it is `at_most(high) - at_most(low - 1)`, never below 0. Nothing is at most "one less than `i64::MIN`".

**`at_most(v)`** (your own helper) is:

- 0 if there are no cuts or `v` is below the first cut; `present` if `v` is at or above the last.
- Otherwise find the last slice `i` (0 to 31) with `cuts[i] <= v`. Let `a = cuts[i]` and `b = cuts[i + 1]`. The answer is `(i + (v - a + 1) / (b - a + 1)) / 32 * present`, with real-number division.

Answers must match these formulas to within 0.000001. Any `i64` can be a value. A column can have 200 000 rows.
