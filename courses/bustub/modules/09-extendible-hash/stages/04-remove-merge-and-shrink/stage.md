Insertion made the table grow; removal must let it shrink. `remove` deletes a key from its bucket. If that empties the bucket, the bucket is **merged** into its **split image** (the bucket that would have been split from it), giving back a page and lowering the local depth; and if the merge leaves no bucket needing the directory's last bit, the **directory halves**. Done well, a table that has grown to a million keys and been emptied returns to one bucket, and the pool gets the pages back.

> [!CHECK] After a remove the bucket in slot 5 is empty and its split image (slot 1) has the same local depth. Describe what the merge changes in the directory, which page is deleted, and when the merged bucket might merge again.
> ||Every slot that pointed at the empty bucket now points at the split image, and every slot pointing at the surviving bucket gets local depth lowered by one; the empty bucket's page is deleted from the pool. The merged bucket may itself be empty (if the image was empty too) or may now have an empty split image at the new, lower depth; so after a merge look again at the new situation and repeat. The directory then halves while every local depth is below the global depth.||
>
> - Why must the split image have the same local depth?
> - What if the split image is deeper than the empty bucket?
> - Which page does `delete_page` free?

## The task

`remove(&key) -> bool`: false if the key was not there. Otherwise remove it from its bucket and, if the bucket is now empty, **merge**:

1. If the bucket's local depth is 0 there is nothing to merge with.
2. Find the split image slot (`slot ^ (1 << (local_depth - 1))`). If its local depth differs from this bucket's, stop: the sibling has been split further.
3. If at least one of the two buckets is empty, keep the other (either, if both are empty), point every slot that referred to the empty one at the survivor, lower the local depth of every slot referring to the survivor, and delete the empty bucket's page from the pool.
4. Repeat from step 1 with the survivor, as long as the merged bucket or its new split image is empty.
5. Finally, while the directory can shrink (every local depth is below the global depth), halve it.

Tests: remove reports presence correctly; emptying a table leaves one that still works through repeated fill and empty rounds; empty buckets are given back (the disk sees `delete_page` calls); removing a key does not disturb its neighbours; a slot freed by a remove can be used again; and the full model (inserts, lookups and removals on random table shapes) with `verify_integrity` after every operation and no leaked pins.

## Your freedom

How you find and repair the directory, whether a merge scans the whole directory or only the affected slots, whether you merge only empty buckets (the contract) or also half-empty ones (not tested and not required), and what you free.

## The Rust toolbox

**Latch order for a merge.** `remove` holds the directory (write) and the bucket; to look at the split image's bucket you need its guard too. Drop the first bucket's guard before taking the second if you can (read the emptiness first), and never take two buckets in an order that two threads could reverse: the boss stage runs threads.

**`delete_page` returns a bool.** It fails if the page is pinned: drop your own guard on the bucket before deleting it.

**Collect, then change.** `let slots: Vec<usize> = (0..dir.size()).filter(|&s| dir.bucket_page_id(s) == empty).collect();` then update them: the filter borrows the view, the update needs it mutable.

**Halve with a loop.** `while dir.can_shrink() { dir.decr_global_depth(); }`: `can_shrink` is "no local depth equals the global depth" (and the global depth is above 0).

## If this is new

- [S3 Vec & slices](/t/s3-vec-slices), [L2 Borrowing](/t/l2-borrowing): collect indices before mutating.
- The optional *deadlock and lock ordering* concept for the two-bucket case.
- [C1 Threads & shared state](/t/c1-threads-shared-state): Understand it: latch crabbing: take the child, then let go of the parent; lock ordering.
- Y5 Testing & verification: Understand it: an invariant checker run after every operation; scoped threads for a stress test.

## Tests

- `remove` says whether the key was there.
- Removing everything leaves a table that still works, three rounds over.
- Empty buckets are given back to the pool (`delete_page` is called for most of them), and an emptied table's directory shrinks back to depth 0 (`global_depth`).
- Removing one key does not disturb its neighbours after merges.
- A slot freed by a remove can be used again by an insert that failed before.
- For random shapes and operations with removals, the table agrees with a `HashMap` and `verify_integrity` passes after every step.

## Hints

### The invariant after a merge

State it before you code: every bucket page is referenced by `2^(g - d)` slots, those slots agree on their low `d` bits, `d <= g`. After a merge check all three for the survivor.

### Merge loops

After merging, the survivor has lower local depth, so it has a new split image. Is it empty? Then merge again. Write the loop so that the exit conditions are the three `break`s of the steps above.

### The pool says delete failed

You still hold a guard on the page. Drop it first; the table is consistent without it because the directory no longer refers to the page.

## Performance

A remove is a lookup, one bucket update, and a merge only when a bucket empties; merges touch a few directory slots and delete a page. The pool gets pages back, so the table's footprint tracks its contents rather than its high-water mark.

**Measure it.** Fill with 100 000 keys, remove all but 1 000, and count the pages deleted and the final directory depth. Predict the directory depth from 1 000 keys and the bucket size.

## Experiment

Optional. Predict first, then run.

1. **No merging.** Skip the merge and run the shape tests. Which fail, and what is the final directory depth after an emptied table? The invariant checker is not violated: why does the model still pass?
2. **Merge half-full buckets.** Merge whenever two split images together fit in one bucket. What is the effect on the load factor of a long-running table, and on the cost of each remove?

## Other designs

- **Merge only when empty (ours, and BusTub's).** Simple, leaves some sparse buckets.
- **Merge when two images fit in one.** Better space use; more bookkeeping; merges more often than needed under a workload that fluctuates around a boundary.
- **Never shrink.** Simplest and the usual choice in practice (rebuild offline).

## In BusTub

BusTub's `Remove` is followed by `Merge` of the empty bucket with its split image, and `VerifyIntegrity` of the directory is part of its tests; the course checks the same rules with the checker you write.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `bpm_->DeletePage(page_id)` after releasing the guard | `bpm.delete_page(id)` after `drop(guard)` |
| `while (dir->CanShrink()) dir->DecrGlobalDepth();` | the same loop |
| a `std::vector<uint32_t>` of slots to update | `Vec<usize>` collected first |

**Port rule:** two-phase "find then update" is the same in both languages; in Rust the borrow checker makes you do it.

## Learn more

- Fagin et al., TODS 1979 · [`HashMap::remove`](https://doc.rust-lang.org/std/collections/struct.HashMap.html#method.remove) for the model
