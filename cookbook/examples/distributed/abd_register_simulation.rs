//! RECIPE 930 (distributed): deterministic simulation of an ABD atomic register, with a tape of choices and a shrinker
//! LEVEL: advanced
//! Use in: testing a replicated protocol by replaying it from one seed.   Run: cargo run --release --example abd_register_simulation
//! COMMON USES: simulation testing of consensus and replication code, replaying and shrinking a failure.
#![allow(dead_code)]
//!
//! N replicas keep one value with a timestamp (counter, writer id). A write asks a majority for their timestamps, picks a larger
//! one and stores the value on a majority. A read asks a majority, takes the newest value and stores it back before returning.
//! Two rules that are easy to get wrong, each checked in `main`:
//! - A writer must never use one timestamp for two values, even after it gave up on a write. Without `last_counter`, seeds
//!   585801, 1312797, 1695322 and 1923836 are not linearizable.
//! - A 0 on the tape must mean "no fault", because the shrinker writes zeros and a replay reads 0 after the tape ends.
//!
//! LIMITS: it simulates a model, not shipping code. Clients never crash (a restarted writer would need `last_counter` saved,
//! or a new writer id). A crash loses only unfinished disk writes. Passing seeds are evidence, not proof.
//! Read: https://groups.csail.mit.edu/tds/papers/Attiya/JACM95.pdf

/// A linearizability checker (Wing and Gong's search with Lowe's memo of visited states).
mod lin {
    use std::collections::HashSet;
    use std::hash::Hash;

    /// A sequential specification: how the object behaves when operations are applied one at a time.
    pub trait Model {
        type State: Clone + Eq + Hash;
        type Input: Clone;
        type Output: Clone + PartialEq;
        fn init(&self) -> Self::State;
        /// Apply `input`. `output` is what the client saw, or `None` when the operation never returned. Returns the next state, or `None`
        /// if the object could not have produced that output in this state.
        fn step(
            &self,
            state: &Self::State,
            input: &Self::Input,
            output: Option<&Self::Output>,
        ) -> Option<Self::State>;
    }

    #[derive(Clone, Debug)]
    pub struct Op<I, O> {
        pub client: usize,
        pub input: I,
        /// `None` when the operation never returned.
        pub output: Option<O>,
        /// Logical or physical times; only their order matters. `call < ret`.
        pub call: u64,
        pub ret: u64,
    }

    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    pub enum Verdict {
        Linearizable,
        NotLinearizable,
        /// The search was stopped after `limit` steps without an answer.
        Unknown,
    }

    /// Check one history. `limit` bounds the number of search steps (the problem is NP-complete).
    pub fn check<M: Model>(model: &M, history: &[Op<M::Input, M::Output>], limit: u64) -> Verdict {
        let n = history.len();
        let words = n.div_ceil(64).max(1);
        let completed: Vec<usize> = (0..n).filter(|&i| history[i].output.is_some()).collect();
        let mut cache: HashSet<(Vec<u64>, M::State)> = HashSet::new();
        let mut steps = 0u64;
        let mut done = vec![0u64; words];
        let initial = model.init();
        match search(
            model, history, &completed, &mut done, &initial, &mut cache, &mut steps, limit,
        ) {
            Some(true) => Verdict::Linearizable,
            Some(false) => Verdict::NotLinearizable,
            None => Verdict::Unknown,
        }
    }

    fn bit(v: &[u64], i: usize) -> bool {
        v[i / 64] >> (i % 64) & 1 == 1
    }

    #[allow(clippy::too_many_arguments)]
    fn search<M: Model>(
        model: &M,
        h: &[Op<M::Input, M::Output>],
        completed: &[usize],
        done: &mut Vec<u64>,
        state: &M::State,
        cache: &mut HashSet<(Vec<u64>, M::State)>,
        steps: &mut u64,
        limit: u64,
    ) -> Option<bool> {
        // success: every operation that returned has been placed in the order (pending ones may be left out)
        if completed.iter().all(|&i| bit(done, i)) {
            return Some(true);
        }
        if *steps >= limit {
            return None;
        }
        *steps += 1;
        if cache.contains(&(done.clone(), state.clone())) {
            return Some(false);
        }
        // an operation can go next if no unplaced operation had already returned before it was called
        let first_return = (0..h.len())
            .filter(|&i| !bit(done, i) && h[i].output.is_some())
            .map(|i| h[i].ret)
            .min()
            .unwrap();
        for i in 0..h.len() {
            if bit(done, i) || h[i].call > first_return {
                continue;
            }
            if let Some(next) = model.step(state, &h[i].input, h[i].output.as_ref()) {
                done[i / 64] |= 1 << (i % 64);
                let r = search(model, h, completed, done, &next, cache, steps, limit);
                done[i / 64] &= !(1 << (i % 64));
                match r {
                    Some(true) => return Some(true),
                    None => return None,
                    Some(false) => {}
                }
            }
        }
        cache.insert((done.clone(), state.clone()));
        Some(false)
    }

    // ---- the model -----------------------------------------------------------------

    /// A single read/write register, initially 0.
    pub struct Register;
    #[derive(Clone, Debug, PartialEq)]
    pub enum RegIn {
        Write(i64),
        Read,
    }
    #[derive(Clone, Debug, PartialEq)]
    pub enum RegOut {
        Ack,
        Value(i64),
    }
    impl Model for Register {
        type State = i64;
        type Input = RegIn;
        type Output = RegOut;
        fn init(&self) -> i64 {
            0
        }
        fn step(&self, s: &i64, input: &RegIn, output: Option<&RegOut>) -> Option<i64> {
            match (input, output) {
                (RegIn::Write(v), None | Some(RegOut::Ack)) => Some(*v),
                (RegIn::Read, None) => Some(*s),
                (RegIn::Read, Some(RegOut::Value(v))) if v == s => Some(*s),
                _ => None,
            }
        }
    }
}

/// The simulator: the tape, the network, the disks, the register and the shrinker.
mod dst {
    use super::lin::{self, Op, RegIn, RegOut, Verdict};
    use std::cmp::Reverse;
    use std::collections::{BinaryHeap, HashMap};

    // ---- randomness: a recorded tape of choices ---------------------------------------------------------------------------------------------

    /// Every random decision of a run is one entry of a tape. A fresh run draws from a generator and records the draws; a replay reads a
    /// (possibly edited) tape and draws fresh values only after its end. Editing the tape is how a failing run is shrunk.
    #[derive(Clone)]
    pub struct Tape {
        pub choices: Vec<u64>,
        pos: usize,
        state: u64,
        /// In a replay, a choice beyond the end of the tape is 0, the benign choice, instead of a fresh random value.
        zero_after_end: bool,
    }
    impl Tape {
        pub fn from_seed(seed: u64) -> Tape {
            Tape {
                choices: vec![],
                pos: 0,
                state: seed.wrapping_mul(0x9E3779B97F4A7C15) ^ 0xD1B54A32D192ED03,
                zero_after_end: false,
            }
        }
        pub fn replay(choices: Vec<u64>, seed: u64) -> Tape {
            Tape {
                choices,
                zero_after_end: true,
                ..Tape::from_seed(seed)
            }
        }
        fn fresh(&mut self) -> u64 {
            // splitmix64
            self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
            let mut z = self.state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
            z ^ (z >> 31)
        }
        /// A choice in `0..m`. Small values are the benign ones (no drop, shortest delay, first operation), so shrinking a tape towards zero
        /// shrinks a failing scenario towards the plainest one that still fails.
        pub fn next(&mut self, m: u64) -> u64 {
            let c = if self.pos < self.choices.len() {
                self.choices[self.pos]
            } else if self.zero_after_end {
                0
            } else {
                let c = self.fresh();
                self.choices.push(c);
                c
            };
            self.pos += 1;
            c % m.max(1)
        }
    }

    // ---- configuration -----------------------------------------------------------------------------------------------------------------------

    #[derive(Clone, Copy, Debug, Default)]
    pub struct Bugs {
        /// A read returns after the first phase, without writing the value it found back to a majority.
        pub no_write_back: bool,
        /// Timestamps are bare counters: two clients can pick the same timestamp for different values.
        pub no_tiebreak: bool,
        /// A replica acknowledges a store before it has reached the disk.
        pub ack_before_persist: bool,
        /// A majority is `n / 2`, not `n / 2 + 1`.
        pub small_quorum: bool,
        /// A replica applies a store even when its timestamp is older than the one it holds.
        pub stale_overwrite: bool,
        /// A read returns the first reply's value instead of the one with the highest timestamp.
        pub first_reply: bool,
        /// A write returns after the first phase, before the value is stored anywhere.
        pub early_write_ack: bool,
    }

    impl Bugs {
        pub const NAMES: [&'static str; 7] = [
            "no_write_back",
            "no_tiebreak",
            "ack_before_persist",
            "small_quorum",
            "stale_overwrite",
            "first_reply",
            "early_write_ack",
        ];
        pub fn only(i: usize) -> Bugs {
            let mut b = Bugs::default();
            match i {
                0 => b.no_write_back = true,
                1 => b.no_tiebreak = true,
                2 => b.ack_before_persist = true,
                3 => b.small_quorum = true,
                4 => b.stale_overwrite = true,
                5 => b.first_reply = true,
                _ => b.early_write_ack = true,
            }
            b
        }
    }

    #[derive(Clone, Debug)]
    pub struct Config {
        pub replicas: usize,
        pub clients: usize,
        pub ops_per_client: usize,
        pub bugs: Bugs,
        /// Iterate a `HashMap` where the order matters. Only for showing why a simulator must not: the run stops being a function of its seed.
        pub hazard_hash_order: bool,
    }
    impl Default for Config {
        fn default() -> Self {
            Config {
                replicas: 3,
                clients: 3,
                ops_per_client: 12,
                bugs: Bugs::default(),
                hazard_hash_order: false,
            }
        }
    }

    /// The environment of one run, drawn from the tape: every seed is a different world.
    #[derive(Clone, Debug)]
    struct World {
        drop_permille: u64,
        dup_permille: u64,
        min_delay: u64,
        max_delay: u64,
        disk_delay: u64,
        crashes: u64,
        partitions: u64,
        /// Partitions that outlast a client's patience (360 ms of retries), so that some operations are given up on.
        long_partitions: bool,
    }

    // ---- the simulation ----------------------------------------------------------------------------------------------------------------------

    type Ts = (u64, u64); // (counter, writer id)

    #[derive(Clone, Debug)]
    enum Msg {
        Query {
            op: u64,
            phase: u8,
        },
        QueryReply {
            op: u64,
            phase: u8,
            ts: Ts,
            val: i64,
        },
        Store {
            op: u64,
            phase: u8,
            ts: Ts,
            val: i64,
        },
        StoreAck {
            op: u64,
            phase: u8,
        },
    }

    #[derive(Clone, Debug)]
    enum Event {
        Deliver {
            to: usize,
            from: usize,
            msg: Msg,
        },
        /// A replica's disk write finished: `epoch` identifies the incarnation that issued it; a crash in between loses it.
        Persisted {
            replica: usize,
            epoch: u64,
            ts: Ts,
            val: i64,
            ack_to: Option<(usize, u64, u8)>,
        },
        ClientStep {
            client: usize,
        },
        ClientTimeout {
            client: usize,
            op: u64,
            phase: u8,
            attempt: u32,
        },
        Crash {
            replica: usize,
        },
        Restart {
            replica: usize,
        },
        Partition {
            side: u64,
            heal_after: u64,
        },
        Heal,
    }

    struct Replica {
        up: bool,
        epoch: u64,
        // durable
        ts: Ts,
        val: i64,
    }

    enum Phase {
        Idle,
        Query {
            replies: Vec<(usize, Ts, i64)>,
        },
        Store {
            acks: Vec<usize>,
            ts: Ts,
            val: i64,
            then_return: Option<i64>,
        },
    }

    struct Client {
        next_op: usize,
        op_id: u64,
        phase_no: u8,
        phase: Phase,
        kind: RegIn,
        history_index: usize,
        /// The highest counter this client has used in a write timestamp. A write it gave up on may sit on a minority of
        /// replicas, where the next query can miss it. Without this, the next write could reuse that timestamp for a new value.
        last_counter: u64,
    }

    pub struct Run {
        pub history: Vec<Op<RegIn, RegOut>>,
        pub trace_hash: u64,
        pub events: u64,
        pub virtual_time: u64,
        pub drops: u64,
        pub dups: u64,
        /// Disk writes that were lost because the replica crashed, and of those, the ones that would have finished after it restarted.
        pub lost_writes: u64,
        pub lost_after_restart: u64,
        pub crashes: u64,
        pub partitions: u64,
        pub tape_len: usize,
        pub tape: Vec<u64>,
    }

    struct Sim<'a> {
        cfg: &'a Config,
        /// How many operations each client performs: drawn from the tape, so that shrinking can remove operations.
        ops_for_client: Vec<usize>,
        tape: Tape,
        world: World,
        now: u64,
        seq: u64,
        queue: BinaryHeap<Reverse<(u64, u64, usize)>>,
        events: Vec<Event>,
        replicas: Vec<Replica>,
        clients: Vec<Client>,
        history: Vec<Op<RegIn, RegOut>>,
        blocked: u64, // bitmask of nodes cut off from the rest; nodes 0..replicas are replicas, the rest clients
        hash: u64,
        count: u64,
        drops: u64,
        dups: u64,
        lost_writes: u64,
        lost_after_restart: u64,
        crashes: u64,
        partitions: u64,
        replica_set: HashMap<usize, ()>,
    }

    impl<'a> Sim<'a> {
        fn schedule(&mut self, delay: u64, ev: Event) {
            self.seq += 1;
            self.events.push(ev);
            self.queue
                .push(Reverse((self.now + delay, self.seq, self.events.len() - 1)));
        }

        fn majority(&self) -> usize {
            let n = self.cfg.replicas;
            if self.cfg.bugs.small_quorum {
                n / 2
            } else {
                n / 2 + 1
            }
        }

        fn node_of_client(&self, c: usize) -> usize {
            self.cfg.replicas + c
        }

        /// The operation id that travels in messages: the client and its operation counter.
        fn wire(c: usize, op: u64) -> u64 {
            (c as u64) << 32 | op
        }

        /// True `permille` times in 1000. The HIGH draws count, so that a 0 on the tape means "no fault".
        fn chance(&mut self, permille: u64) -> bool {
            self.tape.next(1000) >= 1000 - permille
        }

        /// Send a message through the faulty network.
        fn send(&mut self, from: usize, to: usize, msg: Msg) {
            let cut = (self.blocked >> from & 1) != (self.blocked >> to & 1);
            if cut || self.chance(self.world.drop_permille) {
                self.drops += 1;
                return;
            }
            let spread = self.world.max_delay - self.world.min_delay + 1;
            let delay = self.world.min_delay + self.tape.next(spread);
            let dup = self.chance(self.world.dup_permille);
            if dup {
                self.dups += 1;
                let d2 = self.world.min_delay + self.tape.next(spread);
                self.schedule(
                    d2,
                    Event::Deliver {
                        to,
                        from,
                        msg: msg.clone(),
                    },
                );
            }
            self.schedule(delay, Event::Deliver { to, from, msg });
        }

        fn mix(&mut self, x: u64) {
            self.hash = (self.hash ^ x).wrapping_mul(0x100000001b3);
        }

        fn start_op(&mut self, c: usize) {
            let cl = &mut self.clients[c];
            if cl.next_op >= self.ops_for_client[c] {
                return;
            }
            let i = cl.next_op;
            cl.next_op += 1;
            cl.op_id += 1;
            cl.phase_no = 1;
            let kind = if self.tape.next(2) == 0 {
                RegIn::Write((c * 1000 + i + 1) as i64)
            } else {
                RegIn::Read
            };
            let cl = &mut self.clients[c];
            cl.kind = kind.clone();
            cl.phase = Phase::Query { replies: vec![] };
            self.history.push(Op {
                client: c,
                input: kind,
                output: None,
                call: self.count,
                ret: u64::MAX,
            });
            cl.history_index = self.history.len() - 1;
            let (op, node) = (cl.op_id, self.node_of_client(c));
            self.broadcast_query(c, node, op, 1);
        }

        fn broadcast_query(&mut self, c: usize, node: usize, op: u64, phase: u8) {
            for r in 0..self.cfg.replicas {
                self.send(
                    node,
                    r,
                    Msg::Query {
                        op: Self::wire(c, op),
                        phase,
                    },
                );
            }
            self.schedule(
                30_000,
                Event::ClientTimeout {
                    client: c,
                    op,
                    phase,
                    attempt: 1,
                },
            );
        }

        fn broadcast_store(&mut self, c: usize, ts: Ts, val: i64) {
            let (op, phase, node) = (
                self.clients[c].op_id,
                self.clients[c].phase_no,
                self.node_of_client(c),
            );
            for r in 0..self.cfg.replicas {
                self.send(
                    node,
                    r,
                    Msg::Store {
                        op: Self::wire(c, op),
                        phase,
                        ts,
                        val,
                    },
                );
            }
            self.schedule(
                30_000,
                Event::ClientTimeout {
                    client: c,
                    op,
                    phase,
                    attempt: 1,
                },
            );
        }

        fn finish_op(&mut self, c: usize, out: RegOut) {
            let i = self.clients[c].history_index;
            self.history[i].output = Some(out);
            self.history[i].ret = self.count;
            self.clients[c].phase = Phase::Idle;
            let think = self.tape.next(3000);
            self.schedule(think, Event::ClientStep { client: c });
        }

        fn handle(&mut self, ev: Event) {
            match ev {
                Event::ClientStep { client } => self.start_op(client),
                Event::Deliver { to, from, msg } => {
                    self.mix(to as u64 * 31 + from as u64);
                    if to < self.cfg.replicas {
                        self.replica_receive(to, from, msg);
                    } else {
                        self.client_receive(to - self.cfg.replicas, from, msg);
                    }
                }
                Event::Persisted {
                    replica,
                    epoch,
                    ts,
                    val,
                    ack_to,
                } => {
                    let r = &mut self.replicas[replica];
                    if !r.up || r.epoch != epoch {
                        // the replica crashed before the write reached the disk
                        self.lost_writes += 1;
                        self.lost_after_restart += (r.up && r.epoch != epoch) as u64;
                        return;
                    }
                    if ts > r.ts || self.cfg.bugs.stale_overwrite {
                        r.ts = ts;
                        r.val = val;
                    }
                    if let Some((client_node, op, phase)) = ack_to {
                        self.send(replica, client_node, Msg::StoreAck { op, phase });
                    }
                }
                Event::ClientTimeout {
                    client,
                    op,
                    phase,
                    attempt,
                } => {
                    let cl = &self.clients[client];
                    if cl.op_id != op || cl.phase_no != phase || matches!(cl.phase, Phase::Idle) {
                        return;
                    }
                    if attempt >= 12 {
                        // give up: the operation never returns (its history entry stays pending)
                        self.clients[client].phase = Phase::Idle;
                        let think = self.tape.next(3000);
                        self.schedule(think, Event::ClientStep { client });
                        return;
                    }
                    let node = self.node_of_client(client);
                    let wire_op = Self::wire(client, op);
                    // resend to every replica. The order of the sends decides which random choices each message gets, so it must not depend on
                    // anything but the seed: the hazard variant takes it from a HashMap, whose iteration order differs from one map to the next
                    let order: Vec<usize> = if self.cfg.hazard_hash_order {
                        self.replica_set.keys().copied().collect()
                    } else {
                        (0..self.cfg.replicas).collect()
                    };
                    match &self.clients[client].phase {
                        Phase::Query { .. } => {
                            for &r in &order {
                                self.send(node, r, Msg::Query { op: wire_op, phase });
                            }
                        }
                        Phase::Store { ts, val, .. } => {
                            let (ts, val) = (*ts, *val);
                            for &r in &order {
                                self.send(
                                    node,
                                    r,
                                    Msg::Store {
                                        op: wire_op,
                                        phase,
                                        ts,
                                        val,
                                    },
                                );
                            }
                        }
                        Phase::Idle => {}
                    }
                    self.schedule(
                        30_000,
                        Event::ClientTimeout {
                            client,
                            op,
                            phase,
                            attempt: attempt + 1,
                        },
                    );
                }
                Event::Crash { replica } => {
                    self.mix(1000 + replica as u64);
                    let r = &mut self.replicas[replica];
                    if r.up {
                        r.up = false;
                        r.epoch += 1;
                        self.crashes += 1;
                        let down = 500 + self.tape.next(40_000);
                        self.schedule(down, Event::Restart { replica });
                    }
                }
                Event::Restart { replica } => {
                    self.mix(2000 + replica as u64);
                    self.replicas[replica].up = true;
                }
                Event::Partition { side, heal_after } => {
                    self.mix(3000 + side);
                    self.blocked = side;
                    self.partitions += 1;
                    self.schedule(heal_after, Event::Heal);
                }
                Event::Heal => {
                    self.mix(4000);
                    self.blocked = 0;
                }
            }
        }

        fn replica_receive(&mut self, r: usize, from: usize, msg: Msg) {
            if !self.replicas[r].up {
                return;
            }
            match msg {
                Msg::Query { op, phase } => {
                    let (ts, val) = (self.replicas[r].ts, self.replicas[r].val);
                    self.send(r, from, Msg::QueryReply { op, phase, ts, val });
                }
                Msg::Store { op, phase, ts, val } => {
                    let epoch = self.replicas[r].epoch;
                    let delay = 1 + self.tape.next(self.world.disk_delay);
                    if self.cfg.bugs.ack_before_persist {
                        self.send(r, from, Msg::StoreAck { op, phase });
                        self.schedule(
                            delay,
                            Event::Persisted {
                                replica: r,
                                epoch,
                                ts,
                                val,
                                ack_to: None,
                            },
                        );
                    } else {
                        self.schedule(
                            delay,
                            Event::Persisted {
                                replica: r,
                                epoch,
                                ts,
                                val,
                                ack_to: Some((from, op, phase)),
                            },
                        );
                    }
                }
                _ => {}
            }
        }

        fn client_receive(&mut self, c: usize, from: usize, msg: Msg) {
            let (wire_op, phase) = match &msg {
                Msg::QueryReply { op, phase, .. } | Msg::StoreAck { op, phase } => (*op, *phase),
                _ => return,
            };
            let cl = &self.clients[c];
            if wire_op != Self::wire(c, cl.op_id) || phase != cl.phase_no {
                return; // a late reply to an earlier operation or phase
            }
            let need = self.majority();
            match (&mut self.clients[c].phase, msg) {
                (Phase::Query { replies }, Msg::QueryReply { ts, val, .. }) => {
                    if replies.iter().any(|(r, _, _)| *r == from) {
                        return;
                    }
                    replies.push((from, ts, val));
                    if replies.len() < need {
                        return;
                    }
                    let replies = replies.clone();
                    let best = if self.cfg.bugs.first_reply {
                        replies[0]
                    } else {
                        *replies.iter().max_by_key(|(_, t, _)| *t).unwrap()
                    };
                    let kind = self.clients[c].kind.clone();
                    match kind {
                        RegIn::Write(v) => {
                            if self.cfg.bugs.early_write_ack {
                                self.finish_op(c, RegOut::Ack);
                                return;
                            }
                            let seen = replies.iter().map(|(_, t, _)| t.0).max().unwrap();
                            let counter = seen.max(self.clients[c].last_counter) + 1;
                            self.clients[c].last_counter = counter;
                            let ts = if self.cfg.bugs.no_tiebreak {
                                (counter, 0)
                            } else {
                                (counter, c as u64 + 1)
                            };
                            self.clients[c].phase_no = 2;
                            self.clients[c].phase = Phase::Store {
                                acks: vec![],
                                ts,
                                val: v,
                                then_return: None,
                            };
                            self.broadcast_store(c, ts, v);
                        }
                        RegIn::Read => {
                            if self.cfg.bugs.no_write_back {
                                self.finish_op(c, RegOut::Value(best.2));
                                return;
                            }
                            self.clients[c].phase_no = 2;
                            self.clients[c].phase = Phase::Store {
                                acks: vec![],
                                ts: best.1,
                                val: best.2,
                                then_return: Some(best.2),
                            };
                            self.broadcast_store(c, best.1, best.2);
                        }
                    }
                }
                (
                    Phase::Store {
                        acks, then_return, ..
                    },
                    Msg::StoreAck { .. },
                ) => {
                    if acks.contains(&from) {
                        return;
                    }
                    acks.push(from);
                    if acks.len() < need {
                        return;
                    }
                    let out = match then_return {
                        Some(v) => RegOut::Value(*v),
                        None => RegOut::Ack,
                    };
                    self.finish_op(c, out);
                }
                _ => {}
            }
        }
    }

    /// Run one simulation from a tape. The result is a pure function of the tape's content and the configuration.
    pub fn run(cfg: &Config, mut tape: Tape) -> Run {
        let world = World {
            drop_permille: [0, 0, 20, 100, 300][tape.next(5) as usize],
            dup_permille: [0, 0, 50, 200][tape.next(4) as usize],
            min_delay: 100,
            max_delay: 100 + [0, 500, 5_000, 20_000][tape.next(4) as usize],
            disk_delay: [1, 500, 5_000][tape.next(3) as usize],
            crashes: [0, 0, 2, 6][tape.next(4) as usize],
            partitions: [0, 0, 1, 3][tape.next(4) as usize],
            long_partitions: tape.next(4) == 3,
        };
        let ops_for_client: Vec<usize> = (0..cfg.clients)
            .map(|_| 1 + tape.next(cfg.ops_per_client as u64) as usize)
            .collect();
        let mut sim = Sim {
            cfg,
            ops_for_client,
            tape,
            world: world.clone(),
            now: 0,
            seq: 0,
            queue: BinaryHeap::new(),
            events: vec![],
            replicas: (0..cfg.replicas)
                .map(|_| Replica {
                    up: true,
                    epoch: 0,
                    ts: (0, 0),
                    val: 0,
                })
                .collect(),
            clients: (0..cfg.clients)
                .map(|_| Client {
                    next_op: 0,
                    op_id: 0,
                    phase_no: 0,
                    phase: Phase::Idle,
                    kind: RegIn::Read,
                    history_index: 0,
                    last_counter: 0,
                })
                .collect(),
            history: vec![],
            blocked: 0,
            hash: 0xcbf29ce484222325,
            count: 0,
            drops: 0,
            dups: 0,
            lost_writes: 0,
            lost_after_restart: 0,
            crashes: 0,
            partitions: 0,
            replica_set: (0..cfg.replicas).map(|r| (r, ())).collect(),
        };
        for c in 0..cfg.clients {
            let start = sim.tape.next(2000);
            sim.schedule(start, Event::ClientStep { client: c });
        }
        for _ in 0..world.crashes {
            let (at, r) = (
                sim.tape.next(300_000),
                sim.tape.next(cfg.replicas as u64) as usize,
            );
            sim.schedule(at, Event::Crash { replica: r });
        }
        for _ in 0..world.partitions {
            let at = sim.tape.next(300_000);
            // a partition cuts a random non-empty proper subset of all nodes off from the rest
            let total = cfg.replicas + cfg.clients;
            let side = 1 + sim.tape.next((1u64 << total) - 2);
            let heal = 5_000
                + sim.tape.next(if world.long_partitions {
                    900_000
                } else {
                    60_000
                });
            sim.schedule(
                at,
                Event::Partition {
                    side,
                    heal_after: heal,
                },
            );
        }
        let limit = 60_000_000u64; // one virtual minute
        while let Some(Reverse((t, _, idx))) = sim.queue.pop() {
            if t > limit {
                break;
            }
            sim.now = t;
            sim.count += 1;
            sim.mix(t);
            let ev = sim.events[idx].clone();
            sim.handle(ev);
        }
        let tape_len = sim.tape.pos;
        let mut history = sim.history;
        // an operation that never returned is pending: its return time is "never"
        for op in history.iter_mut() {
            if op.output.is_none() {
                op.ret = u64::MAX;
            }
        }
        Run {
            history,
            trace_hash: sim.hash,
            events: sim.count,
            virtual_time: sim.now,
            drops: sim.drops,
            dups: sim.dups,
            lost_writes: sim.lost_writes,
            lost_after_restart: sim.lost_after_restart,
            crashes: sim.crashes,
            partitions: sim.partitions,
            tape_len,
            tape: sim.tape.choices,
        }
    }

    pub fn run_seed(cfg: &Config, seed: u64) -> Run {
        run(cfg, Tape::from_seed(seed))
    }

    /// What the checker says about the history of a run. `Unknown` means the search hit its step limit.
    pub fn verdict(run: &Run) -> Verdict {
        lin::check(&lin::Register, &run.history, 20_000_000)
    }

    /// Did the checker prove that no order explains this run? `Unknown` is not a proof.
    pub fn fails(run: &Run) -> bool {
        verdict(run) == Verdict::NotLinearizable
    }

    /// Shrink the tape of a failing run: try deleting stretches of choices and zeroing them (zero is the benign choice everywhere), keeping any
    /// change after which the run still fails. Returns the smallest failing tape found, its run, and the number of replays tried.
    pub fn shrink(cfg: &Config, seed: u64, failing: &Run, max_replays: usize) -> (Run, usize) {
        let tried_cell = std::cell::Cell::new(0usize);
        let fails = |choices: &Vec<u64>| -> Option<Run> {
            tried_cell.set(tried_cell.get() + 1);
            if tried_cell.get() > max_replays {
                return None;
            }
            let r = run(cfg, Tape::replay(choices.clone(), seed));
            super::dst::fails(&r).then_some(r)
        };
        let mut best = failing.tape.clone();
        let mut best_run = run(cfg, Tape::replay(best.clone(), seed));
        let mut progress = true;
        while progress {
            progress = false;
            // delete stretches, longest first
            let mut chunk = best.len().max(1) / 2;
            while chunk >= 1 {
                let mut i = 0;
                while i + chunk <= best.len() {
                    let mut cand = best.clone();
                    cand.drain(i..i + chunk);
                    if let Some(r) = fails(&cand) {
                        best = cand;
                        best_run = r;
                        progress = true;
                    } else {
                        i += chunk;
                    }
                }
                chunk /= 2;
            }
            // zero stretches
            let mut chunk = best.len().max(1) / 2;
            while chunk >= 1 {
                let mut i = 0;
                while i + chunk <= best.len() {
                    if best[i..i + chunk].iter().any(|&c| c != 0) {
                        let mut cand = best.clone();
                        cand[i..i + chunk].iter_mut().for_each(|c| *c = 0);
                        if let Some(r) = fails(&cand) {
                            best = cand;
                            best_run = r;
                            progress = true;
                        }
                    }
                    i += chunk;
                }
                chunk /= 2;
            }
            // lower single choices: to zero, then by halves and by one
            for i in 0..best.len() {
                for lower in [0, best[i] / 2, best[i].saturating_sub(1)] {
                    if lower < best[i] {
                        let mut cand = best.clone();
                        cand[i] = lower;
                        if let Some(r) = fails(&cand) {
                            best = cand;
                            best_run = r;
                            progress = true;
                            break;
                        }
                    }
                }
            }
        }
        best_run.tape = best;
        (best_run, tried_cell.get().min(max_replays))
    }
}

use std::collections::HashSet;

fn main() {
    use dst::*;
    let correct = Config::default();

    // 0. a tape that picks the worst loss and copy rates, then runs out, injects neither: every draw after its end is 0
    let r = run(&correct, Tape::replay(vec![4, 3], 0));
    assert_eq!((r.drops, r.dups), (0, 0), "0 must mean no fault");

    // 1. one seed gives one run
    for seed in 0..200 {
        let (a, b) = (run_seed(&correct, seed), run_seed(&correct, seed));
        assert_eq!(
            (a.trace_hash, &a.tape, a.events),
            (b.trace_hash, &b.tape, b.events),
            "seed {seed}"
        );
    }

    // 2. unless a HashMap decides the order of the resends. Seed 3 resends; Rust randomizes each map's iteration order.
    let traces = |cfg: &Config| {
        (0..40)
            .map(|_| run_seed(cfg, 3).trace_hash)
            .collect::<HashSet<_>>()
            .len()
    };
    let hazard = Config {
        hazard_hash_order: true,
        ..Config::default()
    };
    let (ordered, hashed) = (traces(&correct), traces(&hazard));
    assert!(ordered == 1 && hashed > 1, "{ordered} {hashed}");
    println!(
        "40 runs of seed 3: {ordered} trace with ordered resends, {hashed} with HashMap order"
    );

    // 3. the correct register: every verdict is a real "yes", and the faults really happen
    let (mut ops, mut pending, mut events, mut lost, mut late) = (0, 0, 0, 0, 0);
    let mut micros = 0; // virtual time
    for seed in 0..6000 {
        let r = run_seed(&correct, seed);
        assert_eq!(verdict(&r), lin::Verdict::Linearizable, "seed {seed}");
        ops += r.history.len();
        pending += r.history.iter().filter(|o| o.output.is_none()).count();
        events += r.events;
        micros += r.virtual_time;
        lost += r.lost_writes;
        late += r.lost_after_restart;
    }
    assert!(pending > 0 && lost > 100 && late > 10);
    println!(
        "correct, 6000 seeds: {ops} operations ({pending} never returned), {events} events, {lost} disk writes lost to crashes ({late} after a restart)"
    );
    println!("  simulated time: {} seconds", micros / 1_000_000);

    // 4. regression: each of these seeds made a writer reuse a timestamp
    for seed in [585_801, 1_312_797, 1_695_322, 1_923_836] {
        assert_eq!(
            verdict(&run_seed(&correct, seed)),
            lin::Verdict::Linearizable,
            "regression seed {seed}"
        );
    }

    // 5. each planted bug is found, replays, and shrinks
    println!(
        "{:<20}{:>11}{:>14}{:>14}",
        "bug", "first seed", "failing/2000", "operations"
    );
    for (i, name) in Bugs::NAMES.iter().enumerate() {
        let cfg = Config {
            bugs: Bugs::only(i),
            ..Config::default()
        };
        let failing: Vec<u64> = (0..2000).filter(|&s| fails(&run_seed(&cfg, s))).collect();
        let seed = *failing
            .first()
            .unwrap_or_else(|| panic!("{name} was never found"));
        let first = run_seed(&cfg, seed);
        assert_eq!(first.trace_hash, run_seed(&cfg, seed).trace_hash);
        assert!(!fails(&run_seed(&correct, seed)));
        let (small, _) = shrink(&cfg, seed, &first, 300_000);
        assert!(fails(&small) && small.history.len() <= 6, "{name}");
        println!(
            "{name:<20}{seed:>11}{:>14}{:>9} -> {}",
            failing.len(),
            first.history.len(),
            small.history.len()
        );
        if i == 0 {
            println!("  smallest failing history for {name}, seed {seed}:");
            for o in &small.history {
                let ret = if o.ret == u64::MAX {
                    "never".into()
                } else {
                    o.ret.to_string()
                };
                println!(
                    "    client {} {:?} -> {:?} [{}, {ret}]",
                    o.client, o.input, o.output, o.call
                );
            }
        }
    }
}
