//! RECIPE 404 (distributed): Chandy-Lamport distributed snapshot (consistent cuts) on FIFO channels
//! LEVEL: advanced
//! Use in: checkpointing stream processors, debugging distributed state, detecting stable properties.   Run: cargo run --release --example chandy_lamport_snapshot
#![allow(dead_code)]
//!
//! WHAT: P processes pass "money" tokens over reliable FIFO channels (one per ordered pair). Nobody has a
//! global clock and nobody can freeze the system, yet we want a snapshot: each process's local state plus the
//! messages in flight on every channel, such that the snapshot is a state the system COULD have been in.
//!
//! HOW (the marker rule):
//!  * An initiator records its own state, then sends a MARKER on every outgoing channel and starts recording
//!    every incoming channel.
//!  * On the first marker a process receives (from channel c): record own state, record channel c as EMPTY,
//!    send markers on all outgoing channels, start recording all other incoming channels.
//!  * On a later marker from channel c: stop recording c; what was recorded since is c's in-flight state.
//!  * A process is done when every incoming channel has delivered its marker. Cost: exactly one marker per
//!    channel, i.e. P*(P-1) messages per snapshot; the application never stops.
//!  Because channels are FIFO, a marker separates "sent before the sender recorded" from "sent after", so
//!  every message is counted exactly once: either inside a recorded balance or in a recorded channel. That
//!  is why the recorded balances plus recorded in-flight money add up to the constant total.
//!
//! CONSISTENT CUT: a cut (one local prefix per process) is consistent iff it is closed under happened-before
//! (Lamport): if a receive is inside the cut, its send is too. We check that literally with vector clocks:
//! for every pair, vc_i[j] <= vc_j[j] at the recorded points (nobody has "seen" an event of j that j's recorded
//! state does not include). A naive "each process reports its balance at some moment" violates both the
//! money invariant and cut consistency, as the experiment shows.
//!
//! MULTIPLE INITIATORS: markers carry a snapshot id and every process keeps independent recording state per
//! id, so concurrent snapshots simply run side by side (each is verified separately below). The alternative
//! used by simple textbook versions is to ignore markers for a snapshot once one is in progress.
//!
//! COMMON USES: Apache Flink's checkpoints are Chandy-Lamport with barriers (aligned barriers = markers on
//! FIFO channels; unaligned checkpoints record the in-flight records exactly like channel state here);
//! distributed deadlock detection and termination detection (a stable property true in a consistent cut stays
//! true afterwards); distributed garbage collection; global debugging/replay; cluster-wide consistent backups.
//! Read: Distributed Snapshots: Determining Global States of Distributed Systems (Chandy, Lamport, 1985)
//!   https://lamport.azurewebsites.net/pubs/chandy.pdf
//! Read: Time, Clocks, and the Ordering of Events in a Distributed System (Lamport, 1978)
//!   https://lamport.azurewebsites.net/pubs/time-clocks.pdf

use std::collections::{BTreeMap, VecDeque};

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next() % (hi - lo + 1)
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

type Vc = Vec<u64>;

#[derive(Clone, Debug)]
enum Kind {
    Money(u64),
    Marker(usize), // snapshot id
}

#[derive(Clone, Debug)]
struct Msg {
    kind: Kind,
    vc: Vc,
}

/// One process's view of one snapshot.
#[derive(Clone)]
struct SnapState {
    balance: u64,
    vc: Vc,
    /// recording[j]: still collecting messages arriving from j.
    recording: Vec<bool>,
    /// channel[j]: money recorded in flight on channel j -> me.
    channel: Vec<Vec<u64>>,
}

struct Proc {
    balance: u64,
    vc: Vc,
    snaps: BTreeMap<usize, SnapState>,
}

struct Net {
    p: usize,
    procs: Vec<Proc>,
    /// chans[from][to]: FIFO queue.
    chans: Vec<Vec<VecDeque<Msg>>>,
    markers_sent: BTreeMap<usize, usize>,
}

impl Net {
    fn new(balances: &[u64]) -> Self {
        let p = balances.len();
        Net {
            p,
            procs: balances
                .iter()
                .map(|&b| Proc {
                    balance: b,
                    vc: vec![0; p],
                    snaps: BTreeMap::new(),
                })
                .collect(),
            chans: (0..p)
                .map(|_| (0..p).map(|_| VecDeque::new()).collect())
                .collect(),
            markers_sent: BTreeMap::new(),
        }
    }

    fn tick(&mut self, i: usize) {
        self.procs[i].vc[i] += 1;
    }

    fn send(&mut self, from: usize, to: usize, kind: Kind) {
        self.tick(from);
        let vc = self.procs[from].vc.clone();
        self.chans[from][to].push_back(Msg { kind, vc });
    }

    fn send_money(&mut self, from: usize, to: usize, amt: u64) {
        self.procs[from].balance -= amt;
        self.send(from, to, Kind::Money(amt));
    }

    /// Record local state for snapshot `id`; `closed` is the channel whose marker triggered it (recorded empty).
    fn record(&mut self, i: usize, id: usize, closed: Option<usize>) {
        self.tick(i); // recording is a local event
        let p = self.p;
        let recording = (0..p).map(|j| j != i && Some(j) != closed).collect();
        let proc = &mut self.procs[i];
        proc.snaps.insert(
            id,
            SnapState {
                balance: proc.balance,
                vc: proc.vc.clone(),
                recording,
                channel: vec![Vec::new(); p],
            },
        );
        for j in (0..p).filter(|&j| j != i) {
            self.send(i, j, Kind::Marker(id));
            *self.markers_sent.entry(id).or_insert(0) += 1;
        }
    }

    fn deliver(&mut self, from: usize, to: usize) {
        let msg = self.chans[from][to].pop_front().expect("non-empty channel");
        // Receive event. Only APPLICATION messages carry causality for the cut: markers are control traffic
        // sent after the sender recorded, so merging their clocks would make the cut look inconsistent.
        if matches!(msg.kind, Kind::Money(_)) {
            for k in 0..self.p {
                self.procs[to].vc[k] = self.procs[to].vc[k].max(msg.vc[k]);
            }
        }
        self.tick(to);
        match msg.kind {
            Kind::Money(amt) => {
                // Application first: the balance changes regardless of snapshots...
                self.procs[to].balance += amt;
                // ...and every snapshot still recording this channel counts the message as in flight.
                for s in self.procs[to].snaps.values_mut() {
                    if s.recording[from] {
                        s.channel[from].push(amt);
                    }
                }
                // The recorded balance must NOT include it: it was recorded before this message arrived.
            }
            Kind::Marker(id) => {
                if self.procs[to].snaps.contains_key(&id) {
                    self.procs[to].snaps.get_mut(&id).unwrap().recording[from] = false;
                } else {
                    self.record(to, id, Some(from));
                }
            }
        }
    }

    fn complete(&self, id: usize) -> bool {
        self.procs.iter().all(|p| {
            p.snaps
                .get(&id)
                .is_some_and(|s| s.recording.iter().all(|r| !r))
        })
    }

    fn nonempty_channels(&self) -> Vec<(usize, usize)> {
        let mut v = Vec::new();
        for i in 0..self.p {
            for j in 0..self.p {
                if !self.chans[i][j].is_empty() {
                    v.push((i, j));
                }
            }
        }
        v
    }

    fn in_flight_money(&self) -> u64 {
        self.chans
            .iter()
            .flatten()
            .flatten()
            .map(|m| if let Kind::Money(a) = m.kind { a } else { 0 })
            .sum()
    }
}

/// Consistent cut test via vector clocks: nobody's recorded state has seen more of j than j recorded.
fn consistent_cut(vcs: &[Vc]) -> bool {
    let p = vcs.len();
    (0..p).all(|i| (0..p).all(|j| vcs[i][j] <= vcs[j][j]))
}

struct SnapshotResult {
    balances: u64,
    in_flight: u64,
    consistent: bool,
}

fn collect(net: &Net, id: usize) -> SnapshotResult {
    let balances = net.procs.iter().map(|p| p.snaps[&id].balance).sum();
    let in_flight = net
        .procs
        .iter()
        .map(|p| p.snaps[&id].channel.iter().flatten().sum::<u64>())
        .sum();
    let vcs: Vec<Vc> = net.procs.iter().map(|p| p.snaps[&id].vc.clone()).collect();
    SnapshotResult {
        balances,
        in_flight,
        consistent: consistent_cut(&vcs),
    }
}

#[derive(Default)]
struct RunStats {
    snapshots: u64,
    initiators: u64,
    recorded_in_flight: u64,
    snapshots_with_in_flight: u64,
    naive_sum_broken: bool,
    naive_cut_inconsistent: bool,
    instant_sum_broken: bool,
}

/// One random run: random topology size, balances, traffic, initiators and start times.
fn run(seed: u64, verbose: bool) -> RunStats {
    let mut rng = Rng::new(seed);
    let p = rng.range(3, 6) as usize;
    let balances: Vec<u64> = (0..p).map(|_| rng.range(50, 200)).collect();
    let total: u64 = balances.iter().sum();
    let mut net = Net::new(&balances);

    // 1..=3 distinct initiators at random times (concurrent snapshots overlap in flight).
    let k = rng.range(1, 3) as usize;
    let mut starts: Vec<(u64, usize, usize)> = Vec::new(); // (step, proc, id)
    let mut taken = Vec::new();
    while starts.len() < k {
        let who = rng.next() as usize % p;
        if !taken.contains(&who) {
            taken.push(who);
            starts.push((rng.range(0, 150), who, who)); // id = initiator
        }
    }
    // Naive observers: every process reports its balance at its own random moment.
    let read_at: Vec<u64> = (0..p).map(|_| rng.range(0, 250)).collect();
    let mut naive_balance = vec![0u64; p];
    let mut naive_vc: Vec<Vc> = vec![Vec::new(); p];

    let mut stats = RunStats {
        initiators: k as u64,
        ..Default::default()
    };
    let mut instant_taken = false;
    let mut step = 0u64;
    loop {
        for &(_, who, id) in starts.iter().filter(|s| s.0 == step) {
            if !instant_taken {
                // Everyone's balance at ONE instant, ignoring channels: also wrong when money is in flight.
                let instant: u64 = net.procs.iter().map(|q| q.balance).sum();
                stats.instant_sum_broken = instant != total;
                instant_taken = true;
            }
            net.record(who, id, None);
        }
        for i in 0..p {
            if read_at[i] == step {
                naive_balance[i] = net.procs[i].balance;
                naive_vc[i] = net.procs[i].vc.clone();
            }
        }
        // Application traffic stops after step 400 so the run terminates.
        let nonempty = net.nonempty_channels();
        let send = step < 400 && (nonempty.is_empty() || rng.chance(45));
        if send {
            let from = rng.next() as usize % p;
            let to = (from + 1 + rng.next() as usize % (p - 1)) % p;
            let bal = net.procs[from].balance;
            if bal > 0 {
                let amt = rng.range(1, bal.min(40));
                net.send_money(from, to, amt);
            }
        } else if !nonempty.is_empty() {
            // Which channel delivers next is the nondeterminism; FIFO order inside it is not.
            let (i, j) = nonempty[rng.next() as usize % nonempty.len()];
            net.deliver(i, j);
        }
        step += 1;
        let all_started = starts.iter().all(|s| s.0 < step);
        let all_done = starts.iter().all(|s| net.complete(s.2));
        // Run past the traffic cutoff so channels drain and every marker is delivered.
        if all_started && step > 400 && all_done {
            break;
        }
        assert!(step < 100_000, "simulation must terminate");
    }

    // Verify every snapshot.
    for &(_, _, id) in &starts {
        assert!(net.complete(id));
        assert_eq!(
            net.markers_sent[&id],
            p * (p - 1),
            "exactly one marker per channel"
        );
        let r = collect(&net, id);
        assert_eq!(
            r.balances + r.in_flight,
            total,
            "seed {seed}: snapshot {id} must conserve money"
        );
        assert!(
            r.consistent,
            "seed {seed}: snapshot {id} must be a consistent cut"
        );
        stats.snapshots += 1;
        stats.recorded_in_flight += r.in_flight;
        stats.snapshots_with_in_flight += (r.in_flight > 0) as u64;
        if verbose {
            println!(
                "  snapshot {id}: balances {:?} + in-flight {} = {}",
                net.procs
                    .iter()
                    .map(|q| q.snaps[&id].balance)
                    .collect::<Vec<_>>(),
                r.in_flight,
                r.balances + r.in_flight
            );
        }
    }
    // Real money is also conserved at the end (sanity of the simulation itself).
    assert_eq!(
        net.procs.iter().map(|q| q.balance).sum::<u64>() + net.in_flight_money(),
        total
    );

    // The naive reading: valid arithmetic only if nothing moved between the reads.
    stats.naive_sum_broken = naive_balance.iter().sum::<u64>() != total;
    stats.naive_cut_inconsistent = !consistent_cut(&naive_vc);
    stats
}

fn main() {
    println!("one run in detail (seed 7):");
    let _ = run(7, true);

    let mut agg = RunStats::default();
    let (mut naive_sum, mut naive_cut, mut instant) = (0u32, 0u32, 0u32);
    let runs = 2000u64;
    for seed in 1..=runs {
        let s = run(seed, false);
        agg.snapshots += s.snapshots;
        agg.initiators += s.initiators;
        agg.recorded_in_flight += s.recorded_in_flight;
        agg.snapshots_with_in_flight += s.snapshots_with_in_flight;
        naive_sum += s.naive_sum_broken as u32;
        naive_cut += s.naive_cut_inconsistent as u32;
        instant += s.instant_sum_broken as u32;
    }
    println!(
        "\n{runs} random runs, {} snapshots (1-3 concurrent initiators per run):",
        agg.snapshots
    );
    println!(
        "  Chandy-Lamport: money conserved and cut consistent in {}/{} snapshots",
        agg.snapshots, agg.snapshots
    );
    println!(
        "  snapshots that captured in-flight money: {} (avg {:.1} tokens in channels per snapshot)",
        agg.snapshots_with_in_flight,
        agg.recorded_in_flight as f64 / agg.snapshots as f64
    );
    println!(
        "  naive staggered balance reads: sum != total in {naive_sum}/{runs} runs, inconsistent cut in {naive_cut}/{runs}"
    );
    println!(
        "  same-instant balances without channel state: sum != total in {instant}/{runs} runs (money in flight)"
    );
    assert!(
        agg.snapshots_with_in_flight > 0,
        "the in-flight part of the state really matters"
    );
    assert!(
        naive_sum > runs as u32 / 4,
        "naive reads usually break the invariant"
    );
    assert!(naive_cut > 0, "naive reads yield inconsistent cuts");
    assert!(instant > 0, "ignoring channel state loses money");
    println!("\nall snapshot invariants held");
}
