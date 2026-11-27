A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`HashRing` in `src/container/hash/hash_ring.rs`: a ring that maps keys to named nodes. Each node is placed on the ring at `vnodes` points (virtual nodes); a key belongs to the node at the first point at or after the key's own point, wrapping around. Adding or removing a node must move only the keys that have to move.

## Why

`hash(key) % n` sends almost every key to a different node when `n` changes. Distributed caches, sharded databases and object stores need the opposite: when a node joins, it takes a slice from each neighbour and nothing else moves. This is the core of that idea, in a form small enough to test exhaustively.

## The contract

- `add_node(name)` places `vnodes` points at `hash(name, i)` for `i in 0..vnodes`; false if the node is already there.
- `remove_node(name)` removes its points; false if unknown.
- `node_for(key)` is the node owning the first point at or after `hash(key)` (wrapping), `None` on an empty ring.
- `hash64` is given, so that results are deterministic.

## Invariants

These must hold after every step, whatever the input:

- Every key maps to a node that is currently on the ring.
- The same ring and key always give the same node.
- With one node, every key maps to it.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Adding a node moves keys only **to** the new node; no key moves between two old nodes.
- Removing a node moves only the keys that were on it.
- Adding a node and removing it again restores every key's owner.
- With enough virtual nodes the keys spread within a factor of about 2 of an even split.

## Examples

Worked cases (the tests include them):

```text
ring {A, B}: node_for(k) is A or B; add C: each key is still on its old node or on C
remove C: every key is back where it was
```

## What the tests check

- Mapping, wrapping, empty ring.
- Keys that move when a node is added or removed.
- Balance with many virtual nodes.

## Done when

All the `s2b_c3` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Consistent hashing ring with virtual nodes](/cookbook/p/consistent-hashing): a ring with virtual nodes that moves about 1/N of keys.
- [Partitioning keys across N nodes](/cookbook/p/hash-partitioning): ring, rendezvous, jump and Maglev hashing compared on keys moved.
