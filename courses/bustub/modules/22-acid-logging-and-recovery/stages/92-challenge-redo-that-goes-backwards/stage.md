A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`redo` in `src/recovery/redo_pass.rs` replays log records onto pages after a crash. Each page remembers the LSN of the last record applied to it (`page_lsn`). It looks right, and on a page that had already been flushed with newer changes, recovery writes an **older** value back over it. Find the bug and fix it.

## Why

Redo must be **idempotent**: replaying the whole log from the start must give the same result as replaying only what the disk is missing, because recovery cannot know which pages were flushed before the crash.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||Redo applied every record, even to a page whose `page_lsn` was already newer, so an old value overwrote a flushed newer one. A record is applied only when its LSN is greater than the page's.||

## The contract

- `redo(pages, log)`: `pages[p]` is `(page_lsn, value)`; each record is `(lsn, page, new value)` in increasing LSN order.
- A record is applied iff `lsn > page_lsn`; applying sets the value and `page_lsn = lsn`.

## Invariants

These must hold after every step, whatever the input:

- A page's `page_lsn` never decreases.
- After redo, every page's value is the value of the record with the highest LSN for it that is at least its previous `page_lsn` (or its original value).

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Running `redo` twice gives the same pages as running it once.
- Starting from pages that already reflect a prefix of the log gives the same result as starting from empty pages.
- Records already reflected are skipped, and nothing is applied out of order.

## Examples

Worked cases (the tests include them):

```text
page 0 = (5, 'new'); log [(3, 0, 'old'), (7, 0, 'newer')] -> (7, 'newer'), and 'old' is never written
```

## What the tests check

- A page ahead of the log.
- Repeated redo.
- A property: any prefix already applied gives the same final state.

## Done when

All the `s4c_c3` tests pass, and you can say in one sentence what the bug was.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Build WAL and ARIES](/cookbook/p/build-wal-and-aries): redo skipped by the dirty page table, recLSN and pageLSN tests, counted.
- [ARIES crash recovery](/cookbook/p/aries-recovery): page LSNs and the WAL rule in ARIES.
- [Crash recovery (ARIES)](/cookbook/p/crash-recovery-aries): pageLSN, recLSN and flushedLSN, and what each decides.
