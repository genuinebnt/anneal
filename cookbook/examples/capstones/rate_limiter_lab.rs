//! CAPSTONE 04 (rate-limiting): a deterministic laboratory comparing six rate-limit algorithms
//! LEVEL: capstone
//! Use in: API gateways, per-tenant quotas, choosing between limiter algorithms.   Run: cargo run --release --example rate_limiter_lab
#![allow(dead_code)]
//!
//! WHAT: six algorithms behind one `Limiter` trait (fixed window, sliding log, sliding counter, token bucket,
//! leaky-bucket meter, GCRA), a per-key `KeyedLimiter` with idle eviction, and a lock-free concurrency limiter
//! with an AIMD (additive-increase / multiplicative-decrease) adaptive limit; its in-flight count and limit share one atomic
//! word, so admission checks the limit that is current at the moment it takes the slot.
//! WHY: every limiter trades accuracy, burst behaviour and memory differently. Seeing them run on identical
//! seeded traffic makes the trade-offs concrete instead of folklore.
//! HOW: time is never read from the OS. Each `check` takes `now_ms`, so a replay is exactly reproducible and the
//! whole lab runs in milliseconds. The harness measures the metric that matters to a protected backend: the
//! maximum number of ADMITTED requests in any 1-second sliding window.
//! CLOCKS: `now_ms` may go backwards. The five limiters that measure elapsed time keep the latest time they have seen and
//! treat an earlier one as that, so a rollback never refills or resets anything. GCRA compares the earlier time with its
//! stored schedule, which can only make it stricter, so it matches the token bucket only while time does not go back.
//!
//! COMMON USES: API gateway quotas (Stripe-style), login brute-force throttling, per-tenant fairness in multi-tenant
//! SaaS, outbound politeness toward third-party APIs, tower/governor middleware, database connection or in-flight
//! request caps, adaptive concurrency control (TCP-Vegas / Netflix-style) to shed load before collapse.
//! Read: https://stripe.com/blog/rate-limiters, https://github.com/boinkor-net/governor,
//! https://docs.rs/tower/latest/tower/limit/index.html
use std::collections::{HashMap, VecDeque};
use std::mem::size_of;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::thread;

const LIMIT: u32 = 10; // requests per window / burst size
const WINDOW_MS: u64 = 1000;
const INTERVAL_MS: u64 = WINDOW_MS / LIMIT as u64; // steady-state spacing: one request per 100 ms

/// Every algorithm answers one question: may a request arriving at `now_ms` proceed?
/// Time is an argument (not a global clock) so tests are deterministic. An earlier time than before must not restore quota.
trait Limiter {
    fn name(&self) -> &'static str;
    fn check(&mut self, now_ms: u64) -> bool;
    /// How long after its last check this limiter behaves exactly like a new one: the shortest safe idle eviction.
    fn recovery_ms(&self) -> u64;
    /// Bytes of the state that changes per key, counted the same way for every algorithm: field sizes without padding,
    /// plus heap for the log. Settings (limit, window, interval, tolerance) are left out because keys can share them; the
    /// structs in this lab store them in each instance anyway, so a `size_of` here would be larger.
    fn state_bytes(&self) -> usize;
}

// ---- 1. fixed window: a counter that resets at window edges. O(1) memory, but allows 2x at the boundary ----
struct FixedWindow {
    limit: u32,
    window_ms: u64,
    last_ms: u64, // latest time seen; its window is the one `count` belongs to
    count: u32,
}
impl FixedWindow {
    fn new(limit: u32, window_ms: u64) -> Self {
        Self {
            limit,
            window_ms,
            last_ms: 0,
            count: 0,
        }
    }
}
impl Limiter for FixedWindow {
    fn name(&self) -> &'static str {
        "fixed-window"
    }
    fn check(&mut self, now_ms: u64) -> bool {
        let now_ms = now_ms.max(self.last_ms);
        if now_ms / self.window_ms != self.last_ms / self.window_ms {
            self.count = 0;
        }
        self.last_ms = now_ms;
        let ok = self.count < self.limit;
        self.count += ok as u32;
        ok
    }
    fn recovery_ms(&self) -> u64 {
        self.window_ms
    }
    fn state_bytes(&self) -> usize {
        size_of_val(&self.last_ms) + size_of_val(&self.count)
    }
}

// ---- 2. sliding window log: remember every admitted timestamp. Exact, but O(limit) memory per key ----
struct SlidingLog {
    limit: usize,
    window_ms: u64,
    log: VecDeque<u64>,
}
impl SlidingLog {
    fn new(limit: u32, window_ms: u64) -> Self {
        Self {
            limit: limit as usize,
            window_ms,
            log: VecDeque::with_capacity(limit as usize),
        }
    }
}
impl Limiter for SlidingLog {
    fn name(&self) -> &'static str {
        "sliding-log"
    }
    // No clamp needed for rollback: a stamp taken at an earlier time sits behind the newer front entry, and only the front
    // is popped, so it stays at least as long as that newer one (the self-check compares this with a clamped clock).
    fn check(&mut self, now_ms: u64) -> bool {
        // Keep only timestamps inside (now - window, now]; this matches the half-open window the harness measures.
        while self
            .log
            .front()
            .is_some_and(|&t| t + self.window_ms <= now_ms)
        {
            self.log.pop_front();
        }
        let ok = self.log.len() < self.limit;
        if ok {
            self.log.push_back(now_ms);
        }
        ok
    }
    fn recovery_ms(&self) -> u64 {
        self.window_ms
    }
    fn state_bytes(&self) -> usize {
        size_of_val(&self.log) + self.log.capacity() * size_of::<u64>()
    }
}

// ---- 3. sliding window counter: estimate = prev * (fraction of prev still in view) + cur. O(1), approximate ----
struct SlidingCounter {
    limit: f64,
    window_ms: u64,
    last_ms: u64,
    cur: u32,
    prev: u32,
}
impl SlidingCounter {
    fn new(limit: u32, window_ms: u64) -> Self {
        Self {
            limit: limit as f64,
            window_ms,
            last_ms: 0,
            cur: 0,
            prev: 0,
        }
    }
    /// `now_ms` is never before `last_ms`.
    fn roll(&mut self, now_ms: u64) {
        match now_ms / self.window_ms - self.last_ms / self.window_ms {
            0 => {}
            1 => (self.prev, self.cur) = (self.cur, 0),
            _ => (self.prev, self.cur) = (0, 0), // a whole idle window passed: nothing is left to weigh
        }
        self.last_ms = now_ms;
    }
}
impl Limiter for SlidingCounter {
    fn name(&self) -> &'static str {
        "sliding-counter"
    }
    fn check(&mut self, now_ms: u64) -> bool {
        let now_ms = now_ms.max(self.last_ms);
        self.roll(now_ms);
        let elapsed = (now_ms % self.window_ms) as f64 / self.window_ms as f64;
        let estimate = self.prev as f64 * (1.0 - elapsed) + self.cur as f64;
        let ok = estimate + 1.0 <= self.limit;
        self.cur += ok as u32;
        ok
    }
    fn recovery_ms(&self) -> u64 {
        2 * self.window_ms // after only one window the old count is still weighed as `prev`
    }
    fn state_bytes(&self) -> usize {
        size_of_val(&self.last_ms) + size_of_val(&self.cur) + size_of_val(&self.prev)
    }
}

// ---- 4. token bucket (integer "credit" in ms: one token costs INTERVAL ms, credit refills 1 per ms) ----
// Integers avoid float drift, which is what lets us prove exact equivalence with GCRA below.
struct TokenBucket {
    cap: u64,
    cost: u64,
    credit: u64,
    last_ms: u64,
}
impl TokenBucket {
    fn new(burst: u32, interval_ms: u64) -> Self {
        let cap = burst as u64 * interval_ms;
        Self {
            cap,
            cost: interval_ms,
            credit: cap,
            last_ms: 0,
        }
    }
}
impl Limiter for TokenBucket {
    fn name(&self) -> &'static str {
        "token-bucket"
    }
    fn check(&mut self, now_ms: u64) -> bool {
        let now_ms = now_ms.max(self.last_ms); // never move `last_ms` back: that time would be credited twice
        self.credit = (self.credit + (now_ms - self.last_ms)).min(self.cap);
        self.last_ms = now_ms;
        let ok = self.credit >= self.cost;
        if ok {
            self.credit -= self.cost;
        }
        ok
    }
    fn recovery_ms(&self) -> u64 {
        self.cap // empty to full
    }
    fn state_bytes(&self) -> usize {
        size_of_val(&self.credit) + size_of_val(&self.last_ms)
    }
}

// ---- 5. leaky bucket as a meter: a water level that drains at a constant rate; requests pour in `cost` ----
// It is the mirror image of the token bucket (level = cap - credit), so it makes the same decisions.
struct LeakyMeter {
    cap: u64,
    cost: u64,
    level: u64,
    last_ms: u64,
}
impl LeakyMeter {
    fn new(burst: u32, interval_ms: u64) -> Self {
        Self {
            cap: burst as u64 * interval_ms,
            cost: interval_ms,
            level: 0,
            last_ms: 0,
        }
    }
}
impl Limiter for LeakyMeter {
    fn name(&self) -> &'static str {
        "leaky-meter"
    }
    fn check(&mut self, now_ms: u64) -> bool {
        let now_ms = now_ms.max(self.last_ms);
        self.level = self.level.saturating_sub(now_ms - self.last_ms);
        self.last_ms = now_ms;
        let ok = self.level + self.cost <= self.cap;
        if ok {
            self.level += self.cost;
        }
        ok
    }
    fn recovery_ms(&self) -> u64 {
        self.cap // full to drained
    }
    fn state_bytes(&self) -> usize {
        size_of_val(&self.level) + size_of_val(&self.last_ms)
    }
}

// ---- 6. GCRA: store ONE number, the theoretical arrival time (TAT) of the next conforming request ----
struct Gcra {
    interval: u64, // T: emission interval
    // tau: how far ahead of schedule a request may arrive = (burst - 1) * T; `None` for burst 0, which admits nothing
    tolerance: Option<u64>,
    tat: u64,
}
impl Gcra {
    fn new(burst: u32, interval_ms: u64) -> Self {
        Self {
            interval: interval_ms,
            tolerance: (burst as u64).checked_sub(1).map(|b| b * interval_ms),
            tat: 0,
        }
    }
}
impl Limiter for Gcra {
    fn name(&self) -> &'static str {
        "gcra"
    }
    fn check(&mut self, now_ms: u64) -> bool {
        let ok = self.tolerance.is_some_and(|tau| self.tat <= now_ms + tau);
        if ok {
            self.tat = self.tat.max(now_ms) + self.interval;
        }
        ok
    }
    fn recovery_ms(&self) -> u64 {
        // an admit leaves `tat` at most tau + T ahead of the time it happened
        self.tolerance.map_or(0, |tau| tau + self.interval)
    }
    fn state_bytes(&self) -> usize {
        size_of_val(&self.tat)
    }
}

fn lineup() -> Vec<Box<dyn Limiter>> {
    vec![
        Box::new(FixedWindow::new(LIMIT, WINDOW_MS)),
        Box::new(SlidingLog::new(LIMIT, WINDOW_MS)),
        Box::new(SlidingCounter::new(LIMIT, WINDOW_MS)),
        Box::new(TokenBucket::new(LIMIT, INTERVAL_MS)),
        Box::new(LeakyMeter::new(LIMIT, INTERVAL_MS)),
        Box::new(Gcra::new(LIMIT, INTERVAL_MS)),
    ]
}

// ---- per-key wrapper: one limiter per tenant, idle keys swept so the map cannot grow without bound ----
struct KeyedLimiter<L, F: Fn() -> L> {
    make: F,
    idle_ms: u64,
    clock_ms: u64, // latest time seen from any key; everything below runs on it, so it never goes back
    last_sweep: u64,
    entries: HashMap<u32, (L, u64)>, // limiter + last-seen time
}
impl<L: Limiter, F: Fn() -> L> KeyedLimiter<L, F> {
    fn new(make: F, idle_ms: u64) -> Self {
        Self {
            make,
            idle_ms,
            clock_ms: 0,
            last_sweep: 0,
            entries: HashMap::new(),
        }
    }
    fn check(&mut self, key: u32, now_ms: u64) -> bool {
        self.clock_ms = self.clock_ms.max(now_ms);
        let now_ms = self.clock_ms;
        self.sweep_if_due(now_ms);
        let (limiter, seen) = self
            .entries
            .entry(key)
            .or_insert_with(|| ((self.make)(), now_ms));
        *seen = now_ms;
        limiter.check(now_ms)
    }
    /// Dropping a key forgets its state, which is safe only once a fresh limiter would behave the same. So a key is kept
    /// for `idle_ms` or the limiter's `recovery_ms`, whichever is longer: a short `idle_ms` cannot hand quota back.
    fn sweep_if_due(&mut self, now_ms: u64) {
        if now_ms - self.last_sweep >= self.idle_ms {
            let idle = self.idle_ms;
            self.entries
                .retain(|_, (l, seen)| now_ms - *seen < idle.max(l.recovery_ms()));
            self.last_sweep = now_ms;
        }
    }
    fn len(&self) -> usize {
        self.entries.len()
    }
}

// ---- concurrency limiter: caps requests IN FLIGHT (not per time). RAII permit releases on Drop ----
struct Shared {
    /// limit in the high 32 bits, requests in flight in the low 32. Admission compares the two, so they must change as one:
    /// with two atomics the limit can drop between reading it and taking the slot.
    state: AtomicU64,
    min: usize,
    max: usize,
}
#[derive(Clone)]
struct ConcurrencyLimiter(Arc<Shared>);
struct Permit(Arc<Shared>);

fn pack(limit: usize, in_flight: usize) -> u64 {
    (limit as u64) << 32 | in_flight as u64
}
fn unpack(state: u64) -> (usize, usize) {
    ((state >> 32) as usize, (state & u32::MAX as u64) as usize)
}

impl ConcurrencyLimiter {
    fn new(limit: usize, min: usize, max: usize) -> Self {
        assert!(limit.max(max).max(min) <= u32::MAX as usize);
        Self(Arc::new(Shared {
            state: AtomicU64::new(pack(limit, 0)),
            min,
            max,
        }))
    }
    fn try_acquire(&self) -> Option<Permit> {
        self.try_acquire_observed(|| {})
    }
    /// `after_read` runs between reading the state and trying to change it: the self-check uses it to change the limit at
    /// exactly that point, without threads or timing.
    fn try_acquire_observed(&self, mut after_read: impl FnMut()) -> Option<Permit> {
        // fetch_update is a CAS loop. If anything changed since the read (a slot or the limit), it decides again.
        self.0
            .state
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |state| {
                after_read();
                let (limit, n) = unpack(state);
                (n < limit).then(|| pack(limit, n + 1))
            })
            .ok()?;
        Some(Permit(Arc::clone(&self.0)))
    }
    fn in_flight(&self) -> usize {
        unpack(self.0.state.load(Ordering::Acquire)).1
    }
    fn limit(&self) -> usize {
        unpack(self.0.state.load(Ordering::Acquire)).0
    }
    fn update_limit(&self, f: impl Fn(usize) -> usize) {
        let _ = self
            .0
            .state
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |state| {
                let (limit, n) = unpack(state);
                Some(pack(f(limit), n))
            });
    }
    /// AIMD: probe gently upwards (+1) on success...
    fn on_success(&self) {
        self.update_limit(|l| (l + 1).min(self.0.max));
    }
    /// ...but back off fast (halve) on overload, as TCP congestion control does.
    fn on_overload(&self) {
        self.update_limit(|l| (l / 2).max(self.0.min));
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        // the count is the low half and is at least 1 while a permit lives, so this never borrows from the limit
        self.0.state.fetch_sub(1, Ordering::Release); // runs on every exit path, including panics
    }
}

// ---- seeded traffic + simulation harness ----
struct XorShift(u64);
impl XorShift {
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

#[derive(Clone, Copy)]
struct Req {
    t: u64,
    key: u32,
}

fn sorted(mut v: Vec<Req>) -> Vec<Req> {
    v.sort_by_key(|r| r.t); // arrival order must be time order or limiters would see time go backwards
    v
}
/// 20 req/s with jitter: twice the limit, forever.
fn steady(rng: &mut XorShift) -> Vec<Req> {
    sorted(
        (0..200)
            .map(|k| Req {
                t: k * 50 + rng.below(40),
                key: 0,
            })
            .collect(),
    )
}
/// Every 2 s a 30-request spike inside 100 ms, silence in between.
fn bursty(rng: &mut XorShift) -> Vec<Req> {
    sorted(
        (0..10)
            .flat_map(|b| (0..30).map(move |_| b * 2000))
            .map(|base| Req {
                t: base + rng.below(100),
                key: 0,
            })
            .collect(),
    )
}
/// LIMIT requests just before a window edge and LIMIT just after it: the fixed window's worst case.
fn boundary() -> Vec<Req> {
    let edges = [1000u64, 3000, 5000];
    sorted(
        edges
            .iter()
            .flat_map(|&e| (0..LIMIT as u64).flat_map(move |i| [e - 10 + i, e + i]))
            .map(|t| Req { t, key: 0 })
            .collect(),
    )
}
/// Tenant 0 is a noisy neighbour (200 req/s); tenants 1..=10 are polite (about 4 req/s each) for 10 s.
fn tenants(rng: &mut XorShift) -> Vec<Req> {
    let mut v: Vec<Req> = (0..2000).map(|k| Req { t: k * 5, key: 0 }).collect();
    for key in 1..=10 {
        v.extend((0..40).map(|k| Req {
            t: k * 250 + rng.below(250),
            key,
        }));
    }
    sorted(v)
}

struct Stats {
    allowed: usize,
    denied: usize,
    max_window: usize,
    decisions: Vec<bool>,
}
/// Largest number of timestamps that fit in any half-open window [t, t + span): classic two-pointer sweep.
fn max_in_window(times: &[u64], span: u64) -> usize {
    let (mut lo, mut best) = (0, 0);
    for hi in 0..times.len() {
        while times[hi] - times[lo] >= span {
            lo += 1;
        }
        best = best.max(hi - lo + 1);
    }
    best
}
fn replay(traffic: &[Req], mut decide: impl FnMut(&Req) -> bool) -> Stats {
    let decisions: Vec<bool> = traffic.iter().map(&mut decide).collect();
    let admitted: Vec<u64> = traffic
        .iter()
        .zip(&decisions)
        .filter(|(_, ok)| **ok)
        .map(|(r, _)| r.t)
        .collect();
    Stats {
        allowed: admitted.len(),
        denied: decisions.len() - admitted.len(),
        max_window: max_in_window(&admitted, WINDOW_MS),
        decisions,
    }
}
fn run_global(l: &mut dyn Limiter, traffic: &[Req]) -> Stats {
    replay(traffic, |r| l.check(r.t))
}

fn print_pattern(title: &str, traffic: &[Req]) -> HashMap<&'static str, Stats> {
    println!(
        "\n== {title}: {} requests, limit {LIMIT}/s ==",
        traffic.len()
    );
    println!(
        "{:<16}{:>9}{:>9}{:>14}{:>12}",
        "limiter", "allowed", "denied", "max in 1s", "bytes/key"
    );
    let mut out = HashMap::new();
    for mut l in lineup() {
        let s = run_global(l.as_mut(), traffic);
        println!(
            "{:<16}{:>9}{:>9}{:>14}{:>12}",
            l.name(),
            s.allowed,
            s.denied,
            s.max_window,
            l.state_bytes()
        );
        out.insert(l.name(), s);
    }
    out
}

/// (allowed, total) for tenants matching `pick`.
fn tally(traffic: &[Req], decisions: &[bool], pick: impl Fn(u32) -> bool) -> (usize, usize) {
    let sel = traffic.iter().zip(decisions).filter(|(r, _)| pick(r.key));
    sel.fold((0, 0), |(a, n), (_, ok)| (a + *ok as usize, n + 1))
}

fn demo_concurrency() {
    // Real threads: the invariant must hold under any interleaving, so the assertion is deterministic even though timing is not.
    let cl = ConcurrencyLimiter::new(4, 1, 4);
    let (observed, peak) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let (cl, observed, peak) = (cl.clone(), Arc::clone(&observed), Arc::clone(&peak));
            thread::spawn(move || {
                let mut granted = 0usize;
                for _ in 0..5000 {
                    if let Some(_permit) = cl.try_acquire() {
                        granted += 1;
                        // `observed` is incremented after and decremented before the permit, so it never over-counts.
                        peak.fetch_max(
                            observed.fetch_add(1, Ordering::AcqRel) + 1,
                            Ordering::AcqRel,
                        );
                        thread::yield_now();
                        observed.fetch_sub(1, Ordering::AcqRel);
                    }
                }
                granted
            })
        })
        .collect();
    let granted: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();
    println!("\n== concurrency limiter: 8 threads x 5000 tries, max 4 in flight ==");
    println!(
        "granted {granted}, peak observed in flight {}, leaked permits {}",
        peak.load(Ordering::Acquire),
        cl.in_flight()
    );
    assert!(
        granted > 0 && peak.load(Ordering::Acquire) <= 4,
        "never more than max in flight"
    );
    assert_eq!(cl.in_flight(), 0, "Drop released every permit");

    // Deterministic AIMD: the backend can really serve `capacity` at once; load always exceeds it.
    let aimd = ConcurrencyLimiter::new(2, 1, 100);
    let mut limits = Vec::new();
    for round in 0..300 {
        let capacity = if round < 150 { 40 } else { 10 }; // the backend degrades halfway through
        let held: Vec<Permit> = std::iter::from_fn(|| aimd.try_acquire())
            .take(200)
            .collect();
        if held.len() > capacity {
            aimd.on_overload()
        } else {
            aimd.on_success()
        }
        limits.push(aimd.limit());
    }
    let (before, after) = (
        limits[100..150].iter().max().unwrap(),
        limits[200..].iter().max().unwrap(),
    );
    println!(
        "AIMD limit: peaked at {before} while backend capacity was 40, then {after} after it dropped to 10"
    );
    assert!(
        *before > 20 && *before <= 41,
        "AIMD probes up to just above capacity"
    );
    assert!(
        *after <= 11 && limits.iter().all(|&l| l >= 1),
        "AIMD backs off after the backend degrades"
    );
}

// ---- self-checks for the edges: a clock that goes back, burst 0, eviction, a limit change during admission ----
fn check_clock_rollback() {
    for mut l in lineup() {
        let name = l.name();
        let mut offer = |t| (0..20).filter(|_| l.check(t)).count();
        let seen = (offer(10_000), offer(500), offer(10_000));
        assert_eq!(
            seen,
            (LIMIT as usize, 0, 0),
            "{name}: rollback restored quota"
        );
    }
    // Stronger, for all but GCRA: a trace whose clock jumps back gets the same decisions as the same trace on a clock that
    // only moves forward (each time replaced by the latest so far).
    let mut rng = XorShift(0xC10C_C10C);
    for trace in 0..300 {
        let mut base = 5_000u64;
        let times: Vec<u64> = (0..300)
            .map(|_| {
                base += rng.below(60);
                if rng.below(8) == 0 {
                    base.saturating_sub(rng.below(4_000))
                } else {
                    base
                }
            })
            .collect();
        for (mut jumpy, mut steady) in lineup().into_iter().zip(lineup()) {
            let mut latest = 0;
            for &t in &times {
                latest = latest.max(t);
                let (got, clamped) = (jumpy.check(t), steady.check(latest));
                // GCRA may be stricter and then the two drift apart: only the quota bound below applies to it
                if jumpy.name() != "gcra" {
                    assert_eq!(
                        got,
                        clamped,
                        "{} trace {trace} at {t} (latest {latest})",
                        jumpy.name()
                    );
                }
            }
        }
        // GCRA: never more than the burst plus what the furthest time reached has earned
        let mut g = Gcra::new(LIMIT, INTERVAL_MS);
        let admitted = times.iter().filter(|&&t| g.check(t)).count() as u64;
        let span = times.iter().max().unwrap() - times[0];
        assert!(
            admitted <= LIMIT as u64 + span / INTERVAL_MS,
            "gcra trace {trace}"
        );
    }
    let mut keyed = KeyedLimiter::new(|| TokenBucket::new(LIMIT, INTERVAL_MS), 2000);
    let mut offer = |key, t| (0..20).filter(|_| keyed.check(key, t)).count();
    assert_eq!(offer(1, 10_000), LIMIT as usize);
    assert_eq!((offer(1, 500), offer(2, 500)), (0, LIMIT as usize));
    assert_eq!((offer(1, 10_000), offer(2, 10_000)), (0, 0));
}

fn check_burst_zero() {
    let mut none: [Box<dyn Limiter>; 6] = [
        Box::new(FixedWindow::new(0, WINDOW_MS)),
        Box::new(SlidingLog::new(0, WINDOW_MS)),
        Box::new(SlidingCounter::new(0, WINDOW_MS)),
        Box::new(TokenBucket::new(0, INTERVAL_MS)),
        Box::new(LeakyMeter::new(0, INTERVAL_MS)),
        Box::new(Gcra::new(0, INTERVAL_MS)),
    ];
    for l in &mut none {
        assert!(
            [0, 1, 5_000, 2].iter().all(|&t| !l.check(t)),
            "{}",
            l.name()
        );
    }
}

/// With an eviction time of 1 ms, a keyed limiter must still decide exactly like one that never forgets a key.
fn check_eviction<L: Limiter>(make: fn() -> L) {
    let mut rng = XorShift(0xE51C_7ED5);
    let mut keyed = KeyedLimiter::new(make, 1);
    let mut kept: HashMap<u32, L> = HashMap::new();
    let (mut t, mut evicted) = (0u64, false);
    for step in 0..20_000 {
        // mostly a burst at one instant, sometimes a gap on either side of each recovery time
        if rng.below(12) == 0 {
            t += [1, 99, 999, 1000, 1001, 1999, 2000, 2500][rng.below(8) as usize];
        }
        let key = rng.below(3) as u32;
        let want = kept.entry(key).or_insert_with(make).check(t);
        let got = keyed.check(key, t);
        assert_eq!(got, want, "{} step {step} key {key} at {t}", make().name());
        evicted |= keyed.len() < kept.len();
    }
    assert!(evicted, "the check must actually evict");
}

fn check_state_bytes() {
    let bytes: HashMap<_, _> = lineup()
        .iter()
        .map(|l| (l.name(), l.state_bytes()))
        .collect();
    assert_eq!(bytes["fixed-window"], 12); // time + count
    assert_eq!(bytes["sliding-counter"], 16); // time + two counts
    assert_eq!(bytes["token-bucket"], 16); // credit + time
    assert_eq!(bytes["leaky-meter"], 16); // level + time
    assert_eq!(bytes["gcra"], 8); // one time
    assert!(bytes["sliding-log"] >= size_of::<VecDeque<u64>>() + LIMIT as usize * 8);
}

/// The limit drops from 4 to 2 after an admission has read the state and before it takes the slot.
fn check_limit_change_during_acquire() {
    let cl = ConcurrencyLimiter::new(4, 1, 4);
    let held = [cl.try_acquire().unwrap(), cl.try_acquire().unwrap()];
    let mut lowered = false;
    let late = cl.try_acquire_observed(|| {
        if !std::mem::replace(&mut lowered, true) {
            cl.on_overload();
        }
    });
    assert!(
        late.is_none(),
        "admitted against a limit that was already lowered"
    );
    assert_eq!((cl.limit(), cl.in_flight()), (2, held.len()));
    drop(held);
    assert_eq!((cl.limit(), cl.in_flight()), (2, 0));
}

fn main() {
    check_clock_rollback();
    check_burst_zero();
    check_eviction(|| FixedWindow::new(LIMIT, WINDOW_MS));
    check_eviction(|| SlidingLog::new(LIMIT, WINDOW_MS));
    check_eviction(|| SlidingCounter::new(LIMIT, WINDOW_MS));
    check_eviction(|| TokenBucket::new(LIMIT, INTERVAL_MS));
    check_eviction(|| LeakyMeter::new(LIMIT, INTERVAL_MS));
    check_eviction(|| Gcra::new(LIMIT, INTERVAL_MS));
    check_state_bytes();
    check_limit_change_during_acquire();

    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    let steady_t = steady(&mut rng);
    let bursty_t = bursty(&mut rng);
    let boundary_t = boundary();

    let steady_r = print_pattern("steady 2x overload", &steady_t);
    let bursty_r = print_pattern("bursty (30-request spikes)", &bursty_t);
    let boundary_r = print_pattern("boundary-burst straddling window edges", &boundary_t);

    // 1. Fixed window: LIMIT just before the edge + LIMIT just after = 2x LIMIT in ~20 ms.
    assert_eq!(boundary_r["fixed-window"].max_window, 2 * LIMIT as usize);
    // 2. Sliding log is exact: never more than LIMIT in ANY sliding window, on every pattern.
    for r in [&steady_r, &bursty_r, &boundary_r] {
        assert!(r["sliding-log"].max_window <= LIMIT as usize);
    }
    assert_eq!(boundary_r["sliding-log"].max_window, LIMIT as usize);
    // The sliding counter is only an estimate, but it should stay far below the fixed window's 2x worst case.
    assert!(boundary_r["sliding-counter"].max_window < 2 * LIMIT as usize);

    // 3. Token bucket: a burst up to capacity, then exactly the steady rate.
    let mut tb = TokenBucket::new(LIMIT, INTERVAL_MS);
    let burst = (0..25).filter(|_| tb.check(0)).count();
    let steady_phase = (1..=40).filter(|k| tb.check(k * 50)).count(); // offered 20/s for 2 s
    println!(
        "\ntoken bucket: burst of 25 admitted {burst}; then 40 offers at 20/s admitted {steady_phase} (10/s)"
    );
    assert_eq!(burst, LIMIT as usize);
    assert_eq!(steady_phase, 20);

    // 4. GCRA == token bucket (and the leaky meter). GCRA's TAT is the time at which the bucket would be empty-to-full
    // again, so credit = cap - max(0, TAT - now). Allow-test and update are algebraically the same, and with
    // integer ms there is no float rounding to make them diverge.
    for (name, traffic, r) in [
        ("steady", &steady_t, &steady_r),
        ("bursty", &bursty_t, &bursty_r),
        ("boundary", &boundary_t, &boundary_r),
    ] {
        assert_eq!(
            r["gcra"].decisions, r["token-bucket"].decisions,
            "{name}: gcra vs token bucket"
        );
        assert_eq!(
            r["leaky-meter"].decisions, r["token-bucket"].decisions,
            "{name}: leaky vs token bucket"
        );
        assert_eq!(r["gcra"].decisions.len(), traffic.len());
    }

    // 5. Noisy neighbour: shared limiter starves quiet tenants; per-key limiting protects them.
    let tenant_t = tenants(&mut rng);
    let mut shared = TokenBucket::new(LIMIT, INTERVAL_MS);
    let shared_s = run_global(&mut shared, &tenant_t);
    let mut keyed = KeyedLimiter::new(|| TokenBucket::new(LIMIT, INTERVAL_MS), 2000);
    let keyed_s = replay(&tenant_t, |r| keyed.check(r.key, r.t));
    println!("\n== many tenants: noisy tenant 0 (200 req/s) + 10 quiet tenants (~4 req/s each) ==");
    println!(
        "{:<14}{:>16}{:>16}",
        "strategy", "quiet allowed", "noisy allowed"
    );
    for (label, s) in [("one shared", &shared_s), ("per-key", &keyed_s)] {
        let (qa, qn) = tally(&tenant_t, &s.decisions, |k| k != 0);
        let (na, nn) = tally(&tenant_t, &s.decisions, |k| k == 0);
        println!("{label:<14}{:>12}/{qn:<3}{:>12}/{nn}", qa, na);
    }
    let (shared_qa, quiet_n) = tally(&tenant_t, &shared_s.decisions, |k| k != 0);
    let (keyed_qa, _) = tally(&tenant_t, &keyed_s.decisions, |k| k != 0);
    let (noisy_a, _) = tally(&tenant_t, &keyed_s.decisions, |k| k == 0);
    assert!(
        shared_qa * 2 < quiet_n,
        "a shared bucket starves quiet tenants"
    );
    assert_eq!(
        keyed_qa, quiet_n,
        "per-key limiting never rejects a polite tenant"
    );
    assert!(
        noisy_a <= LIMIT as usize * 11,
        "the noisy tenant is held to its own quota (~10/s + burst)"
    );
    assert!(keyed.len() <= 11);

    // Idle eviction: 1000 one-shot keys vanish once they have been quiet longer than `idle_ms`.
    let mut churn = KeyedLimiter::new(|| Gcra::new(LIMIT, INTERVAL_MS), 2000);
    for k in 0..1000 {
        assert!(churn.check(k, k as u64));
    }
    let before = churn.len();
    churn.check(9999, 10_000);
    println!(
        "\nkey eviction: {before} keys tracked, {} after 2 s idle sweep",
        churn.len()
    );
    assert_eq!((before, churn.len()), (1000, 1));

    demo_concurrency();
    println!("\nall assertions passed");
}
