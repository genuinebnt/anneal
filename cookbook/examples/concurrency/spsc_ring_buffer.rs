//! RECIPE 111 (concurrency): a lock-free single-producer single-consumer (SPSC) bounded ring buffer.
//! LEVEL: advanced
//! Use in: audio/real-time pipelines, logging hand-off, stage-to-stage pipelines, anything with exactly one writer and one reader.   Run: cargo run --release --example spsc_ring_buffer
//! (unsafe part: also try `cargo +nightly miri run --example spsc_ring_buffer`, it is slow, so lower the item count first)
#![allow(dead_code)]
//!
//! Design: a power-of-two slot array, so `index & mask` replaces `%`. `head` (next slot to read) and `tail` (next slot to write)
//! are monotonically increasing counters (wrapping), never reduced; `tail - head` = length, which makes full vs empty unambiguous
//! without wasting a slot. Each counter has ONE writer, so no CAS is needed, only load/store.
//!   push (producer): load own `tail` (Relaxed), load `head` (Acquire: pairs with consumer's Release, proves the slot was vacated),
//!     write the slot, then store `tail+1` (Release: publishes the slot contents to the consumer).
//!   pop  (consumer): load own `head` (Relaxed), load `tail` (Acquire: pairs with producer's Release, makes the slot contents visible),
//!     read the slot, then store `head+1` (Release: tells the producer the slot may be reused).
//! `Producer` and `Consumer` are separate handle types that are NOT `Clone`, so "single producer / single consumer" is enforced by
//! ownership: you cannot accidentally have two writers. Head and tail live on separate 64-byte cache lines (`#[repr(align(64))]`) to
//! avoid false sharing, where two cores fight over one line although they touch different variables.
//! COMMON USES: the `rtrb` and `ringbuf` crates, Linux kernel `kfifo`, audio callback queues, LMAX-Disruptor-style pipelines.
//! Verify such code with loom (https://github.com/tokio-rs/loom) and see memory_ordering_litmus for the ordering rules.
//! Read: https://marabos.nl/atomics/ , https://doc.rust-lang.org/nomicon/atomics.html , https://doc.rust-lang.org/std/cell/struct.UnsafeCell.html
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

#[repr(align(64))]
struct CachePadded<T>(T);

struct Shared<T> {
    buf: Box<[UnsafeCell<MaybeUninit<T>>]>,
    mask: usize,
    head: CachePadded<AtomicUsize>, // written only by the consumer
    tail: CachePadded<AtomicUsize>, // written only by the producer
}

// SAFETY: slots are handed between exactly one producer and one consumer, and the Release/Acquire pairs on head/tail ensure a slot
// is never accessed by both at once; values move across threads, hence T: Send. No `&T` is ever shared, so T: Sync is not needed.
unsafe impl<T: Send> Send for Shared<T> {}
// SAFETY: see above; all cross-thread access to the UnsafeCells is mediated by the atomics.
unsafe impl<T: Send> Sync for Shared<T> {}

impl<T> Drop for Shared<T> {
    fn drop(&mut self) {
        // &mut self: both handles are gone, so no concurrency. Drop the still-queued elements [head, tail).
        let (head, tail) = (*self.head.0.get_mut(), *self.tail.0.get_mut());
        let mut i = head;
        while i != tail {
            // SAFETY: slots in [head, tail) were written by push and not yet moved out by pop, so they are initialized.
            unsafe { (*self.buf[i & self.mask].get()).assume_init_drop() };
            i = i.wrapping_add(1);
        }
    }
}

pub struct Producer<T>(Arc<Shared<T>>);
pub struct Consumer<T>(Arc<Shared<T>>);

pub fn channel<T>(capacity: usize) -> (Producer<T>, Consumer<T>) {
    let cap = capacity.max(1).next_power_of_two();
    let buf = (0..cap)
        .map(|_| UnsafeCell::new(MaybeUninit::uninit()))
        .collect();
    let s = Arc::new(Shared {
        buf,
        mask: cap - 1,
        head: CachePadded(AtomicUsize::new(0)),
        tail: CachePadded(AtomicUsize::new(0)),
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
        if tail.wrapping_sub(head) == s.mask + 1 {
            return Err(v);
        }
        // SAFETY: the slot at `tail` is outside [head, tail), so the consumer will not touch it, and `&mut self` makes us the
        // only producer. It is uninitialized/moved-out, so overwriting without dropping is correct.
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
        // SAFETY: head != tail so the slot at `head` was fully written (Acquire above) and not yet read; `&mut self` makes
        // us the only consumer. We move it out; the slot is then logically uninitialized and head advances past it.
        let v = unsafe { (*s.buf[head & s.mask].get()).assume_init_read() };
        s.head.0.store(head.wrapping_add(1), Ordering::Release); // hand the slot back
        Some(v)
    }
}

static DROPS: AtomicUsize = AtomicUsize::new(0);
struct Tracked(u64);
impl Drop for Tracked {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::Relaxed);
    }
}

fn main() {
    // edge cases
    let (mut p, mut c) = channel::<u32>(3); // rounds up to 4
    assert_eq!(p.capacity(), 4);
    assert_eq!(c.pop(), None);
    for i in 0..4 {
        assert!(p.push(i).is_ok());
    }
    assert_eq!(p.push(99), Err(99)); // full
    assert_eq!(c.pop(), Some(0));
    assert!(p.push(4).is_ok()); // wraps around
    assert_eq!(
        (1..=4).map(|_| c.pop().unwrap()).collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    assert_eq!(c.pop(), None);

    // two threads, 2 million items, strict order
    const N: u64 = 2_000_000;
    let (mut p, mut c) = channel::<u64>(1024);
    let start = Instant::now();
    let prod = thread::spawn(move || {
        for i in 0..N {
            let mut v = i;
            while let Err(back) = p.push(v) {
                v = back;
                std::hint::spin_loop();
            }
        }
    });
    let cons = thread::spawn(move || {
        let (mut sum, mut expect) = (0u64, 0u64);
        while expect < N {
            match c.pop() {
                Some(v) => {
                    assert_eq!(v, expect, "out-of-order item");
                    sum += v;
                    expect += 1;
                }
                None => std::hint::spin_loop(),
            }
        }
        sum
    });
    prod.join().unwrap();
    let sum = cons.join().unwrap();
    let dt = start.elapsed().as_secs_f64();
    assert_eq!(sum, N * (N - 1) / 2);
    println!(
        "transferred {N} u64 in {dt:.3}s = {:.1} M items/s (not asserted)",
        N as f64 / dt / 1e6
    );

    // drop accounting: 10 pushed, 3 popped (dropped by us), 7 remain when the queue is dropped
    let (mut p, mut c) = channel::<Tracked>(16);
    for i in 0..10 {
        assert!(p.push(Tracked(i)).is_ok());
    }
    for i in 0..3 {
        assert_eq!(c.pop().unwrap().0, i);
    }
    assert_eq!(DROPS.load(Ordering::Relaxed), 3);
    drop(p);
    drop(c);
    assert_eq!(
        DROPS.load(Ordering::Relaxed),
        10,
        "every pushed item dropped exactly once"
    );
    println!("drop accounting OK");
}
