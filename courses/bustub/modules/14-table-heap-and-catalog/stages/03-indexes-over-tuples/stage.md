The B+ tree of module 2c stores fixed-size byte keys. The rest of the engine thinks in **key tuples**: the indexed columns of a row, arranged under a *key schema*. This stage is the layer between: the `Index` trait that executors use (`insert_entry`, `delete_entry`, `scan_key`, `scan_all`, `scan_from`), and `BPlusTreeIndex`, which turns a key tuple into the bytes of a `GenericKey<N>` and orders keys by comparing their **column values** left to right, so that negative numbers sort before positive ones and a composite key sorts by its first column first. The point of the trait is the one you met in 1a: the engine must not know which index it is talking to.

> [!CHECK] Two integer keys are stored as 4 little-endian bytes each. Comparing the bytes with `memcmp` puts `-1` after `1` and `256` before `1`. What does your comparator do instead, and why does it need the key schema? What would change if the key were `(INTEGER, VARCHAR)`?
> ||The comparator reads each column's value out of the key bytes using the key schema (which says where each column is and what type) and compares values with the SQL comparison from module 3a, first column first, moving to the next on a tie. It needs the schema because the bytes alone do not say whether four bytes are an integer, part of a string or a timestamp. For `(INTEGER, VARCHAR)` the key would not be fixed-size, so the byte key would not work: BusTub supports only integer keys of a few columns; other designs store a prefix or a separate variable-length key.||
>
> - A row's key is `(NULL, 3)`. Is it in the index? What does a lookup of `(NULL, 3)` return, and why is that the right answer?
> - Where does the comparator get the column offsets?
> - What does a tie on the first column mean for ordering?
> - Why is `scan_key` a `Vec` and not an `Option`?

## The task

In `index.rs`: `IndexMetadata::new(name, table, tuple_schema, key_attrs, is_primary_key)` (the key schema is the table schema restricted to `key_attrs` by `copy_schema`), `generic_key_from_tuple::<N>(tuple)` (a zeroed key with the tuple's bytes at the start; a tuple longer than `N` is a bug and panics), `SchemaComparator::<N>::compare` (column by column, by value; a NULL is smaller than any value and equal to another NULL, so the order stays total), and `BPlusTreeIndex::<N>`: `new(metadata, bpm)` (a tree with the default node sizes on a freshly allocated header page) and the five `Index` methods over the tree.

**A key with a NULL in it is never indexed.** `NULL = 5` is not true and neither is `NULL = NULL`, so no lookup could ever ask for such a row by key: `insert_entry` returns `false` for it, `delete_entry` ignores it, and `scan_key` finds nothing. (A scan of the whole index therefore does not list rows whose key is NULL; a table with NULLs in the indexed column must not be read in key order through its index.)

The tests: the metadata's key schema; a key tuple becomes fixed-size bytes; keys compare column by column as numbers (negative and composite keys); an index inserts (refusing a repeated key), scans and deletes (a missing key is no error); a key with a NULL in any column is not indexed and finds nothing; and a property: any inserts, deletes and scans on a two-column index (some keys with a NULL) agree with a `BTreeMap` from the key to its rid (`scan_key`, `scan_all` and `scan_from`).

## Your freedom

How the comparator reads the keys (decoding through values, or comparing bytes of order-preserving columns), and whether `scan_from` and `scan_all` collect eagerly or lazily (the trait returns `Vec`).

## The Rust toolbox

**A trait object for "some index".** `Box<dyn Index + 'a>` in the catalog (next stage) means executors call `index.scan_key(..)` without knowing the type; the trait needs `Send + Sync` so it can be shared between threads.

**Const generics for the key size.** `BPlusTreeIndex<'a, const N: usize>` and `GenericKey<N>`: the catalog picks `N` (4, 8, 16, 32 or 64) from the key's size and builds the matching type behind the trait object.

**Implementing a comparator trait.** `impl<const N: usize> KeyComparator<GenericKey<N>> for SchemaComparator<N> { fn compare(&self, a, b) -> Ordering }`: return `Ordering::Less`/`Greater` at the first differing column.

**`Arc<Schema>` for sharing.** The comparator keeps a clone of the key schema's `Arc`; cloning an `Arc` is a counter increment.

**Reusing 3a.** `Value::compare_less_than` returns a `CmpBool` in a `Result`; for non-NULL integer columns `unwrap()` is fine, and a NULL key is a design question the tests do not ask (the catalog only indexes integer columns).

## If this is new

- [L4 Traits & dispatch](/t/l4-traits-dispatch): `dyn Trait`, `Send + Sync` bounds.
- [L5 Generics & associated types](/t/l5-generics): const generics.
- S8 The core traits: `Ord` and comparators.
- Y5 Testing & verification: Understand it: a heap checked against a `Vec`; the Halloween problem as a property.

## Tests

- The metadata's key schema; a key tuple becomes the bytes of a fixed-size key; an oversized key panics.
- Keys compare column by column as numbers.
- An index inserts, scans and deletes; scans are in key order for negative and composite keys; a key with a NULL is not indexed.
- Property: any operations agree with a `BTreeMap`.

## Hints

### The comparator and the schema

`key_to_value(&key, &key_schema, i)` reads column `i` of a key; compare two such values; return at the first difference.

### Which tree does the index own?

The index owns its tree and the tree owns nothing but pages: dropping the index does not free the pages (they live in the buffer pool).

## Performance

Each comparison decodes two values: a few ns for integers. A B+ tree lookup does about 10; a faster index compares the bytes directly by storing integers in an order-preserving encoding (flip the sign bit, big-endian), which turns `compare` into `memcmp`.

**Measure it.** Look up a million keys through `scan_key` and compute the share of time in `compare` with a profiler or a comparison counter.

## Experiment

Optional. Predict first, then run.

1. **Order-preserving keys.** Encode each integer as `(x as u32 ^ 0x8000_0000).to_be_bytes()` and compare with `memcmp`. Does the property still hold? What did you gain?
2. **A second index type.** Implement `Index` for a `BTreeMap` behind a `Mutex`. Do the executors of module 3e work with either?

## Other designs

- **A B+ tree behind a trait object (ours, BusTub's).**
- **An in-memory ordered map** (BusTub's STL indexes): the same trait, no pages.
- **A hash index:** point lookups only; `scan_from` unsupported.
- **A covering index** (the key holds extra columns): fewer heap reads.

## In BusTub

`TableHeap`, `TableIterator`, `Catalog`, `IndexInfo` and `TableInfo` are the pieces the execution engine of Project 3 uses: executors never touch pages, only the catalog and the table heap.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `class Index { virtual auto InsertEntry(...) = 0; }` | `trait Index: Send + Sync { fn insert_entry(..); }` |
| `std::unique_ptr<Index>` | `Box<dyn Index>` |
| `GenericComparator<N>(key_schema)` | `SchemaComparator<N>` implementing `KeyComparator` |
| `template <size_t KeySize>` | `const N: usize` |

**Port rule:** an abstract base class with pure virtual methods becomes a trait; a `unique_ptr` to it becomes a `Box<dyn Trait>`.

## Learn more

- [Trait objects](https://doc.rust-lang.org/book/ch18-02-trait-objects.html) · [`Ordering`](https://doc.rust-lang.org/std/cmp/enum.Ordering.html)
