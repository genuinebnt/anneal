//! RECIPE 910 (microservices): API gateway and service discovery: routing, TTL registry, load balancing, circuit breaker, bulkhead, retry budget, deadlines, hedging
//! LEVEL: advanced
//! Use in: the policy layer of a gateway, sidecar or RPC client: deciding WHERE a request goes and WHEN to stop sending it.   Run: cargo run --release --example api_gateway_service_discovery
//! COMMON USES: Envoy (outlier detection, circuit breaking, retry budgets), Linkerd (P2C + latency, retry budgets), Consul/Eureka/etcd leases for registration, gRPC deadlines, hedged reads in large storage and search tiers.
#![allow(dead_code)]
//!
//! WHAT THIS IS: only the DECISIONS, on a VIRTUAL CLOCK (integer ms) with seeded latency and failure injection, so every threshold is
//! asserted exactly and every run is identical. For real sockets see `reverse_proxy_load_balancer` and `http_client_resilience`.
//! THE LAYERS, in the order a request meets them:
//!   1. ROUTE      : longest matching path prefix on SEGMENT boundaries (`/api/users` must not capture `/api/users-admin`).
//!   2. DEADLINE   : the client sends a remaining BUDGET in a header (`x-deadline-ms`); every hop subtracts the time it spent and forwards
//!                   the rest. A budget, not a wall-clock instant, so clocks need not agree. Nothing starts with an empty budget.
//!   3. DISCOVER   : instances register with a TTL lease and keep it alive by HEARTBEAT; active health checks eject and restore with
//!                   consecutive-result thresholds. A re-registration gets a new GENERATION, so a late message from the old process is ignored.
//!                   Both are needed: a heartbeat proves the process is alive, a probe proves it can serve.
//!   4. BALANCE    : round robin, least in-flight, POWER OF TWO CHOICES (pick 2 at random, take the less loaded), consistent hashing.
//!   5. PROTECT    : circuit BREAKER per instance (closed -> open -> half-open), BULKHEAD (cap in-flight per instance), RETRY BUDGET
//!                   (retries may add at most a fraction of the original traffic), HEDGING (for idempotent reads only).
//! BREAKER SUBTLETY: a call admitted while CLOSED may finish after the breaker opened and half-opened again. Every permit carries the
//! breaker's EPOCH; results from an older epoch are ignored, so a stale success cannot close a breaker.
//! RETRY ARITHMETIC: with 3 attempts at each of 3 layers a dead leaf receives 27x traffic. A budget bounds the multiplier at (1 + ratio)
//! per layer. P2C's point: random placement has maximum load ~ ln n / ln ln n above the mean; one extra sample makes it ~ ln ln n.
//! Read: https://sre.google/sre-book/handling-overload/ , https://sre.google/sre-book/addressing-cascading-failures/
//! Read: https://www.envoyproxy.io/docs/envoy/latest/intro/arch_overview/upstream/circuit_breaking
//! Read: Dean & Barroso, "The Tail at Scale" (CACM 2013, title only). Mitzenmacher, "The Power of Two Choices" (title only).
//!
//! TRY NEXT:
//!   - [S] Remove the epoch check in `Breaker::record`. Predict which assertion in `breaker_exact` fails, then run.
//!   - [S] Set the retry budget ratio to 100% in the outage run. Predict the attempts-per-request figure before running.
//!   - [M] Add slow-call detection: a success slower than a threshold counts as a failure. Show a brown-out that the error-only breaker misses.
//!   - [M] Make least-connections and P2C weighted (instances of different capacity) and extend the balls-into-bins test.
//!   - [A] PROJECT: put this policy layer in front of `axum` upstreams on loopback, with the registry in SQLite and the fencing store of `idempotency_dedup_leader_fencing_store` electing the health checker.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, VecDeque};

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

// ---------------------------------------------------------------------------
// 1. routing
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq)]
struct Route {
    method: Option<&'static str>,
    prefix: &'static str,
    service: &'static str,
}
#[derive(Debug, PartialEq)]
enum RouteError {
    NotFound,
    MethodNotAllowed,
}
fn segments(path: &str) -> Vec<&str> {
    path.split('?')
        .next()
        .unwrap()
        .split('/')
        .filter(|s| !s.is_empty())
        .collect()
}
#[derive(Default)]
struct Node {
    children: BTreeMap<String, Node>,
    routes: Vec<Route>,
}
#[derive(Default)]
struct Router(Node);
impl Router {
    fn add(&mut self, r: Route) {
        let mut n = &mut self.0;
        for s in segments(r.prefix) {
            n = n.children.entry(s.to_string()).or_default();
        }
        n.routes.push(r);
    }
    /// The DEEPEST node with routes wins; the method is chosen there. A more specific path with the wrong method is a 405, not a fall back.
    fn find(&self, method: &str, path: &str) -> Result<&Route, RouteError> {
        let mut n = &self.0;
        let mut best = (!n.routes.is_empty()).then_some(n);
        for s in segments(path) {
            let Some(next) = n.children.get(s) else { break };
            n = next;
            if !n.routes.is_empty() {
                best = Some(n);
            }
        }
        let routes = &best.ok_or(RouteError::NotFound)?.routes;
        let exact = routes.iter().find(|r| r.method == Some(method));
        exact
            .or_else(|| routes.iter().find(|r| r.method.is_none()))
            .ok_or(RouteError::MethodNotAllowed)
    }
}
/// Oracle: scan all routes.
fn route_naive<'a>(routes: &'a [Route], method: &str, path: &str) -> Result<&'a Route, RouteError> {
    let p = segments(path);
    let depth = routes
        .iter()
        .filter(|r| p.starts_with(&segments(r.prefix)))
        .map(|r| segments(r.prefix).len())
        .max()
        .ok_or(RouteError::NotFound)?;
    let at: Vec<&Route> = routes
        .iter()
        .filter(|r| segments(r.prefix).len() == depth && p.starts_with(&segments(r.prefix)))
        .collect();
    at.iter()
        .find(|r| r.method == Some(method))
        .or_else(|| at.iter().find(|r| r.method.is_none()))
        .copied()
        .ok_or(RouteError::MethodNotAllowed)
}
fn routing_test() {
    let table = [
        Route {
            method: None,
            prefix: "/api",
            service: "web",
        },
        Route {
            method: Some("GET"),
            prefix: "/api/users",
            service: "users-read",
        },
        Route {
            method: Some("POST"),
            prefix: "/api/users",
            service: "users-write",
        },
        Route {
            method: None,
            prefix: "/api/users/admin",
            service: "admin",
        },
        Route {
            method: Some("GET"),
            prefix: "/health",
            service: "gateway",
        },
    ];
    let mut router = Router::default();
    table.iter().cloned().for_each(|r| router.add(r));
    assert_eq!(
        router
            .find("GET", "/api/users/42?x=/health")
            .unwrap()
            .service,
        "users-read"
    );
    assert_eq!(
        router.find("GET", "/api/users-admin").unwrap().service,
        "web",
        "prefixes match whole segments only"
    );
    assert_eq!(
        router.find("DELETE", "/api/users/7"),
        Err(RouteError::MethodNotAllowed)
    );
    assert_eq!(router.find("GET", "/nope"), Err(RouteError::NotFound));
    let words = ["api", "users", "admin", "health", "users-admin", "7", ""];
    let mut r = Rng(1);
    for _ in 0..3000 {
        let path: String = (0..r.below(5))
            .map(|_| format!("/{}", words[r.below(words.len() as u64) as usize]))
            .collect();
        let method = ["GET", "POST", "PUT"][r.below(3) as usize];
        assert_eq!(
            router.find(method, &path),
            route_naive(&table, method, &path),
            "{method} {path}"
        );
    }
    println!("  3000 random paths: trie router == linear-scan oracle");
}

// ---------------------------------------------------------------------------
// 2. deadlines
// ---------------------------------------------------------------------------
const DEADLINE_HEADER: &str = "x-deadline-ms";
type Headers = BTreeMap<&'static str, String>;
/// Parse the incoming budget, cap it by our own maximum, subtract our own cost and what we keep to answer. `None` = do not call.
fn forward_budget(
    incoming: &Headers,
    max_ms: u64,
    spent_ms: u64,
    reserve_ms: u64,
) -> Option<Headers> {
    let budget = match incoming.get(DEADLINE_HEADER) {
        Some(v) => v.parse::<u64>().ok()?.min(max_ms), // a malformed header must not become "no limit"
        None => max_ms,
    };
    let left = budget.checked_sub(spent_ms.checked_add(reserve_ms)?)?;
    (left > 0).then(|| Headers::from([(DEADLINE_HEADER, left.to_string())]))
}
fn deadline_test() {
    let mut r = Rng(2);
    for _ in 0..2000 {
        let client = 1 + r.below(500);
        let mut headers = Headers::from([(DEADLINE_HEADER, client.to_string())]);
        let (mut elapsed, mut hops) = (0, 0);
        while hops < 10 {
            let spent = r.below(60);
            let Some(next) = forward_budget(&headers, 1000, spent, 5) else {
                break;
            };
            elapsed += spent;
            let left: u64 = next[DEADLINE_HEADER].parse().unwrap();
            assert!(
                elapsed + left + 5 * (hops + 1) <= client,
                "a hop may never be promised more time than the client has"
            );
            headers = next;
            hops += 1;
        }
    }
    assert!(
        forward_budget(
            &Headers::from([(DEADLINE_HEADER, "soon".into())]),
            100,
            0,
            0
        )
        .is_none()
    );
    assert!(
        forward_budget(&Headers::new(), 100, 100, 0).is_none(),
        "an empty budget starts nothing"
    );
    assert_eq!(
        forward_budget(
            &Headers::from([(DEADLINE_HEADER, "999999".into())]),
            100,
            10,
            0
        )
        .unwrap()[DEADLINE_HEADER],
        "90"
    );
    println!("  2000 call chains: the forwarded budget never exceeded what the client had left");
}

// ---------------------------------------------------------------------------
// 3. service registry
// ---------------------------------------------------------------------------
struct Lease {
    generation: u64,
    expires_at: u64,
    healthy: bool,
    streak: u32, // consecutive probe results that disagree with `healthy`
}
#[derive(Default)]
struct Registry {
    instances: BTreeMap<u32, Lease>,
    next_generation: u64,
}
impl Registry {
    const TTL: u64 = 3000;
    const UNHEALTHY_AFTER: u32 = 3;
    const HEALTHY_AFTER: u32 = 2;
    fn register(&mut self, id: u32, now: u64) -> u64 {
        self.next_generation += 1;
        self.instances.insert(
            id,
            Lease {
                generation: self.next_generation,
                expires_at: now + Self::TTL,
                healthy: true,
                streak: 0,
            },
        );
        self.next_generation
    }
    /// False means "your lease is gone: register again".
    fn heartbeat(&mut self, id: u32, generation: u64, now: u64) -> bool {
        match self.instances.get_mut(&id) {
            Some(l) if l.generation == generation && l.expires_at > now => {
                l.expires_at = now + Self::TTL;
                true
            }
            _ => false,
        }
    }
    fn deregister(&mut self, id: u32, generation: u64) -> bool {
        let ours = self
            .instances
            .get(&id)
            .is_some_and(|l| l.generation == generation);
        if ours {
            self.instances.remove(&id);
        }
        ours
    }
    fn probe(&mut self, id: u32, ok: bool) {
        if let Some(l) = self.instances.get_mut(&id) {
            if ok == l.healthy {
                l.streak = 0;
                return;
            }
            l.streak += 1;
            if l.streak
                >= if l.healthy {
                    Self::UNHEALTHY_AFTER
                } else {
                    Self::HEALTHY_AFTER
                }
            {
                l.healthy = ok;
                l.streak = 0;
            }
        }
    }
    /// Expiry is evaluated on READ, so a missed sweeper run can never hand out a dead instance.
    fn healthy(&self, now: u64) -> Vec<u32> {
        self.instances
            .iter()
            .filter(|(_, l)| l.healthy && l.expires_at > now)
            .map(|(&id, _)| id)
            .collect()
    }
}
fn registry_test() {
    let mut reg = Registry::default();
    let old = reg.register(1, 0);
    reg.register(2, 0);
    assert!(reg.heartbeat(1, old, 2999));
    assert_eq!(reg.healthy(2999), [1, 2]);
    assert_eq!(
        reg.healthy(3000),
        [1],
        "instance 2 expired exactly at its TTL"
    );
    assert!(
        !reg.heartbeat(2, 2, 3000),
        "a lapsed lease cannot be revived by a heartbeat"
    );
    reg.probe(1, false);
    reg.probe(1, false);
    reg.probe(1, true); // breaks the streak
    reg.probe(1, false);
    reg.probe(1, false);
    assert_eq!(reg.healthy(3001), [1], "only CONSECUTIVE failures count");
    reg.probe(1, false);
    assert!(reg.healthy(3001).is_empty());
    reg.probe(1, true);
    assert!(reg.healthy(3001).is_empty());
    reg.probe(1, true);
    assert_eq!(reg.healthy(3001), [1]);
    let new = reg.register(1, 4000); // the process restarted
    assert!(
        !reg.deregister(1, old),
        "the old process's late shutdown must not remove the new one"
    );
    assert!(!reg.heartbeat(1, old, 4001));
    assert_eq!(reg.healthy(4001), [1]);
    assert!(reg.deregister(1, new) && reg.healthy(4001).is_empty());
    println!(
        "  TTL expiry, probe hysteresis and generation checks behave exactly at their thresholds"
    );
}

// ---------------------------------------------------------------------------
// 4. load balancing
// ---------------------------------------------------------------------------
#[derive(Clone, Copy, Debug, PartialEq)]
enum Policy {
    Random,
    RoundRobin,
    LeastConnections,
    PowerOfTwo,
}
struct Balancer {
    policy: Policy,
    cursor: usize,
}
impl Balancer {
    fn pick(&mut self, candidates: &[u32], load: &dyn Fn(u32) -> u64, r: &mut Rng) -> Option<u32> {
        let n = candidates.len();
        if n == 0 {
            return None;
        }
        Some(match self.policy {
            Policy::Random => candidates[r.below(n as u64) as usize],
            Policy::RoundRobin => {
                self.cursor += 1;
                candidates[(self.cursor - 1) % n]
            }
            Policy::LeastConnections => *candidates.iter().min_by_key(|&&c| load(c))?,
            Policy::PowerOfTwo if n == 1 => candidates[0],
            Policy::PowerOfTwo => {
                let a = r.below(n as u64) as usize;
                let b = (a + 1 + r.below(n as u64 - 1) as usize) % n; // always two DIFFERENT candidates
                let (a, b) = (candidates[a], candidates[b]);
                if load(b) < load(a) { b } else { a }
            }
        })
    }
}
/// Rendezvous (highest random weight) hashing: sticky by key, and removing an instance moves only that instance's keys.
fn consistent_pick(candidates: &[u32], key: u64) -> Option<u32> {
    candidates
        .iter()
        .copied()
        .max_by_key(|&c| (mix(key ^ mix(u64::from(c) + 1)), c))
}
fn balancing_test() {
    let ids: Vec<u32> = (0..50).collect();
    let mut r = Rng(3);
    let (mut p2c_wins, mut ties, mut sum_max) = (0, 0, [0u64; 2]);
    for _ in 0..2000 {
        let mut max = [0u64; 2];
        for (slot, policy) in [Policy::Random, Policy::PowerOfTwo].into_iter().enumerate() {
            let mut load = vec![0u64; 50];
            let mut b = Balancer { policy, cursor: 0 };
            for _ in 0..500 {
                let c = b.pick(&ids, &|c| load[c as usize], &mut r).unwrap();
                load[c as usize] += 1;
            }
            max[slot] = *load.iter().max().unwrap();
            sum_max[slot] += max[slot];
        }
        p2c_wins += u32::from(max[1] < max[0]);
        ties += u32::from(max[1] == max[0]);
    }
    assert!(
        p2c_wins >= 1990 && ties <= 10,
        "P2C beat random in {p2c_wins}/2000 trials ({ties} ties)"
    );
    let (random, p2c) = (sum_max[0] as f64 / 2000.0, sum_max[1] as f64 / 2000.0);
    assert!(
        p2c < 12.5 && random > 15.0,
        "mean max load with a mean of 10: random {random:.2}, P2C {p2c:.2}"
    );
    println!(
        "  500 requests on 50 instances, 2000 trials: mean max load random {random:.2} vs P2C {p2c:.2}; P2C better in {p2c_wins}, tied in {ties}"
    );

    // round robin is exactly fair; least-connections routes around a slow instance
    let mut rr = Balancer {
        policy: Policy::RoundRobin,
        cursor: 0,
    };
    let mut counts = [0; 4];
    (0..400).for_each(|_| counts[rr.pick(&[0, 1, 2, 3], &|_| 0, &mut r).unwrap() as usize] += 1);
    assert_eq!(counts, [100; 4]);
    let mut lc = Balancer {
        policy: Policy::LeastConnections,
        cursor: 0,
    };
    assert_eq!(
        lc.pick(&[0, 1, 2], &|c| [5, 1, 9][c as usize], &mut r),
        Some(1)
    );
    assert_eq!(lc.pick(&[], &|_| 0, &mut r), None);

    let all: Vec<u32> = (0..10).collect();
    let without_3: Vec<u32> = all.iter().copied().filter(|&c| c != 3).collect();
    let moved = (0..10_000u64)
        .filter(|&k| consistent_pick(&all, k) != consistent_pick(&without_3, k))
        .inspect(|&k| assert_eq!(consistent_pick(&all, k), Some(3)))
        .count();
    assert!((800..1200).contains(&moved), "{moved}");
    println!(
        "  consistent hashing: removing 1 of 10 instances moved {moved}/10000 keys, all of them its own"
    );
}

// ---------------------------------------------------------------------------
// 5a. circuit breaker
// ---------------------------------------------------------------------------
#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    Closed,
    Open { until: u64 },
    HalfOpen { admitted: u32, succeeded: u32 },
}
#[derive(Default, Debug)]
struct BreakerMetrics {
    opened: u32,
    half_opened: u32,
    closed: u32,
    rejected: u64,
    stale_ignored: u64,
}
#[derive(Clone, Copy)]
struct Permit {
    epoch: u64,
}
struct Breaker {
    state: State,
    window: VecDeque<bool>, // true = failure
    epoch: u64,
    metrics: BreakerMetrics,
}
impl Breaker {
    const WINDOW: usize = 20;
    const MIN_VOLUME: usize = 10;
    const FAIL_PERCENT: usize = 50;
    const COOLDOWN: u64 = 500;
    const PROBES: u32 = 3;
    fn new() -> Self {
        Breaker {
            state: State::Closed,
            window: VecDeque::new(),
            epoch: 0,
            metrics: BreakerMetrics::default(),
        }
    }
    fn transition(&mut self, to: State) {
        match to {
            State::Open { .. } => self.metrics.opened += 1,
            State::HalfOpen { .. } => self.metrics.half_opened += 1,
            State::Closed => self.metrics.closed += 1,
        }
        self.state = to;
        self.epoch += 1;
        self.window.clear();
    }
    /// Whether a call COULD be admitted (used to filter balancer candidates without consuming a probe slot).
    fn available(&self, now: u64) -> bool {
        match self.state {
            State::Closed => true,
            State::Open { until } => now >= until,
            State::HalfOpen { admitted, .. } => admitted < Self::PROBES,
        }
    }
    fn acquire(&mut self, now: u64) -> Option<Permit> {
        if let State::Open { until } = self.state {
            if now >= until {
                self.transition(State::HalfOpen {
                    admitted: 0,
                    succeeded: 0,
                });
            }
        }
        let admit = self.available(now) && !matches!(self.state, State::Open { .. });
        if !admit {
            self.metrics.rejected += 1;
            return None;
        }
        if let State::HalfOpen { admitted, .. } = &mut self.state {
            *admitted += 1;
        }
        Some(Permit { epoch: self.epoch })
    }
    fn record(&mut self, permit: Permit, ok: bool, now: u64) {
        if permit.epoch != self.epoch {
            self.metrics.stale_ignored += 1;
            return;
        }
        match self.state {
            State::Closed => {
                self.window.push_back(!ok);
                if self.window.len() > Self::WINDOW {
                    self.window.pop_front();
                }
                let failures = self.window.iter().filter(|&&f| f).count();
                if self.window.len() >= Self::MIN_VOLUME
                    && failures * 100 >= self.window.len() * Self::FAIL_PERCENT
                {
                    self.transition(State::Open {
                        until: now + Self::COOLDOWN,
                    });
                }
            }
            State::HalfOpen {
                admitted,
                succeeded,
            } => {
                if !ok {
                    self.transition(State::Open {
                        until: now + Self::COOLDOWN,
                    });
                } else if succeeded + 1 == Self::PROBES {
                    self.transition(State::Closed);
                } else {
                    self.state = State::HalfOpen {
                        admitted,
                        succeeded: succeeded + 1,
                    };
                }
            }
            State::Open { .. } => {
                unreachable!("opening bumps the epoch, so no permit of this epoch exists")
            }
        }
    }
}
fn breaker_exact() {
    let mut b = Breaker::new();
    // 9 failures: below the minimum volume
    for _ in 0..9 {
        let p = b.acquire(0).unwrap();
        b.record(p, false, 0);
    }
    assert_eq!(b.state, State::Closed);
    let straggler = b.acquire(0).unwrap(); // admitted while closed, finishes much later
    let p = b.acquire(0).unwrap();
    b.record(p, false, 0);
    assert_eq!(
        b.state,
        State::Open { until: 500 },
        "the 10th failure opens"
    );
    assert!(b.acquire(499).is_none());
    let probes: Vec<Permit> = (0..3).map(|_| b.acquire(500).unwrap()).collect();
    assert!(b.acquire(500).is_none(), "only 3 probes in half-open");
    b.record(straggler, true, 501);
    assert_eq!(
        b.state,
        State::HalfOpen {
            admitted: 3,
            succeeded: 0
        },
        "a result from the closed epoch is ignored"
    );
    b.record(probes[0], true, 502);
    b.record(probes[1], false, 503);
    assert_eq!(
        b.state,
        State::Open { until: 1003 },
        "one failed probe re-opens"
    );
    b.record(probes[2], true, 504); // stale again
    let probes: Vec<Permit> = (0..3).map(|_| b.acquire(1003).unwrap()).collect();
    probes.into_iter().for_each(|p| b.record(p, true, 1004));
    assert_eq!(b.state, State::Closed);
    assert_eq!(
        (
            b.metrics.opened,
            b.metrics.half_opened,
            b.metrics.closed,
            b.metrics.stale_ignored
        ),
        (2, 2, 1, 2)
    );
    // ratio threshold: exactly 50% of a full window opens, 9 failures in 20 never does
    for (failures_per_20, should_open) in [(9, false), (10, true)] {
        let mut b = Breaker::new();
        // successes first, so no short prefix of the stream reaches the ratio early
        for i in 0..20 {
            if let Some(p) = b.acquire(0) {
                b.record(p, i < 20 - failures_per_20, 0);
            }
        }
        assert_eq!(
            matches!(b.state, State::Open { .. }),
            should_open,
            "{failures_per_20}/20"
        );
    }
    println!(
        "  breaker: opens at volume 10 and ratio 50%, ignores stale results, needs 3 good probes to close"
    );
}

// ---------------------------------------------------------------------------
// 5b. retry budget
// ---------------------------------------------------------------------------
/// Integer token bucket in thousandths of a retry: each ORIGINAL request deposits `per_request`, each retry costs 1000.
struct RetryBudget {
    millis: u64,
    per_request: u64,
    cap: u64,
}
impl RetryBudget {
    fn new(percent: u64, burst: u64) -> Self {
        RetryBudget {
            millis: burst * 1000,
            per_request: percent * 10,
            cap: burst * 1000,
        }
    }
    fn on_request(&mut self) {
        self.millis = (self.millis + self.per_request).min(self.cap);
    }
    fn try_retry(&mut self) -> bool {
        let ok = self.millis >= 1000;
        if ok {
            self.millis -= 1000;
        }
        ok
    }
}
fn retry_budget_test() {
    let mut r = Rng(5);
    for _ in 0..500 {
        let (percent, burst) = (r.below(30), 1 + r.below(10));
        let mut b = RetryBudget::new(percent, burst);
        let (mut requests, mut retries) = (0u64, 0u64);
        for _ in 0..1000 {
            if r.below(3) == 0 {
                b.on_request();
                requests += 1;
            } else {
                retries += u64::from(b.try_retry());
            }
            assert!(
                retries * 100 <= requests * percent + burst * 100,
                "{retries} retries for {requests} requests at {percent}% + {burst}"
            );
        }
    }
    println!("  500 random streams: retries <= ratio x requests + burst after EVERY event");
}

// ---------------------------------------------------------------------------
// 5c. hedging
// ---------------------------------------------------------------------------
fn latency(r: &mut Rng) -> u64 {
    if r.below(100) < 3 {
        200 + r.below(300)
    } else {
        5 + r.below(15)
    }
}
/// Send a second copy if the first has not answered after `delay`; take whichever finishes first.
/// Returns (latency seen by the caller, copies sent).
fn hedged(delay: u64, r: &mut Rng) -> (u64, u32) {
    let first = latency(r);
    if first <= delay {
        (first, 1)
    } else {
        (first.min(delay + latency(r)), 2)
    }
}
fn percentile(sorted: &[u64], p: f64) -> u64 {
    sorted[((sorted.len() as f64 * p).ceil() as usize).clamp(1, sorted.len()) - 1]
}
fn hedging_test() {
    let n = 100_000;
    let mut r = Rng(6);
    let mut plain: Vec<u64> = (0..n).map(|_| latency(&mut r)).collect();
    plain.sort_unstable();
    let delay = percentile(&plain, 0.95);
    let mut copies = 0;
    let mut with: Vec<u64> = (0..n)
        .map(|_| {
            let (l, c) = hedged(delay, &mut r);
            copies += c;
            l
        })
        .collect();
    with.sort_unstable();
    let (p99, p99_hedged) = (percentile(&plain, 0.99), percentile(&with, 0.99));
    let extra = f64::from(copies) / f64::from(n) - 1.0;
    assert!(
        extra < 0.06,
        "hedging after p95 adds about 5% load, got {extra:.3}"
    );
    assert!(p99 > 200 && p99_hedged < 50, "p99 {p99} -> {p99_hedged}");
    println!(
        "  hedge after p95 = {delay} ms: p99 {p99} -> {p99_hedged} ms for {:.1}% more requests (independent slowness assumed)",
        extra * 100.0
    );
}

// ---------------------------------------------------------------------------
// 6. the whole gateway under failure injection (discrete-event simulation)
// ---------------------------------------------------------------------------
const INSTANCES: u32 = 5;
const BULKHEAD: u64 = 20;
const CLIENT_BUDGET: u64 = 100;
const ATTEMPT_TIMEOUT: u64 = 40;
const MAX_ATTEMPTS: u32 = 3;

struct Attempt {
    request: usize,
    instance: u32,
    permit: Permit,
    ok: bool,
}
#[derive(Clone, Copy)]
struct Request {
    arrived: u64,
    attempts: u32,
}
struct Faults {
    failing: fn(u32, u64) -> bool,
    use_breaker: bool,
    budget_percent: u64,
}
#[derive(Default)]
struct Report {
    requests: u64,
    ok: u64,
    attempts: u64,
    retries: u64,
    budget_denied: u64,
    shed: u64,
    max_in_flight: u64,
    per_instance_bad_window: Vec<u64>,
    breakers: Vec<(u32, u32, State)>, // opened, closed, final state
    ok_after_recovery: u64,
    requests_after_recovery: u64,
}
struct Sim {
    breakers: Vec<Breaker>,
    in_flight: Vec<u64>,
    events: BinaryHeap<Reverse<(u64, u64, usize)>>, // (finish time, seq, attempt)
    attempts: Vec<Attempt>,
    requests: Vec<Request>,
    budget: RetryBudget,
    balancer: Balancer,
    rng: Rng,
    report: Report,
}
impl Sim {
    /// Try to start one attempt for `request`; false if there is no instance or no time.
    fn start(&mut self, request: usize, now: u64, f: &Faults) -> bool {
        let remaining = (self.requests[request].arrived + CLIENT_BUDGET).saturating_sub(now);
        if remaining == 0 {
            return false;
        }
        let candidates: Vec<u32> = (0..INSTANCES)
            .filter(|&i| {
                self.in_flight[i as usize] < BULKHEAD
                    && (!f.use_breaker || self.breakers[i as usize].available(now))
            })
            .collect();
        let in_flight = &self.in_flight;
        let Some(i) = self
            .balancer
            .pick(&candidates, &|c| in_flight[c as usize], &mut self.rng)
        else {
            return false;
        };
        let permit = if f.use_breaker {
            self.breakers[i as usize]
                .acquire(now)
                .expect("an available breaker admits")
        } else {
            Permit { epoch: 0 }
        };
        let bad = (f.failing)(i, now);
        // a failing instance is slow AND wrong: most calls hang until the timeout, the rest error quickly
        let natural = if bad {
            if self.rng.below(10) < 6 { 1000 } else { 3 }
        } else {
            5 + self.rng.below(15)
        };
        let limit = ATTEMPT_TIMEOUT.min(remaining);
        let ok = !bad && natural <= limit;
        self.in_flight[i as usize] += 1;
        assert!(self.in_flight[i as usize] <= BULKHEAD);
        self.report.max_in_flight = self.report.max_in_flight.max(self.in_flight[i as usize]);
        self.report.attempts += 1;
        if (2000..4000).contains(&now) {
            self.report.per_instance_bad_window[i as usize] += 1;
        }
        self.requests[request].attempts += 1;
        let seq = self.attempts.len();
        self.events
            .push(Reverse((now + natural.min(limit), seq as u64, seq)));
        self.attempts.push(Attempt {
            request,
            instance: i,
            permit,
            ok,
        });
        true
    }
    fn finish(&mut self, id: usize, now: u64, f: &Faults) {
        let (request, i, permit, ok) = {
            let a = &self.attempts[id];
            (a.request, a.instance as usize, a.permit, a.ok)
        };
        self.in_flight[i] -= 1;
        if f.use_breaker {
            self.breakers[i].record(permit, ok, now);
        }
        let req = self.requests[request];
        assert!(
            now <= req.arrived + CLIENT_BUDGET,
            "no attempt may outlive the client's deadline"
        );
        if ok {
            self.report.ok += 1;
            self.report.ok_after_recovery += u64::from(req.arrived >= 6000);
        } else if req.attempts < MAX_ATTEMPTS && now < req.arrived + CLIENT_BUDGET {
            if self.budget.try_retry() {
                if self.start(request, now, f) {
                    self.report.retries += 1;
                }
            } else {
                self.report.budget_denied += 1;
            }
        }
    }
}
fn simulate(f: &Faults, seed: u64) -> Report {
    let mut s = Sim {
        breakers: (0..INSTANCES).map(|_| Breaker::new()).collect(),
        in_flight: vec![0; INSTANCES as usize],
        events: BinaryHeap::new(),
        attempts: vec![],
        requests: vec![],
        budget: RetryBudget::new(f.budget_percent, 20),
        balancer: Balancer {
            policy: Policy::PowerOfTwo,
            cursor: 0,
        },
        rng: Rng(seed),
        report: Report {
            per_instance_bad_window: vec![0; INSTANCES as usize],
            ..Default::default()
        },
    };
    for now in 0..10_000u64 {
        while s.events.peek().is_some_and(|e| e.0.0 <= now) {
            let Reverse((at, _, id)) = s.events.pop().unwrap();
            s.finish(id, at, f);
        }
        for _ in 0..s.rng.below(3) {
            s.budget.on_request();
            s.report.requests += 1;
            s.report.requests_after_recovery += u64::from(now >= 6000);
            s.requests.push(Request {
                arrived: now,
                attempts: 0,
            });
            let id = s.requests.len() - 1;
            if !s.start(id, now, f) {
                s.report.shed += 1;
            }
        }
        assert!(
            s.report.retries * 100 <= s.report.requests * f.budget_percent + 20 * 100,
            "retry budget exceeded at t={now}"
        );
    }
    while let Some(Reverse((at, _, id))) = s.events.pop() {
        s.finish(id, at, f);
    }
    assert!(
        s.in_flight.iter().all(|&n| n == 0),
        "every attempt released its bulkhead slot"
    );
    s.report.breakers = s
        .breakers
        .iter()
        .map(|b| (b.metrics.opened, b.metrics.closed, b.state))
        .collect();
    s.report
}
fn one_bad(i: u32, now: u64) -> bool {
    i == 2 && (2000..4000).contains(&now)
}
fn all_bad(_: u32, now: u64) -> bool {
    (2000..4000).contains(&now)
}
fn gateway_test() {
    println!(
        "  {:<34}{:>9}{:>9}{:>10}{:>9}{:>12}",
        "scenario", "requests", "success", "attempts", "retries", "to sick #2"
    );
    let run = |name: &str, f: Faults| {
        let rep = simulate(&f, 7);
        println!(
            "  {name:<34}{:>9}{:>8.2}%{:>10}{:>9}{:>12}",
            rep.requests,
            rep.ok as f64 * 100.0 / rep.requests as f64,
            rep.attempts,
            rep.retries,
            rep.per_instance_bad_window[2]
        );
        rep
    };
    let protected = run(
        "one sick instance, breaker",
        Faults {
            failing: one_bad,
            use_breaker: true,
            budget_percent: 10,
        },
    );
    let naked = run(
        "one sick instance, no breaker",
        Faults {
            failing: one_bad,
            use_breaker: false,
            budget_percent: 10,
        },
    );
    let outage = run(
        "full outage, 10% budget",
        Faults {
            failing: all_bad,
            use_breaker: false,
            budget_percent: 10,
        },
    );
    let storm = run(
        "full outage, 200% budget",
        Faults {
            failing: all_bad,
            use_breaker: false,
            budget_percent: 200,
        },
    );

    // SLO-style invariants
    assert!(
        protected.ok * 1000 >= protected.requests * 995,
        "99.5% success with one of five instances sick"
    );
    for (i, &(opened, closed, state)) in protected.breakers.iter().enumerate() {
        if i == 2 {
            assert!(
                opened >= 1 && closed >= 1 && state == State::Closed,
                "the sick breaker opened and later recovered: {opened} {closed} {state:?}"
            );
        } else {
            assert_eq!(opened, 0, "healthy instance {i} must never trip");
        }
    }
    assert!(
        protected.per_instance_bad_window[2] * 4 < naked.per_instance_bad_window[2],
        "the breaker shields the sick instance"
    );
    assert!(protected.ok > naked.ok);
    assert_eq!(
        protected.ok_after_recovery, protected.requests_after_recovery,
        "after recovery every request succeeds"
    );
    assert!(
        outage.budget_denied > 0 && outage.attempts * 100 <= outage.requests * 110 + 2000,
        "load stays within 1.1x + burst during an outage"
    );
    assert!(
        storm.attempts > outage.attempts + outage.requests / 10,
        "a loose budget multiplies load on a dead service"
    );
    assert!(protected.max_in_flight <= BULKHEAD && naked.max_in_flight <= BULKHEAD);
}

// ---------------------------------------------------------------------------
fn main() {
    println!("== 1. routing ==");
    routing_test();
    println!("== 2. deadline propagation ==");
    deadline_test();
    println!("== 3. service registry ==");
    registry_test();
    println!("== 4. load balancing ==");
    balancing_test();
    println!("== 5. breaker, retry budget, hedging ==");
    breaker_exact();
    retry_budget_test();
    hedging_test();
    println!(
        "== 6. gateway under failure injection (virtual 10 s, instance(s) sick from 2 s to 4 s) =="
    );
    gateway_test();
    println!("all gateway checks passed");
}
