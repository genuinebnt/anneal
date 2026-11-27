A bucket fills up. Instead of rehashing the whole table, an extendible hash table **splits just that bucket**: it looks at one more bit of the hash, keeps the keys whose bit is 0 in the old bucket, moves the keys whose bit is 1 to a new one, and updates the directory slots that pointed at the old bucket. If the bucket was already as deep as the directory (its **local depth** equals the directory's **global depth**), the directory has no slot for the new bucket, so it **doubles** first. That is the whole algorithm, and getting the bookkeeping right is the stage.

> [!CHECK] The directory has global depth 2 (four slots) and a bucket with local depth 1 is shared by slots 0 and 2. It fills up. Does the directory double? Which slots point at the new bucket afterwards, and which keys move?
> ||No doubling: the bucket's local depth (1) is below the global depth (2), so there are already two slots for it. After the split both buckets have local depth 2; slot 0 keeps the old bucket and slot 2 points at the new one (the slots differ in bit 1, the new bit). Keys whose hash has bit 1 set move to the new bucket. Doubling is needed only when local depth equals global depth, because then the old bucket is reached through exactly one slot.||
>
> - Why do slots 0 and 2 share a bucket at local depth 1?
> - Which bit of the hash decides after the split?
> - What if all the moved keys land in one bucket again?

## The task

`insert` now handles a full bucket. After every call:

- **It succeeds exactly when the key is new and the table can make room.** A full bucket is split, the directory doubling first when the bucket already uses every bit the directory has, as many times as it takes for the key to fit. It fails (false) only when that would need more than `directory_max_depth` bits, and a failed insert leaves the table exactly as it was.
- **The structure stays extendible.** No bucket's local depth exceeds the global depth; the `2^(global - local)` slots that share a bucket's low `local` bits all point at it, with that local depth; and every key sits in the bucket its hash's low bits select.

> [!ASIDE] The steps, if you would rather not work them out
> 1. If the key is present: false. Otherwise find the bucket; if it has room, insert.
> 2. If the bucket is full: if its local depth equals the directory's global depth, then if the global depth is already `directory_max_depth` the insert **fails** (false, and the table is unchanged); otherwise **double the directory** (global depth + 1; the new half mirrors the old).
> 3. **Split** the full bucket: allocate a new bucket; local depth + 1 for both; move the entries whose hash has the new bit (bit `new_depth - 1`) set to the new bucket; update every directory slot that pointed at the old bucket: the ones with the new bit set point at the new bucket, all get the new local depth.
> 4. Try again: the key may still land in a full bucket (all the old keys moved the same way), in which case you split again, until it fits or the directory cannot grow.

The tests check, for random keys and many table shapes, that `insert` succeeds **exactly when** the keys already present that share the new key's header slot and low `directory_max_depth` hash bits number fewer than `bucket_max_size` (nothing can separate keys that agree on all those bits); that every inserted key is found afterwards; that a failed insert leaves everything as it was; that `verify_integrity` passes after every operation; and that guards are released (a pool of four frames is enough for 500 keys).

## Your freedom

How the directory doubles (copying slots), how you decide which entries move, whether the new bucket is filled in one pass, and what `verify_integrity` now checks. Your checker should enforce the standard invariants: every slot's local depth is at most the global depth; a bucket with local depth `d` is pointed at by exactly `2^(g-d)` slots; all slots that point at one bucket agree on their low `d` bits; and every key in a bucket belongs to it.

## The Rust toolbox

**Bit tricks you will need.** The low `g` bits: `hash & ((1 << g) - 1)`; the bit that distinguishes the new bucket: `1 << (new_depth - 1)`; a slot's **split image**: `slot ^ (1 << (local_depth - 1))`.

**Doubling a directory in a page.** Copy slots `0..n` to `n..2n` with `copy_within`, then increment the global depth. The array lives in the page, so check `2 * n` fits: `directory_max_depth` bounds it.

**Walking backwards while removing.** If you remove entries from the old bucket while iterating, go from the last index to the first (`for i in (0..size).rev()`), so removal does not shift the entries you have yet to visit.

**Several guards at once.** Splitting holds the directory (write) and the old bucket (write) and creates a new bucket page; take the new page's guard, finish all three updates, and drop in any order only after the table is consistent. A panic in the middle leaves a half-split table: assert your preconditions at the start.

**`loop` for retry.** `loop { ... if fits { return true } ... split ... }` states "split again if needed" directly; a bounded depth guarantees it ends.

## If this is new

- [S3 Vec & slices](/t/s3-vec-slices): `copy_within`, reverse ranges.
- [L2 Borrowing](/t/l2-borrowing): why two `&mut` views into two pages are fine but two views into one page are not.
- The optional *shifts, masks and bit tricks* concept has `1 << n`, masks and XOR.

## Tests

- A full bucket splits so every key is still found; the directory grows as keys arrive (`global_depth` rises as buckets fill).
- Keys that cannot be separated make the insert fail; a directory that may not grow is full when its bucket is.
- Failed inserts leave the table as it was.
- A pool of four frames is enough for 500 keys: guards are released.
- For random shapes and operations, the success of every insert matches the hash-class rule and everything inserted is found.

## Hints

### Walk one split on paper

Take bucket size 2, directory max depth 3, and insert keys whose hashes you write in binary: 0b000, 0b100, 0b010. Draw the directory before and after each step.

### Which slots change?

After a split, the slots that pointed at the old bucket are those that agree with the old slot on the low `old_depth` bits. List them in an example with global depth 3 and old depth 1: four slots; two keep the old bucket, two get the new one.

### A split that moves nothing

Two keys can differ only in bit 5 while the table's depth is 2. A split on bit 1 moves nothing and the key still does not fit. Your retry loop must split again; the directory grows until the keys separate or the maximum is reached.

## Performance

A split reads one bucket, writes two, and updates up to `2^(g - d)` directory slots: constant work, no rehash of the table. Doubling copies the directory (up to 512 slots). The cost per insert is amortised constant, and the table never pauses to rebuild: the property that made extendible hashing attractive for databases.

**Measure it.** Insert one million keys with a bucket size of 100 and record how many splits and doublings happened and the load factor (keys / capacity of buckets). Predict the load factor: it is known to hover around `ln 2`, about 69%.

## Experiment

Optional. Predict first, then run.

1. **Never double.** Make `directory_max_depth` 0 and bucket size 2. What is the largest table you can build? Which test shows it?
2. **Split image.** Add a test of your own that, after random inserts, checks that slot `s` and its split image have local depths differing by at most... (what does the invariant say?).

## Other designs

- **Split by the new bit (ours).** The textbook rule, one more hash bit.
- **Linear hashing.** Split buckets in a fixed order, not the full one; no directory, a different cost profile.
- **Chained overflow pages.** A full bucket gets an overflow page instead of splitting: simple, degrades under skew.
- **Cuckoo hashing.** Two choices per key, kicked out on collision; used in memory, rarely on disk.

## In BusTub

BusTub's `InsertToNewBucket`, `UpdateDirectoryMapping` and `MigrateEntries` helper names correspond to the pieces above. A key that cannot be inserted because the directory has reached its maximum depth makes `Insert` return false, as here.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `dir->IncrGlobalDepth()` | your directory view's method that doubles the slots |
| `auto split_image = idx ^ (1 << (local_depth - 1))` | the same expression on `u32` |
| iterating a `std::vector` with `erase` | iterate in reverse and remove, or collect then remove |

**Port rule:** the bit expressions carry over unchanged; the shifting `erase` becomes a reverse loop or a `retain`.

## Learn more

- Fagin et al., TODS 1979 · [Extendible hashing on Wikipedia](https://en.wikipedia.org/wiki/Extendible_hashing)
