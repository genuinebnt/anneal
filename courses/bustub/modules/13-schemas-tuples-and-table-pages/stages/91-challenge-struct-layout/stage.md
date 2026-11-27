A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`layout` and `packed_order` in `src/storage/table/struct_layout.rs`: given fields as `(size, align)` pairs (alignment a power of two, size a multiple of it), `layout` places them **in the given order** as C does (each at the next multiple of its alignment) and reports the offsets, the total size (rounded up to the largest alignment) and that alignment. `packed_order` returns the field order that gives the **smallest** total size.

## Why

A header laid out carelessly wastes bytes in every one of millions of records, and a `#[repr(C)]` struct's size is exactly this computation. Rust's own `repr(Rust)` reorders for you; on-disk formats must not, so you do it deliberately and write the rule down.

## The contract

- `layout(fields)` returns `Layout { offsets, size, align }`: `offsets[i]` is the lowest multiple of `align_i` at or after the end of the previous field; `align` is the maximum field alignment (1 for no fields); `size` is the end of the last field rounded up to `align`.
- `packed_order(fields)` returns a permutation of the indexes whose `layout` has the smallest `size` (ties: the lowest indexes first, stable).

## Invariants

These must hold after every step, whatever the input:

- Every offset is a multiple of its field's alignment.
- Fields do not overlap and stay in the given order.
- `size` is a multiple of `align`.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Sorting by descending alignment never gives a larger size than any other order (when each size is a multiple of its alignment).
- `size >= sum of field sizes`.
- Reordering fields never changes `align` or the sum of field sizes.

## Examples

Worked cases (the tests include them):

```text
(1,1),(8,8),(2,2) -> offsets [0,8,16], size 24
packed order of the same -> [1,2,0], size 16
```

## What the tests check

- Padding, trailing padding and an empty struct.
- The best order against all permutations for up to six fields.
- Properties.

## Done when

All the `s3b_c2` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Struct layout](/cookbook/p/struct-packing): padding, field order and repr(C) layout, with compile-time size checks.
- [Memory layout](/cookbook/p/data-layout): struct size and padding, and how reordering fields shrinks a struct.
