`PackedVec::new(width)` stores unsigned integers that each fit in `width` bits (1 to 64) back to back in
`Vec<u64>`, so a column of values below 1000 takes 10 bits each instead of 64. `push`, `get` (`None` past the end) and
`set` work on any index, **including values that straddle a word boundary**. `push` and `set` refuse a value that does
not fit (`TooWide`); `set` past the end is `OutOfRange`. `words_used()` reports the storage, which must be
exactly `ceil(len * width / 64)`. `min_width` finds the smallest width that holds a slice of values.
