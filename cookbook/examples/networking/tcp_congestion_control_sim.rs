//! RECIPE 431 (network): discrete-event simulation of TCP congestion control (Reno, NewReno, CUBIC, simplified BBR) over a drop-tail bottleneck
//! LEVEL: advanced
//! Use in: understanding why transfers are slow, bufferbloat, tuning sysctls, building congestion control for QUIC/UDP protocols.   Run: cargo run --release --example tcp_congestion_control_sim
#![allow(dead_code)]
//!
//! WHAT: a packet-level simulator in virtual microseconds. Topology: N senders -> one bottleneck link
//! (1000 packets/s = 10 Mbit/s at 1250 B/packet, a FIFO DROP-TAIL queue of B packets, optional random loss)
//! -> 20 ms propagation -> receivers -> 20 ms back (acks are never lost). Base RTT = 41 ms, so the
//! bandwidth-delay product (BDP) is about 41 packets; a buffer of one BDP is the classic rule of thumb.
//!
//! FLOW CONTROL vs CONGESTION CONTROL: flow control protects the RECEIVER: it advertises a window (rwnd, how
//! much buffer it has left) and the sender never has more than rwnd unacked. Congestion control protects the
//! NETWORK: the sender keeps its own window (cwnd) and learns the safe rate from loss/delay signals. The
//! sender may have min(cwnd, rwnd) packets in flight. Section 1 shows the receiver window capping a flow
//! at rwnd/RTT no matter how large cwnd grows.
//!
//! THE ALGORITHMS (all window-based, in packets):
//!  * SLOW START: cwnd += 1 per ack, i.e. doubles every RTT, until loss or ssthresh.
//!  * CONGESTION AVOIDANCE (AIMD): cwnd += 1/cwnd per ack = +1 per RTT; on loss halve. Additive increase and
//!    multiplicative decrease converge to a fair share (Chiu & Jain): equal flows end up equal.
//!  * FAST RETRANSMIT / FAST RECOVERY: 3 duplicate acks mean one packet was lost but later ones got through,
//!    so resend it at once and set ssthresh = flight/2 instead of waiting for the RTO and restarting at 1.
//!    RENO leaves recovery on the first new ack, so several losses in one window usually cost a timeout;
//!    NEWRENO stays in recovery until everything outstanding at loss time is acked, resending one hole per
//!    partial ack (RFC 6582).
//!  * CUBIC: after a loss (beta = 0.7) the window follows W(t) = C (t-K)^3 + Wmax: fast climb back, a plateau
//!    near the old maximum Wmax (K seconds later), then probing beyond it. Growth depends on elapsed time, not
//!    RTT, so long-RTT flows are not starved. Default in Linux.
//!  * BBR (simplified): model-based, not loss-based. Estimates bottleneck bandwidth (windowed max of delivery
//!    rate) and min RTT, paces at gain*btlbw and caps inflight at 2*BDP. STARTUP (gain 2.885) -> DRAIN ->
//!    PROBE_BW (gain cycle 1.25, 0.75, 1 x6). Omitted: ProbeRTT, app-limited handling, ack aggregation.
//!  * LOSS RECOVERY DETAIL: Reno runs without SACK (RFC 5681 baseline: one hole per RTT, timeouts on multi-loss);
//!    NewReno, CUBIC and BBR run with a SACK scoreboard (3 sack blocks per ack, a hole is lost when 3 packets
//!    above it are sacked, a retransmission outstanding for 2 RTTs is presumed lost), like every modern stack.
//!  * RTO: RFC 6298 estimator with exponential backoff and Karn's rule (see udp_sockets_reliable), with a 200 ms floor
//!    (the RFC says 1 s).
//!
//! METRICS: utilisation (delivered/capacity), mean queue length seen by arriving packets (BUFFERBLOAT: a deep
//! queue keeps the link full but adds latency), RTT inflation (mean RTT / base RTT), loss rate, Jain's fairness
//! index J = (sum x)^2 / (n sum x^2) (1 = perfectly equal). Asserted: slow start doubles per RTT, NewReno's sawtooth
//! halves the window on loss, equal-RTT NewReno flows share fairly, Reno's throughput follows Mathis' 1/sqrt(p) law.
//! The simulator is deterministic (seeded), so the numbers printed are reproducible.
//!
//! COMMON USES: Linux CUBIC/BBR tuning, QUIC congestion controllers (quinn, Google's QUIC), datacenter variants
//! (DCTCP), debugging bufferbloat (fq_codel/CAKE exist because of it), choosing buffer sizes for routers.
//! Read: RFC 5681 (TCP congestion control) https://www.rfc-editor.org/rfc/rfc5681 ; RFC 6582 (NewReno)
//! https://www.rfc-editor.org/rfc/rfc6582 ; RFC 9438 (CUBIC) https://www.rfc-editor.org/rfc/rfc9438 ;
//! RFC 6298 (RTO) https://www.rfc-editor.org/rfc/rfc6298 ; Mathis et al. 1997, "The Macroscopic Behavior of the
//! TCP Congestion Avoidance Algorithm"; Cardwell et al., "BBR: Congestion-Based Congestion Control" (ACM Queue 2016).
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque};

const ST: u64 = 1000; // microseconds to serialise one packet at the bottleneck (1000 packets/s)
const PROP: u64 = 20_000; // one-way propagation delay in microseconds
const BASE_RTT: f64 = (2 * PROP + ST) as f64; // microseconds
const LINK_PPS: f64 = 1e6 / ST as f64;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn chance(&mut self, p: f64) -> bool {
        p > 0.0 && ((self.next() >> 11) as f64 / (1u64 << 53) as f64) < p
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Algo {
    Reno,
    NewReno,
    Cubic,
    Bbr,
}

impl Algo {
    fn name(self) -> &'static str {
        match self {
            Algo::Reno => "Reno",
            Algo::NewReno => "NewReno",
            Algo::Cubic => "CUBIC",
            Algo::Bbr => "BBR",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Ev {
    Start(usize),
    Pace(usize),
    Deliver(usize, u64),
    Ack(usize, u64, [(u64, u64); 3]), // cumulative ack + up to 3 SACK blocks [start, end)
    Rto(usize, u64),
    Mark,
}

#[derive(Clone, Copy)]
struct TxInfo {
    t: u64,
    retx: bool,
    deliv: u64,   // BBR: packets acked when this one was sent
    deliv_t: u64, // BBR: time of the latest delivery when this one was sent
}

/// Simplified BBR state (see header for what is omitted).
struct Bbr {
    mode: u8,   // 0 startup, 1 drain, 2 probe_bw
    btlbw: f64, // packets per microsecond
    hist: VecDeque<(u64, f64)>,
    min_rtt: f64,
    round: u64,
    next_round_deliv: u64,
    full_bw: f64,
    full_cnt: u32,
    cycle: usize,
    cycle_start: u64,
    pacing_gain: f64,
    cwnd_gain: f64,
}

const BBR_GAINS: [f64; 8] = [1.25, 0.75, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
const STARTUP_GAIN: f64 = 2.885;

impl Bbr {
    fn new() -> Self {
        Bbr {
            mode: 0,
            btlbw: 100.0 / 1e6,
            hist: VecDeque::new(),
            min_rtt: f64::INFINITY,
            round: 0,
            next_round_deliv: 0,
            full_bw: 0.0,
            full_cnt: 0,
            cycle: 2,
            cycle_start: 0,
            pacing_gain: STARTUP_GAIN,
            cwnd_gain: STARTUP_GAIN,
        }
    }
    fn bdp(&self) -> f64 {
        if self.min_rtt.is_finite() {
            self.btlbw * self.min_rtt
        } else {
            0.0
        }
    }
    fn cwnd(&self) -> f64 {
        if self.min_rtt.is_finite() {
            (self.cwnd_gain * self.bdp()).max(4.0)
        } else {
            10.0
        }
    }
}

struct Flow {
    algo: Algo,
    start: u64,
    rwnd: f64,
    // sender
    una: u64,
    nxt: u64,
    high: u64, // highest sequence ever sent + 1 (below it a send is a retransmission)
    cwnd: f64,
    ssthresh: f64,
    dups: u32,
    in_rec: bool,
    recover: u64,
    srtt: f64,
    rttvar: f64,
    rto_base: u64,
    backoff: u32,
    rto_gen: u64,
    timer_armed: bool,
    tx: BTreeMap<u64, TxInfo>,
    next_send: u64,
    pace_pending: bool,
    // receiver
    rcv_next: u64,
    ooo: BTreeSet<u64>,
    // SACK scoreboard (every algorithm except classic Reno, like modern stacks; Reno shows the no-SACK crawl)
    sack: bool,
    sacked: BTreeSet<u64>,
    rexmit: BTreeSet<u64>,
    // cubic
    w_max: f64,
    epoch: Option<u64>,
    k: f64,
    // bbr / delivery accounting
    bbr: Bbr,
    acked_total: u64,
    last_deliv_t: u64,
    // stats
    mark: u64,
    rtt_sum: f64,
    rtt_n: u64,
    retx: u64,
    timeouts: u64,
    drops: u64,
    sent: u64,
    loss_events: Vec<(u64, f64, f64)>, // (time, cwnd before, ssthresh after)
    trace: Vec<(u64, f64)>,
}

impl Flow {
    fn new(algo: Algo, start: u64, rwnd: f64) -> Self {
        Flow {
            algo,
            start,
            rwnd,
            una: 0,
            nxt: 0,
            high: 0,
            cwnd: 2.0,
            ssthresh: f64::INFINITY,
            dups: 0,
            in_rec: false,
            recover: 0,
            srtt: 0.0,
            rttvar: 0.0,
            rto_base: 1_000_000,
            backoff: 0,
            rto_gen: 0,
            timer_armed: false,
            tx: BTreeMap::new(),
            next_send: 0,
            pace_pending: false,
            rcv_next: 0,
            ooo: BTreeSet::new(),
            sack: algo != Algo::Reno,
            sacked: BTreeSet::new(),
            rexmit: BTreeSet::new(),
            w_max: 0.0,
            epoch: None,
            k: 0.0,
            bbr: Bbr::new(),
            acked_total: 0,
            last_deliv_t: 0,
            mark: 0,
            rtt_sum: 0.0,
            rtt_n: 0,
            retx: 0,
            timeouts: 0,
            drops: 0,
            sent: 0,
            loss_events: Vec::new(),
            trace: Vec::new(),
        }
    }
    fn cwnd_eff(&self) -> f64 {
        if self.algo == Algo::Bbr {
            self.bbr.cwnd()
        } else {
            self.cwnd
        }
    }
}

struct NetCfg {
    buffer: usize,
    loss: f64,
}

struct Sim {
    now: u64,
    heap: BinaryHeap<Reverse<(u64, u64, Ev)>>,
    order: u64,
    rng: Rng,
    cfg: NetCfg,
    flows: Vec<Flow>,
    dep: VecDeque<u64>,
    link_free: u64,
    q_sum: f64,
    q_n: u64,
    q_max: usize,
    arrivals: u64,
    drops: u64,
}

const C_CUBIC: f64 = 0.4;
const BETA_CUBIC: f64 = 0.7;

impl Sim {
    fn new(cfg: NetCfg, flows: Vec<Flow>, seed: u64) -> Self {
        Sim {
            now: 0,
            heap: BinaryHeap::new(),
            order: 0,
            rng: Rng(seed),
            cfg,
            flows,
            dep: VecDeque::new(),
            link_free: 0,
            q_sum: 0.0,
            q_n: 0,
            q_max: 0,
            arrivals: 0,
            drops: 0,
        }
    }
    fn at(&mut self, t: u64, ev: Ev) {
        self.order += 1;
        self.heap.push(Reverse((t, self.order, ev)));
    }

    fn run(&mut self, duration_us: u64) {
        for i in 0..self.flows.len() {
            let s = self.flows[i].start;
            self.at(s, Ev::Start(i));
        }
        self.at(duration_us / 2, Ev::Mark);
        while let Some(Reverse((t, _, ev))) = self.heap.pop() {
            if t > duration_us {
                break;
            }
            self.now = t;
            self.dispatch(ev);
        }
    }

    fn dispatch(&mut self, ev: Ev) {
        let t = self.now;
        match ev {
            Ev::Start(f) => self.try_send(f),
            Ev::Pace(f) => {
                self.flows[f].pace_pending = false;
                self.try_send(f);
            }
            Ev::Deliver(f, seq) => {
                let fl = &mut self.flows[f];
                if seq == fl.rcv_next {
                    fl.rcv_next += 1;
                    while fl.ooo.remove(&fl.rcv_next) {
                        fl.rcv_next += 1;
                    }
                } else if seq > fl.rcv_next {
                    fl.ooo.insert(seq);
                }
                let cum = fl.rcv_next;
                let mut blocks = [(0u64, 0u64); 3];
                if fl.sack && !fl.ooo.is_empty() {
                    let mut ranges: Vec<(u64, u64)> = Vec::new();
                    for &q in &fl.ooo {
                        match ranges.last_mut() {
                            Some(r) if r.1 == q => r.1 = q + 1,
                            _ => ranges.push((q, q + 1)),
                        }
                    }
                    // newest information first: the block holding the packet that triggered this ack, then highest blocks
                    ranges.sort_by_key(|r| (!(r.0 <= seq && seq < r.1), Reverse(r.0)));
                    for (i, r) in ranges.into_iter().take(3).enumerate() {
                        blocks[i] = r;
                    }
                }
                self.at(t + PROP, Ev::Ack(f, cum, blocks));
            }
            Ev::Ack(f, cum, blocks) => self.on_ack(f, cum, blocks),
            Ev::Rto(f, g) => {
                if self.flows[f].rto_gen == g && self.flows[f].timer_armed {
                    self.on_timeout(f);
                }
            }
            Ev::Mark => {
                for fl in &mut self.flows {
                    fl.mark = fl.rcv_next;
                }
            }
        }
    }

    /// A packet arrives at the bottleneck: random loss, then drop-tail, else it is serialised behind the queue.
    fn enqueue(&mut self, f: usize, seq: u64) {
        while let Some(&d) = self.dep.front() {
            if d <= self.now {
                self.dep.pop_front();
            } else {
                break;
            }
        }
        let qlen = self.dep.len();
        self.q_sum += qlen as f64;
        self.q_n += 1;
        self.q_max = self.q_max.max(qlen);
        self.arrivals += 1;
        if self.rng.chance(self.cfg.loss) || qlen >= self.cfg.buffer {
            self.drops += 1;
            self.flows[f].drops += 1;
            return;
        }
        let depart = self.now.max(self.link_free) + ST;
        self.link_free = depart;
        self.dep.push_back(depart);
        self.at(depart + PROP, Ev::Deliver(f, seq));
    }

    fn send_packet(&mut self, f: usize, seq: u64, retx: bool) {
        let now = self.now;
        let fl = &mut self.flows[f];
        fl.tx.insert(
            seq,
            TxInfo {
                t: now,
                retx,
                deliv: fl.acked_total,
                deliv_t: fl.last_deliv_t,
            },
        );
        fl.sent += 1;
        if retx {
            fl.retx += 1;
        }
        let arm = !fl.timer_armed;
        self.enqueue(f, seq);
        if arm {
            self.arm_timer(f);
        }
    }

    fn arm_timer(&mut self, f: usize) {
        let fl = &mut self.flows[f];
        fl.rto_gen += 1;
        fl.timer_armed = true;
        let d = (fl.rto_base << fl.backoff.min(6)).min(60_000_000);
        let (g, t) = (fl.rto_gen, self.now + d);
        self.at(t, Ev::Rto(f, g));
    }

    /// Packets in the network ("pipe") and, with SACK in recovery, the lowest hole that deserves a retransmission
    /// (RFC 6675 style: an unsacked packet with >= 3 sacked packets above it is deemed lost).
    fn pipe_and_hole(fl: &Flow, now: u64) -> (f64, Option<u64>) {
        if !(fl.sack && fl.in_rec) {
            return ((fl.nxt - fl.una) as f64 - fl.dups as f64, None);
        }
        let (mut pipe, mut above, mut hole) = (0.0, 0, None);
        for q in (fl.una..fl.nxt).rev() {
            if fl.sacked.contains(&q) {
                above += 1;
            } else if above >= 3 {
                // A retransmission that has been outstanding for over 2 RTTs is presumed lost again (as RACK does).
                let fresh = fl.tx.get(&q).map_or(false, |i| {
                    ((now - i.t) as f64) < 2.0 * fl.srtt.max(BASE_RTT)
                });
                if fl.rexmit.contains(&q) && fresh {
                    pipe += 1.0;
                } else {
                    hole = Some(q);
                }
            } else {
                pipe += 1.0;
            }
        }
        (pipe, hole)
    }

    fn try_send(&mut self, f: usize) {
        loop {
            let now = self.now;
            let fl = &self.flows[f];
            let (pipe, hole) = Self::pipe_and_hole(fl, now);
            if pipe >= fl.cwnd_eff().min(fl.rwnd) {
                break;
            }
            if fl.algo == Algo::Bbr && now < fl.next_send {
                if !fl.pace_pending {
                    let t = fl.next_send;
                    self.flows[f].pace_pending = true;
                    self.at(t, Ev::Pace(f));
                }
                break;
            }
            if let Some(h) = hole {
                self.flows[f].rexmit.insert(h);
                self.send_packet(f, h, true);
            } else {
                let seq = fl.nxt;
                self.flows[f].nxt += 1;
                let again = seq < self.flows[f].high;
                self.flows[f].high = self.flows[f].high.max(seq + 1);
                self.send_packet(f, seq, again);
            }
            let fl = &mut self.flows[f];
            if fl.algo == Algo::Bbr {
                let rate = (fl.bbr.pacing_gain * fl.bbr.btlbw).max(1e-9);
                fl.next_send = now + (1.0 / rate) as u64;
            }
        }
        let now = self.now;
        let fl = &mut self.flows[f];
        if fl.trace.last().map_or(true, |&(t, _)| now >= t + 20_000) {
            let w = fl.cwnd_eff();
            fl.trace.push((now, w));
        }
    }

    fn on_timeout(&mut self, f: usize) {
        let now = self.now;
        let fl = &mut self.flows[f];
        if fl.una >= fl.nxt {
            fl.timer_armed = false;
            return;
        }
        fl.timeouts += 1;
        let flight = (fl.nxt - fl.una) as f64;
        match fl.algo {
            Algo::Cubic => {
                fl.w_max = fl.cwnd;
                fl.ssthresh = (fl.cwnd * BETA_CUBIC).max(2.0);
                fl.epoch = None;
            }
            Algo::Bbr => {}
            _ => fl.ssthresh = (flight / 2.0).max(2.0),
        }
        fl.loss_events.push((now, fl.cwnd, fl.ssthresh));
        fl.cwnd = 1.0;
        fl.backoff += 1;
        fl.dups = 0;
        fl.in_rec = false;
        fl.recover = fl.nxt;
        let seq = fl.una;
        if fl.sack {
            // SACK stacks keep the scoreboard: only unsacked holes are resent, not the whole window.
            fl.rexmit.clear();
            fl.rexmit.insert(seq);
            fl.in_rec = true;
            fl.recover = fl.nxt;
        } else {
            fl.nxt = fl.una + 1; // no SACK: go back and resend from the first unacked packet, slow-starting
        }
        self.send_packet(f, seq, true);
        self.arm_timer(f);
        self.try_send(f);
    }

    fn on_ack(&mut self, f: usize, cum: u64, blocks: [(u64, u64); 3]) {
        let now = self.now;
        let fl = &mut self.flows[f];
        if fl.sack {
            for &(s, e) in blocks.iter().filter(|b| b.1 > b.0) {
                for q in s.max(fl.una)..e {
                    if fl.sacked.insert(q) {
                        fl.acked_total += 1; // delivery is counted when a packet is SACKed, not when the hole below fills
                    }
                }
            }
        }
        if cum > fl.nxt {
            fl.nxt = cum;
        }
        if cum > fl.una {
            let acked = cum - fl.una;
            // The RTT sample comes from the FIRST newly acked packet: a cumulative ack that jumps over data that sat
            // in the receiver's out-of-order buffer would otherwise report the hole's repair time as RTT.
            let info = fl.tx.get(&fl.una).copied();
            fl.tx.retain(|&s, _| s >= cum);
            fl.una = cum;
            let before = fl.sacked.len();
            fl.sacked.retain(|&q| q >= cum);
            let already = (before - fl.sacked.len()) as u64; // these were counted when SACKed
            fl.rexmit.retain(|&q| q >= cum);
            fl.acked_total += acked - already;
            fl.last_deliv_t = now;
            fl.backoff = 0;
            if let Some(i) = info.filter(|i| !i.retx) {
                // Karn: only never-retransmitted packets give RTT samples. RFC 6298 estimator.
                let r = (now - i.t) as f64;
                fl.rtt_sum += r;
                fl.rtt_n += 1;
                if fl.srtt == 0.0 {
                    fl.srtt = r;
                    fl.rttvar = r / 2.0;
                } else {
                    fl.rttvar = 0.75 * fl.rttvar + 0.25 * (fl.srtt - r).abs();
                    fl.srtt = 0.875 * fl.srtt + 0.125 * r;
                }
                fl.rto_base = ((fl.srtt + 4.0 * fl.rttvar) as u64).clamp(200_000, 60_000_000);
            }
            if let (Algo::Bbr, Some(i)) = (fl.algo, info) {
                // Delivery-rate samples may use retransmitted packets (their record is rewritten on each send);
                // min RTT may not (Karn).
                let valid_rtt = (!i.retx).then(|| (now - i.t) as f64);
                Self::bbr_update(fl, now, valid_rtt, &i);
            }
            if fl.in_rec {
                if fl.algo != Algo::Reno && cum < fl.recover {
                    // NewReno partial ack: another hole; resend it, stay in recovery.
                    fl.dups = fl.dups.saturating_sub((acked - 1) as u32);
                    if !fl.sack {
                        let seq = fl.una;
                        self.send_packet(f, seq, true);
                    }
                } else {
                    fl.in_rec = false;
                    fl.rexmit.clear();
                    fl.dups = 0;
                    if fl.algo != Algo::Bbr {
                        fl.cwnd = fl.ssthresh;
                    }
                }
            } else {
                fl.dups = 0;
                Self::grow(fl, now, acked);
            }
            let fl = &mut self.flows[f];
            if fl.una < fl.nxt {
                self.arm_timer(f);
            } else {
                fl.timer_armed = false;
                fl.rto_gen += 1;
            }
        } else if cum == fl.una && fl.nxt > fl.una {
            fl.dups += 1;
            if fl.dups >= 3 && !fl.in_rec && (fl.algo == Algo::Reno || fl.una >= fl.recover) {
                let flight = (fl.nxt - fl.una) as f64;
                let before = fl.cwnd_eff();
                match fl.algo {
                    Algo::Cubic => {
                        fl.w_max = fl.cwnd;
                        fl.ssthresh = (fl.cwnd * BETA_CUBIC).max(2.0);
                        fl.epoch = None;
                    }
                    Algo::Bbr => {}
                    _ => fl.ssthresh = (flight / 2.0).max(2.0),
                }
                fl.loss_events.push((
                    now,
                    before,
                    if fl.algo == Algo::Bbr {
                        before
                    } else {
                        fl.ssthresh
                    },
                ));
                if fl.algo != Algo::Bbr {
                    fl.cwnd = fl.ssthresh;
                }
                fl.in_rec = true;
                fl.recover = fl.nxt;
                if !fl.sack {
                    let seq = fl.una;
                    self.send_packet(f, seq, true);
                }
            }
        }
        self.try_send(f);
    }

    /// Window growth on a new ack outside recovery.
    fn grow(fl: &mut Flow, now: u64, acked: u64) {
        match fl.algo {
            Algo::Reno | Algo::NewReno => {
                for _ in 0..acked {
                    fl.cwnd += if fl.cwnd < fl.ssthresh {
                        1.0
                    } else {
                        1.0 / fl.cwnd
                    };
                }
            }
            Algo::Cubic => {
                for _ in 0..acked {
                    if fl.cwnd < fl.ssthresh {
                        fl.cwnd += 1.0;
                        continue;
                    }
                    let epoch = *fl.epoch.get_or_insert_with(|| {
                        if fl.cwnd < fl.w_max {
                            fl.k = ((fl.w_max - fl.cwnd) / C_CUBIC).cbrt();
                        } else {
                            fl.k = 0.0;
                            fl.w_max = fl.cwnd;
                        }
                        now
                    });
                    let t = (now - epoch) as f64 / 1e6;
                    let rtt = fl.srtt / 1e6;
                    let target = C_CUBIC * (t + rtt - fl.k).powi(3) + fl.w_max;
                    // TCP-friendly region: never grow slower than Reno would from the same loss.
                    let w_est = fl.w_max * BETA_CUBIC
                        + 3.0 * (1.0 - BETA_CUBIC) / (1.0 + BETA_CUBIC) * t / rtt.max(1e-3);
                    let target = target.max(w_est);
                    fl.cwnd += if target > fl.cwnd {
                        ((target - fl.cwnd) / fl.cwnd).min(0.5)
                    } else {
                        0.01 / fl.cwnd
                    };
                }
            }
            Algo::Bbr => {}
        }
    }

    fn bbr_update(fl: &mut Flow, now: u64, rtt: Option<f64>, info: &TxInfo) {
        let flight = (fl.nxt - fl.una) as f64;
        let b = &mut fl.bbr;
        if let Some(r) = rtt {
            b.min_rtt = b.min_rtt.min(r);
        }
        let dt = (now - info.deliv_t) as f64;
        if dt > 0.0 {
            let rate = (fl.acked_total - info.deliv) as f64 / dt;
            if info.deliv >= b.next_round_deliv {
                b.round += 1;
                b.next_round_deliv = fl.acked_total;
                if b.mode == 0 {
                    // full pipe detection: bandwidth stopped growing by 25% for 3 rounds
                    if b.btlbw >= b.full_bw * 1.25 {
                        b.full_bw = b.btlbw;
                        b.full_cnt = 0;
                    } else {
                        b.full_cnt += 1;
                        if b.full_cnt >= 3 {
                            b.mode = 1;
                            b.pacing_gain = 1.0 / STARTUP_GAIN;
                            b.cwnd_gain = STARTUP_GAIN;
                        }
                    }
                }
            }
            b.hist.push_back((b.round, rate));
            while b.hist.front().map_or(false, |&(r, _)| r + 10 < b.round) {
                b.hist.pop_front();
            }
            b.btlbw = b.hist.iter().map(|&(_, r)| r).fold(0.0, f64::max);
        }
        if b.mode == 1 && flight <= b.bdp() {
            b.mode = 2;
            b.cwnd_gain = 2.0;
            b.cycle_start = now;
            b.pacing_gain = BBR_GAINS[b.cycle];
        }
        if b.mode == 2 && (now - b.cycle_start) as f64 >= b.min_rtt {
            b.cycle = (b.cycle + 1) % 8;
            b.cycle_start = now;
            b.pacing_gain = BBR_GAINS[b.cycle];
        }
    }
}

struct Report {
    algo: &'static str,
    util: f64,
    avg_q: f64,
    max_q: usize,
    rtt_infl: f64,
    loss: f64,
    per_flow_pps: Vec<f64>,
    jain: f64,
    timeouts: u64,
    fast_rec: usize,
}

fn jain(x: &[f64]) -> f64 {
    let s: f64 = x.iter().sum();
    let q: f64 = x.iter().map(|v| v * v).sum();
    if q == 0.0 {
        1.0
    } else {
        s * s / (x.len() as f64 * q)
    }
}

fn simulate(
    algos: &[(Algo, u64, f64)],
    buffer: usize,
    loss: f64,
    secs: u64,
    seed: u64,
) -> (Sim, Report) {
    let flows = algos
        .iter()
        .map(|&(a, start, rwnd)| Flow::new(a, start, rwnd))
        .collect();
    let mut sim = Sim::new(NetCfg { buffer, loss }, flows, seed);
    sim.run(secs * 1_000_000);
    let half = secs as f64 / 2.0;
    let pps: Vec<f64> = sim
        .flows
        .iter()
        .map(|f| (f.rcv_next - f.mark) as f64 / half)
        .collect();
    let total: u64 = sim.flows.iter().map(|f| f.rcv_next).sum();
    let rtt_n: u64 = sim.flows.iter().map(|f| f.rtt_n).sum();
    let rtt_sum: f64 = sim.flows.iter().map(|f| f.rtt_sum).sum();
    let rep = Report {
        algo: algos[0].0.name(),
        util: total as f64 / (LINK_PPS * secs as f64),
        avg_q: sim.q_sum / sim.q_n.max(1) as f64,
        max_q: sim.q_max,
        rtt_infl: rtt_sum / rtt_n.max(1) as f64 / BASE_RTT,
        loss: sim.drops as f64 / sim.arrivals.max(1) as f64,
        jain: jain(&pps),
        per_flow_pps: pps,
        timeouts: sim.flows.iter().map(|f| f.timeouts).sum(),
        fast_rec: sim.flows.iter().map(|f| f.loss_events.len()).sum::<usize>()
            - sim.flows.iter().map(|f| f.timeouts as usize).sum::<usize>(),
    };
    (sim, rep)
}

/// ASCII plot of a time series: one column per time bucket (max in bucket), `height` rows.
fn plot(title: &str, data: &[(u64, f64)], t_end_us: u64, width: usize, height: usize) {
    let (mut sum, mut cnt) = (vec![0.0f64; width], vec![0u32; width]);
    for &(t, v) in data {
        let c = ((t as u128 * width as u128 / t_end_us.max(1) as u128) as usize).min(width - 1);
        sum[c] += v;
        cnt[c] += 1;
    }
    let cols: Vec<Option<f64>> = (0..width)
        .map(|c| (cnt[c] > 0).then(|| sum[c] / cnt[c] as f64))
        .collect();
    let maxv = cols.iter().flatten().cloned().fold(1.0, f64::max) * 1.05;
    println!(
        "   {title}: cwnd in packets (column = mean over {:.1} s), x axis 0..{} s",
        t_end_us as f64 / 1e6 / width as f64,
        t_end_us / 1_000_000
    );
    for row in (0..height).rev() {
        let (lo, hi) = (
            maxv * row as f64 / height as f64,
            maxv * (row + 1) as f64 / height as f64,
        );
        let line: String = cols
            .iter()
            .map(|c| {
                if c.map_or(false, |v| v > lo && v <= hi) {
                    '*'
                } else {
                    ' '
                }
            })
            .collect();
        println!("   {:>4.0} |{}", hi, line);
    }
    println!("        +{}", "-".repeat(width));
}

fn main() {
    let mbps = LINK_PPS * 1250.0 * 8.0 / 1e6;
    println!(
        "link {LINK_PPS} pkt/s ({mbps} Mbit/s at 1250 B), base RTT {:.0} ms, BDP ~ {:.0} packets",
        BASE_RTT / 1000.0,
        LINK_PPS * BASE_RTT / 1e6
    );

    // 1. Flow control: the receiver window caps a flow at rwnd/RTT regardless of cwnd.
    let (sim, rep) = simulate(&[(Algo::Reno, 0, 10.0)], 1000, 0.0, 20, 1);
    let want = 10.0 / (BASE_RTT / 1e6);
    println!(
        "-- 1. flow control: rwnd = 10 packets -> {:.0} pkt/s measured vs rwnd/RTT = {want:.0}; cwnd grew to {:.0} but is not the limit",
        rep.per_flow_pps[0], sim.flows[0].cwnd
    );
    assert!(
        (rep.per_flow_pps[0] / want - 1.0).abs() < 0.1,
        "throughput must equal rwnd/RTT"
    );

    // 2. Slow start doubles cwnd every RTT (unbounded buffer, no loss, initial window 2).
    let mut sim = Sim::new(
        NetCfg {
            buffer: 100_000,
            loss: 0.0,
        },
        vec![Flow::new(Algo::Reno, 0, 1e9)],
        1,
    );
    let mut samples = Vec::new();
    for k in 1..=5u64 {
        sim.run_until(((k as f64 + 0.9) * BASE_RTT) as u64);
        samples.push(sim.flows[0].cwnd);
    }
    println!("-- 2. slow start: cwnd sampled once per RTT = {samples:?}");
    for w in samples.windows(2) {
        let r = w[1] / w[0];
        assert!(
            (1.7..2.3).contains(&r),
            "slow start must roughly double per RTT, got x{r:.2}"
        );
    }

    // 3. AIMD sawtooth: one Reno flow, buffer = 1 BDP.
    let secs = 120;
    let (rsim, rrep) = simulate(&[(Algo::Reno, 0, 1e9)], 40, 0.0, secs, 2);
    let (sim, rep) = simulate(&[(Algo::NewReno, 0, 1e9)], 40, 0.0, secs, 2);
    let f = &sim.flows[0];
    let halvings: Vec<f64> = f
        .loss_events
        .iter()
        .skip(1)
        .map(|&(_, before, after)| after / before)
        .collect();
    println!(
        "-- 3. NewReno sawtooth (classic Reno on the same run: {} loss events, {} RTO timeouts, util {:.0}%):",
        rsim.flows[0].loss_events.len(),
        rsim.flows[0].timeouts,
        rrep.util * 100.0
    );
    println!(
        "   {} loss events ({} timeouts); cwnd after / before each loss: {:?}",
        f.loss_events.len(),
        f.timeouts,
        halvings
            .iter()
            .take(6)
            .map(|x| (x * 100.0).round() / 100.0)
            .collect::<Vec<_>>()
    );
    assert!(f.loss_events.len() >= 5, "several sawtooth cycles expected");
    let fast: Vec<&f64> = halvings.iter().collect();
    let in_band = fast.iter().filter(|&&&r| (0.4..0.62).contains(&r)).count();
    assert!(
        in_band * 10 >= fast.len() * 8,
        "the window must be about halved at each loss ({in_band}/{})",
        fast.len()
    );
    assert!(
        rep.util > 0.8,
        "a 1-BDP buffer keeps Reno above 80% utilisation: {:.2}",
        rep.util
    );
    // sawtooth shape: trough right after a loss vs peak right before the next one
    let (t0, t1) = (f.loss_events[2].0, f.loss_events[3].0);
    let lo = f
        .trace
        .iter()
        .find(|&&(t, _)| t > t0 + 60_000)
        .map(|x| x.1)
        .unwrap();
    let hi = f
        .trace
        .iter()
        .filter(|&&(t, _)| t < t1)
        .map(|x| x.1)
        .last()
        .unwrap();
    println!(
        "   one cycle: cwnd {lo:.0} -> {hi:.0} (peak/trough {:.2}) over {:.1} s: slow linear climb, sudden halving",
        hi / lo,
        (t1 - t0) as f64 / 1e6
    );
    assert!(hi > lo + 3.0 && (1.4..2.7).contains(&(hi / lo)));
    let tail: Vec<(u64, f64)> = f
        .trace
        .iter()
        .filter(|&&(t, _)| t >= 5_000_000 && t < 30_000_000)
        .map(|&(t, w)| (t - 5_000_000, w))
        .collect();
    plot(
        "NewReno (5 s..30 s, startup overshoot cut)",
        &tail,
        25_000_000,
        100,
        10,
    );

    // 4. All algorithms, single flow, buffer = 1 BDP, then bloated 10 BDP, then 1% random loss.
    println!("-- 4. one flow per algorithm, 60 s");
    for (label, buffer, loss) in [
        ("buffer 40 (1 BDP)", 40usize, 0.0),
        ("buffer 400 (10 BDP, bufferbloat)", 400, 0.0),
        ("buffer 40 + 1% random loss", 40, 0.01),
    ] {
        println!("   [{label}]");
        println!(
            "   {:<8}{:>7}{:>9}{:>8}{:>10}{:>8}{:>9}{:>9}",
            "algo", "util", "avg q", "max q", "RTT x", "loss %", "RTOs", "fast-rec"
        );
        for a in [Algo::Reno, Algo::NewReno, Algo::Cubic, Algo::Bbr] {
            let (_, r) = simulate(&[(a, 0, 1e9)], buffer, loss, 60, 3);
            println!(
                "   {:<8}{:>6.0}%{:>9.1}{:>8}{:>10.2}{:>8.2}{:>9}{:>9}",
                a.name(),
                r.util * 100.0,
                r.avg_q,
                r.max_q,
                r.rtt_infl,
                r.loss * 100.0,
                r.timeouts,
                r.fast_rec
            );
            if loss == 0.0 && buffer == 400 && a == Algo::Reno {
                assert!(
                    r.rtt_infl > 2.0,
                    "a deep drop-tail buffer inflates Reno's RTT (bufferbloat): x{:.2}",
                    r.rtt_infl
                );
            }
            if loss == 0.0 && buffer == 400 && a == Algo::Bbr {
                assert!(
                    r.rtt_infl < 2.0 && r.util > 0.8,
                    "BBR keeps the queue short with the link busy: util {:.2}, RTT x{:.2}",
                    r.util,
                    r.rtt_infl
                );
            }
            if loss > 0.0 && a == Algo::Bbr {
                assert!(r.util > 0.8, "BBR ignores random loss: util {:.2}", r.util);
            }
            if loss > 0.0 && a == Algo::Reno {
                assert!(
                    r.util < 0.5,
                    "loss-based Reno collapses under random loss: util {:.2}",
                    r.util
                );
            }
        }
    }
    println!("   cwnd traces (60 s, one flow each, buffer 40):");
    for a in [Algo::NewReno, Algo::Cubic, Algo::Bbr] {
        let (sim, _) = simulate(&[(a, 0, 1e9)], 40, 0.0, 60, 3);
        plot(a.name(), &sim.flows[0].trace, 60_000_000, 100, 8);
    }

    // 5. Fairness among competing equal-RTT flows (Jain's index over the second half of the run).
    println!("-- 5. fairness, 120 s, buffer 40");
    for n in 2..=4usize {
        let flows: Vec<(Algo, u64, f64)> = (0..n)
            .map(|i| (Algo::NewReno, i as u64 * 300_000, 1e9))
            .collect();
        let (_, r) = simulate(&flows, 40, 0.0, 120, 4);
        println!(
            "   {n} x NewReno: shares {:?} pkt/s, Jain {:.3}, utilisation {:.0}%",
            r.per_flow_pps.iter().map(|x| x.round()).collect::<Vec<_>>(),
            r.jain,
            r.util * 100.0
        );
        assert!(
            r.jain > 0.9,
            "equal-RTT AIMD flows converge to near-equal shares: Jain {:.3}",
            r.jain
        );
        let mean = r.per_flow_pps.iter().sum::<f64>() / n as f64;
        assert!(
            r.per_flow_pps.iter().all(|&x| (x / mean - 1.0).abs() < 0.4),
            "every share within 40% of the mean"
        );
    }
    for (name, mix) in [
        ("2 x CUBIC", vec![Algo::Cubic, Algo::Cubic]),
        ("2 x BBR", vec![Algo::Bbr, Algo::Bbr]),
        ("CUBIC vs NewReno", vec![Algo::Cubic, Algo::NewReno]),
        ("BBR vs NewReno", vec![Algo::Bbr, Algo::NewReno]),
    ] {
        let flows: Vec<(Algo, u64, f64)> = mix
            .iter()
            .enumerate()
            .map(|(i, &a)| (a, i as u64 * 300_000, 1e9))
            .collect();
        let (_, r) = simulate(&flows, 40, 0.0, 120, 4);
        println!(
            "   {name:<17} shares {:?} pkt/s, Jain {:.3}",
            r.per_flow_pps.iter().map(|x| x.round()).collect::<Vec<_>>(),
            r.jain
        );
    }

    // 6. Mathis law: throughput ~ (MSS/RTT) * sqrt(3/2) / sqrt(p) for random loss p (no queue loss: huge buffer).
    println!("-- 6. Reno throughput vs random loss p (Mathis: rate = 1.22 / (RTT sqrt(p)))");
    let mut pts = Vec::new();
    for p in [0.002, 0.005, 0.01, 0.02, 0.04] {
        let (_, r) = simulate(&[(Algo::Reno, 0, 1e9)], 100_000, p, 300, 5);
        let rate = r.per_flow_pps[0];
        let pred = 1.22 / (BASE_RTT / 1e6) / f64::sqrt(p);
        println!(
            "   p={p:<6} measured {rate:>6.1} pkt/s, Mathis {pred:>6.1}  (ratio {:.2})",
            rate / pred
        );
        assert!(
            rate / pred > 0.5 && rate / pred < 1.5,
            "within a loose factor of the Mathis prediction"
        );
        pts.push((p.ln(), rate.ln()));
    }
    let n = pts.len() as f64;
    let (sx, sy) = pts.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
    let slope = (n * pts.iter().map(|p| p.0 * p.1).sum::<f64>() - sx * sy)
        / (n * pts.iter().map(|p| p.0 * p.0).sum::<f64>() - sx * sx);
    println!("   log-log slope of throughput vs p = {slope:.2} (Mathis predicts -0.5)");
    assert!((-0.7..-0.35).contains(&slope));
    println!("tcp_congestion_control_sim: all checks passed");
}

impl Sim {
    /// Run (again) up to an absolute time; used to sample state mid-flight.
    fn run_until(&mut self, t_end: u64) {
        if self.heap.is_empty() && self.now == 0 {
            for i in 0..self.flows.len() {
                let s = self.flows[i].start;
                self.at(s, Ev::Start(i));
            }
        }
        while let Some(&Reverse((t, _, _))) = self.heap.peek() {
            if t > t_end {
                break;
            }
            let Reverse((t, _, ev)) = self.heap.pop().unwrap();
            self.now = t;
            self.dispatch(ev);
        }
        self.now = t_end;
    }
}
