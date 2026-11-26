//! RECIPE 237 (distributed): the saga pattern: distributed transactions without 2PC, via compensation, a durable log and idempotent steps
//! LEVEL: advanced
//! Use in: multi-service business transactions (booking, checkout, onboarding) where a global lock or two-phase commit is not available.   Run: cargo run --release --example saga_orchestrator
#![allow(dead_code)]
//!
//! PROBLEM: a trip booking touches three independent services (flight, hotel, card) that each have their own database. One
//! ACID transaction cannot span them. Two-phase commit can, but it BLOCKS: if the coordinator dies after the participants
//! voted yes, they hold their locks until it returns (demonstrated below). A SAGA instead runs a sequence of LOCAL
//! transactions and, if one fails, runs COMPENSATING actions for the completed ones in REVERSE order.
//! STEP KINDS decide a legal order: COMPENSATABLE steps (can be undone) first, then at most one PIVOT (the go/no-go step:
//! once it succeeds the saga must finish), then RETRIABLE steps (must eventually succeed, retried forever). Here:
//! reserve flight, reserve hotel (compensatable), charge card (pivot), issue ticket (retriable).
//! ORCHESTRATION: one coordinator whose entire state is a pure FOLD of the durable SAGA LOG; `next_action(state)` is a pure
//! function. Each transition is appended BEFORE it is executed (started / done / failed / compensating / compensated), so
//! after a crash a fresh orchestrator replays the log and simply continues; no in-memory state is needed for correctness.
//! IDEMPOTENCY: because a crash can land between "log appended" and "effect done" or between "effect done" and "result
//! logged", every step and compensation may run more than once. Services key all operations by saga id (a set insert / a
//! set remove), so repeats are harmless. A compensation also leaves a TOMBSTONE: otherwise a delayed duplicate of the
//! forward request arriving AFTER the cancel would re-create the effect (an orphan); a test below shows this really happens.
//! RETRIES: transient failures retry with exponential backoff and jitter on a virtual clock; a forward step gives up after N
//! attempts (then the saga compensates), compensations never give up. A saga-wide deadline aborts before the pivot.
//! CHOREOGRAPHY: the same saga without a coordinator: each service reacts to events and emits the next one (the bus
//! is at-least-once with duplicates). Less coupling, but the flow lives nowhere in particular and is harder to follow.
//! ISOLATION: sagas are ACD, not I: intermediate effects are visible. A dirty-read demo shows a reader acting on a seat count
//! that is later compensated; the SEMANTIC LOCK countermeasure marks the seat `Pending(saga)` so readers can tell.
//! HOW (verification): seeded failure injection (permanent failures, transient errors with lost replies, duplicate and
//! delayed deliveries, orchestrator crashes at both awkward points). 5000 random schedules must end fully COMMITTED or
//! fully COMPENSATED, never partial, with each compensation effective at most once.
//! COMMON USES: e-commerce checkout (inventory, payment, shipping), travel booking, order fulfilment, account provisioning.
//! Real-world engines: Temporal and Cadence workflows, AWS Step Functions, Camunda, Eventuate Tram.
//! Read: https://microservices.io/patterns/data/saga.html , https://www.cs.cornell.edu/andru/cs711/2002fa/reading/sagas.pdf
//! (the original 1987 paper by Garcia-Molina and Salem)
use std::collections::BTreeSet;

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
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
        self.next() % n
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

// ---------------------------------------------------------------- the plan: step kinds and legal ordering

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Compensatable,
    Pivot,
    Retriable,
}

const N: usize = 4;
const PLAN: [(&str, Kind); N] = [
    ("flight", Kind::Compensatable),
    ("hotel", Kind::Compensatable),
    ("card", Kind::Pivot),
    ("ticket", Kind::Retriable),
];
const PIVOT: usize = 2;

/// compensatable* pivot? retriable*: anything else can leave a saga that can neither go back nor forward
fn validate_order(kinds: &[Kind]) -> Result<(), String> {
    let rank = |k: &Kind| match k {
        Kind::Compensatable => 0,
        Kind::Pivot => 1,
        Kind::Retriable => 2,
    };
    if kinds.windows(2).any(|w| rank(&w[0]) > rank(&w[1])) {
        return Err(
            "order must be: compensatable steps, then the pivot, then retriable steps".into(),
        );
    }
    if kinds.iter().filter(|k| **k == Kind::Pivot).count() > 1 {
        return Err("at most one pivot".into());
    }
    Ok(())
}

// ---------------------------------------------------------------- the services' shared world (idempotent + tombstones)

#[derive(Default)]
struct World {
    present: [BTreeSet<u64>; N],   // saga ids whose effect exists, per step
    cancelled: [BTreeSet<u64>; N], // tombstones left by compensations
    do_effective: [u32; N],
    undo_effective: [u32; N],
}

impl World {
    /// returns whether the effect is in place afterwards. A tombstone refuses late forward requests.
    fn apply(&mut self, step: usize, saga: u64, tombstones: bool) -> bool {
        if tombstones && self.cancelled[step].contains(&saga) {
            return false;
        }
        if self.present[step].insert(saga) {
            self.do_effective[step] += 1;
        }
        true
    }

    fn undo(&mut self, step: usize, saga: u64) {
        self.cancelled[step].insert(saga); // record even if there was nothing to cancel yet
        if self.present[step].remove(&saga) {
            self.undo_effective[step] += 1;
        }
    }

    fn effects(&self, saga: u64) -> usize {
        self.present.iter().filter(|s| s.contains(&saga)).count()
    }
}

// ---------------------------------------------------------------- the saga log and the pure orchestrator

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Outcome {
    Committed,
    Compensated,
}

#[derive(Clone, PartialEq, Debug)]
enum LogRec {
    StepStarted(usize),
    StepDone(usize),
    StepFailed(usize),
    Abort(&'static str),
    CompStarted(usize),
    CompDone(usize),
    Finished(Outcome),
}

#[derive(Default, Debug)]
struct SagaState {
    started: [bool; N],
    done: [bool; N],
    comp_started: [bool; N],
    comp_done: [bool; N],
    aborting: bool,
    outcome: Option<Outcome>,
}

/// the orchestrator's whole state is this fold: recovery = fold the log again
fn fold_log(log: &[LogRec]) -> SagaState {
    let mut st = SagaState::default();
    for r in log {
        match *r {
            LogRec::StepStarted(i) => st.started[i] = true,
            LogRec::StepDone(i) => st.done[i] = true,
            LogRec::StepFailed(_) | LogRec::Abort(_) => st.aborting = true,
            LogRec::CompStarted(i) => st.comp_started[i] = true,
            LogRec::CompDone(i) => st.comp_done[i] = true,
            LogRec::Finished(o) => st.outcome = Some(o),
        }
    }
    st
}

#[derive(Debug, PartialEq)]
enum Action {
    Exec(usize),
    Comp(usize),
    Finish(Outcome),
    Finished(Outcome),
}

fn next_action(st: &SagaState) -> Action {
    if let Some(o) = st.outcome {
        return Action::Finished(o);
    }
    if !st.aborting {
        return match (0..N).find(|&i| !st.done[i]) {
            Some(i) => Action::Exec(i),
            None => Action::Finish(Outcome::Committed),
        };
    }
    // reverse order; every step that STARTED may have taken effect (a lost reply looks like a failure), so it is
    // compensated too; the compensation of a step that never took effect is a harmless no-op
    match (0..N)
        .rev()
        .find(|&i| st.started[i] && !st.comp_done[i] && PLAN[i].1 != Kind::Retriable)
    {
        Some(i) => Action::Comp(i),
        None => Action::Finish(Outcome::Compensated),
    }
}

// ---------------------------------------------------------------- the failure-injecting runner

#[derive(Clone)]
struct Cfg {
    p_transient: u64,
    p_dup: u64,
    p_crash: u64,
    tombstones: bool,
    max_attempts: u32,
    deadline: u64,
}

impl Cfg {
    fn clean() -> Cfg {
        Cfg {
            p_transient: 0,
            p_dup: 0,
            p_crash: 0,
            tombstones: true,
            max_attempts: 5,
            deadline: u64::MAX,
        }
    }
    fn chaos() -> Cfg {
        Cfg {
            p_transient: 30,
            p_dup: 25,
            p_crash: 12,
            tombstones: true,
            max_attempts: 6,
            deadline: 700,
        }
    }
}

enum Call {
    Ok,
    Transient,
    Permanent,
}

struct Run {
    world: World,
    log: Vec<LogRec>, // the DURABLE saga log (survives crashes)
    rng: Rng,
    cfg: Cfg,
    saga: u64,
    fail_at: Option<usize>, // a step that fails for business reasons (no room, card declined)
    strays: Vec<(usize, bool)>, // delayed duplicate requests still travelling the network
    clock: u64,
    attempts: [u32; N], // VOLATILE: lost on crash
    crashes: u32,
    retries: u32,
}

fn backoff(attempt: u32, rng: &mut Rng) -> u64 {
    (10u64 << attempt.min(4)) + rng.below(10) // exponential, capped, with jitter so retries do not synchronise
}

impl Run {
    fn new(saga: u64, seed: u64, fail_at: Option<usize>, cfg: Cfg) -> Run {
        Run {
            world: World::default(),
            log: vec![],
            rng: Rng::new(seed),
            cfg,
            saga,
            fail_at,
            strays: vec![],
            clock: 0,
            attempts: [0; N],
            crashes: 0,
            retries: 0,
        }
    }

    fn effect(&mut self, step: usize, undo: bool) {
        if undo {
            self.world.undo(step, self.saga);
        } else {
            self.world.apply(step, self.saga, self.cfg.tombstones);
        }
    }

    /// one remote call. A transient failure may or may not have taken effect (lost reply): the caller cannot tell.
    fn call(&mut self, step: usize, undo: bool) -> Call {
        self.clock += 5 + self.rng.below(20);
        if !undo && self.fail_at == Some(step) {
            return Call::Permanent;
        }
        if self.rng.chance(self.cfg.p_dup) {
            self.strays.push((step, undo)); // the network will deliver a copy later, maybe after we moved on
        }
        if self.rng.chance(self.cfg.p_transient) {
            if self.rng.chance(50) {
                self.effect(step, undo);
            }
            return Call::Transient;
        }
        self.effect(step, undo);
        Call::Ok
    }

    /// crash injection: volatile state vanishes, the log stays; the loop then "recovers" by re-folding the log
    fn crashed(&mut self) -> bool {
        if self.rng.chance(self.cfg.p_crash) {
            self.crashes += 1;
            self.attempts = [0; N];
            return true;
        }
        false
    }

    fn deliver_stray(&mut self) {
        if !self.strays.is_empty() && self.rng.chance(30) {
            let k = self.rng.below(self.strays.len() as u64) as usize;
            let (step, undo) = self.strays.swap_remove(k);
            self.effect(step, undo);
        }
    }

    fn run(&mut self) -> Outcome {
        for _ in 0..100_000 {
            self.deliver_stray();
            let st = fold_log(&self.log);
            match next_action(&st) {
                Action::Finished(o) => {
                    for (step, undo) in std::mem::take(&mut self.strays) {
                        self.effect(step, undo); // finally the network delivers every straggler
                    }
                    return o;
                }
                Action::Finish(o) => self.log.push(LogRec::Finished(o)),
                Action::Exec(i) => self.exec(i, &st),
                Action::Comp(i) => self.comp(i, &st),
            }
        }
        panic!("saga did not terminate");
    }

    fn exec(&mut self, i: usize, st: &SagaState) {
        if !st.done[PIVOT] && self.clock > self.cfg.deadline {
            self.log.push(LogRec::Abort("deadline")); // before the pivot we may still give up; after it we must not
            return;
        }
        if !st.started[i] {
            self.log.push(LogRec::StepStarted(i)); // WRITE-AHEAD: intent is durable before the effect
            if self.crashed() {
                return; // crash point 1: logged, effect not yet attempted
            }
        }
        match self.call(i, false) {
            Call::Ok => {
                if self.crashed() {
                    return; // crash point 2: effect happened, result not logged; the re-run is a no-op thanks to idempotency
                }
                self.log.push(LogRec::StepDone(i));
            }
            Call::Permanent => self.log.push(LogRec::StepFailed(i)),
            Call::Transient => {
                self.attempts[i] += 1;
                self.retries += 1;
                if PLAN[i].1 != Kind::Retriable && self.attempts[i] >= self.cfg.max_attempts {
                    self.log.push(LogRec::StepFailed(i));
                } else {
                    self.clock += backoff(self.attempts[i], &mut self.rng); // retriable steps retry forever
                }
            }
        }
    }

    fn comp(&mut self, i: usize, st: &SagaState) {
        if !st.comp_started[i] {
            self.log.push(LogRec::CompStarted(i));
            if self.crashed() {
                return;
            }
        }
        if let Call::Ok = self.call(i, true) {
            if !self.crashed() {
                self.log.push(LogRec::CompDone(i));
            }
        } else {
            self.attempts[i] += 1;
            self.retries += 1;
            self.clock += backoff(self.attempts[i], &mut self.rng); // compensations never give up
        }
    }

    /// the voluntary failure (business rule) is expected; anything else (retry exhaustion, deadline) is "involuntary"
    fn involuntary_abort(&self) -> bool {
        self.log.iter().any(|r| match r {
            LogRec::StepFailed(j) => self.fail_at != Some(*j),
            LogRec::Abort(_) => true,
            _ => false,
        })
    }
}

fn fail_plan(rng: &mut Rng) -> Option<usize> {
    match rng.below(6) {
        k @ 0..3 => Some(k as usize),
        _ => None,
    }
}

/// invariants of one finished schedule; returns false only for the tombstone-less experiment
fn check(run: &Run, outcome: Outcome, reference: Outcome) -> bool {
    let present = run.world.effects(run.saga);
    let ok = match outcome {
        Outcome::Committed => present == N,
        Outcome::Compensated => present == 0,
    };
    if !ok {
        return false;
    }
    assert_eq!(
        fold_log(&run.log).outcome,
        Some(outcome),
        "the log alone reproduces the outcome"
    );
    for i in 0..N {
        assert!(
            run.world.undo_effective[i] <= 1,
            "compensation took effect more than once"
        );
        assert!(run.world.undo_effective[i] <= run.world.do_effective[i]);
    }
    if run.log.contains(&LogRec::StepDone(PIVOT)) {
        assert_eq!(
            outcome,
            Outcome::Committed,
            "after the pivot there is no way back"
        );
        assert!(!run.log.iter().any(|r| matches!(r, LogRec::CompStarted(_))));
    }
    if run.fail_at.is_some() {
        assert_eq!(outcome, Outcome::Compensated);
    }
    if !run.involuntary_abort() {
        assert_eq!(
            outcome, reference,
            "faults and crashes must not change the outcome"
        );
    } else {
        assert_eq!(
            outcome,
            Outcome::Compensated,
            "involuntary aborts always compensate"
        );
    }
    true
}

// ---------------------------------------------------------------- choreography: the same saga without a coordinator

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ev {
    Started,
    FlightReserved,
    FlightFailed,
    HotelReserved,
    HotelFailed,
    HotelCancelled,
    CardCharged,
    CardFailed,
    Ticketed,
}

struct Choreo {
    world: World,
    saga: u64,
    fail_at: Option<usize>,
    rng: Rng,
}

impl Choreo {
    fn step(&mut self, i: usize, ok: Ev, failed: Ev) -> Vec<Ev> {
        if self.fail_at == Some(i) || !self.world.apply(i, self.saga, true) {
            return vec![failed];
        }
        vec![ok]
    }

    /// each service's reaction to an event; Err = transient failure (message is not acked and will be redelivered)
    fn handle(&mut self, ev: Ev) -> Result<Vec<Ev>, ()> {
        if self.rng.chance(25) {
            if self.rng.chance(50)
                && !matches!(ev, Ev::HotelFailed | Ev::CardFailed | Ev::HotelCancelled)
            {
                let _ = self.step_effect_only(ev);
            }
            return Err(());
        }
        Ok(match ev {
            Ev::Started => self.step(0, Ev::FlightReserved, Ev::FlightFailed),
            Ev::FlightReserved => self.step(1, Ev::HotelReserved, Ev::HotelFailed),
            Ev::HotelReserved => self.step(2, Ev::CardCharged, Ev::CardFailed),
            Ev::CardCharged => {
                self.world.apply(3, self.saga, true);
                vec![Ev::Ticketed]
            }
            Ev::HotelFailed => {
                self.world.undo(0, self.saga); // flight service compensates
                vec![]
            }
            Ev::CardFailed => {
                self.world.undo(2, self.saga); // card may have been charged under a lost reply
                self.world.undo(1, self.saga); // hotel service compensates, then tells the flight service
                vec![Ev::HotelCancelled]
            }
            Ev::HotelCancelled => {
                self.world.undo(0, self.saga);
                vec![]
            }
            Ev::FlightFailed | Ev::Ticketed => vec![],
        })
    }

    fn step_effect_only(&mut self, ev: Ev) {
        let i = match ev {
            Ev::Started => 0,
            Ev::FlightReserved => 1,
            Ev::HotelReserved => 2,
            Ev::CardCharged => 3,
            _ => return,
        };
        if self.fail_at != Some(i) {
            self.world.apply(i, self.saga, true); // lost reply: effect without the follow-up event
        }
    }
}

/// at-least-once bus with duplicates, redelivery after lost acks and random order; the bus dedupes identical events
fn run_choreography(seed: u64, fail_at: Option<usize>) -> (Outcome, usize) {
    let mut c = Choreo {
        world: World::default(),
        saga: 1,
        fail_at,
        rng: Rng::new(seed),
    };
    let mut queue: Vec<(Ev, bool)> = vec![(Ev::Started, false)]; // (event, acked)
    let mut deliveries = 0;
    while queue.iter().any(|(_, acked)| !acked) {
        let k = c.rng.below(queue.len() as u64) as usize;
        let ev = queue[k].0;
        deliveries += 1;
        if let Ok(out) = c.handle(ev) {
            for o in out {
                if !queue.iter().any(|(e, _)| *e == o) {
                    queue.push((o, false));
                }
            }
            if c.rng.chance(80) {
                queue[k].1 = true; // ack (otherwise the ack was lost and it will be delivered again)
            }
        }
    }
    // duplicate storm: replay every event once more, in random order, ignoring outputs
    let events: Vec<Ev> = queue.iter().map(|(e, _)| *e).collect();
    for _ in 0..events.len() * 2 {
        let ev = events[c.rng.below(events.len() as u64) as usize];
        let _ = c.handle(ev);
    }
    let present = c.world.effects(1);
    let outcome = if present == N {
        Outcome::Committed
    } else {
        Outcome::Compensated
    };
    assert!(
        present == N || present == 0,
        "choreography ended partial: {present} effects (fail_at {fail_at:?})"
    );
    (outcome, deliveries)
}

// ---------------------------------------------------------------- 2PC for contrast: blocking

#[derive(Default)]
struct Participant {
    prepared_by: Option<u64>, // holds its lock between PREPARE(yes) and the decision
}

fn demo_two_phase_commit() {
    let mut parts = [
        Participant::default(),
        Participant::default(),
        Participant::default(),
    ];
    // phase 1: everyone votes yes and locks
    for p in &mut parts {
        p.prepared_by = Some(1);
    }
    let coordinator_decision_logged = true; // the coordinator logs COMMIT, then crashes before telling anyone
    // meanwhile another transaction needs the same rows: it can only wait
    let mut blocked_ticks = 0;
    for _tick in 0..50 {
        if parts.iter().any(|p| p.prepared_by.is_some()) {
            blocked_ticks += 1; // cannot unilaterally abort: the vote was yes, the outcome is not ours to guess
        }
    }
    // coordinator comes back, reads its log, resends the decision; only now are locks released
    assert!(coordinator_decision_logged);
    for p in &mut parts {
        p.prepared_by = None;
    }
    println!(
        "2PC: coordinator crash left participants blocked for {blocked_ticks} ticks (locks held, no progress possible)"
    );
    assert_eq!(blocked_ticks, 50);
    println!(
        "saga: no locks are held between steps, so nothing blocks; the price is that intermediate states are visible"
    );
}

// ---------------------------------------------------------------- isolation anomaly: dirty read and semantic lock

#[derive(Clone, Copy, PartialEq, Debug)]
enum Seat {
    Free,
    Pending(u32), // SEMANTIC LOCK: reserved by saga n, outcome not yet known
    Booked(u32),
}

#[derive(Debug, PartialEq)]
enum Read {
    SoldOut,
    Available,
    TryLater, // the lock tells the reader the value is provisional
}

fn read_seat(s: Seat) -> Read {
    match s {
        Seat::Free => Read::Available,
        Seat::Pending(_) => Read::TryLater,
        Seat::Booked(_) => Read::SoldOut,
    }
}

fn demo_isolation() {
    // naive: just a counter of free seats
    let mut free = 1i32;
    free -= 1; // saga A reserves the last seat (local commit, visible immediately)
    let b_sees_sold_out = free == 0; // saga B reads between A's steps
    free += 1; // A's card is declined: compensation returns the seat
    assert!(b_sees_sold_out && free == 1);
    println!(
        "dirty read: B told its customer 'sold out' based on A's uncommitted reservation; A then compensated, seat is free"
    );

    // semantic lock: the seat records WHO holds it and that it is provisional
    let mut seat = Seat::Free;
    assert_eq!(read_seat(seat), Read::Available);
    seat = Seat::Pending(1); // A reserves
    assert_eq!(read_seat(seat), Read::TryLater); // B does not conclude "sold out"; it waits / retries
    seat = Seat::Free; // A compensates (only Pending(1) may be reset by saga 1)
    assert_eq!(read_seat(seat), Read::Available);
    seat = Seat::Pending(2); // B proceeds
    assert_eq!(read_seat(seat), Read::TryLater);
    seat = Seat::Booked(2);
    assert_eq!(read_seat(seat), Read::SoldOut); // now "sold out" is a committed fact
    println!(
        "semantic lock: readers saw TryLater while Pending, Available after the compensation, SoldOut only once Booked"
    );
    // other countermeasures: commutative updates, re-read value (optimistic check before the pivot), versioned writes,
    // putting the saga's decision data in the saga itself, ordering steps so the riskiest step comes first
}

// ---------------------------------------------------------------- main

fn main() {
    // 0. plan validation
    assert!(validate_order(&PLAN.map(|p| p.1)).is_ok());
    assert!(validate_order(&[Kind::Retriable, Kind::Compensatable]).is_err());
    assert!(validate_order(&[Kind::Pivot, Kind::Pivot]).is_err());

    // 1. the happy path and one business failure, no chaos: look at the log
    let mut ok = Run::new(1, 1, None, Cfg::clean());
    assert_eq!(ok.run(), Outcome::Committed);
    println!("happy path log: {:?}", ok.log);
    let mut bad = Run::new(2, 1, Some(2), Cfg::clean()); // the card is declined
    assert_eq!(bad.run(), Outcome::Compensated);
    println!("card declined log: {:?}", bad.log);
    // compensations ran in reverse order: hotel before flight
    let comps: Vec<usize> = bad
        .log
        .iter()
        .filter_map(|r| {
            if let LogRec::CompDone(i) = r {
                Some(*i)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(comps, vec![2, 1, 0]);
    assert_eq!(bad.world.effects(2), 0);

    // 2. 5000 random schedules under chaos
    let (mut committed, mut compensated, mut crashes, mut retries, mut involuntary, mut dup_aborts) =
        (0, 0, 0, 0, 0, 0);
    let mut orphans_without_tombstones = 0;
    let mut sample_log: Option<Vec<LogRec>> = None;
    for seed in 1..=5000u64 {
        let fail_at = fail_plan(&mut Rng::new(seed ^ 0xABCD));
        let reference = {
            let mut r = Run::new(seed, seed, fail_at, Cfg::clean());
            r.run()
        };
        let mut run = Run::new(seed, seed, fail_at, Cfg::chaos());
        let outcome = run.run();
        assert!(
            check(&run, outcome, reference),
            "seed {seed}: partial outcome with tombstones enabled"
        );
        match outcome {
            Outcome::Committed => committed += 1,
            Outcome::Compensated => compensated += 1,
        }
        crashes += run.crashes;
        retries += run.retries;
        involuntary += run.involuntary_abort() as u32;
        dup_aborts += run.log.iter().any(|r| matches!(r, LogRec::Abort(_))) as u32;
        if sample_log.is_none() && run.crashes >= 2 && outcome == Outcome::Compensated {
            sample_log = Some(run.log.clone());
        }

        // the SAME schedule with tombstones disabled: late duplicate forward requests can resurrect effects
        let mut naive = Run::new(
            seed,
            seed,
            fail_at,
            Cfg {
                tombstones: false,
                ..Cfg::chaos()
            },
        );
        let o = naive.run();
        let present = naive.world.effects(seed);
        if (o == Outcome::Compensated && present != 0) || (o == Outcome::Committed && present != N)
        {
            orphans_without_tombstones += 1;
        }
    }
    println!(
        "5000 chaos schedules: {committed} committed, {compensated} compensated, 0 partial; {crashes} orchestrator crashes, {retries} retries, {involuntary} involuntary aborts ({dup_aborts} deadlines)"
    );
    println!(
        "without tombstones the same schedules left {orphans_without_tombstones} partial sagas (orphan effects)"
    );
    assert!(
        committed > 1000 && compensated > 1000,
        "both outcomes must be exercised"
    );
    assert!(crashes > 1000 && retries > 1000);
    assert!(
        orphans_without_tombstones > 0,
        "the tombstone countermeasure must be demonstrably necessary"
    );
    if let Some(log) = sample_log {
        println!("a compensated saga that survived several crashes: {log:?}");
    }

    // 3. recovery: crash the orchestrator at EVERY possible point of a saga; each must reach the clean outcome
    for fail_at in [None, Some(0), Some(1), Some(2)] {
        let reference = Run::new(9, 9, fail_at, Cfg::clean()).run();
        let mut crash_points = 0;
        for seed in 0..200u64 {
            // p_crash = 40: crashes land at both points of many steps; no other faults, so the outcome is fully determined
            let mut r = Run::new(
                9,
                seed,
                fail_at,
                Cfg {
                    p_crash: 40,
                    ..Cfg::clean()
                },
            );
            let o = r.run();
            crash_points += r.crashes;
            assert_eq!(
                o, reference,
                "recovery from the log must reach the same outcome (fail_at {fail_at:?}, seed {seed})"
            );
            assert_eq!(
                r.world.effects(9),
                if o == Outcome::Committed { N } else { 0 }
            );
        }
        assert!(crash_points > 100);
    }
    println!(
        "recovery: the orchestrator was killed ~hundreds of times per plan; the log always led to the same outcome"
    );

    // 4. choreography: same business outcomes, no coordinator
    let (mut c_commit, mut c_comp, mut deliveries) = (0, 0, 0);
    for seed in 1..=2000u64 {
        let fail_at = fail_plan(&mut Rng::new(seed ^ 0x1234));
        let (o, d) = run_choreography(seed, fail_at);
        assert_eq!(
            o,
            if fail_at.is_some() {
                Outcome::Compensated
            } else {
                Outcome::Committed
            }
        );
        deliveries += d;
        if o == Outcome::Committed {
            c_commit += 1
        } else {
            c_comp += 1
        }
    }
    println!(
        "choreography, 2000 schedules: {c_commit} committed, {c_comp} compensated, {deliveries} deliveries; never partial"
    );
    assert!(c_commit > 500 && c_comp > 500);

    // 5. comparison with 2PC and the isolation anomaly
    demo_two_phase_commit();
    demo_isolation();
    println!("OK");
}
