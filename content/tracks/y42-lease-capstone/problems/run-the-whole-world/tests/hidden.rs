use solution::*;
use std::collections::HashMap;

/// Delays that a test writes down: for each client and kind, the first calls take the listed values, and every
/// later call takes the default. It counts the calls.
#[derive(Default)]
struct Script {
    listed: HashMap<(u32, Wait), Vec<u64>>,
    calls: HashMap<(u32, Wait), usize>,
}

impl Script {
    #[allow(dead_code)]
    fn with(mut self, client: u32, kind: Wait, values: &[u64]) -> Script {
        self.listed.insert((client, kind), values.to_vec());
        self
    }
}

impl Delays for Script {
    fn wait(&mut self, client: u32, kind: Wait) -> u64 {
        let nth = self.calls.entry((client, kind)).or_insert(0);
        let value = self.listed.get(&(client, kind)).and_then(|v| v.get(*nth).copied());
        *nth += 1;
        value.unwrap_or(match kind {
            Wait::Start | Wait::Pause => 0,
            Wait::Net | Wait::Work => 1,
            Wait::Backoff | Wait::Rest => 100,
        })
    }
}

#[allow(dead_code)]
fn config(clients: u32, leases: u32, writes: u32, ttl_ms: u64, guarded: bool) -> Config {
    Config {
        clients,
        leases_per_client: leases,
        writes_per_lease: writes,
        ttl_ms,
        guarded,
        max_events: 100_000,
    }
}

#[allow(dead_code)]
fn run(config: &Config, script: &mut Script) -> Report {
    simulate(config, script).expect("this run must finish")
}

#[allow(dead_code)]
fn lines(report: &Report) -> Vec<(u64, u32, What)> {
    report.trace.iter().map(|e| (e.time, e.client, e.what)).collect()
}

#[test]
fn requests_that_arrive_together_are_served_in_planning_order() {
    // client 1 wakes first, so its request is planned first; both arrive at time 6
    let mut script = Script::default()
        .with(0, Wait::Start, &[5])
        .with(1, Wait::Start, &[3])
        .with(1, Wait::Net, &[3]);
    let report = run(&config(2, 1, 0, 100, false), &mut script);
    check!("client 1 first", report.trace[0], Entry { time: 6, client: 1, what: What::Granted { token: 1, expires_at: 106 } });
    check!("client 0 refused", report.trace[1], Entry { time: 6, client: 0, what: What::Refused });
}

#[test]
fn equal_start_times_go_by_client_number() {
    let report = run(&config(3, 1, 0, 10, false), &mut Script::default());
    let first: Vec<u32> = report.trace.iter().take(3).map(|e| e.client).collect();
    check!("0, 1, 2", first, vec![0, 1, 2]);
}

#[test]
fn the_service_acts_when_the_request_arrives() {
    // the request spends 500 ms on its way: the lease counts from its arrival
    let mut script = Script::default().with(0, Wait::Net, &[500]);
    let report = run(&config(1, 1, 1, 100, false), &mut script);
    check!("grant", report.trace[0], Entry { time: 500, client: 0, what: What::Granted { token: 1, expires_at: 600 } });
    check!("write", report.arrived.first().map(|w| w.time), Some(503));
}

#[test]
fn work_ends_when_it_ends() {
    // granted until 11, told at 2; the work takes 8, 9 or 10 ms
    for (work, passed) in [(8, true), (9, false), (10, false)] {
        let mut script = Script::default().with(0, Wait::Work, &[work]);
        let report = run(&config(1, 1, 1, 10, false), &mut script);
        check!(format!("work {work}"), report.trace[1], Entry { time: 2 + work, client: 0, what: What::Checked { passed } });
    }
}

#[test]
fn values_are_given_when_sent() {
    // client 0 sends first, and its write takes so long that client 1's arrives before it
    let mut script = Script::default()
        .with(0, Wait::Net, &[1, 1, 400])
        .with(1, Wait::Start, &[150]);
    let report = run(&config(2, 1, 1, 100, true), &mut script);
    let order: Vec<(u32, u64, u64)> = report.arrived.iter().map(|w| (w.client, w.token, w.value)).collect();
    check!("arrival order", order, vec![(1, 2, 2), (0, 1, 1)]);
    check!("accepted", report.accepted.len(), 1);
}

#[test]
fn a_renewal_that_comes_too_late() {
    // the answer to the write takes 200 ms
    let mut script = Script::default().with(0, Wait::Net, &[1, 1, 1, 200]);
    let report = run(&config(1, 1, 2, 100, false), &mut script);
    check!("refused", report.trace.last().map(|e| e.what), Some(What::Renewal { token: 1, expires_at: None }));
    check!("one write only", report.arrived.len(), 1);
}

#[test]
fn times_beyond_u64_are_an_error() {
    let mut script = Script::default().with(0, Wait::Start, &[u64::MAX]);
    check!("on the first message", simulate(&config(1, 1, 1, 10, false), &mut script), Err(SimError::TimeOverflow));
    let mut script = Script::default().with(0, Wait::Rest, &[u64::MAX]);
    check!("at the very end", simulate(&config(1, 1, 1, 10, false), &mut script), Err(SimError::TimeOverflow));
}

#[test]
#[should_panic]
fn a_lease_of_no_length_panics() {
    let _ = simulate(&config(1, 1, 1, 0, false), &mut Script::default());
}

/// Random delays that depend only on the seed, the client, the kind and how many came before.
struct Chance {
    seed: u64,
    calls: HashMap<(u32, Wait), u64>,
    log: HashMap<(u32, Wait), Vec<u64>>,
}

impl Delays for Chance {
    fn wait(&mut self, client: u32, kind: Wait) -> u64 {
        let nth = self.calls.entry((client, kind)).or_insert(0);
        let mix = self.seed * 1_000_000 + u64::from(client) * 10_000 + kind as u64 * 1000 + *nth;
        *nth += 1;
        let mut rng = anneal_prelude::Rng::new(mix);
        let stall = if rng.below(5) == 0 { rng.int(20, 120) as u64 } else { 0 };
        let value = match kind {
            Wait::Start => rng.int(0, 10) as u64,
            Wait::Net => rng.int(1, 8) as u64,
            Wait::Work => rng.int(1, 10) as u64 + stall,
            Wait::Pause => stall,
            Wait::Backoff => rng.int(5, 20) as u64,
            Wait::Rest => rng.int(5, 15) as u64,
        };
        self.log.entry((client, kind)).or_default().push(value);
        value
    }
}

#[test]
fn random_worlds_keep_their_promises() {
    let (mut open_bad, mut rejected, mut failed_checks) = (0, 0, 0);
    for seed in 0..250u64 {
        let ttl = 40;
        let mut reports = Vec::new();
        for guarded in [false, true] {
            let mut chance = Chance { seed, calls: HashMap::new(), log: HashMap::new() };
            let cfg = config(3, 2, 3, ttl, guarded);
            let report = simulate(&cfg, &mut chance).expect("finishes");
            let label = format!("seed {seed}, guarded {guarded}");

            check!(format!("{label}: time never goes back"), report.trace.windows(2).all(|p| p[0].time <= p[1].time), true);
            // the service: tokens count up, and no grant comes before the earlier lease has run out
            let (mut token, mut end, mut owner, mut grants) = (0, 0, 0, [0; 3]);
            let mut sent = 0;
            let mut checked: [Option<(u64, u64)>; 3] = [None; 3]; // (time, pause)
            let mut on_the_way: HashMap<u64, u64> = HashMap::new(); // value -> when sent
            let (mut stored, mut pauses_seen) = (Vec::new(), [0usize; 3]);
            for e in &report.trace {
                let c = e.client as usize;
                match e.what {
                    What::Granted { token: t, expires_at } => {
                        check!(format!("{label}: token at {e:?}"), t, token + 1);
                        check!(format!("{label}: the lease before has run out at {e:?}"), e.time >= end, true);
                        check!(format!("{label}: end of {e:?}"), expires_at, e.time + ttl);
                        (token, end, owner) = (t, expires_at, e.client);
                        grants[c] += 1;
                    }
                    What::Refused => check!(format!("{label}: refused only while held, {e:?}"), e.time < end, true),
                    What::Renewal { token: t, expires_at } => {
                        let live = t == token && owner == e.client && e.time < end;
                        check!(format!("{label}: {e:?}"), expires_at, live.then_some(e.time + ttl));
                        if let Some(new_end) = expires_at {
                            end = new_end;
                        }
                    }
                    What::Checked { passed } => {
                        checked[c] = passed.then_some((e.time, 0));
                        pauses_seen[c] += usize::from(passed);
                        failed_checks += u32::from(!passed);
                    }
                    What::Paused { ms } => {
                        check!(format!("{label}: a pause is above 0"), ms > 0, true);
                        checked[c] = checked[c].map(|(at, _)| (at, ms));
                    }
                    What::Sent { value, .. } => {
                        sent += 1;
                        check!(format!("{label}: values count up at {e:?}"), value, sent);
                        let (at, pause) = checked[c].take().expect("a write needs a passed check");
                        check!(format!("{label}: sent when the pause ends, {e:?}"), e.time, at + pause);
                        on_the_way.insert(value, e.time);
                    }
                    What::Stored { token, value, accepted } => {
                        let travelled = e.time - on_the_way[&value];
                        check!(format!("{label}: travel time of value {value}"), chance.log[&(e.client, Wait::Net)].contains(&travelled), true);
                        stored.push((Write { time: e.time, client: e.client, token, value }, accepted));
                    }
                }
            }
            check!(format!("{label}: every client got its leases"), grants, [2, 2, 2]);
            for c in 0..3u32 {
                let asked = chance.log.get(&(c, Wait::Pause)).map_or(0, Vec::len);
                check!(format!("{label}: one Pause call per passed check, client {c}"), asked, pauses_seen[c as usize]);
                check!(format!("{label}: one Start call, client {c}"), chance.log[&(c, Wait::Start)].len(), 1);
            }
            let arrived: Vec<Write> = stored.iter().map(|s| s.0).collect();
            let accepted: Vec<Write> = stored.iter().filter(|s| s.1).map(|s| s.0).collect();
            check!(format!("{label}: arrived matches the trace"), report.arrived.clone(), arrived);
            check!(format!("{label}: accepted matches the trace"), report.accepted.clone(), accepted);
            check!(format!("{label}: value"), report.value, report.accepted.last().map(|w| w.value));
            let verdict = judge(&report.arrived, &report.accepted, report.value);
            if guarded {
                check!(format!("{label}: a guarded disk is safe"), verdict, Ok(()));
                rejected += report.arrived.len() - report.accepted.len();
            } else {
                open_bad += u32::from(verdict.is_err());
            }
            reports.push(report);
        }
        let (open, guarded) = (&reports[0], &reports[1]);
        check!(format!("seed {seed}: the disk does not change what is sent"), &guarded.arrived, &open.arrived);
        check!(format!("seed {seed}: nor when the run ends"), guarded.end_time, open.end_time);
    }
    check!("some unguarded runs break", open_bad > 20, true);
    check!("some writes are turned away", rejected > 20, true);
    check!("some checks fail", failed_checks > 20, true);
}

#[test]
fn a_crowd_of_three_hundred() {
    let mut cfg = config(300, 1, 2, 5, true);
    cfg.max_events = 5_000_000;
    // refused clients come back every few milliseconds
    let mut script = Script::default();
    for c in 0..300 {
        script = script.with(c, Wait::Backoff, &[7; 3000]);
    }
    let report = simulate(&cfg, &mut script).expect("finishes");
    let grants = report.trace.iter().filter(|e| matches!(e.what, What::Granted { .. })).count();
    check!("one lease each", grants, 300);
    check!("a busy world", report.trace.len() > 10_000, true);
}
