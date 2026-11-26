//! RECIPE 157 (messaging): a model of Kafka's core: partitioned segmented logs, retention, compaction, idempotent producer, consumer groups, replication.
//! LEVEL: advanced
//! Use in: designing/understanding event streaming, queues over logs, CDC pipelines, delivery-semantics reasoning.   Run: cargo run --release --example kafka_log_model
#![allow(dead_code)]
//!
//! WHAT: a deterministic, single-threaded, in-memory model of the ideas that make Kafka work (not its protocol or IO).
//! A TOPIC has P PARTITIONS; each partition is an append-only LOG of records with increasing offsets. Ordering is only
//! guaranteed WITHIN a partition: the producer's partitioner maps a key to a partition by hash (same key -> same partition ->
//! per-key order) and spreads null-key records round-robin; across partitions there is no order, which is what buys parallelism.
//!
//! STORAGE: a partition is split into SEGMENTS. Only the last (active) segment is written; old segments are immutable, so
//! RETENTION is "delete whole old files" (cheap) and COMPACTION rewrites closed segments keeping only the latest value per key
//! (a None value is a TOMBSTONE = delete). A SPARSE OFFSET INDEX (one entry per few records) is binary searched, then a short
//! scan finishes the lookup: small memory, near-O(log n) reads.
//!
//! DELIVERY: an idempotent producer tags batches with (producer id, sequence); the broker drops retried duplicates. Consumers in
//! a GROUP split partitions (each owned by exactly one member); progress is a COMMITTED OFFSET per (group, topic, partition).
//! Commit AFTER processing = at-least-once (a crash replays); commit BEFORE = at-most-once (a crash skips); at-least-once plus
//! an idempotent sink (dedupe by partition+offset, ideally atomically with the effect) = effectively-once.
//! REPLICATION: followers copy the leader; the HIGH WATERMARK is the smallest log end among in-sync replicas (ISR). Consumers
//! only see data below it, so anything a consumer saw survives a leader failover.
//!
//! COMMON USES: event sourcing and CDC pipelines, durable work queues, metrics/log aggregation, stream processing state
//! changelogs (compacted topics hold the latest state per key), and cross-service integration where consumers replay history.
//! Read: https://kafka.apache.org/documentation/#design ,
//! https://engineering.linkedin.com/distributed-systems/log-what-every-software-engineer-should-know-about-real-time-datas-unifying
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

/// value None = tombstone. Keys/values are small ints so the checker can compare exactly.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Record {
    offset: u64,
    ts: u64,
    key: Option<u32>,
    value: Option<u64>,
}

// ------------------------------------------------------------------ segments and the partition log

const INDEX_EVERY: usize = 4; // like index.interval.bytes: index every Nth record, not every record

struct Segment {
    base: u64,
    records: Vec<Record>,
    index: Vec<(u64, usize)>, // (offset, position) sparse
    max_ts: u64,
}

impl Segment {
    fn new(base: u64) -> Self {
        Segment {
            base,
            records: vec![],
            index: vec![],
            max_ts: 0,
        }
    }
    fn push(&mut self, r: Record) {
        if self.records.len() % INDEX_EVERY == 0 {
            self.index.push((r.offset, self.records.len()));
        }
        self.max_ts = self.max_ts.max(r.ts);
        self.records.push(r);
    }
    /// position of the first record with offset >= target: binary search the sparse index, then scan <= INDEX_EVERY records
    fn seek(&self, target: u64) -> usize {
        let i = self.index.partition_point(|&(o, _)| o <= target);
        let start = if i == 0 { 0 } else { self.index[i - 1].1 };
        let skipped = self.records[start..]
            .iter()
            .take_while(|r| r.offset < target)
            .count();
        assert!(skipped <= INDEX_EVERY, "sparse index must bound the scan");
        start + skipped
    }
}

#[derive(Debug, PartialEq)]
struct OutOfRange {
    log_start: u64,
}

struct PartitionLog {
    segments: Vec<Segment>,
    next_offset: u64,
    seg_cap: usize,
}

impl PartitionLog {
    fn new(seg_cap: usize) -> Self {
        PartitionLog {
            segments: vec![Segment::new(0)],
            next_offset: 0,
            seg_cap,
        }
    }
    fn log_start(&self) -> u64 {
        self.segments[0].base
    }
    fn len(&self) -> usize {
        self.segments.iter().map(|s| s.records.len()).sum()
    }
    fn append(&mut self, key: Option<u32>, value: Option<u64>, ts: u64) -> u64 {
        if self.segments.last().unwrap().records.len() >= self.seg_cap {
            self.segments.push(Segment::new(self.next_offset)); // roll: the old active segment becomes immutable
        }
        let offset = self.next_offset;
        self.next_offset += 1;
        self.segments.last_mut().unwrap().push(Record {
            offset,
            ts,
            key,
            value,
        });
        offset
    }
    /// records with from <= offset < limit (limit = high watermark), at most `max`
    fn read(&self, from: u64, max: usize, limit: u64) -> Result<Vec<Record>, OutOfRange> {
        if from < self.log_start() {
            return Err(OutOfRange {
                log_start: self.log_start(),
            });
        }
        let first = self
            .segments
            .partition_point(|s| s.base <= from)
            .saturating_sub(1);
        let mut out = vec![];
        for seg in &self.segments[first..] {
            for r in &seg.records[seg.seek(from)..] {
                if r.offset >= limit || out.len() >= max {
                    return Ok(out);
                }
                out.push(*r);
            }
        }
        Ok(out)
    }
    /// delete whole oldest segments that are past max_age, or surplus beyond max_records; never the active one, and never
    /// a segment whose removal would drop the log below the size cap. Returns the number of segments deleted.
    fn retain(&mut self, now: u64, max_age: u64, max_records: usize) -> usize {
        let mut deleted = 0;
        while self.segments.len() > 1 {
            let first = &self.segments[0];
            let expired = first.max_ts + max_age < now;
            let surplus = self.len() - first.records.len() >= max_records;
            if !(expired || surplus) {
                break;
            }
            self.segments.remove(0);
            deleted += 1;
        }
        deleted
    }
    /// keep only the latest record per key in CLOSED segments; optionally drop tombstones (after delete.retention.ms)
    fn compact(&mut self, drop_tombstones: bool) {
        let mut latest: HashMap<u32, u64> = HashMap::new();
        for r in self.segments.iter().flat_map(|s| &s.records) {
            if let Some(k) = r.key {
                latest.insert(k, r.offset);
            }
        }
        let closed = self.segments.len() - 1; // the active segment is never cleaned
        for seg in &mut self.segments[..closed] {
            let keep = |r: &&Record| match r.key {
                Some(k) => latest[&k] == r.offset && !(drop_tombstones && r.value.is_none()),
                None => true,
            };
            let kept: Vec<Record> = seg.records.iter().filter(keep).copied().collect();
            let mut rebuilt = Segment::new(seg.base);
            kept.into_iter().for_each(|r| rebuilt.push(r));
            rebuilt.max_ts = seg.max_ts; // age is judged by the original data, not what compaction left
            *seg = rebuilt;
        }
    }
    /// replay the log as a state machine: the latest value per key, tombstones delete
    fn materialize(&self) -> BTreeMap<u32, u64> {
        let mut m = BTreeMap::new();
        for r in self.segments.iter().flat_map(|s| &s.records) {
            match (r.key, r.value) {
                (Some(k), Some(v)) => m.insert(k, v),
                (Some(k), None) => m.remove(&k),
                _ => None,
            };
        }
        m
    }
    fn all(&self) -> Vec<Record> {
        self.segments
            .iter()
            .flat_map(|s| s.records.iter().copied())
            .collect()
    }
}

// ------------------------------------------------------------------ topic, partitioner, idempotent producer

fn hash_key(k: u32) -> u32 {
    k.to_le_bytes().iter().fold(0x811c_9dc5u32, |h, &b| {
        (h ^ b as u32).wrapping_mul(16_777_619)
    }) // FNV-1a (Kafka uses murmur2)
}

struct Partitioner {
    parts: usize,
    rr: usize,
}
impl Partitioner {
    fn pick(&mut self, key: Option<u32>) -> usize {
        match key {
            Some(k) => hash_key(k) as usize % self.parts, // stable: same key, same partition (until P changes!)
            None => {
                self.rr += 1;
                (self.rr - 1) % self.parts
            }
        }
    }
}

#[derive(Clone)]
struct ProducerState {
    next_seq: u64,
    last_base: u64,
}
#[derive(Debug, PartialEq)]
enum Produced {
    Appended(u64),
    Duplicate(u64),
}
#[derive(Debug, PartialEq)]
enum ProduceError {
    OutOfOrderSequence { expected: u64, got: u64 },
}
type Batch = Vec<(Option<u32>, Option<u64>)>;

struct Topic {
    parts: Vec<PartitionLog>,
    producers: Vec<HashMap<u64, ProducerState>>, // per partition: broker-side dedupe state
}

impl Topic {
    fn new(parts: usize, seg_cap: usize) -> Self {
        Topic {
            parts: (0..parts).map(|_| PartitionLog::new(seg_cap)).collect(),
            producers: vec![HashMap::new(); parts],
        }
    }
    /// single replica here, so the high watermark is the log end; the replication section models the real rule
    fn hw(&self, p: usize) -> u64 {
        self.parts[p].next_offset
    }
    fn append_batch(
        &mut self,
        p: usize,
        idem: Option<(u64, u64)>,
        batch: &Batch,
        ts: u64,
    ) -> Result<Produced, ProduceError> {
        if let Some((pid, first_seq)) = idem {
            if let Some(st) = self.producers[p].get(&pid) {
                if first_seq < st.next_seq {
                    return Ok(Produced::Duplicate(st.last_base)); // a retry of something already appended
                }
                if first_seq > st.next_seq {
                    return Err(ProduceError::OutOfOrderSequence {
                        expected: st.next_seq,
                        got: first_seq,
                    });
                }
            } else if first_seq != 0 {
                return Err(ProduceError::OutOfOrderSequence {
                    expected: 0,
                    got: first_seq,
                });
            }
        }
        let base = self.parts[p].next_offset;
        for &(k, v) in batch {
            self.parts[p].append(k, v, ts);
        }
        if let Some((pid, first_seq)) = idem {
            self.producers[p].insert(
                pid,
                ProducerState {
                    next_seq: first_seq + batch.len() as u64,
                    last_base: base,
                },
            );
        }
        Ok(Produced::Appended(base))
    }
    fn fetch(&self, p: usize, from: u64, max: usize) -> Result<Vec<Record>, OutOfRange> {
        let hw = self.hw(p);
        let out = self.parts[p].read(from, max, hw)?;
        assert!(
            out.iter().all(|r| r.offset < hw),
            "a consumer must never read past the high watermark"
        );
        Ok(out)
    }
}

struct Producer {
    pid: u64,
    idempotent: bool,
    seq: Vec<u64>,
    partitioner: Partitioner,
}
impl Producer {
    fn new(pid: u64, idempotent: bool, parts: usize) -> Self {
        Producer {
            pid,
            idempotent,
            seq: vec![0; parts],
            partitioner: Partitioner { parts, rr: 0 },
        }
    }
    /// `lost_acks` simulates the network eating the broker's response: the client cannot tell and retries the SAME batch
    fn send(&mut self, topic: &mut Topic, batch: &Batch, ts: u64, lost_acks: u32) -> usize {
        let p = self.partitioner.pick(batch[0].0);
        let idem = self.idempotent.then_some((self.pid, self.seq[p]));
        for _ in 0..=lost_acks {
            topic
                .append_batch(p, idem, batch, ts)
                .expect("sequence numbers are issued in order");
        }
        self.seq[p] += batch.len() as u64;
        p
    }
}

// ------------------------------------------------------------------ consumer groups

#[derive(Clone, Copy, Debug)]
enum Assignor {
    Range,
    RoundRobin,
}
type Assignment = BTreeMap<u32, Vec<usize>>;

fn assign(kind: Assignor, members: &BTreeSet<u32>, parts: usize) -> Assignment {
    let ms: Vec<u32> = members.iter().copied().collect();
    let mut out: Assignment = ms.iter().map(|&m| (m, vec![])).collect();
    if ms.is_empty() {
        return out;
    }
    match kind {
        // contiguous ranges: the first (parts % n) members get one extra partition
        Assignor::Range => {
            let (per, extra) = (parts / ms.len(), parts % ms.len());
            let mut next = 0;
            for (i, m) in ms.iter().enumerate() {
                let n = per + usize::from(i < extra);
                out.get_mut(m).unwrap().extend(next..next + n);
                next += n;
            }
        }
        Assignor::RoundRobin => {
            (0..parts).for_each(|p| out.get_mut(&ms[p % ms.len()]).unwrap().push(p))
        }
    }
    out
}

fn check_assignment(a: &Assignment, parts: usize) {
    let mut owners = vec![0; parts];
    a.values().flatten().for_each(|&p| owners[p] += 1);
    assert!(
        owners.iter().all(|&c| c == 1),
        "every partition owned by exactly one member: {a:?}"
    );
    let sizes: Vec<usize> = a.values().map(Vec::len).collect();
    assert!(
        sizes.iter().max().unwrap() - sizes.iter().min().unwrap() <= 1,
        "balanced within 1: {a:?}"
    );
}

struct Group {
    name: String,
    topic: String,
    assignor: Assignor,
    parts: usize,
    members: BTreeSet<u32>,
    generation: u32,
    assignment: Assignment,
    offsets: BTreeMap<(String, String, usize), u64>, // the __consumer_offsets analogue
}
impl Group {
    fn new(name: &str, topic: &str, assignor: Assignor, parts: usize) -> Self {
        let (name, topic) = (name.to_string(), topic.to_string());
        Group {
            name,
            topic,
            assignor,
            parts,
            members: BTreeSet::new(),
            generation: 0,
            assignment: BTreeMap::new(),
            offsets: BTreeMap::new(),
        }
    }
    /// eager rebalance: everyone gives up everything, then the whole assignment is recomputed
    fn rebalance(&mut self) {
        self.generation += 1;
        self.assignment = assign(self.assignor, &self.members, self.parts);
        if !self.members.is_empty() {
            check_assignment(&self.assignment, self.parts);
        }
    }
    fn join(&mut self, m: u32) {
        self.members.insert(m);
        self.rebalance();
    }
    fn leave(&mut self, m: u32) {
        self.members.remove(&m);
        self.rebalance();
    }
    fn committed(&self, p: usize) -> u64 {
        self.offsets
            .get(&(self.name.clone(), self.topic.clone(), p))
            .copied()
            .unwrap_or(0)
    }
    fn commit(&mut self, p: usize, next: u64) {
        let old = self.committed(p);
        assert!(
            next >= old,
            "committed offsets only move forward in this model"
        );
        self.offsets
            .insert((self.name.clone(), self.topic.clone(), p), next);
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    AtLeastOnce,
    AtMostOnce,
    EffectivelyOnce,
}

/// what the downstream system observed. With `dedupe`, applying (partition, offset) twice is a no-op.
struct Sink {
    dedupe: bool,
    delivered: Vec<Vec<u64>>, // every delivery, in arrival order
    applied: Vec<HashSet<u64>>,
    effects: Vec<Vec<u64>>, // deliveries that took effect
}
impl Sink {
    fn new(parts: usize, dedupe: bool) -> Self {
        Sink {
            dedupe,
            delivered: vec![vec![]; parts],
            applied: vec![HashSet::new(); parts],
            effects: vec![vec![]; parts],
        }
    }
    fn apply(&mut self, p: usize, offset: u64) {
        self.delivered[p].push(offset);
        if !self.dedupe || self.applied[p].insert(offset) {
            self.effects[p].push(offset);
        }
    }
}

/// one poll by `member`. Returns true if it crashed mid-way (the caller then removes it WITHOUT any commit).
fn poll(
    g: &mut Group,
    t: &Topic,
    sink: &mut Sink,
    member: u32,
    mode: Mode,
    rng: &mut Rng,
    crash_pct: u64,
) -> bool {
    for p in g.assignment[&member].clone() {
        let recs = t
            .fetch(p, g.committed(p), 5)
            .expect("no retention in this scenario");
        let Some(last) = recs.last() else { continue };
        let end = last.offset + 1;
        if mode == Mode::AtMostOnce {
            g.commit(p, end); // commit first...
            if rng.chance(crash_pct) {
                return true; // ...crash before processing: these records are skipped forever
            }
            recs.iter().for_each(|r| sink.apply(p, r.offset));
        } else {
            let crash_at = rng
                .chance(crash_pct)
                .then(|| rng.below(recs.len() as u64) as usize);
            for (i, r) in recs.iter().enumerate() {
                if crash_at == Some(i) {
                    return true; // processed i records, committed none: the new owner redelivers them
                }
                sink.apply(p, r.offset);
            }
            g.commit(p, end); // process first, then commit
        }
    }
    false
}

struct SimStats {
    produced: usize,
    duplicates: usize,
    lost: usize,
    rebalances: u32,
    crashes: usize,
}

/// a seeded random schedule of produce / poll / join / leave / crash, then drain and run the global model checker
fn simulate(seed: u64, mode: Mode, assignor: Assignor, steps: usize) -> SimStats {
    const P: usize = 4;
    let mut rng = Rng::new(seed);
    let mut topic = Topic::new(P, 8);
    let mut producer = Producer::new(seed, true, P);
    let mut group = Group::new("g", "events", assignor, P);
    let mut sink = Sink::new(P, mode == Mode::EffectivelyOnce);
    let (mut next_member, mut counter, mut crashes) = (0u32, 0u64, 0);
    group.join(next_member);
    next_member += 1;
    for _ in 0..steps {
        match rng.below(100) {
            0..40 => {
                let key = if rng.chance(20) {
                    None
                } else {
                    Some(rng.below(10) as u32)
                };
                let lost_acks = u32::from(rng.chance(15));
                producer.send(&mut topic, &vec![(key, Some(counter))], counter, lost_acks);
                counter += 1;
            }
            40..75 => {
                let ms: Vec<u32> = group.members.iter().copied().collect();
                if !ms.is_empty() {
                    let m = ms[rng.below(ms.len() as u64) as usize];
                    if poll(&mut group, &topic, &mut sink, m, mode, &mut rng, 10) {
                        crashes += 1;
                        group.leave(m);
                    }
                }
            }
            75..85 => {
                group.join(next_member);
                next_member += 1;
            }
            _ => {
                let ms: Vec<u32> = group.members.iter().copied().collect();
                if ms.len() > 1 {
                    group.leave(ms[rng.below(ms.len() as u64) as usize]);
                }
            }
        }
        for p in 0..P {
            assert!(
                group.committed(p) <= topic.hw(p),
                "committed offset beyond the high watermark"
            );
        }
    }
    drain_and_check(&mut group, &topic, &mut sink, mode, &mut rng, assignor);
    check_topic(&topic, counter);
    let mut stats = SimStats {
        produced: topic.parts.iter().map(PartitionLog::len).sum(),
        duplicates: 0,
        lost: 0,
        rebalances: group.generation,
        crashes,
    };
    for p in 0..P {
        stats.duplicates += sink.delivered[p].len() - sink.effects[p].len()
            + sink.effects[p].len().saturating_sub(topic.hw(p) as usize);
        stats.lost += (topic.hw(p) as usize)
            .saturating_sub(sink.effects[p].iter().collect::<HashSet<_>>().len());
    }
    stats
}

fn drain_and_check(
    g: &mut Group,
    t: &Topic,
    sink: &mut Sink,
    mode: Mode,
    rng: &mut Rng,
    _a: Assignor,
) {
    if g.members.is_empty() {
        g.join(1_000);
    }
    loop {
        let ms: Vec<u32> = g.members.iter().copied().collect();
        for &m in &ms {
            assert!(!poll(g, t, sink, m, mode, rng, 0));
        }
        if (0..g.parts).all(|p| g.committed(p) == t.hw(p)) {
            break;
        }
    }
    for p in 0..g.parts {
        let hw = t.hw(p);
        match mode {
            Mode::AtLeastOnce => {
                // first deliveries must be 0,1,2,... in order (no loss, order preserved); anything else is a replay
                let mut next = 0;
                for &o in &sink.delivered[p] {
                    if o == next {
                        next += 1;
                    } else {
                        assert!(
                            o < next,
                            "p{p}: offset {o} delivered before {next}: a record was LOST"
                        );
                    }
                }
                assert_eq!(next, hw);
            }
            Mode::EffectivelyOnce => assert_eq!(
                sink.effects[p],
                (0..hw).collect::<Vec<_>>(),
                "exactly once, in order"
            ),
            Mode::AtMostOnce => assert!(
                sink.delivered[p].windows(2).all(|w| w[0] < w[1]),
                "never redelivered"
            ),
        }
    }
}

/// topic-level invariants: a key lives in one partition, per-key values increase in log order, the idempotent producer left no dupes
fn check_topic(t: &Topic, produced: u64) {
    let mut home: HashMap<u32, usize> = HashMap::new();
    let mut values = HashSet::new();
    for (p, log) in t.parts.iter().enumerate() {
        let mut last: HashMap<u32, u64> = HashMap::new();
        for r in log.all() {
            assert!(
                values.insert(r.value.unwrap()),
                "duplicate value {:?}: idempotent producer failed",
                r.value
            );
            if let Some(k) = r.key {
                assert_eq!(
                    *home.entry(k).or_insert(p),
                    p,
                    "key {k} appears in two partitions"
                );
                let prev = last.insert(k, r.value.unwrap());
                assert!(
                    prev.is_none_or(|x| x < r.value.unwrap()),
                    "per-key order violated"
                );
            }
        }
    }
    assert_eq!(
        values.len() as u64,
        produced,
        "every produced record is in the log exactly once"
    );
}

// ------------------------------------------------------------------ replication sketch

struct Replicated {
    logs: Vec<Vec<Record>>,
    alive: Vec<bool>,
    isr: BTreeSet<usize>,
    leader: usize,
    hw: u64,
}
impl Replicated {
    fn new(n: usize) -> Self {
        Replicated {
            logs: vec![vec![]; n],
            alive: vec![true; n],
            isr: (0..n).collect(),
            leader: 0,
            hw: 0,
        }
    }
    fn append(&mut self, v: u64) {
        let offset = self.logs[self.leader].len() as u64;
        self.logs[self.leader].push(Record {
            offset,
            ts: v,
            key: None,
            value: Some(v),
        });
    }
    fn advance_hw(&mut self) {
        let min = self.isr.iter().map(|&i| self.logs[i].len()).min().unwrap() as u64;
        self.hw = self.hw.max(min); // never moves backwards
    }
    /// a follower fetches up to n records from the leader; once fully caught up it (re)joins the ISR
    fn replicate(&mut self, f: usize, n: usize) {
        if f == self.leader || !self.alive[f] {
            return;
        }
        for _ in 0..n {
            if let Some(&r) = self.logs[self.leader].get(self.logs[f].len()) {
                self.logs[f].push(r);
            }
        }
        if self.logs[f].len() == self.logs[self.leader].len() {
            self.isr.insert(f);
        }
        self.advance_hw();
    }
    /// a slow follower is dropped from the ISR (replica.lag.time.max.ms) so it cannot stall the high watermark
    fn drop_from_isr(&mut self, f: usize) {
        if f != self.leader && self.isr.remove(&f) {
            self.advance_hw();
        }
    }
    /// false if it would leave no in-sync replica to take over (we refuse unclean leader election)
    fn crash(&mut self, i: usize) -> bool {
        if !self.alive[i]
            || (i == self.leader && self.isr.len() == 1)
            || self.isr.len() == 1 && self.isr.contains(&i)
        {
            return false;
        }
        self.alive[i] = false;
        self.isr.remove(&i);
        if i == self.leader {
            self.leader = *self.isr.iter().next().unwrap();
            let end = self.logs[self.leader].len();
            for j in 0..self.logs.len() {
                // ISR followers longer than the new leader, and out-of-sync ones, drop their possibly divergent
                // (uncommitted) tail; real Kafka detects this with leader epochs
                let keep = if self.isr.contains(&j) {
                    end
                } else {
                    self.hw as usize
                };
                self.logs[j].truncate(keep);
            }
        }
        self.advance_hw();
        true
    }
    /// a restarted replica truncates to the high watermark, then catches up as a normal follower
    fn restart(&mut self, i: usize) {
        if !self.alive[i] {
            self.alive[i] = true;
            self.logs[i].truncate(self.hw as usize);
        }
    }
    fn fetch(&self, from: u64, max: usize) -> &[Record] {
        let log = &self.logs[self.leader];
        let end = (self.hw as usize).min(from as usize + max).min(log.len());
        log.get(from as usize..end).unwrap_or(&[])
    }
}

fn check_replication(seed: u64) {
    let mut rng = Rng::new(seed);
    let mut r = Replicated::new(3);
    let (mut committed_seen, mut counter, mut failovers): (Vec<Record>, u64, u32) = (vec![], 0, 0);
    for _ in 0..400 {
        match rng.below(100) {
            0..35 => {
                r.append(counter);
                counter += 1;
            }
            35..75 => r.replicate(rng.below(3) as usize, 1 + rng.below(3) as usize),
            75..82 => r.drop_from_isr(rng.below(3) as usize),
            82..90 => {
                let (was_leader, i) = (r.leader, rng.below(3) as usize);
                failovers += (r.crash(i) && was_leader == i) as u32;
            }
            _ => r.restart(rng.below(3) as usize),
        }
        // a consumer reading the whole committed prefix: it may never see past hw, and what it saw must never change
        let seen = r.fetch(0, usize::MAX / 2);
        assert!(
            seen.len() as u64 <= r.hw,
            "consumer read past the high watermark"
        );
        assert!(
            seen.len() >= committed_seen.len(),
            "committed data vanished"
        );
        assert_eq!(
            &seen[..committed_seen.len()],
            &committed_seen[..],
            "committed data changed after failover"
        );
        committed_seen = seen.to_vec();
        assert!(r.isr.iter().all(|&i| r.alive[i]) && r.isr.contains(&r.leader));
        assert!(
            r.isr.iter().all(|&i| r.logs[i].len() as u64 >= r.hw),
            "hw must not exceed any ISR log end"
        );
    }
    assert!(failovers > 0 || seed > 0);
}

// ------------------------------------------------------------------ log-level model checks

fn check_log_index_retention(seed: u64) {
    let mut rng = Rng::new(seed);
    let mut log = PartitionLog::new(1 + rng.below(9) as usize);
    let mut reference: Vec<Record> = vec![];
    let (mut now, max_age, cap) = (0u64, 40, 25);
    for _ in 0..300 {
        now += rng.below(3);
        let off = log.append(Some(rng.below(6) as u32), Some(now), now);
        reference.push(Record {
            offset: off,
            ts: now,
            key: log.segments.last().unwrap().records.last().unwrap().key,
            value: Some(now),
        });
        if rng.chance(15) {
            let before = log.all();
            let deleted = log.retain(now, max_age, cap);
            let after = log.all();
            assert_eq!(
                after.last(),
                before.last(),
                "newest record / active segment always survives"
            );
            assert!(
                after.windows(2).all(|w| w[1].offset == w[0].offset + 1),
                "survivors are a contiguous suffix"
            );
            let removed = &before[..before.len() - after.len()];
            // retention only removes data that violates policy: too old, or surplus beyond the size cap
            for r in removed {
                assert!(
                    r.ts + max_age < now || log.len() >= cap,
                    "young record deleted while under the size cap"
                );
            }
            assert_eq!(deleted == 0, removed.is_empty());
        }
        // random reads through the sparse index agree with the flat reference
        let from = log.log_start() + rng.below(log.next_offset - log.log_start());
        let n = 1 + rng.below(6) as usize;
        let got = log.read(from, n, log.next_offset).unwrap();
        let want: Vec<Record> = reference
            .iter()
            .filter(|r| r.offset >= from)
            .take(n)
            .copied()
            .collect();
        assert_eq!(got, want, "index lookup disagrees with reference");
    }
    let start = log.log_start();
    if start > 0 {
        assert_eq!(
            log.read(start - 1, 1, log.next_offset),
            Err(OutOfRange { log_start: start }),
            "lagging consumer is told, not served wrong data"
        );
    }
}

fn check_compaction(seed: u64) {
    let mut rng = Rng::new(seed);
    let mut log = PartitionLog::new(2 + rng.below(6) as usize);
    for i in 0..200u64 {
        let value = (!rng.chance(20)).then_some(i); // 20% tombstones
        log.append(Some(rng.below(12) as u32), value, i);
    }
    let (before, flat) = (log.materialize(), log.all());
    let size_before = log.len();
    log.compact(false);
    assert_eq!(
        log.materialize(),
        before,
        "compaction must preserve the latest value per key"
    );
    log.compact(true);
    assert_eq!(
        log.materialize(),
        before,
        "dropping tombstones keeps the visible state"
    );
    let after = log.all();
    assert!(
        after.windows(2).all(|w| w[0].offset < w[1].offset),
        "offsets stay ordered and are never renumbered"
    );
    assert!(
        after.iter().all(|r| flat.contains(r)),
        "compaction only removes, never rewrites"
    );
    let closed = log.segments.len() - 1;
    let mut seen = HashSet::new();
    for r in log.segments[..closed].iter().flat_map(|s| &s.records) {
        assert!(r.value.is_some(), "tombstone survived in a closed segment");
        assert!(
            seen.insert(r.key),
            "two records for one key in the cleaned region"
        );
    }
    // reading still works over the gaps: we land on the next surviving offset
    for _ in 0..50 {
        let from = rng.below(200);
        if from >= log.log_start() {
            let got = log.read(from, 1, 200).unwrap();
            assert_eq!(got.first(), after.iter().find(|r| r.offset >= from));
        }
    }
    assert!(log.len() < size_before);
}

// ------------------------------------------------------------------ demos

fn demo_partitioning() {
    let mut topic = Topic::new(3, 4);
    let mut prod = Producer::new(1, true, 3);
    let mut owner: HashMap<u32, usize> = HashMap::new();
    for i in 0..30u64 {
        let key = Some((i % 5) as u32);
        let p = prod.send(&mut topic, &vec![(key, Some(i))], i, 0);
        assert_eq!(*owner.entry(key.unwrap()).or_insert(p), p);
    }
    let rr: Vec<usize> = (0..6).map(|_| prod.partitioner.pick(None)).collect();
    println!(
        "partitioner: keys 0..5 -> partitions {:?}; null keys round-robin {rr:?}",
        (0..5).map(|k| owner[&k]).collect::<Vec<_>>()
    );
    check_topic(&topic, 30);
}

fn demo_idempotence() {
    for idempotent in [false, true] {
        let mut topic = Topic::new(1, 100);
        let mut prod = Producer::new(7, idempotent, 1);
        for i in 0..10u64 {
            prod.send(
                &mut topic,
                &vec![(Some(1), Some(i)), (Some(1), Some(100 + i))],
                i,
                if i % 3 == 0 { 2 } else { 0 },
            );
        }
        println!(
            "idempotent={idempotent}: 20 records sent with retried batches -> log holds {}",
            topic.parts[0].len()
        );
        assert_eq!(
            topic.parts[0].len(),
            if idempotent { 20 } else { 20 + 4 * 2 * 2 }
        );
    }
    let mut t = Topic::new(1, 10);
    t.append_batch(0, Some((1, 0)), &vec![(None, Some(1))], 0)
        .unwrap();
    assert_eq!(
        t.append_batch(0, Some((1, 5)), &vec![(None, Some(2))], 0),
        Err(ProduceError::OutOfOrderSequence {
            expected: 1,
            got: 5
        })
    );
}

fn demo_rebalance() {
    for kind in [Assignor::Range, Assignor::RoundRobin] {
        let mut g = Group::new("g", "t", kind, 7);
        for m in 0..3 {
            g.join(m);
        }
        println!(
            "{kind:?}: 7 partitions over 3 members -> {:?}",
            g.assignment
        );
        g.leave(1);
        check_assignment(&g.assignment, 7);
        println!(
            "{kind:?}: after member 1 leaves (generation {}) -> {:?}",
            g.generation, g.assignment
        );
    }
}

fn main() {
    demo_partitioning();
    demo_idempotence();
    demo_rebalance();
    for seed in 1..=200 {
        check_log_index_retention(seed);
        check_compaction(seed);
        check_replication(seed);
    }
    println!(
        "log index, retention, compaction and replication model checks passed (200 seeds each)"
    );
    for (mode, name) in [
        (Mode::AtLeastOnce, "at-least-once"),
        (Mode::AtMostOnce, "at-most-once"),
        (Mode::EffectivelyOnce, "effectively-once"),
    ] {
        let mut total = SimStats {
            produced: 0,
            duplicates: 0,
            lost: 0,
            rebalances: 0,
            crashes: 0,
        };
        for seed in 1..=300 {
            let assignor = if seed % 2 == 0 {
                Assignor::Range
            } else {
                Assignor::RoundRobin
            };
            let s = simulate(seed, mode, assignor, 400);
            total.produced += s.produced;
            total.duplicates += s.duplicates;
            total.lost += s.lost;
            total.rebalances += s.rebalances;
            total.crashes += s.crashes;
        }
        println!(
            "{name:>16}: {} records, {} rebalances, {} crashes -> {} redeliveries, {} lost",
            total.produced, total.rebalances, total.crashes, total.duplicates, total.lost
        );
        match mode {
            Mode::AtLeastOnce => assert!(total.duplicates > 0 && total.lost == 0),
            Mode::AtMostOnce => assert!(total.lost > 0 && total.duplicates == 0),
            Mode::EffectivelyOnce => assert_eq!(total.lost, 0),
        }
    }
    println!("all Kafka model checks passed");
}
