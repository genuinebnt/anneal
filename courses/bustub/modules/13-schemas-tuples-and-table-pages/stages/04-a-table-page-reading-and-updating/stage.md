A page that can only be written is of no use. This stage adds the other half: read a tuple by its **slot number** (and say clearly when the slot does not exist), change a tuple's metadata (the **delete flag** and the timestamp), and overwrite a tuple's bytes **in place** when the new tuple has exactly the same length. Together these are everything the table heap and, later, the transaction layer need from a page.

> [!CHECK] `update_tuple_in_place_unsafe` only accepts a tuple of the same length as the one already stored. Why that restriction, what would a longer tuple require, and what makes the function "unsafe" when it contains no `unsafe` block?
> ||A tuple of the same length fits exactly where the old one was, so nothing else in the page moves and every slot stays valid. A longer one needs more room: it must be stored elsewhere (and the slot pointed at it, or the old one deleted and a new inserted, changing the record id). "Unsafe" is BusTub's name for "the caller must already hold the page's write latch": the function does no locking of its own, so two threads calling it would race; Rust has no `unsafe` block because nothing here violates memory safety, only the *logical* protocol.||
>
> - What does a reader of a deleted tuple see?
> - What happens to `num_deleted_tuples` when the same tuple is marked deleted twice?
> - Which operations can fail because of a bad slot?

## The task

- `get_tuple(rid) -> Result<(TupleMeta, Tuple)>`: the slot's metadata and a **copy** of its tuple (carrying `rid`); a slot past the last tuple is an **error** (kind `Invalid`), not a panic.
- `get_tuple_meta(rid) -> Result<TupleMeta>`.
- `update_tuple_meta(meta, rid) -> Result<()>`: replaces the metadata, keeping the tuple. `get_num_deleted_tuples` goes up by one **each time a live tuple becomes deleted**; marking an already deleted tuple again does not count again.
- `update_tuple_in_place_unsafe(meta, tuple, rid) -> Result<()>`: overwrites the tuple with one **of the same length** and sets the metadata; another length is an error and **changes nothing**.
- A deleted tuple keeps its bytes (it can still be read: the transaction layer needs old versions).
- The page is only its bytes: copy them to another buffer and the copy reads the same.

The tests: a property that runs random insert/read/delete/overwrite/resize operations against a `Vec` of `(meta, bytes)`: every read agrees with the model, bad slots are errors, the tuple carries its `rid`, resizing is refused and nothing changes, `num_tuples` and the deleted count agree; marking deleted twice counts once; a page copied byte for byte reads identically; the error kind.

## Your freedom

Your layout (from the last stage), how you share slot-reading code between the three read functions, and how you compute the count.

## The Rust toolbox

**`Result` for expected failures.** `Err(Exception::new(ExceptionType::Invalid, "Tuple ID out of range"))` for a bad slot; the caller (the table heap) turns it into a user-visible error or a retry.

**Copy out, do not borrow.** `get_tuple` returns an owned `Tuple` (`Tuple::from_bytes(rid, &bytes[offset..offset + size])`): the page may be unlatched and evicted after the call, so a borrowed slice would dangle (the borrow checker would not allow it either).

**Early `return Err(..)` with `?` helpers.** A small `fn check(&self, rid) -> Result<u32>` that validates the slot and returns its index avoids repeating the same three lines in the three functions.

**Compare old and new metadata.** `if !old.is_deleted && meta.is_deleted { count += 1 }` is the whole counting rule.

**Property testing a stateful object.** The test keeps a `Vec` model next to the page; reading it is the specification of "what the page should hold now". Write the same kind of model for your own code when you debug.

## If this is new

- [S1 Option & Result](/t/s1-option-result): `Result`, `?`, `map_err`.
- [L1 Ownership & moves](/t/l1-ownership-moves): why returning a copy is right here.
- The optional *errors as values with Result* concept covers error kinds.
- [S3 Vec & slices](/t/s3-vec-slices): Understand it: sub-slices, `split_at`.

## Tests

- A page behaves like a `Vec` of `(meta, bytes)` under random inserts, reads, deletes, overwrites and refused resizes.
- Marking deleted twice counts once, also through an in-place update; a deleted tuple keeps its bytes.
- A page is nothing but its bytes: a copy reads the same.
- A bad slot's error has kind `Invalid`.

## Hints

### Where do slot reads go wrong?

An off-by-one on the last slot (`id >= num_tuples` vs `>`) fails the first property in one step; the shrunk counterexample is "insert one tuple, read slot 1".

### Count only the transition

`update_tuple_meta` receives the new metadata; compare it with the old before writing. The same rule applies in `update_tuple_in_place_unsafe`.

### Keep the length check first

For the in-place update, compare lengths before you write anything; a refused update must leave metadata and bytes as they were.

## Performance

Reading a tuple copies its bytes (tens of nanoseconds for a small tuple); an executor that scans a page calls `get_tuple` once per slot. A zero-copy read (returning a slice into the guard) is faster and is what a real engine does, at the price of tying the tuple's lifetime to the latch.

**Measure it.** Scan a full page of 100-byte tuples 100 000 times with `get_tuple` and compare with reading the same bytes through a slice: predict the ratio.

## Experiment

Optional. Predict first, then run.

1. **Undelete.** Mark a deleted tuple live again (`is_deleted: false`). What does `num_deleted_tuples` say, and should it?
2. **Grow in place.** Allow a longer tuple when the free space allows it by moving later tuples' offsets. Which property of record ids breaks?

## Other designs

- **Copy out (ours).** Safe and simple.
- **Return a guard-bound slice:** no copy; the tuple holds the page latch.
- **Versioned in-place updates** (undo logs in module 4): the page holds the newest version and the log the older ones.
- **Delete by moving the last tuple into the hole:** keeps pages dense, but changes a record id; databases that do it keep a forwarding pointer.

## In BusTub

`Column`, `Schema` and `Tuple` are classes in `src/catalog` and `src/storage/table`; `TablePage` is the slotted page in `src/storage/page/table_page.{h,cpp}`. BusTub's `TableHeap` (module 3c) links `TablePage`s into a table.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `auto GetTuple(const RID &rid) -> std::pair<TupleMeta, Tuple>` | `Result<(TupleMeta, Tuple)>` |
| `BUSTUB_ENSURE(tuple_id < num_tuples_, "Tuple ID out of range")` (throws) | `return Err(..)` |
| a `std::mutex` held by the caller | the caller holds the page's write latch (the guard) |

**Port rule:** an exception for "no such slot" becomes an `Err` the caller can handle.

## Learn more

- [`Result`](https://doc.rust-lang.org/std/result/enum.Result.html) · [`slice::to_vec`](https://doc.rust-lang.org/std/primitive.slice.html#method.to_vec) (copying a slice)
