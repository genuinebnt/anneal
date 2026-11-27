A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`step` in `src/common/session_state.rs` is the transition function of a session: from a state (idle, in a transaction, failed) and an input (BEGIN, a statement that succeeded or failed, COMMIT, ROLLBACK) to the next state and a reply. It looks right and has one wrong transition: it lets a client believe work was saved that was not. Find the bug and fix it.

## Why

Every other mistake in a transaction state machine is loud: a refused statement, a warning. This one is silent, and it is the worst thing a database can do: it lets an application carry on believing its changes are durable.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||`COMMIT` in a failed transaction replied `COMMIT`. The transaction is rolled back, so the reply must be `ROLLBACK`, as PostgreSQL does.||

## The contract

- Idle: BEGIN opens a transaction (`BEGIN`); a statement runs by itself (`Done` or `Error`, state unchanged); COMMIT and ROLLBACK only warn.
- In a transaction: BEGIN warns; a statement that succeeds is `Done`; one that fails is `Error` and fails the transaction; COMMIT answers `COMMIT`, ROLLBACK answers `ROLLBACK`, both go idle.
- Failed: BEGIN and statements are refused; COMMIT and ROLLBACK both answer `ROLLBACK` and go idle.

## Invariants

These must hold after every step, whatever the input:

- A reply of `COMMIT` is only ever given for a transaction in which no statement failed.
- From any state, ROLLBACK ends in idle.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Replaying the same inputs gives the same replies.
- Inserting a COMMIT after a failed statement never yields `COMMIT`.

## Examples

Worked cases (the tests include them):

```text
BEGIN, failed statement, COMMIT -> BEGIN, Error, ROLLBACK
BEGIN, ok statement, COMMIT -> BEGIN, Done, COMMIT
```

## What the tests check

- Each row of the transition table.
- A property over input sequences.

## Done when

All the `s4e_c3` tests pass, and you can say in one sentence what the bug was.
