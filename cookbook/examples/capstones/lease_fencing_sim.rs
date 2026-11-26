//! CAPSTONE 05 (distributed): deterministic simulation of leases, pauses and fencing tokens
//! LEVEL: capstone
//! Use in: designing or reviewing distributed locks, leader election, or any "only one writer at a time" protocol.   Run: cargo run --release --example lease_fencing_sim
#![allow(dead_code)]
//!
//! WHAT: a discrete-event simulation of a lease service (grants a TTL lease plus a monotonically increasing FENCING TOKEN),
//! several clients (acquire, write to shared storage, renew via heartbeat) and a storage service that either ignores or
//! enforces the tokens. Time is a simulated clock; the event queue is a `BinaryHeap<Reverse<(time, seq, Event)>>` and every
//! delay comes from a seeded xorshift RNG, so each schedule is exactly reproducible from its seed.
//! WHY: a lease alone cannot give mutual exclusion. The service decides "your lease expired" using ITS clock, but a client
//! can only act on what it believed when it last looked. If the client then stalls (GC pause, VM freeze, slow network) for
//! longer than the TTL, it wakes up still believing it holds the lease while a new holder is already writing. No check on the
//! client side closes this gap, because the pause can land between the check and the write. This is the argument of
//! Martin Kleppmann's essay (Read: below). The fix must live at the resource: reject any write whose token is lower than
//! the highest token already seen.
//! HOW: scenario A (no fencing) is searched for deterministically and replayed with a timeline showing the corruption;
//! scenario B replays the SAME schedule against a fencing storage and shows the stale write rejected; then 2000 random
//! schedules check the safety invariant: accepted tokens never go backwards and the final value is the last write of the
//! highest token. Without fencing the same schedules violate it. Clients deliberately ignore the write ack when deciding what
//! to do next, so the schedule is identical in both modes and the comparison is apples to apples.
//! COMMON USES: ZooKeeper / etcd / Consul leases with revision numbers as fencing tokens, Chubby sequencers, HBase and
//! Kafka controller epochs, Raft terms rejecting stale leaders, S3 or DynamoDB conditional writes (version check),
//! database "SELECT ... FOR UPDATE" plus version columns, Kubernetes leader election with resourceVersion.
//! Testing pattern: deterministic simulation testing (FoundationDB, TigerBeetle, Jepsen/Maelstrom style fault injection).
//! Read: https://martin.kleppmann.com/2016/02/08/how-to-do-distributed-locking.html , https://fly.io/dist-sys/ and
//! https://github.com/jepsen-io/maelstrom
use std::cmp::Reverse;
use std::collections::BinaryHeap;

type Time = u64;
/// Lease length in simulated milliseconds. Pauses are drawn relative to it so they can outlive a lease.
const TTL: Time = 100;
/// Safety valve: a correct schedule finishes in far fewer events; hitting this means a livelock bug.
const MAX_STEPS: u32 = 100_000;

// ---------------------------------------------------------------- deterministic randomness

/// xorshift64: tiny, fast, and identical on every machine (no `rand` dependency needed).
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // xorshift must never hold 0; the multiplier spreads small consecutive seeds apart.
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        for _ in 0..4 {
            rng.next();
        }
        rng
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    /// Uniform-ish value in `lo..=hi`.
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next() % (hi - lo + 1)
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

// ---------------------------------------------------------------- events

/// Every message arrival or timer is an event. `Ord` is derived only because the heap tuple needs it;
/// the unique `seq` field decides ties first so equal timestamps stay deterministic (FIFO).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Event {
    Wake { c: usize },
    AcquireAtSvc { c: usize },
    GrantAtClient { c: usize, token: u64, expiry: Time },
    DeniedAtClient { c: usize },
    DoWrite { c: usize },
    WriteAtStorage { c: usize, token: u64, value: u64 },
    WriteAck { c: usize, accepted: bool },
    RenewAtSvc { c: usize, token: u64 },
    RenewAck { c: usize, ok: bool },
}

// ---------------------------------------------------------------- lease service

struct Holder {
    client: usize,
    token: u64,
    expiry: Time,
}

/// Grants at most one UNEXPIRED lease at a time (by its own clock) and hands out strictly increasing tokens.
struct LeaseService {
    next_token: u64,
    holder: Option<Holder>,
}

impl LeaseService {
    fn new() -> Self {
        LeaseService {
            next_token: 1,
            holder: None,
        }
    }

    fn acquire(&mut self, now: Time, client: usize) -> Option<(u64, Time)> {
        if self.holder.as_ref().is_some_and(|h| h.expiry > now) {
            return None; // someone else (or we, earlier) still holds a live lease
        }
        let token = self.next_token;
        self.next_token += 1; // the token is the ONLY thing storage can trust, so it must never repeat or go back
        let expiry = now + TTL;
        self.holder = Some(Holder {
            client,
            token,
            expiry,
        });
        Some((token, expiry))
    }

    /// Heartbeat: extends the lease only if it is still ours and has not already expired on the service's clock.
    fn renew(&mut self, now: Time, client: usize, token: u64) -> Option<Time> {
        let h = self.holder.as_mut()?;
        if h.client != client || h.token != token || h.expiry <= now {
            return None;
        }
        h.expiry = now + TTL;
        Some(h.expiry)
    }
}

// ---------------------------------------------------------------- storage

#[derive(Clone, Copy, Debug)]
struct Write {
    time: Time,
    client: usize,
    token: u64,
    value: u64,
}

/// The shared resource. With `fencing` it refuses writes carrying a token below the highest one seen.
struct Storage {
    fencing: bool,
    value: u64,
    max_token: u64,
    accepted: Vec<Write>,
    arrived: Vec<Write>,
    rejected: u32,
}

impl Storage {
    fn new(fencing: bool) -> Self {
        Storage {
            fencing,
            value: 0,
            max_token: 0,
            accepted: vec![],
            arrived: vec![],
            rejected: 0,
        }
    }

    fn write(&mut self, w: Write) -> bool {
        self.arrived.push(w);
        if self.fencing && w.token < self.max_token {
            self.rejected += 1;
            return false;
        }
        self.max_token = self.max_token.max(w.token);
        self.value = w.value;
        self.accepted.push(w);
        true
    }
}

/// SAFETY: (1) accepted tokens never decrease; (2) the final value is the last write of the highest token.
fn check_safety(s: &Storage) -> Result<(), String> {
    for pair in s.accepted.windows(2) {
        if pair[1].token < pair[0].token {
            return Err(format!(
                "stale write accepted: token {} (client {}) at {}ms landed after token {}",
                pair[1].token, pair[1].client, pair[1].time, pair[0].token
            ));
        }
    }
    // Value ids grow with send order, and one token's writes are serialized by acks, so (token, value) orders them.
    match s.arrived.iter().max_by_key(|w| (w.token, w.value)) {
        Some(best) if best.value != s.value => Err(format!(
            "final value {} != newest holder's write {} (token {})",
            s.value, best.value, best.token
        )),
        _ => Ok(()),
    }
}

// ---------------------------------------------------------------- clients and simulation

struct Client {
    token: Option<u64>,
    leases_left: u32,
    writes_left: u32,
}

#[derive(Clone, Copy)]
struct Config {
    clients: usize,
    leases_per_client: u32,
    writes_per_lease: u32,
    pause_percent: u64,
}

struct Sim {
    now: Time,
    seq: u64,
    queue: BinaryHeap<Reverse<(Time, u64, Event)>>,
    rng: Rng,
    svc: LeaseService,
    storage: Storage,
    clients: Vec<Client>,
    cfg: Config,
    next_value: u64,
    steps: u32,
    trace: Option<Vec<String>>,
}

impl Sim {
    fn new(seed: u64, cfg: Config, fencing: bool, trace: bool) -> Self {
        let clients = (0..cfg.clients)
            .map(|_| Client {
                token: None,
                leases_left: cfg.leases_per_client,
                writes_left: 0,
            })
            .collect();
        Sim {
            now: 0,
            seq: 0,
            queue: BinaryHeap::new(),
            rng: Rng::new(seed),
            svc: LeaseService::new(),
            storage: Storage::new(fencing),
            clients,
            cfg,
            next_value: 1,
            steps: 0,
            trace: trace.then(Vec::new),
        }
    }

    fn log(&mut self, line: String) {
        if let Some(t) = self.trace.as_mut() {
            t.push(line);
        }
    }

    fn schedule(&mut self, delay: Time, ev: Event) {
        self.seq += 1;
        self.queue.push(Reverse((self.now + delay, self.seq, ev)));
    }

    /// One-way message latency.
    fn net(&mut self) -> Time {
        self.rng.range(1, 20)
    }

    /// Time a client spends "working" before its next write; sometimes it stalls far past the TTL.
    fn work_delay(&mut self, c: usize) -> Time {
        let work = self.rng.range(5, 30);
        if !self.rng.chance(self.cfg.pause_percent) {
            return work;
        }
        let pause = self.rng.range(TTL / 2, 3 * TTL);
        self.log(format!(
            "        client {c} PAUSES for {pause}ms (GC / VM stall); it cannot know that"
        ));
        work + pause
    }

    fn run(&mut self) {
        for c in 0..self.cfg.clients {
            let start = self.rng.range(0, 10);
            self.schedule(start, Event::Wake { c });
        }
        while let Some(Reverse((time, _, ev))) = self.queue.pop() {
            self.now = time;
            self.steps += 1;
            assert!(self.steps < MAX_STEPS, "simulation did not terminate");
            self.log(format!("{time:>5}ms  {ev:?}"));
            self.handle(ev);
        }
    }

    fn handle(&mut self, ev: Event) {
        match ev {
            Event::Wake { c } if self.clients[c].leases_left > 0 => {
                let d = self.net();
                self.schedule(d, Event::AcquireAtSvc { c });
            }
            Event::Wake { .. } => {}
            Event::AcquireAtSvc { c } => {
                let d = self.net();
                match self.svc.acquire(self.now, c) {
                    Some((token, expiry)) => {
                        self.log(format!(
                            "        service: grant token {token} to client {c}, expires {expiry}ms"
                        ));
                        self.schedule(d, Event::GrantAtClient { c, token, expiry });
                    }
                    None => self.schedule(d, Event::DeniedAtClient { c }),
                }
            }
            Event::GrantAtClient { c, token, expiry } => {
                let client = &mut self.clients[c];
                client.token = Some(token);
                client.leases_left -= 1;
                client.writes_left = self.cfg.writes_per_lease;
                // The client CHECKS validity here and it passes. The pause comes after the check: that gap is the bug.
                debug_assert!(self.now < expiry || self.cfg.pause_percent > 0);
                let d = self.work_delay(c);
                self.schedule(d, Event::DoWrite { c });
            }
            Event::DeniedAtClient { c } => {
                let backoff = self.rng.range(20, 60);
                self.schedule(backoff, Event::Wake { c });
            }
            Event::DoWrite { c } => {
                let token = self.clients[c].token.expect("write without a lease");
                self.clients[c].writes_left -= 1;
                let value = self.next_value;
                self.next_value += 1;
                let d = self.net();
                self.schedule(d, Event::WriteAtStorage { c, token, value });
            }
            Event::WriteAtStorage { c, token, value } => {
                let w = Write {
                    time: self.now,
                    client: c,
                    token,
                    value,
                };
                let accepted = self.storage.write(w);
                let verdict = if accepted {
                    "accepted"
                } else {
                    "REJECTED (token below highest seen)"
                };
                self.log(format!(
                    "        storage: value {value} token {token} from client {c} {verdict}"
                ));
                let d = self.net();
                self.schedule(d, Event::WriteAck { c, accepted });
            }
            // The ack's verdict is only logged, never acted on: this keeps the schedule identical with and without fencing.
            Event::WriteAck { c, .. } => {
                if self.clients[c].writes_left > 0 {
                    let token = self.clients[c].token.expect("ack without a lease");
                    let d = self.net();
                    self.schedule(d, Event::RenewAtSvc { c, token });
                } else {
                    self.clients[c].token = None;
                    let d = self.rng.range(10, 40);
                    self.schedule(d, Event::Wake { c });
                }
            }
            Event::RenewAtSvc { c, token } => {
                let renewed = self.svc.renew(self.now, c, token);
                self.log(format!(
                    "        service: heartbeat from client {c} token {token}: {}",
                    renewed.map_or("too late".into(), |e| format!("extended to {e}ms"))
                ));
                let d = self.net();
                self.schedule(
                    d,
                    Event::RenewAck {
                        c,
                        ok: renewed.is_some(),
                    },
                );
            }
            Event::RenewAck { c, ok: true } => {
                let d = self.work_delay(c);
                self.schedule(d, Event::DoWrite { c });
            }
            Event::RenewAck { c, ok: false } => {
                // Lost the lease: stop writing under this token and compete again.
                self.clients[c].token = None;
                self.clients[c].writes_left = 0;
                let d = self.rng.range(10, 40);
                self.schedule(d, Event::Wake { c });
            }
        }
    }
}

fn simulate(seed: u64, cfg: Config, fencing: bool, trace: bool) -> Sim {
    let mut sim = Sim::new(seed, cfg, fencing, trace);
    sim.run();
    sim
}

// ---------------------------------------------------------------- demo and checks

fn print_trace(title: &str, sim: &Sim, only_storage: bool) {
    println!("--- {title} ---");
    for line in sim.trace.as_deref().unwrap_or_default() {
        if !only_storage || line.contains("storage:") || line.contains("PAUSES") {
            println!("{line}");
        }
    }
}

/// Unit checks of the lease service on its own: mutual exclusion by ITS clock, monotone tokens, renewal.
fn check_lease_service() {
    let mut svc = LeaseService::new();
    let (t1, exp1) = svc.acquire(0, 0).expect("free lease is granted");
    assert!(
        svc.acquire(exp1 - 1, 1).is_none(),
        "live lease must not be granted twice"
    );
    assert_eq!(
        svc.renew(50, 0, t1),
        Some(50 + TTL),
        "heartbeat extends the lease"
    );
    assert!(
        svc.renew(50 + TTL, 0, t1).is_none(),
        "an expired lease cannot be renewed"
    );
    let (t2, _) = svc
        .acquire(50 + TTL, 1)
        .expect("expired lease is re-granted");
    assert!(t2 > t1, "tokens strictly increase");
    assert!(
        svc.renew(50 + TTL + 1, 0, t1).is_none(),
        "the old holder's heartbeat is refused"
    );
}

/// Find the first seed whose schedule corrupts unfenced storage: a concrete, reproducible bad interleaving.
fn find_corrupting_seed(cfg: Config) -> u64 {
    (1..10_000)
        .find(|&seed| check_safety(&simulate(seed, cfg, false, false).storage).is_err())
        .expect("pauses longer than the TTL must eventually corrupt unfenced storage")
}

fn main() {
    check_lease_service();

    let demo = Config {
        clients: 2,
        leases_per_client: 1,
        writes_per_lease: 2,
        pause_percent: 50,
    };
    let seed = find_corrupting_seed(demo);
    println!("demo schedule: seed {seed}, TTL {TTL}ms, 2 clients\n");

    // Scenario A: no fencing. The stale write goes through and overwrites the newer holder's data.
    let a = simulate(seed, demo, false, true);
    print_trace("scenario A: storage WITHOUT fencing", &a, false);
    let err = check_safety(&a.storage).expect_err("scenario A must corrupt data");
    println!("\nCORRUPTION: {err}\nfinal value = {}\n", a.storage.value);
    assert_eq!(a.storage.rejected, 0);

    // Scenario B: identical schedule, fencing storage. The stale write is rejected.
    let b = simulate(seed, demo, true, true);
    print_trace(
        "scenario B: same schedule WITH fencing (storage events only)",
        &b,
        true,
    );
    assert!(b.storage.rejected >= 1, "the stale write must be rejected");
    check_safety(&b.storage).expect("fencing keeps storage safe");
    assert_eq!(
        b.storage.arrived.len(),
        a.storage.arrived.len(),
        "same schedule, same writes"
    );
    println!(
        "\nSAFE: {} stale write(s) rejected, final value = {}\n",
        b.storage.rejected, b.storage.value
    );

    // Randomized campaign: same seeds, both storages.
    let cfg = Config {
        clients: 3,
        leases_per_client: 3,
        writes_per_lease: 3,
        pause_percent: 30,
    };
    let (mut unfenced_bad, mut fenced_bad, mut stale_rejected, mut writes) = (0, 0, 0, 0);
    for seed in 0..2000 {
        let unfenced = simulate(seed, cfg, false, false);
        let fenced = simulate(seed, cfg, true, false);
        unfenced_bad += u32::from(check_safety(&unfenced.storage).is_err());
        fenced_bad += u32::from(check_safety(&fenced.storage).is_err());
        stale_rejected += fenced.storage.rejected;
        writes += fenced.storage.arrived.len();
        assert!(
            fenced.clients.iter().all(|c| c.leases_left == 0),
            "every client finished its leases"
        );
    }
    println!("2000 random schedules ({writes} writes):");
    println!("  without fencing: {unfenced_bad} safety violations");
    println!(
        "  with fencing:    {fenced_bad} safety violations ({stale_rejected} stale writes rejected)"
    );
    assert_eq!(fenced_bad, 0, "fencing must make every schedule safe");
    assert!(
        unfenced_bad > 0,
        "without fencing some schedules must violate safety"
    );
    assert!(stale_rejected > 0);

    // Determinism: the same seed replays to the identical outcome.
    let again = simulate(seed, demo, false, true);
    assert_eq!(again.trace, a.trace, "same seed, same timeline");
    println!("\nall assertions passed");
}
