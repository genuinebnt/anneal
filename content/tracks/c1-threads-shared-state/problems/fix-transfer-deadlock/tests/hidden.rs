use solution::*;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[allow(dead_code)]
fn within<T: Send + 'static>(ms: u64, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(Duration::from_millis(ms)).ok()
}

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
fn probe_is_called_while_the_first_lock_is_held() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let c = calls.clone();
    let bank = Bank::with_probe(&[5, 5, 5], Arc::new(move |s| c.lock().unwrap().push(s)));
    let done = within(2000, move || {
        bank.transfer(2, 0, 1).unwrap();
        bank.transfer(1, 1, 1).unwrap();
        bank.total()
    });
    check!("transfer(2,0), transfer(1,1), total() finish", done, Some(15));
    let got = calls.lock().unwrap().clone();
    use Step::*;
    check!("transfer(2,0), transfer(1,1), total()", got, vec![HoldingFirst, HoldingFirst, Counted(0), Counted(1), Counted(2)]);
}

#[test]
fn ring_of_three_does_not_deadlock() {
    let arrived = Arc::new(AtomicUsize::new(0));
    let a = arrived.clone();
    let probe: Probe = Arc::new(move |step| {
        if step == Step::HoldingFirst {
            a.fetch_add(1, Ordering::SeqCst);
            let start = Instant::now();
            while a.load(Ordering::SeqCst) < 3 && start.elapsed() < Duration::from_millis(300) {
                std::thread::yield_now();
            }
        }
    });
    let bank = Arc::new(Bank::with_probe(&[10, 10, 10], probe));
    let b = bank.clone();
    let done = within(4000, move || {
        std::thread::scope(|s| {
            for (from, to) in [(0, 1), (1, 2), (2, 0)] {
                let b = &b;
                s.spawn(move || b.transfer(from, to, 3).unwrap());
            }
        });
        b.total()
    });
    check!("0->1, 1->2, 2->0 all paused holding one lock", done, Some(30));
}

#[test]
fn unrelated_transfers_do_not_wait() {
    let release = Arc::new(AtomicBool::new(false));
    let first = AtomicUsize::new(0);
    let r = release.clone();
    let probe: Probe = Arc::new(move |step| {
        // Only the first transfer is held up.
        if step == Step::HoldingFirst && first.fetch_add(1, Ordering::SeqCst) == 0 {
            let start = Instant::now();
            while !r.load(Ordering::SeqCst) && start.elapsed() < Duration::from_millis(3000) {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    });
    let bank = Arc::new(Bank::with_probe(&[10, 10, 10, 10], probe));
    let slow = {
        let bank = bank.clone();
        std::thread::spawn(move || bank.transfer(0, 1, 1))
    };
    std::thread::sleep(Duration::from_millis(100));
    let b = bank.clone();
    let other = within(1000, move || b.transfer(2, 3, 5));
    release.store(true, Ordering::SeqCst);
    let slow = slow.join().unwrap();
    check!("2->3 while 0->1 is paused holding a lock", (other, slow), (Some(Ok(())), Ok(())));
}

#[test]
fn unknown_to_account() {
    let bank = Bank::new(&[1]);
    check!("to out of range", (bank.transfer(0, 4, 1), bank.balance(0)), (Err(TransferError::UnknownAccount(4)), Some(1)));
}

#[test]
fn unknown_beats_insufficient() {
    let bank = Bank::new(&[1]);
    check!("too much, to an unknown account", bank.transfer(0, 9, 100), Err(TransferError::UnknownAccount(9)));
}

#[test]
fn zero_and_everything() {
    let bank = Bank::new(&[0, 7]);
    check!("move 0 from an empty account", bank.transfer(0, 1, 0), Ok(()));
    check!("move everything", (bank.transfer(1, 0, 7), bank.balance(0), bank.balance(1)), (Ok(()), Some(7), Some(0)));
    check!("empty bank total", Bank::new(&[]).total(), 0);
}

#[test]
fn many_threads_keep_the_money() {
    let bank = Arc::new(Bank::new(&[1000; 5]));
    let b = bank.clone();
    let done = within(8000, move || {
        let audits_ok = AtomicBool::new(true);
        std::thread::scope(|s| {
            for t in 0..4u64 {
                let b = &b;
                s.spawn(move || {
                    let mut rng = anneal_prelude::Rng::new(8106 + t);
                    for _ in 0..4000 {
                        let (from, to) = (rng.below(5), rng.below(5));
                        let _ = b.transfer(from, to, rng.below(300) as u64);
                    }
                });
            }
            s.spawn(|| {
                for _ in 0..2000 {
                    if b.total() != 5000 {
                        audits_ok.store(false, Ordering::SeqCst);
                    }
                }
            });
        });
        (audits_ok.load(Ordering::SeqCst), b.total())
    });
    check!("4 threads of random transfers plus an auditor: (every audit 5000, final total)", done, Some((true, 5000)));
}

#[test]
fn random_sequence_matches_a_model() {
    let mut rng = anneal_prelude::Rng::new(8110);
    for _ in 0..300 {
        let n = 1 + rng.below(4);
        let mut model: Vec<u64> = rng.vec(n, 0, 20);
        let bank = Arc::new(Bank::new(&model));
        for _ in 0..12 {
            let (from, to, amount) = (rng.below(n + 1), rng.below(n + 1), rng.below(25) as u64);
            let want = if from >= n {
                Err(TransferError::UnknownAccount(from))
            } else if to >= n {
                Err(TransferError::UnknownAccount(to))
            } else if model[from] < amount {
                Err(TransferError::Insufficient)
            } else {
                model[from] -= amount;
                model[to] += amount;
                Ok(())
            };
            // Same-account calls are made on a watched thread, since a double lock would hang.
            let got = if from == to {
                let b = bank.clone();
                within(1000, move || b.transfer(from, to, amount)).expect("transfer to the same account hung")
            } else {
                bank.transfer(from, to, amount)
            };
            check!(format!("transfer({from}, {to}, {amount}) on {n} accounts"), got, want);
        }
        let got: Vec<u64> = (0..n).map(|i| bank.balance(i).unwrap()).collect();
        check!("balances after 12 random transfers", got, model);
    }
}
