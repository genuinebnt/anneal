//! RECIPE 231 (data-structures): a ring buffer zoo: fixed ring, growable deque, flight recorder, byte ring, sliding-window stats
//! LEVEL: intermediate
//! Use in: logging/telemetry, I/O buffers (pipes, UART, sockets), queues, moving statistics, as the base of an SPSC channel.   Run: cargo run --release --example ring_buffer_zoo
//! COMMON USES: the Linux kernel's pipe and kfifo buffers, UART/serial drivers, network receive buffers (read into the tail, parse from the head),
//! audio sample queues, "last N events" crash/flight recorders, moving averages in monitoring, std's `VecDeque` itself (a growable ring).
#![allow(dead_code)]
//!
//! What: five ring-buffer flavours built from scratch, all safe code (the only `unsafe` in the file is the 3-line counting allocator wrapper used to
//! PROVE the steady state does not allocate, same pattern as counting_allocator).
//!  1. `Ring<T, N>`: fixed capacity, N a power of two so `counter & (N-1)` replaces `%`. `head` and `tail` are free-running WRAPPING counters
//!     (never reduced), so `len = tail - head` (wrapping) and full (`len == N`) is distinguishable from empty (`len == 0`) with NO wasted slot. The
//!     naive "head == tail" design needs a spare slot or a flag. Wrapping arithmetic stays correct when the counters overflow usize (tested by
//!     starting them near usize::MAX).
//!  2. `MyDeque<T>`: a growable ring like `VecDeque`. When full it doubles and UNWRAPS: element i goes to slot i of the new buffer so head = 0.
//!     `as_slices` returns the two segments, `make_contiguous` rotates the buffer so the live data is one slice. (Storage is `Vec<T>` with
//!     `T: Default` so no `MaybeUninit`/unsafe is needed; std uses uninitialised slots.)
//!  3. `FlightRecorder`: overwrite-on-full ring of log lines (keeps the LAST N); lines are formatted into reused Strings, so logging is
//!     allocation free once each slot has grown to the longest line.
//!  4. `ByteRing`: bytes with `write`/`read` that copy in at most TWO segments (to the end of the buffer, then from the start), plus zero-copy views
//!     (`readable`, `writable`, `commit`, `consume`) returning contiguous slices: the shape of `read(fd, ring.writable())` then parse in place.
//!  5. `WindowStats`: moving average/min/max over the last W samples; min/max use a MONOTONIC deque (each sample is pushed and popped at most once,
//!     so amortised O(1)) built on `MyDeque`.
//! Verified by differential tests against `VecDeque` (50k seeded random operations), brute-force window statistics, and an allocation count of 0
//! over 100k operations after warm-up.
//! PROMOTING TO SPSC (see spsc_ring_buffer): make `head` and `tail` atomics, the producer owns `tail` and the consumer owns `head`. The producer
//! writes the slot FIRST, then publishes with `tail.store(.., Release)`; the consumer reads `tail` with Acquire before touching the slot, then frees it
//! with `head.store(.., Release)`, which the producer observes with an Acquire load of `head` before reusing the slot. Each side may cache the other's
//! counter and reload only when the ring looks full/empty. Put head and tail on separate cache lines (false sharing). Slots need interior mutability
//! (UnsafeCell), which is where the unsafe in spsc_ring_buffer comes from; the wrapping-counter indexing carries over unchanged. Relaxed is NOT enough: it
//! would let the consumer see the new tail before the slot's contents.
//! Read: https://en.wikipedia.org/wiki/Circular_buffer , https://doc.rust-lang.org/std/collections/struct.VecDeque.html , https://marabos.nl/atomics/
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::VecDeque;
use std::fmt::Write;
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

struct Counting;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;
fn allocs<R>(f: impl FnOnce() -> R) -> usize {
    let a = ALLOCS.load(Relaxed);
    black_box(f());
    ALLOCS.load(Relaxed) - a
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

// ------------------------------------------------------------------ 1. fixed ring with wrapping counters
struct Ring<T, const N: usize> {
    slots: [Option<T>; N],
    head: usize,
    tail: usize,
}
impl<T, const N: usize> Ring<T, N> {
    const MASK: usize = {
        assert!(N.is_power_of_two(), "N must be a power of two");
        N - 1
    };
    fn new() -> Self {
        Self::starting_at(0)
    }
    /// Start the free-running counters at `c` (tests use values near usize::MAX to exercise overflow).
    fn starting_at(c: usize) -> Self {
        let _ = Self::MASK;
        Ring {
            slots: std::array::from_fn(|_| None),
            head: c,
            tail: c,
        }
    }
    fn len(&self) -> usize {
        self.tail.wrapping_sub(self.head)
    }
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn is_full(&self) -> bool {
        self.len() == N
    }
    fn push_back(&mut self, x: T) -> Result<(), T> {
        if self.is_full() {
            return Err(x);
        }
        self.slots[self.tail & Self::MASK] = Some(x);
        self.tail = self.tail.wrapping_add(1);
        Ok(())
    }
    fn pop_front(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }
        let v = self.slots[self.head & Self::MASK].take();
        self.head = self.head.wrapping_add(1);
        v
    }
    fn peek_front(&self) -> Option<&T> {
        if self.is_empty() {
            None
        } else {
            self.slots[self.head & Self::MASK].as_ref()
        }
    }
    fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        (0..self.len()).map(move |i| {
            self.slots[self.head.wrapping_add(i) & Self::MASK]
                .as_ref()
                .unwrap()
        })
    }
}

// ------------------------------------------------------------------ 2. growable deque
struct MyDeque<T: Default> {
    buf: Vec<T>,
    head: usize,
    len: usize,
    grows: usize,
}
impl<T: Default> MyDeque<T> {
    fn new() -> Self {
        MyDeque {
            buf: (0..4).map(|_| T::default()).collect(),
            head: 0,
            len: 0,
            grows: 0,
        }
    }
    fn mask(&self) -> usize {
        self.buf.len() - 1
    }
    fn phys(&self, i: usize) -> usize {
        (self.head + i) & self.mask()
    }
    fn len(&self) -> usize {
        self.len
    }
    fn grow(&mut self) {
        let mut nb: Vec<T> = (0..self.buf.len() * 2).map(|_| T::default()).collect();
        for i in 0..self.len {
            let p = self.phys(i);
            nb[i] = std::mem::take(&mut self.buf[p]);
        } // unwrap: logical i -> slot i
        self.buf = nb;
        self.head = 0;
        self.grows += 1;
    }
    fn push_back(&mut self, x: T) {
        if self.len == self.buf.len() {
            self.grow();
        }
        let p = self.phys(self.len);
        self.buf[p] = x;
        self.len += 1;
    }
    fn push_front(&mut self, x: T) {
        if self.len == self.buf.len() {
            self.grow();
        }
        self.head = self.head.wrapping_sub(1) & self.mask();
        self.buf[self.head] = x;
        self.len += 1;
    }
    fn pop_front(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        let v = std::mem::take(&mut self.buf[self.head]);
        self.head = (self.head + 1) & self.mask();
        self.len -= 1;
        Some(v)
    }
    fn pop_back(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        let p = self.phys(self.len - 1);
        self.len -= 1;
        Some(std::mem::take(&mut self.buf[p]))
    }
    fn get(&self, i: usize) -> Option<&T> {
        if i < self.len {
            Some(&self.buf[self.phys(i)])
        } else {
            None
        }
    }
    fn front(&self) -> Option<&T> {
        self.get(0)
    }
    fn back(&self) -> Option<&T> {
        self.len.checked_sub(1).and_then(|i| self.get(i))
    }
    fn insert(&mut self, i: usize, x: T) {
        assert!(i <= self.len);
        self.push_back(x);
        for j in (i..self.len - 1).rev() {
            let (a, b) = (self.phys(j), self.phys(j + 1));
            self.buf.swap(a, b);
        } // bubble the new item down to i
    }
    fn remove(&mut self, i: usize) -> Option<T> {
        if i >= self.len {
            return None;
        }
        for j in i..self.len - 1 {
            let (a, b) = (self.phys(j), self.phys(j + 1));
            self.buf.swap(a, b);
        }
        self.pop_back()
    }
    fn as_slices(&self) -> (&[T], &[T]) {
        if self.head + self.len <= self.buf.len() {
            (&self.buf[self.head..self.head + self.len], &[])
        } else {
            (
                &self.buf[self.head..],
                &self.buf[..self.head + self.len - self.buf.len()],
            )
        }
    }
    fn make_contiguous(&mut self) -> &mut [T] {
        self.buf.rotate_left(self.head);
        self.head = 0;
        &mut self.buf[..self.len]
    }
    fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        let (a, b) = self.as_slices();
        a.iter().chain(b.iter())
    }
    fn clear(&mut self) {
        while self.pop_front().is_some() {}
    }
}

// ------------------------------------------------------------------ 3. flight recorder (overwrite oldest)
struct FlightRecorder {
    lines: Vec<String>,
    next: u64,
} // `next` = total lines ever logged; slot = next % N
impl FlightRecorder {
    fn new(n: usize) -> Self {
        assert!(n.is_power_of_two());
        FlightRecorder {
            lines: vec![String::new(); n],
            next: 0,
        }
    }
    fn log(&mut self, args: std::fmt::Arguments) {
        let slot = (self.next as usize) & (self.lines.len() - 1);
        let s = &mut self.lines[slot];
        s.clear();
        let _ = s.write_fmt(args); // reuse the String's capacity
        self.next += 1;
    }
    fn snapshot(&self) -> Vec<&str> {
        // oldest -> newest
        let n = self.lines.len() as u64;
        (self.next.saturating_sub(n)..self.next)
            .map(|i| self.lines[(i & (n - 1)) as usize].as_str())
            .collect()
    }
}

// ------------------------------------------------------------------ 4. byte ring
struct ByteRing {
    buf: Vec<u8>,
    head: usize,
    tail: usize,
}
impl ByteRing {
    fn new(cap: usize) -> Self {
        Self::starting_at(cap, 0)
    }
    fn starting_at(cap: usize, c: usize) -> Self {
        assert!(cap.is_power_of_two());
        ByteRing {
            buf: vec![0; cap],
            head: c,
            tail: c,
        }
    }
    fn cap(&self) -> usize {
        self.buf.len()
    }
    fn len(&self) -> usize {
        self.tail.wrapping_sub(self.head)
    }
    fn free(&self) -> usize {
        self.cap() - self.len()
    }
    /// Copy as much of `data` as fits; at most two memcpy segments (up to the physical end, then from the start).
    fn write(&mut self, data: &[u8]) -> usize {
        let n = data.len().min(self.free());
        let start = self.tail & (self.cap() - 1);
        let first = n.min(self.cap() - start);
        self.buf[start..start + first].copy_from_slice(&data[..first]);
        self.buf[..n - first].copy_from_slice(&data[first..n]);
        self.tail = self.tail.wrapping_add(n);
        n
    }
    fn read(&mut self, out: &mut [u8]) -> usize {
        let n = out.len().min(self.len());
        let start = self.head & (self.cap() - 1);
        let first = n.min(self.cap() - start);
        out[..first].copy_from_slice(&self.buf[start..start + first]);
        out[first..n].copy_from_slice(&self.buf[..n - first]);
        self.head = self.head.wrapping_add(n);
        n
    }
    /// Zero-copy view of the unread bytes: (up to the physical end, wrapped remainder).
    fn readable(&self) -> (&[u8], &[u8]) {
        let start = self.head & (self.cap() - 1);
        let n = self.len();
        let first = n.min(self.cap() - start);
        (&self.buf[start..start + first], &self.buf[..n - first])
    }
    fn consume(&mut self, n: usize) {
        assert!(n <= self.len());
        self.head = self.head.wrapping_add(n);
    }
    /// Largest CONTIGUOUS free region (what you would hand to `read(fd, ..)`); fill a prefix then `commit`.
    fn writable(&mut self) -> &mut [u8] {
        let start = self.tail & (self.cap() - 1);
        let n = self.free().min(self.cap() - start);
        &mut self.buf[start..start + n]
    }
    fn commit(&mut self, n: usize) {
        assert!(n <= self.free());
        self.tail = self.tail.wrapping_add(n);
    }
}

// ------------------------------------------------------------------ 5. sliding-window statistics
struct WindowStats {
    w: usize,
    vals: Vec<i64>,
    count: u64,
    sum: i64,
    minq: MyDeque<(u64, i64)>,
    maxq: MyDeque<(u64, i64)>,
}
impl WindowStats {
    fn new(w: usize) -> Self {
        let mut s = WindowStats {
            w,
            vals: vec![0; w],
            count: 0,
            sum: 0,
            minq: MyDeque::new(),
            maxq: MyDeque::new(),
        };
        for _ in 0..w {
            s.minq.push_back((0, 0));
            s.maxq.push_back((0, 0));
        } // pre-grow so steady state never reallocates
        s.minq.clear();
        s.maxq.clear();
        s
    }
    fn push(&mut self, v: i64) {
        let slot = (self.count % self.w as u64) as usize;
        if self.count >= self.w as u64 {
            self.sum -= self.vals[slot];
        } // the sample falling out of the window
        self.vals[slot] = v;
        self.sum += v;
        while self.minq.back().is_some_and(|&(_, x)| x >= v) {
            self.minq.pop_back();
        }
        while self.maxq.back().is_some_and(|&(_, x)| x <= v) {
            self.maxq.pop_back();
        }
        self.minq.push_back((self.count, v));
        self.maxq.push_back((self.count, v));
        self.count += 1;
        let oldest = self.count.saturating_sub(self.w as u64); // indices below this have expired
        while self.minq.front().is_some_and(|&(i, _)| i < oldest) {
            self.minq.pop_front();
        }
        while self.maxq.front().is_some_and(|&(i, _)| i < oldest) {
            self.maxq.pop_front();
        }
    }
    fn n(&self) -> usize {
        (self.count as usize).min(self.w)
    }
    fn mean(&self) -> f64 {
        self.sum as f64 / self.n() as f64
    }
    fn min(&self) -> i64 {
        self.minq.front().unwrap().1
    }
    fn max(&self) -> i64 {
        self.maxq.front().unwrap().1
    }
}

fn main() {
    // ---- demo
    let mut r: Ring<&str, 4> = Ring::new();
    for w in ["a", "b", "c", "d"] {
        r.push_back(w).unwrap();
    }
    assert_eq!(r.push_back("e"), Err("e")); // full with all 4 slots usable
    r.pop_front();
    r.push_back("e").unwrap(); // wraps physically
    println!(
        "Ring<_, 4> after wrap: {:?} len={} full={}",
        r.iter().collect::<Vec<_>>(),
        r.len(),
        r.is_full()
    );
    let mut fr = FlightRecorder::new(4);
    for i in 0..10 {
        fr.log(format_args!("event {i} ok"));
    }
    println!("flight recorder keeps the last 4: {:?}", fr.snapshot());
    assert_eq!(
        fr.snapshot(),
        ["event 6 ok", "event 7 ok", "event 8 ok", "event 9 ok"]
    );
    let mut br = ByteRing::new(8);
    br.write(b"abcdef");
    let mut tmp = [0u8; 4];
    br.read(&mut tmp);
    br.write(b"ghijk");
    let (a, b) = br.readable();
    println!(
        "byte ring wrapped view: {:?} + {:?}",
        String::from_utf8_lossy(a),
        String::from_utf8_lossy(b)
    );
    assert_eq!((a, b), (&b"efgh"[..], &b"ijk"[..]));
    let mut ws = WindowStats::new(3);
    for v in [5, 1, 4, 9, 2] {
        ws.push(v);
    }
    println!(
        "window(3) over 5,1,4,9,2: mean={:.2} min={} max={}",
        ws.mean(),
        ws.min(),
        ws.max()
    );
    assert_eq!((ws.min(), ws.max()), (2, 9));

    // ---- Ring<T,N> vs VecDeque, counters starting near overflow
    let mut g = Rng(0x1234_5678_9ABC_DEF1);
    let mut ring: Ring<u32, 16> = Ring::starting_at(usize::MAX - 20);
    let mut model: VecDeque<u32> = VecDeque::new();
    for step in 0..50_000u32 {
        if g.below(2) == 0 {
            let r1 = ring.push_back(step);
            if model.len() < 16 {
                assert!(r1.is_ok());
                model.push_back(step);
            } else {
                assert_eq!(r1, Err(step));
            }
        } else {
            assert_eq!(ring.pop_front(), model.pop_front());
        }
        assert_eq!(ring.len(), model.len());
        assert_eq!(ring.peek_front(), model.front());
        if step % 997 == 0 {
            assert!(ring.iter().eq(model.iter()));
        }
    }

    // ---- MyDeque vs VecDeque, 50k random operations including insert/remove/make_contiguous
    let mut d: MyDeque<u32> = MyDeque::new();
    let mut m: VecDeque<u32> = VecDeque::new();
    for step in 0..50_000u32 {
        match g.below(10) {
            0 | 1 => {
                d.push_back(step);
                m.push_back(step);
            }
            2 | 3 => {
                d.push_front(step);
                m.push_front(step);
            }
            4 => assert_eq!(d.pop_front(), m.pop_front()),
            5 => assert_eq!(d.pop_back(), m.pop_back()),
            6 => {
                let i = g.below(m.len() + 1);
                d.insert(i, step);
                m.insert(i, step);
            }
            7 => {
                let i = g.below(m.len() + 2);
                assert_eq!(d.remove(i), if i < m.len() { m.remove(i) } else { None });
            }
            8 if step % 50 == 0 => {
                assert_eq!(d.make_contiguous(), m.make_contiguous());
                let (a, b) = d.as_slices();
                assert!(b.is_empty() && a.len() == m.len());
            }
            _ => {
                if m.len() > 600 {
                    for _ in 0..300 {
                        d.pop_front();
                        m.pop_front();
                    }
                }
            } // keep sizes moderate
        }
        assert_eq!(d.len(), m.len());
        assert_eq!(d.front(), m.front());
        assert_eq!(d.back(), m.back());
        if step % 331 == 0 {
            assert!(d.iter().eq(m.iter()));
            let (a, b) = d.as_slices();
            assert_eq!(a.len() + b.len(), m.len());
            let joined: Vec<u32> = a.iter().chain(b).copied().collect();
            assert!(joined.iter().eq(m.iter()));
        }
    }
    println!(
        "MyDeque: 50k random ops agree with VecDeque (final len {}, doublings {})",
        d.len(),
        d.grows
    );

    // ---- ByteRing vs VecDeque<u8> with random chunk sizes, counters near overflow, and the zero-copy path
    let mut br = ByteRing::starting_at(64, usize::MAX - 100);
    let mut bm: VecDeque<u8> = VecDeque::new();
    let (mut seq, mut wrapped_views) = (0u8, 0);
    for _ in 0..50_000 {
        if g.below(2) == 0 {
            let n = g.below(40);
            let data: Vec<u8> = (0..n)
                .map(|_| {
                    seq = seq.wrapping_add(1);
                    seq
                })
                .collect();
            let w = br.write(&data);
            assert_eq!(w, n.min(64 - bm.len()));
            bm.extend(&data[..w]);
            seq = seq.wrapping_sub((n - w) as u8); // unwritten bytes are regenerated
        } else if g.below(3) == 0 {
            // zero-copy: fill the contiguous free slice directly
            let region = br.writable();
            let k = region.len().min(g.below(20));
            for b in region[..k].iter_mut() {
                seq = seq.wrapping_add(1);
                *b = seq;
                bm.push_back(seq);
            }
            br.commit(k);
        } else {
            let mut out = vec![0u8; g.below(40)];
            let r = br.read(&mut out);
            assert_eq!(r, out.len().min(bm.len()));
            for &b in &out[..r] {
                assert_eq!(Some(b), bm.pop_front());
            }
        }
        assert_eq!(br.len(), bm.len());
        let (a, b) = br.readable();
        if !b.is_empty() {
            wrapped_views += 1;
        }
        assert!(a.iter().chain(b).eq(bm.iter()));
    }
    assert!(
        wrapped_views > 100,
        "the wrapped (two-segment) case must be exercised"
    );
    println!(
        "ByteRing: 50k ops agree with VecDeque<u8>; {wrapped_views} reads saw a wrapped two-slice view"
    );

    // ---- sliding window vs brute force
    let w = 17usize;
    let mut ws = WindowStats::new(w);
    let mut hist: Vec<i64> = vec![];
    for _ in 0..50_000 {
        let v = (g.below(2001) as i64) - 1000;
        ws.push(v);
        hist.push(v);
        let win = &hist[hist.len().saturating_sub(w)..];
        assert_eq!(ws.min(), *win.iter().min().unwrap());
        assert_eq!(ws.max(), *win.iter().max().unwrap());
        assert!((ws.mean() - win.iter().sum::<i64>() as f64 / win.len() as f64).abs() < 1e-9);
    }

    // ---- no allocation after warm-up
    let mut ring: Ring<u64, 64> = Ring::new();
    let mut fr = FlightRecorder::new(32);
    let mut brr = ByteRing::new(256);
    let mut win = WindowStats::new(64);
    let mut dq: MyDeque<u64> = MyDeque::new();
    let chunk = [7u8; 100];
    let mut sink = [0u8; 100];
    let mut drive = |n: u64| {
        for i in 0..n {
            if ring.is_full() {
                ring.pop_front();
            }
            ring.push_back(i).unwrap();
            fr.log(format_args!("req={i:>8} status=200 latency_us={}", i % 997));
            brr.write(&chunk);
            brr.read(&mut sink);
            win.push((i % 101) as i64);
            dq.push_back(i);
            if dq.len() > 40 {
                dq.pop_front();
            }
        }
    };
    drive(5_000); // warm-up: the deque doubles to its steady size, each log String grows to its longest line
    let a = allocs(|| drive(100_000));
    println!("allocations during 100k steady-state steps across all five rings: {a}");
    assert_eq!(a, 0);
    println!("all assertions passed");
}
