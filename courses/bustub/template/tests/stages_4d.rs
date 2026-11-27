//! Tests for module 4d, the lock manager. A test name starts with its stage: `s4d_03_…` belongs to stage 4d-03.
//!
//! Stages 1 and 2 are exact tables and a model; stages 3 and 4 start threads and look at who waits and who goes first (a thread that waits
//! forever fails the test after ten seconds instead of hanging it); stage 5 checks the rules of each isolation level; stage 6 builds the
//! waits-for graph and breaks real deadlocks; the last stage runs workloads of many threads and checks the history they leave.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use proptest::prelude::*;

use bustub::common::config::PageId;
use bustub::common::rid::Rid;
use bustub::concurrency::deadlock::{DeadlockDetector, WaitsForGraph};
use bustub::concurrency::lock_error::{LockError, LockErrorKind};
use bustub::concurrency::lock_manager::{LockManager, Resource};
use bustub::concurrency::lock_mode::{can_upgrade, compatible, LockMode};
use bustub::concurrency::lock_queue::LockQueue;
use bustub::concurrency::lock_txn::{LockIsolation, LockState, LockTxn};
use bustub::concurrency::transaction::TxnId;

use LockMode::*;

const ALL: [LockMode; 5] = [IntentionShared, IntentionExclusive, Shared, SharedIntentionExclusive, Exclusive];

fn config() -> ProptestConfig {
    ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() }
}

fn any_mode() -> impl Strategy<Value = LockMode> {
    prop::sample::select(ALL.to_vec())
}

/// The textbook compatibility table, written out independently of the code under test. Rows and columns: IS, IX, S, SIX, X.
fn truth(a: LockMode, b: LockMode) -> bool {
    const T: [[bool; 5]; 5] = [
        [true, true, true, true, false],
        [true, true, false, false, false],
        [true, false, true, false, false],
        [true, false, false, false, false],
        [false, false, false, false, false],
    ];
    let ix = |m: LockMode| ALL.iter().position(|&x| x == m).unwrap();
    T[ix(a)][ix(b)]
}

/// Runs `f` on its own thread and fails the test, instead of hanging, if it does not finish: a thread that waits for a lock that is never
/// granted or a wake-up that never comes.
fn finish_within<T: Send + 'static>(what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)));
    });
    match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(Ok(value)) => value,
        Ok(Err(panic)) => std::panic::resume_unwind(panic),
        Err(_) => panic!("{what}: still going after 20 seconds. A lock request is waiting for a grant that never comes, or a thread was never woken."),
    }
}

fn pause() {
    thread::sleep(Duration::from_millis(80));
}

fn table(n: u32) -> Resource {
    Resource::Table(n)
}

fn rid(n: u32) -> Rid {
    Rid::new(PageId(0), n)
}

fn row(t: u32, n: u32) -> Resource {
    Resource::Row(t, rid(n))
}

fn kind<T: std::fmt::Debug>(r: Result<T, LockError>) -> LockErrorKind {
    r.expect_err("expected an error").kind
}

/// Starts `f` on a thread and returns a flag that says whether it has finished, and the handle.
fn spawn_flag<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> (Arc<AtomicBool>, thread::JoinHandle<T>) {
    let done = Arc::new(AtomicBool::new(false));
    let d = Arc::clone(&done);
    let h = thread::spawn(move || {
        let r = f();
        d.store(true, Ordering::SeqCst);
        r
    });
    (done, h)
}

fn finished(flag: &AtomicBool) -> bool {
    flag.load(Ordering::SeqCst)
}

// ---- 4d-01: lock modes -------------------------------------------------------------------------------------------------------------------

#[test]
fn s4d_01_the_compatibility_table_is_the_textbook_one() {
    for &a in &ALL {
        for &b in &ALL {
            assert_eq!(compatible(a, b), truth(a, b), "compatible({a:?}, {b:?})");
        }
    }
}

#[test]
fn s4d_01_an_exclusive_lock_is_compatible_with_nothing() {
    for &m in &ALL {
        assert!(!compatible(Exclusive, m), "X with {m:?}");
        assert!(!compatible(m, Exclusive), "{m:?} with X");
    }
}

#[test]
fn s4d_01_intention_shared_only_conflicts_with_exclusive() {
    for &m in &ALL {
        assert_eq!(compatible(IntentionShared, m), m != Exclusive, "IS with {m:?}");
    }
}

#[test]
fn s4d_01_readers_and_intending_writers_do_not_mix() {
    assert!(compatible(Shared, Shared));
    assert!(compatible(IntentionExclusive, IntentionExclusive), "two writers of different rows can share a table");
    assert!(!compatible(Shared, IntentionExclusive), "a reader of the whole table and a writer of one row of it conflict");
    assert!(!compatible(SharedIntentionExclusive, SharedIntentionExclusive));
    assert!(!compatible(SharedIntentionExclusive, Shared));
}

#[test]
fn s4d_01_only_a_stronger_mode_is_an_upgrade() {
    let expected: [(LockMode, Vec<LockMode>); 5] = [
        (IntentionShared, vec![Shared, IntentionExclusive, SharedIntentionExclusive, Exclusive]),
        (IntentionExclusive, vec![SharedIntentionExclusive, Exclusive]),
        (Shared, vec![SharedIntentionExclusive, Exclusive]),
        (SharedIntentionExclusive, vec![Exclusive]),
        (Exclusive, vec![]),
    ];
    for (from, tos) in expected {
        for &to in &ALL {
            assert_eq!(can_upgrade(from, to), tos.contains(&to), "can_upgrade({from:?}, {to:?})");
        }
    }
}

proptest! {
    #![proptest_config(config())]

    /// Property: compatibility is symmetric (it describes two holders, not an order).
    #[test]
    fn s4d_01_property_compatibility_is_symmetric(a in any_mode(), b in any_mode()) {
        prop_assert_eq!(compatible(a, b), compatible(b, a));
    }

    /// Property: an upgrade only strengthens: whatever is compatible with the new mode was compatible with the old one, and there is no way back.
    #[test]
    fn s4d_01_property_an_upgrade_only_strengthens(a in any_mode(), b in any_mode(), c in any_mode()) {
        prop_assert!(!can_upgrade(a, a), "the same mode is not an upgrade");
        if can_upgrade(a, b) {
            prop_assert!(!can_upgrade(b, a));
            if compatible(b, c) {
                prop_assert!(compatible(a, c), "{:?} -> {:?}: {:?} fits the new mode but not the old", a, b, c);
            }
        }
    }
}

// ---- 4d-02: one lock, no waiting ---------------------------------------------------------------------------------------------------------

#[test]
fn s4d_02_a_new_lock_has_no_holders() {
    let q = LockQueue::new();
    assert!(q.holders().is_empty());
    assert!(q.is_empty());
    assert_eq!(q.mode_of(1), None);
}

#[test]
fn s4d_02_compatible_requests_share_the_lock() {
    let mut q = LockQueue::new();
    assert_eq!(q.try_acquire(2, Shared), Ok(true));
    assert_eq!(q.try_acquire(1, Shared), Ok(true));
    assert_eq!(q.try_acquire(3, IntentionShared), Ok(true));
    assert_eq!(q.holders(), vec![(1, Shared), (2, Shared), (3, IntentionShared)], "holders come back ordered by transaction id");
    assert_eq!(q.mode_of(2), Some(Shared));
}

#[test]
fn s4d_02_an_incompatible_request_is_refused_and_changes_nothing() {
    let mut q = LockQueue::new();
    assert_eq!(q.try_acquire(1, Exclusive), Ok(true));
    assert_eq!(q.try_acquire(2, Shared), Ok(false));
    assert_eq!(q.try_acquire(2, IntentionShared), Ok(false));
    assert_eq!(q.holders(), vec![(1, Exclusive)]);
    assert_eq!(q.mode_of(2), None);
}

#[test]
fn s4d_02_asking_again_for_the_mode_you_hold_is_fine() {
    let mut q = LockQueue::new();
    assert_eq!(q.try_acquire(1, Shared), Ok(true));
    assert_eq!(q.try_acquire(1, Shared), Ok(true));
    assert_eq!(q.holders(), vec![(1, Shared)], "still one entry");
}

#[test]
fn s4d_02_an_upgrade_replaces_the_mode_when_nobody_is_in_the_way() {
    let mut q = LockQueue::new();
    q.try_acquire(1, Shared).unwrap();
    assert_eq!(q.try_acquire(1, Exclusive), Ok(true));
    assert_eq!(q.holders(), vec![(1, Exclusive)], "the old mode is replaced, not added");
    let mut q = LockQueue::new();
    q.try_acquire(1, Shared).unwrap();
    q.try_acquire(2, Shared).unwrap();
    assert_eq!(q.try_acquire(1, Exclusive), Ok(false), "another holder is in the way");
    assert_eq!(q.mode_of(1), Some(Shared), "a refused upgrade leaves the old lock alone");
    assert!(q.release(2));
    assert_eq!(q.try_acquire(1, Exclusive), Ok(true));
}

#[test]
fn s4d_02_asking_for_a_weaker_mode_is_an_error() {
    let mut q = LockQueue::new();
    q.try_acquire(1, Exclusive).unwrap();
    assert_eq!(q.try_acquire(1, Shared), Err(LockErrorKind::IncompatibleUpgrade));
    q.release(1);
    q.try_acquire(1, IntentionExclusive).unwrap();
    assert_eq!(q.try_acquire(1, Shared), Err(LockErrorKind::IncompatibleUpgrade), "IX to S is not an upgrade either");
    assert_eq!(q.mode_of(1), Some(IntentionExclusive));
}

#[test]
fn s4d_02_releasing_frees_the_lock_for_others() {
    let mut q = LockQueue::new();
    q.try_acquire(1, Exclusive).unwrap();
    assert!(!q.release(2), "2 held nothing");
    assert!(q.release(1));
    assert!(!q.release(1), "already released");
    assert_eq!(q.try_acquire(2, Exclusive), Ok(true));
}

#[derive(Clone, Debug)]
enum QOp {
    Acquire(TxnId, LockMode),
    Release(TxnId),
}

proptest! {
    #![proptest_config(config())]

    /// Property: against a model written from the textbook table, every answer agrees, and the holders are always compatible with each other.
    #[test]
    fn s4d_02_property_a_lock_queue_matches_a_model(ops in proptest::collection::vec(
        prop_oneof![
            3 => (1..5i64, any_mode()).prop_map(|(t, m)| QOp::Acquire(t, m)),
            2 => (1..5i64).prop_map(QOp::Release),
        ], 1..80)) {
        let mut q = LockQueue::new();
        let mut model: HashMap<TxnId, LockMode> = HashMap::new();
        let up = |from: LockMode, to: LockMode| -> bool {
            let rank = |m: LockMode| ALL.iter().position(|&x| x == m).unwrap();
            matches!((rank(from), rank(to)), (0, 1..=4) | (1, 3 | 4) | (2, 3 | 4) | (3, 4))
        };
        for op in ops {
            match op {
                QOp::Acquire(t, m) => {
                    let held = model.get(&t).copied();
                    let want: Result<bool, LockErrorKind> = match held {
                        Some(h) if h == m => Ok(true),
                        Some(h) if !up(h, m) => Err(LockErrorKind::IncompatibleUpgrade),
                        _ => {
                            if model.iter().any(|(&o, &om)| o != t && !truth(om, m)) { Ok(false) } else { model.insert(t, m); Ok(true) }
                        }
                    };
                    prop_assert_eq!(q.try_acquire(t, m), want, "acquire({}, {:?})", t, m);
                }
                QOp::Release(t) => {
                    prop_assert_eq!(q.release(t), model.remove(&t).is_some(), "release({})", t);
                }
            }
            let mut want: Vec<_> = model.iter().map(|(&t, &m)| (t, m)).collect();
            want.sort();
            prop_assert_eq!(q.holders(), want.clone());
            for (i, &(_, a)) in want.iter().enumerate() {
                for &(_, b) in &want[i + 1..] {
                    prop_assert!(truth(a, b), "two holders that cannot share: {:?} and {:?}", a, b);
                }
            }
        }
    }
}

// ---- 4d-03: waiting for a lock -----------------------------------------------------------------------------------------------------------

#[test]
fn s4d_03_try_lock_of_a_mode_already_held_is_true_even_with_a_writer_waiting() {
    finish_within("try_lock of a held mode", || {
        let m = Arc::new(LockManager::new());
        let a = table(1);
        m.lock(1, a, Shared).unwrap();
        let writer = {
            let m = Arc::clone(&m);
            thread::spawn(move || m.lock(2, a, Exclusive))
        };
        while m.waiting(a) != 1 {
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(m.try_lock(1, a, Shared), Ok(true), "a transaction that holds S already has what it asks for: try_lock agrees with lock");
        m.unlock(1, a).unwrap();
        assert!(writer.join().unwrap().is_ok(), "the waiting writer gets the lock once S is released");
    });
}

#[test]
fn s4d_03_a_free_lock_is_granted_at_once_and_shows_its_holder() {
    let m = LockManager::new();
    assert_eq!(m.lock(1, table(1), Shared), Ok(()));
    assert_eq!(m.holders(table(1)), vec![(1, Shared)]);
    assert_eq!(m.waiting(table(1)), 0);
    assert!(m.holders(table(2)).is_empty(), "another table is untouched");
}

#[test]
fn s4d_03_shared_locks_are_granted_together() {
    finish_within("shared locks", || {
        let m = Arc::new(LockManager::new());
        let hs: Vec<_> = (1..=4)
            .map(|t| {
                let m = Arc::clone(&m);
                thread::spawn(move || m.lock(t, table(1), Shared).unwrap())
            })
            .collect();
        for h in hs {
            h.join().unwrap();
        }
        assert_eq!(m.holders(table(1)).len(), 4);
    });
}

#[test]
fn s4d_03_an_exclusive_request_waits_until_the_holder_lets_go() {
    finish_within("exclusive waits", || {
        let m = Arc::new(LockManager::new());
        m.lock(1, table(1), Exclusive).unwrap();
        let (done, h) = spawn_flag({
            let m = Arc::clone(&m);
            move || m.lock(2, table(1), Exclusive).unwrap()
        });
        pause();
        assert!(!finished(&done), "the second exclusive request must wait");
        assert_eq!(m.waiting(table(1)), 1);
        assert_eq!(m.holders(table(1)), vec![(1, Exclusive)]);
        m.unlock(1, table(1)).unwrap();
        h.join().unwrap();
        assert_eq!(m.holders(table(1)), vec![(2, Exclusive)]);
        assert_eq!(m.waiting(table(1)), 0);
    });
}

#[test]
fn s4d_03_a_reader_does_not_jump_a_waiting_writer() {
    finish_within("fair queue", || {
        let m = Arc::new(LockManager::new());
        let order = Arc::new(Mutex::new(Vec::new()));
        m.lock(1, table(1), Shared).unwrap();
        let writer = {
            let (m, order) = (Arc::clone(&m), Arc::clone(&order));
            thread::spawn(move || {
                m.lock(2, table(1), Exclusive).unwrap();
                order.lock().unwrap().push(2);
                m.unlock(2, table(1)).unwrap();
            })
        };
        pause();
        // transaction 3 would be compatible with the holder, but a writer is ahead of it
        let reader = {
            let (m, order) = (Arc::clone(&m), Arc::clone(&order));
            thread::spawn(move || {
                m.lock(3, table(1), Shared).unwrap();
                order.lock().unwrap().push(3);
                m.unlock(3, table(1)).unwrap();
            })
        };
        pause();
        assert_eq!(m.waiting(table(1)), 2, "both wait: the reader is behind the writer");
        assert_eq!(m.holders(table(1)), vec![(1, Shared)]);
        m.unlock(1, table(1)).unwrap();
        writer.join().unwrap();
        reader.join().unwrap();
        assert_eq!(*order.lock().unwrap(), vec![2, 3], "the writer was first in line");
    });
}

#[test]
fn s4d_03_waiting_requests_are_granted_in_the_order_they_arrived() {
    finish_within("fifo", || {
        let m = Arc::new(LockManager::new());
        let order = Arc::new(Mutex::new(Vec::new()));
        m.lock(0, table(1), Exclusive).unwrap();
        let mut hs = Vec::new();
        for t in 1..=4 {
            let (m, order) = (Arc::clone(&m), Arc::clone(&order));
            hs.push(thread::spawn(move || {
                m.lock(t, table(1), Exclusive).unwrap();
                order.lock().unwrap().push(t);
                m.unlock(t, table(1)).unwrap();
            }));
            pause();
        }
        m.unlock(0, table(1)).unwrap();
        for h in hs {
            h.join().unwrap();
        }
        assert_eq!(*order.lock().unwrap(), vec![1, 2, 3, 4]);
    });
}

#[test]
fn s4d_03_readers_that_queued_together_are_granted_together() {
    finish_within("batch of readers", || {
        let m = Arc::new(LockManager::new());
        m.lock(0, table(1), Exclusive).unwrap();
        let hs: Vec<_> = (1..=3)
            .map(|t| {
                let m = Arc::clone(&m);
                let h = thread::spawn(move || m.lock(t, table(1), Shared).unwrap());
                pause();
                h
            })
            .collect();
        m.unlock(0, table(1)).unwrap();
        for h in hs {
            h.join().unwrap();
        }
        assert_eq!(m.holders(table(1)).len(), 3, "all three readers hold it at the same time");
    });
}

#[test]
fn s4d_03_unlocking_what_you_do_not_hold_is_an_error() {
    let m = LockManager::new();
    assert_eq!(kind(m.unlock(1, table(1))), LockErrorKind::AttemptedUnlockButNoLockHeld);
    m.lock(2, table(1), Shared).unwrap();
    assert_eq!(kind(m.unlock(1, table(1))), LockErrorKind::AttemptedUnlockButNoLockHeld, "1 does not hold what 2 holds");
    assert_eq!(m.holders(table(1)), vec![(2, Shared)]);
}

#[test]
fn s4d_03_try_lock_never_waits() {
    let m = LockManager::new();
    assert_eq!(m.try_lock(1, row(1, 1), Exclusive), Ok(true));
    assert_eq!(m.try_lock(2, row(1, 1), Shared), Ok(false));
    assert_eq!(m.try_lock(2, row(1, 2), Shared), Ok(true), "another row is free");
    assert_eq!(m.waiting(row(1, 1)), 0, "a refused try_lock leaves nobody waiting");
    m.unlock(1, row(1, 1)).unwrap();
    assert_eq!(m.try_lock(2, row(1, 1), Shared), Ok(true));
}

#[test]
fn s4d_03_try_lock_does_not_jump_the_queue_either() {
    finish_within("try_lock and the queue", || {
        let m = Arc::new(LockManager::new());
        m.lock(1, table(1), Shared).unwrap();
        let h = {
            let m = Arc::clone(&m);
            thread::spawn(move || m.lock(2, table(1), Exclusive).unwrap())
        };
        pause();
        assert_eq!(m.try_lock(3, table(1), Shared), Ok(false), "a writer is waiting ahead of it");
        m.unlock(1, table(1)).unwrap();
        h.join().unwrap();
    });
}

proptest! {
    #![proptest_config(config())]

    /// Property: on one thread with only try_lock and unlock, the manager agrees with a model: a lock is granted exactly when it is compatible with
    /// what the others hold, and nothing ever waits.
    #[test]
    fn s4d_03_property_try_lock_matches_a_model(ops in proptest::collection::vec(
        prop_oneof![
            3 => (1..4i64, 0..2u32, prop::sample::select(vec![Shared, Exclusive, IntentionShared, IntentionExclusive])).prop_map(|(t, r, m)| (true, t, r, m)),
            2 => (1..4i64, 0..2u32).prop_map(|(t, r)| (false, t, r, Shared)),
        ], 1..80)) {
        let m = LockManager::new();
        let mut model: HashMap<(u32, TxnId), LockMode> = HashMap::new();
        for (acquire, t, r, mode) in ops {
            let res = table(r);
            if acquire {
                if model.get(&(r, t)).is_some() { continue; } // upgrades are another stage
                let ok = model.iter().all(|(&(rr, o), &om)| rr != r || o == t || truth(om, mode));
                prop_assert_eq!(m.try_lock(t, res, mode), Ok(ok), "try_lock({}, {}, {:?})", t, r, mode);
                if ok { model.insert((r, t), mode); }
            } else {
                let held = model.remove(&(r, t)).is_some();
                prop_assert_eq!(m.unlock(t, res).is_ok(), held, "unlock({}, {})", t, r);
            }
            for rr in 0..2u32 {
                let mut want: Vec<_> = model.iter().filter(|(&(x, _), _)| x == rr).map(|(&(_, o), &om)| (o, om)).collect();
                want.sort();
                prop_assert_eq!(m.holders(table(rr)), want);
                prop_assert_eq!(m.waiting(table(rr)), 0);
            }
        }
    }
}

// ---- 4d-04: upgrades ---------------------------------------------------------------------------------------------------------------------

#[test]
fn s4d_04_a_lone_holder_upgrades_at_once() {
    let m = LockManager::new();
    m.lock(1, table(1), Shared).unwrap();
    assert_eq!(m.lock(1, table(1), Exclusive), Ok(()));
    assert_eq!(m.holders(table(1)), vec![(1, Exclusive)], "the shared lock became an exclusive one");
    m.lock(2, table(2), IntentionShared).unwrap();
    m.lock(2, table(2), SharedIntentionExclusive).unwrap();
    assert_eq!(m.holders(table(2)), vec![(2, SharedIntentionExclusive)]);
}

#[test]
fn s4d_04_asking_again_for_the_mode_you_hold_changes_nothing() {
    let m = LockManager::new();
    m.lock(1, table(1), Exclusive).unwrap();
    assert_eq!(m.lock(1, table(1), Exclusive), Ok(()));
    assert_eq!(m.holders(table(1)), vec![(1, Exclusive)]);
}

#[test]
fn s4d_04_an_upgrade_waits_for_the_other_holders() {
    finish_within("upgrade waits", || {
        let m = Arc::new(LockManager::new());
        m.lock(1, table(1), Shared).unwrap();
        m.lock(2, table(1), Shared).unwrap();
        let (done, h) = spawn_flag({
            let m = Arc::clone(&m);
            move || m.lock(1, table(1), Exclusive).unwrap()
        });
        pause();
        assert!(!finished(&done), "transaction 2 still holds a shared lock");
        assert_eq!(m.holders(table(1)).len(), 2, "nothing changes while the upgrade waits");
        m.unlock(2, table(1)).unwrap();
        h.join().unwrap();
        assert_eq!(m.holders(table(1)), vec![(1, Exclusive)]);
    });
}

#[test]
fn s4d_04_an_upgrade_goes_before_everybody_already_waiting() {
    finish_within("upgrade first", || {
        let m = Arc::new(LockManager::new());
        let order = Arc::new(Mutex::new(Vec::new()));
        m.lock(1, table(1), Shared).unwrap();
        m.lock(2, table(1), Shared).unwrap();
        let writer = {
            let (m, order) = (Arc::clone(&m), Arc::clone(&order));
            thread::spawn(move || {
                m.lock(3, table(1), Exclusive).unwrap();
                order.lock().unwrap().push(3);
                m.unlock(3, table(1)).unwrap();
            })
        };
        pause();
        let upgrader = {
            let (m, order) = (Arc::clone(&m), Arc::clone(&order));
            thread::spawn(move || {
                m.lock(1, table(1), Exclusive).unwrap();
                order.lock().unwrap().push(1);
                m.unlock(1, table(1)).unwrap();
            })
        };
        pause();
        m.unlock(2, table(1)).unwrap();
        upgrader.join().unwrap();
        writer.join().unwrap();
        assert_eq!(*order.lock().unwrap(), vec![1, 3], "the upgrade was granted first although it asked last");
    });
}

#[test]
fn s4d_04_a_second_upgrade_of_the_same_lock_is_refused() {
    finish_within("upgrade conflict", || {
        let m = Arc::new(LockManager::new());
        m.lock(1, table(1), Shared).unwrap();
        m.lock(2, table(1), Shared).unwrap();
        let (_done, first) = spawn_flag({
            let m = Arc::clone(&m);
            move || m.lock(1, table(1), Exclusive)
        });
        pause();
        assert_eq!(kind(m.lock(2, table(1), Exclusive)), LockErrorKind::UpgradeConflict, "only one transaction may be upgrading");
        m.unlock(2, table(1)).unwrap();
        assert_eq!(first.join().unwrap(), Ok(()), "the first upgrade is not harmed by the refused one");
        assert_eq!(m.holders(table(1)), vec![(1, Exclusive)]);
    });
}

#[test]
fn s4d_04_a_weaker_mode_than_the_one_held_is_not_an_upgrade() {
    let m = LockManager::new();
    m.lock(1, table(1), Exclusive).unwrap();
    assert_eq!(kind(m.lock(1, table(1), Shared)), LockErrorKind::IncompatibleUpgrade);
    assert_eq!(kind(m.lock(1, table(1), IntentionShared)), LockErrorKind::IncompatibleUpgrade);
    m.lock(2, table(2), IntentionExclusive).unwrap();
    assert_eq!(kind(m.lock(2, table(2), Shared)), LockErrorKind::IncompatibleUpgrade, "IX to S is sideways, not up");
    assert_eq!(m.holders(table(1)), vec![(1, Exclusive)], "a refused request changes nothing");
}

#[test]
fn s4d_04_an_upgrade_in_the_middle_of_a_queue_does_not_lose_the_others() {
    finish_within("queue after an upgrade", || {
        let m = Arc::new(LockManager::new());
        m.lock(1, table(1), IntentionShared).unwrap();
        m.lock(2, table(1), IntentionShared).unwrap();
        let mut hs = Vec::new();
        for t in 3..=4 {
            let m = Arc::clone(&m);
            hs.push(thread::spawn(move || {
                m.lock(t, table(1), Exclusive).unwrap();
                m.unlock(t, table(1)).unwrap();
            }));
            pause();
        }
        m.lock(1, table(1), Shared).unwrap();
        assert_eq!(m.holders(table(1)), vec![(1, Shared), (2, IntentionShared)]);
        m.unlock(1, table(1)).unwrap();
        m.unlock(2, table(1)).unwrap();
        for h in hs {
            h.join().unwrap();
        }
        assert!(m.holders(table(1)).is_empty());
    });
}

// ---- 4d-05: two-phase locking and isolation levels ---------------------------------------------------------------------------------------

fn txn(id: TxnId, iso: LockIsolation) -> Arc<LockTxn> {
    LockTxn::new(id, iso)
}

#[test]
fn s4d_05_read_uncommitted_never_takes_shared_locks() {
    for mode in [Shared, IntentionShared, SharedIntentionExclusive] {
        let m = LockManager::new();
        let t = txn(1, LockIsolation::ReadUncommitted);
        assert_eq!(kind(m.lock_table(&t, mode, 1)), LockErrorKind::LockSharedOnReadUncommitted, "{mode:?}");
        assert_eq!(t.state(), LockState::Aborted, "the transaction is aborted by the violation");
    }
    let m = LockManager::new();
    let t = txn(1, LockIsolation::ReadUncommitted);
    assert_eq!(m.lock_table(&t, IntentionExclusive, 1), Ok(()));
    assert_eq!(m.lock_table(&t, Exclusive, 2), Ok(()));
    assert_eq!(t.state(), LockState::Growing);
}

#[test]
fn s4d_05_releasing_an_exclusive_lock_starts_the_shrinking_phase_at_every_level() {
    for iso in [LockIsolation::ReadUncommitted, LockIsolation::ReadCommitted, LockIsolation::RepeatableRead] {
        let m = LockManager::new();
        let t = txn(1, iso);
        m.lock_table(&t, Exclusive, 1).unwrap();
        m.unlock_table(&t, 1).unwrap();
        assert_eq!(t.state(), LockState::Shrinking, "{iso:?}");
        assert_eq!(kind(m.lock_table(&t, Exclusive, 2)), LockErrorKind::LockOnShrinking, "{iso:?}: no new exclusive lock once shrinking");
        assert_eq!(t.state(), LockState::Aborted);
    }
}

#[test]
fn s4d_05_releasing_a_shared_lock_shrinks_only_under_repeatable_read() {
    let m = LockManager::new();
    let rr = txn(1, LockIsolation::RepeatableRead);
    m.lock_table(&rr, Shared, 1).unwrap();
    m.unlock_table(&rr, 1).unwrap();
    assert_eq!(rr.state(), LockState::Shrinking);
    assert_eq!(kind(m.lock_table(&rr, Shared, 2)), LockErrorKind::LockOnShrinking, "repeatable read takes no lock at all once shrinking");

    let rc = txn(2, LockIsolation::ReadCommitted);
    m.lock_table(&rc, Shared, 3).unwrap();
    m.unlock_table(&rc, 3).unwrap();
    assert_eq!(rc.state(), LockState::Growing, "read committed lets go of its read locks freely");
    assert_eq!(m.lock_table(&rc, Shared, 4), Ok(()));
}

#[test]
fn s4d_05_read_committed_may_still_read_while_shrinking() {
    let m = LockManager::new();
    let t = txn(1, LockIsolation::ReadCommitted);
    m.lock_table(&t, Exclusive, 1).unwrap();
    m.unlock_table(&t, 1).unwrap();
    assert_eq!(t.state(), LockState::Shrinking);
    assert_eq!(m.lock_table(&t, IntentionShared, 2), Ok(()), "IS is allowed while shrinking");
    assert_eq!(m.lock_table(&t, Shared, 3), Ok(()), "S is allowed while shrinking");
    assert_eq!(kind(m.lock_table(&t, IntentionExclusive, 4)), LockErrorKind::LockOnShrinking, "writes are not");
}

#[test]
fn s4d_05_a_row_can_only_be_locked_shared_or_exclusive() {
    let m = LockManager::new();
    let t = txn(1, LockIsolation::RepeatableRead);
    m.lock_table(&t, IntentionExclusive, 1).unwrap();
    for mode in [IntentionShared, IntentionExclusive, SharedIntentionExclusive] {
        let t2 = txn(2, LockIsolation::RepeatableRead);
        m.lock_table(&t2, IntentionExclusive, 2).unwrap();
        assert_eq!(kind(m.lock_row(&t2, mode, 2, rid(1))), LockErrorKind::AttemptedIntentionLockOnRow, "{mode:?}");
        assert_eq!(t2.state(), LockState::Aborted);
        m.release_all(&t2);
    }
    assert_eq!(m.lock_row(&t, Exclusive, 1, rid(1)), Ok(()));
}

#[test]
fn s4d_05_a_row_lock_needs_a_table_lock_that_is_enough() {
    let m = LockManager::new();
    let none = txn(1, LockIsolation::RepeatableRead);
    assert_eq!(kind(m.lock_row(&none, Shared, 1, rid(1))), LockErrorKind::TableLockNotPresent, "no table lock at all");

    let is = txn(2, LockIsolation::RepeatableRead);
    m.lock_table(&is, IntentionShared, 1).unwrap();
    assert_eq!(m.lock_row(&is, Shared, 1, rid(1)), Ok(()), "IS is enough to read a row");
    assert_eq!(kind(m.lock_row(&is, Exclusive, 1, rid(2))), LockErrorKind::TableLockNotPresent, "IS is not enough to write a row");

    for (id, table_mode) in [(3, IntentionExclusive), (4, SharedIntentionExclusive), (5, Exclusive)] {
        let t = txn(id, LockIsolation::RepeatableRead);
        let tbl = 10 + id as u32;
        m.lock_table(&t, table_mode, tbl).unwrap();
        assert_eq!(m.lock_row(&t, Exclusive, tbl, rid(1)), Ok(()), "{table_mode:?} is enough to write a row");
    }
    let s = txn(6, LockIsolation::RepeatableRead);
    m.lock_table(&s, Shared, 9).unwrap();
    assert_eq!(kind(m.lock_row(&s, Exclusive, 9, rid(1))), LockErrorKind::TableLockNotPresent, "a shared table lock does not allow writing a row");
}

#[test]
fn s4d_05_a_table_cannot_be_unlocked_while_its_rows_are() {
    let m = LockManager::new();
    let t = txn(1, LockIsolation::RepeatableRead);
    m.lock_table(&t, IntentionExclusive, 1).unwrap();
    m.lock_row(&t, Exclusive, 1, rid(1)).unwrap();
    assert_eq!(kind(m.unlock_table(&t, 1)), LockErrorKind::TableUnlockedBeforeUnlockingRows);
    assert_eq!(t.state(), LockState::Aborted);

    let t = txn(2, LockIsolation::RepeatableRead);
    m.lock_table(&t, IntentionShared, 2).unwrap();
    m.lock_row(&t, Shared, 2, rid(1)).unwrap();
    m.unlock_row(&t, 2, rid(1)).unwrap();
    assert_eq!(m.unlock_table(&t, 2), Ok(()), "with its rows gone the table can go");
}

#[test]
fn s4d_05_unlocking_what_is_not_locked_aborts() {
    let m = LockManager::new();
    let t = txn(1, LockIsolation::RepeatableRead);
    assert_eq!(kind(m.unlock_table(&t, 1)), LockErrorKind::AttemptedUnlockButNoLockHeld);
    assert_eq!(t.state(), LockState::Aborted);
    let t = txn(2, LockIsolation::RepeatableRead);
    m.lock_table(&t, IntentionShared, 1).unwrap();
    assert_eq!(kind(m.unlock_row(&t, 1, rid(5))), LockErrorKind::AttemptedUnlockButNoLockHeld);
}

#[test]
fn s4d_05_upgrades_follow_the_rules_and_a_bad_one_aborts() {
    let m = LockManager::new();
    let t = txn(1, LockIsolation::RepeatableRead);
    m.lock_table(&t, IntentionShared, 1).unwrap();
    assert_eq!(m.lock_table(&t, Exclusive, 1), Ok(()));
    assert_eq!(m.holders(table(1)), vec![(1, Exclusive)]);
    assert_eq!(kind(m.lock_table(&t, Shared, 1)), LockErrorKind::IncompatibleUpgrade);
    assert_eq!(t.state(), LockState::Aborted);
}

#[test]
fn s4d_05_an_aborted_or_committed_transaction_takes_no_locks() {
    let m = LockManager::new();
    let t = txn(1, LockIsolation::RepeatableRead);
    t.set_state(LockState::Aborted);
    assert_eq!(kind(m.lock_table(&t, Shared, 1)), LockErrorKind::NotRunning);
    let t = txn(2, LockIsolation::RepeatableRead);
    t.set_state(LockState::Committed);
    assert_eq!(kind(m.lock_table(&t, Shared, 1)), LockErrorKind::NotRunning);
    assert!(m.holders(table(1)).is_empty());
}

#[test]
fn s4d_05_release_all_lets_go_of_rows_then_tables_and_leaves_the_phase_alone() {
    let m = LockManager::new();
    let t = txn(1, LockIsolation::RepeatableRead);
    m.lock_table(&t, IntentionExclusive, 1).unwrap();
    m.lock_row(&t, Exclusive, 1, rid(1)).unwrap();
    m.lock_row(&t, Exclusive, 1, rid(2)).unwrap();
    m.lock_table(&t, Exclusive, 2).unwrap();
    m.release_all(&t);
    for r in [table(1), table(2), row(1, 1), row(1, 2)] {
        assert!(m.holders(r).is_empty(), "{r:?} is free");
    }
    assert_eq!(t.state(), LockState::Growing, "commit and abort set the state themselves");
    let other = txn(2, LockIsolation::RepeatableRead);
    assert_eq!(m.lock_table(&other, Exclusive, 1), Ok(()));
}

#[derive(Clone, Debug)]
enum TOp {
    Lock(u32, LockMode),
    Unlock(u32),
}

proptest! {
    #![proptest_config(config())]

    /// Property: for one transaction at any isolation level, every answer and every change of phase agrees with the rules written out again here.
    #[test]
    fn s4d_05_property_the_rules_hold_for_any_sequence(iso in prop::sample::select(vec![LockIsolation::ReadUncommitted, LockIsolation::ReadCommitted, LockIsolation::RepeatableRead]),
        ops in proptest::collection::vec(prop_oneof![
            3 => (0..3u32, any_mode()).prop_map(|(t, m)| TOp::Lock(t, m)),
            2 => (0..3u32).prop_map(TOp::Unlock),
        ], 1..40)) {
        let m = LockManager::new();
        let t = txn(1, iso);
        let mut held: HashMap<u32, LockMode> = HashMap::new();
        let mut state = LockState::Growing;
        let up = |from: LockMode, to: LockMode| matches!((ALL.iter().position(|&x| x == from).unwrap(), ALL.iter().position(|&x| x == to).unwrap()), (0, 1..=4) | (1, 3 | 4) | (2, 3 | 4) | (3, 4));
        for op in ops {
            let got: Result<(), LockErrorKind>;
            let want: Result<(), LockErrorKind>;
            match op {
                TOp::Lock(tbl, mode) => {
                    got = m.lock_table(&t, mode, tbl).map_err(|e| e.kind);
                    want = if state == LockState::Aborted {
                        Err(LockErrorKind::NotRunning)
                    } else if iso == LockIsolation::ReadUncommitted && matches!(mode, Shared | IntentionShared | SharedIntentionExclusive) {
                        state = LockState::Aborted;
                        Err(LockErrorKind::LockSharedOnReadUncommitted)
                    } else if state == LockState::Shrinking && !(iso == LockIsolation::ReadCommitted && matches!(mode, IntentionShared | Shared)) {
                        state = LockState::Aborted;
                        Err(LockErrorKind::LockOnShrinking)
                    } else {
                        match held.get(&tbl).copied() {
                            Some(h) if h == mode => Ok(()),
                            Some(h) if !up(h, mode) => { state = LockState::Aborted; Err(LockErrorKind::IncompatibleUpgrade) }
                            _ => { held.insert(tbl, mode); Ok(()) }
                        }
                    };
                }
                TOp::Unlock(tbl) => {
                    got = m.unlock_table(&t, tbl).map_err(|e| e.kind);
                    want = match held.remove(&tbl) {
                        None => { state = LockState::Aborted; Err(LockErrorKind::AttemptedUnlockButNoLockHeld) }
                        Some(mode) => {
                            if state == LockState::Growing {
                                let shrinks = if iso == LockIsolation::RepeatableRead { matches!(mode, Shared | Exclusive) } else { mode == Exclusive };
                                if shrinks { state = LockState::Shrinking; }
                            }
                            Ok(())
                        }
                    };
                }
            }
            prop_assert_eq!(got, want, "{:?}", op);
            prop_assert_eq!(t.state(), state, "state after {:?}", op);
        }
    }
}

// ---- 4d-06: deadlocks --------------------------------------------------------------------------------------------------------------------

fn graph(edges: &[(TxnId, TxnId)]) -> WaitsForGraph {
    let mut g = WaitsForGraph::new();
    for &(a, b) in edges {
        g.add_edge(a, b);
    }
    g
}

#[test]
fn s4d_06_a_graph_without_cycles_has_no_victim() {
    assert_eq!(WaitsForGraph::new().has_cycle(), None);
    assert_eq!(graph(&[(1, 2), (2, 3), (3, 4)]).has_cycle(), None, "a chain");
    assert_eq!(graph(&[(1, 2), (1, 3), (2, 4), (3, 4)]).has_cycle(), None, "a diamond is not a cycle");
}

#[test]
fn s4d_06_the_victim_is_the_youngest_transaction_on_the_cycle() {
    assert_eq!(graph(&[(1, 2), (2, 1)]).has_cycle(), Some(2));
    assert_eq!(graph(&[(1, 2), (2, 3), (3, 1)]).has_cycle(), Some(3));
    assert_eq!(graph(&[(7, 3), (3, 9), (9, 7)]).has_cycle(), Some(9), "ids need not be small or in order");
    assert_eq!(graph(&[(5, 5)]).has_cycle(), Some(5), "waiting for yourself is a cycle");
}

#[test]
fn s4d_06_a_cycle_reached_through_a_tail_is_found() {
    assert_eq!(graph(&[(1, 2), (2, 3), (3, 4), (4, 2)]).has_cycle(), Some(4), "1 is not on the cycle, so it is not the victim");
    assert_eq!(graph(&[(10, 1), (1, 2), (2, 1)]).has_cycle(), Some(2));
}

#[test]
fn s4d_06_removing_an_edge_can_break_the_cycle() {
    let mut g = graph(&[(1, 2), (2, 3), (3, 1)]);
    g.remove_edge(3, 1);
    assert_eq!(g.has_cycle(), None);
    g.add_edge(3, 1);
    g.add_edge(3, 1);
    assert_eq!(g.has_cycle(), Some(3));
    g.remove_edge(9, 9);
    assert_eq!(g.edge_list().len(), 3, "an edge added twice is one edge, and removing a missing edge does nothing");
}

#[test]
fn s4d_06_the_edge_list_is_ordered() {
    let g = graph(&[(3, 1), (1, 3), (1, 2), (2, 1)]);
    assert_eq!(g.edge_list(), vec![(1, 2), (1, 3), (2, 1), (3, 1)]);
}

#[test]
fn s4d_06_two_transactions_that_wait_for_each_other_are_broken_up() {
    finish_within("two-way deadlock", || {
        let m = Arc::new(LockManager::new());
        m.lock(1, table(1), Exclusive).unwrap();
        m.lock(2, table(2), Exclusive).unwrap();
        let t1 = {
            let m = Arc::clone(&m);
            thread::spawn(move || m.lock(1, table(2), Exclusive))
        };
        let t2 = {
            let m = Arc::clone(&m);
            thread::spawn(move || m.lock(2, table(1), Exclusive))
        };
        // both are waiting now; detect until the detector sees it
        let mut victims = Vec::new();
        while victims.is_empty() {
            thread::sleep(Duration::from_millis(20));
            victims = m.detect_deadlocks();
        }
        assert_eq!(victims, vec![2], "the younger transaction is the victim");
        assert_eq!(kind(t2.join().unwrap()), LockErrorKind::Deadlock, "the victim's request gives up");
        m.unlock(2, table(2)).unwrap();
        assert_eq!(t1.join().unwrap(), Ok(()), "the other one now gets what it waited for");
        assert!(m.detect_deadlocks().is_empty(), "no deadlock is left");
    });
}

#[test]
fn s4d_06_a_cycle_of_three_loses_its_youngest() {
    finish_within("three-way deadlock", || {
        let m = Arc::new(LockManager::new());
        for t in 1..=3 {
            m.lock(t, table(t as u32), Exclusive).unwrap();
        }
        // each transaction wants the next one's table; whoever finishes lets go of everything it holds, which is what frees the others
        let hs: Vec<_> = (1..=3i64)
            .map(|t| {
                let m = Arc::clone(&m);
                thread::spawn(move || {
                    let next = (t % 3 + 1) as u32;
                    let r = m.lock(t, table(next), Exclusive);
                    if r.is_ok() {
                        m.unlock(t, table(next)).unwrap();
                    }
                    m.unlock(t, table(t as u32)).unwrap();
                    r
                })
            })
            .collect();
        let mut victims = Vec::new();
        while victims.is_empty() {
            thread::sleep(Duration::from_millis(20));
            victims = m.detect_deadlocks();
        }
        assert_eq!(victims, vec![3], "the youngest of the three");
        let results: Vec<_> = hs.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(kind(results[2].clone()), LockErrorKind::Deadlock);
        assert!(results[0].is_ok() && results[1].is_ok(), "the other two get their locks once the victim lets go");
    });
}

#[test]
fn s4d_06_two_separate_deadlocks_are_both_broken_in_one_pass() {
    finish_within("two deadlocks", || {
        let m = Arc::new(LockManager::new());
        for t in 1..=4 {
            m.lock(t, table(t as u32), Exclusive).unwrap();
        }
        // pairs (1, 2) and (3, 4) each deadlock
        let wants = [(1i64, 2u32), (2, 1), (3, 4), (4, 3)];
        let hs: Vec<_> = wants
            .iter()
            .map(|&(t, other)| {
                let m = Arc::clone(&m);
                thread::spawn(move || {
                    let r = m.lock(t, table(other), Exclusive);
                    m.unlock(t, table(t as u32)).unwrap();
                    if r.is_ok() {
                        m.unlock(t, table(other)).unwrap();
                    }
                    r
                })
            })
            .collect();
        // wait until all four requests are queued, then ask once
        while (1..=4).any(|t| m.waiting(table(t)) != 1) {
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(m.detect_deadlocks(), vec![2, 4], "one pass breaks every cycle: the youngest of each pair");
        let results: Vec<_> = hs.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(kind(results[1].clone()), LockErrorKind::Deadlock);
        assert_eq!(kind(results[3].clone()), LockErrorKind::Deadlock);
        assert!(results[0].is_ok() && results[2].is_ok(), "the older transaction of each pair goes on");
    });
}

#[test]
fn s4d_06_a_deadlock_through_queue_order_is_found() {
    // T1 holds S(A) and T3 holds X(B). T2 asks X(A) and waits for T1. T3 asks S(A): compatible with the holder T1, but the queue is
    // fair, so it waits behind T2's X. T1 asks S(B) and waits for T3. T1 -> T3 -> T2 -> T1 is a deadlock with no cycle among holders.
    finish_within("a deadlock through queue order", || {
        let m = Arc::new(LockManager::new());
        let (a, b) = (table(1), table(2));
        m.lock(1, a, Shared).unwrap();
        m.lock(3, b, Exclusive).unwrap();
        let until = |n: usize, r: Resource| {
            while m.waiting(r) != n {
                thread::sleep(Duration::from_millis(2));
            }
        };
        let t2 = {
            let m = Arc::clone(&m);
            thread::spawn(move || m.lock(2, a, Exclusive))
        };
        until(1, a);
        let t3 = {
            let m = Arc::clone(&m);
            thread::spawn(move || m.lock(3, a, Shared))
        };
        until(2, a);
        let t1 = {
            let m = Arc::clone(&m);
            thread::spawn(move || m.lock(1, b, Shared))
        };
        until(1, b);
        assert_eq!(m.detect_deadlocks(), vec![3], "a request also waits for the earlier waiters it conflicts with: T3 is behind T2, and the youngest on the cycle is the victim");
        assert_eq!(kind(t3.join().unwrap()), LockErrorKind::Deadlock, "the victim's waiting lock call returns Deadlock");
        m.unlock(3, b).unwrap();
        assert!(t1.join().unwrap().is_ok(), "once the victim lets go, T1 gets S(B)");
        m.unlock(1, a).unwrap();
        m.unlock(1, b).unwrap();
        assert!(t2.join().unwrap().is_ok(), "and T2 gets X(A) after T1");
    });
}

#[test]
fn s4d_06_the_detector_thread_breaks_a_deadlock_by_itself() {
    finish_within("detector thread", || {
        let m = Arc::new(LockManager::new());
        let _detector = DeadlockDetector::start(Arc::clone(&m), Duration::from_millis(20));
        m.lock(1, table(1), Exclusive).unwrap();
        m.lock(2, table(2), Exclusive).unwrap();
        let t1 = {
            let m = Arc::clone(&m);
            thread::spawn(move || m.lock(1, table(2), Exclusive))
        };
        let t2 = {
            let m = Arc::clone(&m);
            thread::spawn(move || {
                let r = m.lock(2, table(1), Exclusive);
                if r.is_err() {
                    m.unlock(2, table(2)).unwrap();
                }
                r
            })
        };
        assert_eq!(kind(t2.join().unwrap()), LockErrorKind::Deadlock, "the youngest was aborted without anyone calling detect_deadlocks");
        assert_eq!(t1.join().unwrap(), Ok(()));
    });
}

#[test]
fn s4d_06_a_waiting_request_that_is_no_deadlock_is_left_alone() {
    finish_within("no false alarm", || {
        let m = Arc::new(LockManager::new());
        m.lock(1, table(1), Exclusive).unwrap();
        let h = {
            let m = Arc::clone(&m);
            thread::spawn(move || m.lock(2, table(1), Exclusive))
        };
        pause();
        assert!(m.detect_deadlocks().is_empty(), "2 waits for 1, and 1 waits for nobody");
        m.unlock(1, table(1)).unwrap();
        assert_eq!(h.join().unwrap(), Ok(()));
    });
}

proptest! {
    #![proptest_config(config())]

    /// Property: a cycle is found exactly when some transaction can reach itself; the victim lies on a cycle of which it is the youngest; and
    /// removing victims one after another leaves a graph with no cycle.
    #[test]
    fn s4d_06_property_the_graph_finds_exactly_the_cycles(edges in proptest::collection::vec((1..8i64, 1..8i64), 0..24)) {
        let g = graph(&edges);
        let set: BTreeSet<(TxnId, TxnId)> = edges.iter().copied().collect();
        prop_assert_eq!(g.edge_list(), set.iter().copied().collect::<Vec<_>>());
        // brute force: can `a` reach `a` using only transactions no younger than `limit`?
        let reaches_self = |a: TxnId, limit: TxnId, edges: &BTreeSet<(TxnId, TxnId)>| -> bool {
            let mut seen = HashSet::new();
            let mut stack = vec![a];
            while let Some(n) = stack.pop() {
                for &(x, y) in edges.iter() {
                    if x == n && y <= limit {
                        if y == a {
                            return true;
                        }
                        if seen.insert(y) {
                            stack.push(y);
                        }
                    }
                }
            }
            false
        };
        let any_cycle = (1..8).any(|a| reaches_self(a, 7, &set));
        match g.has_cycle() {
            None => prop_assert!(!any_cycle, "a cycle exists but none was found"),
            Some(v) => {
                prop_assert!(any_cycle);
                prop_assert!(reaches_self(v, v, &set), "{} is not the youngest transaction of any cycle it is on", v);
            }
        }
        // breaking cycles by removing victims ends
        let mut g = g;
        let mut rounds = 0;
        while let Some(v) = g.has_cycle() {
            for (a, b) in g.edge_list() {
                if a == v || b == v {
                    g.remove_edge(a, b);
                }
            }
            rounds += 1;
            prop_assert!(rounds <= 8, "never ends");
        }
    }
}

// ---- 4d-07: the whole lock manager under load --------------------------------------------------------------------------------------------

/// The history of one finished transaction: its operations in the global order they happened, (sequence number, row, wrote).
type History = Vec<(u64, u32, bool)>;

/// Conflict serializability: a graph with an edge a -> b when a's operation on a row came before b's conflicting operation (at least one of
/// them a write) on the same row. A history is equal to some serial order exactly when that graph has no cycle.
fn serializable(histories: &[History]) -> bool {
    let n = histories.len();
    let mut edges: Vec<HashSet<usize>> = vec![HashSet::new(); n];
    for a in 0..n {
        for b in 0..n {
            if a == b {
                continue;
            }
            let conflict = histories[a].iter().any(|&(sa, ra, wa)| histories[b].iter().any(|&(sb, rb, wb)| ra == rb && (wa || wb) && sa < sb));
            if conflict {
                edges[a].insert(b);
            }
        }
    }
    // a cycle exists when some node can reach itself
    for start in 0..n {
        let mut seen = vec![false; n];
        let mut stack: Vec<usize> = edges[start].iter().copied().collect();
        while let Some(x) = stack.pop() {
            if x == start {
                return false;
            }
            if !seen[x] {
                seen[x] = true;
                stack.extend(edges[x].iter().copied());
            }
        }
    }
    true
}

#[test]
fn s4d_07_the_serializability_check_itself_is_right() {
    // two transactions that each read before the other writes: a cycle, so not serializable
    let a: History = vec![(1, 1, false), (4, 2, true)];
    let b: History = vec![(2, 2, false), (3, 1, true)];
    assert!(!serializable(&[a, b]));
    let c: History = vec![(1, 1, false), (2, 2, true)];
    let d: History = vec![(3, 1, false), (4, 2, true)];
    assert!(serializable(&[c, d]));
}

/// A bank of `rows` accounts, each started with 100. Each worker runs `transfers` transactions (move an amount between two random accounts, locking
/// them in random order so that deadlocks happen) and retries one that the deadlock detector aborted. Returns the histories of the committed ones.
fn run_bank(iso: LockIsolation, workers: usize, transfers: usize, rows: u32) -> (Vec<History>, Vec<i64>, usize) {
    let m = Arc::new(LockManager::new());
    let _detector = DeadlockDetector::start(Arc::clone(&m), Duration::from_millis(4));
    let balances: Arc<Vec<AtomicI64>> = Arc::new((0..rows).map(|_| AtomicI64::new(100)).collect());
    let seq = Arc::new(AtomicUsize::new(0));
    let next_id = Arc::new(AtomicI64::new(1));
    let aborts = Arc::new(AtomicUsize::new(0));
    let histories: Arc<Mutex<Vec<History>>> = Arc::new(Mutex::new(Vec::new()));
    let hs: Vec<_> = (0..workers)
        .map(|w| {
            let (m, balances, seq, next_id, aborts, histories) = (Arc::clone(&m), Arc::clone(&balances), Arc::clone(&seq), Arc::clone(&next_id), Arc::clone(&aborts), Arc::clone(&histories));
            thread::spawn(move || {
                let mut rng = (w as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
                let mut rand = move |n: u32| {
                    rng ^= rng << 13;
                    rng ^= rng >> 7;
                    rng ^= rng << 17;
                    (rng % n as u64) as u32
                };
                for _ in 0..transfers {
                    let (a, b) = {
                        let a = rand(rows);
                        let mut b = rand(rows);
                        while b == a {
                            b = rand(rows);
                        }
                        (a, b)
                    };
                    let amount = rand(10) as i64 + 1;
                    loop {
                        let id = next_id.fetch_add(1, Ordering::SeqCst);
                        let t = LockTxn::new(id, iso);
                        let mut hist: History = Vec::new();
                        let attempt = (|| -> Result<(), LockError> {
                            m.lock_table(&t, IntentionExclusive, 1)?;
                            m.lock_row(&t, Exclusive, 1, rid(a))?;
                            hist.push((seq.fetch_add(1, Ordering::SeqCst) as u64, a, true));
                            let va = balances[a as usize].load(Ordering::SeqCst);
                            thread::sleep(Duration::from_micros(200));
                            m.lock_row(&t, Exclusive, 1, rid(b))?;
                            hist.push((seq.fetch_add(1, Ordering::SeqCst) as u64, b, true));
                            let vb = balances[b as usize].load(Ordering::SeqCst);
                            balances[a as usize].store(va - amount, Ordering::SeqCst);
                            balances[b as usize].store(vb + amount, Ordering::SeqCst);
                            Ok(())
                        })();
                        match attempt {
                            Ok(()) => {
                                t.set_state(LockState::Committed);
                                m.release_all(&t);
                                histories.lock().unwrap().push(hist);
                                break;
                            }
                            Err(_) => {
                                // aborted: nothing was written (the writes happen after both locks), let go and try again
                                t.set_state(LockState::Aborted);
                                m.release_all(&t);
                                aborts.fetch_add(1, Ordering::SeqCst);
                            }
                        }
                    }
                }
            })
        })
        .collect();
    for h in hs {
        h.join().unwrap();
    }
    let final_balances: Vec<i64> = balances.iter().map(|b| b.load(Ordering::SeqCst)).collect();
    let h = histories.lock().unwrap().clone();
    (h, final_balances, aborts.load(Ordering::SeqCst))
}

#[test]
fn s4d_07_a_bank_under_strict_two_phase_locking_loses_no_money_and_is_serializable() {
    finish_within("bank workload", || {
        let (histories, balances, _aborts) = run_bank(LockIsolation::RepeatableRead, 6, 25, 5);
        assert_eq!(histories.len(), 6 * 25, "every transfer committed in the end, after retries");
        assert_eq!(balances.iter().sum::<i64>(), 5 * 100, "the total never changes: no transfer was lost or half done");
        assert!(serializable(&histories), "the history of the committed transactions is equal to some serial order");
    });
}

#[test]
fn s4d_07_deadlocks_really_happen_in_the_workload_and_are_always_resolved() {
    finish_within("deadlocks resolved", || {
        let mut total_aborts = 0;
        for _ in 0..3 {
            let (histories, balances, aborts) = run_bank(LockIsolation::RepeatableRead, 6, 12, 3);
            total_aborts += aborts;
            assert_eq!(histories.len(), 6 * 12);
            assert_eq!(balances.iter().sum::<i64>(), 300);
        }
        assert!(total_aborts > 0, "three accounts locked in random order by six threads must have deadlocked at least once; the test would prove nothing otherwise");
    });
}

#[test]
fn s4d_07_two_writers_are_never_inside_the_same_row_at_once() {
    finish_within("mutual exclusion", || {
        let m = Arc::new(LockManager::new());
        let inside = Arc::new(AtomicUsize::new(0));
        let worst = Arc::new(AtomicUsize::new(0));
        let hs: Vec<_> = (1..=8i64)
            .map(|id| {
                let (m, inside, worst) = (Arc::clone(&m), Arc::clone(&inside), Arc::clone(&worst));
                thread::spawn(move || {
                    for k in 0..30 {
                        let t = LockTxn::new(id * 1000 + k, LockIsolation::RepeatableRead);
                        m.lock_table(&t, IntentionExclusive, 1).unwrap();
                        m.lock_row(&t, Exclusive, 1, rid(0)).unwrap();
                        let now = inside.fetch_add(1, Ordering::SeqCst) + 1;
                        worst.fetch_max(now, Ordering::SeqCst);
                        thread::yield_now();
                        inside.fetch_sub(1, Ordering::SeqCst);
                        m.release_all(&t);
                    }
                })
            })
            .collect();
        for h in hs {
            h.join().unwrap();
        }
        assert_eq!(worst.load(Ordering::SeqCst), 1, "an exclusive row lock admitted two transactions at once");
    });
}

#[test]
fn s4d_07_readers_share_and_a_table_scan_excludes_row_writers() {
    finish_within("readers and a scan", || {
        let m = Arc::new(LockManager::new());
        // three transactions read the same row at the same time under IS and S: each waits at the barrier while it holds its S lock,
        // so the barrier opens only if all three hold it together (a manager that treated S like X would time out here)
        let together = Arc::new(std::sync::Barrier::new(3));
        let readers: Vec<_> = (1..=3i64)
            .map(|id| {
                let (m, together) = (Arc::clone(&m), Arc::clone(&together));
                thread::spawn(move || {
                    let t = LockTxn::new(id, LockIsolation::RepeatableRead);
                    m.lock_table(&t, IntentionShared, 1).unwrap();
                    m.lock_row(&t, Shared, 1, rid(1)).unwrap();
                    together.wait();
                    m.release_all(&t);
                })
            })
            .collect();
        for r in readers {
            r.join().unwrap();
        }
        // a whole-table shared lock keeps row writers (IX) out until it is released
        let scan = LockTxn::new(10, LockIsolation::RepeatableRead);
        m.lock_table(&scan, Shared, 1).unwrap();
        let writer = LockTxn::new(11, LockIsolation::RepeatableRead);
        let (done, h) = spawn_flag({
            let (m, writer) = (Arc::clone(&m), Arc::clone(&writer));
            move || m.lock_table(&writer, IntentionExclusive, 1).unwrap()
        });
        pause();
        assert!(!finished(&done), "IX must wait for the table scan's S lock");
        m.release_all(&scan);
        h.join().unwrap();
    });
}

#[test]
fn s4d_07_a_victim_that_is_waiting_for_a_row_gives_up_and_the_survivor_commits() {
    finish_within("row deadlock", || {
        let m = Arc::new(LockManager::new());
        let _d = DeadlockDetector::start(Arc::clone(&m), Duration::from_millis(15));
        let a = LockTxn::new(1, LockIsolation::RepeatableRead);
        let b = LockTxn::new(2, LockIsolation::RepeatableRead);
        for t in [&a, &b] {
            m.lock_table(t, IntentionExclusive, 1).unwrap();
        }
        m.lock_row(&a, Exclusive, 1, rid(1)).unwrap();
        m.lock_row(&b, Exclusive, 1, rid(2)).unwrap();
        let ha = {
            let (m, a) = (Arc::clone(&m), Arc::clone(&a));
            thread::spawn(move || m.lock_row(&a, Exclusive, 1, rid(2)))
        };
        let hb = {
            let (m, b) = (Arc::clone(&m), Arc::clone(&b));
            thread::spawn(move || {
                let r = m.lock_row(&b, Exclusive, 1, rid(1));
                if r.is_err() {
                    m.release_all(&b);
                }
                r
            })
        };
        assert_eq!(kind(hb.join().unwrap()), LockErrorKind::Deadlock);
        assert_eq!(b.state(), LockState::Aborted, "the victim's transaction is aborted");
        assert_eq!(ha.join().unwrap(), Ok(()));
        assert_eq!(a.state(), LockState::Growing, "the survivor goes on");
    });
}

// @@ challenge 4d-c1 begin
mod ch_4d_c1 {
    use proptest::prelude::*;

    use bustub::concurrency::deadlock_prevention::{decide, Decision::*, Policy::*};

    #[test]
    fn s4d_c1_wait_die() {
        assert_eq!(decide(WaitDie, 5, 9), Wait, "an older transaction may wait for a younger one");
        assert_eq!(decide(WaitDie, 9, 5), AbortRequester, "a younger one dies rather than wait for an older one");
    }

    #[test]
    fn s4d_c1_wound_wait() {
        assert_eq!(decide(WoundWait, 5, 9), AbortHolder, "an older transaction wounds a younger holder");
        assert_eq!(decide(WoundWait, 9, 5), Wait, "a younger one waits for an older one");
    }

    #[test]
    fn s4d_c1_the_decision_depends_only_on_the_order() {
        for p in [WaitDie, WoundWait] {
            assert_eq!(decide(p, 1, 2), decide(p, 100, 200));
            assert_eq!(decide(p, 2, 1), decide(p, 200, 100));
        }
    }

    #[test]
    fn s4d_c1_swapping_the_two_transactions_swaps_who_gives_way() {
        for (a, b) in [(1, 2), (10, 11), (3, 400)] {
            assert_eq!((decide(WaitDie, a, b), decide(WaitDie, b, a)), (Wait, AbortRequester));
            assert_eq!((decide(WoundWait, a, b), decide(WoundWait, b, a)), (AbortHolder, Wait));
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: under either policy the waits-for graph built from Wait decisions is acyclic, for any pattern of requests.
        #[test]
        fn s4d_c1_property_waiting_never_forms_a_cycle(requests in proptest::collection::vec((0u64..6, 0u64..6), 0..30)) {
            for policy in [WaitDie, WoundWait] {
                let mut edges: Vec<(u64, u64)> = Vec::new();
                for &(req, holder) in &requests {
                    if req == holder { continue; }
                    if decide(policy, req, holder) == Wait { edges.push((req, holder)); }
                }
                // a cycle would need an edge that goes "against" the age order; under each policy all waits go one way
                for &(a, b) in &edges {
                    match policy { WaitDie => prop_assert!(a < b), WoundWait => prop_assert!(a > b) }
                }
                // and so no path can return to its start
                let mut reach: Vec<Vec<u64>> = vec![Vec::new(); 6];
                for &(a, b) in &edges { reach[a as usize].push(b); }
                for start in 0..6u64 {
                    let mut seen = std::collections::HashSet::new();
                    let mut stack: Vec<u64> = reach[start as usize].clone();
                    while let Some(x) = stack.pop() {
                        prop_assert!(x != start, "a cycle through {} under {:?}", start, policy);
                        if seen.insert(x) { stack.extend(reach[x as usize].iter().copied()); }
                    }
                }
            }
        }
    }
}
// @@ challenge 4d-c1 end

// @@ challenge 4d-c2 begin
mod ch_4d_c2 {
    use proptest::prelude::*;

    use bustub::concurrency::range_locks::{EmptyRange, Mode::*, RangeLocks};

    #[test]
    fn s4d_c2_shared_ranges_overlap_freely_and_exclusive_ones_do_not() {
        let mut l = RangeLocks::new();
        assert_eq!(l.try_lock(1, 0, 10, Shared), Ok(true));
        assert_eq!(l.try_lock(2, 5, 15, Shared), Ok(true));
        assert_eq!(l.try_lock(3, 9, 12, Exclusive), Ok(false));
        assert_eq!(l.try_lock(3, 20, 30, Exclusive), Ok(true));
    }

    #[test]
    fn s4d_c2_a_phantom_insert_is_a_conflict_with_the_range_not_a_row() {
        let mut l = RangeLocks::new();
        l.try_lock(1, 30, 41, Shared).unwrap();
        assert_eq!(l.try_lock(2, 35, 36, Exclusive), Ok(false), "inserting 35 would change what the scan returns");
        assert_eq!(l.try_lock(2, 41, 42, Exclusive), Ok(true));
    }

    #[test]
    fn s4d_c2_adjacent_ranges_do_not_overlap() {
        let mut l = RangeLocks::new();
        l.try_lock(1, 0, 5, Exclusive).unwrap();
        assert_eq!(l.try_lock(2, 5, 9, Exclusive), Ok(true));
        assert_eq!(l.try_lock(3, 4, 6, Shared), Ok(false));
    }

    #[test]
    fn s4d_c2_a_transaction_does_not_conflict_with_itself_and_unlock_frees_the_range() {
        let mut l = RangeLocks::new();
        assert_eq!(l.try_lock(1, 0, 10, Exclusive), Ok(true));
        assert_eq!(l.try_lock(1, 5, 15, Exclusive), Ok(true));
        assert_eq!(l.held_by(1), 2);
        assert_eq!(l.try_lock(2, 12, 13, Shared), Ok(false));
        assert_eq!(l.unlock_all(1), 2);
        assert_eq!(l.try_lock(2, 12, 13, Shared), Ok(true));
    }

    #[test]
    fn s4d_c2_empty_ranges_are_refused() {
        let mut l = RangeLocks::new();
        assert_eq!(l.try_lock(1, 5, 5, Shared), Err(EmptyRange));
        assert_eq!(l.try_lock(1, 7, 3, Exclusive), Err(EmptyRange));
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: against a per-key brute force: a request conflicts iff some key in it is covered by another transaction's incompatible lock.
        #[test]
        fn s4d_c2_property_ranges_match_a_per_key_check(ops in proptest::collection::vec((1u32..4, 0i64..12, 1i64..6, any::<bool>()), 0..25)) {
            let mut l = RangeLocks::new();
            let mut model: Vec<(u32, i64, i64, bool)> = Vec::new(); // (txn, lo, hi, exclusive)
            for (txn, lo, len, excl) in ops {
                let hi = lo + len;
                let conflict = model.iter().any(|&(t, a, b, e)| t != txn && (lo..hi).any(|k| (a..b).contains(&k)) && (e || excl));
                let got = l.try_lock(txn, lo, hi, if excl { Exclusive } else { Shared }).unwrap();
                prop_assert_eq!(got, !conflict);
                if got { model.push((txn, lo, hi, excl)); }
            }
        }
    }
}
// @@ challenge 4d-c2 end

// @@ challenge 4d-c3 begin
mod ch_4d_c3 {
    use proptest::prelude::*;

    use bustub::concurrency::victim::{choose_victim, TxnInfo, VictimPolicy::*};

    fn t(id: u32, start_ts: u64, locks: usize, work: u64) -> TxnInfo {
        TxnInfo { id, start_ts, locks, work }
    }

    fn cycle() -> Vec<TxnInfo> {
        vec![t(1, 10, 5, 100), t(2, 30, 9, 50), t(3, 20, 2, 70)]
    }

    #[test]
    fn s4d_c3_each_policy_picks_by_its_attribute() {
        assert_eq!(choose_victim(Youngest, &cycle()), Some(1));
        assert_eq!(choose_victim(FewestLocks, &cycle()), Some(2));
        assert_eq!(choose_victim(LeastWork, &cycle()), Some(1));
    }

    #[test]
    fn s4d_c3_ties_go_to_the_larger_id() {
        let c = vec![t(4, 10, 3, 5), t(9, 10, 3, 5), t(7, 10, 3, 5)];
        for p in [Youngest, FewestLocks, LeastWork] {
            assert_eq!(choose_victim(p, &c), Some(1), "{p:?}");
        }
    }

    #[test]
    fn s4d_c3_empty_and_single_cycles() {
        assert_eq!(choose_victim(Youngest, &[]), None);
        assert_eq!(choose_victim(LeastWork, &[t(1, 1, 1, 1)]), Some(0));
    }

    #[test]
    fn s4d_c3_the_order_of_the_cycle_does_not_change_who_is_chosen() {
        let c = cycle();
        let r = vec![t(3, 20, 2, 70), t(2, 30, 9, 50), t(1, 10, 5, 100)];
        for p in [Youngest, FewestLocks, LeastWork] {
            let (a, b) = (choose_victim(p, &c).unwrap(), choose_victim(p, &r).unwrap());
            assert_eq!(c[a].id, r[b].id, "{p:?}");
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: the same transaction is chosen whatever the order of the cycle.
        #[test]
        fn s4d_c3_property_the_choice_does_not_depend_on_the_order(raw in proptest::collection::vec((0u64..5, 0usize..4, 0u64..5), 1..6), policy in prop::sample::select(vec![Youngest, FewestLocks, LeastWork])) {
            let c: Vec<TxnInfo> = raw.iter().enumerate().map(|(i, &(s, l, w))| t(i as u32 + 1, s, l, w)).collect();
            let chosen = c[choose_victim(policy, &c).unwrap()].id;
            let mut rev = c.clone();
            rev.reverse();
            prop_assert_eq!(rev[choose_victim(policy, &rev).unwrap()].id, chosen);
            let attr = |x: &TxnInfo| match policy { Youngest => -(x.start_ts as i128), FewestLocks => x.locks as i128, LeastWork => x.work as i128 };
            let best = c.iter().map(attr).min().unwrap();
            let v = c.iter().find(|x| x.id == chosen).unwrap();
            prop_assert_eq!(attr(v), best);
            prop_assert!(c.iter().filter(|x| attr(x) == best).all(|x| x.id <= chosen));
        }
    }
}
// @@ challenge 4d-c3 end

// @@ challenge 4d-c4 begin
mod ch_4d_c4 {
    use proptest::prelude::*;

    use bustub::concurrency::grant::{grantable, Mode::*};

    #[test]
    fn s4d_c4_a_reader_may_not_pass_a_waiting_writer() {
        assert_eq!(grantable(&[S], &[X, S]), Vec::<usize>::new());
        assert_eq!(grantable(&[S, S], &[X, S, S]), Vec::<usize>::new());
    }

    #[test]
    fn s4d_c4_readers_at_the_front_are_granted_together_up_to_the_first_writer() {
        assert_eq!(grantable(&[], &[S, S, X, S]), vec![0, 1]);
        assert_eq!(grantable(&[S], &[S, S, X]), vec![0, 1]);
    }

    #[test]
    fn s4d_c4_a_writer_at_the_front_goes_alone_when_free() {
        assert_eq!(grantable(&[], &[X, S, S]), vec![0]);
        assert_eq!(grantable(&[X], &[S]), Vec::<usize>::new());
    }

    #[test]
    fn s4d_c4_nothing_waiting_nothing_granted() {
        assert_eq!(grantable(&[S], &[]), Vec::<usize>::new());
        assert_eq!(grantable(&[], &[]), Vec::<usize>::new());
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 512, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: the granted requests are a prefix of the queue, compatible with the holders and with each other; and the first refused one is
        /// really incompatible with a holder or an earlier grant.
        #[test]
        fn s4d_c4_property_the_grant_is_a_compatible_prefix(holders in proptest::collection::vec(prop_oneof![Just(S), Just(X)], 0..3), waiting in proptest::collection::vec(prop_oneof![Just(S), Just(X)], 0..6)) {
            // holders are mutually compatible in a real system: keep only legal sets
            prop_assume!(holders.iter().all(|&h| h == S) || holders.len() <= 1);
            let g = grantable(&holders, &waiting);
            prop_assert_eq!(g.clone(), (0..g.len()).collect::<Vec<_>>());
            let all_ok = |upto: usize| waiting[..upto].iter().all(|&a| holders.iter().all(|&h| h == S && a == S)) && (upto <= 1 || waiting[..upto].iter().all(|&a| a == S));
            let _ = all_ok;
            let mut modes: Vec<_> = holders.clone();
            modes.extend(g.iter().map(|&i| waiting[i]));
            prop_assert!(modes.len() <= 1 || modes.iter().all(|&m| m == S));
            if g.len() < waiting.len() {
                let next = waiting[g.len()];
                prop_assert!(!(modes.iter().all(|&m| m == S && next == S)), "the first refused request was grantable");
            }
        }
    }
}
// @@ challenge 4d-c4 end

// @@ challenge 4d-c5 begin
mod ch_4d_c5 {
    use proptest::prelude::*;

    use bustub::concurrency::escalation::{Escalator, RowLockResult::*};

    #[test]
    fn s4d_c5_the_row_after_the_threshold_escalates_and_then_everything_is_covered() {
        let mut e = Escalator::new(2);
        assert_eq!(e.row_lock(1, 7, 10), Granted);
        assert_eq!(e.row_lock(1, 7, 11), Granted);
        assert_eq!(e.row_lock(1, 7, 12), Escalate { table: 7, released: vec![10, 11] });
        assert!(e.holds_table(1, 7));
        assert_eq!(e.row_count(1, 7), 0);
        assert_eq!(e.row_lock(1, 7, 13), Covered);
    }

    #[test]
    fn s4d_c5_the_same_row_twice_is_not_counted_twice() {
        let mut e = Escalator::new(2);
        for _ in 0..5 {
            assert_eq!(e.row_lock(1, 7, 10), Granted);
        }
        assert_eq!(e.row_count(1, 7), 1);
    }

    #[test]
    fn s4d_c5_tables_and_transactions_are_counted_separately() {
        let mut e = Escalator::new(1);
        assert_eq!(e.row_lock(1, 7, 1), Granted);
        assert_eq!(e.row_lock(1, 8, 1), Granted);
        assert_eq!(e.row_lock(2, 7, 1), Granted);
        assert!(matches!(e.row_lock(1, 7, 2), Escalate { .. }));
        assert!(!e.holds_table(1, 8));
        assert!(!e.holds_table(2, 7));
        assert_eq!(e.row_lock(1, 8, 2), Escalate { table: 8, released: vec![1] });
    }

    #[test]
    fn s4d_c5_release_all_forgets_the_transaction() {
        let mut e = Escalator::new(1);
        e.row_lock(1, 7, 1);
        e.row_lock(1, 7, 2);
        assert!(e.holds_table(1, 7));
        e.release_all(1);
        assert!(!e.holds_table(1, 7));
        assert_eq!(e.row_lock(1, 7, 3), Granted);
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: row counts never exceed the threshold; escalation happens exactly at the threshold + 1-th distinct row; released rows are the held ones.
        #[test]
        fn s4d_c5_property_escalation_follows_the_count(threshold in 1usize..4, reqs in proptest::collection::vec((1u32..3, 1u32..3, 0u64..6), 0..30)) {
            let mut e = Escalator::new(threshold);
            let mut rows: std::collections::BTreeMap<(u32, u32), std::collections::BTreeSet<u64>> = Default::default();
            let mut tables: std::collections::BTreeSet<(u32, u32)> = Default::default();
            for (txn, table, row) in reqs {
                let key = (txn, table);
                let got = e.row_lock(txn, table, row);
                if tables.contains(&key) {
                    prop_assert_eq!(got, Covered);
                } else {
                    let set = rows.entry(key).or_default();
                    if set.contains(&row) {
                        prop_assert_eq!(got, Granted);
                    } else if set.len() + 1 > threshold {
                        prop_assert_eq!(got, Escalate { table, released: set.iter().copied().collect() });
                        rows.remove(&key);
                        tables.insert(key);
                    } else {
                        prop_assert_eq!(got, Granted);
                        set.insert(row);
                    }
                }
                prop_assert!(e.row_count(txn, table) <= threshold);
                prop_assert_eq!(e.holds_table(txn, table), tables.contains(&key));
            }
        }
    }
}
// @@ challenge 4d-c5 end

// @@ challenge 4d-c6 begin
mod ch_4d_c6 {
    use proptest::prelude::*;

    use bustub::concurrency::deadlock::DeadlockDetector;
    use bustub::concurrency::hybrid_store::HybridStore;
    use bustub::concurrency::lock_error::LockErrorKind;
    use bustub::concurrency::lock_manager::LockManager;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    fn finish_within<T: Send + 'static>(secs: u64, what: &'static str, f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(Duration::from_secs(secs)).unwrap_or_else(|_| panic!("{what}: did not finish in {secs}s (a wait that never ends?)"))
    }

    fn store_with(rows: &[(u32, i64)]) -> Arc<HybridStore> {
        let s = Arc::new(HybridStore::new(Arc::new(LockManager::new())));
        let t = s.begin();
        for &(k, v) in rows {
            s.put(&t, k, v).unwrap();
        }
        s.commit(t);
        s
    }

    #[test]
    fn s4d_c6_a_reader_keeps_its_snapshot_while_others_commit() {
        let s = store_with(&[(1, 10)]);
        let (a, b) = (s.begin(), s.begin());
        s.put(&b, 1, 50).unwrap();
        s.commit(b);
        assert_eq!(s.get(&a, 1), Some(10), "a began before b committed");
        let c = s.begin();
        assert_eq!(s.get(&c, 1), Some(50));
        assert_eq!(s.get(&a, 1), Some(10), "and keeps reading the same value");
        assert_eq!(s.get(&a, 2), None);
    }

    #[test]
    fn s4d_c6_a_read_does_not_wait_for_a_writer_holding_the_row() {
        let s = store_with(&[(1, 10)]);
        let w = s.begin();
        s.put(&w, 1, 99).unwrap();
        let s2 = s.clone();
        let seen = finish_within(5, "a snapshot read", move || {
            let r = s2.begin();
            s2.get(&r, 1)
        });
        assert_eq!(seen, Some(10), "the writer's uncommitted 99 must not be visible");
        assert_eq!(s.get(&w, 1), Some(99), "a writer sees its own write");
        s.abort(w);
    }

    #[test]
    fn s4d_c6_a_second_writer_waits_instead_of_aborting() {
        let s = store_with(&[(1, 0)]);
        let w1 = s.begin();
        s.put(&w1, 1, 1).unwrap();
        let done = Arc::new(AtomicBool::new(false));
        let h = {
            let (s, done) = (s.clone(), done.clone());
            std::thread::spawn(move || {
                let w2 = s.begin();
                s.put(&w2, 1, 2).unwrap();
                done.store(true, Ordering::SeqCst);
                s.commit(w2);
            })
        };
        std::thread::sleep(Duration::from_millis(100));
        assert!(!done.load(Ordering::SeqCst), "the second writer must wait for the row, not run ahead");
        s.commit(w1);
        finish_within(5, "second writer", move || h.join().unwrap());
        let r = s.begin();
        assert_eq!(s.get(&r, 1), Some(2), "the writer that waited committed last");
    }

    #[test]
    fn s4d_c6_abort_discards_the_writes_and_frees_the_row() {
        let s = store_with(&[(1, 10)]);
        let w = s.begin();
        s.put(&w, 1, 77).unwrap();
        s.abort(w);
        let s2 = s.clone();
        finish_within(5, "a writer after an abort", move || {
            let t = s2.begin();
            assert_eq!(s2.get(&t, 1), Some(10));
            s2.put(&t, 1, 11).unwrap();
            s2.commit(t);
        });
        let r = s.begin();
        assert_eq!(s.get(&r, 1), Some(11));
    }

    #[test]
    fn s4d_c6_for_update_reads_the_newest_committed_value_not_the_snapshot() {
        let s = store_with(&[(1, 0)]);
        let a = s.begin();
        assert_eq!(s.get(&a, 1), Some(0));
        let b = s.begin();
        s.put(&b, 1, 5).unwrap();
        s.commit(b);
        assert_eq!(s.get(&a, 1), Some(0), "the snapshot still says 0");
        assert_eq!(s.get_for_update(&a, 1), Ok(Some(5)), "but the locked read must see what is really there");
        s.put(&a, 1, 6).unwrap();
        s.commit(a);
        let r = s.begin();
        assert_eq!(s.get(&r, 1), Some(6), "an increment on top of 5, not on top of the stale 0");
    }

    #[test]
    fn s4d_c6_increments_from_many_threads_are_not_lost() {
        let s = store_with(&[(1, 0)]);
        finish_within(30, "counter", {
            let s = s.clone();
            move || {
                let hs: Vec<_> = (0..4)
                    .map(|_| {
                        let s = s.clone();
                        std::thread::spawn(move || {
                            for _ in 0..50 {
                                let t = s.begin();
                                let v = s.get_for_update(&t, 1).unwrap().unwrap_or(0);
                                s.put(&t, 1, v + 1).unwrap();
                                s.commit(t);
                            }
                        })
                    })
                    .collect();
                for h in hs {
                    h.join().unwrap();
                }
            }
        });
        let r = s.begin();
        assert_eq!(s.get(&r, 1), Some(200));
    }

    #[test]
    fn s4d_c6_a_deadlock_is_broken_and_the_survivor_commits() {
        let locks = Arc::new(LockManager::new());
        let _detector = DeadlockDetector::start(locks.clone(), Duration::from_millis(15));
        let s = Arc::new(HybridStore::new(locks));
        let (a, b) = (s.begin(), s.begin());
        s.put(&a, 1, 1).unwrap();
        s.put(&b, 2, 2).unwrap();
        let (s1, s2) = (s.clone(), s.clone());
        let ha = std::thread::spawn(move || {
            let r = s1.put(&a, 2, 10);
            match r {
                Ok(()) => {
                    s1.commit(a);
                    None
                }
                Err(e) => {
                    s1.abort(a);
                    Some(e.kind)
                }
            }
        });
        let hb = std::thread::spawn(move || {
            let r = s2.put(&b, 1, 20);
            match r {
                Ok(()) => {
                    s2.commit(b);
                    None
                }
                Err(e) => {
                    s2.abort(b);
                    Some(e.kind)
                }
            }
        });
        let (ra, rb) = finish_within(20, "deadlock", move || (ha.join().unwrap(), hb.join().unwrap()));
        let victims: Vec<_> = [ra, rb].into_iter().flatten().collect();
        assert_eq!(victims, vec![LockErrorKind::Deadlock], "exactly one transaction is the victim; the other commits");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 64, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: sequential transactions agree with a map; a reader opened at the start never sees any of them.
        #[test]
        fn s4d_c6_property_sequential_transactions_match_a_map_and_old_snapshots_stay_old(txns in proptest::collection::vec((proptest::collection::vec((0u32..5, -50i64..50), 0..4), any::<bool>()), 0..12)) {
            let s = store_with(&[(0, 100), (1, 101)]);
            let old = s.begin();
            let mut model: HashMap<u32, i64> = HashMap::from([(0, 100), (1, 101)]);
            for (writes, commit) in txns {
                let t = s.begin();
                for &(k, v) in &writes { s.put(&t, k, v).unwrap(); }
                if commit {
                    for &(k, v) in &writes { model.insert(k, v); }
                    s.commit(t);
                } else {
                    s.abort(t);
                }
                prop_assert_eq!(s.get(&old, 0), Some(100));
                prop_assert_eq!(s.get(&old, 1), Some(101));
                prop_assert_eq!(s.get(&old, 4), None);
            }
            let r = s.begin();
            for k in 0..5 { prop_assert_eq!(s.get(&r, k), model.get(&k).copied()); }
        }
    }
}
// @@ challenge 4d-c6 end

// @@ challenge 4d-c7 begin
mod ch_4d_c7 {

    use bustub::concurrency::anomaly_lab::{dirty_read, lost_update, non_repeatable_read, phantom, possible, write_skew, Anomaly};
    use bustub::concurrency::lab_db::{LabDb, Level, Level::*};

    /// The textbook table for the five levels of the lab database: `true` where the anomaly can happen.
    fn truth(level: Level, a: Anomaly) -> bool {
        use Anomaly::*;
        match (level, a) {
            (ReadUncommitted, _) => true,
            (_, DirtyRead) => false,
            (ReadCommitted, _) => true,
            (_, NonRepeatableRead | LostUpdate) => false,
            (Snapshot, WriteSkew) => true,
            (RepeatableRead, Phantom) => true,
            _ => false,
        }
    }

    fn observe(level: Level, rows: &[(i64, i64)], scenario: fn(&mut LabDb) -> bool) -> bool {
        let mut db = LabDb::new(level, rows);
        let happened = scenario(&mut db);
        assert_eq!(db.open_count(), 0, "{level:?}: the scenario left a transaction open");
        happened
    }

    fn check(a: Anomaly, rows: &[(i64, i64)], scenario: fn(&mut LabDb) -> bool) {
        for level in Level::ALL {
            assert_eq!(observe(level, rows, scenario), truth(level, a), "{a:?} at {level:?}");
        }
    }

    #[test]
    fn s4d_c7_a_dirty_read_is_reading_a_write_that_never_committed() {
        check(Anomaly::DirtyRead, &[(1, 100)], dirty_read);
    }

    #[test]
    fn s4d_c7_a_non_repeatable_read_is_two_reads_of_one_row_that_differ() {
        check(Anomaly::NonRepeatableRead, &[(1, 100)], non_repeatable_read);
    }

    #[test]
    fn s4d_c7_a_lost_update_is_two_increments_that_add_up_to_one() {
        check(Anomaly::LostUpdate, &[(1, 0)], lost_update);
    }

    #[test]
    fn s4d_c7_write_skew_breaks_a_constraint_that_each_transaction_kept() {
        check(Anomaly::WriteSkew, &[(1, 1), (2, 1)], write_skew);
    }

    #[test]
    fn s4d_c7_a_phantom_is_a_row_that_appears_in_a_range_you_already_read() {
        check(Anomaly::Phantom, &[(1, 10), (2, 20)], phantom);
    }

    #[test]
    fn s4d_c7_your_table_agrees_with_what_the_scenarios_saw() {
        use Anomaly::*;
        for level in Level::ALL {
            for a in [DirtyRead, NonRepeatableRead, LostUpdate, WriteSkew, Phantom] {
                assert_eq!(possible(level, a), truth(level, a), "possible({level:?}, {a:?})");
            }
        }
    }
}
// @@ challenge 4d-c7 end
