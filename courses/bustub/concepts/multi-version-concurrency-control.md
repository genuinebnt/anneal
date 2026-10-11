---
title: Multi-version concurrency control: readers that never wait
summary: Why a database keeps several versions of a row, how a timestamp tells a reader which one it may see, and the three ways to store the old versions (BusTub keeps the newest in the table and the older ones as undo logs).
minutes: 8
---
Two transactions touch the same row: one is adding up a balance sheet, the other is moving money. With **locks**, the reader waits for the writer or the writer waits for the reader, and a long report stalls every update it touches. **Multi-version concurrency control (MVCC)** avoids the wait by never overwriting what somebody may still be reading: a write makes a **new version** of the row, and a reader looks at the version that was current when *it* began.

```svg
caption: One row, three versions. Each transaction reads the newest version committed at or before its own read timestamp; the writer's uncommitted version is invisible to everyone else.
<svg viewBox="0 0 760 200" role="img" aria-label="A timeline of commit timestamps with the version of a row each transaction sees">
<line class="ln" x1="30" y1="90" x2="730" y2="90"/>
<g><rect class="live" x="60" y="62" width="150" height="56" rx="3"/><text class="mid fg" x="135" y="86">balance = 100</text><text class="mid dim sm" x="135" y="106">committed at ts 1</text></g>
<g><rect class="live" x="270" y="62" width="150" height="56" rx="3"/><text class="mid fg" x="345" y="86">balance = 80</text><text class="mid dim sm" x="345" y="106">committed at ts 3</text></g>
<g><rect class="hot" x="480" y="62" width="150" height="56" rx="3"/><text class="mid fg" x="555" y="86">balance = 50</text><text class="mid dim sm" x="555" y="106">txn 9, not committed</text></g>
<text class="mid blue sm" x="135" y="160">reader, read ts 2 → 100</text>
<text class="mid blue sm" x="345" y="160">reader, read ts 3 → 80</text>
<text class="mid blue sm" x="555" y="160">reader, read ts 5 → 80</text>
<text class="mid dim sm" x="555" y="182">only txn 9 sees 50</text>
</svg>
```

## The three parts of any MVCC design

1. **A timestamp on every version**: when it was committed (a number from a counter, see *logical clocks*). Until its writer commits, a version carries a *temporary* timestamp that no reader can mistake for a commit time.
2. **A rule for what a reader sees**: the newest version with a timestamp at or before the reader's **read timestamp**, or one the reader itself wrote. The read timestamp is fixed when the transaction begins, so it sees one consistent *snapshot* however long it runs.
3. **A place for the old versions**, and a way to find them from the newest.

Part 2 is what makes reads lock-free: a reader never changes anything, never waits for a writer, and a writer never waits for a reader. Writers still conflict with other writers; that is the next article (*snapshot isolation*).

## Where the old versions live

| design | newest version | older versions | used by |
|---|---|---|---|
| **append-only** | in the table, with the others | in the table, in the same heap | PostgreSQL |
| **time travel** | in the table | in a separate table of old rows | SQL Server's version store |
| **delta / undo log** | in the table, **updated in place** | as small *deltas* in per-transaction undo logs, linked newest to oldest | MySQL InnoDB, HyPer; **BusTub** |

The delta design makes the common case cheap: most reads want the newest version, and it is right there in the table, where an index also points. A reader that needs an older one follows the chain of undo logs and *undoes* changes until it reaches the version it may see. That chain is the next article.

## The costs

- **Space.** Old versions pile up until nobody can need them: that is **garbage collection**, driven by the *watermark* (the oldest read timestamp still running).
- **Time for old readers.** A long-running reader has to walk long chains.
- **Weaker isolation than it looks.** A snapshot is not serializable by itself (*write skew*); stronger levels add validation.

## Invariants: what must always be true

- **A version's timestamp is a commit time, or a temporary one.** Until its writer commits it carries a temporary timestamp that no reader can mistake for a commit time, and only its writer can see it.
- **A reader sees the newest version at or before its read timestamp,** or its own writes. The read timestamp is fixed when the transaction begins, so the whole transaction sees one snapshot.
- **The chain is ordered.** Walking from the newest version meets timestamps that only go down, so the first visible version is the right one.
- **Readers never wait for writers, and writers never wait for readers.** Writers conflict only with other writers.
- **An old version is kept while some active transaction may still need it,** and no longer: the oldest read timestamp of any active transaction is the line below which versions can be collected.
- **An aborted writer leaves nothing visible.** No version keeps its temporary timestamp after the abort.

## C++ comparison

| C / C++ | Rust |
|---|---|
| a version = `struct { timestamp_t ts; Tuple tuple; Version *next; }` with raw `next` pointers | a version = a value in a `Vec` owned by its transaction; the "pointer" is a plain `(txn id, index)` pair that can be checked (and may dangle after garbage collection, returning `None`) |
| `std::atomic<timestamp_t>` read by every reader | `AtomicI64` with `Ordering::SeqCst` (or a `Mutex`) |

**Port rule:** replace a pointer to a version by an id you look up in a map; "the version is gone" becomes an `Option`, not a crash.

## In real code

### Using it: a versioned cell

```rust test
/// A value with its history: `(commit timestamp, value)`, oldest first.
#[derive(Default)]
struct Cell {
    versions: Vec<(i64, i32)>,
}

impl Cell {
    /// A committed write at `ts` (timestamps grow).
    fn write(&mut self, ts: i64, value: i32) {
        assert!(self.versions.last().map_or(true, |&(last, _)| last < ts));
        self.versions.push((ts, value));
    }

    /// The newest version committed at or before `read_ts`.
    fn read(&self, read_ts: i64) -> Option<i32> {
        self.versions.iter().rev().find(|&&(ts, _)| ts <= read_ts).map(|&(_, v)| v)
    }
}

#[test]
fn a_reader_sees_the_version_at_its_read_timestamp() {
    let mut balance = Cell::default();
    balance.write(1, 100);
    balance.write(3, 80);
    assert_eq!(balance.read(2), Some(100));
    assert_eq!(balance.read(3), Some(80));
    assert_eq!(balance.read(99), Some(80));
}

#[test]
fn a_row_did_not_exist_before_its_first_version() {
    let mut balance = Cell::default();
    balance.write(5, 1);
    assert_eq!(balance.read(4), None);
}
```

### Using it: a snapshot does not move

```rust test
use std::sync::{Arc, Mutex};

struct Db {
    // (commit ts, value) pairs and the last commit timestamp
    rows: Mutex<(Vec<(i64, i32)>, i64)>,
}

impl Db {
    fn begin(&self) -> i64 {
        self.rows.lock().unwrap().1 // read ts = the newest commit
    }
    fn read(&self, read_ts: i64) -> i32 {
        let guard = self.rows.lock().unwrap();
        guard.0.iter().rev().find(|&&(ts, _)| ts <= read_ts).unwrap().1
    }
    fn commit_write(&self, value: i32) {
        let mut guard = self.rows.lock().unwrap();
        guard.1 += 1;
        let ts = guard.1;
        guard.0.push((ts, value));
    }
}

#[test]
fn a_long_reader_is_not_disturbed_by_commits() {
    let db = Arc::new(Db { rows: Mutex::new((vec![(0, 10)], 0)) });
    let reader = db.begin();
    let writer = {
        let db = db.clone();
        std::thread::spawn(move || {
            for v in 11..=15 {
                db.commit_write(v);
            }
        })
    };
    writer.join().unwrap();
    assert_eq!(db.read(reader), 10, "the snapshot is the one from when it began");
    assert_eq!(db.read(db.begin()), 15, "a new reader sees the latest");
}
```

### In the exercises

- **4a-02:** the transaction manager hands out the read timestamp (`begin`) and the commit timestamp (`commit`) that make the rule above work.
- **4a-05:** the sequential scan applies the rule to every tuple of a table.
- **Module 4b:** writes that leave a version behind, and the garbage collector that removes versions nobody can read.

### Where it is used

- **PostgreSQL**: every `UPDATE` writes a new row version; `xmin`/`xmax` say which transactions created and deleted it, and `VACUUM` reclaims dead versions.
- **MySQL InnoDB**: the clustered index holds the newest version; older ones are rebuilt from undo logs by a *read view*.
- **HyPer** (Neumann, Mühlbauer, Kemper, "Fast Serializable Multi-Version Concurrency Control for Main-Memory Database Systems", SIGMOD 2015): in-place updates with undo-buffer chains, the design BusTub's project follows.
- **SQL Server** snapshot isolation and **Oracle**'s read consistency also serve readers from older versions rather than locking.
