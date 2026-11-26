Shopping carts, tag lists and presence lists are edited on several devices that are sometimes offline. Write an
**observed-remove set** (OR-Set): each replica edits its own copy, and copies are combined with `merge`.

- `OrSet::new(replica)`: an empty set owned by the replica with that id. Every replica in a system has a different id.
- `add(item)` adds on this replica.
- `remove(&item)` removes **the additions of `item` that this replica has seen so far**. It returns whether the item was
  present. Removing something absent does nothing.
- `merge(&other)` brings in everything `other` knows. It does not change `other`, nor this replica's id.
- `contains(&item)` and `items()` (the present elements, ascending, each once).

The behaviour that follows:

- an element removed on one replica and **concurrently added** on another is **present** after merging (add wins);
- a removed element can be added again;
- merging an old copy never brings a removed element back;
- merging in any order, in any grouping and any number of times gives the same result, so replicas that have received
  the same updates show the same items.

`T` is `Ord + Clone`. Memory may grow with the number of additions ever made.
