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
