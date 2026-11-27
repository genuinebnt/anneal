A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`src/storage/table/var_row.rs` stores a row of variable-length byte columns as an array of end offsets followed by the data, and `column(bytes, i)` slices out column `i`. It returns the wrong bytes for some columns. Find the bug and fix it.

## Why

Offset arrays are how every variable-length format finds a field in constant time, and a slip in them does not crash: it shows as one column holding the tail of another.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||Column `i` started at its own end offset instead of at the previous column's end (0 for the first column), so it sliced the wrong range.||

## The contract

- `encode(columns)`: a count byte, then `n` little-endian `u16` end offsets (cumulative, relative to the start of the data), then the concatenated data.
- `column(bytes, i)` is the bytes of column `i`, or `None` if `i` is out of range or the bytes are malformed.

## Invariants

These must hold after every step, whatever the input:

- Column `i` starts at the end of column `i - 1` (0 for the first) and ends at its own end offset.
- The columns are contiguous and cover the data exactly.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Concatenating `column(bytes, 0..n)` gives the data section.
- `column(encode(cols), i) == cols[i]` for every `i`.
- The lengths of the columns add up to the data length.

## Examples

Worked cases (the tests include them):

```text
["ab", "", "cde"] -> ends [2, 2, 5]; column 0 = "ab", 1 = "", 2 = "cde"
```

## What the tests check

- Each column of a small row, including empty columns.
- Out-of-range index.
- A property over random rows.

## Done when

All the `s3b_c3` tests pass, and you can say in one sentence what the bug was.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Pages and tuples](/cookbook/p/pages-and-tuples): variable-length columns found through (offset, length) slots in a tuple.
- [The document store](/cookbook/p/the-document-store): binary documents with offset tables, so a field is found without parsing.
