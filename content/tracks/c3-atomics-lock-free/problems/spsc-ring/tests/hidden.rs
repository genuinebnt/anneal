use solution::*;

#[test]
fn zero_capacity_panics() {
    check!(r#"capacity 0"#, std::panic::catch_unwind(|| channel::<u8>(0)).is_err(), true);
}

#[test]
fn wraps_around_many_times() {
    check!(r#"capacity 3, 1000 push/pop pairs"#, { let (mut p, mut c) = channel(3); let mut ok = true; for i in 0..1000u32 { p.push(i).unwrap(); p.push(i + 1).unwrap(); ok &= c.pop() == Some(i); ok &= c.pop() == Some(i + 1); } ok }, true);
}

#[test]
fn abandon_flags() {
    check!(r#"dropping one end shows on the other"#, { let (p, c) = channel::<u8>(2); let before = (p.is_abandoned(), c.is_abandoned()); drop(p); let after = c.is_abandoned(); (before, after) }, ((false, false), true));
}

#[test]
fn abandoned_consumer_seen_by_producer() {
    check!(r#"drop consumer"#, { let (p, c) = channel::<u8>(2); drop(c); p.is_abandoned() }, true);
}

#[test]
fn items_left_after_producer_dropped() {
    check!(r#"pop still works"#, { let (mut p, mut c) = channel(4); p.push(1).unwrap(); p.push(2).unwrap(); drop(p); (c.pop(), c.pop(), c.pop(), c.is_abandoned()) }, (Some(1), Some(2), None, true));
}

#[test]
fn string_values_are_moved_not_copied() {
    check!(r#"heap values round-trip"#, { let (mut p, mut c) = channel(2); p.push(vec![1u8; 100]).unwrap(); c.pop().map(|v| v.len()) }, Some(100));
}

#[test]
fn leftover_items_dropped_once() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    struct D(Arc<AtomicUsize>);
    impl Drop for D {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let n = Arc::new(AtomicUsize::new(0));
    let (mut p, mut c) = channel(5);
    for _ in 0..5 {
        p.push(D(n.clone())).ok().unwrap();
    }
    drop(c.pop()); // one consumed (dropped by us)
    drop(c.pop());
    check!("two popped and dropped", n.load(Ordering::SeqCst), 2);
    p.push(D(n.clone())).ok().unwrap(); // wraps into slot 0
    drop(p);
    drop(c);
    check!("six pushed in total, all dropped exactly once", n.load(Ordering::SeqCst), 6);
}

#[test]
fn rejected_value_is_not_dropped_twice() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    struct D(Arc<AtomicUsize>);
    impl Drop for D {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let n = Arc::new(AtomicUsize::new(0));
    let (mut p, c) = channel(1);
    p.push(D(n.clone())).ok().unwrap();
    let back = p.push(D(n.clone())).err().unwrap();
    check!("full: nothing dropped yet", n.load(Ordering::SeqCst), 0);
    drop(back);
    drop(p);
    drop(c);
    check!("rejected value and stored value", n.load(Ordering::SeqCst), 2);
}

#[test]
fn ends_are_send() {
    fn needs_send<T: Send>() {}
    needs_send::<Producer<String>>();
    needs_send::<Consumer<String>>();
}

#[test]
fn random_vs_model() {
    let mut rng = anneal_prelude::Rng::new(4303);
    for _ in 0..300 {
        let cap = rng.below(6) + 1;
        let (mut p, mut c) = channel(cap);
        let mut model = std::collections::VecDeque::new();
        let mut log = Vec::new();
        for _ in 0..rng.below(40) {
            if rng.bool() {
                let v = rng.below(1000) as u32;
                let want = if model.len() == cap { Err(v) } else { model.push_back(v); Ok(()) };
                log.push(format!("push {v}"));
                check!(format!("cap {cap}: {}", log.join(", ")), p.push(v), want);
            } else {
                log.push("pop".to_string());
                check!(format!("cap {cap}: {}", log.join(", ")), c.pop(), model.pop_front());
            }
        }
    }
}

fn pump(cap: usize, n: u64) -> (u64, bool) {
    let (mut p, mut c) = channel::<u64>(cap);
    let prod = std::thread::spawn(move || {
        for i in 0..n {
            let mut v = i;
            loop {
                match p.push(v) {
                    Ok(()) => break,
                    Err(back) => {
                        v = back;
                        std::hint::spin_loop();
                    }
                }
            }
        }
    });
    let mut expect = 0u64;
    let mut in_order = true;
    let mut sum = 0u64;
    while expect < n {
        match c.pop() {
            Some(v) => {
                in_order &= v == expect;
                sum += v;
                expect += 1;
            }
            None => std::hint::spin_loop(),
        }
    }
    prod.join().unwrap();
    (sum, in_order)
}

#[test]
fn million_items_in_order_through_a_small_ring() {
    let (sum, in_order) = pump(64, 1_000_000);
    check!("1,000,000 items through capacity 64: sum and order", (sum, in_order), (499_999_500_000, true));
}

#[test]
fn capacity_one_ping_pong_between_threads() {
    let (sum, in_order) = pump(1, 50_000);
    check!("50,000 items through capacity 1: sum and order", (sum, in_order), (1_249_975_000, true));
}

#[test]
fn consumer_drains_until_the_producer_is_gone() {
    let (mut p, mut c) = channel::<String>(8);
    let prod = std::thread::spawn(move || {
        for i in 0..20_000 {
            let mut s = format!("msg{i}");
            while let Err(back) = p.push(s) {
                s = back;
                std::thread::yield_now();
            }
        }
    });
    let mut got = 0usize;
    let mut last_ok = true;
    loop {
        match c.pop() {
            Some(s) => {
                last_ok &= s == format!("msg{got}");
                got += 1;
            }
            None if c.is_abandoned() => match c.pop() {
                Some(s) => {
                    last_ok &= s == format!("msg{got}");
                    got += 1;
                }
                None => break,
            },
            None => std::thread::yield_now(),
        }
    }
    prod.join().unwrap();
    check!("consumer loops until abandoned and empty", (got, last_ok), (20_000, true));
}
