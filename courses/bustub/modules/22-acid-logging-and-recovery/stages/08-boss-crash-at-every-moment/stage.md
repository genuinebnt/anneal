**Where this fits.** A log, a store that obeys the write-ahead rule, redo, undo and checkpoints, each tested alone. This stage kills the machine **at every moment** and at the worst ones, and asks the only question that matters: *is the database exactly the committed transactions?* Three judges. A **bank**: four accounts, transfers that commit, one that rolls back, one cut short by the crash; money must be neither lost nor made, whatever step the crash falls after and whichever pages had been written. A **garbage tail** on the log, as a record half written when the power went, must be ignored and must not stand in the way of new work. And a **crash in the middle of recovery**: recovery itself can be interrupted, and running it again must still reach the right state. A random property does all of it at once with checkpoints, flushes and open transactions.

> [!CHECK] The bank script crashes after every step, with the pages written never, after every step, and after every other step. For each crash point, what are the two invariants the test checks, and which of them would a recovery that forgets to undo the loser *still satisfy*? Which bug would the "pages written after every step" variant catch that "never written" cannot, and the other way round?
> ||The two invariants: the total of the four accounts is 400 (atomicity: a half-done transfer must vanish), and each account equals the state after the last committed transfer (durability: committed work survives). A recovery that forgets to undo the loser *breaks the total* (a debit without its credit), so it fails the first; a recovery that undoes committed work fails the second. "Pages written after every step" puts uncommitted data on disk: it catches a missing **undo**. "Never written" leaves the pages empty: it catches a missing **redo**.||
>
> - Which crash points leave the open transfer with only the debit logged?
> - What does the log look like after the rolled-back transfer?
> - Why does every committed transfer have to be in the log before the crash?

## The task

Nothing new to write. Make `cargo test --test stages_4c s4c_08` pass:

- **the bank**: for every prefix of a script of transfers (three flush modes: no pages written, every page written after every step, one page written after every other step) crash, recover, and check that the four balances add up to 400 and equal the last committed state;
- **a garbage tail**: bytes of a half-written record after the intact log are ignored by recovery, and a new transaction after recovery survives another crash (the log after the garbage is readable);
- **a crash between redo and undo**: recovery reaches the point where history is repeated and the pages are written, the machine dies again, and a second recovery finishes the job;
- **a random property**: random runs with checkpoints, page flushes, log flushes, rollbacks and open transactions; a random garbage tail; sometimes a crash after redo; after the final recovery the store holds exactly the committed records, and a new transaction, another crash and another recovery still give the right answer.

## Your freedom

None new: a failure belongs to one of your earlier stages.

## The Rust toolbox

**Crashing is copying.** The test disk keeps pages and log bytes in memory; `crash()` copies them, which is what a power cut leaves. Everything else (the pool, the log buffer, the store) is rebuilt from the copy with `World::restart`.

**Reading a failure.** The failing property prints the shrunk run; replay it in a `#[test]` with `println!("{:?}", log.records())` after each step and look for the first record that is not where the write-ahead rule or the end-of-log rule says it should be.

**Replaying by hand.** `analyse(&records)`, `redo(..)` and `undo(..)` are public: call them one at a time and print `store.scan()` between them to see which pass goes wrong.

## If this is new

- Everything is in the earlier stages of this module.
- [Y5 Testing & verification](/t/y5-testing-verification): Build it: crash a disk at a random point, then recover; a model of the committed state.

## Tests

- The bank at every crash point and every flush mode; the garbage tail; a crash between redo and undo.
- Property: random runs with everything on, a garbage tail and an interrupted recovery.

## Hints

### Total is wrong

The money is not conserved: look at undo (a loser's debit stayed) or at redo (a committed credit is missing).

### Correct total, wrong accounts

Committed work was lost or undone: look at commit's flush, at the checkpoint (redo skipped something not in the pages) and at the swapped records of a rolled-back transaction.

### Fails only after a garbage tail

`LogManager::new` must cut the torn tail before anything is appended.

## Performance

The bank test recovers a few dozen times in milliseconds. The property's runs of up to 100 operations recover in well under a millisecond each: recovery time is proportional to the log since the last checkpoint.

## Experiment

Optional. Predict first, then run.

1. **Break one rule.** Remove the log flush in `flush_page`. Which of the three bank flush modes finds it?
2. **Delay the commit's flush by one record.** Does any test notice? What does that tell you about the limits of testing durability with only whole-record crashes?

## Explain it aloud

The tests check that recovery is right. An interviewer checks that you know **why**, with no keyboard. Answer each question out loud in about a minute before opening the model answer; record yourself once and listen for the places you stall.

> [!CHECK] A client is told "committed", and the machine loses power a millisecond later. What guarantees the transaction survives, and what exactly had to be on the disk before the client heard "committed"?
> ||The `Commit` record, and every record before it, flushed to stable storage (an `fsync` of the log): the log is read back on restart and the transaction is a winner. The data pages did **not** have to be written: they may be written minutes later or lost, because redo can rebuild them from the log. That is why a commit costs one sequential log write and one sync, not a write of every page it touched. The rule that makes it safe is write-ahead: no page reaches the disk before the log records describing it.
>
> - What is on disk when the client is told, and what is not?
> - Why is flushing the log enough, when the pages are not written?
> - Which rule stops a page from being written before its log record?

> [!CHECK] Recovery redoes the changes of transactions that never committed. Why not skip them, and what does the undo pass then do?
> ||Because the pages may hold any mixture of old and new states, and "put every change in place, in log order, whoever made it" is the one procedure that is correct from every starting point: afterwards the pages are exactly as they were at the instant of the crash. That is a clean base for undo, which then rolls back each loser's changes, newest first, restoring the before-state. Skipping the losers' redo would leave pages in a state undo does not expect. Redo is safe to repeat because each record says "put this slot in this state", not "add 5".
>
> - What state are the pages in after redo, and why does that help?
> - Why must redo be idempotent?
> - In what order does undo go, and why?

> [!CHECK] The machine dies again in the middle of recovery. Why is it safe to run recovery a second time, and what would break if undo steps were not logged?
> ||Redo repeats history, so a second recovery repeats everything the first one did, including the undo steps already done: they were logged as changes with before and after swapped, so they are replayed like any other record, and only the remaining undo work is left. Without that logging a second recovery would not know which of a loser's changes were already rolled back and would undo them again, possibly on top of newer work. Logging the undo is what makes recovery make progress even if it keeps being interrupted.
>
> - What exactly is logged when a loser is undone?
> - What does a second recovery redo?
> - Which bug does the "crash between redo and undo" test catch?

> [!CHECK] The last record in the log is half written. How does recovery know, and what must happen to the log afterwards?
> ||Every record carries a length and a checksum, so a record that is cut short or damaged fails the check and is treated as the end of the log; everything before it is intact. Afterwards new records must be appended where the last whole record ended, not after the garbage, or the next recovery would stop at the garbage and lose them. The garbage-tail test checks exactly that: a new transaction after recovery survives another crash.
>
> - What stops recovery from reading garbage as a record?
> - Where does the next record go?
> - What would a missing checksum let through?

> [!CHECK] Compare a sharp checkpoint with a fuzzy one: what does each cost while the database runs, and what does each save at restart?
> ||A sharp checkpoint writes every dirty page and then logs a record listing the active transactions: simple, and redo starts at the checkpoint, but it stops the world while the pages are written. A fuzzy checkpoint does not stop anything: it logs the set of pages that were dirty and the transactions that were active, and restart redoes from the oldest change that might not have reached the disk. Normal operation is smoother, restart does more work, and the bookkeeping (a dirty page table) is the price. Either way, losers that were active at the checkpoint still have changes before it, so undo must read back to their first record.
>
> - What is guaranteed to be on the disk after a sharp checkpoint?
> - Where does redo start in each design?
> - Why must undo still look before the checkpoint?

> [!CHECK] Back of the envelope. A database commits 20 000 transactions a second and one `fsync` of its log takes 1 ms. How can that be, and what does the latency of one commit look like?
> ||One device can do at most 1 000 syncs a second at 1 ms each, so 20 000 commits a second means each sync must cover about 20 commits: **group commit**. While one sync is in flight, the commits that arrive wait; the next sync makes all of them durable together. A commit's latency is therefore between 1 and 2 ms: it may wait for the sync already running, then for its own. Throughput is limited by how many commits share a sync, not by the sync itself, and the price is a little latency for every commit.
>
> - How many syncs per second does one device allow?
> - How many commits must share each one?
> - What is the best and the worst latency of a single commit?

## Write it up

Write `notes/recovery.md` in your repo, one page, in your own words:

- **The design.** What a log record holds, where you flush the log, and how you chose which pages to write and when.
- **A measurement.** How many crash points the bank script tries, and what the flush modes (no pages, every step, every other step) found or ruled out. One sentence on what the numbers say.
- **A bug.** One failure you hit, how the crash-at-every-moment test or the shrunk property showed it, and the fix.
- **A limit.** One thing your recovery cannot do, such as fuzzy checkpoints or group commit, and what you would add.

Then read it aloud in three minutes. If a sentence needs the code open to make sense, rewrite it.

## Other designs

None for this stage. The *Other designs* sections of 4c-01 to 4c-07 list the alternatives to compare with yours.

## In BusTub

BusTub's 2025 projects stop before recovery: Project 4 is concurrency control. The lectures on logging and recovery (CMU 15-445) teach the write-ahead log and ARIES, and BusTub's older years had a `LogManager` and `LogRecovery` that students filled in. This module is new in this course: a small store that logs before it writes, and recovery you can crash at every step.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| kill -9 the server in a shell script and diff the data | `disk.crash()` and `World::restart` in a `#[test]` |
| a `LOG_DEBUG` of every record | `log.records()` in the test |

**Port rule:** crash testing is the same idea (kill, restart, compare), done in-process with a disk you can copy.

## Learn more

- *Testing for crash consistency*: [ALICE (OSDI 2014)](https://www.usenix.org/conference/osdi14/technical-sessions/presentation/pillai) · [SQLite's testing](https://www.sqlite.org/testing.html) (it crashes at every I/O)
