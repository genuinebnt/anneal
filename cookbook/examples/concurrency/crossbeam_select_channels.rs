//! RECIPE 443 (concurrency): crossbeam-channel patterns: rendezvous, select!, timers, pipelines, request/reply, broadcast.
//! LEVEL: intermediate
//! Use in: thread pipelines, worker pools, timeouts and tickers, request/reply between threads.   Run: cargo run --release --example crossbeam_select_channels
#![allow(dead_code)]
//!
//! WHAT: a tour of crossbeam-channel, an MPMC channel (both `Sender` and `Receiver` are `Clone`), and the idioms
//! built on it. Every section asserts its claims, so the demo doubles as a test.
//! CAPACITIES: `unbounded()` never blocks the sender (memory is the limit); `bounded(n)` blocks the sender when n
//! items are queued, which is BACKPRESSURE: a fast producer is slowed to the speed of the slowest consumer;
//! `bounded(0)` is a RENDEZVOUS channel: `send` completes only when a receiver takes the value, so a successful send
//! means "the other side has it" (a synchronisation point, not just a queue).
//! DISCONNECT SEMANTICS: when ALL senders are dropped, `recv` first drains queued items, then returns `Err`;
//! `for x in rx` ends. When all receivers are dropped, `send` returns the value back in `Err`. So shutdown is just
//! "drop the senders": no sentinel values. The classic bug is keeping one spare `Sender` clone alive in main, which
//! makes the consumers wait forever.
//! SELECT: `select!` waits on several operations and runs the branch of the first ready one (fair random choice among
//! ready ones); `default` makes it non-blocking, `default(timeout)` bounds the wait. `Select` is the dynamic form
//! for a runtime-sized list of channels. `after(d)` is a channel that delivers ONE `Instant` after d, `tick(d)` one
//! every d: timers without extra threads. `never()` is a channel that never fires (disable a branch).
//! CLONING A RECEIVER IS NOT BROADCAST: clones share one queue, each message goes to exactly ONE of them (work
//! sharing). Broadcast needs one channel per subscriber and a loop that sends a clone to each (shown below).
//! REQUEST/REPLY: put a `Sender<Reply>` inside the request; the server answers on it. A `bounded(1)` channel used
//! once is a one-shot: the reply `send` never blocks and a dropped server surfaces as `RecvError`.
//! ORDERING: messages from ONE sender arrive in send order (FIFO queue). Across senders there is no global order.
//! After multi-worker stages, only the order along one FIFO path (same worker at every stage) survives; the pipeline checks exactly that.
//!
//! SEMANTICS TABLE (verify against the docs before relying on corner cases)
//! | feature            | std::sync::mpsc          | crossbeam-channel         | tokio::sync::mpsc / oneshot / broadcast / watch |
//! | consumers          | single receiver          | MPMC, Receiver: Clone     | mpsc single; broadcast = every receiver sees all|
//! | bounded            | sync_channel(n)          | bounded(n), bounded(0)    | mpsc::channel(n) (n>=1, no rendezvous)          |
//! | select             | none                     | select!, Select           | tokio::select! over futures                     |
//! | blocking           | blocking recv            | blocking recv             | async `.await`; blocking_recv for threads       |
//! | timers             | recv_timeout             | after, tick, recv_timeout | tokio::time::{sleep, interval}                  |
//! | disconnect         | Err when senders gone    | same, both directions     | recv -> None when senders gone                  |
//! COMMON USES: thread-based pipelines (parse -> transform -> write), worker pools with graceful stop by dropping
//! senders, rendezvous hand-off between a producer and a consumer thread, a `tick` driven housekeeping loop next to
//! a work channel (metrics flush, heartbeat), `select!` on data + shutdown channel, rayon/crossbeam style scoped
//! workers, request/reply actors (see the actor_model recipe).
//! Read: "Crossbeam: Channels" https://docs.rs/crossbeam-channel/latest/crossbeam_channel/ and https://github.com/crossbeam-rs/crossbeam and the Rust Book chapter "Message Passing" https://doc.rust-lang.org/book/ch16-02-message-passing.html
use crossbeam_channel::{
    RecvTimeoutError, Select, SendTimeoutError, TryRecvError, TrySendError, after, bounded, never,
    select, tick, unbounded,
};
use std::sync::atomic::{AtomicUsize, Ordering::*};
use std::thread;
use std::time::{Duration, Instant};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

// ---------------------------------------------------------------- 1. capacities

fn capacities() {
    // unbounded: sends never block, FIFO.
    let (tx, rx) = unbounded();
    for i in 0..1000 {
        tx.send(i).unwrap();
    }
    assert_eq!(rx.len(), 1000);
    assert_eq!(rx.recv().unwrap(), 0);

    // bounded(2): the third try_send reports Full and hands the value back.
    let (tx, rx) = bounded(2);
    tx.send('a').unwrap();
    tx.send('b').unwrap();
    assert!(matches!(tx.try_send('c'), Err(TrySendError::Full('c'))));
    assert!(matches!(
        tx.send_timeout('c', ms(5)),
        Err(SendTimeoutError::Timeout('c'))
    ));
    assert_eq!(rx.recv().unwrap(), 'a');
    tx.try_send('c').unwrap(); // room again

    // bounded(0): rendezvous. With nobody waiting, try_send must fail.
    let (tx, rx) = bounded::<u32>(0);
    assert!(matches!(tx.try_send(1), Err(TrySendError::Full(1))));
    let handle = thread::spawn(move || {
        thread::sleep(ms(30));
        rx.recv().unwrap()
    });
    let t = Instant::now();
    tx.send(7).unwrap(); // blocks until the receiver thread arrives
    let waited = t.elapsed();
    assert_eq!(handle.join().unwrap(), 7);
    assert!(rx_is_gone(&tx), "receiver dropped: send now fails");
    println!("1. capacities: rendezvous send blocked {waited:?} until the receiver arrived");
}

fn rx_is_gone(tx: &crossbeam_channel::Sender<u32>) -> bool {
    tx.send(0).is_err()
}

// ---------------------------------------------------------------- 2. disconnect

fn disconnect_semantics() {
    let (tx, rx) = unbounded();
    tx.send(1).unwrap();
    tx.send(2).unwrap();
    drop(tx);
    // Queued items are still delivered after the last sender is gone...
    assert_eq!(rx.recv(), Ok(1));
    assert_eq!(rx.try_recv(), Ok(2));
    // ...and only then does the channel report disconnection (not "empty").
    assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
    assert_eq!(rx.recv_timeout(ms(1)), Err(RecvTimeoutError::Disconnected));

    let (tx, rx) = unbounded::<i32>();
    assert_eq!(rx.try_recv(), Err(TryRecvError::Empty)); // alive but empty
    drop(rx);
    let err = tx.send(9).unwrap_err();
    assert_eq!(err.into_inner(), 9, "the value is returned, not lost");
    println!("2. disconnect: drain-then-Err on receivers, value handed back on senders");
}

// ---------------------------------------------------------------- 3. select

fn select_patterns() {
    let (data_tx, data_rx) = unbounded::<u32>();
    let (stop_tx, stop_rx) = bounded::<()>(0);

    // Non-blocking poll: `default` runs when nothing is ready.
    select! {
        recv(data_rx) -> _ => panic!("nothing was sent"),
        default => {}
    }

    // Timeout branch: nothing arrives within 20 ms.
    let t = Instant::now();
    select! {
        recv(data_rx) -> _ => panic!("nothing was sent"),
        default(ms(20)) => {}
    }
    assert!(t.elapsed() >= ms(20));

    // Data + shutdown channel, the canonical worker loop.
    let worker = thread::spawn(move || {
        let mut sum = 0u32;
        loop {
            select! {
                recv(data_rx) -> m => match m {
                    Ok(v) => sum += v,
                    Err(_) => break, // all senders dropped
                },
                recv(stop_rx) -> _ => break, // stop requested (or stop sender dropped)
            }
        }
        sum
    });
    for v in 1..=100 {
        data_tx.send(v).unwrap();
    }
    // Rendezvous stop: returns only once the worker has received it. We sleep first so the worker has drained the
    // queue (select picks randomly among ready branches, so a stop sent too early could cut the drain short).
    thread::sleep(ms(20));
    stop_tx.send(()).unwrap();
    let sum = worker.join().unwrap();
    assert_eq!(
        sum, 5050,
        "20 ms is plenty for a worker to drain 100 queued items"
    );

    // Dynamic `Select` over a Vec of receivers; each channel gets one message.
    let chans: Vec<_> = (0..5).map(|_| unbounded::<usize>()).collect();
    for (i, (tx, _)) in chans.iter().enumerate() {
        tx.send(i * 10).unwrap();
    }
    let mut seen = Vec::new();
    for _ in 0..5 {
        let mut sel = Select::new();
        for (_, rx) in &chans {
            sel.recv(rx);
        }
        let op = sel.select();
        let idx = op.index();
        let v = op.recv(&chans[idx].1).unwrap();
        assert_eq!(v, idx * 10);
        seen.push(idx);
    }
    seen.sort();
    assert_eq!(seen, [0, 1, 2, 3, 4]);

    // `never()` disables a branch: it is selectable but never ready.
    let off = never::<u8>();
    let (tx, rx) = unbounded();
    tx.send(5u8).unwrap();
    select! {
        recv(off) -> _ => panic!("never fires"),
        recv(rx) -> v => assert_eq!(v, Ok(5)),
    }
    println!("3. select: default, timeout, shutdown channel, dynamic Select over 5 channels");
}

// ---------------------------------------------------------------- 4. timers

fn timers() {
    // `after`: one-shot deadline shared by a whole loop (a single deadline, not a per-iteration timeout).
    let deadline = after(ms(60));
    let ticker = tick(ms(10));
    let mut ticks = 0;
    loop {
        select! {
            recv(ticker) -> _ => ticks += 1,
            recv(deadline) -> _ => break,
        }
    }
    // Timing is machine dependent, so only assert loose bounds.
    assert!((2..=7).contains(&ticks), "ticks = {ticks}");
    println!("4. timers: {ticks} ticks of 10 ms before a 60 ms `after` deadline");
}

// ---------------------------------------------------------------- 5. request/reply

enum Request {
    Square {
        n: u64,
        reply: crossbeam_channel::Sender<u64>,
    },
    Shutdown,
}

fn request_reply() {
    let (tx, rx) = unbounded::<Request>();
    let server = thread::spawn(move || {
        let mut handled = 0;
        for req in rx {
            match req {
                Request::Square { n, reply } => {
                    // One-shot reply: bounded(1) so this never blocks; ignore a caller that gave up.
                    let _ = reply.send(n * n);
                    handled += 1;
                }
                Request::Shutdown => break,
            }
        }
        handled
    });
    let clients: Vec<_> = (0..4u64)
        .map(|c| {
            let tx = tx.clone();
            thread::spawn(move || {
                for i in 0..250 {
                    let n = c * 1000 + i;
                    let (rtx, rrx) = bounded(1);
                    tx.send(Request::Square { n, reply: rtx }).unwrap();
                    assert_eq!(rrx.recv().unwrap(), n * n);
                }
            })
        })
        .collect();
    for c in clients {
        c.join().unwrap();
    }
    tx.send(Request::Shutdown).unwrap();
    assert_eq!(server.join().unwrap(), 1000);

    // A server that dies without replying shows up as a disconnect, not a hang.
    let (tx, rx) = unbounded::<Request>();
    drop(rx);
    let (rtx, rrx) = bounded::<u64>(1);
    assert!(tx.send(Request::Square { n: 1, reply: rtx }).is_err()); // request (and its reply sender) dropped
    assert!(rrx.recv().is_err());
    println!("5. request/reply: 1000 round trips over one-shot channels, dead server -> Err");
}

// ---------------------------------------------------------------- 6. receiver clones vs broadcast

fn clones_are_not_broadcast() {
    let (tx, rx) = unbounded::<u32>();
    let counts: Vec<_> = (0..3).map(|_| AtomicUsize::new(0)).collect();
    thread::scope(|s| {
        for c in &counts {
            let rx = rx.clone();
            s.spawn(move || {
                for _ in rx {
                    c.fetch_add(1, Relaxed);
                }
            });
        }
        drop(rx);
        for i in 0..3000 {
            tx.send(i).unwrap();
        }
        drop(tx);
    });
    let total: usize = counts.iter().map(|c| c.load(Relaxed)).sum();
    assert_eq!(
        total, 3000,
        "each message went to exactly ONE of the three receivers"
    );
    let split: Vec<_> = counts.iter().map(|c| c.load(Relaxed)).collect();

    // Correct broadcast: one channel per subscriber.
    let (txs, rxs): (Vec<_>, Vec<_>) = (0..3).map(|_| unbounded::<u32>()).unzip();
    for i in 0..3000 {
        for tx in &txs {
            tx.send(i).unwrap();
        }
    }
    drop(txs);
    for rx in rxs {
        assert_eq!(
            rx.iter().count(),
            3000,
            "every subscriber sees every message"
        );
    }
    println!(
        "6. cloned receivers split {split:?} (sum 3000); per-subscriber channels deliver 3000 each"
    );
}

// ---------------------------------------------------------------- 7. backpressure

fn backpressure() {
    let (tx, rx) = bounded::<u32>(4);
    let max_ahead = AtomicUsize::new(0);
    let consumed = AtomicUsize::new(0);
    let produced = AtomicUsize::new(0);
    thread::scope(|s| {
        s.spawn(|| {
            for i in 0..2000 {
                tx.send(i).unwrap();
                let p = produced.fetch_add(1, SeqCst) + 1;
                let c = consumed.load(SeqCst);
                max_ahead.fetch_max(p.saturating_sub(c), SeqCst);
            }
        });
        s.spawn(|| {
            // Slow-ish consumer: yields so the producer would run far ahead if the queue were unbounded.
            for expect in 0..2000 {
                assert_eq!(rx.recv().unwrap(), expect);
                consumed.fetch_add(1, SeqCst);
                if expect % 16 == 0 {
                    thread::yield_now();
                }
            }
        });
    });
    // Queue holds 4, plus the item the consumer has taken but not yet counted, plus the one the producer has sent
    // but not yet counted: a small constant, independent of the 2000 items.
    let ahead = max_ahead.load(SeqCst);
    assert!(
        ahead <= 4 + 2,
        "producer ran {ahead} ahead of a bounded(4) channel"
    );
    println!("7. backpressure: producer never more than {ahead} items ahead (capacity 4)");
}

// ---------------------------------------------------------------- 8. four-stage pipeline

#[derive(Clone, Copy)]
struct Msg {
    seq: u32,
    val: u64,
}

const ITEMS: u32 = 100_000;

/// source -> 4 x map -> 2 x filter -> sink. Returns (count, sum) seen by the sink.
fn pipeline() -> (usize, u64) {
    let (src_tx, src_rx) = bounded::<Msg>(64);
    let (map_tx, map_rx) = bounded::<(usize, Msg)>(64);
    let (flt_tx, flt_rx) = bounded::<(usize, usize, Msg)>(64);

    let mut handles = Vec::new();
    // Stage 1: single source; its output order is the global order.
    handles.push(thread::spawn(move || {
        for seq in 0..ITEMS {
            src_tx
                .send(Msg {
                    seq,
                    val: seq as u64,
                })
                .unwrap();
        }
        // src_tx dropped here -> stage 2 sees disconnect after draining.
    }));
    // Stage 2: 4 workers share src_rx (fan-out) and the one map_tx channel (fan-in).
    for w in 0..4 {
        let (rx, tx) = (src_rx.clone(), map_tx.clone());
        handles.push(thread::spawn(move || {
            for m in rx {
                tx.send((
                    w,
                    Msg {
                        val: m.val * 3 + 1,
                        ..m
                    },
                ))
                .unwrap();
            }
        }));
    }
    // Stage 3: 2 workers keep even values (3v+1 is even iff v is odd), tagging both worker ids.
    for w in 0..2 {
        let (rx, tx) = (map_rx.clone(), flt_tx.clone());
        handles.push(thread::spawn(move || {
            for (w2, m) in rx {
                if m.val % 2 == 0 {
                    tx.send((w2, w, m)).unwrap();
                }
            }
        }));
    }
    // Drop main's own endpoints, otherwise the stages would never see a disconnect.
    drop((src_rx, map_tx, map_rx, flt_tx));

    // Stage 4: the sink runs on this thread and verifies per-sender ordering.
    // Order survives only per (stage-2 worker, stage-3 worker) pair: one FIFO path through the pipeline.
    let mut last = [[None::<u32>; 2]; 4];
    let (mut count, mut sum) = (0usize, 0u64);
    for (w2, w3, m) in flt_rx {
        if let Some(prev) = last[w2][w3] {
            assert!(
                m.seq > prev,
                "path ({w2},{w3}) emitted seq {} after {prev}",
                m.seq
            );
        }
        last[w2][w3] = Some(m.seq);
        count += 1;
        sum += m.val;
    }
    for h in handles {
        h.join().unwrap();
    }
    (count, sum)
}

fn main() {
    capacities();
    disconnect_semantics();
    select_patterns();
    timers();
    request_reply();
    clones_are_not_broadcast();
    backpressure();

    let t = Instant::now();
    let (count, sum) = pipeline();
    let el = t.elapsed();
    let odd = (0..ITEMS as u64).filter(|v| v % 2 == 1);
    let expect_count = odd.clone().count();
    let expect_sum: u64 = odd.map(|v| v * 3 + 1).sum();
    assert_eq!(count, expect_count);
    assert_eq!(sum, expect_sum);
    println!(
        "8. pipeline: {ITEMS} items through 4 stages -> {count} kept, sum {sum} in {el:?} ({:.1} M items/s)",
        ITEMS as f64 / el.as_secs_f64() / 1e6
    );
}
