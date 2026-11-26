use solution::*;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

/// Runs `f` on another thread and gives up after `ms`, so a deadlock fails the test instead of hanging it.
#[allow(dead_code)]
fn within<T: Send + 'static>(ms: u64, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(Duration::from_millis(ms)).ok()
}

/// A probe that makes each of the first two transfers wait, while holding its first lock, until the other holds one too.
/// It stops waiting after 300 ms, for the case where the second transfer cannot get a first lock yet.
#[allow(dead_code)]
fn meet_while_holding_one() -> Probe {
    let arrived = Arc::new(AtomicUsize::new(0));
    Arc::new(move |step| {
        if step == Step::HoldingFirst {
            arrived.fetch_add(1, Ordering::SeqCst);
            let start = Instant::now();
            while arrived.load(Ordering::SeqCst) < 2 && start.elapsed() < Duration::from_millis(300) {
                std::thread::yield_now();
            }
        }
    })
}

#[test]
fn moves_money() {
    let bank = Bank::new(&[100, 5]);
    check!("100 -> 30 -> second", (bank.transfer(0, 1, 30), bank.balance(0), bank.balance(1), bank.total()), (Ok(()), Some(70), Some(35), 105));
}

#[test]
fn refuses_without_changing_anything() {
    let bank = Bank::new(&[10, 0]);
    check!("11 from a balance of 10", (bank.transfer(0, 1, 11), bank.balance(0), bank.balance(1)), (Err(TransferError::Insufficient), Some(10), Some(0)));
    check!("unknown accounts, from first", bank.transfer(7, 9, 1), Err(TransferError::UnknownAccount(7)));
}

#[test]
fn transfer_to_itself() {
    let bank = Arc::new(Bank::new(&[10]));
    let b = bank.clone();
    check!("transfer(0, 0, 10), then (0, 0, 11)", within(2000, move || (b.transfer(0, 0, 10), b.transfer(0, 0, 11))), Some((Ok(()), Err(TransferError::Insufficient))));
    check!("balance unchanged", bank.balance(0), Some(10));
}

#[test]
fn opposite_transfers_at_the_worst_moment() {
    let bank = Arc::new(Bank::with_probe(&[100, 100], meet_while_holding_one()));
    let b = bank.clone();
    let done = within(3000, move || {
        std::thread::scope(|s| {
            s.spawn(|| b.transfer(0, 1, 10).unwrap());
            s.spawn(|| b.transfer(1, 0, 25).unwrap());
        });
        (b.balance(0), b.balance(1))
    });
    check!("0->1 and 1->0, each paused holding its first lock", done, Some((Some(115), Some(85))));
}

#[test]
fn total_is_one_moment() {
    // Just after `total` reads account 0, another thread moves 40 from account 1 to account 0.
    let slot: Arc<OnceLock<Arc<Bank>>> = Arc::new(OnceLock::new());
    let s = slot.clone();
    let fired = AtomicUsize::new(0);
    let probe: Probe = Arc::new(move |step| {
        if step == Step::Counted(0) && fired.fetch_add(1, Ordering::SeqCst) == 0 {
            let bank = s.get().unwrap().clone();
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let _ = tx.send(bank.transfer(1, 0, 40));
            });
            // A correct bank makes the transfer wait for `total`, so this times out.
            let _ = rx.recv_timeout(Duration::from_millis(300));
        }
    });
    let bank = Arc::new(Bank::with_probe(&[50, 50], probe));
    slot.set(bank.clone()).ok().unwrap();
    let b = bank.clone();
    check!("total() while 40 moves from account 1 to account 0", within(3000, move || b.total()), Some(100));
}
