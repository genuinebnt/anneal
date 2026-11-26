use solution::*;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::time::{Instant, sleep};

#[allow(dead_code)]
fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap().block_on(f)
}

#[test]
fn results_in_input_order() {
    run(async {
        let got: Result<Vec<u64>, String> = fan_out(vec![30u64, 10, 20], 3, |ms| async move {
            sleep(Duration::from_millis(ms)).await;
            Ok(ms * 2)
        })
        .await;
        check!("delays 30, 10, 20 ms", got, Ok(vec![60, 20, 40]));
    });
}

#[test]
fn never_more_than_the_limit() {
    run(async {
        let now = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let got: Result<Vec<()>, ()> = fan_out((0..20).collect(), 4, |_: u32| {
            let (now, peak) = (now.clone(), peak.clone());
            async move {
                peak.fetch_max(now.fetch_add(1, Ordering::SeqCst) + 1, Ordering::SeqCst);
                sleep(Duration::from_millis(5)).await;
                now.fetch_sub(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .await;
        check!("20 inputs, limit 4: highest in flight", (got.map(|v| v.len()), peak.load(Ordering::SeqCst)), (Ok(20), 4));
    });
}

#[test]
fn a_slow_input_does_not_block_the_rest() {
    run(async {
        let start = Instant::now();
        let delays = vec![100u64, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10];
        let _: Result<Vec<()>, ()> = fan_out(delays, 2, |ms| async move {
            sleep(Duration::from_millis(ms)).await;
            Ok(())
        })
        .await;
        check!("one 100 ms and ten 10 ms, limit 2: virtual time", start.elapsed(), Duration::from_millis(100));
    });
}

#[test]
fn first_error_in_time_wins() {
    run(async {
        let got: Result<Vec<()>, u64> = fan_out(vec![50u64, 20, 90], 3, |ms| async move {
            sleep(Duration::from_millis(ms)).await;
            Err(ms)
        })
        .await;
        check!("errors after 50, 20, 90 ms", got, Err(20));
    });
}

#[test]
fn no_inputs() {
    run(async {
        let got: Result<Vec<u8>, ()> = fan_out(Vec::<u8>::new(), 1, |x| async move { Ok(x) }).await;
        check!("empty input", got, Ok(Vec::new()));
    });
}
