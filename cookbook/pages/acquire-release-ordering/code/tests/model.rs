//! loom tests. They compile to nothing unless you pass `--cfg loom`:
//!
//!     RUSTFLAGS="--cfg loom" cargo test --release --test model
#![cfg(loom)]

use acquire_release_ordering::litmus::{Dekker, Mailbox, TwoFlags};
use acquire_release_ordering::{countdown, publish};
use loom::sync::Arc;
use loom::thread;
use std::sync::atomic::Ordering;

#[test]
fn publication_has_no_race() {
    loom::model(|| {
        let (publisher, reader) = publish::slot();
        let writer = thread::spawn(move || publisher.publish(42));
        // One attempt is enough: loom runs the schedules where it succeeds and
        // the ones where it comes too early. A spin loop is not needed.
        if let Some(value) = reader.try_read() {
            assert_eq!(value, 42);
        }
        writer.join().unwrap();
    });
}

#[test]
fn last_one_out_has_no_race() {
    loom::model(|| {
        let [a, b] = countdown::pair();
        let other = thread::spawn(move || a.finish(2));
        let mine = b.finish(3);
        let theirs = other.join().unwrap();
        assert_eq!(mine.or(theirs), Some(5));
        assert!(mine.is_none() || theirs.is_none());
    });
}

fn message_passing(flag_store: Ordering, flag_load: Ordering) {
    loom::model(move || {
        let mailbox = Arc::new(Mailbox::new());
        let sender = mailbox.clone();
        let writer = thread::spawn(move || sender.send(42, flag_store));
        if let Some(value) = mailbox.try_receive(flag_load) {
            assert_eq!(value, 42, "stale read");
        }
        writer.join().unwrap();
    });
}

#[test]
fn release_acquire_passes_the_message() {
    message_passing(Ordering::Release, Ordering::Acquire);
}

#[test]
#[should_panic(expected = "stale read")]
fn relaxed_flag_store_is_caught() {
    message_passing(Ordering::Relaxed, Ordering::Acquire);
}

#[test]
#[should_panic(expected = "stale read")]
fn relaxed_flag_load_is_caught() {
    message_passing(Ordering::Release, Ordering::Relaxed);
}

fn store_buffering(how: Dekker) {
    loom::model(move || {
        let flags = Arc::new(TwoFlags::new());
        let other = flags.clone();
        let left = thread::spawn(move || other.enter_left(how));
        let right = flags.enter_right(how);
        let left = left.join().unwrap();
        assert!(left == 1 || right == 1, "both read 0");
    });
}

#[test]
#[should_panic(expected = "both read 0")]
fn release_acquire_allows_both_zero() {
    store_buffering(Dekker::ReleaseAcquire);
}

#[test]
fn seqcst_fences_forbid_both_zero() {
    store_buffering(Dekker::SeqCstFence);
}

/// This is a FALSE ALARM. The language forbids this outcome with `SeqCst` accesses.
/// loom 0.7 treats a `SeqCst` access as `AcqRel` (see "Unsupported features" in its
/// README), so it reports it anyway. If this test starts to fail, loom has improved.
#[test]
#[should_panic(expected = "both read 0")]
fn loom_reports_a_false_alarm_for_seqcst_accesses() {
    store_buffering(Dekker::SeqCstAccesses);
}
