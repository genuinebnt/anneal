use solution::*;

#[allow(dead_code)]
fn config() -> Config {
    Config {
        clients: 3,
        leases_per_client: 3,
        writes_per_lease: 3,
        ttl_ms: 100,
        guarded: false,
        max_events: 100_000,
    }
}

#[allow(dead_code)]
fn breaks(report: &Report) -> bool {
    judge(&report.arrived, &report.accepted, report.value).is_err()
}

#[test]
fn every_kind_stays_in_its_range() {
    let mut delays = SeededDelays::new(1, 0, 100);
    for _ in 0..200 {
        for (kind, lo, hi) in [(Wait::Start, 0, 10), (Wait::Net, 1, 20), (Wait::Work, 5, 30), (Wait::Pause, 0, 0), (Wait::Backoff, 20, 60), (Wait::Rest, 10, 40)] {
            let got = delays.wait(0, kind);
            check!(format!("{kind:?} gave {got}, expected {lo}..={hi}"), (lo..=hi).contains(&got), true);
        }
    }
}

#[test]
fn a_seed_replays_its_run() {
    let a = run_seed(&config(), 30, 5).expect("finishes");
    check!("the same run again", run_seed(&config(), 30, 5), Ok(a));
}

#[test]
fn the_same_seed_against_two_disks() {
    let pair = run_pair(&config(), 30, 5).expect("finishes");
    check!("the same writes arrive", &pair.open.arrived, &pair.guarded.arrived);
    check!("the unguarded disk takes everything", pair.open.accepted.len(), pair.open.arrived.len());
    check!("the guarded disk is safe", breaks(&pair.guarded), false);
}

#[test]
fn stalls_find_a_bad_run() {
    let (seed, breach) = first_breach(&config(), 30, 0..1000).expect("finishes").expect("a bad run exists");
    let report = run_seed(&config(), 30, seed).expect("finishes");
    check!(format!("seed {seed} replays the breach"), judge(&report.arrived, &report.accepted, report.value), Err(breach));
}

#[test]
fn the_campaign_shows_what_the_guard_does() {
    let summary = campaign(&config(), 30, 0..200).expect("finishes");
    check!("runs", summary.runs, 200);
    check!("the guard never breaks", summary.guarded_breaches, 0);
    check!(format!("unguarded disks often do: {}", summary.open_breaches), summary.open_breaches > 20, true);
    check!("because stale writes were turned away", summary.rejected > 0, true);
}

#[test]
fn the_story_of_a_run_written_by_hand() {
    let entry = |time, client, what| Entry { time, client, what };
    let old = Write { time: 50, client: 0, token: 1, value: 2 };
    let new = Write { time: 40, client: 1, token: 2, value: 1 };
    let trace = vec![
        entry(1, 2, What::Refused),
        entry(2, 0, What::Granted { token: 1, expires_at: 12 }),
        entry(3, 0, What::Checked { passed: true }),
        entry(3, 0, What::Paused { ms: 40 }),
        entry(20, 1, What::Granted { token: 2, expires_at: 30 }),
        entry(21, 2, What::Refused),
        entry(40, 1, What::Stored { token: 2, value: 1, accepted: true }),
        entry(43, 0, What::Sent { token: 1, value: 2 }),
        entry(50, 0, What::Stored { token: 1, value: 2, accepted: true }),
        entry(60, 0, What::Renewal { token: 1, expires_at: None }),
    ];
    let report = Report {
        trace: trace.clone(),
        arrived: vec![new, old],
        accepted: vec![new, old],
        value: Some(2),
        end_time: 60,
    };
    let want: Vec<Entry> = [1, 2, 3, 4, 6, 7, 8].iter().map(|&i| trace[i]).collect();
    check!("story", story(&report), Some(want));
}
