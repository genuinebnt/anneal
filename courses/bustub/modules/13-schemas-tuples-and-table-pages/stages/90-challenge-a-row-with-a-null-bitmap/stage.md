A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`encode_row` and `decode_row` in `src/storage/table/null_row.rs`: a row of `Option<Cell>` (each cell an `Int(i64)` or a `Text(String)`) as bytes. A **null bitmap** (one bit per column, rounded up to whole bytes) comes first; then, for each column that is **not** NULL, its value: 8 little-endian bytes for an `Int`, a 4-byte little-endian length and the UTF-8 bytes for a `Text`.

## Why

A row with many NULLs should cost almost nothing, and that is how every serious row format works (PostgreSQL's heap tuples, SQLite records, InnoDB's compact format). The decoder is the delicate half: it reads a length from disk and must not trust it.

## The contract

- `encode_row(row)` writes the bitmap (bit `i` set = column `i` is NULL, least significant bit first) then the non-NULL values in column order.
- `decode_row(bytes, types)` takes the schema (`Ty::Int` or `Ty::Text` per column) and returns the row, or `None` if the bytes are not exactly one row of that schema (truncated, a text that is not UTF-8, trailing bytes, a NULL in a column the schema says is present... any inconsistency).

## Invariants

These must hold after every step, whatever the input:

- `encoded_len == ceil(columns / 8) + sum(8 for each non-NULL Int, 4 + len for each non-NULL Text)`.
- `decode_row(encode_row(r), types_of(r)) == Some(r)`.
- `decode_row` never panics on any bytes.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Setting a column to NULL never makes the row longer.
- Two rows that differ only in a NULL column's former value encode alike.
- Any strict prefix of an encoded row is rejected.

## Examples

Worked cases (the tests include them):

```text
[Int(1), NULL, Text("ab")] -> [0b010, 1,0,0,0,0,0,0,0, 2,0,0,0, a, b]
[NULL, NULL] -> [0b11]
```

## What the tests check

- Exact bytes for a mixed row.
- Round trips; the size formula; many columns (bitmaps over 8 columns).
- Malformed input: truncated, bad UTF-8, trailing bytes.
- A property over random bytes.

## Done when

All the `s3b_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Pages and tuples](/cookbook/p/pages-and-tuples): tuple layout with a null bitmap, a fixed region and variable-length slots.
