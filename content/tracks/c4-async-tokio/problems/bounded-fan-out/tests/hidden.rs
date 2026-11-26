use solution::*;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::{Instant, sleep};

#[allow(dead_code)]
fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap().block_on(f)
}

/// Counts the futures that exist: created and not yet dropped.
struct Alive(Arc<AtomicUsize>);

impl Alive {
    fn new(n: &Arc<AtomicUsize>) -> Self {
        n.fetch_add(1, Ordering::SeqCst);
        Alive(n.clone())
    }
}

impl Drop for Alive {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[test]
fn f_is_called_only_when_a_slot_is_free() {
    run(async {
        let alive = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let _: Result<Vec<()>, ()> = fan_out((0..30).collect(), 3, |_: u32| {
            let guard = Alive::new(&alive);
            peak.fetch_max(alive.load(Ordering::SeqCst), Ordering::SeqCst);
            async move {
                let _guard = guard;
                sleep(Duration::from_millis(3)).await;
                Ok(())
            }
        })
        .await;
        check!("30 inputs, limit 3: most futures alive at once", peak.load(Ordering::SeqCst), 3);
    });
}

#[test]
fn inputs_start_in_order() {
    run(async {
        let order = Arc::new(Mutex::new(Vec::new()));
        let _: Result<Vec<()>, ()> = fan_out((0..8).collect(), 3, |i: u32| {
            order.lock().unwrap().push(i);
            async move {
                sleep(Duration::from_millis(u64::from(9 - i))).await;
                Ok(())
            }
        })
        .await;
        let got = order.lock().unwrap().clone();
        check!("order of calls to f", got, (0..8).collect::<Vec<_>>());
    });
}

#[test]
fn error_leaves_nothing_running() {
    run(async {
        let alive = Arc::new(AtomicUsize::new(0));
        let got: Result<Vec<()>, &str> = fan_out((0..5).collect(), 5, |i: u32| {
            let guard = Alive::new(&alive);
            async move {
                let _guard = guard;
                sleep(Duration::from_millis(if i == 2 { 5 } else { 1000 })).await;
                if i == 2 { Err("two") } else { Ok(()) }
            }
        })
        .await;
        let left = alive.load(Ordering::SeqCst);
        check!("error from input 2: (result, futures still alive on return)", (got, left), (Err("two"), 0));
    });
}

#[test]
fn error_does_not_wait_for_the_others() {
    run(async {
        let start = Instant::now();
        let got: Result<Vec<()>, ()> = fan_out(vec![10_000u64, 5], 2, |ms| async move {
            sleep(Duration::from_millis(ms)).await;
            if ms == 5 { Err(()) } else { Ok(()) }
        })
        .await;
        check!("a 10 s task and a 5 ms failure", (got, start.elapsed()), (Err(()), Duration::from_millis(5)));
    });
}

#[test]
fn no_input_is_started_after_the_error() {
    run(async {
        let calls = Arc::new(AtomicUsize::new(0));
        let got: Result<Vec<()>, u32> = fan_out((0..100).collect(), 2, |i: u32| {
            calls.fetch_add(1, Ordering::SeqCst);
            async move {
                sleep(Duration::from_millis(if i == 0 { 1 } else { 50 })).await;
                if i == 0 { Err(i) } else { Ok(()) }
            }
        })
        .await;
        check!("input 0 fails at once, limit 2: (result, calls to f)", (got, calls.load(Ordering::SeqCst)), (Err(0), 2));
    });
}

#[test]
fn limit_one_is_sequential() {
    run(async {
        let start = Instant::now();
        let got: Result<Vec<u64>, ()> = fan_out(vec![5u64, 5, 5, 5], 1, |ms| async move {
            sleep(Duration::from_millis(ms)).await;
            Ok(ms)
        })
        .await;
        check!("four 5 ms tasks, limit 1", (got, start.elapsed()), (Ok(vec![5; 4]), Duration::from_millis(20)));
    });
}

#[test]
fn limit_larger_than_input_runs_all_together() {
    run(async {
        let start = Instant::now();
        let _: Result<Vec<()>, ()> = fan_out(vec![(); 10], 1000, |()| async {
            sleep(Duration::from_millis(7)).await;
            Ok(())
        })
        .await;
        check!("ten 7 ms tasks, limit 1000", start.elapsed(), Duration::from_millis(7));
    });
}

#[test]
fn work_runs_on_spawned_tasks() {
    run(async {
        let caller = tokio::spawn(async {
            let me = tokio::task::id();
            let ids: Result<Vec<bool>, ()> = fan_out(vec![1, 2, 3], 2, move |_| async move { Ok(tokio::task::id() != me) }).await;
            ids
        });
        check!("each input ran on another task", caller.await.unwrap(), Ok(vec![true; 3]));
    });
}

#[test]
#[should_panic(expected = "boom 7")]
fn a_panic_is_passed_on() {
    run(async {
        let _: Result<Vec<()>, ()> = fan_out(vec![1, 7, 3], 3, |x: u32| async move {
            if x == 7 {
                panic!("boom {x}");
            }
            sleep(Duration::from_millis(1)).await;
            Ok(())
        })
        .await;
    });
}

#[test]
#[should_panic]
fn zero_limit_panics() {
    run(async {
        let _: Result<Vec<u8>, ()> = tokio::time::timeout(Duration::from_secs(1), fan_out(vec![1u8], 0, |x| async move { Ok(x) })).await.unwrap_or(Ok(Vec::new()));
    });
}

#[test]
fn random_schedules_match_a_simulation() {
    let mut rng = anneal_prelude::Rng::new(8104);
    for _ in 0..200 {
        let n = rng.below(12);
        let limit = 1 + rng.below(4);
        let delays: Vec<u64> = rng.vec(n, 1, 20);
        // The finishing time of a list schedule with `limit` workers, taking inputs in order.
        let mut workers = vec![0u64; limit];
        for d in &delays {
            let next = workers.iter_mut().min().unwrap();
            *next += d;
        }
        let want_time = workers.into_iter().max().unwrap();
        let (got, took) = run(async {
            let start = Instant::now();
            let got: Result<Vec<u64>, ()> = fan_out(delays.clone(), limit, |ms| async move {
                sleep(Duration::from_millis(ms)).await;
                Ok(ms)
            })
            .await;
            (got, start.elapsed().as_millis() as u64)
        });
        check!(format!("delays {delays:?}, limit {limit}: (outputs, virtual ms)"), (got, took), (Ok(delays.clone()), want_time));
    }
}
