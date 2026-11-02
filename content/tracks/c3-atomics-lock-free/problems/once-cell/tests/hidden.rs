use solution::*;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct Probe(Arc<AtomicUsize>, u32);
impl Drop for Probe {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn value_dropped_exactly_once() {
    let drops = Arc::new(AtomicUsize::new(0));
    {
        let c = Once::new();
        c.get_or_init(|| Probe(drops.clone(), 1));
        c.get_or_init(|| Probe(drops.clone(), 2));
        check!("two get_or_init calls, drop the cell", drops.load(Ordering::SeqCst), 0);
    }
    check!("after the cell is dropped", drops.load(Ordering::SeqCst), 1);
}

#[test]
fn empty_cell_drops_nothing() {
    let drops = Arc::new(AtomicUsize::new(0));
    drop(Once::<Probe>::new());
    check!("an empty cell is dropped", drops.load(Ordering::SeqCst), 0);
}

#[test]
fn into_inner_moves_the_value_out_without_dropping_it() {
    let drops = Arc::new(AtomicUsize::new(0));
    let c = Once::new();
    c.get_or_init(|| Probe(drops.clone(), 7));
    let p = c.into_inner().unwrap();
    check!("into_inner, value still alive", (drops.load(Ordering::SeqCst), p.1), (0, 7));
    drop(p);
    check!("after dropping the moved value", drops.load(Ordering::SeqCst), 1);
}

#[test]
fn panic_leaves_it_empty() {
    check!(r#"initializer panics, then another succeeds"#, { let c = Once::new(); let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { c.get_or_init(|| -> u32 { panic!("boom") }); })); (r.is_err(), c.get().copied(), *c.get_or_init(|| 9)) }, (true, None, 9));
}

#[test]
fn panic_twice_then_ok() {
    check!(r#"two failed initializers, then success"#, { let c = Once::new(); for _ in 0..2 { let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { c.get_or_init(|| -> u8 { panic!("no") }); })); } *c.get_or_init(|| 4) }, 4);
}

#[test]
fn initializer_not_run_when_full() {
    check!(r#"closure counts its calls"#, { let c = Once::new(); let mut calls = 0; c.get_or_init(|| { calls += 1; 1 }); c.get_or_init(|| { calls += 1; 2 }); calls }, 1);
}

#[test]
fn borrowing_initializer() {
    check!(r#"FnOnce moves a captured String"#, { let s = String::from("moved"); let c = Once::new(); c.get_or_init(move || s).len() }, 5);
}

#[test]
fn sixteen_threads_one_initializer() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    let cell = Arc::new(Once::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(Barrier::new(16));
    let hs: Vec<_> = (0..16usize)
        .map(|t| {
            let (cell, calls, barrier) = (cell.clone(), calls.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                let r = cell.get_or_init(|| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    vec![t; 1000]
                });
                // every thread must see the finished vector, never a half-written one
                (r.len(), r.iter().all(|&x| x == r[0]), r as *const Vec<usize> as usize)
            })
        })
        .collect();
    let results: Vec<_> = hs.into_iter().map(|h| h.join().unwrap()).collect();
    let same_addr = results.iter().all(|r| r.2 == results[0].2);
    check!("16 racing callers: initializer calls", calls.load(Ordering::SeqCst), 1);
    check!("all see len 1000 and one address", (results.iter().all(|r| r.0 == 1000 && r.1), same_addr), (true, true));
}

#[test]
fn many_cells_many_rounds() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    for _ in 0..200 {
        let cell = Arc::new(Once::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let hs: Vec<_> = (0..4)
            .map(|_| {
                let (cell, calls) = (cell.clone(), calls.clone());
                std::thread::spawn(move || *cell.get_or_init(|| { calls.fetch_add(1, Ordering::SeqCst); 42u64 }))
            })
            .collect();
        let vals: Vec<u64> = hs.into_iter().map(|h| h.join().unwrap()).collect();
        check!("4 threads racing on a fresh cell", (vals, calls.load(Ordering::SeqCst)), (vec![42; 4], 1));
    }
}

#[test]
fn is_sync_and_send_for_the_right_types() {
    fn needs_sync<T: Sync>() {}
    fn needs_send<T: Send>() {}
    needs_sync::<Once<String>>();
    needs_send::<Once<String>>();
    needs_sync::<Once<Vec<u8>>>();
}
