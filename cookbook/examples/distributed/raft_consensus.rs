//! RECIPE 400 (distributed): Raft consensus in a deterministic simulation: elections, log replication, repair, crashes and partitions
//! LEVEL: advanced
//! Use in: building or reviewing a replicated state machine (config store, metadata service, lock service) or testing any consensus code.   Run: cargo run --release --example raft_consensus
#![allow(dead_code)]
//!
//! WHAT: five Raft nodes (follower / candidate / leader) with terms, RequestVote, AppendEntries with the prev-index/prev-term
//! CONSISTENCY CHECK and log repair, a commit index that advances only when a majority has replicated an entry of the
//! CURRENT term, randomised election timeouts, heartbeats, PERSISTENT state (currentTerm, votedFor, log) that survives a
//! crash while everything else (role, commit index, applied state, leader bookkeeping) is lost and rebuilt, and a replicated
//! KV state machine. The whole cluster runs inside one thread: a simulated network (drop, delay, duplication, reordering by
//! random delay, partitions) and all timers are events in a time-ordered queue, and ALL randomness comes from a seeded
//! xorshift, so any failing schedule is reproduced exactly from its seed.
//! WHY: consensus bugs hide in rare interleavings (a vote lost here, a stale leader there). Real tests cannot reach them
//! reliably, a simulator can: it plays thousands of schedules in seconds and checks the paper's safety properties AFTER
//! EVERY SINGLE EVENT, not just at the end. This is deterministic simulation testing (FoundationDB, TigerBeetle, Jepsen).
//! HOW (the properties of Figure 3 in the paper):
//!  * Election safety: a node votes once per term (votedFor persisted BEFORE replying) and a leader needs a majority.
//!  * Log matching: AppendEntries carries (prevIndex, prevTerm); a follower refuses unless it has that entry, so "same
//!    index and term => same prefix" holds by induction. A refusal makes the leader back nextIndex up (with a hint of the
//!    follower's length) until logs agree; the follower then overwrites only CONFLICTING entries. Truncating on a match
//!    would let a delayed duplicate delete good entries.
//!  * Leader completeness: a candidate's log must be at least as up to date as the voter's (last term, then index).
//!  * State machine safety: entries are applied in order from the commit index.
//! A leader counts replicas only for entries of its OWN term (Figure 8): an old-term entry on a majority can still be
//! overwritten by a later leader, so old entries commit indirectly; hence each new leader appends a no-op at once.
//! Liveness needs randomised timeouts so one candidate usually wins before the others time out.
//! TEETH: the checker is only credible if it can fail, so a deliberately broken node (it grants votes without the up-to-date
//! check) is run through the same schedules and the invariant checker must catch it.
//! COMMON USES: etcd and the Kubernetes control plane, Consul, CockroachDB and TiKV (a Raft group per range), RethinkDB,
//! Kafka KRaft metadata quorum, HashiCorp Vault integrated storage, NATS JetStream; the hashicorp/raft and etcd-io/raft libraries.
//! Read: Ongaro and Ousterhout, "In Search of an Understandable Consensus Algorithm" (USENIX ATC 2014)
//! https://raft.github.io/raft.pdf , https://raft.github.io/ (visualisation, the full spec and the thesis)
use std::collections::{BTreeMap, BTreeSet};

const N: usize = 5;
const MAJ: usize = N / 2 + 1;
const HEARTBEAT: u64 = 50;
const ELECT_MIN: u64 = 150;
const ELECT_SPAN: u64 = 150;
/// Entries per AppendEntries message: small so that catching up a far-behind follower takes several round trips.
const BATCH: usize = 8;

// ---------------------------------------------------------------- deterministic randomness

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        (0..4).for_each(|_| {
            r.next();
        });
        r
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        (self.next() >> 11) % n
    }
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.below(hi - lo)
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

// ---------------------------------------------------------------- protocol data

#[derive(Clone, Debug, PartialEq, Eq)]
enum Cmd {
    Noop,
    Put(u32, u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    term: u64,
    cmd: Cmd,
}

#[derive(Clone, Debug)]
enum Msg {
    RequestVote {
        term: u64,
        cand: usize,
        last_idx: usize,
        last_term: u64,
    },
    VoteResp {
        term: u64,
        granted: bool,
    },
    Append {
        term: u64,
        prev_idx: usize,
        prev_term: u64,
        entries: Vec<Entry>,
        commit: usize,
    },
    AppendResp {
        term: u64,
        ok: bool,
        match_idx: usize,
        hint: usize,
    },
}

impl Msg {
    fn term(&self) -> u64 {
        match self {
            Msg::RequestVote { term, .. }
            | Msg::VoteResp { term, .. }
            | Msg::Append { term, .. }
            | Msg::AppendResp { term, .. } => *term,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Role {
    Follower,
    Candidate,
    Leader,
}

/// What Raft requires to be on stable storage BEFORE answering any RPC. Everything else in `Node` is volatile.
#[derive(Default)]
struct Persist {
    term: u64,
    voted_for: Option<usize>,
    /// 1-based in the paper; `log[i - 1]` is index i here.
    log: Vec<Entry>,
}

struct Node {
    up: bool,
    incarnation: u32,
    p: Persist,
    // ---- volatile: rebuilt after a restart
    role: Role,
    votes: BTreeSet<usize>,
    commit: usize,
    applied: Vec<Entry>,
    kv: BTreeMap<u32, u32>,
    next_index: [usize; N],
    match_index: [usize; N],
    /// Invalidates stale timer events (a timer is "cancelled" by bumping this).
    epoch: u64,
}

impl Node {
    fn new() -> Node {
        let (p, role, votes) = (Persist::default(), Role::Follower, BTreeSet::new());
        Node {
            up: true,
            incarnation: 0,
            p,
            role,
            votes,
            commit: 0,
            applied: vec![],
            kv: BTreeMap::new(),
            next_index: [1; N],
            match_index: [0; N],
            epoch: 0,
        }
    }
    fn last_index(&self) -> usize {
        self.p.log.len()
    }
    fn term_at(&self, i: usize) -> u64 {
        if i == 0 { 0 } else { self.p.log[i - 1].term }
    }
    fn last_term(&self) -> u64 {
        self.term_at(self.last_index())
    }
    /// Apply committed entries in order. The state machine only ever sees the committed prefix.
    fn apply(&mut self) {
        while self.applied.len() < self.commit {
            let e = self.p.log[self.applied.len()].clone();
            if let Cmd::Put(k, v) = e.cmd {
                self.kv.insert(k, v);
            }
            self.applied.push(e);
        }
    }
}

// ---------------------------------------------------------------- simulated network and events

#[derive(Clone, Copy)]
struct Net {
    drop_pct: u64,
    dup_pct: u64,
    min_delay: u64,
    max_delay: u64,
    spike_pct: u64,
}

impl Net {
    fn lossy() -> Net {
        Net {
            drop_pct: 10,
            dup_pct: 5,
            min_delay: 5,
            max_delay: 40,
            spike_pct: 5,
        }
    }
    fn calm() -> Net {
        Net {
            drop_pct: 0,
            dup_pct: 0,
            min_delay: 5,
            max_delay: 25,
            spike_pct: 0,
        }
    }
}

enum Ev {
    Deliver {
        from: usize,
        to: usize,
        msg: Msg,
    },
    Election {
        node: usize,
        epoch: u64,
    },
    Heartbeat {
        node: usize,
        epoch: u64,
    },
    Crash(usize),
    Restart(usize),
    Partition([u8; N]),
    Propose(Cmd),
    /// End of the chaos phase: heal, calm the network, restart everyone except `down`.
    Stabilize {
        down: Vec<usize>,
    },
}

macro_rules! say {
    ($s:expr, $($a:tt)*) => {
        if $s.trace { println!("  [{:>5}ms] {}", $s.now, format!($($a)*)); }
    };
}

#[derive(Default, Clone, Debug)]
struct Stats {
    sent: u64,
    dropped: u64,
    duplicated: u64,
    events: u64,
    leaders_elected: u64,
    crashes: u64,
    max_term: u64,
}

// ---------------------------------------------------------------- the invariant checker

/// Global observer. It sees every node's persistent log (even a crashed node's disk) and checks the paper's Figure 3 after
/// every event. It keeps the history that no single node has: leaders per term and the globally committed prefix.
struct Checker {
    leader_of_term: BTreeMap<u64, usize>,
    committed: Vec<Entry>,
    seen_commit: Vec<(u32, usize)>,
    checks: u64,
}

impl Checker {
    fn new() -> Checker {
        Checker {
            leader_of_term: BTreeMap::new(),
            committed: Vec::new(),
            seen_commit: vec![(0, 0); N],
            checks: 0,
        }
    }

    fn check(&mut self, nodes: &[Node]) -> Result<(), String> {
        self.checks += 1;
        self.election_safety(nodes)?;
        self.commit_rules(nodes)?;
        self.state_machine_safety(nodes)?;
        self.leader_completeness(nodes)?;
        self.log_matching(nodes)
    }

    fn election_safety(&mut self, nodes: &[Node]) -> Result<(), String> {
        for (i, n) in nodes.iter().enumerate() {
            if n.up && n.role == Role::Leader {
                let prev = *self.leader_of_term.entry(n.p.term).or_insert(i);
                if prev != i {
                    return Err(format!(
                        "ELECTION SAFETY: nodes {prev} and {i} both lead term {}",
                        n.p.term
                    ));
                }
            }
        }
        Ok(())
    }

    /// Commit index is monotonic within an incarnation and never beyond the log; the global committed prefix only grows
    /// and every node's committed entries agree with it.
    fn commit_rules(&mut self, nodes: &[Node]) -> Result<(), String> {
        for (i, n) in nodes.iter().enumerate() {
            let (inc, seen) = self.seen_commit[i];
            let base = if inc == n.incarnation { seen } else { 0 };
            if n.commit < base {
                return Err(format!(
                    "COMMIT MONOTONIC: node {i} commit went {base} -> {}",
                    n.commit
                ));
            }
            if n.commit > n.last_index() || n.applied.len() != n.commit {
                return Err(format!(
                    "node {i}: commit {} beyond log {} or unapplied",
                    n.commit,
                    n.last_index()
                ));
            }
            self.seen_commit[i] = (n.incarnation, n.commit);
            for idx in 0..n.commit {
                let e = &n.p.log[idx];
                match self.committed.get(idx) {
                    Some(c) if c != e => {
                        return Err(format!(
                            "STATE MACHINE SAFETY: index {} committed as {c:?} and as {e:?}",
                            idx + 1
                        ));
                    }
                    Some(_) => {}
                    None => self.committed.push(e.clone()),
                }
            }
        }
        Ok(())
    }

    fn state_machine_safety(&self, nodes: &[Node]) -> Result<(), String> {
        let longest = nodes
            .iter()
            .map(|n| &n.applied)
            .max_by_key(|a| a.len())
            .unwrap();
        for (i, n) in nodes.iter().enumerate() {
            if n.applied[..] != longest[..n.applied.len()] {
                return Err(format!(
                    "STATE MACHINE SAFETY: node {i} applied sequence is not a prefix of the longest"
                ));
            }
        }
        Ok(())
    }

    /// Every leader of term T holds every entry committed in a term <= T.
    fn leader_completeness(&self, nodes: &[Node]) -> Result<(), String> {
        for (i, n) in nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.up && n.role == Role::Leader)
        {
            for (idx, c) in self.committed.iter().enumerate() {
                if c.term > n.p.term {
                    break;
                }
                if n.p.log.get(idx) != Some(c) {
                    return Err(format!(
                        "LEADER COMPLETENESS: leader {i} (term {}) lacks committed entry {} {c:?}",
                        n.p.term,
                        idx + 1
                    ));
                }
            }
        }
        Ok(())
    }

    /// Same (index, term) in two logs => identical prefixes. The highest agreeing index covers all lower ones.
    fn log_matching(&self, nodes: &[Node]) -> Result<(), String> {
        for a in 0..N {
            for b in a + 1..N {
                let (la, lb) = (&nodes[a].p.log, &nodes[b].p.log);
                let top = (0..la.len().min(lb.len()))
                    .rev()
                    .find(|&i| la[i].term == lb[i].term);
                if let Some(i) = top {
                    if la[..=i] != lb[..=i] {
                        return Err(format!(
                            "LOG MATCHING: nodes {a} and {b} agree on term at index {} but differ before",
                            i + 1
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- the simulator and the Raft rules

struct Sim {
    now: u64,
    seq: u64,
    q: BTreeMap<(u64, u64), Ev>,
    nodes: Vec<Node>,
    rng: Rng,
    net: Net,
    group: [u8; N],
    trace: bool,
    chk: Checker,
    stats: Stats,
    /// Deliberately broken voting rule, used only to show the checker has teeth.
    bug_ignore_log_check: bool,
}

impl Sim {
    fn new(seed: u64, net: Net) -> Sim {
        let mut s = Sim {
            now: 0,
            seq: 0,
            q: BTreeMap::new(),
            nodes: (0..N).map(|_| Node::new()).collect(),
            rng: Rng::new(seed),
            net,
            group: [0; N],
            trace: false,
            chk: Checker::new(),
            stats: Stats::default(),
            bug_ignore_log_check: false,
        };
        for i in 0..N {
            s.reset_timer(i);
        }
        s
    }

    fn schedule(&mut self, at: u64, ev: Ev) {
        self.seq += 1;
        self.q.insert((at, self.seq), ev);
    }

    fn send(&mut self, from: usize, to: usize, msg: Msg) {
        self.stats.sent += 1;
        if self.rng.chance(self.net.drop_pct) {
            self.stats.dropped += 1;
            return;
        }
        let copies = if self.rng.chance(self.net.dup_pct) {
            2
        } else {
            1
        };
        self.stats.duplicated += copies - 1;
        for _ in 0..copies {
            let mut d = self.rng.range(self.net.min_delay, self.net.max_delay);
            if self.rng.chance(self.net.spike_pct) {
                d += self.rng.range(100, 300); // a straggler: overtaken by later messages
            }
            self.schedule(
                self.now + d,
                Ev::Deliver {
                    from,
                    to,
                    msg: msg.clone(),
                },
            );
        }
    }

    /// Randomised timeouts are what makes split votes rare and self-correcting.
    fn reset_timer(&mut self, i: usize) {
        self.nodes[i].epoch += 1;
        let epoch = self.nodes[i].epoch;
        let at = self.now + ELECT_MIN + self.rng.below(ELECT_SPAN);
        self.schedule(at, Ev::Election { node: i, epoch });
    }

    /// Process one event and run the full invariant checker. `Ok(false)` means the queue is empty.
    fn step(&mut self) -> Result<bool, String> {
        let Some(((t, _), ev)) = self.q.pop_first() else {
            return Ok(false);
        };
        self.now = t;
        self.stats.events += 1;
        self.dispatch(ev);
        self.stats.max_term = self
            .stats
            .max_term
            .max(self.nodes.iter().map(|n| n.p.term).max().unwrap());
        self.chk.check(&self.nodes)?;
        Ok(true)
    }

    fn run_until(&mut self, t: u64) -> Result<(), String> {
        while self.q.first_key_value().is_some_and(|(k, _)| k.0 <= t) {
            self.step()?;
        }
        self.now = self.now.max(t);
        Ok(())
    }

    fn dispatch(&mut self, ev: Ev) {
        match ev {
            Ev::Deliver { from, to, msg } => {
                if self.nodes[to].up && self.group[from] == self.group[to] {
                    self.handle(to, from, msg);
                }
            }
            Ev::Election { node, epoch } => {
                let n = &self.nodes[node];
                if n.up && n.epoch == epoch && n.role != Role::Leader {
                    self.start_election(node);
                }
            }
            Ev::Heartbeat { node, epoch } => {
                let n = &self.nodes[node];
                if n.up && n.epoch == epoch && n.role == Role::Leader {
                    self.broadcast_append(node);
                    self.schedule(self.now + HEARTBEAT, Ev::Heartbeat { node, epoch });
                }
            }
            Ev::Crash(i) => self.crash(i),
            Ev::Restart(i) => self.restart(i),
            Ev::Partition(g) => self.group = g,
            Ev::Propose(cmd) => {
                self.propose(cmd);
            }
            Ev::Stabilize { down } => {
                self.group = [0; N];
                self.net = Net::calm();
                for i in 0..N {
                    if down.contains(&i) {
                        self.crash(i);
                    } else {
                        self.restart(i);
                    }
                }
            }
        }
    }

    fn crash(&mut self, i: usize) {
        if !self.nodes[i].up {
            return;
        }
        say!(
            self,
            "node {i} CRASHES (role {:?}, term {}, log len {})",
            self.nodes[i].role,
            self.nodes[i].p.term,
            self.nodes[i].last_index()
        );
        self.stats.crashes += 1;
        let n = &mut self.nodes[i];
        n.up = false;
        n.role = Role::Follower;
    }

    /// Restart keeps only `Persist`: commit index, applied state and leader bookkeeping are rebuilt from the leader.
    fn restart(&mut self, i: usize) {
        if self.nodes[i].up {
            return;
        }
        let n = &mut self.nodes[i];
        n.up = true;
        n.incarnation += 1;
        n.role = Role::Follower;
        n.votes.clear();
        n.commit = 0;
        n.applied.clear();
        n.kv.clear();
        say!(
            self,
            "node {i} RESTARTS from disk: term {}, votedFor {:?}, log len {}, commit forgotten",
            n.p.term,
            n.p.voted_for,
            n.last_index()
        );
        self.reset_timer(i);
    }

    fn isolate(&mut self, i: usize) {
        self.group = [0; N];
        self.group[i] = 1;
    }

    /// Seeing a higher term (in any message) demotes any role: that is how a stale leader learns it was deposed.
    fn step_down(&mut self, i: usize, term: u64) {
        let was = self.nodes[i].role;
        let n = &mut self.nodes[i];
        n.p.term = term;
        n.p.voted_for = None;
        n.role = Role::Follower;
        if was != Role::Follower {
            say!(
                self,
                "node {i} was {was:?}, sees term {term}: steps down to follower"
            );
            self.reset_timer(i);
        }
    }

    fn start_election(&mut self, i: usize) {
        let n = &mut self.nodes[i];
        n.p.term += 1;
        n.p.voted_for = Some(i);
        n.role = Role::Candidate;
        n.votes = BTreeSet::from([i]);
        let (term, last_idx, last_term) = (n.p.term, n.last_index(), n.last_term());
        say!(
            self,
            "node {i} election timeout: becomes CANDIDATE for term {term} (last log {last_idx}@{last_term})"
        );
        self.reset_timer(i);
        for p in (0..N).filter(|&p| p != i) {
            self.send(
                i,
                p,
                Msg::RequestVote {
                    term,
                    cand: i,
                    last_idx,
                    last_term,
                },
            );
        }
    }

    fn become_leader(&mut self, i: usize) {
        let n = &mut self.nodes[i];
        n.role = Role::Leader;
        n.epoch += 1; // cancels the election timer
        let len = n.last_index();
        n.next_index = [len + 1; N];
        n.match_index = [0; N];
        // The no-op lets the leader commit entries of earlier terms indirectly (Figure 8).
        n.p.log.push(Entry {
            term: n.p.term,
            cmd: Cmd::Noop,
        });
        n.match_index[i] = len + 1;
        let (epoch, term) = (n.epoch, n.p.term);
        self.stats.leaders_elected += 1;
        say!(
            self,
            "node {i} wins a majority: LEADER of term {term}, appends a no-op at index {}",
            len + 1
        );
        self.broadcast_append(i);
        self.schedule(self.now + HEARTBEAT, Ev::Heartbeat { node: i, epoch });
    }

    /// The live leader with the highest term, if any.
    fn leader(&self) -> Option<usize> {
        (0..N)
            .filter(|&i| self.nodes[i].up && self.nodes[i].role == Role::Leader)
            .max_by_key(|&i| self.nodes[i].p.term)
    }

    fn propose(&mut self, cmd: Cmd) -> Option<usize> {
        let l = self.leader()?;
        let n = &mut self.nodes[l];
        n.p.log.push(Entry {
            term: n.p.term,
            cmd: cmd.clone(),
        });
        n.match_index[l] = n.last_index();
        say!(
            self,
            "leader {l} appends {cmd:?} at index {} (term {})",
            n.last_index(),
            n.p.term
        );
        self.broadcast_append(l);
        Some(l)
    }

    fn broadcast_append(&mut self, l: usize) {
        for p in (0..N).filter(|&p| p != l) {
            self.send_append(l, p);
        }
    }

    fn send_append(&mut self, l: usize, p: usize) {
        let n = &self.nodes[l];
        let prev = n.next_index[p] - 1;
        let lo = prev.min(n.last_index());
        let hi = (lo + BATCH).min(n.last_index());
        let msg = Msg::Append {
            term: n.p.term,
            prev_idx: lo,
            prev_term: n.term_at(lo),
            entries: n.p.log[lo..hi].to_vec(),
            commit: n.commit,
        };
        self.send(l, p, msg);
    }

    /// Only entries of the leader's own term are committed by counting replicas.
    fn advance_commit(&mut self, l: usize) {
        let n = &mut self.nodes[l];
        let from = n.commit;
        for idx in (n.commit + 1..=n.last_index()).rev() {
            let replicas = (0..N).filter(|&p| n.match_index[p] >= idx).count();
            if replicas >= MAJ && n.term_at(idx) == n.p.term {
                n.commit = idx;
                break;
            }
        }
        if n.commit > from {
            n.apply();
            say!(
                self,
                "leader {l} commit {from} -> {} (majority replicated, term {})",
                self.nodes[l].commit,
                self.nodes[l].p.term
            );
        }
    }

    fn handle(&mut self, to: usize, from: usize, msg: Msg) {
        if msg.term() > self.nodes[to].p.term {
            self.step_down(to, msg.term());
        }
        match msg {
            Msg::RequestVote {
                term,
                cand,
                last_idx,
                last_term,
            } => self.on_request_vote(to, from, term, cand, last_idx, last_term),
            Msg::VoteResp { term, granted } => {
                let n = &mut self.nodes[to];
                if n.role == Role::Candidate && term == n.p.term && granted {
                    n.votes.insert(from);
                    if n.votes.len() >= MAJ {
                        self.become_leader(to);
                    }
                }
            }
            Msg::Append {
                term,
                prev_idx,
                prev_term,
                entries,
                commit,
            } => self.on_append(to, from, term, prev_idx, prev_term, entries, commit),
            Msg::AppendResp {
                term,
                ok,
                match_idx,
                hint,
            } => self.on_append_resp(to, from, term, ok, match_idx, hint),
        }
    }

    fn on_request_vote(
        &mut self,
        to: usize,
        from: usize,
        term: u64,
        cand: usize,
        last_idx: usize,
        last_term: u64,
    ) {
        let n = &mut self.nodes[to];
        let up_to_date = (last_term, last_idx) >= (n.last_term(), n.last_index());
        let log_ok = up_to_date || self.bug_ignore_log_check;
        let grant = term == n.p.term && log_ok && n.p.voted_for.is_none_or(|v| v == cand);
        if grant {
            n.p.voted_for = Some(cand); // persisted before the reply leaves the node
        }
        let reply = Msg::VoteResp {
            term: n.p.term,
            granted: grant,
        };
        if grant {
            say!(self, "node {to} votes for {cand} in term {term}");
            self.reset_timer(to);
        } else if term == self.nodes[to].p.term {
            say!(
                self,
                "node {to} refuses {cand} in term {term} (already voted {:?} or candidate log behind)",
                self.nodes[to].p.voted_for
            );
        }
        self.send(to, from, reply);
    }

    fn on_append(
        &mut self,
        to: usize,
        from: usize,
        term: u64,
        prev_idx: usize,
        prev_term: u64,
        entries: Vec<Entry>,
        commit: usize,
    ) {
        if term < self.nodes[to].p.term {
            let reply = Msg::AppendResp {
                term: self.nodes[to].p.term,
                ok: false,
                match_idx: 0,
                hint: 0,
            };
            self.send(to, from, reply);
            return;
        }
        // A valid leader for this term exists: a candidate of the same term concedes.
        if self.nodes[to].role == Role::Candidate {
            say!(
                self,
                "node {to} (candidate, term {term}) hears the term's leader {from}: becomes follower"
            );
            self.nodes[to].role = Role::Follower;
        }
        self.reset_timer(to);
        let n = &mut self.nodes[to];
        let ok = prev_idx <= n.last_index() && n.term_at(prev_idx) == prev_term;
        if !ok {
            let hint = n.last_index();
            say!(
                self,
                "node {to} REFUSES append from {from}: no entry {prev_idx}@{prev_term} (its log len {hint}); leader will back up"
            );
            let reply = Msg::AppendResp {
                term: n.p.term,
                ok: false,
                match_idx: 0,
                hint,
            };
            self.send(to, from, reply);
            return;
        }
        let mut truncated = 0;
        for (k, e) in entries.iter().enumerate() {
            let idx = prev_idx + 1 + k;
            if idx <= n.last_index() {
                if n.p.log[idx - 1].term == e.term {
                    continue; // already have it: never truncate on a match (duplicates and reordering)
                }
                truncated += n.last_index() - idx + 1;
                n.p.log.truncate(idx - 1);
            }
            n.p.log.push(e.clone());
        }
        let matched = prev_idx + entries.len();
        if commit > n.commit {
            n.commit = n.commit.max(commit.min(matched));
            n.apply();
        }
        let reply = Msg::AppendResp {
            term: n.p.term,
            ok: true,
            match_idx: matched,
            hint: 0,
        };
        if truncated > 0 {
            say!(
                self,
                "node {to} LOG REPAIR: discards {truncated} conflicting entries from the new leader {from}, log now len {}",
                self.nodes[to].last_index()
            );
        }
        self.send(to, from, reply);
    }

    fn on_append_resp(
        &mut self,
        l: usize,
        from: usize,
        term: u64,
        ok: bool,
        match_idx: usize,
        hint: usize,
    ) {
        let n = &mut self.nodes[l];
        if n.role != Role::Leader || term != n.p.term {
            return;
        }
        if ok {
            n.match_index[from] = n.match_index[from].max(match_idx);
            n.next_index[from] = n.next_index[from].max(n.match_index[from] + 1);
            let more = n.next_index[from] <= n.last_index();
            self.advance_commit(l);
            if more {
                self.send_append(l, from);
            }
        } else {
            // Back up, using the follower's length as a hint, but never below what is known to match.
            let floor = n.match_index[from] + 1;
            n.next_index[from] = (n.next_index[from] - 1).min(hint + 1).max(floor).max(1);
            self.send_append(l, from);
        }
    }
}

// ---------------------------------------------------------------- random schedules

struct Outcome {
    events: u64,
    checks: u64,
    committed: usize,
    stats: Stats,
}

/// Chaos phase (crashes, restarts, partitions, loss, client load) then a stable phase with a live majority.
fn run_schedule(seed: u64, bug: bool, check_liveness: bool) -> Result<Outcome, String> {
    let mut s = Sim::new(seed, Net::lossy());
    s.bug_ignore_log_check = bug;
    let mut plan = Rng::new(seed ^ 0xABCD_EF01);
    const CHAOS_END: u64 = 4000;
    for _ in 0..plan.range(3, 9) {
        let (node, t) = (plan.below(N as u64) as usize, plan.range(100, 3300));
        s.schedule(t, Ev::Crash(node));
        s.schedule(t + plan.range(100, 600), Ev::Restart(node));
    }
    for _ in 0..plan.range(1, 5) {
        let t = plan.range(100, 3000);
        let g: [u8; N] = std::array::from_fn(|_| plan.below(2) as u8);
        s.schedule(t, Ev::Partition(g));
        s.schedule(t + plan.range(200, 1000), Ev::Partition([0; N]));
    }
    for k in 0..90u32 {
        s.schedule(100 + 40 * k as u64, Ev::Propose(Cmd::Put(k % 7, k)));
    }
    let down: Vec<usize> = (0..plan.below(3) as usize)
        .map(|j| (j * 2 + plan.below(2) as usize) % N)
        .collect();
    s.schedule(CHAOS_END, Ev::Stabilize { down: down.clone() });
    s.run_until(CHAOS_END + 3000)?;
    if check_liveness {
        liveness(&mut s, &down)?;
    }
    Ok(Outcome {
        events: s.stats.events,
        checks: s.chk.checks,
        committed: s.chk.committed.len(),
        stats: s.stats.clone(),
    })
}

/// With a stable live majority a leader emerges and NEW proposals commit on every live node.
fn liveness(s: &mut Sim, down: &[usize]) -> Result<(), String> {
    let live: Vec<usize> = (0..N).filter(|i| !down.contains(i)).collect();
    assert!(
        live.len() >= MAJ,
        "liveness is only promised with a live majority"
    );
    let Some(l) = s.leader() else {
        return Err("LIVENESS: no leader 3s after stabilisation".into());
    };
    let term = s.nodes[l].p.term;
    if live.iter().any(|&i| s.nodes[i].p.term != term) {
        return Err("LIVENESS: live nodes disagree on the term after 3s".into());
    }
    for i in 0..5u32 {
        let mut done = false;
        for attempt in 0..8u32 {
            let cmd = Cmd::Put(1000 + i, attempt);
            if s.propose(cmd.clone()).is_none() {
                s.run_until(s.now + 400)?;
                continue;
            }
            s.run_until(s.now + 500)?;
            if s.chk.committed.iter().any(|e| e.cmd == cmd) {
                done = true;
                break;
            }
        }
        if !done {
            return Err(format!("LIVENESS: proposal {i} never committed"));
        }
    }
    s.run_until(s.now + 500)?;
    for &i in &live {
        for k in 0..5u32 {
            if !s.nodes[i].kv.contains_key(&(1000 + k)) {
                return Err(format!(
                    "LIVENESS: live node {i} did not apply proposal {k}"
                ));
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- annotated timeline

fn commit_all(s: &mut Sim, keys: std::ops::Range<u32>, base: u32) -> Result<(), String> {
    for k in keys {
        s.propose(Cmd::Put(k, base + k))
            .ok_or("no leader to propose to")?;
        s.run_until(s.now + 30)?;
    }
    Ok(())
}

fn timeline() -> Result<(), String> {
    let mut s = Sim::new(11, Net::calm());
    s.trace = true;
    println!(
        "--- phase 1: cold start. Everyone is a follower; the first timeout starts an election"
    );
    s.run_until(800)?;
    let l1 = s.leader().ok_or("no leader")?;
    println!("--- phase 2: clients write three keys through leader {l1}");
    commit_all(&mut s, 0..3, 100)?;
    s.run_until(s.now + 200)?;
    assert!(
        (0..N).all(|i| s.nodes[i].kv.len() == 3),
        "all five nodes applied the three writes"
    );

    println!("--- phase 3: crash leader {l1}: followers time out and elect a successor (failover)");
    let t1 = s.nodes[l1].p.term;
    s.crash(l1);
    s.run_until(s.now + 800)?;
    let l2 = s.leader().ok_or("no failover leader")?;
    assert!(
        l2 != l1 && s.nodes[l2].p.term > t1,
        "failover elected a different leader in a higher term"
    );
    commit_all(&mut s, 3..5, 100)?;
    s.run_until(s.now + 200)?;
    println!("--- phase 4: old leader {l1} restarts, catches up from its disk state");
    s.restart(l1);
    s.run_until(s.now + 600)?;
    assert_eq!(
        s.nodes[l1].kv, s.nodes[l2].kv,
        "restarted node rebuilt the same state machine"
    );
    assert_eq!(s.nodes[l1].kv.len(), 5);

    println!(
        "--- phase 5: partition leader {l2} away WITH uncommitted writes, then heal: log repair"
    );
    s.isolate(l2);
    for k in 0..3 {
        s.propose(Cmd::Put(900 + k, 666)).ok_or("isolated leader")?; // goes to l2: cannot reach a majority
    }
    s.run_until(s.now + 900)?;
    let l3 = (0..N)
        .filter(|&i| i != l2 && s.nodes[i].role == Role::Leader)
        .next()
        .ok_or("majority side elected no leader")?;
    println!(
        "  (isolated old leader {l2} holds {} entries, 3 never committed; new leader {l3} has term {})",
        s.nodes[l2].last_index(),
        s.nodes[l3].p.term
    );
    commit_all(&mut s, 5..7, 100)?;
    s.run_until(s.now + 300)?;
    s.group = [0; N];
    println!("--- healing: the stale leader {l2} meets a higher term");
    s.run_until(s.now + 800)?;
    let lead = s.leader().ok_or("no leader after heal")?;
    for i in 0..N {
        assert_eq!(
            s.nodes[i].p.log, s.nodes[lead].p.log,
            "node {i} log converged to the leader's"
        );
        assert!(
            !s.nodes[i].kv.contains_key(&900),
            "the stale uncommitted writes must never take effect"
        );
        assert_eq!(s.nodes[i].kv.len(), 7);
    }
    println!(
        "  result: all {N} logs identical ({} entries); old leader {l2}'s 3 uncommitted writes discarded; 0 violations in {} checks",
        s.nodes[lead].last_index(),
        s.chk.checks
    );
    Ok(())
}

fn main() {
    println!("== annotated timeline (calm network, seed 11) ==");
    timeline().expect("timeline scenario");

    println!(
        "\n== 500 random chaos schedules: crashes, partitions, 10% loss, 5% duplication, stragglers; invariants after EVERY event =="
    );
    let mut tot = (0u64, 0u64, 0usize, Stats::default());
    for seed in 0..500u64 {
        let o = run_schedule(seed, false, true).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        let st = &mut tot.3;
        tot.0 += o.events;
        tot.1 += o.checks;
        tot.2 += o.committed;
        st.sent += o.stats.sent;
        st.dropped += o.stats.dropped;
        st.duplicated += o.stats.duplicated;
        st.leaders_elected += o.stats.leaders_elected;
        st.crashes += o.stats.crashes;
        st.max_term = st.max_term.max(o.stats.max_term);
    }
    let (events, checks, committed, st) = tot;
    println!(
        "  500/500 schedules: safety held and, with a stable majority, a leader emerged and 5 new proposals committed on every live node"
    );
    println!(
        "  {events} events, {checks} full invariant checks; {} messages ({} dropped, {} duplicated); {} crashes; {} elections won; highest term {}",
        st.sent, st.dropped, st.duplicated, st.crashes, st.leaders_elected, st.max_term
    );
    println!(
        "  committed entries across all schedules: {committed} (avg {:.1} per schedule)",
        committed as f64 / 500.0
    );

    println!(
        "\n== teeth: the same schedules with a voter that skips the 'candidate log up to date' check =="
    );
    let mut caught = 0;
    let mut first = None;
    for seed in 0..500u64 {
        if let Err(e) = run_schedule(seed, true, false) {
            caught += 1;
            first.get_or_insert((seed, e));
        }
    }
    assert!(caught > 0, "the checker must detect the broken voting rule");
    let (seed, e) = first.unwrap();
    println!("  checker flagged {caught}/500 schedules; first: seed {seed}: {e}");
    println!("  (so the 500 clean runs above are evidence, not an artefact of a blind checker)");
}
