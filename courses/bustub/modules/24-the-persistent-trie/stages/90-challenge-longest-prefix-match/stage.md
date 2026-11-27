A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`PrefixTable` in `src/primer/prefix_table.rs`: a table of bit-string prefixes (`"1010"`, `""`, ...) with a value each. `lookup(address)` returns the entry whose prefix is the **longest** prefix of the address bit string.

## Why

IP routers, URL routers, phone-number switches and `LIKE 'abc%'` indexes all answer "which of these prefixes is the longest one that matches". A trie answers it in time proportional to the address length, whatever the number of entries; the *longest* part is what makes it more than a membership test.

## The contract

- `insert(prefix, value)` replaces the value of an existing prefix and returns the old one. Prefixes and addresses contain only `0` and `1`; the empty prefix matches everything.
- `lookup(address)` returns `Some((prefix, value))` for the longest stored prefix that is a prefix of `address`, or `None`.
- `remove(prefix)` deletes an entry.

## Invariants

These must hold after every step, whatever the input:

- A returned prefix is a prefix of the address.
- No stored prefix that is a prefix of the address is longer than the returned one.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Adding a longer matching prefix changes the answer to it; adding a shorter or non-matching one never does.
- Removing the answered entry falls back to the next longest.
- `lookup(a)` and `lookup(b)` for addresses sharing their first `n` bits return the same entry if it is at most `n` long.

## Examples

Worked cases (the tests include them):

```text
table {"": A, "10": B, "1011": C}: lookup "10110" -> ("1011", C); "1001" -> ("10", B); "0" -> ("", A)
```

## What the tests check

- Longest, shorter and default matches.
- Insert, replace, remove.
- A property against a scan over all entries.

## Done when

All the `s0a_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [A trie and a compressed radix tree](/cookbook/p/trie-radix): a trie and radix tree with longest-prefix match.
