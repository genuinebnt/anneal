use solution::*;

#[test]
fn many_laps_single_thread() {
    check!(r#"capacity 4, 10000 push/pop pairs"#, { let q = Queue::new(4); let mut ok = true; for i in 0..10_000u32 { ok &= q.push(i).is_ok(); ok &= q.push(i + 1).is_ok(); ok &= q.pop() == Some(i); ok &= q.pop() == Some(i + 1); } (ok, q.pop()) }, (true, None));
}

#[test]
fn zero_and_one_capacity() {
    check!(r#"capacity 0 and 1 both give 2 usable slots"#, { let q = Queue::<u8>::new(0); let r = Queue::<u8>::new(1); (q.capacity(), r.capacity(), q.push(1).is_ok(), q.push(2).is_ok(), q.push(3).is_err()) }, (2, 2, true, true, true));
}

#[test]
fn full_then_empty_cycles() {
    check!(r#"fill to the brim each lap"#, { let q = Queue::new(4); let mut ok = true; for lap in 0..50u32 { for i in 0..4 { ok &= q.push(lap * 4 + i).is_ok(); } ok &= q.push(0).is_err(); for i in 0..4 { ok &= q.pop() == Some(lap * 4 + i); } ok &= q.pop().is_none(); } ok }, true);
}

#[test]
fn string_values() {
    check!(r#"heap values move through"#, { let q = Queue::new(2); q.push("a".to_string()).unwrap(); q.push("b".to_string()).unwrap(); (q.pop(), q.pop()) }, (Some("a".to_string()), Some("b".to_string())));
}

#[test]
fn leftovers_dropped_exactly_once() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    struct D(Arc<AtomicUsize>);
    impl Drop for D {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let n = Arc::new(AtomicUsize::new(0));
    let q = Queue::new(8);
    for _ in 0..6 {
        q.push(D(n.clone())).ok().unwrap();
    }
    drop(q.pop());
    check!("one popped and dropped", n.load(Ordering::SeqCst), 1);
    let rejected = {
        let q2 = Queue::new(2);
        q2.push(D(n.clone())).ok().unwrap();
        q2.push(D(n.clone())).ok().unwrap();
        q2.push(D(n.clone())).err().unwrap()
    }; // q2 dropped with its two values; `rejected` is still alive
    check!("q2 dropped: its two values are gone, the rejected one is not", n.load(Ordering::SeqCst), 3);
    drop(rejected);
    drop(q);
    check!("everything dropped", n.load(Ordering::SeqCst), 9);
}

#[test]
fn random_vs_model() {
    let mut rng = anneal_prelude::Rng::new(4505);
    for _ in 0..300 {
        let cap = rng.below(9) + 1;
        let q = Queue::new(cap);
        let real = cap.max(2).next_power_of_two();
        let mut model = std::collections::VecDeque::new();
        let mut log = Vec::new();
        for _ in 0..rng.below(60) {
            if rng.bool() {
                let v = rng.below(1000) as u32;
                let want = if model.len() == real { Err(v) } else { model.push_back(v); Ok(()) };
                log.push(format!("push {v}"));
                check!(format!("cap {cap}: {}", log.join(", ")), q.push(v), want);
            } else {
                log.push("pop".to_string());
                check!(format!("cap {cap}: {}", log.join(", ")), q.pop(), model.pop_front());
            }
        }
    }
}

// 4 producers and 4 consumers. Every value must come out exactly once, and the values of one producer must
// reach any single consumer in the order that producer pushed them.
#[test]
fn four_by_four_exactly_once_and_per_producer_order() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    let per = 50_000u64;
    let q = Arc::new(Queue::new(64));
    let finished = Arc::new(AtomicUsize::new(0));
    let producers: Vec<_> = (0..4u64)
        .map(|p| {
            let (q, finished) = (q.clone(), finished.clone());
            std::thread::spawn(move || {
                for i in 0..per {
                    let mut v = (p << 32) | i;
                    while let Err(back) = q.push(v) {
                        v = back;
                        std::hint::spin_loop();
                    }
                }
                finished.fetch_add(1, Ordering::SeqCst);
            })
        })
        .collect();
    let consumers: Vec<_> = (0..4)
        .map(|_| {
            let (q, finished) = (q.clone(), finished.clone());
            std::thread::spawn(move || {
                let mut got: Vec<u64> = Vec::new();
                loop {
                    match q.pop() {
                        Some(v) => got.push(v),
                        None if finished.load(Ordering::SeqCst) == 4 => match q.pop() {
                            Some(v) => got.push(v),
                            None => break,
                        },
                        None => std::hint::spin_loop(),
                    }
                }
                got
            })
        })
        .collect();
    for p in producers {
        p.join().unwrap();
    }
    let per_consumer: Vec<Vec<u64>> = consumers.into_iter().map(|c| c.join().unwrap()).collect();
    let mut ordered = true;
    for got in &per_consumer {
        let mut last = [None::<u64>; 4];
        for &v in got {
            let (p, i) = ((v >> 32) as usize, v & 0xffff_ffff);
            if let Some(prev) = last[p] {
                ordered &= i > prev;
            }
            last[p] = Some(i);
        }
    }
    let mut seen = vec![false; 4 * per as usize];
    let mut dup_or_bad = 0;
    let mut total = 0usize;
    for &v in per_consumer.iter().flatten() {
        let idx = ((v >> 32) * per + (v & 0xffff_ffff)) as usize;
        if idx >= seen.len() || seen[idx] {
            dup_or_bad += 1;
        } else {
            seen[idx] = true;
        }
        total += 1;
    }
    check!("4 producers x 50000, 4 consumers: total, duplicates/invented, per-producer order", (total, dup_or_bad, ordered), (200_000, 0, true));
}

#[test]
fn tiny_queue_heavy_contention() {
    use std::sync::Arc;
    let q = Arc::new(Queue::new(2));
    let hs: Vec<_> = (0..6u64)
        .map(|t| {
            let q = q.clone();
            std::thread::spawn(move || {
                let mut sum = 0u64;
                for i in 1..=10_000u64 {
                    let mut v = t * 100_000 + i;
                    while let Err(back) = q.push(v) {
                        v = back;
                        // make room by consuming: every thread is both producer and consumer
                        if let Some(x) = q.pop() {
                            sum += x;
                        }
                    }
                }
                sum
            })
        })
        .collect();
    let mut total: u64 = hs.into_iter().map(|h| h.join().unwrap()).sum();
    while let Some(x) = q.pop() {
        total += x;
    }
    let want: u64 = (0..6u64).map(|t| (1..=10_000u64).map(|i| t * 100_000 + i).sum::<u64>()).sum();
    check!("6 threads push into a queue of 2, draining it themselves", total, want);
}
