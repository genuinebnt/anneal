//! RECIPE 441 (concurrency): seqlock for tear-free multi-word reads, and priority inversion vs inheritance vs ceiling.
//! LEVEL: advanced
//! Use in: read-mostly shared records (clocks, stats, config snapshots), real-time scheduling and lock design.   Run: cargo run --release --example seqlock_and_priority_inversion
#![allow(dead_code)]
//!
//! PART A, SEQLOCK. WHAT: a lock for small `Copy` data where readers never block writers and never write shared
//! memory (no cache-line ping-pong between readers). A sequence counter `seq` is EVEN when the data is stable and
//! ODD while a writer is inside. Writer: `seq += 1` (odd), write data, `seq += 1` (even). Reader: read `seq`
//! (retry if odd), copy the data, re-read `seq`, retry if it changed. A reader may see garbage in the middle of its
//! copy, but then `seq` differs and the copy is thrown away, so garbage is never RETURNED.
//! WHY THE NAIVE VERSION IS UNSOUND IN RUST: copying a plain struct (`*ptr` / `ptr::read`) while a writer writes
//! it is a DATA RACE, which is undefined behaviour in the Rust and C++ memory models even if the result is
//! discarded afterwards: the compiler may assume no concurrent write and, for example, reload or tear the copy
//! in ways the retry logic cannot undo. SOUND OPTIONS: (1) store every word as an `AtomicU64` and access it with
//! Relaxed loads/stores (this file: racing atomics are not UB, only possibly stale/torn ACROSS words, which the
//! sequence check detects); (2) `MaybeUninit<T>` plus `ptr::read_volatile` copies, which is what several crates do,
//! though strictly the language reference still calls a racing non-atomic read UB (the C++ proposal for
//! `atomic_load_per_byte_memcpy` exists to bless it); option 1 needs no `unsafe` at all, so we use it.
//! ORDERINGS (Hans Boehm, "Can Seqlocks Get Along with Programming Language Memory Models?"):
//! writer: `seq.store(s+1, Relaxed); fence(Release); data stores (Relaxed); seq.store(s+2, Release)`.
//! The release FENCE keeps the odd store from sinking below the data stores; the final release store publishes them.
//! reader: `s1 = seq.load(Acquire); data loads (Relaxed); fence(Acquire); s2 = seq.load(Relaxed)`. The acquire
//! FENCE keeps the data loads from sinking below the re-check of `seq`. Writers are serialised by a Mutex (a seqlock
//! has one writer at a time). Readers can starve if writers never pause; that is the price of wait-free writers.
//! PART B, PRIORITY INVERSION. A deterministic discrete-event simulation of a fixed-priority preemptive single CPU
//! (one tick per step, no threads, no timing). Mars Pathfinder (1997): a low-priority task L holds the bus mutex;
//! the high-priority task H needs it and blocks; medium tasks M (which need no lock) preempt L, so L cannot release
//! the lock and H waits for ALL the medium work: unbounded inversion (the watchdog reset the lander; the fix was
//! enabling priority inheritance remotely).
//! PRIORITY INHERITANCE: while H waits for a lock L holds, L runs at H's priority (transitively through chains), so
//! M cannot preempt it. H's blocking is at most one critical section per lock. Does NOT prevent deadlock.
//! PRIORITY CEILING (immediate / ICPP, POSIX PRIO_PROTECT): every lock has a ceiling = highest priority of any task
//! that ever uses it; a task that takes the lock immediately runs at the ceiling. Blocking is at most one critical
//! section, a lock is never found held by a blocked task, so it also prevents deadlock from nested locks.
//! Cost: you must know all lockers in advance, and tasks are boosted even without contention.
//! We measure INVERSION TICKS: ticks where H is released and unfinished, yet a lower-priority task is running.
//! COMMON USES: seqlocks: the Linux kernel (`seqlock_t`, used for jiffies/xtime and VDSO time reads), game and trading
//! engines publishing the latest quote or state snapshot to many readers, metrics counters read by a scraper;
//! priority inheritance/ceiling: POSIX `PTHREAD_PRIO_INHERIT`/`PTHREAD_PRIO_PROTECT`, RTOS mutexes (VxWorks, which
//! ran Pathfinder; FreeRTOS mutexes inherit), Linux `rt_mutex` / PI futexes, the Ada protected-object ceiling rule.
//! Read: Boehm, "Can Seqlocks Get Along with Programming Language Memory Models?" (MSPC 2012);
//! Sha, Rajkumar, Lehoczky, "Priority Inheritance Protocols: An Approach to Real-Time Synchronization" (1990);
//! https://en.wikipedia.org/wiki/Seqlock and https://en.wikipedia.org/wiki/Priority_inheritance and https://en.wikipedia.org/wiki/Priority_ceiling_protocol
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::*, fence};
use std::thread;
use std::time::Instant;

// ================================================================ PART A: seqlock

/// A seqlock over N 64-bit words. All shared data is atomic, so there is no data race and no `unsafe`.
struct SeqLock<const N: usize> {
    seq: AtomicU64,
    words: [AtomicU64; N],
    writers: Mutex<()>,
}

impl<const N: usize> SeqLock<N> {
    fn new(init: [u64; N]) -> Self {
        Self {
            seq: AtomicU64::new(0),
            words: init.map(AtomicU64::new),
            writers: Mutex::new(()),
        }
    }

    fn write(&self, v: &[u64; N]) {
        let _one_writer = self.writers.lock().unwrap();
        let s = self.seq.load(Relaxed); // the mutex already orders us after the previous writer
        self.seq.store(s + 1, Relaxed); // odd: "write in progress"
        fence(Release); // the odd store must be visible before any data store
        for (w, x) in self.words.iter().zip(v) {
            w.store(*x, Relaxed);
        }
        self.seq.store(s + 2, Release); // even again: publishes the data
    }

    /// Returns the snapshot and how many attempts were discarded.
    fn read(&self) -> ([u64; N], u32) {
        let mut retries = 0;
        loop {
            let s1 = self.seq.load(Acquire);
            if s1 & 1 == 0 {
                let mut out = [0u64; N];
                for (o, w) in out.iter_mut().zip(&self.words) {
                    *o = w.load(Relaxed);
                }
                fence(Acquire); // data loads must complete before the re-check below
                if self.seq.load(Relaxed) == s1 {
                    return (out, retries);
                }
            }
            retries += 1;
            std::hint::spin_loop();
        }
    }
}

/// The multi-word record. Invariants tie all four fields to `bid`, so any mix of two versions is detectable.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Quote {
    bid: u64,
    ask: u64,
    tag: u64,
    checksum: u64,
}

impl Quote {
    fn from_bid(bid: u64) -> Self {
        Quote {
            bid,
            ask: bid + 7,
            tag: bid ^ 0xABCD_EF01_2345_6789,
            checksum: bid.wrapping_mul(31),
        }
    }
    fn is_consistent(&self) -> bool {
        *self == Quote::from_bid(self.bid)
    }
    fn to_words(self) -> [u64; 4] {
        [self.bid, self.ask, self.tag, self.checksum]
    }
    fn from_words(w: [u64; 4]) -> Self {
        Quote {
            bid: w[0],
            ask: w[1],
            tag: w[2],
            checksum: w[3],
        }
    }
}

const READERS: usize = 4;
const READS_PER_READER: u64 = 5_000_000; // 20M reads in total

fn seqlock_stress() {
    let lock = SeqLock::<4>::new(Quote::from_bid(0).to_words());
    let done = AtomicBool::new(false);
    let t = Instant::now();
    let (total_reads, total_retries, writes) = thread::scope(|s| {
        let writer = s.spawn(|| {
            let mut n = 0u64;
            while !done.load(Relaxed) {
                n += 1;
                lock.write(&Quote::from_bid(n).to_words());
                for _ in 0..20 {
                    std::hint::spin_loop(); // let readers finish a copy; a 100% hot writer would starve them
                }
            }
            n
        });
        let readers: Vec<_> = (0..READERS)
            .map(|_| {
                s.spawn(|| {
                    let (mut last_bid, mut retries) = (0u64, 0u64);
                    for _ in 0..READS_PER_READER {
                        let (words, r) = lock.read();
                        let q = Quote::from_words(words);
                        assert!(q.is_consistent(), "TORN READ: {q:?}");
                        assert!(
                            q.bid >= last_bid,
                            "time went backwards: {} after {last_bid}",
                            q.bid
                        );
                        last_bid = q.bid;
                        retries += r as u64;
                    }
                    retries
                })
            })
            .collect::<Vec<_>>();
        let retries: u64 = readers.into_iter().map(|h| h.join().unwrap()).sum();
        done.store(true, Relaxed);
        (
            READERS as u64 * READS_PER_READER,
            retries,
            writer.join().unwrap(),
        )
    });
    assert_eq!(total_reads, 20_000_000);
    assert!(writes > 0);
    let final_q = Quote::from_words(lock.read().0);
    assert_eq!(
        final_q,
        Quote::from_bid(writes),
        "the last write is the one readers see"
    );
    println!(
        "A. seqlock: {total_reads} reads, 0 torn, {writes} writes, {total_retries} discarded attempts, {:?}",
        t.elapsed()
    );
}

/// The same four words WITHOUT the sequence check: shows what the seqlock is protecting you from. Torn count is
/// machine dependent, so it is printed, never asserted.
fn naive_torn_demo() {
    let words: [AtomicU64; 4] = Quote::from_bid(0).to_words().map(AtomicU64::new);
    let done = AtomicBool::new(false);
    let torn = thread::scope(|s| {
        s.spawn(|| {
            let mut n = 0;
            while !done.load(Relaxed) {
                n += 1;
                for (w, x) in words.iter().zip(Quote::from_bid(n).to_words()) {
                    w.store(x, Relaxed);
                }
            }
        });
        let reader = s.spawn(|| {
            let mut torn = 0u32;
            for _ in 0..2_000_000 {
                let q = Quote::from_words(std::array::from_fn(|i| words[i].load(Relaxed)));
                torn += !q.is_consistent() as u32;
            }
            torn
        });
        let torn = reader.join().unwrap();
        done.store(true, Relaxed);
        torn
    });
    println!(
        "A. naive per-word atomics with no sequence counter: {torn} torn reads out of 2M (machine dependent)"
    );
}

// ================================================================ PART B: scheduler simulation

#[derive(Clone, Copy, Debug)]
enum Op {
    Compute(u32),
    Lock(usize),
    Unlock(usize),
}

struct TaskSpec {
    name: &'static str,
    prio: u8, // larger = more urgent
    release: u32,
    ops: Vec<Op>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Protocol {
    None,
    Inheritance,
    Ceiling,
}

#[derive(Debug)]
struct Outcome {
    finish: Vec<Option<u32>>,
    /// Ticks in which task i was released and unfinished while a lower-base-priority task ran.
    inversion: Vec<u32>,
    deadlock_at: Option<u32>,
    timeline: String,
}

struct Task<'a> {
    spec: &'a TaskSpec,
    pc: usize,
    left: u32, // ticks left in the current Compute
    held: Vec<usize>,
    blocked_on: Option<usize>,
    done: bool,
}

#[derive(PartialEq)]
enum Settled {
    Ready,
    Blocked,
    Done,
}

struct Sim<'a> {
    proto: Protocol,
    tasks: Vec<Task<'a>>,
    owner: Vec<Option<usize>>,
    ceiling: Vec<u8>,
}

impl<'a> Sim<'a> {
    fn new(specs: &'a [TaskSpec], proto: Protocol) -> Self {
        let nlocks = specs
            .iter()
            .flat_map(|s| &s.ops)
            .filter_map(|o| {
                if let Op::Lock(l) = o {
                    Some(l + 1)
                } else {
                    None
                }
            })
            .max()
            .unwrap_or(0);
        let mut ceiling = vec![0u8; nlocks];
        for s in specs {
            for op in &s.ops {
                if let Op::Lock(l) = op {
                    ceiling[*l] = ceiling[*l].max(s.prio); // ceiling = highest priority of any user
                }
            }
        }
        let tasks = specs
            .iter()
            .map(|spec| Task {
                spec,
                pc: 0,
                left: 0,
                held: vec![],
                blocked_on: None,
                done: false,
            })
            .collect();
        Sim {
            proto,
            tasks,
            owner: vec![None; nlocks],
            ceiling,
        }
    }

    /// Effective priority of every task under the protocol.
    fn effective(&self) -> Vec<u8> {
        let mut eff: Vec<u8> = self.tasks.iter().map(|t| t.spec.prio).collect();
        match self.proto {
            Protocol::None => {}
            Protocol::Ceiling => {
                for (i, t) in self.tasks.iter().enumerate() {
                    for &l in &t.held {
                        eff[i] = eff[i].max(self.ceiling[l]);
                    }
                }
            }
            Protocol::Inheritance => loop {
                // Fixed point so inheritance is transitive: H -> L1 (holds lock) which itself waits on L2's lock...
                let mut changed = false;
                for b in 0..self.tasks.len() {
                    if let Some(l) = self.tasks[b].blocked_on {
                        if let Some(o) = self.owner[l] {
                            if eff[o] < eff[b] {
                                eff[o] = eff[b];
                                changed = true;
                            }
                        }
                    }
                }
                if !changed {
                    break;
                }
            },
        }
        eff
    }

    /// Execute zero-time operations (Lock/Unlock) of task `i` until it has CPU work, blocks, or finishes.
    fn settle(&mut self, i: usize, now: u32, finish: &mut [Option<u32>]) -> Settled {
        loop {
            if self.tasks[i].left > 0 {
                return Settled::Ready;
            }
            let Some(&op) = self.tasks[i].spec.ops.get(self.tasks[i].pc) else {
                self.tasks[i].done = true;
                finish[i] = Some(now);
                return Settled::Done;
            };
            match op {
                Op::Compute(n) => {
                    self.tasks[i].left = n;
                    self.tasks[i].pc += 1;
                }
                Op::Lock(l) => {
                    if self.owner[l].is_some() {
                        assert!(
                            self.proto != Protocol::Ceiling,
                            "under ICPP a lock is never found held"
                        );
                        self.tasks[i].blocked_on = Some(l);
                        return Settled::Blocked;
                    }
                    self.owner[l] = Some(i);
                    self.tasks[i].held.push(l);
                    self.tasks[i].blocked_on = None;
                    self.tasks[i].pc += 1;
                }
                Op::Unlock(l) => {
                    assert_eq!(self.owner[l], Some(i));
                    self.owner[l] = None;
                    self.tasks[i].held.retain(|&h| h != l);
                    self.tasks[i].pc += 1;
                    for t in &mut self.tasks {
                        if t.blocked_on == Some(l) {
                            t.blocked_on = None; // waiters retry their Lock when next scheduled
                        }
                    }
                }
            }
        }
    }

    fn run(mut self) -> Outcome {
        let n = self.tasks.len();
        let mut finish = vec![None; n];
        let mut inversion = vec![0u32; n];
        let mut timeline = String::new();
        let mut last: Option<usize> = None;
        let mut now = 0u32;
        while self.tasks.iter().any(|t| !t.done) {
            assert!(now < 100_000, "simulation did not terminate");
            // Pick the highest effective priority runnable task; ties keep the running task (no preemption by equals).
            let pick = loop {
                let eff = self.effective();
                let released = |t: &Task| t.spec.release <= now && !t.done;
                let best = (0..n)
                    .filter(|&i| released(&self.tasks[i]) && self.tasks[i].blocked_on.is_none())
                    .max_by_key(|&i| (eff[i], Some(i) == last, std::cmp::Reverse(i)));
                match best {
                    None => break None,
                    Some(i) => match self.settle(i, now, &mut finish) {
                        Settled::Ready => break Some(i),
                        _ => {} // blocked or finished at once: choose again
                    },
                }
            };
            let waiting = (0..n)
                .filter(|&i| self.tasks[i].spec.release <= now && !self.tasks[i].done)
                .count();
            let Some(i) = pick else {
                if waiting > 0 {
                    return Outcome {
                        finish,
                        inversion,
                        deadlock_at: Some(now),
                        timeline,
                    };
                }
                timeline.push('.');
                now += 1;
                last = None;
                continue;
            };
            for h in 0..n {
                let t = &self.tasks[h];
                if h != i
                    && t.spec.release <= now
                    && !t.done
                    && self.tasks[i].spec.prio < t.spec.prio
                {
                    inversion[h] += 1;
                }
            }
            timeline.push(self.tasks[i].spec.name.chars().next().unwrap());
            self.tasks[i].left -= 1;
            now += 1;
            last = Some(i);
            self.settle(i, now, &mut finish); // run Unlock etc. at the instant the compute ends
        }
        Outcome {
            finish,
            inversion,
            deadlock_at: None,
            timeline,
        }
    }
}

const CS_LEN: u32 = 4; // length of L's critical section

/// Mars Pathfinder shape: L(prio 1) holds the lock for 4 ticks, H(prio 3) arrives at t=2 and needs it,
/// `mediums` tasks (prio 2) of 20 ticks each arrive at t=3 and need no lock.
fn pathfinder(mediums: usize) -> Vec<TaskSpec> {
    let mut v = vec![
        TaskSpec {
            name: "L",
            prio: 1,
            release: 0,
            ops: vec![Op::Lock(0), Op::Compute(CS_LEN), Op::Unlock(0)],
        },
        TaskSpec {
            name: "H",
            prio: 3,
            release: 2,
            ops: vec![Op::Lock(0), Op::Compute(2), Op::Unlock(0)],
        },
    ];
    for _ in 0..mediums {
        v.push(TaskSpec {
            name: "M",
            prio: 2,
            release: 3,
            ops: vec![Op::Compute(20)],
        });
    }
    v
}

fn inversion_experiment() {
    println!(
        "B. priority inversion: ticks H is delayed by lower-priority work (critical section = {CS_LEN} ticks)"
    );
    println!("   mediums | none    | inheritance | ceiling");
    let mut none_prev = 0;
    for m in [0usize, 1, 3, 10] {
        let specs = pathfinder(m);
        let res: Vec<Outcome> = [Protocol::None, Protocol::Inheritance, Protocol::Ceiling]
            .map(|p| Sim::new(&specs, p).run())
            .into();
        let inv: Vec<u32> = res.iter().map(|o| o.inversion[1]).collect();
        println!("   {m:7} | {:7} | {:11} | {:7}", inv[0], inv[1], inv[2]);
        // Without a protocol every medium task adds its full 20 ticks to H's delay: unbounded in the number of mediums.
        assert!(inv[0] >= 20 * m as u32);
        assert!(m == 0 || inv[0] > none_prev);
        none_prev = inv[0];
        // With either protocol the delay is at most one critical section, whatever the medium load.
        assert!(
            inv[1] <= CS_LEN && inv[2] <= CS_LEN,
            "bounded blocking: {inv:?}"
        );
        for o in &res {
            assert!(o.deadlock_at.is_none() && o.finish.iter().all(Option::is_some));
        }
        if m == 1 {
            for (p, o) in ["none", "inheritance", "ceiling"].iter().zip(&res) {
                println!("   timeline m=1 {p:11}: {}", o.timeline);
            }
            // H finishes much later without a protocol.
            assert!(res[0].finish[1] > res[1].finish[1] && res[1].finish[1] == res[2].finish[1]);
        }
    }
}

/// Nested locks taken in opposite orders: the classic deadlock. T1 (low) takes A then B, T2 (high) takes B then A.
fn nested_locks() -> Vec<TaskSpec> {
    vec![
        TaskSpec {
            name: "1",
            prio: 1,
            release: 0,
            ops: vec![
                Op::Lock(0),
                Op::Compute(2),
                Op::Lock(1),
                Op::Compute(1),
                Op::Unlock(1),
                Op::Unlock(0),
            ],
        },
        TaskSpec {
            name: "2",
            prio: 3,
            release: 1,
            ops: vec![
                Op::Lock(1),
                Op::Compute(1),
                Op::Lock(0),
                Op::Compute(1),
                Op::Unlock(0),
                Op::Unlock(1),
            ],
        },
    ]
}

fn deadlock_experiment() {
    let specs = nested_locks();
    println!("B. nested locks taken in opposite order (A then B vs B then A):");
    for p in [Protocol::None, Protocol::Inheritance, Protocol::Ceiling] {
        let o = Sim::new(&specs, p).run();
        match (p, o.deadlock_at) {
            (Protocol::Ceiling, None) => {
                assert!(o.finish.iter().all(Option::is_some));
                println!(
                    "   {p:?}: no deadlock, finished at {:?}, timeline {}",
                    o.finish, o.timeline
                );
            }
            (_, Some(t)) => println!("   {p:?}: DEADLOCK at t={t}, timeline {}", o.timeline),
            (p, None) => panic!("{p:?} was expected to deadlock"),
        }
    }
    // Inheritance fixes inversion but not the lock-order cycle; the ceiling serialises the critical sections so the cycle never forms.
    assert!(Sim::new(&specs, Protocol::None).run().deadlock_at.is_some());
    assert!(
        Sim::new(&specs, Protocol::Inheritance)
            .run()
            .deadlock_at
            .is_some()
    );
    assert!(
        Sim::new(&specs, Protocol::Ceiling)
            .run()
            .deadlock_at
            .is_none()
    );
}

fn main() {
    seqlock_stress();
    naive_torn_demo();
    inversion_experiment();
    deadlock_experiment();
}
