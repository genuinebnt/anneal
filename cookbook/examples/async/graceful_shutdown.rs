//! RECIPE 22 (async): graceful shutdown: stop accepting work, drain what is queued, then exit.
//! LEVEL: intermediate
//! Use in: worker pool / service stages.   Run: cargo run --example graceful_shutdown
//! COMMON USES: servers and workers: finish accepted work before a restart or deploy instead of dropping it. This
//! example sends the stop signal from `main`; it does not listen for operating system signals such as SIGTERM.
//!
//! Pattern: a `watch` channel broadcasts "shutting down". The worker `select!`s between a job and that
//! signal; when it sees it, it calls `close` on the job channel and then `recv`s until `None`. After `close`, new sends
//! fail and return the job, while queued jobs and slots already reserved with `reserve` are still received. So every job
//! the channel accepted is run, as long as the worker is not aborted or panics. There is no time limit: a slot that is
//! reserved and never used or dropped keeps the worker waiting. `try_recv` until it fails is not enough: "empty" does
//! not mean "nothing more is coming".
//! Docs: https://tokio.rs/tokio/topics/shutdown
#![allow(dead_code)]
use std::time::Duration;
use tokio::sync::{mpsc, watch};

async fn process(job: u32) {
    tokio::time::sleep(Duration::from_millis(5)).await;
    println!("processed job {job}");
}

/// Runs jobs until told to stop (or all senders are gone), then drains. Returns the jobs it ran, in order.
/// `after` is a hook the self-checks use to act as a producer between two jobs.
async fn run_worker(
    mut job_rx: mpsc::Receiver<u32>,
    mut stop_rx: watch::Receiver<bool>,
    mut after: impl FnMut(u32) + Send,
) -> Vec<u32> {
    let mut done = Vec::new();
    loop {
        tokio::select! {
            _ = stop_rx.changed() => break,
            job = job_rx.recv() => match job {
                Some(j) => { process(j).await; done.push(j); after(j); }
                None => break,
            },
        }
    }
    job_rx.close(); // refuse new sends; what was already accepted can still be received
    while let Some(j) = job_rx.recv().await {
        process(j).await;
        done.push(j);
        after(j);
    }
    done
}

/// A sender that reserved a slot before shutdown was told "there is room"; its job must run.
async fn check_reserved_slot_is_honoured() {
    let (job_tx, job_rx) = mpsc::channel::<u32>(16);
    let (stop_tx, stop_rx) = watch::channel(false);
    let permit = job_tx.reserve().await.unwrap();
    stop_tx.send(true).unwrap();
    let worker = tokio::spawn(run_worker(job_rx, stop_rx, |_| {}));
    job_tx.closed().await; // the worker has closed (or dropped) its receiver
    permit.send(7);
    let done = worker.await.unwrap();
    assert_eq!(done, [7], "a job sent through a reserved slot was lost");
}

/// A producer that sends again after every job must be refused once shutdown begins, so the drain ends.
async fn check_busy_producer_cannot_keep_the_worker_alive() {
    const CAP: u32 = 200; // a drain that never closes the channel would run into this cap
    let (job_tx, job_rx) = mpsc::channel::<u32>(16);
    let (stop_tx, stop_rx) = watch::channel(false);
    job_tx.send(0).await.unwrap();
    stop_tx.send(true).unwrap();
    let accepted = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(1));
    let count = accepted.clone();
    let done = tokio::spawn(run_worker(job_rx, stop_rx, move |j| {
        if j + 1 < CAP && job_tx.try_send(j + 1).is_ok() {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }))
    .await
    .unwrap();
    let accepted = accepted.load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        accepted < CAP,
        "the producer was never refused ({accepted} jobs accepted)"
    );
    assert_eq!(
        done,
        (0..accepted).collect::<Vec<_>>(),
        "every accepted job runs once, in order"
    );
}

#[tokio::main]
async fn main() {
    let (job_tx, job_rx) = mpsc::channel::<u32>(16);
    let (stop_tx, stop_rx) = watch::channel(false);

    let worker = tokio::spawn(run_worker(job_rx, stop_rx, |_| {}));

    for j in 0..5 {
        job_tx.send(j).await.unwrap();
    }
    stop_tx.send(true).unwrap(); // shutdown requested immediately after queuing
    let done = worker.await.unwrap();
    println!("worker exited after processing {} jobs", done.len());
    assert_eq!(done, [0, 1, 2, 3, 4]);

    check_reserved_slot_is_honoured().await;
    println!("a job sent through a reserved slot ran");
    check_busy_producer_cannot_keep_the_worker_alive().await;
    println!("a busy producer was refused and the drain ended");
}
