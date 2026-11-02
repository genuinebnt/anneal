use solution::*;

#[test]
fn zero_capacity() {
    check!(r#"capacity 0"#, { let s = Stack::new(0); (s.push(1), s.pop(), s.capacity()) }, (Err(1), None, 0));
}

#[test]
fn pack_extremes() {
    check!(r#"max tag, NIL index"#, (unpack(pack(u32::MAX, NIL)), unpack(pack(0, 0))), ((u32::MAX, NIL), (0, 0)));
}

#[test]
fn values_are_full_u64() {
    check!(r#"u64::MAX and 0 round-trip"#, { let s = Stack::new(2); s.push(u64::MAX).unwrap(); s.push(0).unwrap(); (s.pop(), s.pop()) }, (Some(0), Some(u64::MAX)));
}

#[test]
fn capacity_reported() {
    check!(r#"capacity 7"#, Stack::new(7).capacity(), 7);
}

#[test]
fn slots_recycle_many_times() {
    check!(r#"capacity 3, 10000 push/pop"#, { let s = Stack::new(3); let mut ok = true; for i in 0..10_000u64 { ok &= s.push(i).is_ok(); ok &= s.pop() == Some(i); } (ok, s.pop()) }, (true, None));
}

#[test]
fn interleaved_depths() {
    check!(r#"push 3, pop 2, push 2, drain"#, { let s = Stack::new(4); for v in 1..=3 { s.push(v).unwrap(); } let a = (s.pop(), s.pop()); s.push(8).unwrap(); s.push(9).unwrap(); let b: Vec<u64> = std::iter::from_fn(|| s.pop()).collect(); (a, b) }, ((Some(3), Some(2)), vec![9, 8, 1]));
}

#[test]
fn random_vs_model() {
    let mut rng = anneal_prelude::Rng::new(4404);
    for _ in 0..300 {
        let cap = rng.below(6);
        let s = Stack::new(cap);
        let mut model: Vec<u64> = Vec::new();
        let mut log = Vec::new();
        for _ in 0..rng.below(40) {
            if rng.bool() {
                let v = rng.below(100) as u64;
                let want = if model.len() == cap { Err(v) } else { model.push(v); Ok(()) };
                log.push(format!("push {v}"));
                check!(format!("cap {cap}: {}", log.join(", ")), s.push(v), want);
            } else {
                log.push("pop".to_string());
                check!(format!("cap {cap}: {}", log.join(", ")), s.pop(), model.pop());
            }
        }
    }
}

// Every thread pushes its own distinct values and pops as many; whoever pops what, no value may be lost,
// duplicated or invented. A tiny pool makes slots recycle constantly, which is what provokes ABA.
#[test]
fn small_pool_four_threads_conserve_every_value() {
    for round in 0..20u64 {
        let s = std::sync::Arc::new(Stack::new(5));
        let per = 5_000u64;
        let hs: Vec<_> = (0..4u64)
            .map(|t| {
                let s = s.clone();
                std::thread::spawn(move || {
                    let mut got = Vec::new();
                    for i in 0..per {
                        let mut v = t * per + i + 1;
                        while let Err(back) = s.push(v) {
                            v = back;
                            std::thread::yield_now();
                        }
                        loop {
                            if let Some(x) = s.pop() {
                                got.push(x);
                                break;
                            }
                            std::thread::yield_now();
                        }
                    }
                    got
                })
            })
            .collect();
        let mut all: Vec<u64> = hs.into_iter().flat_map(|h| h.join().unwrap()).collect();
        while let Some(x) = s.pop() {
            all.push(x);
        }
        all.sort_unstable();
        let want: Vec<u64> = (1..=4 * per).collect();
        check!(format!("round {round}: 4 threads x {per} values through a pool of 5"), all == want, true);
    }
}

#[test]
fn producers_and_consumers_split() {
    let s = std::sync::Arc::new(Stack::new(64));
    let done = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let producers: Vec<_> = (0..3u64)
        .map(|t| {
            let (s, done) = (s.clone(), done.clone());
            std::thread::spawn(move || {
                for i in 1..=30_000u64 {
                    let mut v = t * 1_000_000 + i;
                    while let Err(back) = s.push(v) {
                        v = back;
                        std::thread::yield_now();
                    }
                }
                done.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
        })
        .collect();
    let consumers: Vec<_> = (0..3)
        .map(|_| {
            let (s, done) = (s.clone(), done.clone());
            std::thread::spawn(move || {
                let mut sum = 0u64;
                let mut n = 0u64;
                loop {
                    match s.pop() {
                        Some(v) => {
                            sum += v;
                            n += 1;
                        }
                        None if done.load(std::sync::atomic::Ordering::SeqCst) == 3 => match s.pop() {
                            Some(v) => {
                                sum += v;
                                n += 1;
                            }
                            None => break,
                        },
                        None => std::thread::yield_now(),
                    }
                }
                (n, sum)
            })
        })
        .collect();
    for p in producers {
        p.join().unwrap();
    }
    let (n, sum) = consumers.into_iter().map(|c| c.join().unwrap()).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
    let want: u64 = (0..3u64).map(|t| (1..=30_000u64).map(|i| t * 1_000_000 + i).sum::<u64>()).sum();
    check!("3 producers x 30000 into 3 consumers: count and sum", (n, sum), (90_000, want));
}
