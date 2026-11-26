use solution::*;

use std::sync::atomic::{AtomicU32, Ordering};

#[test]
fn somebody_takes_a_place_while_the_policy_is_thinking() {
    let gate = Gate::new(2);
    let mut inner = None;
    let mut asked = Vec::new();
    let outer = gate.try_acquire_when(|in_flight, limit| {
        asked.push((in_flight, limit));
        if inner.is_none() {
            inner = gate.try_acquire();
        }
        true
    });
    check!("both got one", (inner.is_some(), outer.is_some(), gate.in_flight()), (true, true, 2));
    check!("asked again with the new count", asked.last().copied(), Some((1, 2)));
}

#[test]
fn the_last_place_is_taken_in_the_middle() {
    let gate = Gate::new(1);
    let mut inner = None;
    let outer = gate.try_acquire_when(|_, _| {
        if inner.is_none() {
            inner = gate.try_acquire();
        }
        true
    });
    check!("only one", (inner.is_some(), outer.is_some(), gate.in_flight()), (true, false, 1));
}

#[test]
fn a_policy_is_judged_on_the_values_that_hold() {
    let gate = Gate::new(4);
    let mut inner = Vec::new();
    // "only into an empty gate", and somebody gets in first
    let outer = gate.try_acquire_when(|in_flight, _| {
        if inner.is_empty() {
            inner.extend(gate.try_acquire());
        }
        in_flight == 0
    });
    check!("refused", (outer.is_some(), gate.in_flight()), (false, 1));
}

#[test]
fn a_raised_limit_is_noticed_too() {
    let gate = Gate::new(1);
    let mut seen = 0;
    let got = gate.try_acquire_when(|_, limit| {
        if seen == 0 {
            gate.set_limit(7);
        }
        seen = limit;
        true
    });
    check!("(permit, limit finally shown)", (got.is_some(), seen), (true, 7));
}

#[test]
fn no_room_means_no_question() {
    let gate = Gate::new(0);
    let got = gate.try_acquire_when(|_, _| panic!("asked with no room"));
    check!("none", got.is_some(), false);
}

#[test]
fn a_panic_gives_the_place_back() {
    let gate = Gate::new(1);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _permit = gate.try_acquire().unwrap();
        panic!("the work failed");
    }));
    check!("(panicked, in flight)", (result.is_err(), gate.in_flight()), (true, 0));
}

#[test]
fn a_panic_in_the_policy_takes_nothing() {
    let gate = Gate::new(1);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        gate.try_acquire_when(|_, _| panic!("bad policy"));
    }));
    check!("(panicked, in flight)", (result.is_err(), gate.in_flight()), (true, 0));
}

#[test]
fn a_permit_travels_and_outlives_the_gate() {
    fn takes_send_and_static<T: Send + 'static>(_: &T) {}
    let gate = Gate::new(1);
    let permit = gate.try_acquire().unwrap();
    takes_send_and_static(&permit);
    let other = gate.clone();
    drop(gate);
    std::thread::spawn(move || drop(permit)).join().unwrap();
    check!("released from another thread", other.in_flight(), 0);
    let last = other.try_acquire().unwrap();
    drop(other);
    drop(last);
}

#[test]
fn the_extreme_limit() {
    let gate = Gate::new(u32::MAX);
    let a = gate.try_acquire().unwrap();
    check!("(limit, in flight)", (gate.limit(), gate.in_flight()), (u32::MAX, 1));
    gate.set_limit(0);
    drop(a);
    check!("(limit, in flight)", (gate.limit(), gate.in_flight()), (0, 0));
}

#[test]
fn random_calls_match_a_counter() {
    let mut rng = anneal_prelude::Rng::new(9213);
    for _ in 0..300 {
        let start = rng.below(5) as u32;
        let gate = Gate::new(start);
        let (mut limit, mut peak) = (start, 0);
        let mut held: Vec<Permit> = Vec::new();
        let mut calls = format!("limit {start}:");
        for _ in 0..60 {
            match rng.below(4) {
                0 => {
                    limit = rng.below(6) as u32;
                    gate.set_limit(limit);
                    calls += &format!(" set_limit({limit})");
                }
                1 if !held.is_empty() => {
                    held.swap_remove(rng.below(held.len()));
                    calls += " drop";
                }
                _ => {
                    let want = (held.len() as u32) < limit;
                    let got = gate.try_acquire();
                    calls += " acquire";
                    check!(&calls, got.is_some(), want);
                    held.extend(got);
                }
            }
            peak = peak.max(held.len() as u32);
            check!(&calls, (gate.limit(), gate.in_flight(), gate.peak()), (limit, held.len() as u32, peak));
        }
    }
}

#[test]
fn threads_never_exceed_the_limit() {
    for limit in [1, 3, 7] {
        let gate = Gate::new(limit);
        let inside = AtomicU32::new(0);
        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| {
                    for _ in 0..3000 {
                        if let Some(_permit) = gate.try_acquire() {
                            let now = inside.fetch_add(1, Ordering::SeqCst) + 1;
                            assert!(now <= limit, "{now} inside a gate of {limit}");
                            std::thread::yield_now();
                            inside.fetch_sub(1, Ordering::SeqCst);
                        }
                    }
                });
            }
        });
        check!("nothing leaked", gate.in_flight(), 0);
        let peak = gate.peak();
        check!(format!("peak {peak} with limit {limit}"), peak >= 1 && peak <= limit, true);
    }
}
