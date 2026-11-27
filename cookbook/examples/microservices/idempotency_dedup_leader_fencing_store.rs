//! RECIPE 912 (microservices): doing things once: idempotency keys, consumer dedup windows, CAS leader election with fencing, and a fenced resource under chaos
//! LEVEL: advanced
//! Use in: any API that clients retry, any consumer of an at-least-once stream, any "only one active worker" job.   Run: cargo run --release --example idempotency_dedup_leader_fencing_store
//! COMMON USES: Stripe-style Idempotency-Key headers, Kafka idempotent producer sequence numbers, IPsec/DTLS anti-replay windows, etcd/ZooKeeper leader election with revisions, Kafka controller and Raft term fencing.
#![allow(dead_code)]
//!
//! Companions: `axum_idempotency` is the HTTP shape; `lease_fencing_sim` shows WHY a lease without tokens fails (not repeated here);
//! `transactional_outbox_inbox_sqlite` makes "record the key" and "do the effect" one transaction.
//!
//! A. IDEMPOTENCY KEYS. State per (key): IN-FLIGHT(attempt, fingerprint, lock expiry) or DONE(response, fingerprint, retention expiry).
//!    Same key + different request body is a CLIENT BUG and is refused, never replayed. A concurrent duplicate gets "in flight"
//!    (HTTP 409), not a second execution. A handler that dies leaves a lock that expires; the takeover gets a NEW attempt number and
//!    the old attempt can no longer complete. WHAT THIS CANNOT DO: if the handler died AFTER the effect and BEFORE `complete`, the
//!    retry runs the effect again. Only an effect committed ATOMICALLY with the key (same database transaction) or an effect that is
//!    itself keyed closes that gap; both outcomes are demonstrated.
//! B. DEDUP FOR CONSUMERS. Three tools: (1) exact set with a time window; (2) per-producer SLIDING WINDOW over sequence numbers (a
//!    high-water mark plus a bitmap; anything older than the window is refused, so memory is constant); (3) a BLOOM FILTER as a
//!    pre-check: "definitely new" skips the expensive exact lookup, "maybe seen" MUST fall through to it. Dropping on a Bloom hit
//!    silently loses new messages. No window is forever: a duplicate older than the window is a design decision, not a surprise.
//! C. LEADER ELECTION on a linearisable store with only `read` and `compare_and_set(revision)`. The record is (holder, expires_at,
//!    token); every change of leadership increments the token. Expiry is judged by the STORE's clock. Nodes measure their own
//!    lease with their own, skewed clocks, and may be paused at any instruction.
//! D. THE FENCED RESOURCE rejects a token lower than the highest it has seen, checking and writing in ONE atomic step.
//!    THE SUBTLE PART: that alone is not enough. Until the new leader TOUCHES the resource, the resource has not heard of the new
//!    token, so an old leader's delayed write is still accepted, possibly AFTER the new leader has read the state it is about to
//!    overwrite: a lost update. So a new leader must first FENCE the resource (publish its token) and only THEN read.
//!    The chaos test runs the same schedules with: no check, check only, check + fence-before-read.
//! SIMULATION: single threaded and seeded; each node is a state machine advanced one step at a time, so a "pause" is simply not
//! being scheduled while the store clock runs. In-flight messages are delayed and reordered. Not modelled: the lease store itself
//! failing (that is consensus: see the Raft recipes), or Byzantine nodes that forge tokens. Nor a message delivered TWICE: a token
//! check alone accepts a repeated write from the current leader, which can undo its later write; that also needs a sequence number
//! or a version check. LIMITS: `IdemStore` never evicts a key (an expired one stays until the same key is used again), and a key
//! left in flight by a crash stays tied to its first request body. `Window::accept` panics on sequence number 0, so check
//! untrusted input before calling it; and one forged huge number moves the window past all real ones (authenticate first).
//! Read: https://stripe.com/blog/idempotency , https://martin.kleppmann.com/2016/02/08/how-to-do-distributed-locking.html
//! Read: https://etcd.io/docs/v3.5/learning/api/ and https://www.rfc-editor.org/rfc/rfc4303#section-3.4.3 (anti-replay window)
//!
//! TRY NEXT:
//!   - [S] In `IdemStore::complete` stop checking the attempt number. Which assertion in the random model test fails first?
//!   - [S] In `Window::accept` treat "older than the window" as new. Predict the first oracle disagreement.
//!   - [M] Make the node renew its lease while working and measure how often a renewal is itself delayed past expiry. Does safety change? Should it?
//!   - [M] Replace the resource's token check by a compare-and-set on a data VERSION. Which of the three chaos variants does it make safe without any lease?
//!   - [A] PROJECT: put `IdemStore` in SQLite with the effect in the same transaction, then kill the process at every statement boundary and prove exactly-once.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix(self.0)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}
fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

// ===========================================================================
// A. idempotency keys
// ===========================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
enum Entry {
    InFlight {
        fingerprint: u64,
        attempt: u64,
        lock_until: u64,
    },
    Done {
        fingerprint: u64,
        response: u64,
        keep_until: u64,
    },
}
#[derive(Debug, PartialEq)]
enum Begin {
    Execute { attempt: u64 },
    Replay { response: u64 },
    InFlight,
    FingerprintMismatch,
}
#[derive(Default)]
struct IdemStore {
    entries: HashMap<u64, Entry>,
    attempts: u64,
}
const LOCK_MS: u64 = 30;
const RETAIN_MS: u64 = 200;
impl IdemStore {
    /// One atomic step: in SQL, `INSERT ... ON CONFLICT DO NOTHING` plus a conditional takeover `UPDATE`.
    fn begin(&mut self, key: u64, fingerprint: u64, now: u64) -> Begin {
        match self.entries.get(&key) {
            Some(&Entry::Done {
                fingerprint: f,
                response,
                keep_until,
            }) if keep_until > now => {
                return if f == fingerprint {
                    Begin::Replay { response }
                } else {
                    Begin::FingerprintMismatch
                };
            }
            Some(&Entry::InFlight {
                fingerprint: f,
                lock_until,
                ..
            }) => {
                if f != fingerprint {
                    return Begin::FingerprintMismatch;
                }
                if lock_until > now {
                    return Begin::InFlight;
                }
            }
            _ => {}
        }
        self.attempts += 1;
        self.entries.insert(
            key,
            Entry::InFlight {
                fingerprint,
                attempt: self.attempts,
                lock_until: now + LOCK_MS,
            },
        );
        Begin::Execute {
            attempt: self.attempts,
        }
    }
    fn owns(&self, key: u64, mine: u64) -> bool {
        matches!(self.entries.get(&key), Some(Entry::InFlight { attempt, .. }) if *attempt == mine)
    }
    /// False: someone took over; our result must be discarded (and our effect must not have been committed separately).
    fn complete(&mut self, key: u64, mine: u64, response: u64, now: u64) -> bool {
        if !self.owns(key, mine) {
            return false;
        }
        let Entry::InFlight { fingerprint, .. } = self.entries[&key] else {
            unreachable!()
        };
        self.entries.insert(
            key,
            Entry::Done {
                fingerprint,
                response,
                keep_until: now + RETAIN_MS,
            },
        );
        true
    }
    /// A failure that certainly had no effect releases the key so a retry can run at once.
    fn fail(&mut self, key: u64, mine: u64) {
        if self.owns(key, mine) {
            self.entries.remove(&key);
        }
    }
}
fn idempotency_basics() {
    let mut s = IdemStore::default();
    let Begin::Execute { attempt: a } = s.begin(1, 100, 0) else {
        panic!()
    };
    assert_eq!(s.begin(1, 100, 10), Begin::InFlight);
    assert_eq!(s.begin(1, 999, 10), Begin::FingerprintMismatch);
    let Begin::Execute { attempt: b } = s.begin(1, 100, 30) else {
        panic!("an expired lock can be taken over")
    };
    assert!(
        !s.complete(1, a, 7, 31),
        "the zombie attempt cannot publish its response"
    );
    s.fail(1, a);
    assert_eq!(
        s.begin(1, 100, 32),
        Begin::InFlight,
        "the zombie cannot release the new owner's lock either"
    );
    assert!(s.complete(1, b, 8, 40));
    assert_eq!(s.begin(1, 100, 239), Begin::Replay { response: 8 });
    assert_eq!(s.begin(1, 999, 239), Begin::FingerprintMismatch);
    assert!(
        matches!(s.begin(1, 100, 240), Begin::Execute { .. }),
        "after retention the key is forgotten: a LATE retry runs again"
    );
}
/// Random clients retrying 20 keys with crashes. `atomic` = the effect commits inside `complete` (same transaction).
/// Returns (effects per key, number of crashes injected after the effect but before completion).
fn idempotency_chaos(seed: u64, atomic: bool) -> (Vec<u32>, u32) {
    let mut r = Rng(seed);
    let mut store = IdemStore::default();
    let mut effects = vec![0u32; 20];
    let mut running: Vec<(u64, u64, u8)> = vec![]; // (key, attempt, stage: 0 = started, 1 = effect done)
    let mut answers: HashMap<u64, u64> = HashMap::new();
    let (mut crashes_in_gap, mut now) = (0, 0);
    // retention far longer than the run, so only locks expire here
    while now < RETAIN_MS - 1 {
        now += u64::from(r.below(8) == 0);
        if r.below(2) == 0 || running.is_empty() {
            let key = r.below(20);
            match store.begin(key, mix(key), now) {
                Begin::Execute { attempt } => running.push((key, attempt, 0)),
                Begin::Replay { response } => assert_eq!(
                    response, answers[&key],
                    "every replay returns the first stored answer"
                ),
                Begin::InFlight => {}
                Begin::FingerprintMismatch => panic!("same key always carries the same body here"),
            }
            continue;
        }
        let i = r.below(running.len() as u64) as usize;
        let (key, attempt, stage) = running[i];
        if r.below(10) == 0 {
            crashes_in_gap += u32::from(stage == 1);
            running.swap_remove(i); // crash: no `fail`, no `complete`; the lock must expire
        } else if stage == 0 && !atomic {
            effects[key as usize] += 1; // effect in ANOTHER system, committed on its own
            running[i].2 = 1;
        } else {
            let response = mix(key) ^ attempt;
            if store.complete(key, attempt, response, now) {
                if atomic {
                    effects[key as usize] += 1;
                }
                assert!(
                    answers.insert(key, response).is_none(),
                    "a key completes once"
                );
            }
            running.swap_remove(i);
        }
    }
    (effects, crashes_in_gap)
}
fn idempotency_test() {
    idempotency_basics();
    let (mut dup_separate, mut gap_crashes, mut done_atomic) = (0, 0, 0);
    for seed in 0..1000 {
        let (effects, crashes) = idempotency_chaos(seed, false);
        dup_separate += effects.iter().filter(|&&e| e > 1).count();
        gap_crashes += crashes;
        let (effects, _) = idempotency_chaos(seed, true);
        assert!(
            effects.iter().all(|&e| e <= 1),
            "atomic effect + key: never twice (seed {seed})"
        );
        done_atomic += effects.iter().filter(|&&e| e == 1).count();
    }
    assert!(dup_separate > 0 && gap_crashes > 0 && done_atomic > 10_000);
    // real threads: 8 clients race the same keys; the store mutex is the "transaction"
    let shared = Arc::new(Mutex::new((IdemStore::default(), vec![0u32; 200])));
    let barrier = Arc::new(Barrier::new(8));
    let hs: Vec<_> = (0..8)
        .map(|_| {
            let (shared, barrier) = (shared.clone(), barrier.clone());
            thread::spawn(move || {
                barrier.wait();
                for key in 0..200u64 {
                    loop {
                        let begin = shared.lock().unwrap().0.begin(key, key, 0); // time frozen: locks never expire
                        match begin {
                            Begin::Execute { attempt } => {
                                thread::yield_now();
                                let mut g = shared.lock().unwrap();
                                assert!(g.0.complete(key, attempt, key * 2, 0));
                                g.1[key as usize] += 1;
                            }
                            Begin::Replay { response } => {
                                assert_eq!(response, key * 2);
                                break;
                            }
                            Begin::InFlight => thread::yield_now(),
                            Begin::FingerprintMismatch => panic!(),
                        }
                    }
                }
            })
        })
        .collect();
    hs.into_iter().for_each(|h| h.join().unwrap());
    assert!(shared.lock().unwrap().1.iter().all(|&e| e == 1));
    println!(
        "  separate effect: {dup_separate} keys executed twice or more ({gap_crashes} crashes landed in the gap); atomic effect: 0 duplicates in 1000 schedules"
    );
    println!("  8 threads x 200 keys: every key executed exactly once, all replays identical");
}

// ===========================================================================
// B. consumer deduplication
// ===========================================================================
#[derive(Debug, PartialEq, Clone, Copy)]
enum Seen {
    New,
    Duplicate,
    TooOld,
}
const WINDOW: u64 = 64;
/// Sliding anti-replay window: bit i of `bits` means "high - i was accepted". 16 bytes per producer.
#[derive(Default)]
struct Window {
    high: u64,
    bits: u64,
}
impl Window {
    /// Sequence numbers start at 1.
    fn accept(&mut self, seq: u64) -> Seen {
        assert!(seq > 0);
        if seq > self.high {
            let shift = seq - self.high;
            self.bits = if shift >= WINDOW {
                1
            } else {
                (self.bits << shift) | 1
            };
            self.high = seq;
            return Seen::New;
        }
        let back = self.high - seq;
        if back >= WINDOW {
            return Seen::TooOld;
        }
        if self.bits & (1 << back) != 0 {
            return Seen::Duplicate;
        }
        self.bits |= 1 << back;
        Seen::New
    }
}
fn window_test() {
    let mut r = Rng(10);
    let mut tallies = [0u32; 3];
    for _ in 0..2000 {
        let (mut w, mut accepted, mut high) = (Window::default(), HashSet::new(), 0u64);
        let mut next = 1u64;
        for _ in 0..300 {
            let seq = match r.below(10) {
                0 => next.saturating_sub(r.below(150)).max(1), // replay, maybe ancient
                1 => {
                    next += r.below(100); // a jump bigger than the window
                    next
                }
                2..=4 => next.saturating_sub(r.below(8)).max(1), // local reordering
                _ => {
                    next += 1;
                    next
                }
            };
            // oracle: unlimited memory, with the age rule stated directly
            let want = if seq + WINDOW <= high {
                Seen::TooOld
            } else if accepted.contains(&seq) {
                Seen::Duplicate
            } else {
                Seen::New
            };
            assert_eq!(w.accept(seq), want, "seq {seq} high {high}");
            if want == Seen::New {
                accepted.insert(seq);
                high = high.max(seq);
            }
            tallies[want as usize] += 1;
        }
    }
    assert!(
        tallies.iter().all(|&t| t > 1000),
        "all three verdicts exercised: {tallies:?}"
    );
    println!(
        "  sliding window == unlimited-memory oracle on 600000 arrivals (new/dup/too-old {tallies:?})"
    );
}
struct Bloom(Vec<u64>);
impl Bloom {
    fn new(bits: usize) -> Self {
        Bloom(vec![0; bits.div_ceil(64)])
    }
    fn positions(&self, x: u64) -> impl Iterator<Item = usize> {
        let n = self.0.len() * 64;
        let (a, b) = (mix(x), mix(x ^ 0xABCD) | 1);
        (0..4u64).map(move |i| (a.wrapping_add(i.wrapping_mul(b)) % n as u64) as usize)
    }
    fn insert(&mut self, x: u64) {
        for p in self.positions(x).collect::<Vec<_>>() {
            self.0[p / 64] |= 1 << (p % 64);
        }
    }
    fn maybe(&self, x: u64) -> bool {
        self.positions(x)
            .all(|p| self.0[p / 64] & (1 << (p % 64)) != 0)
    }
}
/// Time-bucketed dedup: remembers ids for at least `span` and at most 2 x `span` time units.
/// The exact sets stand for the expensive store; each generation has a Bloom filter in front.
struct Generations {
    span: u64,
    epoch: u64,
    gens: VecDeque<(Bloom, HashSet<u64>)>, // front = current
    exact_lookups: u64,
}
impl Generations {
    fn new(span: u64) -> Self {
        Generations {
            span,
            epoch: 0,
            gens: VecDeque::from([(Bloom::new(1 << 13), HashSet::new())]),
            exact_lookups: 0,
        }
    }
    /// `now` must not decrease.
    fn first_time(&mut self, id: u64, now: u64) -> bool {
        let epoch = now / self.span;
        for _ in 0..(epoch - self.epoch).min(2) {
            self.gens.push_front((Bloom::new(1 << 13), HashSet::new()));
            self.gens.truncate(2);
        }
        self.epoch = epoch;
        for (bloom, exact) in &self.gens {
            if bloom.maybe(id) {
                self.exact_lookups += 1;
                if exact.contains(&id) {
                    return false;
                }
            }
        }
        let (bloom, exact) = &mut self.gens[0];
        bloom.insert(id);
        exact.insert(id);
        true
    }
}
fn generations_test() {
    let mut r = Rng(11);
    let mut d = Generations::new(100);
    let mut first_seen: HashMap<u64, u64> = HashMap::new(); // id -> time it was last treated as new
    let (mut now, mut arrivals, mut dups, mut bloom_only_drops) = (0u64, 0u64, 0u64, 0u64);
    let mut recent: Vec<u64> = vec![];
    let mut naive = Bloom::new(1 << 13); // the WRONG design: drop whenever the filter says "maybe"
    for step in 0..50_000u64 {
        now += u64::from(r.below(10) == 0) + if r.below(5000) == 0 { 350 } else { 0 };
        let id = if r.below(4) == 0 && !recent.is_empty() {
            recent[r.below(recent.len() as u64) as usize]
        } else {
            mix(step)
        };
        recent.push(id);
        if recent.len() > 3000 {
            recent.remove(0);
        }
        let fresh = d.first_time(id, now);
        match first_seen.get(&id) {
            Some(&t) if now - t < 100 => assert!(
                !fresh,
                "a duplicate within the guaranteed window was accepted"
            ),
            Some(&t) if now / 100 >= t / 100 + 2 => {
                assert!(fresh, "ids older than two generations are forgotten")
            }
            None => {
                assert!(
                    fresh,
                    "a never-seen id must NEVER be dropped, whatever the Bloom filter says"
                );
                bloom_only_drops += u64::from(naive.maybe(id));
            }
            _ => {}
        }
        if fresh {
            first_seen.insert(id, now);
        } else {
            dups += 1;
        }
        naive.insert(id);
        arrivals += 1;
    }
    assert!(dups > 1000 && bloom_only_drops > 1000);
    assert!(
        d.exact_lookups < arrivals,
        "the filter spared exact lookups: {} for {arrivals}",
        d.exact_lookups
    );
    println!(
        "  {arrivals} arrivals: {dups} duplicates caught, {} exact lookups; dropping on a (never rotated) Bloom hit would have lost {bloom_only_drops} NEW messages",
        d.exact_lookups
    );
}

// ===========================================================================
// C + D. CAS lease store, leader election, fenced resource
// ===========================================================================
#[derive(Clone, Copy, Debug, PartialEq, Default)]
struct LeaseRecord {
    holder: u32,
    expires_at: u64, // in STORE time
    token: u64,
}
/// A linearisable register. Each method is one atomic step.
#[derive(Default)]
struct CasStore {
    record: LeaseRecord,
    revision: u64,
}
impl CasStore {
    fn read(&self) -> (LeaseRecord, u64) {
        (self.record, self.revision)
    }
    fn cas(&mut self, expected_revision: u64, new: LeaseRecord) -> bool {
        let ok = self.revision == expected_revision;
        if ok {
            (self.record, self.revision) = (new, self.revision + 1);
        }
        ok
    }
}
const TTL: u64 = 20;
/// The election is written only with read + CAS: two candidates that read the same revision cannot both win.
fn try_acquire(
    store: &mut CasStore,
    seen: (LeaseRecord, u64),
    node: u32,
    store_now: u64,
) -> Option<u64> {
    let (rec, revision) = seen;
    if rec.token != 0 && rec.expires_at > store_now {
        return None;
    }
    let new = LeaseRecord {
        holder: node,
        expires_at: store_now + TTL,
        token: rec.token + 1,
    };
    store.cas(revision, new).then_some(new.token)
}
fn election_threads() {
    // 8 real threads read the same state, then race the CAS; repeat over 200 terms
    let store = Arc::new(Mutex::new(CasStore::default()));
    let (read_done, term_done) = (Arc::new(Barrier::new(8)), Arc::new(Barrier::new(8)));
    let hs: Vec<_> = (0..8u32)
        .map(|node| {
            let (store, read_done, term_done) =
                (store.clone(), read_done.clone(), term_done.clone());
            thread::spawn(move || {
                let mut won = vec![];
                for term in 0..200u64 {
                    let now = term * (TTL + 1);
                    let seen = store.lock().unwrap().read();
                    read_done.wait();
                    if let Some(token) = try_acquire(&mut store.lock().unwrap(), seen, node, now) {
                        won.push(token);
                    }
                    term_done.wait();
                }
                won
            })
        })
        .collect();
    let mut tokens: Vec<u64> = hs.into_iter().flat_map(|h| h.join().unwrap()).collect();
    tokens.sort_unstable();
    assert_eq!(
        tokens,
        (1..=200).collect::<Vec<_>>(),
        "exactly one winner per term, tokens strictly increasing"
    );
    println!("  CAS election: 8 threads, 200 terms, exactly one winner each, tokens 1..=200");
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Fencing {
    None,
    CheckOnWrite,
    FenceThenRead,
}
#[derive(Default)]
struct Resource {
    value: u64, // a counter that leaders increment by read-modify-write
    highest: u64,
}
impl Resource {
    /// Publish a token without writing: from now on older leaders are rejected.
    fn fence(&mut self, token: u64) -> bool {
        let ok = token >= self.highest;
        if ok {
            self.highest = token;
        }
        ok
    }
}
#[derive(Clone, Copy)]
enum Message {
    Fence { token: u64 },
    Read { token: u64 },
    Write { token: u64, new_value: u64 },
}
#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Follower,
    NeedFence,
    NeedRead,
    HaveValue(u64),
    Waiting, // a request is on the wire
}
struct Node {
    phase: Phase,
    token: u64,
    clock_rate_percent: u64, // 100 = correct; 50 = this clock runs at half speed, so the node thinks less time passed
    lease_started_local: u64,
    seen: Option<(LeaseRecord, u64)>,
}
#[derive(Default, Debug)]
struct Tally {
    stale_writes: u64, // accepted although a NEWER leader had already fenced or written
    lost_updates: u64, // accepted write did not equal previous value + 1
    accepted: u64,
    rejected: u64,
    terms: u64,
    writes_after_expiry: u64, // sent while the store already considered the lease over: the client-side check failed
}
fn chaos(seed: u64, mode: Fencing) -> Tally {
    let mut r = Rng(seed);
    let (mut store, mut res, mut tally) =
        (CasStore::default(), Resource::default(), Tally::default());
    let mut nodes: Vec<Node> = (0..3)
        .map(|_| Node {
            phase: Phase::Follower,
            token: 0,
            clock_rate_percent: 50 + r.below(101),
            lease_started_local: 0,
            seen: None,
        })
        .collect();
    let mut wire: Vec<(usize, Message)> = vec![];
    let mut now = 0u64; // store (true) time
    let mut newest_at_resource = 0u64; // oracle's own record of the newest token that reached the resource
    for _ in 0..1500 {
        match r.below(20) {
            0..=6 => now += 1,
            7 if r.below(4) == 0 => now += r.below(3 * TTL), // a long stall in which nobody is scheduled
            // deliver a random in-flight message: delay and reordering
            8..=11 if !wire.is_empty() => {
                let (n, message) = wire.swap_remove(r.below(wire.len() as u64) as usize);
                let current = nodes[n].phase == Phase::Waiting;
                match message {
                    Message::Fence { token } => {
                        let ok = res.fence(token);
                        newest_at_resource = newest_at_resource.max(token);
                        if current && nodes[n].token == token {
                            nodes[n].phase = if ok { Phase::NeedRead } else { Phase::Follower };
                        }
                    }
                    Message::Read { token } => {
                        let allowed = mode != Fencing::FenceThenRead || token >= res.highest;
                        if current && nodes[n].token == token {
                            nodes[n].phase = if allowed {
                                Phase::HaveValue(res.value)
                            } else {
                                Phase::Follower
                            };
                        }
                    }
                    Message::Write { token, new_value } => {
                        if mode != Fencing::None && token < res.highest {
                            tally.rejected += 1;
                            if current && nodes[n].token == token {
                                nodes[n].phase = Phase::Follower;
                            }
                        } else {
                            tally.stale_writes += u64::from(token < newest_at_resource);
                            tally.lost_updates += u64::from(new_value != res.value + 1);
                            tally.accepted += 1;
                            res.value = new_value;
                            res.highest = res.highest.max(token);
                            newest_at_resource = newest_at_resource.max(token);
                            if current && nodes[n].token == token {
                                nodes[n].phase = Phase::NeedRead;
                            }
                        }
                    }
                }
            }
            // advance one node by ONE step; a node that is not picked is paused
            _ => {
                let n = r.below(3) as usize;
                let local_now = now * nodes[n].clock_rate_percent / 100;
                // the client-side lease check: by the node's own clock, with a safety margin
                let believes_leader = nodes[n].phase != Phase::Follower
                    && local_now.saturating_sub(nodes[n].lease_started_local) < TTL - 4;
                if nodes[n].phase == Phase::Follower
                    || (!believes_leader && nodes[n].phase != Phase::Waiting)
                {
                    nodes[n].phase = Phase::Follower;
                    match nodes[n].seen.take() {
                        None => {
                            nodes[n].seen = Some(store.read());
                        }
                        Some(seen) => {
                            if let Some(token) = try_acquire(&mut store, seen, n as u32, now) {
                                tally.terms += 1;
                                nodes[n].token = token;
                                nodes[n].lease_started_local = local_now;
                                nodes[n].phase = if mode == Fencing::FenceThenRead {
                                    Phase::NeedFence
                                } else {
                                    Phase::NeedRead
                                };
                            }
                        }
                    }
                    continue;
                }
                let token = nodes[n].token;
                let message = match nodes[n].phase {
                    Phase::NeedFence => Message::Fence { token },
                    Phase::NeedRead => Message::Read { token },
                    Phase::HaveValue(v) => {
                        tally.writes_after_expiry += u64::from(
                            store.record.token != token || store.record.expires_at <= now,
                        );
                        Message::Write {
                            token,
                            new_value: v + 1,
                        }
                    }
                    Phase::Waiting => {
                        // give up on a request after the lease is gone; the message may STILL arrive later
                        if !believes_leader {
                            nodes[n].phase = Phase::Follower;
                        }
                        continue;
                    }
                    Phase::Follower => unreachable!(),
                };
                wire.push((n, message));
                nodes[n].phase = Phase::Waiting;
            }
        }
    }
    if mode == Fencing::FenceThenRead {
        assert_eq!(
            res.value, tally.accepted,
            "the counter equals the number of accepted increments"
        );
    }
    tally
}
fn chaos_test() {
    println!(
        "  {:<18}{:>8}{:>10}{:>10}{:>14}{:>14}{:>16}",
        "resource",
        "terms",
        "accepted",
        "rejected",
        "stale writes",
        "lost updates",
        "sent too late"
    );
    let mut totals = vec![];
    for mode in [Fencing::None, Fencing::CheckOnWrite, Fencing::FenceThenRead] {
        let mut sum = Tally::default();
        let mut bad_schedules = 0;
        for seed in 0..2000 {
            let t = chaos(seed, mode);
            bad_schedules += u32::from(t.stale_writes + t.lost_updates > 0);
            sum.terms += t.terms;
            sum.accepted += t.accepted;
            sum.rejected += t.rejected;
            sum.stale_writes += t.stale_writes;
            sum.lost_updates += t.lost_updates;
            sum.writes_after_expiry += t.writes_after_expiry;
        }
        println!(
            "  {:<18}{:>8}{:>10}{:>10}{:>14}{:>14}{:>16}",
            format!("{mode:?}"),
            sum.terms,
            sum.accepted,
            sum.rejected,
            sum.stale_writes,
            sum.lost_updates,
            sum.writes_after_expiry
        );
        totals.push((sum, bad_schedules));
    }
    let (none, check, full) = (&totals[0], &totals[1], &totals[2]);
    assert!(
        totals.iter().all(|t| t.0.writes_after_expiry > 100),
        "every mode really faced leaders acting after expiry"
    );
    assert!(none.0.stale_writes > 0 && none.0.lost_updates > 0);
    assert_eq!(
        check.0.stale_writes, 0,
        "a token check stops writes older than one ALREADY SEEN"
    );
    assert!(
        check.0.lost_updates > 0,
        "but an old write can slip in between the new leader's read and its first write"
    );
    assert_eq!(
        (full.0.stale_writes, full.0.lost_updates, full.1),
        (0, 0, 0),
        "fence, then read: 2000 schedules, nothing stale accepted"
    );
    assert!(
        full.0.accepted > 20_000 && full.0.rejected > 0,
        "and the system still made progress"
    );
    println!(
        "  schedules with any violation: none={}, check-only={}, fence-then-read={}",
        none.1, check.1, full.1
    );
}

fn main() {
    println!("== A. idempotency keys ==");
    idempotency_test();
    println!("== B. consumer dedup ==");
    window_test();
    generations_test();
    println!("== C. leader election by compare-and-set ==");
    election_threads();
    println!(
        "== D. fenced resource under pauses, clock skew and delayed messages (2000 schedules each) =="
    );
    chaos_test();
    println!(
        "all checks passed (not simulated: one message delivered twice, which a token check alone does not stop)"
    );
}
