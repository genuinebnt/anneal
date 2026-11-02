**Where this fits.** The toolkit assembled on a real page, before the hash table and the B+ tree use it.

> [!CHECK] A bucket page keeps `(key, rid)` entries sorted. A caller asks to insert a key that is already present. Where does `lower_bound` point, and what should the page do? Is the answer the same for a hash table (unique keys) and a non-unique index?
> ||`lower_bound` points at the existing entry (the first not less than the key). A unique index sees the equal key there and refuses the insert; a non-unique index inserts *before* the equal entries (or after, by convention) and keeps duplicates together. The search is the same; what the page does with "equal" is policy.||
>
> - What does `lower_bound` return for a key that is present twice?
> - Which line of the insert code decides uniqueness?

## The task

Nothing new to write. The property test builds, with only what you implemented in this module, a page that is a bucket page in all but name: a `u32` length at offset 0, a `u32` capacity at offset 4, then `(GenericKey<8>, Rid)` entries from offset 8, **kept sorted** with `lower_bound`, `insert_at` and `remove_at`, inside a write guard from the buffer pool. After any sequence of inserts and removes, and after the page has been **pushed out of a two-frame pool** and read back, it must hold exactly the keys of a `BTreeMap` model, in order, with their rids. A second test checks that your comparator is a total order (antisymmetric, transitive).

## Your freedom

None new: the test is written with your pieces.

## The Rust toolbox

**Reading and writing the header by hand.** `u32::from_le_bytes(data[..4].try_into().unwrap())` and `data[..4].copy_from_slice(&n.to_le_bytes())`: the length lives in the page, not in a Rust struct, which is why the bytes survive eviction.

**Borrowing a sub-slice for the array.** `PageArray::new(&mut data[8..])` borrows the page bytes after the header; the header and the array are disjoint ranges, but the borrow checker needs you to finish with one before using the other. Read the length first, then build the array.

**A model that is a standard collection.** `BTreeMap<i64, u32>` is the model: ordered, unique keys. Property tests against a standard collection are the fastest way to trust a page.

## If this is new

- Everything is in the earlier stages of this module.
- Y5 Testing & verification: Understand it: round-trip properties and a `Vec` model.

## Tests

- A sorted bucket built from the pieces survives the buffer pool (a property over random insert/remove sequences with the page evicted in between).
- The key comparator is a total order.

## Hints

### A failure after eviction only

If everything works until the page is pushed out and fails after, the page depends on something that is not in its bytes: a length kept in a Rust variable, a stale view. Everything must live in the 8 KiB.

### The shrunk case is long

Remove operations from the failing list one at a time and see whether it still fails. The property test already does this; read the final list.

## Performance

A bucket of 511 entries is searched in 9 probes and shifted by at most 8 KiB. Compare with a linear unsorted bucket: insert is `O(1)` but lookup scans every entry. The hash table (next module) chooses the unsorted layout, the B+ tree the sorted one.

## Experiment

Optional. Predict first, then run.

1. **Capacity edge.** Fill the bucket to `array_size` entries and insert one more. What does your page do, and where should that decision live (the page, or the hash table above it)?
2. **Duplicate keys.** Allow duplicates and see which of the three jobs of `lower_bound` becomes ambiguous.

## Other designs

None for this stage. The *Other designs* sections of 2a-01 to 2a-04 list the alternatives to compare with yours.

## In BusTub

This is the shape of `ExtendibleHTableBucketPage` (unsorted) and of B+ tree leaf pages (sorted) in the next two modules.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| a page struct `{ size, max_size, array_[] }` overlaid on the buffer | a header read with `from_le_bytes` plus a `PageArray` over the rest |
| `std::map` as the oracle | `BTreeMap` as the model |

**Port rule:** an overlaid struct becomes a header you read and write explicitly and an array view over the rest.

## Learn more

- [`BTreeMap`](https://doc.rust-lang.org/std/collections/struct.BTreeMap.html)
