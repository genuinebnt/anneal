A challenge: no walkthrough, no hints, no solution. It adds to something you built in this module, using what you learned there. It is extra practice and does not count towards the course.

## What to build

`check_integrity` in `src/primer/skiplist_extras.rs`: given **your** `SkipList<i32>`, verify its structural invariants through its public view (`nodes()` for the keys with their heights, `level(l)` for the keys linked at level `l`) and return the first violation found. Then a test drives your list with thousands of random inserts and erases and runs the checker after every one.

## Why

The point of an invariant checker is that it makes every later bug local: a skip list that loses a key at level 3 passes small hand-written tests and fails once in ten thousand operations. A checker that runs after each operation turns that failure into the exact operation that caused it. Writing it also forces you to say what "a correct skip list" means.

## The contract

- Level 0 holds every key, strictly increasing, and `size()` equals its length.
- For every level `l >= 1`, `level(l)` is strictly increasing and a **subsequence** of `level(l - 1)`.
- The height of each node in `nodes()` equals the number of levels in which its key appears (and is at least 1).
- `check_integrity(list) -> Result<(), String>`; the message names the level and the key.

## Invariants

These must hold after every step, whatever the input:

- A correctly implemented list always passes after any sequence of operations.
- The checker reads only; it never changes the list.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Removing a key from the middle of one level of a good list makes the checker fail (tested on a corrupted copy of the data).
- The checker agrees with `nodes()` and `level()` on an empty list.
- Passing after every step of a random workload means every intermediate state was valid.

## Examples

Worked cases (the tests include them):

```text
list {1, 2, 3} with heights {1, 3, 2}: level 1 = [2, 3]; level 2 = [2]
```

## What the tests check

- Small lists; the empty list.
- Corrupted level data is rejected (the checker works on the views, so the tests feed it constructed views).
- A random workload against your list.

## Done when

All the `s0b_c2` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Skip list](/cookbook/p/skiplist): the structure behind the invariants: towers, sorted lanes and a full level 0.
