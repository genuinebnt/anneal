//! RECIPE 865 (distributed): stream processing: event-time windows, watermarks, triggers, late data, aligned checkpoint barriers, exactly-once effects, stream-stream joins, backpressure.
//! LEVEL: advanced
//! Use in: understanding what a stream processor promises when data is out of order and machines crash.   Run: cargo run --release --example stream_processing_windows
//! COMMON USES: Apache Flink (watermarks, allowed lateness, side outputs, aligned barriers, two-phase-commit sinks), Google Dataflow / Apache Beam (windows,
//! triggers, accumulation modes), Kafka Streams and Spark Structured Streaming (replayable offsets + state checkpoints), Heron (the CMU time-series talk);
//! real jobs: per-minute metrics rollups, user sessionization, joining impressions to clicks within a time bound, continuous alerting.
//! Course: CMU Database Group time-series talks (Twitter Heron); Chandy-Lamport itself is recipe 404.
#![allow(dead_code)]
//!
//! WHAT: records carry the time something HAPPENED (event time); the engine sees them later, in some other order (processing time). Grouping by arrival
//! gives answers that change with scheduling; grouping by event time gives one answer, but then the engine must decide when a window is complete.
//! THE MODEL (a deterministic simulator; a seeded scheduler picks one small step at a time, so every interleaving is reproducible):
//!   P partitions of a replayable log -> P sources -> bounded FIFO channels (one per source/operator pair, records routed by key) -> N parallel keyed
//!   operators, each with P input channels -> transactional sink.
//! HOW:
//! * WATERMARK W = "I expect no more records with ts <= W". A source broadcasts max_ts_seen - B - 1 (B = out-of-orderness bound); an end-of-partition source
//!   sends i64::MAX. An operator's W is the MINIMUM over its input channels, so the slowest partition holds everyone back (Flink also marks idle partitions
//!   so they do not stall W forever; not modelled here).
//! * WINDOWS [start, end): tumbling, sliding (a record is in about size/slide windows) and session (each record opens [ts, ts+gap); windows of a key that
//!   OVERLAP merge; merely touching ones do not, whereas Flink's TimeWindow.intersects also merges touching ones: see below).
//! * LATENESS: a window is complete when W >= end-1 and its state is kept until W >= end-1+L (L = allowed lateness). A record for a window with end-1+L <= W is
//!   dropped from it; if it is dropped from all of its windows it goes to the LATE side output. A session record is judged after merging with live sessions.
//! * WHY DISORDER <= B + L IS ENOUGH FOR EXACT RESULTS: if no record is more than B+L behind the newest earlier record of its partition, then ts >= W+1-L at every operator.
//!   So its window is still live, and it cannot overlap a session that was already purged (those end at or before W+1-L). Hence: nothing dropped and final
//!   results equal a batch GROUP BY, for ANY interleaving and ANY crash schedule. Asserted on 1000 random streams.
//! * BEYOND THE BOUND results depend on the interleaving (which watermark overtook which record). The oracle therefore reads each operator's consumption trace,
//!   recomputes W by brute force, derives the dropped (record, window) pairs, and expects batch results minus exactly those. After a crash the trace is cut
//!   back to the checkpoint, so it is the history that actually took effect. For sessions beyond the bound a purged session can sit next to a later one that
//!   batch would have merged with it, so only conservation is asserted: counted + dropped == all records, count and sum per key.
//! * TRIGGERS decide when to emit a PANE: on the watermark (one on-time pane, then one per accepted late record), early every n records, or per record.
//!   ACCUMULATION: accumulating panes carry the running total (the sink overwrites); discarding panes carry only the delta (the sink adds). Session merges
//!   must RETRACT panes already emitted for the windows that disappear. (Discarding mode is limited to tumbling/sliding here.)
//! * CHECKPOINTS (Carbone et al., "asynchronous barrier snapshotting", a Chandy-Lamport relative for dataflow DAGs): each source records (offset, max_ts) and
//!   puts barrier n in all its channels. An operator that has barrier n on one channel BLOCKS that channel until it has it on all, then snapshots its state and
//!   its per-channel watermarks. No in-flight records need saving: everything before the barriers is in the state, everything after will be replayed.
//! * EXACTLY-ONCE EFFECTS: output since the last barrier stays in a pending transaction, is pre-committed with the snapshot, and is committed once ALL
//!   operators have acked. A crash can fall between "checkpoint durable" and "sink committed", so recovery re-sends the commit and the sink ignores an epoch
//!   it has applied. Records may be PROCESSED many times; their effect appears once. Output is only visible after a checkpoint: that is the latency price.
//! * WHAT BREAKS IT (run as mutations, and the checks must catch them): not blocking channels (state then contains records the restored offsets will replay:
//!   at-least-once) and a sink that applies output immediately (replayed deltas are added twice). On in-bounds input both only OVER-count: every record
//!   missing from a snapshot comes later in its partition than any watermark in that snapshot, so the B + L argument still protects it.
//! * STREAM-STREAM INTERVAL JOIN: pair (a, b) with equal key and b.ts - a.ts in [lo, hi]. Each side is buffered; the watermark evicts a once W >= a.ts+hi and b
//!   once W >= b.ts-lo, because every record still accepted has ts > W. It runs on the same runtime, so it gets the same crash testing.
//! * BACKPRESSURE: channels are bounded; a source that cannot place a record (and watermark) stalls. Alignment blocks channels, which pushes back to the
//!   sources, but cannot deadlock: a channel is blocked only after its source has already sent that barrier, so sources that still owe it can drain.
//!
//! Read: Akidau et al., "The Dataflow Model", VLDB 2015  https://www.vldb.org/pvldb/vol8/p1792-Akidau.pdf
//! Read: Carbone et al., "Lightweight Asynchronous Snapshots for Distributed Dataflows"  https://arxiv.org/abs/1506.08603
//! Read: Chandy and Lamport, "Distributed Snapshots", 1985  https://lamport.azurewebsites.net/pubs/chandy.pdf
//! Read: Flink docs, timely stream processing  https://nightlies.apache.org/flink/flink-docs-stable/docs/concepts/time/
//! Read: Flink docs, stateful stream processing and checkpoints  https://nightlies.apache.org/flink/flink-docs-stable/docs/concepts/stateful-stream-processing/
//! Read: Flink `TimeWindow.java` (`intersects`)  https://github.com/apache/flink/blob/master/flink-runtime/src/main/java/org/apache/flink/streaming/api/windowing/windows/TimeWindow.java
//!
//! TRY NEXT:
//!   - [S] In `WindowOp::on_watermark`, purge a window as soon as it has fired (ignore `lateness`). Predict which of the in-bounds and out-of-bounds checks
//!     fails first and with which window kind, then run.
//!   - [S] Make `Runtime::op_watermark` take the MAXIMUM over channels instead of the minimum. Which assertion notices, and why does a single partition hide it?
//!   - [M] Add idle-partition detection (a source that emits nothing for k steps is excluded from the minimum until it speaks again). Which exactness argument
//!     in the header no longer holds, and what does the oracle need to know?
//!   - [M] Replace the full scan in `WindowOp::on_watermark` with a timer queue ordered by firing time; session merges must delete stale timers. Compare the
//!     number of windows visited before and after.
//!   - [A] PROJECT: implement UNALIGNED checkpoints (Flink offers them): do not block, but save the records that overtake the barrier as channel state and replay
//!     them on restore. The `Mode::NoAlignment` mutation is the broken half of this; make it exact and check it with the same oracle.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

// ---------------------------------------------------------------- rng
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        (self.next() >> 8) % n
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.below((hi - lo + 1) as u64) as i64
    }
}

// ---------------------------------------------------------------- data model
#[derive(Clone, Copy, Debug, PartialEq)]
struct Rec {
    id: u32,
    stream: u8, // 0 or 1; only the join looks at it
    key: u32,
    ts: i64,
    val: i64,
}
#[derive(Clone, Copy, Debug)]
enum Msg {
    Record(Rec),
    Watermark(i64),
    Barrier(u64),
}
/// A mergeable aggregate (needed for session merges and for adding discarding-mode deltas).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Agg {
    count: u64,
    sum: i64,
    min: i64,
    max: i64,
}
impl Agg {
    const EMPTY: Agg = Agg {
        count: 0,
        sum: 0,
        min: i64::MAX,
        max: i64::MIN,
    };
    fn add(&mut self, v: i64) {
        self.merge(&Agg {
            count: 1,
            sum: v,
            min: v,
            max: v,
        });
    }
    fn merge(&mut self, o: &Agg) {
        self.count += o.count;
        self.sum += o.sum;
        self.min = self.min.min(o.min);
        self.max = self.max.max(o.max);
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Tumbling { size: i64 },
    Sliding { size: i64, slide: i64 },
    Session { gap: i64 },
}
impl Kind {
    /// The windows a record is assigned to (for sessions: its not-yet-merged proto-window).
    fn assign(&self, ts: i64) -> Vec<(i64, i64)> {
        match *self {
            Kind::Tumbling { size } => {
                let s = ts - ts.rem_euclid(size);
                vec![(s, s + size)]
            }
            Kind::Sliding { size, slide } => {
                let mut s = ts - ts.rem_euclid(slide); // latest window start <= ts
                let mut out = vec![];
                while s + size > ts {
                    out.push((s, s + size));
                    s -= slide;
                }
                out
            }
            Kind::Session { gap } => vec![(ts, ts + gap)],
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Trigger {
    OnWatermark,
    EarlyEvery(u64),
    PerRecord,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Accumulation {
    Accumulating,
    Discarding,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Timing {
    Early,
    OnTime,
    Late,
}
type WinKey = (u32, i64, i64); // (key, start, end)
#[derive(Clone, Debug, PartialEq)]
enum Out {
    Pane {
        win: WinKey,
        agg: Agg,
        timing: Timing,
        mode: Accumulation,
        fired_at: i64,
    },
    Retract {
        win: WinKey,
    },
    LateRecord {
        id: u32,
    },
    Joined {
        a: u32,
        b: u32,
    },
}
/// Allowed lateness test, used by the operators. `saturating_add` because the final watermark is i64::MAX.
fn expired(end: i64, lateness: i64, w: i64) -> bool {
    (end - 1).saturating_add(lateness) <= w
}

/// An operator is deterministic state + two callbacks. `Clone` IS the snapshot: nothing outside the value may influence its behaviour.
trait Operator: Clone {
    fn on_record(&mut self, r: Rec, w: i64, out: &mut Vec<Out>);
    fn on_watermark(&mut self, w: i64, out: &mut Vec<Out>);
    fn state_size(&self) -> usize;
}

// ---------------------------------------------------------------- keyed window operator
#[derive(Clone, Debug)]
struct WinState {
    total: Agg,
    delta: Agg, // since the last pane
    on_time_done: bool,
}
#[derive(Clone)]
struct WindowOp {
    kind: Kind,
    lateness: i64,
    trigger: Trigger,
    mode: Accumulation,
    windows: BTreeMap<WinKey, WinState>, // keyed state
    dropped_pairs: u64,                  // part of the snapshot, so replays do not count twice
}
impl WindowOp {
    fn new(kind: Kind, lateness: i64, trigger: Trigger, mode: Accumulation) -> Self {
        assert!(
            !(matches!(kind, Kind::Session { .. }) && mode == Accumulation::Discarding),
            "discarding sessions would need delta retractions"
        );
        WindowOp {
            kind,
            lateness,
            trigger,
            mode,
            windows: BTreeMap::new(),
            dropped_pairs: 0,
        }
    }
    fn emit(
        mode: Accumulation,
        win: WinKey,
        st: &mut WinState,
        timing: Timing,
        w: i64,
        out: &mut Vec<Out>,
    ) {
        let agg = match mode {
            Accumulation::Accumulating => st.total,
            Accumulation::Discarding => st.delta,
        };
        st.delta = Agg::EMPTY;
        out.push(Out::Pane {
            win,
            agg,
            timing,
            mode,
            fired_at: w,
        });
    }
    /// Put `val` into window `win` (already merged for sessions) and fire whatever the trigger asks for.
    fn update(&mut self, win: WinKey, mut st: WinState, val: i64, w: i64, out: &mut Vec<Out>) {
        st.total.add(val);
        st.delta.add(val);
        if w >= win.2 - 1 {
            // the watermark already passed the end: a late but allowed record refines the result at once
            let timing = if st.on_time_done {
                Timing::Late
            } else {
                Timing::OnTime
            };
            st.on_time_done = true;
            Self::emit(self.mode, win, &mut st, timing, w, out);
        } else {
            let early = match self.trigger {
                Trigger::OnWatermark => false,
                Trigger::EarlyEvery(n) => st.delta.count >= n,
                Trigger::PerRecord => true,
            };
            if early {
                Self::emit(self.mode, win, &mut st, Timing::Early, w, out);
            }
        }
        self.windows.insert(win, st);
    }
}
impl Operator for WindowOp {
    fn on_record(&mut self, r: Rec, w: i64, out: &mut Vec<Out>) {
        let fresh = WinState {
            total: Agg::EMPTY,
            delta: Agg::EMPTY,
            on_time_done: false,
        };
        let assigned = self.kind.assign(r.ts);
        let mut accepted = 0;
        for &(mut start, mut end) in &assigned {
            if let Kind::Session { .. } = self.kind {
                // merge with every LIVE session of this key that overlaps the proto-window
                let hits: Vec<WinKey> = self
                    .windows
                    .range((r.key, i64::MIN, i64::MIN)..=(r.key, i64::MAX, i64::MAX))
                    .map(|(k, _)| *k)
                    .filter(|k| k.1 < end && start < k.2)
                    .collect();
                let (ms, me) = hits
                    .iter()
                    .fold((start, end), |a, k| (a.0.min(k.1), a.1.max(k.2)));
                if expired(me, self.lateness, w) {
                    debug_assert!(hits.is_empty(), "a live session cannot be expired");
                    self.dropped_pairs += 1;
                    continue;
                }
                (start, end) = (ms, me);
                let mut merged = fresh.clone();
                for k in hits {
                    let old = self.windows.remove(&k).unwrap();
                    merged.total.merge(&old.total);
                    // accumulating mode: the new window's next pane carries the full total, so panes of vanished windows are retracted
                    merged.delta.merge(&old.delta);
                    if k == (r.key, start, end) {
                        merged.on_time_done = old.on_time_done;
                    } else if old.delta.count < old.total.count {
                        out.push(Out::Retract { win: k }); // at least one pane had been emitted for it
                    }
                }
                self.update((r.key, start, end), merged, r.val, w, out);
            } else {
                if expired(end, self.lateness, w) {
                    self.dropped_pairs += 1;
                    continue;
                }
                let win = (r.key, start, end);
                let st = self.windows.remove(&win).unwrap_or(fresh.clone());
                self.update(win, st, r.val, w, out);
            }
            accepted += 1;
        }
        if accepted == 0 {
            out.push(Out::LateRecord { id: r.id });
        }
    }
    fn on_watermark(&mut self, w: i64, out: &mut Vec<Out>) {
        let (mode, lateness) = (self.mode, self.lateness);
        self.windows.retain(|&win, st| {
            if w >= win.2 - 1 && !st.on_time_done {
                st.on_time_done = true;
                // always fire on completion, even if early panes already carried everything (that confirms the result as complete)
                Self::emit(mode, win, st, Timing::OnTime, w, out);
            }
            !expired(win.2, lateness, w) // state cleanup
        });
    }
    fn state_size(&self) -> usize {
        self.windows.len()
    }
}

// ---------------------------------------------------------------- stream-stream interval join
#[derive(Clone)]
struct JoinOp {
    lo: i64,
    hi: i64,                                  // match iff b.ts - a.ts in [lo, hi]
    buf: [BTreeMap<(u32, i64, u32), Rec>; 2], // per side: (key, ts, id)
}
impl JoinOp {
    fn new(lo: i64, hi: i64) -> Self {
        assert!(lo <= hi);
        JoinOp {
            lo,
            hi,
            buf: [BTreeMap::new(), BTreeMap::new()],
        }
    }
}
impl Operator for JoinOp {
    fn on_record(&mut self, r: Rec, w: i64, out: &mut Vec<Out>) {
        if r.ts <= w {
            out.push(Out::LateRecord { id: r.id });
            return;
        }
        let side = r.stream as usize;
        // the time range on the OTHER side that matches r
        let (from, to) = if side == 0 {
            (r.ts + self.lo, r.ts + self.hi)
        } else {
            (r.ts - self.hi, r.ts - self.lo)
        };
        for (_, o) in self.buf[1 - side].range((r.key, from, 0)..=(r.key, to, u32::MAX)) {
            let (a, b) = if side == 0 {
                (r.id, o.id)
            } else {
                (o.id, r.id)
            };
            out.push(Out::Joined { a, b });
        }
        self.buf[side].insert((r.key, r.ts, r.id), r);
    }
    fn on_watermark(&mut self, w: i64, _out: &mut Vec<Out>) {
        let (lo, hi) = (self.lo, self.hi);
        self.buf[0].retain(|_, a| a.ts.saturating_add(hi) > w); // a future b has ts > w; it matches a only if ts <= a.ts+hi
        self.buf[1].retain(|_, b| b.ts.saturating_sub(lo) > w); // a future a has ts > w; it matches b only if ts <= b.ts-lo
    }
    fn state_size(&self) -> usize {
        self.buf[0].len() + self.buf[1].len()
    }
}

// ---------------------------------------------------------------- transactional sink
#[derive(Default)]
struct Sink {
    table: BTreeMap<WinKey, Agg>,
    late: Vec<u32>,
    joined: Vec<(u32, u32)>,
    panes: Vec<Out>, // every applied pane, for trigger statistics and checks
    last_epoch: u64,
}
impl Sink {
    fn apply(&mut self, o: &Out) {
        match o {
            Out::Pane { win, agg, mode, .. } => {
                match mode {
                    Accumulation::Accumulating => {
                        self.table.insert(*win, *agg);
                    }
                    Accumulation::Discarding => {
                        self.table.entry(*win).or_insert(Agg::EMPTY).merge(agg)
                    }
                }
                self.panes.push(o.clone());
            }
            Out::Retract { win } => {
                self.table.remove(win);
            }
            Out::LateRecord { id } => self.late.push(*id),
            Out::Joined { a, b } => self.joined.push((*a, *b)),
        }
    }
    /// Phase two of the two-phase commit. Idempotent by epoch number.
    fn commit(&mut self, epoch: u64, outs: &[Out]) -> bool {
        if epoch <= self.last_epoch {
            return false;
        }
        outs.iter().for_each(|o| self.apply(o));
        self.last_epoch = epoch;
        true
    }
}

// ---------------------------------------------------------------- runtime
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    ExactlyOnce,
    NoAlignment,   // mutation: barriers do not block their channel
    ImmediateSink, // mutation: output is applied as soon as it is produced
}
#[derive(Clone, Copy, Debug)]
struct Config {
    ops: usize,
    bound: i64, // watermark out-of-orderness bound B
    capacity: usize,
    mode: Mode,
    checkpoint_every: u64, // 1 in n scheduler steps starts a checkpoint (if none is in flight)
    crash_every: u64,      // 1 in n scheduler steps crashes (0 = never)
    max_crashes: u32,
    op_speed: u64, // operator scheduling weight (sources have weight 4): small = slow consumers
}
#[derive(Clone)]
struct SourceState {
    offset: usize,
    max_ts: i64,
    end_sent: bool,
}
#[derive(Clone)]
struct OpSlot<O> {
    op: O,
    input_wm: Vec<i64>,
    pending: Vec<Out>,     // the open transaction
    barrier_in: Vec<bool>, // which inputs delivered the current barrier
    trace: usize,          // length of the consumption trace (so a restore can cut it back)
}
/// What survives a crash: the last COMPLETE checkpoint and its pre-committed output.
#[derive(Clone)]
struct Checkpoint<O> {
    id: u64,
    sources: Vec<SourceState>,
    ops: Vec<OpSlot<O>>,
    precommitted: Vec<Out>,
    final_one: bool, // taken after all input had been consumed
}
struct InFlight<O> {
    id: u64,
    barrier_owed: Vec<bool>, // per source
    sources: Vec<Option<SourceState>>,
    ops: Vec<Option<(OpSlot<O>, Vec<Out>)>>,
    drained_at_start: bool,
}
#[derive(Default, Debug, Clone)]
struct Stats {
    steps: u64,
    delivered: u64, // records consumed by operators, replays included
    checkpoints: u64,
    crashes: u32,
    crashes_mid_checkpoint: u64,
    crashes_before_commit: u64,
    duplicate_commits_ignored: u64,
    source_stalls: u64, // (scheduler step, source) pairs in which a source had something to send but no room
    max_depth: usize,
    max_state: usize,
}
struct Runtime<'a, O: Operator> {
    cfg: Config,
    log: &'a [Vec<Rec>], // the replayable, durable input
    sources: Vec<SourceState>,
    chans: Vec<Vec<VecDeque<Msg>>>, // chans[source][op]
    ops: Vec<OpSlot<O>>,
    /// Per operator: (input channel, message) in consumption order. Test instrumentation for the oracle, not engine state.
    traces: Vec<Vec<(usize, Msg)>>,
    inflight: Option<InFlight<O>>,
    durable: Checkpoint<O>,
    committed: bool, // has the sink applied `durable`?
    next_id: u64,
    sink: Sink,
    stats: Stats,
}
#[derive(Clone, Copy)]
enum Action {
    Source(usize),
    Consume(usize, usize), // (source channel, operator)
    Commit,
}
impl<'a, O: Operator> Runtime<'a, O> {
    fn new(cfg: Config, log: &'a [Vec<Rec>], op: O) -> Self {
        assert!(cfg.capacity >= 2, "a record and its watermark must fit");
        let p = log.len();
        let sources = vec![
            SourceState {
                offset: 0,
                max_ts: i64::MIN,
                end_sent: false
            };
            p
        ];
        let slot = OpSlot {
            op,
            input_wm: vec![i64::MIN; p],
            pending: vec![],
            barrier_in: vec![false; p],
            trace: 0,
        };
        let ops = vec![slot; cfg.ops];
        let durable = Checkpoint {
            id: 0,
            sources: sources.clone(),
            ops: ops.clone(),
            precommitted: vec![],
            final_one: false,
        };
        Runtime {
            cfg,
            log,
            sources,
            chans: vec![vec![VecDeque::new(); cfg.ops]; p],
            ops,
            traces: vec![vec![]; cfg.ops],
            inflight: None,
            durable,
            committed: true,
            next_id: 1,
            sink: Sink::default(),
            stats: Stats::default(),
        }
    }
    fn route(&self, r: &Rec) -> usize {
        r.key as usize % self.cfg.ops
    }
    fn room(&self, s: usize, need: usize) -> bool {
        self.chans[s]
            .iter()
            .all(|c| c.len() + need <= self.cfg.capacity)
    }
    fn owes_barrier(&self, s: usize) -> bool {
        self.inflight.as_ref().is_some_and(|f| f.barrier_owed[s])
    }
    /// Free slots source `s` needs in each of its channels for its next step; None = it has nothing left to send.
    fn source_need(&self, s: usize) -> Option<usize> {
        let st = &self.sources[s];
        if self.owes_barrier(s) {
            Some(1)
        } else if st.offset < self.log[s].len() {
            Some(2) // the record and possibly a watermark
        } else {
            (!st.end_sent).then_some(1)
        }
    }
    fn push_all(&mut self, s: usize, m: Msg) {
        for c in &mut self.chans[s] {
            c.push_back(m);
        }
    }
    /// One source step: an owed barrier first, else the next record (plus watermark), else the end-of-partition watermark.
    fn source_step(&mut self, s: usize) {
        let barrier = self.owes_barrier(s);
        let has_record = self.sources[s].offset < self.log[s].len();
        debug_assert!(
            self.source_need(s).is_some_and(|n| self.room(s, n)),
            "the scheduler only runs enabled steps"
        );
        if barrier {
            let state = self.sources[s].clone();
            let f = self.inflight.as_mut().unwrap();
            f.barrier_owed[s] = false;
            f.sources[s] = Some(state);
            let id = f.id;
            self.push_all(s, Msg::Barrier(id));
        } else if has_record {
            let r = self.log[s][self.sources[s].offset];
            self.sources[s].offset += 1;
            let o = self.route(&r);
            self.chans[s][o].push_back(Msg::Record(r));
            if r.ts > self.sources[s].max_ts {
                self.sources[s].max_ts = r.ts;
                self.push_all(s, Msg::Watermark(r.ts - self.cfg.bound - 1));
            }
        } else {
            self.sources[s].end_sent = true;
            self.push_all(s, Msg::Watermark(i64::MAX));
        }
        self.stats.max_depth = self
            .stats
            .max_depth
            .max(self.chans[s].iter().map(|c| c.len()).max().unwrap());
    }
    fn op_watermark(slot: &OpSlot<O>) -> i64 {
        *slot.input_wm.iter().min().unwrap()
    }
    fn consume(&mut self, s: usize, o: usize) {
        let msg = self.chans[s][o].pop_front().unwrap();
        let slot = &mut self.ops[o];
        let before = Self::op_watermark(slot);
        let mut out = vec![];
        match msg {
            Msg::Record(r) => {
                self.traces[o].push((s, msg));
                slot.trace = self.traces[o].len();
                self.stats.delivered += 1;
                slot.op.on_record(r, before, &mut out);
            }
            Msg::Watermark(w) => {
                self.traces[o].push((s, msg));
                slot.trace = self.traces[o].len();
                // Only alignment guarantees this: the NoAlignment mutant can restore a channel watermark that is ahead of its rewound source.
                assert!(
                    self.cfg.mode == Mode::NoAlignment || w >= slot.input_wm[s],
                    "watermarks are monotone per channel"
                );
                slot.input_wm[s] = w.max(slot.input_wm[s]);
                let after = Self::op_watermark(slot);
                if after > before {
                    slot.op.on_watermark(after, &mut out);
                }
            }
            Msg::Barrier(id) => {
                assert_eq!(
                    self.inflight.as_ref().map(|f| f.id),
                    Some(id),
                    "only one checkpoint is ever in flight"
                );
                assert!(!slot.barrier_in[s]);
                slot.barrier_in[s] = true;
                if slot.barrier_in.iter().all(|&b| b) {
                    // aligned: snapshot, pre-commit the open transaction, unblock
                    slot.barrier_in.iter_mut().for_each(|b| *b = false);
                    let txn = std::mem::take(&mut slot.pending);
                    let snapshot = slot.clone();
                    self.inflight.as_mut().unwrap().ops[o] = Some((snapshot, txn));
                    self.try_complete();
                }
            }
        }
        self.stats.max_state = self.stats.max_state.max(self.ops[o].op.state_size());
        if self.cfg.mode == Mode::ImmediateSink {
            out.iter().for_each(|x| self.sink.apply(x));
        } else {
            self.ops[o].pending.extend(out);
        }
    }
    /// All operators acked (so every source has sent the barrier): the checkpoint becomes durable atomically.
    fn try_complete(&mut self) {
        if self
            .inflight
            .as_ref()
            .is_some_and(|f| f.ops.iter().all(|a| a.is_some()))
        {
            let f = self.inflight.take().unwrap();
            let (ops, txns): (Vec<_>, Vec<_>) = f.ops.into_iter().map(|a| a.unwrap()).unzip();
            self.durable = Checkpoint {
                id: f.id,
                sources: f.sources.into_iter().map(|s| s.unwrap()).collect(),
                ops,
                precommitted: txns.concat(),
                final_one: f.drained_at_start,
            };
            self.committed = false;
            self.stats.checkpoints += 1;
        }
    }
    fn drained(&self) -> bool {
        self.sources
            .iter()
            .zip(self.log)
            .all(|(s, l)| s.end_sent && s.offset == l.len())
            && self.chans.iter().flatten().all(|c| c.is_empty())
    }
    fn start_checkpoint(&mut self) {
        let drained_at_start = self.drained();
        let (p, n) = (self.log.len(), self.cfg.ops);
        self.inflight = Some(InFlight {
            id: self.next_id,
            barrier_owed: vec![true; p],
            sources: vec![None; p],
            ops: (0..n).map(|_| None).collect(),
            drained_at_start,
        });
        self.next_id += 1; // ids are never reused, even after a crash aborted one
    }
    fn commit(&mut self) {
        if !self
            .sink
            .commit(self.durable.id, &self.durable.precommitted)
        {
            self.stats.duplicate_commits_ignored += 1;
        }
        self.committed = true;
    }
    fn crash_and_recover(&mut self) {
        self.stats.crashes += 1;
        self.stats.crashes_mid_checkpoint += self.inflight.is_some() as u64;
        self.stats.crashes_before_commit += (!self.committed) as u64;
        // everything volatile is gone: channels, operator state, open transactions, the half-done checkpoint
        self.chans.iter_mut().flatten().for_each(|c| c.clear());
        self.inflight = None;
        self.sources = self.durable.sources.clone();
        self.ops = self.durable.ops.clone();
        for (t, slot) in self.traces.iter_mut().zip(&self.ops) {
            t.truncate(slot.trace);
        }
        // the recovering coordinator cannot know whether the commit reached the sink, so it always sends it again
        if self.durable.id > 0 {
            self.commit();
        }
    }
    /// Run to completion under a random schedule. Done = a checkpoint that started with all input consumed has been committed.
    fn run(&mut self, rng: &mut Rng) {
        loop {
            if self.committed && self.durable.final_one {
                break;
            }
            self.stats.steps += 1;
            assert!(
                self.stats.steps < 2_000_000,
                "no progress (deadlock or livelock)"
            );
            if self.cfg.crash_every > 0
                && self.stats.crashes < self.cfg.max_crashes
                && rng.below(self.cfg.crash_every) == 0
            {
                self.crash_and_recover();
                continue;
            }
            if self.inflight.is_none()
                && self.committed
                && (self.drained() || rng.below(self.cfg.checkpoint_every) == 0)
            {
                self.start_checkpoint();
            }
            let mut acts: Vec<(Action, u64)> = vec![];
            for s in 0..self.log.len() {
                match self.source_need(s) {
                    Some(n) if self.room(s, n) => acts.push((Action::Source(s), 4)),
                    Some(_) => self.stats.source_stalls += 1, // backpressure
                    None => {}
                }
                for o in 0..self.cfg.ops {
                    // alignment: a channel that already delivered the barrier is not read
                    let blocked = self.cfg.mode != Mode::NoAlignment && self.ops[o].barrier_in[s];
                    // without alignment, a SECOND barrier cannot arrive early: only one checkpoint is in flight
                    if !blocked && !self.chans[s][o].is_empty() {
                        acts.push((Action::Consume(s, o), self.cfg.op_speed));
                    }
                }
            }
            if !self.committed {
                acts.push((Action::Commit, 2));
            }
            assert!(!acts.is_empty(), "deadlock: nothing can move");
            let mut pick = rng.below(acts.iter().map(|a| a.1).sum());
            for (a, weight) in acts {
                if pick < weight {
                    match a {
                        Action::Source(s) => self.source_step(s),
                        Action::Consume(s, o) => self.consume(s, o),
                        Action::Commit => self.commit(),
                    }
                    break;
                }
                pick -= weight;
            }
        }
        assert!(
            self.ops.iter().all(|o| o.op.state_size() == 0),
            "the final watermark must clean up all state"
        );
    }
}

// ---------------------------------------------------------------- input generation
/// `partitions` logs of records whose event time is at most `disorder` behind the newest earlier record of the same partition.
/// With `stragglers`, about 1 record in 12 is delayed much further.
fn gen_log(
    rng: &mut Rng,
    partitions: usize,
    per_partition: usize,
    keys: u64,
    disorder: i64,
    stragglers: Option<i64>,
) -> Vec<Vec<Rec>> {
    let mut id = 0;
    let origin = rng.range(-300, 300); // negative event times exercise euclidean window alignment
    (0..partitions)
        .map(|_| {
            let mut front = origin + rng.range(0, 20);
            let n = rng.below(per_partition as u64 + 1) as usize;
            (0..n)
                .map(|_| {
                    front += if rng.below(15) == 0 {
                        rng.range(10, 60)
                    } else {
                        rng.range(0, 3)
                    }; // occasional gaps separate sessions
                    let mut delay = rng.range(0, disorder);
                    if let Some(extra) = stragglers {
                        if rng.below(12) == 0 {
                            delay = disorder + rng.range(1, extra);
                        }
                    }
                    id += 1;
                    Rec {
                        id,
                        stream: rng.below(2) as u8,
                        key: rng.below(keys) as u32,
                        ts: front - delay,
                        val: rng.range(-100, 100),
                    }
                })
                .collect()
        })
        .collect()
}
/// How far behind its partition's running maximum the latest straggler is.
fn max_disorder(log: &[Vec<Rec>]) -> i64 {
    log.iter()
        .map(|p| {
            p.iter()
                .fold((i64::MIN, 0), |(seen, worst), r| {
                    let seen = seen.max(r.ts);
                    (seen, i64::max(worst, seen - r.ts))
                })
                .1
        })
        .max()
        .unwrap_or(0)
}

// ---------------------------------------------------------------- oracles (batch code that shares only `Kind::assign` with the engine)
/// Batch GROUP BY over all records, leaving out the (record id, window start) pairs in `dropped`.
fn batch_windows(
    log: &[Vec<Rec>],
    kind: Kind,
    dropped: &BTreeSet<(u32, i64)>,
) -> BTreeMap<WinKey, Agg> {
    let mut out: BTreeMap<WinKey, Agg> = BTreeMap::new();
    if let Kind::Session { gap } = kind {
        assert!(dropped.is_empty());
        let mut by_key: BTreeMap<u32, Vec<(i64, i64)>> = BTreeMap::new();
        for r in log.iter().flatten() {
            by_key.entry(r.key).or_default().push((r.ts, r.val));
        }
        for (key, mut rows) in by_key {
            rows.sort();
            let mut i = 0;
            while i < rows.len() {
                let mut j = i;
                let mut agg = Agg::EMPTY;
                agg.add(rows[i].1);
                while j + 1 < rows.len() && rows[j + 1].0 - rows[j].0 < gap {
                    j += 1;
                    agg.add(rows[j].1);
                }
                out.insert((key, rows[i].0, rows[j].0 + gap), agg);
                i = j + 1;
            }
        }
    } else {
        for r in log.iter().flatten() {
            for (s, e) in kind.assign(r.ts) {
                if !dropped.contains(&(r.id, s)) {
                    out.entry((r.key, s, e)).or_insert(Agg::EMPTY).add(r.val);
                }
            }
        }
    }
    out
}
/// Replays an operator's consumption trace and calls `f(record, watermark in force)`; the watermark is the brute-force minimum over channels.
fn walk_trace(trace: &[(usize, Msg)], partitions: usize, mut f: impl FnMut(usize, Rec, i64)) {
    let mut wm = vec![i64::MIN; partitions];
    for &(ch, m) in trace {
        match m {
            Msg::Watermark(w) => wm[ch] = w,
            Msg::Record(r) => f(ch, r, *wm.iter().min().unwrap()),
            Msg::Barrier(_) => unreachable!("barriers are not traced"),
        }
    }
}
/// The history that took effect must contain every input record exactly once, in partition order, with every watermark exactly where the source put it.
fn check_effective_history<O: Operator>(rt: &Runtime<O>) {
    for o in 0..rt.cfg.ops {
        let mut got: Vec<Vec<Rec>> = vec![vec![]; rt.log.len()];
        for &(ch, m) in &rt.traces[o] {
            match m {
                Msg::Record(r) => got[ch].push(r),
                Msg::Watermark(w) => {
                    // The source sends watermark w right after the FIRST record with ts == w + B + 1, which must have been a new maximum; the final
                    // watermark follows the last record. On a FIFO channel, exactly the records routed here up to that point must have arrived.
                    let sent_before = if w == i64::MAX {
                        &rt.log[ch][..]
                    } else {
                        let ts = w + rt.cfg.bound + 1;
                        let k = rt.log[ch]
                            .iter()
                            .position(|r| r.ts == ts)
                            .expect("watermark value comes from a real record");
                        assert!(
                            rt.log[ch][..k].iter().all(|r| r.ts < ts),
                            "only a new maximum advances the watermark"
                        );
                        &rt.log[ch][..=k]
                    };
                    let want = sent_before.iter().filter(|r| rt.route(r) == o).count();
                    assert_eq!(
                        got[ch].len(),
                        want,
                        "operator {o}: watermark {w} of partition {ch} is out of place"
                    );
                }
                Msg::Barrier(_) => unreachable!("barriers are not traced"),
            }
        }
        for (p, got) in got.iter().enumerate() {
            let want: Vec<Rec> = rt.log[p]
                .iter()
                .copied()
                .filter(|r| rt.route(r) == o)
                .collect();
            assert!(
                *got == want,
                "operator {o}, partition {p}: effective input differs from the log ({} vs {} records)",
                got.len(),
                want.len()
            );
        }
    }
}
/// Expected drops for tumbling/sliding windows: (record id, window start) pairs, and the ids dropped from every window.
fn expected_drops<O: Operator>(
    rt: &Runtime<O>,
    kind: Kind,
    lateness: i64,
) -> (BTreeSet<(u32, i64)>, Vec<u32>) {
    let (mut pairs, mut side) = (BTreeSet::new(), vec![]);
    for t in &rt.traces {
        walk_trace(t, rt.log.len(), |_, r, w| {
            let wins = kind.assign(r.ts);
            let mut lost = 0;
            for &(s, e) in &wins {
                // w >= (e - 1) + lateness, written without the engine's helper; i128 because w may still be i64::MIN
                if w == i64::MAX || w as i128 - lateness as i128 >= (e - 1) as i128 {
                    pairs.insert((r.id, s));
                    lost += 1;
                }
            }
            if lost == wins.len() {
                side.push(r.id);
            }
        });
    }
    side.sort();
    (pairs, side)
}
fn totals_per_key(table: &BTreeMap<WinKey, Agg>) -> BTreeMap<u32, (u64, i64)> {
    let mut m: BTreeMap<u32, (u64, i64)> = BTreeMap::new();
    for (k, a) in table {
        let e = m.entry(k.0).or_default();
        e.0 += a.count;
        e.1 += a.sum;
    }
    m
}
fn sorted<T: Ord + Clone>(v: &[T]) -> Vec<T> {
    let mut v = v.to_vec();
    v.sort();
    v
}

// ---------------------------------------------------------------- random scenarios
struct Scenario {
    log: Vec<Vec<Rec>>,
    cfg: Config,
    kind: Kind,
    lateness: i64,
    trigger: Trigger,
    accumulation: Accumulation,
}
fn scenario(rng: &mut Rng, in_bounds: bool) -> Scenario {
    let kind = match rng.below(3) {
        0 => Kind::Tumbling {
            size: rng.range(1, 25),
        },
        1 => {
            let size = rng.range(2, 30);
            Kind::Sliding {
                size,
                slide: rng.range(1, size),
            }
        }
        _ => Kind::Session {
            gap: rng.range(1, 15),
        },
    };
    let (bound, lateness) = (
        rng.range(0, 12),
        if rng.below(2) == 0 {
            0
        } else {
            rng.range(1, 12)
        },
    );
    let trigger = [
        Trigger::OnWatermark,
        Trigger::EarlyEvery(1 + rng.below(4)),
        Trigger::PerRecord,
    ][rng.below(3) as usize];
    let accumulation = if !matches!(kind, Kind::Session { .. }) && rng.below(2) == 0 {
        Accumulation::Discarding
    } else {
        Accumulation::Accumulating
    };
    let partitions = 1 + rng.below(4) as usize;
    let log = if in_bounds {
        gen_log(
            rng,
            partitions,
            60,
            1 + partitions as u64 * 2,
            bound + lateness,
            None,
        )
    } else {
        gen_log(
            rng,
            partitions,
            60,
            1 + partitions as u64 * 2,
            bound + lateness,
            Some(60),
        )
    };
    let cfg = Config {
        ops: 1 + rng.below(3) as usize,
        bound,
        capacity: 2 + rng.below(7) as usize,
        mode: Mode::ExactlyOnce,
        checkpoint_every: 3 + rng.below(60),
        crash_every: 30 + rng.below(250),
        max_crashes: 8,
        op_speed: 1 + rng.below(6),
    };
    Scenario {
        log,
        cfg,
        kind,
        lateness,
        trigger,
        accumulation,
    }
}
fn run_windows(sc: &Scenario, seed: u64) -> Runtime<'_, WindowOp> {
    let mut rt = Runtime::new(
        sc.cfg,
        &sc.log,
        WindowOp::new(sc.kind, sc.lateness, sc.trigger, sc.accumulation),
    );
    rt.run(&mut Rng::new(seed));
    rt
}
/// Per window: at most one on-time pane per lifetime, and under the plain watermark trigger no early panes at all.
fn check_panes(sink: &Sink, trigger: Trigger, session: bool) {
    let mut on_time = BTreeSet::new();
    for p in &sink.panes {
        if let Out::Pane {
            win,
            timing,
            fired_at,
            ..
        } = p
        {
            match timing {
                Timing::Early => assert!(
                    trigger != Trigger::OnWatermark && *fired_at < win.2 - 1,
                    "early pane after completion"
                ),
                Timing::OnTime => assert!(
                    on_time.insert(*win) && *fired_at >= win.2 - 1,
                    "second or premature on-time pane for {win:?}"
                ),
                Timing::Late => assert!(
                    on_time.contains(win) || session,
                    "late pane without an on-time pane"
                ),
            }
        }
    }
}
#[derive(Default)]
struct Totals {
    records: u64,
    windows: u64,
    panes: [u64; 3],
    late_side: u64,
    dropped_pairs: u64,
    stats: Stats,
    by_kind: [u64; 3],
}
impl Totals {
    fn absorb<O: Operator>(&mut self, rt: &Runtime<O>) {
        self.records += rt.log.iter().map(|p| p.len() as u64).sum::<u64>();
        self.windows += rt.sink.table.len() as u64;
        for p in &rt.sink.panes {
            if let Out::Pane { timing, .. } = p {
                self.panes[*timing as usize] += 1;
            }
        }
        self.late_side += rt.sink.late.len() as u64;
        let (a, b) = (&mut self.stats, &rt.stats);
        a.delivered += b.delivered;
        a.checkpoints += b.checkpoints;
        a.crashes += b.crashes;
        a.crashes_mid_checkpoint += b.crashes_mid_checkpoint;
        a.crashes_before_commit += b.crashes_before_commit;
        a.duplicate_commits_ignored += b.duplicate_commits_ignored;
        a.source_stalls += b.source_stalls;
        a.max_depth = a.max_depth.max(b.max_depth);
        a.max_state = a.max_state.max(b.max_state);
    }
    fn print(&self) {
        println!(
            "  input records {}, final windows {}, committed panes early/on-time/late {}/{}/{}, late side output {}, dropped (record, window) pairs {}",
            self.records,
            self.windows,
            self.panes[0],
            self.panes[1],
            self.panes[2],
            self.late_side,
            self.dropped_pairs
        );
        let s = &self.stats;
        println!(
            "  runs by window kind tumbling/sliding/session {:?}; checkpoints {}, crashes {} (mid-checkpoint {}, durable-but-uncommitted {}), repeat commits ignored {}",
            self.by_kind,
            s.checkpoints,
            s.crashes,
            s.crashes_mid_checkpoint,
            s.crashes_before_commit,
            s.duplicate_commits_ignored
        );
        println!(
            "  records processed {} for {} inputs ({:.2}x: the replays), deepest channel {}, source stalls {}",
            s.delivered,
            self.records,
            s.delivered as f64 / self.records as f64,
            s.max_depth,
            s.source_stalls
        );
    }
}
fn kind_index(k: Kind) -> usize {
    match k {
        Kind::Tumbling { .. } => 0,
        Kind::Sliding { .. } => 1,
        Kind::Session { .. } => 2,
    }
}
fn test_in_bounds() {
    let mut t = Totals::default();
    for seed in 0..1000u64 {
        let sc = scenario(&mut Rng::new(seed), true);
        assert!(max_disorder(&sc.log) <= sc.cfg.bound + sc.lateness);
        let rt = run_windows(&sc, seed ^ 0xABCD);
        let want = batch_windows(&sc.log, sc.kind, &BTreeSet::new());
        assert!(
            rt.sink.table == want,
            "seed {seed} {:?} {:?} {:?}: streaming != batch\n got {:?}\nwant {:?}",
            sc.kind,
            sc.trigger,
            sc.accumulation,
            rt.sink.table,
            want
        );
        assert!(
            rt.sink.late.is_empty() && rt.ops.iter().all(|o| o.op.dropped_pairs == 0),
            "seed {seed}: nothing may be dropped within B + L"
        );
        check_effective_history(&rt);
        check_panes(
            &rt.sink,
            sc.trigger,
            matches!(sc.kind, Kind::Session { .. }),
        );
        t.by_kind[kind_index(sc.kind)] += 1;
        t.absorb(&rt);
    }
    println!(
        "1000 random streams with disorder <= B + L, with crashes: committed windows == batch GROUP BY, nothing dropped"
    );
    t.print();
    let s = &t.stats;
    assert!(
        s.crashes > 1000
            && s.crashes_mid_checkpoint > 100
            && s.crashes_before_commit > 50
            && s.duplicate_commits_ignored > 500,
        "the failure cases must really occur"
    );
    assert!(t.panes.iter().all(|&n| n > 100) && t.by_kind.iter().all(|&n| n > 200));
}
fn test_out_of_bounds() {
    let mut t = Totals::default();
    let mut runs_with_drops = 0;
    for seed in 0..1000u64 {
        let sc = scenario(&mut Rng::new(seed + 50_000), false);
        let rt = run_windows(&sc, seed ^ 0x1234);
        check_effective_history(&rt);
        let session = matches!(sc.kind, Kind::Session { .. });
        check_panes(&rt.sink, sc.trigger, session);
        let engine_dropped: u64 = rt.ops.iter().map(|o| o.op.dropped_pairs).sum();
        let late = sorted(&rt.sink.late);
        if session {
            // conservation: each record is in exactly one final session or in the side output
            assert_eq!(engine_dropped, late.len() as u64);
            let mut want = totals_per_key(&batch_windows(&sc.log, sc.kind, &BTreeSet::new()));
            for r in sc
                .log
                .iter()
                .flatten()
                .filter(|r| late.binary_search(&r.id).is_ok())
            {
                let e = want.get_mut(&r.key).unwrap();
                e.0 -= 1;
                e.1 -= r.val;
            }
            want.retain(|_, v| v.0 > 0);
            assert!(
                totals_per_key(&rt.sink.table) == want,
                "seed {seed}: session conservation"
            );
        } else {
            let (pairs, side) = expected_drops(&rt, sc.kind, sc.lateness);
            assert_eq!(
                engine_dropped,
                pairs.len() as u64,
                "seed {seed}: late-dropped pair count"
            );
            assert_eq!(late, side, "seed {seed}: late side output");
            assert!(
                rt.sink.table == batch_windows(&sc.log, sc.kind, &pairs),
                "seed {seed} {:?}: streaming != batch minus the dropped pairs",
                sc.kind
            );
        }
        runs_with_drops += (engine_dropped > 0) as u32;
        t.dropped_pairs += engine_dropped;
        t.by_kind[kind_index(sc.kind)] += 1;
        t.absorb(&rt);
    }
    println!(
        "\n1000 random streams with stragglers beyond B + L ({runs_with_drops} of them lost data): drops and side output match the trace oracle"
    );
    println!(
        "  (tumbling/sliding: results == batch minus exactly those pairs; sessions: count and sum conservation only)"
    );
    t.print();
    assert!(
        runs_with_drops > 600 && t.dropped_pairs > 3000 && t.late_side > 1000 && t.panes[2] > 100
    );
}
/// The checks above are only worth something if broken protocols fail them.
fn test_mutations() {
    println!(
        "\nmutation check, 600 in-bounds tumbling runs each (discarding panes, so every duplicate effect is visible):"
    );
    println!(
        "  {:<14} {:>11} {:>17} {:>18}",
        "protocol", "runs wrong", "extra count", "missing count"
    );
    for mode in [Mode::ExactlyOnce, Mode::NoAlignment, Mode::ImmediateSink] {
        let (mut wrong, mut extra, mut missing) = (0, 0u64, 0u64);
        for seed in 0..600u64 {
            let mut sc = scenario(&mut Rng::new(seed + 90_000), true);
            sc.kind = Kind::Tumbling { size: 10 };
            sc.accumulation = Accumulation::Discarding;
            sc.cfg.mode = mode;
            let rt = run_windows(&sc, seed);
            let want = batch_windows(&sc.log, sc.kind, &BTreeSet::new());
            wrong += (rt.sink.table != want) as u32;
            let (got, want) = (totals_per_key(&rt.sink.table), totals_per_key(&want));
            for (k, w) in &want {
                let g = got.get(k).map_or(0, |g| g.0);
                extra += g.saturating_sub(w.0);
                missing += w.0.saturating_sub(g);
            }
        }
        println!(
            "  {:<14} {wrong:>11} {extra:>17} {missing:>18}",
            format!("{mode:?}")
        );
        match mode {
            Mode::ExactlyOnce => assert!(wrong == 0 && extra == 0 && missing == 0),
            // At-least-once. Records that overtake a barrier are counted in the snapshot AND replayed. A replayed copy may even look late (its channel
            // watermark was snapshotted ahead of the rewound source), but then the original is already counted. Records not in the snapshot are still
            // within B + L of every watermark in it, so nothing can go missing.
            Mode::NoAlignment => assert!(
                wrong > 30 && extra > 100 && missing == 0,
                "skipping alignment must be detected, as over-counting"
            ),
            Mode::ImmediateSink => assert!(
                wrong > 300 && extra > 1000 && missing == 0,
                "a non-transactional sink must be detected"
            ),
        }
    }
}

// ---------------------------------------------------------------- stream-stream join
fn test_interval_join() {
    let (mut pairs_total, mut late_total, mut crashes, mut max_state) =
        (0usize, 0usize, 0u32, 0usize);
    for seed in 0..1000u64 {
        let mut rng = Rng::new(seed + 130_000);
        let in_bounds = seed % 2 == 0;
        let mut sc = scenario(&mut rng, in_bounds);
        sc.lateness = 0; // the join has no allowed lateness, so regenerate the log for a bound of B alone
        let partitions = sc.log.len();
        sc.log = gen_log(
            &mut rng,
            partitions,
            60,
            3,
            sc.cfg.bound,
            if in_bounds { None } else { Some(40) },
        );
        let lo = rng.range(-15, 10);
        let hi = lo + rng.range(0, 20);
        let mut rt = Runtime::new(sc.cfg, &sc.log, JoinOp::new(lo, hi));
        rt.run(&mut Rng::new(seed));
        check_effective_history(&rt);
        let mut dropped = vec![];
        for t in &rt.traces {
            walk_trace(t, partitions, |_, r, w| {
                if w >= r.ts {
                    dropped.push(r.id);
                }
            });
        }
        dropped.sort();
        assert_eq!(
            sorted(&rt.sink.late),
            dropped,
            "seed {seed}: join late side output"
        );
        assert!(!in_bounds || dropped.is_empty());
        let all: Vec<Rec> = sc
            .log
            .iter()
            .flatten()
            .copied()
            .filter(|r| dropped.binary_search(&r.id).is_err())
            .collect();
        let mut want = vec![];
        for a in all.iter().filter(|r| r.stream == 0) {
            for b in all.iter().filter(|r| r.stream == 1) {
                if a.key == b.key && (lo..=hi).contains(&(b.ts - a.ts)) {
                    want.push((a.id, b.id));
                }
            }
        }
        want.sort();
        assert_eq!(
            sorted(&rt.sink.joined),
            want,
            "seed {seed}: join [{lo},{hi}] != nested loop"
        );
        pairs_total += want.len();
        late_total += dropped.len();
        crashes += rt.stats.crashes;
        max_state = max_state.max(rt.stats.max_state);
    }
    println!(
        "\ninterval join, 1000 random two-stream runs (half in bounds) == nested loop over accepted records"
    );
    println!(
        "  pairs {pairs_total}, late records {late_total}, crashes {crashes}, largest per-operator join buffer {max_state}"
    );
    assert!(pairs_total > 5000 && late_total > 500 && crashes > 500);
}

// ---------------------------------------------------------------- demonstrations
fn demo_event_vs_processing_time() {
    let mut rng = Rng::new(7);
    let log = gen_log(&mut rng, 3, 200, 1, 8, None);
    let cfg = Config {
        ops: 1,
        bound: 8,
        capacity: 8,
        mode: Mode::ExactlyOnce,
        checkpoint_every: 20,
        crash_every: 0,
        max_crashes: 0,
        op_speed: 4,
    };
    let kind = Kind::Tumbling { size: 100 };
    let mut by_event = vec![];
    let mut by_arrival = vec![];
    for schedule in [1u64, 2] {
        let mut rt = Runtime::new(
            cfg,
            &log,
            WindowOp::new(kind, 0, Trigger::OnWatermark, Accumulation::Accumulating),
        );
        rt.run(&mut Rng::new(schedule));
        by_event.push(rt.sink.table.clone());
        // "processing-time windows": every 100 consumed records form one bucket
        let arrivals: Vec<i64> = rt.traces[0]
            .iter()
            .filter_map(|&(_, m)| {
                if let Msg::Record(r) = m {
                    Some(r.val)
                } else {
                    None
                }
            })
            .collect();
        by_arrival.push(
            arrivals
                .chunks(100)
                .map(|c| c.iter().sum::<i64>())
                .collect::<Vec<_>>(),
        );
    }
    println!("\nsame input, two schedules:");
    println!(
        "  processing-time buckets of 100 arrivals, sums: {:?}\n                                          vs  {:?}",
        by_arrival[0], by_arrival[1]
    );
    println!(
        "  event-time tumbling windows: {} windows, identical in both: {}",
        by_event[0].len(),
        by_event[0] == by_event[1]
    );
    assert!(by_event[0] == by_event[1] && by_arrival[0] != by_arrival[1]);
}
fn demo_latency_completeness() {
    println!(
        "\nthe watermark trade-off: one input (disorder up to 40), one schedule, allowed lateness 0, tumbling 20"
    );
    println!(
        "  delay = how far in EVENT time the slowest partition had got past the window end when the operator fired the on-time pane"
    );
    println!(
        "  {:>4} {:>16} {:>14} {:>22}",
        "B", "records dropped", "on-time panes", "mean delay (event time)"
    );
    let mut rng = Rng::new(11);
    let log = gen_log(&mut rng, 3, 600, 4, 40, None);
    let (mut last_drop, mut last_delay) = (u64::MAX, f64::MIN);
    for bound in [0i64, 5, 15, 25, 40] {
        let cfg = Config {
            ops: 2,
            bound,
            capacity: 4,
            mode: Mode::ExactlyOnce,
            checkpoint_every: 25,
            crash_every: 0,
            max_crashes: 0,
            op_speed: 6,
        };
        let mut rt = Runtime::new(
            cfg,
            &log,
            WindowOp::new(
                Kind::Tumbling { size: 20 },
                0,
                Trigger::OnWatermark,
                Accumulation::Accumulating,
            ),
        );
        // the seed fixes the schedule whatever B is (B changes watermark values, not which messages exist)
        rt.run(&mut Rng::new(99));
        // fired_at = (newest ts the slowest channel had announced) - B - 1; panes flushed by the end-of-input watermark are left out
        let (mut delay, mut n) = (0i64, 0u64);
        for p in &rt.sink.panes {
            if let Out::Pane {
                win,
                timing: Timing::OnTime,
                fired_at,
                ..
            } = p
            {
                if *fired_at != i64::MAX {
                    delay += fired_at + bound + 1 - win.2; // fired_at = (max ts of the slowest partition) - B - 1
                    n += 1;
                }
            }
        }
        let dropped: u64 = rt.ops.iter().map(|o| o.op.dropped_pairs).sum();
        let mean = delay as f64 / n as f64;
        println!("  {bound:>4} {dropped:>16} {n:>14} {mean:>22.1}");
        assert!(
            dropped <= last_drop && mean >= last_delay,
            "a larger B can only drop less and fire later"
        );
        (last_drop, last_delay) = (dropped, mean);
    }
    assert_eq!(last_drop, 0, "B >= disorder drops nothing");
}
fn demo_backpressure() {
    println!(
        "\nbackpressure: slow operators (scheduling weight 1 against 4 for sources), checkpoints on"
    );
    println!(
        "  {:>8} {:>14} {:>14} {:>12}",
        "capacity", "deepest queue", "source stalls", "stalls/rec"
    );
    let mut rng = Rng::new(23);
    let log = gen_log(&mut rng, 4, 800, 8, 5, None);
    let records: u64 = log.iter().map(|p| p.len() as u64).sum();
    let mut prev = u64::MAX;
    for capacity in [2usize, 8, 32, 128, 100_000] {
        let cfg = Config {
            ops: 2,
            bound: 5,
            capacity,
            mode: Mode::ExactlyOnce,
            checkpoint_every: 50,
            crash_every: 0,
            max_crashes: 0,
            op_speed: 1,
        };
        let mut rt = Runtime::new(
            cfg,
            &log,
            WindowOp::new(
                Kind::Tumbling { size: 50 },
                0,
                Trigger::OnWatermark,
                Accumulation::Accumulating,
            ),
        );
        rt.run(&mut Rng::new(5));
        assert!(
            rt.stats.max_depth <= capacity,
            "the bound is never exceeded"
        );
        assert!(
            rt.sink.table == batch_windows(&log, Kind::Tumbling { size: 50 }, &BTreeSet::new()),
            "buffer size changes timing, never results"
        );
        println!(
            "  {capacity:>8} {:>14} {:>14} {:>12.2}",
            rt.stats.max_depth,
            rt.stats.source_stalls,
            rt.stats.source_stalls as f64 / records as f64
        );
        assert!(rt.stats.source_stalls <= prev);
        prev = rt.stats.source_stalls;
    }
    assert_eq!(
        prev, 0,
        "an effectively unbounded buffer never stalls (it just grows)"
    );
}
fn test_assigners_and_session_retraction() {
    let sliding = Kind::Sliding { size: 10, slide: 4 };
    assert_eq!(sliding.assign(9), vec![(8, 18), (4, 14), (0, 10)]);
    assert_eq!(
        Kind::Tumbling { size: 10 }.assign(-1),
        vec![(-10, 0)],
        "euclidean alignment below zero"
    );
    for ts in -50..50 {
        let brute: Vec<(i64, i64)> = (-25..25)
            .rev()
            .map(|i| (i * 4, i * 4 + 10))
            .filter(|w| w.0 <= ts && ts < w.1)
            .collect();
        assert_eq!(sliding.assign(ts), brute);
    }
    // two sessions that emitted early panes are bridged by a third record: both are retracted and one replaces them
    let mut op = WindowOp::new(
        Kind::Session { gap: 5 },
        0,
        Trigger::PerRecord,
        Accumulation::Accumulating,
    );
    let mut out = vec![];
    for (id, ts) in [(1, 0), (2, 8), (3, 4)] {
        op.on_record(
            Rec {
                id,
                stream: 0,
                key: 1,
                ts,
                val: 1,
            },
            i64::MIN,
            &mut out,
        );
    }
    let retracted: Vec<WinKey> = out
        .iter()
        .filter_map(|o| {
            if let Out::Retract { win } = o {
                Some(*win)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(retracted, vec![(1, 0, 5), (1, 8, 13)]);
    let mut sink = Sink::default();
    assert!(sink.commit(1, &out) && !sink.commit(1, &out));
    assert_eq!(
        sink.table.keys().copied().collect::<Vec<_>>(),
        vec![(1, 0, 13)]
    );
    assert_eq!(sink.table[&(1, 0, 13)].count, 3);
    // touching sessions stay apart (ts difference == gap)
    assert_eq!(
        batch_windows(
            &[vec![
                Rec {
                    id: 1,
                    stream: 0,
                    key: 1,
                    ts: 0,
                    val: 1
                },
                Rec {
                    id: 2,
                    stream: 0,
                    key: 1,
                    ts: 5,
                    val: 1
                }
            ]],
            Kind::Session { gap: 5 },
            &BTreeSet::new()
        )
        .len(),
        2
    );
    println!("window assigners, session merge with retraction, idempotent commit: ok");
}

fn main() {
    test_assigners_and_session_retraction();
    demo_event_vs_processing_time();
    println!();
    test_in_bounds();
    test_out_of_bounds();
    test_mutations();
    test_interval_join();
    demo_latency_completeness();
    demo_backpressure();
    println!("\nall stream_processing_windows checks passed");
}
