A challenge: no walkthrough, no hints, no solution. It adds to something you built in this module, using what you learned there. It is extra practice and does not count towards the course.

## What to build

`node_count` and `shared_nodes` in `src/primer/trie_extras.rs`: `node_count(trie)` counts the nodes of a trie; `shared_nodes(a, b)` counts the nodes that are **the same allocation** in both (`Arc::ptr_eq`), found by walking the two tries together. With these, the tests check that your `put` and `remove` copy only the nodes on one path.

## Why

"Persistent" is a claim about memory, not about results: a trie that copies everything on every `put` returns the right answers and defeats the purpose. Counting shared nodes is how you test the claim. After one `put` of a key of length `n`, at most `n + 1` nodes may be new; every other node must be the *same* `Arc` as before.

## The contract

- `node_count(trie)`: nodes reachable from the root (0 for an empty trie).
- `shared_nodes(a, b)`: the number of positions where both tries have a node at the same path and the two nodes are the same `Arc` (pointer-equal); a shared node's whole subtree is shared and is counted node by node.
- Both read only.

## Invariants

These must hold after every step, whatever the input:

- `shared_nodes(t, t) == node_count(t)`.
- `shared_nodes(a, b) <= min(node_count(a), node_count(b))`.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- After `t.put(key, v)`: `node_count(new) - shared_nodes(old, new) <= key.chars().count() + 1`.
- After `t.remove(key)` the same bound holds.
- Putting an unrelated key into two versions of a trie leaves both new tries sharing everything but their own paths.

## Examples

Worked cases (the tests include them):

```text
t has 100 keys; t2 = t.put("abc", 1): at most 4 nodes of t2 are not in t
```

## What the tests check

- Counting; sharing with itself; one change.
- The path-length bound for put and remove (this tests *your* implementation).
- A property over random tries and keys.

## Done when

All the `s0a_c3` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [More primers](/cookbook/p/more-primers): the copy-on-write trie: put and remove share every untouched node.
