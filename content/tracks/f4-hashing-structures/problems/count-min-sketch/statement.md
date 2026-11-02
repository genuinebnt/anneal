Implement `CountMin`: `depth` rows of `width` counters. `add(item, count)` adds `count` to one counter in every row (the column
is `hash(row_seed, item) mod width`, so each row collides differently). `estimate(item)` is the **minimum** of the item's counters.

`with_error(eps, delta)` picks `width = ceil(e / eps)` and `depth = ceil(ln(1 / delta))`. `total()` is the sum of all adds.
`merge` adds another sketch of the same shape (`Err(())` otherwise). Counters saturate instead of overflowing.

Guarantees the tests check: an estimate is **never below** the true count, and for a skewed stream with a few heavy keys, at least 99% of light
keys are within `eps * total` of the truth.
