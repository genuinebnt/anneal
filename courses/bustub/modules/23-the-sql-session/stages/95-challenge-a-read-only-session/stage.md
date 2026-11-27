A challenge: no walkthrough, no hints, no solution. It adds to something you built in this module, using what you learned there. It is extra practice and does not count towards the course.

## What to build

`ReadOnlySession` in `src/common/read_only.rs`: a wrapper around **your** `Session` that accepts queries and refuses everything that would change the database (`INSERT`, `UPDATE`, `DELETE`, `CREATE TABLE`, `CREATE INDEX`, and an `EXPLAIN ANALYZE` of one of those). The check is made on the whole text *before* anything runs, so a script that ends in a delete never executes its first statements either.

## Why

Reporting users, replicas and dashboards get read-only credentials, and the safe version of "read-only" is enforced where statements are understood, not by hoping the application is careful. The detail that makes it a good exercise is *before anything runs*: refusing at the third statement of three leaves two of them committed, which is exactly what a read-only guarantee exists to prevent.

## The contract

- `new(session)` wraps a session; `execute(sql)` parses the text with `parser::parse`; if any statement is a data or schema change (including one inside `EXPLAIN`), it returns an `Invalid` error that names the kind of statement and runs nothing.
- Otherwise it forwards the text to the wrapped session and returns its answer. `BEGIN`, `COMMIT`, `ROLLBACK`, `SET`, `SHOW`, `SELECT` and `EXPLAIN` of a query are allowed.
- `inner()` gives access to the wrapped session (for `in_transaction()` and so on).

## Invariants

These must hold after every step, whatever the input:

- No statement of a refused text is executed.
- A text of only queries behaves exactly as it does without the wrapper.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Adding a write to the end of any allowed text makes it refused.
- Refusal changes no state: an open transaction stays open.

## Examples

Worked cases (the tests include them):

```text
`select 1; delete from t` -> Err, and `t` is untouched
`explain select * from t` -> allowed
```

## What the tests check

- Queries pass through.
- Each kind of write is refused.
- Nothing of a refused text runs.
- Transaction control and SET are allowed.

## Done when

All the `s4e_c6` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Roles, privileges and row-level security](/cookbook/p/roles-privileges-and-row-level-security): checking which kinds of statement a role may run, before running them.
