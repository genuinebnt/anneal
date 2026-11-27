---
title: Reader-writer latches: many readers or one writer
summary: What an RwLock promises and does not, why read-then-write upgrades deadlock, and where BusTub's page latches and the B+ tree's latch crabbing come from.
minutes: 8
---
A `Mutex` lets one thread in at a time. Most shared data is read far more often than it is written, and readers do not conflict with each other. A **reader-writer lock** (BusTub calls it a *latch*) lets any number of readers in together, or exactly one writer alone.

## The contract

| held by | a new reader | a new writer |
|---|---|---|
| nobody | gets in | gets in |
| one or more readers | gets in (*usually*) | waits until they all leave |
| a writer | waits | waits |

Rust's `RwLock<T>` follows it, in the same ownership-based shape as `Mutex<T>`: `lock.read()` returns a guard that derefs to `&T`, `lock.write()` returns one that derefs to `&mut T`, and dropping the guard releases the latch. The compiler therefore stops you from writing through a read guard.

```rust
pub struct ReaderWriterLatch<T> { inner: RwLock<T> }

impl<T> ReaderWriterLatch<T> {
    pub fn read(&self)  -> RwLockReadGuard<'_, T>  { self.inner.read().unwrap_or_else(PoisonError::into_inner) }
    pub fn write(&self) -> RwLockWriteGuard<'_, T> { self.inner.write().unwrap_or_else(PoisonError::into_inner) }
}
```

BusTub's `ReaderWriterLatch` has four calls (`RLock`, `RUnlock`, `WLock`, `WUnlock`) wrapped around a `std::shared_mutex`, and every code path must pair them. In Rust the pair collapses into a guard's lifetime: there is no `RUnlock` to forget.

```svg
caption: Readers overlap with each other; the writer waits for all of them and then has the latch to itself. A new reader that arrives while the writer holds it waits.
<svg viewBox="0 0 760 230" role="img" aria-label="Timelines of two readers, a writer and a late reader on one latch">
<text class="big" x="20" y="48">reader A</text><text class="big" x="20" y="92">reader B</text><text class="big" x="20" y="136">writer</text><text class="big" x="20" y="180">reader C</text>
<rect class="live" x="130" y="30" width="190" height="26" rx="4"/><text class="mid t-g sm" x="225" y="48">read guard</text>
<rect class="live" x="190" y="74" width="190" height="26" rx="4"/><text class="mid t-g sm" x="285" y="92">read guard</text>
<rect class="never" x="230" y="118" width="150" height="26" rx="4"/><text class="mid dim sm" x="305" y="136">waits for A and B</text>
<rect class="bad" x="380" y="118" width="150" height="26" rx="4"/><text class="mid t-r sm" x="455" y="136">write guard</text>
<rect class="never" x="440" y="162" width="90" height="26" rx="4"/><text class="mid dim sm" x="485" y="180">waits</text>
<rect class="live" x="530" y="162" width="140" height="26" rx="4"/><text class="mid t-g sm" x="600" y="180">read guard</text>
<line class="ln dash" x1="380" y1="24" x2="380" y2="196" style="stroke:var(--warn)"/><text class="t-w sm" x="386" y="22">A and B release</text>
<text class="dim sm" x="130" y="218">time &#8594;</text>
</svg>
```

## What the lock does *not* promise

- **Fairness.** If readers keep arriving, a waiting writer can starve; if a waiting writer blocks *new* readers, readers can stall behind it. The standard library documents that the policy depends on the operating system and that no particular one is guaranteed. Do not write code whose correctness depends on who goes first.
- **Upgrades.** There is no "turn my read latch into a write latch". Releasing the read latch and then taking the write latch leaves a gap in which another writer may have changed the data, so everything you read earlier must be re-checked. Trying to take the write latch *while holding* the read latch deadlocks: you are waiting for readers to leave, and you are one of them.
- **Recursive reads.** A thread that holds a read latch and takes another read latch on the same lock may deadlock if a writer is waiting in between (the writer blocks the second read, the first read blocks the writer). Take a lock once, pass the guard down.

## Never take a read latch you already hold

Two readers can share a page, so taking a read latch twice on the same page in one thread looks harmless. It is not. Most implementations queue a waiting writer ahead of new readers (otherwise a steady stream of readers would starve it). Then:

1. thread A holds a read latch on page P;
2. thread W asks for a write latch on P and waits for A to leave;
3. thread A asks for a read latch on P again: it queues behind W, which waits for A.

Nobody can move. The bug shows once in a few hundred runs, only under contention, and only if the writer arrives between the two read latches, which is why a test that passes ten times proves nothing. The usual place is a helper that returns a guard to a caller who then calls another method that latches the same page again (a scan that starts at a leaf it still holds). The rule: **drop the first guard before latching the page again**, or pass the guard down instead of the page id.

## Poisoning, again

If a thread panics holding the *write* guard, the lock is poisoned, as with a mutex. A panic holding a read guard does not poison it (the data cannot have been changed). The reference `ReaderWriterLatch` uses `unwrap_or_else(PoisonError::into_inner)` to ignore the poison flag: a deliberate choice for a latch whose protected data is plain bytes, made explicit in code.

## Where this goes next

A page in the buffer pool has one of these latches, and the B+ tree in module 2e walks from the root to a leaf *holding a latch on each page it passes*, releasing a parent only when the child is known to be **safe** (it cannot split or merge). That protocol is called **latch crabbing**, and it only works because latches are cheap, scoped by a guard, and acquired in a fixed order: parent before child, left before right.

| | C++ | Rust |
|---|---|---|
| the type | `std::shared_mutex` | `RwLock<T>` |
| read | `std::shared_lock l(m);` | `let g = lock.read().unwrap();` |
| write | `std::unique_lock l(m);` | `let g = lock.write().unwrap();` |
| try without blocking | `try_lock_shared()` | `try_read()` |
| protects the data | no, by convention | yes: the data is inside |

## In real code

### The API you will use

| call | what it does | when |
|---|---|---|
| `RwLock::new(v)` | wraps `v` | construction |
| `l.read().unwrap()` | shared guard (`Deref<Target = T>`); many at once | reading |
| `l.write().unwrap()` | exclusive guard (`DerefMut`); waits for all readers | modifying |
| `l.try_read()` / `l.try_write()` | no waiting: `Err(WouldBlock)` if unavailable | back off or skip |
| `l.get_mut()` / `l.into_inner()` | access without locking when you own it | setup and teardown |
| `Arc<RwLock<T>>` | the shared form | configuration, caches, indexes |

```rust test
use std::sync::{Arc, RwLock};
use std::thread;

#[test]
fn many_readers_one_writer() {
    let table = Arc::new(RwLock::new(vec![1, 2, 3]));
    let readers: Vec<_> = (0..4)
        .map(|_| {
            let t = Arc::clone(&table);
            thread::spawn(move || t.read().unwrap().iter().sum::<i32>())     // shared: these run at the same time
        })
        .collect();
    for r in readers {
        assert_eq!(r.join().unwrap(), 6);
    }
    table.write().unwrap().push(4);                                          // exclusive: waits for any reader still in
    assert_eq!(table.read().unwrap().len(), 4);
}
```

```rust test
use std::sync::RwLock;

#[test]
fn try_variants_and_the_upgrade_trap() {
    let l = RwLock::new(0);
    let r1 = l.read().unwrap();
    let r2 = l.read().unwrap();                         // a second reader: fine
    assert!(l.try_write().is_err());                    // a writer would have to wait for r1 and r2
    drop((r1, r2));
    assert!(l.try_write().is_ok());

    // "Upgrading" means giving up the read latch first, and the data may change in the gap. Re-check after you re-acquire.
    let seen = *l.read().unwrap();
    *l.write().unwrap() += 1;                           // not atomic with the read above
    assert_eq!(*l.read().unwrap(), seen + 1);
}
```

```rust test
use std::sync::{PoisonError, RwLock};
use std::thread;
use std::sync::Arc;

#[test]
fn a_writer_that_panics_poisons_the_latch() {
    let l = Arc::new(RwLock::new(5));
    let l2 = Arc::clone(&l);
    let _ = thread::spawn(move || {
        let _g = l2.write().unwrap();
        panic!("died holding the write latch");
    })
    .join();
    assert!(l.read().is_err());                                     // poisoned
    let v = *l.read().unwrap_or_else(PoisonError::into_inner);       // the page-latch choice: use the bytes anyway
    assert_eq!(v, 5);
}
```

### In the exercises

- **1b-05 (`ReaderWriterLatch`):** a thin wrapper over `RwLock<T>` whose `read()` and `write()` ignore poisoning with `unwrap_or_else(PoisonError::into_inner)` (the third example), because the data is plain page bytes.
- **1g-01 (page guards):** `ReadPageGuard` holds an `RwLockReadGuard`, `WritePageGuard` an `RwLockWriteGuard`. You will store each in an `Option`, so `release()` can `take()` and drop it *before* unpinning.
- **2b-02, 2b-04 (insert and remove):** the directory is taken with `write_page` from the start, because the operation may need to change it, and the header is released as soon as the directory is held.
- **2e-01, 2e-02 (the concurrent B+ tree):** readers share read latches down the tree; a writer's fast path read-latches the path and write-latches only the leaf, which it takes by dropping the leaf's read latch while the parent's read latch is still held, because a `RwLock` read guard cannot be upgraded.

### Where it is used

- **Page latches in every buffer pool**: PostgreSQL's per-buffer content lock and InnoDB's block latches let many readers scan a page while one writer excludes them.
- **B+ tree traversal** (module 2e): readers hold shared latches down the path; inserts take exclusive latches only where they might split.
- **Read-mostly shared state** in services: routing tables, feature flags, schema catalogs (read on every request, rewritten rarely).
- Not for: write-heavy data (readers and writers fight; use a mutex or shard the data), and not as a way to get a "free" upgrade.
