Erasing mirrors inserting: search for the key, remember the update vector, then on every level the node takes part in, make the predecessor skip it. Afterwards the list may have empty top levels, which must be given up, or every later search wastes time walking down empty lanes.

## The task

- `erase(&key) -> bool`: write lock; search; not found: `false`; on each level of the node make the predecessor (`update[level]`) skip it; make the slot reusable; lower the list's height while its top level has no nodes; size - 1; `true`.
- `clear()`: write lock; forget every element; the list is as new (height 1, size 0).

The tests: exact scenarios (erase removes the key; erasing a missing key changes nothing; every level forgets the erased node; erased slots are reused and the list still works; clear empties the list and it can be filled again; erasing the tallest node lowers the height of the list), and a property: **any inserts, erases, lookups and clears against a `BTreeSet`**, checking after every step that the list is well formed (keys in order, every height between 1 and 14, level 0 holds all keys, **each higher level is exactly the nodes tall enough**, nothing above the tallest node).

## Your freedom

How you reuse the slots of erased nodes (a free list, or leave them), and whether you track the list's height.

## The Rust toolbox

**The update vector again.** The same search gives you the predecessors on every level: `for level in 0..node_height { pred[level].next[level] = node.next[level] }` unlinks the node everywhere at once.

**A free list.** `free.push(id)` on erase and `free.pop()` on the next insert keeps the arena from growing under churn.

**Early return.** `let Some(id) = found else { return false };` keeps the erase path flat.

```rust
for level in 0..inner.nodes[id].height() {
    let next = inner.nodes[id].next(level);
    inner.nodes[update[level]].set_next(level, next);
}
inner.release(id);
while inner.height > 1 && inner.nodes[HEADER].next(inner.height - 1).is_none() { inner.height -= 1; }
```

## Design notes

**Which levels.** Only levels below the node's height link to it; `update[level]` for higher levels point at other nodes and must not be touched.

**Shrinking.** The invariant is "`height` is the highest level that has any node". Search starts there; a larger value is still correct but slow.

**The arena.** `release` clears a slot and remembers it; the next `insert` reuses it, so repeated insert/erase does not grow `nodes`.

## If this is new

- [S3 Vec & slices](/t/s3-vec-slices): reusing arena slots.
- [S1 Option & Result](/t/s1-option-result): `let else`.
- The optional *arenas* concept.
- [D5 Linked lists](/t/d5-linked-lists): Linked lists, the Rust way: index-linked nodes in a `Vec`.
- [F3 Memory & allocation](/t/f3-memory-allocation): Arenas and pools: a node arena with a free list.
- Y5 Testing & verification: Build it: a `BTreeSet` as the oracle; a structural check after every step.

## Tests

- Erase present and missing keys; levels; slot reuse; clear; the height of the list after erasing the tallest node.
- Property: a set model with a structural check after every step.

## Hints

### Unlink on the node's own levels only

Loop to the node's height, not the list's.

### Lower the height after unlinking

Check the header's link at the top level; do it in a loop since several levels can empty at once.

### `clear` must also reset the header's links

Truncating the arena leaves the header pointing at slots that no longer exist.

## Performance

Same expected `O(log n)` as insert. `clear` is `O(n)` in the number of nodes (truncating the arena drops each key). The free list makes insert/erase churn allocation-free once the arena has grown to the working-set size.

**Measure it.** Insert and erase 100 000 keys five times: `nodes.len()` stays near 100 001.

## Experiment

Optional. Predict first, then run.

1. **Unlink only level 0.** Which test, and which property, catches it?
2. **Never lower the height.** Does anything observable change? (Think about what the height is for.)

## Other designs

- **Unlink and reuse slots (ours).**
- **Mark as deleted and unlink lazily** (concurrent skip lists): readers still see the node until it is physically removed.
- **Rebuild on a threshold:** tombstones and compaction, as in LSM trees.

## In BusTub

`skiplist.cpp`: "`Erases the key from the skip list.` ... `@return bool true if the element got erased, false otherwise.`" and `Clear`: "`Removes all elements from the skip list. Note: You might want to use the provided Drop helper function.`" (the Rust arena needs no such helper).

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `Drop()` frees links iteratively to avoid a stack overflow | dropping the arena frees every node in a loop |
| `links_[i] = nullptr` to unlink | `set_next(level, next_of_removed)` |

**Port rule:** an unlink is "predecessor.next = removed.next" on each level; no `delete` needed.

## Learn more

- [`Vec::truncate`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.truncate) · [`mem::replace`](https://doc.rust-lang.org/std/mem/fn.replace.html)
