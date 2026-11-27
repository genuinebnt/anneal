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
fn one_client_two_writes() {
    let mut script = Script::default();
    let report = run(&config(1, 1, 2, 50, false), &mut script);
    use What::*;
    check!("trace", lines(&report), vec![
        (1, 0, Granted { token: 1, expires_at: 51 }),
        (3, 0, Checked { passed: true }),
        (3, 0, Sent { token: 1, value: 1 }),
        (4, 0, Stored { token: 1, value: 1, accepted: true }),
        (6, 0, Renewal { token: 1, expires_at: Some(56) }),
        (8, 0, Checked { passed: true }),
        (8, 0, Sent { token: 1, value: 2 }),
        (9, 0, Stored { token: 1, value: 2, accepted: true }),
    ]);
    check!("value", report.value, Some(2));
    check!("the last event is the wake after the rest", report.end_time, 110);
    for (kind, times) in [(Wait::Start, 1), (Wait::Net, 8), (Wait::Work, 2), (Wait::Pause, 2), (Wait::Rest, 1), (Wait::Backoff, 0)] {
        check!(format!("calls for {kind:?}"), script.calls.get(&(0, kind)).copied().unwrap_or(0), times);
    }
}

/// Client 0 passes its check and is then held up for 300 ms. Client 1 arrives in the meantime.
#[allow(dead_code)]
fn stall() -> Script {
    Script::default().with(0, Wait::Pause, &[300]).with(1, Wait::Start, &[150])
}

#[test]
fn a_stall_after_the_check_corrupts_an_unguarded_disk() {
    let report = run(&config(2, 1, 1, 100, false), &mut stall());
    use What::*;
    check!("trace", lines(&report), vec![
        (1, 0, Granted { token: 1, expires_at: 101 }),
        (3, 0, Checked { passed: true }),
        (3, 0, Paused { ms: 300 }),
        (151, 1, Granted { token: 2, expires_at: 251 }),
        (153, 1, Checked { passed: true }),
        (153, 1, Sent { token: 2, value: 1 }),
        (154, 1, Stored { token: 2, value: 1, accepted: true }),
        (303, 0, Sent { token: 1, value: 2 }),
        (304, 0, Stored { token: 1, value: 2, accepted: true }),
    ]);
    check!("the old holder had the last word", report.value, Some(2));
    check!("judged", judge(&report.arrived, &report.accepted, report.value).is_err(), true);
}

#[test]
fn the_guard_stops_the_same_run() {
    let open = run(&config(2, 1, 1, 100, false), &mut stall());
    let guarded = run(&config(2, 1, 1, 100, true), &mut stall());
    check!("the same writes arrive", guarded.arrived.clone(), open.arrived);
    check!("value", guarded.value, Some(1));
    check!("judged", judge(&guarded.arrived, &guarded.accepted, guarded.value), Ok(()));
}

#[test]
fn a_stall_before_the_check_is_caught() {
    let mut script = Script::default().with(0, Wait::Work, &[300]);
    let report = run(&config(1, 1, 3, 100, false), &mut script);
    check!("nothing was written", report.arrived.len(), 0);
    check!("the check failed", report.trace.last().map(|e| e.what), Some(What::Checked { passed: false }));
}

#[test]
fn nobody_there() {
    let report = run(&config(0, 5, 5, 10, true), &mut Script::default());
    check!("nothing", (report.trace.len(), report.value, report.end_time), (0, None, 0));
}

#[test]
fn the_event_budget() {
    // one client with two writes needs 14 events
    let mut cfg = config(1, 1, 2, 50, false);
    cfg.max_events = 14;
    check!("exactly enough", simulate(&cfg, &mut Script::default()).is_ok(), true);
    cfg.max_events = 13;
    check!("one short", simulate(&cfg, &mut Script::default()), Err(SimError::OutOfSteps));
}
