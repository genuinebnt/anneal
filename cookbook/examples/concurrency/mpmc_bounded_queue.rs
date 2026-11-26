//! RECIPE 144 (concurrency): bounded lock-free MPMC queue (Vyukov style): a ring of sequence-numbered cells.
//! LEVEL: advanced
//! Use in: work queues between thread pools, log/metric pipelines, any hot producer/consumer hand-off.   Run: cargo run --release --example mpmc_bounded_queue
#![allow(dead_code)]
//!
//! WHAT: a fixed-capacity queue many threads can push to and pop from at once, with no locks.
//! HOW: the buffer is a ring of cells; each cell has an `AtomicUsize` sequence number `seq` and a slot
//! `UnsafeCell<MaybeUninit<T>>`. Two shared counters `enq` and `deq` only ever grow (they wrap via `& mask`).
//!   cell.seq == pos          -> the slot is EMPTY and is the producer's turn for position `pos`
//!   cell.seq == pos + 1      -> the slot is FULL and is the consumer's turn for position `pos`
//!   after a pop, seq = pos + capacity -> empty again, for the next lap of the ring.
//! A producer claims position `pos` by `compare_exchange(enq: pos -> pos+1)`; only the winner touches the slot, so the
//! slot write itself needs no atomics. The sequence number is the hand-off signal.
//! ORDERINGS: `enq`/`deq` loads and CASes are `Relaxed` (they only pick a ticket; no data is published through them).
//! `cell.seq.load(Acquire)` pairs with the other side's `cell.seq.store(.., Release)`: a consumer that sees seq==pos+1
//! also sees the slot write that preceded that store; a producer that sees the lapped seq also sees that the consumer's
//! read of the slot finished (so overwriting is not a race).
//! SAFETY of Send/Sync: the queue owns `T` values and moves them between threads, so `T: Send` is required and enough;
//! each slot is accessed by exactly one thread at a time (the CAS winner), ordered by the seq Acquire/Release pair.
//! Drop drops the items still queued (positions deq..enq), and nothing else.
//! Not strictly lock-free: a producer that stalls between its CAS and its seq store delays the consumer of that slot.
//! Check with `cargo +nightly miri run --example mpmc_bounded_queue` (shrink the stress sizes first).
//! COMMON USES: bounded channels (back-pressure instead of unbounded memory growth), thread-pool task queues,
//! audio/network pipelines; crossbeam's `ArrayQueue` is this design, and `std::sync::mpsc` bounded channels solve the same problem.
//! Read: https://www.1024cores.net/home/lock-free-algorithms/queues/bounded-mpmc-queue and https://marabos.nl/atomics/ and https://docs.rs/crossbeam-queue/latest/crossbeam_queue/struct.ArrayQueue.html
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering::*};
use std::thread;

/// Keep the two hot counters on separate cache lines (see recipe 85, false sharing).
#[repr(align(64))]
struct Padded<T>(T);

struct Cell<T> {
    seq: AtomicUsize,
    val: UnsafeCell<MaybeUninit<T>>,
}

pub struct BoundedQueue<T> {
    buf: Box<[Cell<T>]>,
    mask: usize,
    enq: Padded<AtomicUsize>,
    deq: Padded<AtomicUsize>,
}

// SAFETY: values of type T are moved across threads (hence T: Send). Every slot is accessed only by the thread that
// won the CAS for its position, and hand-over between threads is ordered by the Release store / Acquire load of `seq`.
unsafe impl<T: Send> Send for BoundedQueue<T> {}
// SAFETY: same argument; all shared mutation goes through atomics or through a slot exclusively owned by a CAS winner.
unsafe impl<T: Send> Sync for BoundedQueue<T> {}

impl<T> BoundedQueue<T> {
    pub fn new(capacity: usize) -> Self {
        assert!(
            capacity >= 2 && capacity.is_power_of_two(),
            "capacity must be a power of two >= 2"
        );
        let buf = (0..capacity)
            .map(|i| Cell {
                seq: AtomicUsize::new(i),
                val: UnsafeCell::new(MaybeUninit::uninit()),
            })
            .collect();
        Self {
            buf,
            mask: capacity - 1,
            enq: Padded(AtomicUsize::new(0)),
            deq: Padded(AtomicUsize::new(0)),
        }
    }

    /// Err(item) hands the item back when the queue is full.
    pub fn push(&self, item: T) -> Result<(), T> {
        let mut pos = self.enq.0.load(Relaxed);
        loop {
            let cell = &self.buf[pos & self.mask];
            let seq = cell.seq.load(Acquire);
            let dif = seq.wrapping_sub(pos) as isize;
            if dif == 0 {
                // slot is empty and it is our lap: try to claim the ticket
                match self
                    .enq
                    .0
                    .compare_exchange_weak(pos, pos.wrapping_add(1), Relaxed, Relaxed)
                {
                    Ok(_) => {
                        // SAFETY: we won the CAS for `pos`, and seq == pos says the previous occupant was fully
                        // consumed, so no other thread reads or writes this slot until we publish below.
                        unsafe { (*cell.val.get()).write(item) };
                        cell.seq.store(pos.wrapping_add(1), Release); // publish the write to the consumer
                        return Ok(());
                    }
                    Err(cur) => pos = cur,
                }
            } else if dif < 0 {
                return Err(item); // seq is a lap behind: slot still holds an unconsumed item, queue is full
            } else {
                pos = self.enq.0.load(Relaxed); // another producer already took this position, reload
            }
        }
    }

    /// None when the queue is empty.
    pub fn pop(&self) -> Option<T> {
        let mut pos = self.deq.0.load(Relaxed);
        loop {
            let cell = &self.buf[pos & self.mask];
            let seq = cell.seq.load(Acquire);
            let dif = seq.wrapping_sub(pos.wrapping_add(1)) as isize;
            if dif == 0 {
                match self
                    .deq
                    .0
                    .compare_exchange_weak(pos, pos.wrapping_add(1), Relaxed, Relaxed)
                {
                    Ok(_) => {
                        // SAFETY: seq == pos+1 (seen with Acquire) means a producer finished writing this slot,
                        // and our CAS win makes us its only reader; we move the value out exactly once.
                        let v = unsafe { (*cell.val.get()).assume_init_read() };
                        cell.seq.store(pos.wrapping_add(self.mask + 1), Release); // hand the slot to the next lap
                        return Some(v);
                    }
                    Err(cur) => pos = cur,
                }
            } else if dif < 0 {
                return None; // slot not yet written for this lap: empty
            } else {
                pos = self.deq.0.load(Relaxed);
            }
        }
    }
}

impl<T> Drop for BoundedQueue<T> {
    fn drop(&mut self) {
        let (mut pos, end) = (*self.deq.0.get_mut(), *self.enq.0.get_mut());
        while pos != end {
            let cell = &mut self.buf[pos & self.mask];
            // SAFETY: &mut self means no concurrent users; every position in deq..enq was fully pushed and never
            // popped, so the slot is initialised and dropped exactly once here.
            unsafe { cell.val.get_mut().assume_init_drop() };
            pos = pos.wrapping_add(1);
        }
    }
}

struct Counted(Arc<AtomicUsize>);
impl Drop for Counted {
    fn drop(&mut self) {
        self.0.fetch_add(1, Relaxed);
    }
}

fn basics() {
    let q = BoundedQueue::new(4);
    assert_eq!(q.pop(), None);
    for i in 0..4 {
        assert!(q.push(i).is_ok());
    }
    assert_eq!(q.push(99), Err(99), "full queue hands the item back");
    assert_eq!(q.pop(), Some(0));
    assert!(q.push(4).is_ok());
    for want in 1..=4 {
        assert_eq!(q.pop(), Some(want));
    }
    assert_eq!(q.pop(), None);
    // many laps around a tiny ring
    for i in 0..1000u32 {
        q.push(i).unwrap();
        assert_eq!(q.pop(), Some(i));
    }
    println!("basics ok: full returns item, empty returns None, wrap-around works");
}

fn drop_counting() {
    let drops = Arc::new(AtomicUsize::new(0));
    let q = BoundedQueue::new(16);
    for _ in 0..10 {
        assert!(q.push(Counted(drops.clone())).is_ok());
    }
    drop(q.pop());
    drop(q.pop());
    drop(q.pop());
    assert_eq!(drops.load(Relaxed), 3);
    drop(q); // the 7 left over are dropped by Queue::drop
    assert_eq!(drops.load(Relaxed), 10);
    // a rejected push returns the item undropped
    let q = BoundedQueue::new(2);
    q.push(Counted(drops.clone())).ok().unwrap();
    q.push(Counted(drops.clone())).ok().unwrap();
    let back = q.push(Counted(drops.clone())).err().expect("full");
    assert_eq!(drops.load(Relaxed), 10);
    drop(back);
    drop(q);
    assert_eq!(drops.load(Relaxed), 13);
    println!("drop counting ok: 13 created, 13 dropped, none twice");
}

fn stress(round: usize) {
    const P: usize = 4;
    const C: usize = 4;
    const PER: u64 = 250_000; // 4 x 250k = 1M distinct values
    let q = BoundedQueue::new(1024);
    let done = AtomicUsize::new(0);
    let (q, done) = (&q, &done);
    let got: Vec<Vec<u64>> = thread::scope(|s| {
        for p in 0..P {
            s.spawn(move || {
                for i in 0..PER {
                    let mut v = ((p as u64) << 32) | i;
                    while let Err(back) = q.push(v) {
                        v = back;
                        thread::yield_now();
                    }
                }
                done.fetch_add(1, Release);
            });
        }
        let hs: Vec<_> = (0..C)
            .map(|_| {
                s.spawn(move || {
                    let mut mine = Vec::new();
                    let mut last = [None::<u64>; P];
                    loop {
                        let v = match q.pop() {
                            Some(v) => v,
                            None if done.load(Acquire) == P => match q.pop() {
                                Some(v) => v, // producers finished before this pop started
                                None => break,
                            },
                            None => {
                                thread::yield_now();
                                continue;
                            }
                        };
                        let (p, i) = ((v >> 32) as usize, v & 0xffff_ffff);
                        // per-producer FIFO: this consumer must see each producer's items in increasing order
                        if let Some(prev) = last[p] {
                            assert!(i > prev, "FIFO violated for producer {p}: {i} after {prev}");
                        }
                        last[p] = Some(i);
                        mine.push(v);
                    }
                    mine
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut seen = vec![false; P * PER as usize];
    let (mut total, mut sum, mut xor) = (0usize, 0u64, 0u64);
    for v in got.iter().flatten() {
        let idx = (v >> 32) as usize * PER as usize + (v & 0xffff_ffff) as usize;
        assert!(!seen[idx], "value {v:#x} received twice");
        seen[idx] = true;
        total += 1;
        sum = sum.wrapping_add(*v);
        xor ^= *v;
    }
    let (mut esum, mut exor) = (0u64, 0u64);
    for p in 0..P as u64 {
        for i in 0..PER {
            esum = esum.wrapping_add((p << 32) | i);
            exor ^= (p << 32) | i;
        }
    }
    assert_eq!(total, P * PER as usize, "lost values");
    assert!(seen.iter().all(|&b| b));
    assert_eq!((sum, xor), (esum, exor));
    let per_consumer: Vec<usize> = got.iter().map(Vec::len).collect();
    println!(
        "round {round}: 1M values exactly once, FIFO per producer ok; per-consumer counts {per_consumer:?}"
    );
}

fn main() {
    basics();
    drop_counting();
    for round in 0..3 {
        stress(round);
    }
    println!("all assertions passed");
}
