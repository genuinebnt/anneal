//! RECIPE 111 (concurrency): a lock-free single-producer single-consumer (SPSC) bounded ring buffer.
//! LEVEL: advanced
//! Use in: audio/real-time pipelines, logging hand-off, stage-to-stage pipelines, anything with exactly one writer and one reader.   Run: cargo run --release --example spsc_ring_buffer
//! (the code has `unsafe`: with Miri installed, also run `cargo +nightly miri run --example spsc_ring_buffer`; it uses fewer items there)
#![allow(dead_code)]
//!
//! Design: a power-of-two slot array, so `index & mask` replaces `%`. `head` (next item to read) and `tail` (next item to write)
//! are counters that only count up and wrap at `usize::MAX`; `tail - head` (wrapping) = length, which tells full from empty
//! without wasting a slot. Each counter has ONE writer, so no CAS is needed, only load/store.
//!   push (producer): load own `tail` (Relaxed), load `head` (Acquire: pairs with consumer's Release, so the consumer's read of
//!     the slot happens before our overwrite), write the slot, then store `tail+1` (Release: publishes the slot contents).
//!   pop  (consumer): load own `head` (Relaxed), load `tail` (Acquire: pairs with producer's Release, makes the slot contents
//!     visible), read the slot, then store `head+1` (Release: our read happens before the producer reuses the slot).
//! `Producer` and `Consumer` are separate handle types that are NOT `Clone`, and `push` and `pop` take `&mut self`, so safe
//! code cannot get two writers. Head and tail are 128-byte aligned, which keeps them apart on common cache line sizes
//! (see `cache_line_distance` for a measurement).
//! Checks: a differential test against `VecDeque` (also across counter wrap-around), two threads, and drop accounting.
//! Passing tests do not prove the orderings: a weaker ordering can pass every normal run. Use Miri or loom as well.
//! COMMON USES: the `rtrb` and `ringbuf` crates, Linux kernel `kfifo`, audio callback queues, LMAX-Disruptor-style pipelines.
//! Verify such code with loom (https://github.com/tokio-rs/loom) and see memory_ordering_litmus for the ordering rules.
//! Read: https://marabos.nl/atomics/ , https://doc.rust-lang.org/nomicon/atomics.html , https://doc.rust-lang.org/std/cell/struct.UnsafeCell.html
use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::mem::MaybeUninit;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

/// 128, not 64: crossbeam uses 128 on x86-64 and aarch64. It is a cautious guess, not a hardware fact.
#[repr(align(128))]
struct CachePadded<T>(T);

struct Shared<T> {
    buf: Box<[UnsafeCell<MaybeUninit<T>>]>,
    mask: usize,
    head: CachePadded<AtomicUsize>, // written only by the consumer
    tail: CachePadded<AtomicUsize>, // written only by the producer
}

// The invariants every `unsafe` block relies on. `used` means `tail.wrapping_sub(head)`.
//  1. `used` is never more than the number of slots.
//  2. The slots of the counts from `head` up to, not including, `tail` hold a value. No other slot does.
//  3. Only the producer stores `tail`, and only the consumer stores `head`. `&mut self` on `push` and `pop` means
//     one call at a time on each side.
//  4. A slot changes sides only through a Release store that the other side reads with an Acquire load.

// SAFETY: `UnsafeCell` makes `Shared` not `Sync` on its own. By 1-4 the two sides never use one slot at the same
// time. Values only move from one thread to another, so `T: Send` is enough: no `&T` is shared between threads.
// (`Send` needs no `unsafe impl`: it already follows from `T: Send`.)
unsafe impl<T: Send> Sync for Shared<T> {}

impl<T> Drop for Shared<T> {
    fn drop(&mut self) {
        // &mut self: both handles are gone. `Arc` makes the last drop see everything the other handle did before
        // it was dropped. Drop the items still queued. (If one item's own drop panics, the rest are leaked.)
        let (head, tail) = (*self.head.0.get_mut(), *self.tail.0.get_mut());
        let mut i = head;
        while i != tail {
            // SAFETY: by invariant 2 this slot holds a value, and each slot in the range is visited once.
            unsafe { (*self.buf[i & self.mask].get()).assume_init_drop() };
            i = i.wrapping_add(1);
        }
    }
}

pub struct Producer<T>(Arc<Shared<T>>);
pub struct Consumer<T>(Arc<Shared<T>>);

/// A queue with room for at least `capacity` items: the number is rounded up to a power of two.
pub fn channel<T>(capacity: usize) -> (Producer<T>, Consumer<T>) {
    channel_from(capacity, 0)
}

/// Starts both counters at `start`. The tests use it to reach wrap-around without 2^64 pushes.
fn channel_from<T>(capacity: usize, start: usize) -> (Producer<T>, Consumer<T>) {
    // plain `next_power_of_two` returns 0 in release builds when the result does not fit
    let cap = capacity
        .max(1)
        .checked_next_power_of_two()
        .expect("capacity too large");
    let buf = (0..cap)
        .map(|_| UnsafeCell::new(MaybeUninit::uninit()))
        .collect();
    let s = Arc::new(Shared {
        buf,
        mask: cap - 1,
        head: CachePadded(AtomicUsize::new(start)),
        tail: CachePadded(AtomicUsize::new(start)),
    });
    (Producer(s.clone()), Consumer(s))
}

impl<T> Producer<T> {
    pub fn capacity(&self) -> usize {
        self.0.mask + 1
    }
    /// Returns the value back if the queue is full.
    pub fn push(&mut self, v: T) -> Result<(), T> {
        let s = &*self.0;
        let tail = s.tail.0.load(Ordering::Relaxed); // we are the only writer of tail
        let head = s.head.0.load(Ordering::Acquire); // pairs with consumer's Release store of head
        if tail.wrapping_sub(head) == s.buf.len() {
            return Err(v);
        }
        // SAFETY: the queue is not full, so by 1 and 2 the slot for `tail` holds no value and writing without a drop is
        // right. An old `head` only makes the queue look fuller. The consumer cannot read this slot before the store
        // below (2, 4), its last read of it happened before the `head` we loaded with Acquire (4), and no other push
        // is running (3).
        unsafe { (*s.buf[tail & s.mask].get()).write(v) };
        s.tail.0.store(tail.wrapping_add(1), Ordering::Release); // publish the slot
        Ok(())
    }
}

impl<T> Consumer<T> {
    pub fn pop(&mut self) -> Option<T> {
        let s = &*self.0;
        let head = s.head.0.load(Ordering::Relaxed); // we are the only writer of head
        let tail = s.tail.0.load(Ordering::Acquire); // pairs with producer's Release store of tail
        if head == tail {
            return None;
        }
        // SAFETY: the queue is not empty, so by 2 the slot for `head` holds a value, and the Acquire load of `tail`
        // makes the write of it visible (4). An old `tail` only makes the queue look emptier. The producer cannot
        // write this slot before the store below, and no other pop is running (3). We move the value out once.
        let v = unsafe { (*s.buf[head & s.mask].get()).assume_init_read() };
        s.head.0.store(head.wrapping_add(1), Ordering::Release); // hand the slot back
        Some(v)
    }
}

// ---------------------------------------------------------------- checks

/// Counts its own drops, so a double drop or a leak changes the count. The `Box` lets Miri see both.
struct Tracked<'a>(Box<u64>, &'a AtomicUsize);
impl Drop for Tracked<'_> {
    fn drop(&mut self) {
        self.1.fetch_add(1, Ordering::Relaxed);
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Random pushes and pops on one thread, compared with a `VecDeque` that has a size limit.
fn check_against_model() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let (cases, steps) = if cfg!(miri) { (2, 60) } else { (200, 2_000) };
    for asked in [0, 1, 2, 3, 4, 5, 8, 9] {
        for case in 0..cases {
            // half the runs start just below usize::MAX, so the counters wrap during the run
            let start = if case % 2 == 0 { 0 } else { usize::MAX - 5 };
            let drops = AtomicUsize::new(0);
            let (mut p, mut c) = channel_from::<Tracked>(asked, start);
            let cap = asked.max(1).next_power_of_two();
            assert_eq!(p.capacity(), cap);
            let mut model = VecDeque::new();
            let (mut made, mut gone) = (0u64, 0);
            let push_share = 2 + rng.next() % 7; // some runs fill the queue, others drain it
            for _ in 0..steps {
                if rng.next() % 10 < push_share {
                    let result = p.push(Tracked(Box::new(made), &drops));
                    if model.len() == cap {
                        let Err(back) = result else {
                            panic!("a full queue took an item");
                        };
                        assert_eq!(*back.0, made);
                        drop(back);
                        gone += 1; // the refused item came back and was dropped here
                    } else {
                        assert!(result.is_ok());
                        model.push_back(made);
                    }
                    made += 1;
                } else {
                    let got = c.pop().map(|t| *t.0);
                    assert_eq!(got, model.pop_front());
                    gone += usize::from(got.is_some());
                }
                assert_eq!(drops.load(Ordering::Relaxed), gone);
            }
            // drop one end first, in either order: the queued items live until the second one goes
            if case % 3 == 0 {
                drop(p);
                assert_eq!(drops.load(Ordering::Relaxed), gone);
                drop(c);
            } else {
                drop(c);
                assert_eq!(drops.load(Ordering::Relaxed), gone);
                drop(p);
            }
            assert_eq!(drops.load(Ordering::Relaxed), made as usize);
        }
    }
}

/// One producer thread, one consumer thread, items on the heap, strict order.
fn two_threads(capacity: usize, n: u64, start_at: usize) {
    let (mut p, mut c) = channel_from::<Box<u64>>(capacity, start_at);
    thread::scope(|s| {
        s.spawn(move || {
            for i in 0..n {
                let mut item = Box::new(i);
                while let Err(back) = p.push(item) {
                    item = back;
                    thread::yield_now();
                }
            }
        });
        // Do not panic inside this loop: the producer would then wait for room for ever, and the failure would
        // look like a hang. Remember the first wrong item and keep taking the rest.
        let mut wrong = None;
        for expect in 0..n {
            let item = loop {
                match c.pop() {
                    Some(item) => break item,
                    None => thread::yield_now(),
                }
            };
            if *item != expect {
                wrong = wrong.or(Some((expect, *item)));
            }
        }
        assert_eq!(
            wrong, None,
            "(expected, got) for the first item out of order"
        );
        assert!(c.pop().is_none());
    });
}

fn main() {
    // the two counters really are far apart
    let (p, _c) = channel::<u8>(1);
    let head = (&raw const p.0.head).addr();
    let tail = (&raw const p.0.tail).addr();
    assert!(head.abs_diff(tail) >= 128 && head % 128 == 0 && tail % 128 == 0);
    // a capacity that cannot be rounded up is refused, in release builds too
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let refused = std::panic::catch_unwind(|| channel::<u8>(usize::MAX));
    std::panic::set_hook(hook);
    assert!(refused.is_err());

    check_against_model();
    println!("random pushes and pops match the model, also across counter wrap-around");

    let n = if cfg!(miri) { 300 } else { 200_000 };
    for capacity in [1, 4, 1024] {
        let start = Instant::now();
        two_threads(capacity, n, usize::MAX - 100);
        println!(
            "capacity {capacity:>4}: {n} boxed items in order, {:.3} s (not checked)",
            start.elapsed().as_secs_f64()
        );
    }
}
