//! RECIPE 180 (systems): CPU scheduling policies: FCFS, SJF/SRTF, round-robin, priority+aging, MLFQ, DRR, stride
//! LEVEL: advanced
//! Use in: OS/runtime scheduler design, task executors, network schedulers, interview prep.   Run: cargo run --release --example scheduler_policies
#![allow(dead_code)]
//!
//! DOMAIN: operating-system style processes (arrival time + CPU burst), NOT a DAG workflow engine. A deterministic
//! unit-time simulator asks a `Policy` "who runs this tick?" and records, per process:
//!   turnaround = finish - arrival;  waiting = turnaround - burst;  response = first_run - arrival.
//! Jain's fairness index J = (sum x)^2 / (n * sum x^2) lies in (1/n, 1]; 1 = perfectly equal. Here x_i = burst_i /
//! turnaround_i (the share of "wall time" each process effectively got); for weighted policies x_i = service_i / weight_i.
//!
//! POLICIES: FCFS (non-preemptive), SJF (non-preemptive shortest job), SRTF (preemptive; provably minimises mean
//! waiting time when bursts are known), RR(quantum), static PRIORITY with AGING (effective priority improves while
//! waiting, so nothing starves), MLFQ (new jobs start at the top queue, burning a full quantum demotes, periodic boost
//! rescues starved or phase-changed jobs), DRR (deficit round robin: each round a flow earns `quantum * weight`
//! credit, spends it per tick, unspent credit carries over) and STRIDE (a weighted-fair-queuing style virtual-time
//! scheduler: run the process with the smallest `pass`, then `pass += BIG / weight`).
//!
//! COMMON USES: SRTF/SJF reasoning appears in shortest-job-first request scheduling and tail-latency work; round
//! robin and MLFQ ideas underlie time-sharing OS schedulers (classic Unix, BSD and Solaris style; Linux's CFS/EEVDF
//! is a weighted-fair design in the same family as stride/WFQ); deficit round robin is widely used in network
//! packet schedulers and fair multi-tenant queues; aging is the standard cure for starvation in priority queues.
//! Read: https://pages.cs.wisc.edu/~remzi/OSTEP/cpu-sched.pdf and https://pages.cs.wisc.edu/~remzi/OSTEP/cpu-sched-mlfq.pdf
//! and https://pages.cs.wisc.edu/~remzi/OSTEP/cpu-sched-lottery.pdf (stride scheduling is covered there)
use std::collections::VecDeque;

#[derive(Clone, Debug)]
struct Proc {
    pid: usize,
    arrival: u64,
    burst: u64,
    remaining: u64,
    prio: i64, // lower = more important
    weight: u64,
    first_run: Option<u64>,
    finish: Option<u64>,
    ran: u64,
}
impl Proc {
    fn new(pid: usize, arrival: u64, burst: u64, prio: i64, weight: u64) -> Self {
        Proc {
            pid,
            arrival,
            burst,
            remaining: burst,
            prio,
            weight,
            first_run: None,
            finish: None,
            ran: 0,
        }
    }
    fn ready(&self, now: u64) -> bool {
        self.arrival <= now && self.remaining > 0
    }
}

trait Policy {
    fn name(&self) -> String;
    fn arrive(&mut self, _pid: usize, _now: u64, _procs: &[Proc]) {}
    /// Called every tick; the choice must be a ready process (or None = idle).
    fn pick(&mut self, now: u64, procs: &[Proc]) -> Option<usize>;
    /// Called after the tick in which `pid` ran.
    fn ran(&mut self, _pid: usize, _now: u64, _finished: bool, _procs: &[Proc]) {}
}

struct Fcfs {
    q: VecDeque<usize>,
    cur: Option<usize>,
}
impl Policy for Fcfs {
    fn name(&self) -> String {
        "FCFS".into()
    }
    fn arrive(&mut self, pid: usize, _: u64, _: &[Proc]) {
        self.q.push_back(pid)
    }
    fn pick(&mut self, _: u64, _: &[Proc]) -> Option<usize> {
        if self.cur.is_none() {
            self.cur = self.q.pop_front();
        }
        self.cur
    }
    fn ran(&mut self, _: usize, _: u64, finished: bool, _: &[Proc]) {
        if finished {
            self.cur = None
        }
    }
}

struct Sjf {
    preemptive: bool,
    cur: Option<usize>,
}
impl Policy for Sjf {
    fn name(&self) -> String {
        if self.preemptive {
            "SRTF".into()
        } else {
            "SJF".into()
        }
    }
    fn pick(&mut self, now: u64, procs: &[Proc]) -> Option<usize> {
        if self.cur.is_some() && !self.preemptive {
            return self.cur;
        }
        let key = |p: &&Proc| {
            if self.preemptive {
                (p.remaining, p.pid)
            } else {
                (p.burst, p.pid)
            }
        };
        self.cur = procs
            .iter()
            .filter(|p| p.ready(now))
            .min_by_key(key)
            .map(|p| p.pid);
        self.cur
    }
    fn ran(&mut self, _: usize, _: u64, finished: bool, _: &[Proc]) {
        if finished {
            self.cur = None
        }
    }
}

struct RoundRobin {
    quantum: u64,
    q: VecDeque<usize>,
    cur: Option<usize>,
    used: u64,
}
impl Policy for RoundRobin {
    fn name(&self) -> String {
        format!("RR(q={})", self.quantum)
    }
    fn arrive(&mut self, pid: usize, _: u64, _: &[Proc]) {
        self.q.push_back(pid)
    }
    fn pick(&mut self, _: u64, _: &[Proc]) -> Option<usize> {
        if self.cur.is_none() {
            self.cur = self.q.pop_front();
            self.used = 0;
        }
        self.cur
    }
    fn ran(&mut self, pid: usize, _: u64, finished: bool, _: &[Proc]) {
        self.used += 1;
        if finished {
            self.cur = None;
        } else if self.used == self.quantum {
            self.q.push_back(pid);
            self.cur = None;
        }
    }
}

/// Preemptive static priority. With `aging`, effective = prio * period - waited, so any waiter eventually wins.
struct Priority {
    aging: bool,
    period: i64,
    waited: Vec<i64>,
}
impl Policy for Priority {
    fn name(&self) -> String {
        format!("PRIO{}", if self.aging { "+aging" } else { "" })
    }
    fn pick(&mut self, now: u64, procs: &[Proc]) -> Option<usize> {
        let eff = |p: &&Proc| {
            (
                p.prio * self.period - if self.aging { self.waited[p.pid] } else { 0 },
                p.pid,
            )
        };
        procs
            .iter()
            .filter(|p| p.ready(now))
            .min_by_key(eff)
            .map(|p| p.pid)
    }
    fn ran(&mut self, pid: usize, now: u64, _: bool, procs: &[Proc]) {
        for p in procs.iter().filter(|p| p.ready(now) && p.pid != pid) {
            self.waited[p.pid] += 1;
        }
        self.waited[pid] = 0;
    }
}

struct Mlfq {
    quanta: Vec<u64>,
    boost_every: u64,
    qs: Vec<VecDeque<usize>>,
    cur: Option<(usize, usize, u64)>, // (pid, level, used)
}
impl Mlfq {
    fn new(boost_every: u64) -> Self {
        let quanta = vec![2, 4, 8, 16];
        Mlfq {
            qs: vec![VecDeque::new(); quanta.len()],
            quanta,
            boost_every,
            cur: None,
        }
    }
}
impl Policy for Mlfq {
    fn name(&self) -> String {
        "MLFQ".into()
    }
    fn arrive(&mut self, pid: usize, _: u64, _: &[Proc]) {
        self.qs[0].push_back(pid)
    }
    fn pick(&mut self, now: u64, _: &[Proc]) -> Option<usize> {
        if now > 0 && now % self.boost_every == 0 {
            // priority boost: everyone back to the top queue (prevents starvation of CPU-bound jobs)
            let mut all: Vec<usize> = self.cur.take().map(|(p, _, _)| p).into_iter().collect();
            for q in self.qs.iter_mut() {
                all.extend(q.drain(..));
            }
            self.qs[0].extend(all);
        }
        if let Some((pid, lvl, _)) = self.cur {
            if self.qs[..lvl].iter().any(|q| !q.is_empty()) {
                self.qs[lvl].push_back(pid); // a higher queue has work: preempt
                self.cur = None;
            }
        }
        if self.cur.is_none() {
            for (lvl, q) in self.qs.iter_mut().enumerate() {
                if let Some(pid) = q.pop_front() {
                    self.cur = Some((pid, lvl, 0));
                    break;
                }
            }
        }
        self.cur.map(|c| c.0)
    }
    fn ran(&mut self, pid: usize, _: u64, finished: bool, _: &[Proc]) {
        let (_, lvl, used) = self.cur.unwrap();
        if finished {
            self.cur = None;
        } else if used + 1 >= self.quanta[lvl] {
            let nl = (lvl + 1).min(self.quanta.len() - 1); // used the whole slice: demote
            self.qs[nl].push_back(pid);
            self.cur = None;
        } else {
            self.cur = Some((pid, lvl, used + 1));
        }
    }
}

/// Deficit round robin. One tick of CPU "costs" COST credit; each visit adds `base * weight` credit.
const COST: u64 = 2;
struct Drr {
    base: u64,
    q: VecDeque<usize>,
    deficit: Vec<u64>,
    cur: Option<usize>,
}
impl Policy for Drr {
    fn name(&self) -> String {
        "DRR".into()
    }
    fn arrive(&mut self, pid: usize, _: u64, _: &[Proc]) {
        self.q.push_back(pid)
    }
    fn pick(&mut self, _: u64, procs: &[Proc]) -> Option<usize> {
        while self.cur.is_none() {
            let pid = self.q.pop_front()?;
            self.deficit[pid] += self.base * procs[pid].weight;
            if self.deficit[pid] >= COST {
                self.cur = Some(pid);
            } else {
                self.q.push_back(pid);
            }
        }
        self.cur
    }
    fn ran(&mut self, pid: usize, _: u64, finished: bool, _: &[Proc]) {
        self.deficit[pid] -= COST;
        if finished {
            self.deficit[pid] = 0;
            self.cur = None;
        } else if self.deficit[pid] < COST {
            self.q.push_back(pid); // leftover credit (< COST) is carried to the next round
            self.cur = None;
        }
    }
}

/// Stride scheduling = virtual-time weighted fair queuing with a one-tick quantum.
struct Stride {
    pass: Vec<u64>,
}
const BIG: u64 = 1 << 20;
impl Policy for Stride {
    fn name(&self) -> String {
        "STRIDE".into()
    }
    fn arrive(&mut self, pid: usize, now: u64, procs: &[Proc]) {
        // join at the current minimum pass so latecomers neither monopolise nor starve
        let min = procs
            .iter()
            .filter(|p| p.ready(now) && p.pid != pid)
            .map(|p| self.pass[p.pid])
            .min();
        self.pass[pid] = min.unwrap_or(0);
    }
    fn pick(&mut self, now: u64, procs: &[Proc]) -> Option<usize> {
        procs
            .iter()
            .filter(|p| p.ready(now))
            .min_by_key(|p| (self.pass[p.pid], p.pid))
            .map(|p| p.pid)
    }
    fn ran(&mut self, pid: usize, _: u64, _: bool, procs: &[Proc]) {
        self.pass[pid] += BIG / procs[pid].weight;
    }
}

struct Report {
    procs: Vec<Proc>,
    makespan: u64,
    timeline: Vec<Option<usize>>,
}
impl Report {
    fn mean(&self, f: impl Fn(&Proc) -> u64) -> f64 {
        self.procs.iter().map(|p| f(p) as f64).sum::<f64>() / self.procs.len() as f64
    }
    fn turnaround(p: &Proc) -> u64 {
        p.finish.unwrap() - p.arrival
    }
    fn waiting(p: &Proc) -> u64 {
        Self::turnaround(p) - p.burst
    }
    fn response(p: &Proc) -> u64 {
        p.first_run.unwrap() - p.arrival
    }
    fn max_response(&self) -> u64 {
        self.procs.iter().map(Self::response).max().unwrap()
    }
    fn jain(&self) -> f64 {
        jain(
            &self
                .procs
                .iter()
                .map(|p| p.burst as f64 / Self::turnaround(p) as f64)
                .collect::<Vec<_>>(),
        )
    }
}

fn jain(x: &[f64]) -> f64 {
    let s: f64 = x.iter().sum();
    s * s / (x.len() as f64 * x.iter().map(|v| v * v).sum::<f64>())
}

fn simulate(policy: &mut dyn Policy, mut procs: Vec<Proc>, horizon: Option<u64>) -> Report {
    let mut timeline = Vec::new();
    let mut now = 0;
    while procs.iter().any(|p| p.remaining > 0) && horizon.is_none_or(|h| now < h) {
        for i in 0..procs.len() {
            if procs[i].arrival == now {
                policy.arrive(i, now, &procs);
            }
        }
        let pick = policy.pick(now, &procs);
        if let Some(pid) = pick {
            assert!(
                procs[pid].ready(now),
                "{} picked a non-ready process",
                policy.name()
            );
            let p = &mut procs[pid];
            p.first_run.get_or_insert(now);
            p.remaining -= 1;
            p.ran += 1;
            let done = p.remaining == 0;
            if done {
                p.finish = Some(now + 1);
            }
            policy.ran(pid, now, done, &procs);
        }
        timeline.push(pick);
        now += 1;
    }
    Report {
        procs,
        makespan: now,
        timeline,
    }
}

struct Rng(u64);
impl Rng {
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        lo + self.0 % (hi - lo + 1)
    }
}

fn cpu_bound_mix(seed: u64) -> Vec<Proc> {
    let mut r = Rng(seed);
    (0..12)
        .map(|i| Proc::new(i, r.range(0, 20), r.range(5, 60), (i % 4) as i64, 1))
        .collect()
}
fn interactive_mix(seed: u64) -> Vec<Proc> {
    let mut r = Rng(seed);
    let mut v: Vec<(u64, u64)> = (0..6).map(|_| (r.range(0, 10), r.range(40, 80))).collect();
    v.extend((0..24).map(|_| (r.range(5, 200), r.range(1, 6))));
    v.into_iter()
        .enumerate()
        .map(|(i, (a, b))| Proc::new(i, a, b, (i % 4) as i64, 1))
        .collect()
}
fn one_hog(seed: u64) -> Vec<Proc> {
    let mut r = Rng(seed);
    let mut v = vec![Proc::new(0, 0, 400, 0, 1)];
    v.extend((1..=20).map(|i| Proc::new(i, r.range(5, 100), r.range(2, 5), (i % 4) as i64, 1)));
    v
}

fn policies(n: usize) -> Vec<Box<dyn Policy>> {
    vec![
        Box::new(Fcfs {
            q: VecDeque::new(),
            cur: None,
        }),
        Box::new(Sjf {
            preemptive: false,
            cur: None,
        }),
        Box::new(Sjf {
            preemptive: true,
            cur: None,
        }),
        Box::new(RoundRobin {
            quantum: 4,
            q: VecDeque::new(),
            cur: None,
            used: 0,
        }),
        Box::new(Priority {
            aging: true,
            period: 10,
            waited: vec![0; n],
        }),
        Box::new(Mlfq::new(100)),
        Box::new(Drr {
            base: 2,
            q: VecDeque::new(),
            deficit: vec![0; n],
            cur: None,
        }),
        Box::new(Stride { pass: vec![0; n] }),
    ]
}

fn main() {
    let workloads: Vec<(&str, Vec<Proc>)> = vec![
        ("cpu-bound mix", cpu_bound_mix(42)),
        ("interactive mix", interactive_mix(7)),
        ("one hog", one_hog(99)),
    ];
    for (wname, procs) in &workloads {
        println!(
            "\n== workload: {wname} ({} procs, total burst {}) ==",
            procs.len(),
            procs.iter().map(|p| p.burst).sum::<u64>()
        );
        println!(
            "{:<10}{:>9}{:>9}{:>9}{:>9}{:>8}{:>10}",
            "policy", "turnarnd", "waiting", "response", "max_resp", "Jain", "makespan"
        );
        let total: u64 = procs.iter().map(|p| p.burst).sum();
        let mut srtf_wait = 0.0;
        let mut rows = Vec::new();
        for mut pol in policies(procs.len()) {
            let r = simulate(pol.as_mut(), procs.clone(), None);
            // universal invariants
            assert!(
                r.procs
                    .iter()
                    .all(|p| p.remaining == 0 && p.finish.unwrap() >= p.arrival + p.burst)
            );
            assert_eq!(
                r.timeline.iter().flatten().count() as u64,
                total,
                "work conservation"
            );
            let name = pol.name();
            println!(
                "{:<10}{:>9.1}{:>9.1}{:>9.1}{:>9}{:>8.3}{:>10}",
                name,
                r.mean(Report::turnaround),
                r.mean(Report::waiting),
                r.mean(Report::response),
                r.max_response(),
                r.jain(),
                r.makespan
            );
            if name == "SRTF" {
                srtf_wait = r.mean(Report::waiting);
            }
            rows.push((name, r));
        }
        // SRTF minimises mean waiting time among ALL these policies (all are work-conserving, single CPU)
        for (name, r) in &rows {
            assert!(
                srtf_wait <= r.mean(Report::waiting) + 1e-9,
                "SRTF must not lose to {name} on {wname}"
            );
        }
        let fcfs = rows.iter().find(|r| r.0 == "FCFS").unwrap();
        assert!(srtf_wait <= fcfs.1.mean(Report::waiting));
        // round-robin bounded response: at most (n-1) other processes run one quantum before you first run
        let rr = rows.iter().find(|r| r.0.starts_with("RR")).unwrap();
        assert!(
            rr.1.max_response() <= (procs.len() as u64 - 1) * 4,
            "RR response bound on {wname}"
        );
        if *wname == "one hog" {
            assert!(
                rr.1.max_response() < fcfs.1.max_response() / 5,
                "RR rescues short jobs stuck behind the hog"
            );
            let mlfq = rows.iter().find(|r| r.0 == "MLFQ").unwrap();
            assert!(
                mlfq.1.mean(Report::waiting) < fcfs.1.mean(Report::waiting),
                "MLFQ demotes the hog, short jobs win"
            );
        }
    }

    // aging prevents starvation: a stream of high-priority jobs would otherwise keep the CPU busy forever
    let mut stream: Vec<Proc> = (0..100)
        .map(|i| Proc::new(i, i as u64 * 5, 5, 0, 1))
        .collect();
    stream.push(Proc::new(100, 0, 10, 2, 1)); // the low-priority victim
    let n = stream.len();
    let no_aging = simulate(
        &mut Priority {
            aging: false,
            period: 10,
            waited: vec![0; n],
        },
        stream.clone(),
        None,
    );
    let aging = simulate(
        &mut Priority {
            aging: true,
            period: 10,
            waited: vec![0; n],
        },
        stream,
        None,
    );
    let (f_no, f_ag) = (
        no_aging.procs[100].finish.unwrap(),
        aging.procs[100].finish.unwrap(),
    );
    println!(
        "\nstarvation test: low-prio job finishes at t={f_no} without aging, t={f_ag} with aging (stream ends ~t=500)"
    );
    // (wait resets each time the victim runs a tick, so it needs ~20 ticks of waiting per CPU tick it gets: slow but bounded)
    assert!(
        f_no >= 500 && f_ag < 300,
        "aging must bound the victim's completion"
    );

    // DRR / stride: CPU share proportional to weight/quantum while everyone is backlogged
    let weights = [1u64, 2, 4];
    let mk = || -> Vec<Proc> {
        weights
            .iter()
            .enumerate()
            .map(|(i, &w)| Proc::new(i, 0, 100_000, 0, w))
            .collect()
    };
    for (label, mut pol) in [
        (
            "DRR",
            Box::new(Drr {
                base: 3,
                q: VecDeque::new(),
                deficit: vec![0; 3],
                cur: None,
            }) as Box<dyn Policy>,
        ),
        ("STRIDE", Box::new(Stride { pass: vec![0; 3] })),
    ] {
        let r = simulate(pol.as_mut(), mk(), Some(7_000));
        let total: u64 = r.procs.iter().map(|p| p.ran).sum();
        let shares: Vec<f64> = r
            .procs
            .iter()
            .map(|p| p.ran as f64 / total as f64)
            .collect();
        let expect: Vec<f64> = weights.iter().map(|&w| w as f64 / 7.0).collect();
        println!(
            "{label} shares for weights {weights:?}: {:.3?} (expected {:.3?}) Jain(service/weight)={:.4}",
            shares,
            expect,
            jain(
                &r.procs
                    .iter()
                    .map(|p| p.ran as f64 / p.weight as f64)
                    .collect::<Vec<_>>()
            )
        );
        for (s, e) in shares.iter().zip(&expect) {
            assert!((s - e).abs() < 0.02, "{label} share {s} too far from {e}");
        }
    }
    println!("\nr180_scheduler_policies: all assertions passed");
}
