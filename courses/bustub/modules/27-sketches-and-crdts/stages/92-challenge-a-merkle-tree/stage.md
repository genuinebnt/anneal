A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`MerkleTree` in `src/primer/merkle.rs`: build a hash tree over a list of byte-string leaves: the hash of a leaf is `leaf_hash(data)`; an inner node is `node_hash(left, right)`; an odd node at the end of a level is paired with itself. `root()` commits to every leaf; `proof(i)` returns the sibling hashes from leaf `i` up to the root; `verify(root, leaf, index, proof)` recomputes the root from the leaf and the proof.

## Why

Anti-entropy in replicated stores (Dynamo, Cassandra), certificate transparency, git and blockchains all compare large data by comparing roots, and prove one item belongs with a logarithmic-size proof. The structure is the same everywhere; the details to get right are the odd node at the end of a level and that a proof is only valid for one index.

## The contract

- `leaf_hash` and `node_hash` are given (64-bit FNV-1a based, with domain separation: a leaf and an inner hash never collide by construction of the inputs).
- `root()` of an empty tree is `0`; of one leaf, that leaf's hash.
- `proof(i)` is a list of `(sibling_hash, sibling_is_right)` from the bottom up; `None` for an index out of range.
- `verify(root, leaf, proof)` folds the leaf hash with the proof and compares it with `root`.

## Invariants

These must hold after every step, whatever the input:

- `verify(root, leaves[i], proof(i))` is true for every `i`.
- Changing any leaf changes the root.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- A proof for leaf `i` does not verify a different leaf (with overwhelming likelihood: for the leaves tested, never).
- Appending a leaf changes the root; the root of the same leaves is always the same.
- Proof length is `ceil(log2(n))`.

## Examples

Worked cases (the tests include them):

```text
4 leaves: proof(2) has 2 siblings; changing leaf 1 changes the root but not proof(2)'s first sibling hash... except that its second sibling (the hash of leaves 0-1) changes
```

## What the tests check

- Roots of 0, 1, 2, 3 and 4 leaves.
- Proof sizes and verification for every leaf.
- Tampering with a leaf, a sibling or the order.

## Done when

All the `s0d_c3` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Merkle tree](/cookbook/p/merkle-tree): Merkle tree with inclusion proofs, and why duplicating the odd node is ambiguous.
