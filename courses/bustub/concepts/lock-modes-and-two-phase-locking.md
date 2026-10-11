---
title: Lock modes and two-phase locking
summary: Why a database has more than one kind of lock, which pairs can be held together, how intention locks let a table and its rows be locked at different sizes, and the one rule (grow, then shrink) that makes locking serializable.
minutes: 12
---
Two transactions both want to change account A. If both read the balance, add 10 and write it back at the same time, one of the additions is lost. The simplest cure is a lock: whoever holds it may touch A, everyone else waits. That works, and it is far too slow: two transactions that only *read* A are made to queue behind each other for no reason.

So databases have **lock modes**. A **shared** lock (S) says "I am reading"; any number of transactions can hold it together. An **exclusive** lock (X) says "I am changing it"; nobody else may hold anything. That already gives the rule most programmers know from reader-writer locks.

## Locking at two sizes

A database can lock a whole table or one row of it. Locking only rows is cheap for small changes but a scan of the table would need a million row locks; locking only the table is cheap for scans but makes every writer wait for every other. Real systems offer both, and a new problem appears: a transaction that holds a shared lock on the whole table must not be allowed to have a writer change one of its rows. How does the table lock know about the row locks below it?

The answer is **intention locks**. Before locking a row, a transaction first takes an intention lock on the table: **IS** ("I will read some rows"), **IX** ("I will change some rows") or **SIX** ("I read the whole table and will change some rows"). Intention locks on the table do not lock anything; they are a sign on the door. Two writers of different rows both hold IX on the table and do not block each other; a reader of the whole table (S) does conflict with IX, because somebody inside is about to change a row.

| held \ asked | IS | IX | S | SIX | X |
|---|---|---|---|---|---|
| **IS** | yes | yes | yes | yes | no |
| **IX** | yes | yes | no | no | no |
| **S** | yes | no | yes | no | no |
| **SIX** | yes | no | no | no | no |
| **X** | no | no | no | no | no |

The table is symmetric, and the strength of a mode is the set of things it keeps out: X keeps out everything, IS keeps out only X. A transaction may **upgrade** a lock it holds to a stronger mode (S to X, IS to anything above it) but never to a weaker one.

## Two-phase locking

Locks alone do not make transactions correct. Suppose a transaction takes the lock on A, reads it, *releases* it, and only then takes the lock on B. In the gap another transaction can change A, and the first transaction acts on a value that is no longer true. **Two-phase locking (2PL)** is the rule that closes the gap:

> A transaction has a **growing** phase, in which it only acquires locks, and then a **shrinking** phase, in which it only releases them. Once it has released one lock, it never acquires another.

If every transaction obeys 2PL, every outcome is equal to some one-at-a-time order of the transactions (**serializable**). **Strict** 2PL goes further and keeps every exclusive lock until commit or abort, which also means nobody ever reads a value that a transaction later rolls back.

The **isolation levels** are 2PL with the rule loosened on purpose, to let more transactions run at once. Under *read uncommitted* a transaction never takes shared locks (it can see changes that are not committed yet). Under *read committed* it takes shared locks but gives them back right after reading. Under *repeatable read* it keeps everything until the end. The price of each loosening is an anomaly that the stronger level does not have.

```svg
caption: The number of locks a transaction holds over time, under two-phase locking. It only grows until the lock point, then only shrinks. Strict two-phase locking (dashed) holds its exclusive locks until the end and releases them together at commit or abort. Acquiring a lock after the first release (red) is what the rule forbids. (Animated: the three phases in turn.)
<svg viewBox="0 0 760 320" role="img" aria-label="A step curve of held locks: growing to a lock point, a plateau, then shrinking; a dashed strict variant and a forbidden late acquire">
<style>
@keyframes tp-1{0%,26%{opacity:.5}32%,100%{opacity:0}}
@keyframes tp-2{0%,30%{opacity:0}36%,60%{opacity:.5}66%,100%{opacity:0}}
@keyframes tp-3{0%,64%{opacity:0}70%,94%{opacity:.5}100%{opacity:0}}
.tp-1{opacity:0;animation:tp-1 12s ease-in-out infinite}.tp-2{opacity:0;animation:tp-2 12s ease-in-out infinite}.tp-3{opacity:0;animation:tp-3 12s ease-in-out infinite}
</style>
<rect class="tp-1" x="100" y="40" width="190" height="200" rx="4" style="fill:var(--grn-bg)"/>
<rect class="tp-2" x="290" y="40" width="210" height="200" rx="4" style="fill:var(--acc-bg)"/>
<rect class="tp-3" x="500" y="40" width="130" height="200" rx="4" style="fill:var(--warn-bg)"/>
<path class="grid" d="M60 240 H730"/><path class="grid" d="M60 40 V240"/>
<text class="dim sm" x="20" y="244">0</text><text class="dim sm" x="20" y="204">1</text><text class="dim sm" x="20" y="164">2</text><text class="dim sm" x="20" y="124">3</text>
<text class="dim sm" x="62" y="30">locks held</text><text class="end dim sm" x="730" y="262">time</text>
<path style="stroke:var(--fn);stroke-width:2.4;fill:none" d="M60 240 H110 V200 H200 V160 H290 V120 H500 V160 H560 V200 H620 V240 H730"/>
<path style="stroke:var(--grn);stroke-width:1.8;fill:none;stroke-dasharray:6 4" d="M290 118 H640 V240"/>
<path style="stroke:var(--bad);stroke-width:1.8;fill:none;stroke-dasharray:3 3" d="M560 200 V160 H600"/>
<text class="t-r sm" x="566" y="150">✗ never</text>
<text class="t-g sm" x="110" y="276">growing phase: only acquire</text>
<text class="t-a sm" x="298" y="108">the lock point: everything is held, the work happens here</text>
<text class="t-w sm" x="470" y="276">shrinking phase: only release</text>
<text class="dim sm" x="60" y="300">Strict 2PL (dashed): exclusive locks wait for commit or abort, so nobody reads what is later rolled back.</text>
</svg>
```

## What can go wrong

- **Starvation.** If readers may always join existing readers, a stream of readers keeps a writer waiting forever. Fair queues grant requests in arrival order, and a reader that is compatible with the holders still waits behind a writer that is waiting.
- **Deadlock.** Two transactions each hold a lock the other wants. Locking never fixes this by itself; see the waits-for graph concept.
- **Upgrade deadlock.** Two transactions hold S on the same row and both ask for X: each waits for the other to let go. Systems allow only one upgrade at a time and abort the other.

## Invariants: what must always be true

- **Compatibility is symmetric, and X is compatible with nothing.** If a held mode and a requested mode conflict one way, they conflict the other way too.
- **Two-phase.** A transaction never acquires a lock after it has released one. Break it, and the gap between two locks is a window for another transaction to change what the first one already read.
- **Strict.** Exclusive locks are kept until the transaction commits or aborts. Break it, and another transaction can read a value that is later rolled back.
- **Intention before detail.** Holding S or X on a row means holding IS or IX (or stronger) on its table first. Break it, and a table lock cannot know a writer is working inside.
- **Upgrades only go up,** and at most one upgrade per resource waits at a time. Break the second, and two shared holders that both ask for X wait for each other forever.
- **Requests are granted in arrival order,** unless a request is compatible with every holder and every request ahead of it. Break it, and a stream of readers starves a writer.

## C++ comparison

| C / C++ | Rust |
|---|---|
| `std::shared_mutex` (shared and exclusive only) | `RwLock<T>`, and your own manager for the other modes |
| a lock manager keyed by `RID` with `std::mutex` and `std::condition_variable` | a `Mutex<State>` and a `Condvar` |
| `throw TransactionAbortException` | a `Result<_, LockError>` the caller must handle |

## In real code

### Using it: modes and the compatibility rule

```rust test
#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    IS,
    IX,
    S,
    SIX,
    X,
}
use Mode::*;

fn compatible(a: Mode, b: Mode) -> bool {
    match (a, b) {
        (X, _) | (_, X) => false,
        (IS, _) | (_, IS) => true,
        (IX, IX) | (S, S) => true,
        _ => false,
    }
}

/// Who holds a lock right now. A request is granted only when it fits with everyone else.
#[derive(Default)]
struct Lock {
    holders: Vec<(u32, Mode)>,
}

impl Lock {
    fn try_acquire(&mut self, txn: u32, mode: Mode) -> bool {
        if self.holders.iter().any(|&(t, m)| t != txn && !compatible(m, mode)) {
            return false;
        }
        self.holders.retain(|&(t, _)| t != txn);
        self.holders.push((txn, mode));
        true
    }
}

#[test]
fn readers_share_and_a_writer_is_alone() {
    let mut l = Lock::default();
    assert!(l.try_acquire(1, S));
    assert!(l.try_acquire(2, S));
    assert!(!l.try_acquire(3, X), "a writer must wait for the readers");
    let mut l = Lock::default();
    assert!(l.try_acquire(1, X));
    assert!(!l.try_acquire(2, S));
}

#[test]
fn two_row_writers_share_a_table_but_a_table_reader_does_not() {
    let mut table = Lock::default();
    assert!(table.try_acquire(1, IX));
    assert!(table.try_acquire(2, IX), "different rows: both announce a write");
    assert!(!table.try_acquire(3, S), "reading the whole table while rows are being changed");
    assert!(table.try_acquire(4, IS), "reading a few rows is fine");
}
```

### In the exercises

- **4d-01** writes the compatibility table and the upgrade rule above.
- **4d-02** is the `Lock` of this example, with holders and the upgrade case.
- **4d-03 to 4d-05** add waiting in a fair queue, upgrades, and the growing and shrinking phases per isolation level.

### Where it is used

PostgreSQL's table locks, MySQL InnoDB's row and intention locks, and SQL Server's lock hierarchy all use these modes. BusTub's lock manager (the 2022 course) is the same table with the same five modes.
