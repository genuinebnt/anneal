A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`src/storage/page/shift_array.rs` inserts into and removes from the middle of a fixed array by shifting the entries after the position. It looks right, and after an insert in the middle several slots hold the same value. Find the bug and fix it.

## Why

Every sorted page does this shift: a leaf, a bucket, a slot directory. A shift that goes wrong does not crash; it copies one entry over its neighbours, and the page still looks well formed.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||The loop copied from the front: `arr[i + 1] = arr[i]` for rising `i` overwrites each element before it is moved, smearing `arr[at]` over the rest. An overlapping copy needs `memmove` semantics: `copy_within`, or a loop that runs from the back.||

## The contract

- `insert_at(arr, len, at, value)` puts `value` at index `at`, shifting `arr[at..len]` one place right; requires `len < arr.len()` and `at <= len`; returns the new length.
- `remove_at(arr, len, at)` removes `arr[at]`, shifting the later entries one place left, and returns the new length.

## Invariants

These must hold after every step, whatever the input:

- After `insert_at`, `arr[..len + 1]` is the old `arr[..len]` with `value` inserted at `at`.
- After `remove_at`, `arr[..len - 1]` is the old `arr[..len]` without index `at`.
- Entries beyond the length are not read.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `remove_at(insert_at(x))` restores the original prefix.
- Inserting at the end and at the front both work.
- The result equals `Vec::insert` / `Vec::remove` on the same prefix.

## Examples

Worked cases (the tests include them):

```text
[1,2,3,_], len 3: insert_at(1, 9) -> [1,9,2,3], len 4
[1,9,2,3], len 4: remove_at(1) -> [1,2,3], len 3
```

## What the tests check

- Insert at the front, middle and end.
- Remove from the front, middle and end.
- A property against `Vec::insert` and `Vec::remove`.

## Done when

All the `s2a_c5` tests pass, and you can say in one sentence what the bug was.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Slice look-up page](/cookbook/p/std-slice-search-sort-binary): copy_within for an overlapping move inside one buffer.
- [B+Tree pages](/cookbook/p/b-tree-pages): copy_within shifting keys and values to insert into a leaf page.
