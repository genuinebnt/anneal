use solution::*;
use std::collections::HashSet;

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

const KINDS: [Wait; 6] = [Wait::Start, Wait::Net, Wait::Work, Wait::Pause, Wait::Backoff, Wait::Rest];

#[allow(dead_code)]
fn verdict(report: &Report) -> Result<(), Breach> {
    judge(&report.arrived, &report.accepted, report.value)
}

#[test]
fn other_calls_change_nothing() {
    let mut rng = anneal_prelude::Rng::new(42_501);
    for round in 0..100 {
        let seed = rng.next_u64();
        let mine = (rng.below(4) as u32, *rng.pick(&KINDS));
        let mut quiet = SeededDelays::new(seed, 40, 100);
        let want: Vec<u64> = (0..20).map(|_| quiet.wait(mine.0, mine.1)).collect();
        let mut busy = SeededDelays::new(seed, 40, 100);
        let mut got = Vec::new();
        while got.len() < 20 {
            if rng.below(4) == 0 {
                got.push(busy.wait(mine.0, mine.1));
            } else {
                let other = (rng.below(4) as u32, *rng.pick(&KINDS));
                if other != mine {
                    busy.wait(other.0, other.1);
                }
            }
        }
        check!(format!("round {round}: seed {seed}, {mine:?} among other calls"), got, want);
    }
}

#[test]
fn clients_kinds_and_seeds_do_not_share_their_luck() {
    // with every stall certain and a huge ttl, Pause and Work take values from a huge range: two of them lie
    // within 30 of each other only if they come from the same raw numbers
    let mut seen: Vec<(u64, u64, u32, Wait)> = Vec::new();
    for seed in 0..6 {
        for client in 0..12 {
            let mut delays = SeededDelays::new(seed, 100, 1 << 50);
            for kind in [Wait::Pause, Wait::Work] {
                seen.extend((0..40).map(|_| (delays.wait(client, kind), seed, client, kind)));
            }
        }
    }
    seen.sort_by_key(|s| s.0);
    let shared = seen.windows(2).find(|p| p[1].0 - p[0].0 <= 30);
    check!("no two delays come from the same numbers", shared.map(|p| (p[0], p[1])), None);
    // small ranges: the sequences of two kinds must not move together
    let mut delays = SeededDelays::new(3, 0, 100);
    let net: Vec<u64> = (0..40).map(|_| delays.wait(0, Wait::Net) - 1).collect();
    let rest: Vec<u64> = (0..40).map(|_| delays.wait(0, Wait::Rest) - 10).collect();
    check!("Net and Rest differ", net != rest, true);
    let other: Vec<u64> = (0..40).map(|_| delays.wait(1, Wait::Net) - 1).collect();
    check!("clients differ", net != other, true);
}

#[test]
fn every_value_of_each_range_can_happen() {
    let mut delays = SeededDelays::new(8, 0, 100);
    for (kind, lo, hi) in [(Wait::Start, 0u64, 10u64), (Wait::Net, 1, 20), (Wait::Work, 5, 30), (Wait::Backoff, 20, 60), (Wait::Rest, 10, 40)] {
        let seen: HashSet<u64> = (0..2000).map(|_| delays.wait(0, kind)).collect();
        check!(format!("{kind:?}: distinct values"), seen.len() as u64, hi - lo + 1);
        check!(format!("{kind:?}: ends"), (seen.iter().min().copied(), seen.iter().max().copied()), (Some(lo), Some(hi)));
    }
}

#[test]
fn stalls_have_the_asked_rate_and_length() {
    for percent in [0u32, 25, 100] {
        let mut delays = SeededDelays::new(9, percent, 1000);
        let pauses: Vec<u64> = (0..4000).map(|_| delays.wait(2, Wait::Pause)).collect();
        let stalls = pauses.iter().filter(|&&p| p > 0).count() as i64;
        check!(format!("{percent}%: {stalls} stalls of 4000"), (stalls - 40 * i64::from(percent)).abs() <= 250, true);
        check!(format!("{percent}%: stall length"), pauses.iter().all(|&p| p == 0 || (500..=3000).contains(&p)), true);
        let work: Vec<u64> = (0..4000).map(|_| delays.wait(2, Wait::Work)).collect();
        let stalled = work.iter().filter(|&&w| w > 30).count() as i64;
        check!(format!("{percent}%: {stalled} stalled works of 4000"), (stalled - 40 * i64::from(percent)).abs() <= 250, true);
        check!(format!("{percent}%: a stall is added to the work"), work.iter().all(|&w| (5..=30).contains(&w) || (505..=3030).contains(&w)), true);
    }
}

#[test]
fn huge_settings_do_not_overflow() {
    let mut delays = SeededDelays::new(1, 100, u64::MAX);
    for _ in 0..100 {
        check!("a stall is at least half the ttl", delays.wait(0, Wait::Pause) >= u64::MAX / 2, true);
        check!("work plus stall stops at the top", delays.wait(0, Wait::Work) >= u64::MAX / 2, true);
    }
    // one client, so that nobody polls while it stalls
    let huge = Config { clients: 1, ttl_ms: u64::MAX, ..config() };
    check!("the world reports it", campaign(&huge, 100, 0..3), Err(SimError::TimeOverflow));
    check!("here too", first_breach(&huge, 100, 0..3), Err(SimError::TimeOverflow));
}

#[test]
fn errors_come_through() {
    let tight = Config { max_events: 5, ..config() };
    check!("run_seed", run_seed(&tight, 30, 1), Err(SimError::OutOfSteps));
    check!("run_pair", run_pair(&tight, 30, 1), Err(SimError::OutOfSteps));
    check!("campaign", campaign(&tight, 30, 0..3), Err(SimError::OutOfSteps));
    check!("first_breach", first_breach(&tight, 30, 0..3), Err(SimError::OutOfSteps));
    check!("an empty range", campaign(&tight, 30, 7..7), Ok(Summary::default()));
}

#[test]
fn run_pair_ignores_the_guard_setting() {
    for guarded in [false, true] {
        let cfg = Config { guarded, ..config() };
        for seed in 0..30 {
            let pair = run_pair(&cfg, 40, seed).expect("finishes");
            check!(format!("seed {seed}: open"), Ok(pair.open), run_seed(&Config { guarded: false, ..cfg }, 40, seed));
            check!(format!("seed {seed}: guarded"), Ok(pair.guarded), run_seed(&Config { guarded: true, ..cfg }, 40, seed));
        }
    }
}

#[test]
fn run_seed_uses_the_seeded_delays() {
    for seed in 0..20 {
        let mut delays = SeededDelays::new(seed, 35, 100);
        check!(format!("seed {seed}"), run_seed(&config(), 35, seed), simulate(&config(), &mut delays));
    }
}

#[test]
fn first_breach_and_campaign_match_doing_it_by_hand() {
    // the guard setting of the config must not matter to either
    let cfg = Config { guarded: true, ..config() };
    let mut want = Summary::default();
    let mut first = None;
    for seed in 100..260 {
        let pair = run_pair(&cfg, 12, seed).expect("finishes");
        want.runs += 1;
        if let Err(breach) = verdict(&pair.open) {
            want.open_breaches += 1;
            first = first.or(Some((seed, breach)));
        }
        want.guarded_breaches += u64::from(verdict(&pair.guarded).is_err());
        want.rejected += (pair.guarded.arrived.len() - pair.guarded.accepted.len()) as u64;
    }
    want.first_breach = first.map(|f| f.0);
    check!("some break, not all", want.open_breaches > 0 && want.open_breaches < want.runs, true);
    check!("campaign", campaign(&cfg, 12, 100..260), Ok(want));
    check!("first_breach", first_breach(&cfg, 12, 100..260), Ok(first));
}

#[test]
fn the_search_stops_at_the_first_bad_seed() {
    let found = first_breach(&config(), 40, 0..u64::MAX).expect("finishes");
    check!("found one early", found.is_some_and(|f| f.0 < 100), true);
}

#[test]
fn without_stalls_nothing_breaks() {
    // a ttl of 100 is far longer than one message and one piece of work
    check!("no breach", first_breach(&config(), 0, 0..300), Ok(None));
    let summary = campaign(&config(), 0, 0..100).expect("finishes");
    check!("quiet", (summary.open_breaches, summary.rejected, summary.first_breach), (0, 0, None));
}

#[test]
fn stories_match_a_slow_cut_of_the_trace() {
    let mut told = 0;
    for seed in 0..300 {
        let pair = run_pair(&config(), 40, seed).expect("finishes");
        check!(format!("seed {seed}: a safe run has no story"), story(&pair.guarded), None);
        let want = match verdict(&pair.open) {
            Err(Breach::StaleWrite { newer, stale }) => {
                let mut keep = Vec::new();
                let mut inside = false;
                for &e in &pair.open.trace {
                    inside |= matches!(e.what, What::Granted { token, .. } if token == stale.token);
                    if inside && [newer.client, stale.client].contains(&e.client) {
                        keep.push(e);
                    }
                    if matches!(e.what, What::Stored { token, value, .. } if token == stale.token && value == stale.value) {
                        break;
                    }
                }
                told += 1;
                Some(keep)
            }
            _ => None,
        };
        check!(format!("seed {seed}: story"), story(&pair.open), want);
    }
    check!("many stories were told", told > 100, true);
}

#[test]
fn a_lost_newest_write_has_no_story() {
    let write = Write { time: 3, client: 0, token: 1, value: 1 };
    let report = Report {
        trace: vec![Entry { time: 1, client: 0, what: What::Granted { token: 1, expires_at: 9 } }],
        arrived: vec![write],
        accepted: vec![],
        value: None,
        end_time: 3,
    };
    check!("no story", story(&report), None);
}
