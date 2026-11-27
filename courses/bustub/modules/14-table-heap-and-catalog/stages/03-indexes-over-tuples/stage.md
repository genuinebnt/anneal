The B+ tree of module 2c stores fixed-size byte keys, **each at most once**. The rest of the engine thinks in **key tuples** (the indexed columns of a row, arranged under a *key schema*) and in rows that may share a key: two people with one surname, a hundred orders on one day. This stage is the layer between: the `Index` trait that executors use (`insert_entry`, `delete_entry`, `scan_key`, `scan_all`, `scan_from`), and `BPlusTreeIndex`, which stores an **entry** `(key, rid)` for every row. The tree key of an entry is the key tuple's bytes followed by the rid's: the key decides the order, the rid breaks ties, and since no two rows have one rid, no two entries collide, however often a key repeats. That is how PostgreSQL, InnoDB and SQLite build non-unique indexes on trees that hold each key once. Keys compare by **column values** left to right, so that negative numbers sort before positive ones and a composite key sorts by its first column first. The point of the trait is the one you met in 1a: the engine must not know which index it is talking to.

> [!CHECK] Three rows have the key 7, at the rids (0, 4), (2, 1) and (0, 9). In what order does the tree hold their entries, and where does a lookup of 7 start reading? Two integer keys are stored as 4 little-endian bytes each: what does comparing the bytes with `memcmp` do to `-1` and `1`, and why does the comparator need the key schema?
> ||The entries are ordered by key, then rid: (7, (0, 4)), (7, (0, 9)), (7, (2, 1)), next to each other. A lookup of 7 starts at the first entry not less than (7, smallest rid), which is the first of them, and reads on while the key is 7. Bytes compared with `memcmp` put `-1` (`FF FF FF FF`) after `1` and `256` (`00 01 00 00`) before `1`; the comparator reads each column's value out of the key bytes using the key schema (which says where each column is and what type it has) and compares values, first column first.||
>
> - How does a lookup know where the entries of one key end?
> - A row's key is `(NULL, 3)`. Does it have an entry? What does a lookup of `(NULL, 3)` return, and why is that the right answer?
> - A primary key must hold each key once. What must its tree compare, and what not?
> - Why is `scan_key` a `Vec` and not an `Option`?

## The task

In `index.rs`, the contract of each item is in its doc comment:

- `IndexMetadata::new(name, table, tuple_schema, key_attrs, is_primary_key)`: the key schema is the table schema restricted to `key_attrs`.
- `generic_key_from_tuple::<N>(tuple)`: the tuple's bytes at the start of a zeroed `N`-byte key; a tuple longer than `N` is a bug and panics.
- `SchemaComparator::<N>::compare`: column by column, by value; a NULL is smaller than any value and equal to another NULL, so the order is total.
- `EntryComparator::<N>`: entries ordered by their key columns, then by the rid stored after them; in a unique index by the key columns alone.
- `BPlusTreeIndex::<N>`: `new(metadata, bpm)` (panics unless a key and a rid fit in `N` bytes) and the five `Index` methods over the tree.

**An index that is not a primary key holds one entry per row.** Keys repeat, and a key with a NULL in it is an entry like any other, ordered first, so a scan of the whole index lists every row it was given. `insert_entry` refuses only an entry that is already there; `delete_entry(key, rid)` removes that one entry and leaves the key's other rows; `scan_key` returns every rid of the key, in rid order, and nothing for a key with a NULL (`NULL = 5` is not true and neither is `NULL = NULL`). `scan_all` and `scan_from` return the entries themselves, `(key tuple, rid)`, in order.

**A primary-key index is unique**: `insert_entry` refuses a key it already holds (whatever the rid) and a key with a NULL, and `delete_entry` removes a key only together with its own rid.

> [!ASIDE] The steps, if you would rather not work them out
> - An entry's tree key: `generic_key_from_tuple::<N>(key)`, then the rid's page id and slot (4 bytes each, little-endian) right after the key's bytes (the key schema's `inlined_storage_size()`).
> - `EntryComparator::compare`: the `SchemaComparator` order of the two keys; on a tie, and unless the index is unique, the order of the two rids read back from their bytes.
> - `scan_key(key)`: `begin_at` the entry `(key, the smallest rid)` and collect while the entry's key bytes equal the key's; a key with a NULL returns nothing before that.
> - `scan_all` and `scan_from`: map each `(tree key, rid)` of the iterator to `(the first key-size bytes as a tuple, rid)`.

## Your freedom

How an entry is laid out in the `N` bytes (the rid after the key is the suggestion; the tests see only the trait), how the comparator reads the keys (decoding through values, or comparing bytes of order-preserving columns), and whether the scans collect eagerly or lazily (the trait returns `Vec`).

## The Rust toolbox

**A trait object for "some index".** `Box<dyn Index + 'a>` in the catalog (next stage) means executors call `index.scan_key(..)` without knowing the type; the trait needs `Send + Sync` so it can be shared between threads.

**Const generics for the entry size.** `BPlusTreeIndex<'a, const N: usize>` and `GenericKey<N>`: the catalog picks `N` from the key's size plus 8 for the rid and builds the matching type behind the trait object.

**Comparators that wrap comparators.** `EntryComparator` holds a `SchemaComparator` and calls it first: `match by_key { Ordering::Equal => rid_order, other => other }`. `Ordering::then_with` says the same in one call.

**Bytes in, bytes out.** `slice.copy_from_slice(&x.to_le_bytes())` writes a number into a key; `i32::from_le_bytes(slice.try_into().unwrap())` reads it back. `Tuple::from_bytes(rid, &bytes)` turns the key's first bytes back into a key tuple.

**`take_while` for a range.** `tree.begin_at(&start).take_while(|(entry, _)| same_key(entry))` reads exactly one key's entries and stops at the first entry of the next key.

**Reusing 3a.** `Value::compare_less_than` returns a `CmpBool` in a `Result`; handle NULLs before calling it, and for non-NULL integer columns `unwrap()` is fine.

## If this is new

- [L4 Traits & dispatch](/t/l4-traits-dispatch): `dyn Trait`, `Send + Sync` bounds.
- [L5 Generics & associated types](/t/l5-generics): const generics.
- S8 The core traits: `Ord` and comparators.
- The *index entries: a key and a rid* concept.

## Tests

- The metadata's key schema; a key tuple becomes the bytes of a fixed-size key; keys compare column by column as numbers.
- Inserts, scans and deletes; many rows under one key, found in rid order; NULL keys indexed first but never found by a lookup.
- A primary-key index holds each key once and no NULL; scans are in key order for negative and composite keys.
- Property: any operations agree with a sorted set of entries (or, for a primary key, a `BTreeMap`).

## Hints

### Which order is the tree's order?

The tree compares tree keys, so whatever must decide the order has to be inside the key: the columns first, the rid second. A tree that holds each key once becomes an index that holds each row once.

### The comparator and the schema

`key_to_value(&key, &key_schema, i)` reads column `i` of a key. NULLs first: compare `is_null()` before calling `compare_less_than`, which answers neither true nor false for a NULL.

### Where a key's entries start

A lookup cannot ask the tree for "the key 7, any rid". It asks for the first entry not less than "the key 7 with the smallest rid there can be" and reads forward while the key is still 7. If you start at a real rid you skip the entries before it.

### A unique index is the same tree with a different comparator

If the comparator ignores the rid, two entries with one key are equal, and the tree's own refusal of a key it holds is the uniqueness check. Deleting then needs care: the tree finds the key's entry whatever rid you give, so check that it is this row's before removing it.

## Performance

Each comparison decodes up to two values per column plus two rids: a few ns for integers. A lookup of a key with `m` rows is one descent (`O(log n)` comparisons) plus `m` entries read in order, usually from one or two leaves. Storing the rid makes every entry 8 bytes longer, so a leaf holds fewer of them: for a one-column key, a 16-byte entry and its 8-byte value instead of 4 and 8.

**Measure it.** Insert a million entries over 100 keys and over a million keys, and time `scan_key` for one key in each: the first reads 10 000 entries per lookup, the second one.

## Experiment

Optional. Predict first, then run.

1. **Drop the tie-breaker.** Make the comparator ignore the rid in every index. Which test fails first, and what does the index do with the second row of a key?
2. **Order-preserving keys.** Encode each integer as `(x as u32 ^ 0x8000_0000).to_be_bytes()` and the rid big-endian too, and compare with `memcmp`. Does the property still hold? What did you gain?

## Other designs

- **The rid inside the key (ours; PostgreSQL since version 12, SQLite's index records).** One tree, one rule, entries unique by construction.
- **A list of rids per key** (posting lists, PostgreSQL's deduplicated B-tree tuples): smaller for keys with many rows, but deleting one row edits a list.
- **The primary key instead of a rid** (InnoDB's secondary indexes): rows can move without touching the index, and a lookup goes through the primary-key tree.
- **A hash index:** point lookups only; `scan_from` unsupported.

## In BusTub

`TableHeap`, `TableIterator`, `Catalog`, `IndexInfo` and `TableInfo` are the pieces the execution engine of Project 3 uses: executors never touch pages, only the catalog and the table heap.

BusTub's `BPlusTreeIndex` maps each key to one rid, and its executors ignore an insert the tree refuses, so a second row with a key is silently missing from the index. This course stores an entry per row instead, as the systems in *Other designs* do.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `class Index { virtual auto InsertEntry(...) = 0; }` | `trait Index: Send + Sync { fn insert_entry(..); }` |
| `std::unique_ptr<Index>` | `Box<dyn Index>` |
| `GenericComparator<N>(key_schema)` | `SchemaComparator<N>` implementing `KeyComparator` |
| `std::multimap`-style duplicate keys (BusTub's B+ tree has none) | `EntryComparator<N>`: the rid breaks ties, every entry unique |
| `template <size_t KeySize>` | `const N: usize` |

**Port rule:** an abstract base class with pure virtual methods becomes a trait; a `unique_ptr` to it becomes a `Box<dyn Trait>`.

## Learn more

- [Trait objects](https://doc.rust-lang.org/book/ch18-02-trait-objects.html) · [`Ordering`](https://doc.rust-lang.org/std/cmp/enum.Ordering.html)
