A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`NameCatalog` in `src/catalog/name_catalog.rs`: the names half of a catalog. `create(name)` returns a new table id (`u32`), `lookup`, `rename` and `drop` work by name, and names are **case-insensitive** (ASCII). Ids are handed out in increasing order and are **never reused**, even after a drop.

## Why

Everything else in the engine refers to a table by id, so an id must mean one table for ever: reusing one would make an old plan or a cached handle silently point at a different table. Names are what users type, and `Users`, `USERS` and `users` must be the same table.

## The contract

- `create(name)` is `Err(Exists)` if a table with that name (any case) exists, `Err(Invalid)` for an empty name; otherwise the next id, starting at 1.
- `lookup(name)` is the id; `drop(name)` removes it (false if absent); `rename(old, new)` moves the name, keeping the id (`Err` if `old` is missing or `new` exists).
- `names()` lists the names in lowercase, sorted.

## Invariants

These must hold after every step, whatever the input:

- Every id is handed out at most once, ever.
- Two live tables never share a name (case-insensitively).

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `lookup(drop-and-create(n))` has a larger id than before.
- `rename(a, b)` then `lookup(b)` equals the id `lookup(a)` had.
- `create(n)` then `drop(n)` leaves `names()` as it was.

## Examples

Worked cases (the tests include them):

```text
create Users -> 1; create USERS -> Exists; lookup users -> 1; drop; create users -> 2
```

## What the tests check

- Case-insensitivity, uniqueness, and ids that stay unique.
- Rename and drop.
- A property against a model.

## Done when

All the `s3c_c2` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Catalog, types, expressions](/cookbook/p/catalog-types-expressions): a catalog with case-insensitive table and column lookup.
- [Slab generational](/cookbook/p/slab-generational): why reusing an id after removal lets old references hit a new object.
