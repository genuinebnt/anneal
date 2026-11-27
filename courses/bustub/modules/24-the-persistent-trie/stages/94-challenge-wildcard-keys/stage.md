A challenge: no walkthrough, no hints, no solution. It adds to something you built in this module, using what you learned there. It is extra practice and does not count towards the course.

## What to build

`keys_matching` in `src/primer/trie_extras.rs`: list the keys of a trie that match a pattern in which `?` matches exactly one character and every other character matches itself. The whole key must match (same length). Written against the public node type of your trie; results in increasing order.

## Why

Wildcard search is where a trie beats a hash map: a hash map has to look at every key, while the trie follows only the branches that can still match, so `a?c` visits the `a` subtree and one level below it, not the whole set. The pruning is the exercise.

## The contract

- `keys_matching(trie, pattern) -> Vec<String>`; `?` is one character, anything else a literal.
- A key matches when it has the same number of characters as the pattern and each position agrees (or the pattern has `?` there).
- The trie is only read.

## Invariants

These must hold after every step, whatever the input:

- Every result has a value in the trie and the pattern's length.
- Results are sorted and unique.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- A pattern without `?` returns at most the key itself.
- Replacing a letter of a pattern by `?` can only add results.
- `keys_matching` of `?` repeated `n` times lists exactly the keys of length `n`.

## Examples

Worked cases (the tests include them):

```text
keys {abc, abd, aec, xyz}: "a?c" -> [abc, aec]; "ab?" -> [abc, abd]; "???" -> all four
```

## What the tests check

- Literals, wildcards, lengths.
- Empty pattern and the empty key.
- A property against filtering the key list.

## Done when

All the `s0a_c5` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Trie interview questions in Rust](/cookbook/p/tries-core-questions): add-and-search words where '.' matches any one letter.
