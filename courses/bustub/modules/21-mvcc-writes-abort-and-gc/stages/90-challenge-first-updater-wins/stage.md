A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`WriteClaims` in `src/concurrency/write_claims.rs`: the rule that stops lost updates under snapshot isolation. A transaction **claims** a key before writing it; if another live transaction already holds the claim, the claim fails with the owner's id and the caller aborts (or retries). `release_all(txn)` drops all of a transaction's claims at commit or abort.

## Why

Snapshot isolation lets two transactions both read a row and both try to change it; without a rule, the second writer silently overwrites the first (a lost update). "First updater wins" is the cheapest rule that prevents it: the first to write a key owns it until it finishes, and the second is refused at once, without blocking.

## The contract

- `claim(txn, key)` is `Ok(())` if the key is free or already held by `txn`; `Err(Conflict { owner })` if another transaction holds it.
- `release_all(txn)` frees every key `txn` holds (returns how many).
- `owner_of(key)` is the holder, if any.

## Invariants

These must hold after every step, whatever the input:

- A key has at most one owner.
- A transaction's claims all belong to it until it releases them.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Two transactions can never both succeed in claiming the same key without a release in between.
- After `release_all(t)`, no key is owned by `t`.
- Claiming the same key twice by the same transaction is the same as once.

## Examples

Worked cases (the tests include them):

```text
claim(1, 'a') ok; claim(2, 'a') -> conflict with 1; release_all(1); claim(2, 'a') ok
```

## What the tests check

- Free, own and foreign keys.
- Release.
- A property against a map.

## Done when

All the `s4b_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Project 4 · MVOCC](/cookbook/p/project-4-mvocc): write-write conflict detection that aborts at write time.
- [MVCC snapshot isolation](/cookbook/p/mvcc-snapshot-isolation): first-committer-wins and the lost updates it prevents.
