//! RECIPE 179 (systems): hierarchical timing wheel with O(1) insert/cancel, cascading and an overflow list
//! LEVEL: advanced
//! Use in: timeouts, retransmit timers, session expiry, delay queues.   Run: cargo run --release --example timing_wheel
#![allow(dead_code)]
//!
//! WHAT: a timer container where `schedule`, `cancel` and "advance one tick" are O(1) amortised, instead of the
//! O(log n) of a binary heap. Time is an integer tick count (resolution = one tick, e.g. 1 ms).
//!
//! HOW (the "digits of the deadline" view):
//!  * LEVELS wheels of 64 slots each. Level 0 slot = the low 6 bits of the deadline, level 1 = the next 6 bits, ...
//!  * A timer goes to the level of the HIGHEST bit in which `deadline` differs from `now` (`deadline ^ now`), into
//!    the slot named by its digit at that level. If the difference is above all levels it parks in an OVERFLOW list.
//!  * `tick()` does `now += 1`. Whenever the low 6*l bits of `now` become zero, slot `(now >> 6l) & 63` of level l
//!    is CASCADED: its timers are re-placed relative to the new `now` and fall to lower levels (like a carry in an
//!    odometer). A full wrap of the top level also re-places the overflow list. Then level-0 slot `now & 63` fires.
//!  * Each timer lives in a slab and remembers (bucket, position) so `cancel` is a `swap_remove` plus one fix-up.
//!  * Each timer is touched at most once per level it descends through, so total work is O(levels) per timer.
//! Time comes from an injectable `Clock` so tests never sleep. A BinaryHeap queue is the oracle in a differential test.
//!
//! COMMON USES: the Linux kernel's timer wheel for timeouts (cascading wheels, tuned for timers that are usually
//! cancelled before they fire), Kafka's purgatory of delayed operations, Netty's HashedWheelTimer, and tokio's timer
//! driver (a hierarchical wheel with 64-slot levels). Also: TCP retransmit and idle timeouts, lease/session expiry,
//! rate-limiter refills, game-server cooldowns. Heaps still win for few timers, exact (non-quantised) deadlines, or
//! when you need "next deadline" cheaply to sleep precisely.
//! Read: https://github.com/tokio-rs/tokio/tree/master/tokio/src/runtime/time and
//! https://netty.io/4.1/api/io/netty/util/HashedWheelTimer.html and the paper "Hashed and Hierarchical Timing Wheels" (Varghese & Lauck, 1987)
use std::cell::Cell;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};
use std::time::Instant;

const BITS: u32 = 6;
const SLOTS: usize = 1 << BITS;
const LEVELS: usize = 3; // covers 2^18 ticks; farther deadlines use the overflow list
const OVERFLOW: usize = LEVELS * SLOTS; // bucket index of the overflow list

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Handle {
    idx: u32,
    gen_: u32,
}

struct Node {
    id: u64,
    deadline: u64,
    bucket: usize,
    pos: usize,
}
struct Entry {
    gen_: u32,
    node: Option<Node>,
}

struct Wheel {
    now: u64,
    buckets: Vec<Vec<u32>>, // LEVELS*SLOTS slots + overflow
    slab: Vec<Entry>,
    free: Vec<u32>,
    live: usize,
    cascades: u64,
}

impl Wheel {
    fn new(start: u64) -> Self {
        Wheel {
            now: start,
            buckets: (0..=OVERFLOW).map(|_| Vec::new()).collect(),
            slab: Vec::new(),
            free: Vec::new(),
            live: 0,
            cascades: 0,
        }
    }

    /// O(1). A deadline not in the future is clamped to the next tick (it fires on the next `tick`).
    fn schedule(&mut self, id: u64, deadline: u64) -> Handle {
        let deadline = deadline.max(self.now + 1);
        let node = Node {
            id,
            deadline,
            bucket: 0,
            pos: 0,
        };
        let idx = match self.free.pop() {
            Some(i) => {
                self.slab[i as usize].node = Some(node);
                i
            }
            None => {
                self.slab.push(Entry {
                    gen_: 0,
                    node: Some(node),
                });
                (self.slab.len() - 1) as u32
            }
        };
        self.place(idx);
        self.live += 1;
        Handle {
            idx,
            gen_: self.slab[idx as usize].gen_,
        }
    }

    fn bucket_for(&self, deadline: u64) -> usize {
        let x = deadline ^ self.now;
        if x == 0 {
            return (deadline as usize) & (SLOTS - 1); // due right now (only during a cascade)
        }
        let level = ((63 - x.leading_zeros()) / BITS) as usize;
        if level >= LEVELS {
            return OVERFLOW;
        }
        level * SLOTS + ((deadline >> (BITS as usize * level)) as usize & (SLOTS - 1))
    }

    fn place(&mut self, idx: u32) {
        let deadline = self.slab[idx as usize].node.as_ref().unwrap().deadline;
        let b = self.bucket_for(deadline);
        let pos = self.buckets[b].len();
        self.buckets[b].push(idx);
        let n = self.slab[idx as usize].node.as_mut().unwrap();
        n.bucket = b;
        n.pos = pos;
    }

    /// O(1): swap_remove from its bucket and repair the moved timer's position.
    fn cancel(&mut self, h: Handle) -> bool {
        let Some(e) = self.slab.get_mut(h.idx as usize) else {
            return false;
        };
        if e.gen_ != h.gen_ || e.node.is_none() {
            return false;
        }
        let node = e.node.take().unwrap();
        e.gen_ += 1;
        self.free.push(h.idx);
        self.live -= 1;
        let b = &mut self.buckets[node.bucket];
        b.swap_remove(node.pos);
        if let Some(&moved) = b.get(node.pos) {
            self.slab[moved as usize].node.as_mut().unwrap().pos = node.pos;
        }
        true
    }

    /// Advance one tick, appending `(deadline, id)` of every timer that fires.
    fn tick(&mut self, fired: &mut Vec<(u64, u64)>) {
        self.now += 1;
        let mut top = 0;
        for l in 1..=LEVELS {
            if self.now & ((1u64 << (BITS as usize * l)) - 1) == 0 {
                top = l
            } else {
                break;
            }
        }
        if top == LEVELS {
            self.recascade(OVERFLOW); // top wheel wrapped: overflow timers may now fit
        }
        for l in (1..top.min(LEVELS - 1) + 1).rev() {
            let slot = (self.now >> (BITS as usize * l)) as usize & (SLOTS - 1);
            self.recascade(l * SLOTS + slot);
        }
        let slot = self.now as usize & (SLOTS - 1);
        for idx in std::mem::take(&mut self.buckets[slot]) {
            let e = &mut self.slab[idx as usize];
            let n = e.node.take().unwrap();
            e.gen_ += 1;
            self.free.push(idx);
            self.live -= 1;
            debug_assert_eq!(n.deadline, self.now);
            fired.push((n.deadline, n.id));
        }
    }

    fn recascade(&mut self, bucket: usize) {
        for idx in std::mem::take(&mut self.buckets[bucket]) {
            self.cascades += 1;
            self.place(idx);
        }
    }
}

/// The oracle: O(log n) insert, lazy cancel, exact next-deadline.
struct HeapTimers {
    now: u64,
    heap: BinaryHeap<Reverse<(u64, u64)>>,
    live: HashSet<u64>,
}
impl HeapTimers {
    fn new(start: u64) -> Self {
        HeapTimers {
            now: start,
            heap: BinaryHeap::new(),
            live: HashSet::new(),
        }
    }
    fn schedule(&mut self, id: u64, deadline: u64) {
        self.heap.push(Reverse((deadline.max(self.now + 1), id)));
        self.live.insert(id);
    }
    fn cancel(&mut self, id: u64) -> bool {
        self.live.remove(&id)
    }
    fn tick(&mut self, fired: &mut Vec<(u64, u64)>) {
        self.now += 1;
        while let Some(&Reverse((d, id))) = self.heap.peek() {
            if d > self.now {
                break;
            }
            self.heap.pop();
            if self.live.remove(&id) {
                fired.push((d, id));
            }
        }
    }
}

// ---- injectable clock + a service that maps wall time to ticks ----
trait Clock {
    fn now_ms(&self) -> u64;
}
struct ManualClock(Cell<u64>);
impl ManualClock {
    fn advance(&self, ms: u64) {
        self.0.set(self.0.get() + ms)
    }
}
impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.0.get()
    }
}
struct SystemMono(Instant);
impl Clock for SystemMono {
    fn now_ms(&self) -> u64 {
        self.0.elapsed().as_millis() as u64
    }
}

/// Tick resolution trade-off: a timer fires at the first tick boundary at or after its deadline, so error < `tick_ms`.
struct TimerService<'c, C: Clock> {
    clock: &'c C,
    tick_ms: u64,
    wheel: Wheel,
}
impl<'c, C: Clock> TimerService<'c, C> {
    fn new(clock: &'c C, tick_ms: u64) -> Self {
        TimerService {
            clock,
            tick_ms,
            wheel: Wheel::new(clock.now_ms() / tick_ms),
        }
    }
    fn after_ms(&mut self, id: u64, delay_ms: u64) -> Handle {
        let deadline_ms = self.clock.now_ms() + delay_ms;
        self.wheel.schedule(id, deadline_ms.div_ceil(self.tick_ms)) // round UP: never fire early
    }
    fn poll(&mut self) -> Vec<u64> {
        let target = self.clock.now_ms() / self.tick_ms;
        let mut out = Vec::new();
        while self.wheel.now < target {
            self.wheel.tick(&mut out);
        }
        out.into_iter().map(|(_, id)| id).collect()
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
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn differential_test(seed: u64, ops: usize) -> (u64, u64, u64) {
    let mut rng = Rng(seed);
    let (mut w, mut h) = (Wheel::new(0), HeapTimers::new(0));
    let mut handles: Vec<(u64, Handle)> = Vec::new();
    let (mut next_id, mut total_fired, mut ticks) = (0u64, 0u64, 0u64);
    let (mut a, mut b) = (Vec::new(), Vec::new());
    for _ in 0..ops {
        match rng.below(100) {
            0..=44 => {
                let delta = match rng.below(10) {
                    0..=3 => 1 + rng.below(64),
                    4..=6 => 1 + rng.below(5_000),
                    7..=8 => 1 + rng.below(200_000),
                    _ => 1 + rng.below(3_000_000), // beyond 2^18: exercises the overflow list
                };
                let deadline = if rng.below(50) == 0 {
                    w.now
                } else {
                    w.now + delta
                }; // some "already due"
                handles.push((next_id, w.schedule(next_id, deadline)));
                h.schedule(next_id, deadline);
                next_id += 1;
            }
            45..=64 if !handles.is_empty() => {
                let (id, hd) = handles.swap_remove(rng.below(handles.len() as u64) as usize);
                assert_eq!(
                    w.cancel(hd),
                    h.cancel(id),
                    "cancel disagreement for timer {id}"
                );
            }
            _ => {
                let steps = match rng.below(100) {
                    0..=59 => 1 + rng.below(8),
                    60..=89 => 1 + rng.below(500),
                    90..=98 => 1 + rng.below(20_000),
                    _ => 1 + rng.below(300_000),
                };
                for _ in 0..steps {
                    a.clear();
                    b.clear();
                    w.tick(&mut a);
                    h.tick(&mut b);
                    ticks += 1;
                    assert_eq!(w.now, h.now);
                    if a.is_empty() && b.is_empty() {
                        continue;
                    }
                    assert!(
                        a.windows(2).all(|p| p[0].0 <= p[1].0),
                        "deadlines must be non-decreasing within a tick"
                    );
                    a.sort();
                    b.sort();
                    assert_eq!(a, b, "fired sets differ at tick {}", w.now);
                    total_fired += a.len() as u64;
                }
                assert_eq!(w.live, h.live.len());
            }
        }
    }
    (total_fired, ticks, w.cascades)
}

fn main() {
    // 1. basic behaviour: fire at exactly the deadline, cancel, clamping
    let mut w = Wheel::new(0);
    let a = w.schedule(1, 5);
    let _b = w.schedule(2, 70); // level 1
    let c = w.schedule(3, 5000); // level 2
    let _d = w.schedule(4, 1 << 20); // overflow
    assert!(w.cancel(c));
    assert!(
        !w.cancel(c),
        "double cancel must be false (generation check)"
    );
    let mut out = Vec::new();
    for _ in 0..5 {
        w.tick(&mut out);
    }
    assert_eq!(out, vec![(5, 1)]);
    assert!(!w.cancel(a), "already fired");
    out.clear();
    while w.now < (1 << 20) {
        w.tick(&mut out);
    }
    assert_eq!(out, vec![(70, 2), (1 << 20, 4)]);
    println!("basic wheel ok: fired {:?}, cascades {}", out, w.cascades);

    // 2. injectable clock + tick resolution (10 ms ticks): never early, late by < tick
    let clock = ManualClock(Cell::new(1_000));
    let mut svc = TimerService::new(&clock, 10);
    svc.after_ms(1, 95);
    svc.after_ms(2, 100);
    let cancelled = svc.after_ms(3, 120);
    clock.advance(94);
    assert!(svc.poll().is_empty());
    clock.advance(6); // t = 100 ms after start -> ticks up to 110
    assert_eq!(svc.poll(), vec![1, 2]);
    assert!(svc.wheel.cancel(cancelled));
    clock.advance(1_000);
    assert!(svc.poll().is_empty());
    let sys = SystemMono(Instant::now());
    let _ = sys.now_ms(); // same trait works with a real monotonic clock
    println!("clock-driven service ok (10ms resolution)");

    // 3. differential test: wheel vs BinaryHeap, 50k seeded random operations, compared at EVERY tick
    let t0 = Instant::now();
    let mut totals = (0, 0, 0);
    for seed in [0x9E3779B97F4A7C15u64, 0xDEADBEEFCAFEF00D] {
        let r = differential_test(seed, 25_000);
        totals = (totals.0 + r.0, totals.1 + r.1, totals.2 + r.2);
    }
    assert!(totals.0 > 1_000, "test should actually fire timers");
    println!(
        "differential: 50000 ops, {} ticks compared, {} timers fired, {} cascade moves, {:?}",
        totals.1,
        totals.0,
        totals.2,
        t0.elapsed()
    );

    // 4. micro-benchmark: schedule N timers then cancel 90% (typical timeout pattern)
    let n = 300_000u64;
    let mut rng = Rng(7);
    let t = Instant::now();
    let mut w = Wheel::new(0);
    let hs: Vec<_> = (0..n)
        .map(|i| w.schedule(i, 1 + rng.below(200_000)))
        .collect();
    for (i, h) in hs.iter().enumerate() {
        if i % 10 != 0 {
            w.cancel(*h);
        }
    }
    let wheel_t = t.elapsed();
    let mut rng = Rng(7);
    let t = Instant::now();
    let mut hp = HeapTimers::new(0);
    for i in 0..n {
        hp.schedule(i, 1 + rng.below(200_000));
    }
    for i in 0..n {
        if i % 10 != 0 {
            hp.cancel(i);
        }
    }
    let heap_t = t.elapsed();
    println!(
        "{n} schedule + 90% cancel: wheel {wheel_t:?}, heap {heap_t:?} (informational, not asserted)"
    );

    println!(
        "\n{:<16}{:<26}{:<30}",
        "operation", "timing wheel", "binary heap"
    );
    println!("{:<16}{:<26}{:<30}", "schedule", "O(1)", "O(log n)");
    println!(
        "{:<16}{:<26}{:<30}",
        "cancel", "O(1) (slab + slot)", "O(log n) or lazy O(1)+memory"
    );
    println!(
        "{:<16}{:<26}{:<30}",
        "advance 1 tick", "O(1) + cascades", "O(k log n) for k expiries"
    );
    println!(
        "{:<16}{:<26}{:<30}",
        "next deadline", "O(slots) scan / ticking", "O(1) peek"
    );
    println!(
        "{:<16}{:<26}{:<30}",
        "precision", "quantised to a tick", "exact"
    );
    println!(
        "wheel wins: huge timer counts, most cancelled, coarse deadlines. heap wins: few timers, exact wake-ups, sparse time."
    );
    println!("timing_wheel: all assertions passed");
}
