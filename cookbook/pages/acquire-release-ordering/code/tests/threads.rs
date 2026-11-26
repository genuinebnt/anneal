//! Real-thread tests. They assert only what the language guarantees, so they can
//! never fail on correct code. They are weak at finding a wrong ordering.
#![cfg(not(loom))]

use acquire_release_ordering::litmus::{Dekker, TwoFlags, count_stale_reads};
use acquire_release_ordering::{countdown, publish};
use std::sync::atomic::Ordering;
use std::thread;

const REPEATS: usize = if cfg!(miri) { 20 } else { 1_000 };

#[test]
fn nothing_is_readable_before_publish() {
    let (publisher, reader) = publish::slot();
    assert_eq!(reader.try_read(), None);
    publisher.publish(7);
    assert_eq!(reader.try_read(), Some(7));
}

#[test]
fn a_reader_on_another_thread_sees_the_published_value() {
    for i in 0..REPEATS as u64 {
        let (publisher, reader) = publish::slot();
        thread::scope(|s| {
            s.spawn(move || publisher.publish(i + 1));
            s.spawn(move || {
                loop {
                    if let Some(value) = reader.try_read() {
                        assert_eq!(value, i + 1);
                        break;
                    }
                    thread::yield_now();
                }
            });
        });
    }
}

#[test]
fn exactly_one_part_gets_the_sum() {
    for i in 0..REPEATS as u64 {
        let [a, b] = countdown::pair();
        let (x, y) = thread::scope(|s| {
            let a = s.spawn(move || a.finish(i));
            let b = s.spawn(move || b.finish(100));
            (a.join().unwrap(), b.join().unwrap())
        });
        assert_eq!([x, y].into_iter().flatten().collect::<Vec<_>>(), [i + 100]);
    }
}

#[test]
fn release_acquire_message_passing_has_no_stale_reads() {
    let rounds = if cfg!(miri) { 50 } else { 50_000 };
    assert_eq!(
        count_stale_reads(rounds, Ordering::Release, Ordering::Acquire),
        0
    );
}

#[test]
fn seqcst_never_lets_both_sides_read_zero() {
    for how in [Dekker::SeqCstAccesses, Dekker::SeqCstFence] {
        for _ in 0..REPEATS {
            let flags = TwoFlags::new();
            let (left, right) = thread::scope(|s| {
                let left = s.spawn(|| flags.enter_left(how));
                let right = s.spawn(|| flags.enter_right(how));
                (left.join().unwrap(), right.join().unwrap())
            });
            assert!(left == 1 || right == 1, "{how:?}: both read 0");
        }
    }
}
