use solution::*;

use std::collections::BTreeMap;

fn at(list: &[(u64, u32)]) -> Vec<Request> {
    list.iter().map(|&(at_ms, client)| Request { at_ms, client }).collect()
}

fn model_busiest(times: &[u64], span: u64) -> usize {
    times.iter().map(|&s| times.iter().filter(|&&t| t >= s && t - s < span).count()).max().unwrap_or(0)
}

fn make(p: Policy) -> Box<dyn Forget> {
    match p {
        Policy::FixedWindow { limit, window_ms } => Box::new(Wrap(FixedWindow::new(limit, window_ms))),
        Policy::SlidingLog { limit, window_ms } => Box::new(Wrap(SlidingLog::new(limit, window_ms))),
        Policy::SlidingCounter { limit, window_ms } => Box::new(Wrap(SlidingCounter::new(limit, window_ms))),
        Policy::Gcra { burst, interval_ms } => Box::new(Wrap(Gcra::new(burst, interval_ms))),
        Policy::TokenBucket { burst, interval_ms } | Policy::LeakyMeter { burst, interval_ms } => {
            Box::new(Wrap(TokenBucket::new(burst, interval_ms)))
        }
    }
}

/// The model only needs `check`; whether a limiter still matters is found by trying it.
struct Wrap<L>(L);
impl<L: Limiter> Limiter for Wrap<L> {
    fn check(&mut self, now_ms: u64) -> bool {
        self.0.check(now_ms)
    }
}
impl<L: Limiter> Forget for Wrap<L> {
    fn is_fresh(&self, _: u64) -> bool {
        unreachable!("the model never asks")
    }
}

/// Whether the limiter that saw `history` still behaves unlike a new one at `t`.
fn matters(p: Policy, history: &[u64], t: u64) -> bool {
    let mut old = make(p);
    for &h in history {
        old.check(h);
    }
    let mut new = make(p);
    let burst = |l: &mut Box<dyn Forget>| (0..20).filter(|_| l.check(t)).count();
    burst(&mut old) != burst(&mut new)
}

fn model(p: Policy, scope: Scope, traffic: &[Request], span: u64) -> Outcome {
    let mut limiters: BTreeMap<u32, (Box<dyn Forget>, Vec<u64>)> = BTreeMap::new();
    let mut latest = 0;
    let mut decisions = Vec::new();
    // (client, effective time, allowed)
    let mut log: Vec<(u32, u64, bool)> = Vec::new();
    for r in traffic {
        latest = r.at_ms.max(latest);
        let key = if scope == Scope::Shared { 0 } else { r.client };
        let (limiter, history) = limiters.entry(key).or_insert_with(|| (make(p), Vec::new()));
        let ok = limiter.check(latest);
        history.push(latest);
        decisions.push(ok);
        log.push((r.client, latest, ok));
    }
    let times = |client: Option<u32>| -> Vec<u64> { log.iter().filter(|e| e.2 && client.is_none_or(|c| c == e.0)).map(|e| e.1).collect() };
    let mut clients = BTreeMap::new();
    for &(client, _, _) in &log {
        let allowed = times(Some(client)).len();
        let all = log.iter().filter(|e| e.0 == client).count();
        clients.insert(client, ClientReport { allowed, denied: all - allowed, busiest: model_busiest(&times(Some(client)), span) });
    }
    let held = limiters.values().filter(|(_, history)| matters(p, history, latest)).count();
    Outcome { busiest: model_busiest(&times(None), span), decisions, clients, differs_from_first_at: None, held }
}

#[test]
fn random_traffic_and_lineups_match_a_model() {
    let mut rng = anneal_prelude::Rng::new(9215);
    for _ in 0..250 {
        let lineup: Vec<(Policy, Scope)> = (0..1 + rng.below(5))
            .map(|_| {
                let limit = rng.below(5) as u32;
                let length = 1 + rng.below(25) as u64;
                let policy = match rng.below(6) {
                    0 => Policy::FixedWindow { limit, window_ms: length },
                    1 => Policy::SlidingLog { limit, window_ms: length },
                    2 => Policy::SlidingCounter { limit, window_ms: length },
                    3 => Policy::TokenBucket { burst: limit, interval_ms: length },
                    4 => Policy::LeakyMeter { burst: limit, interval_ms: length },
                    _ => Policy::Gcra { burst: limit, interval_ms: length },
                };
                (policy, if rng.bool() { Scope::Shared } else { Scope::PerClient })
            })
            .collect();
        let mut now = 0u64;
        let traffic: Vec<Request> = (0..rng.below(40))
            .map(|_| {
                now = if rng.below(9) == 0 { now.saturating_sub(rng.below(20) as u64) } else { now + rng.below(6) as u64 };
                Request { at_ms: now, client: rng.below(4) as u32 }
            })
            .collect();
        let span = 1 + rng.below(30) as u64;
        let mut want: Vec<Outcome> = lineup.iter().map(|&(p, s)| model(p, s, &traffic, span)).collect();
        for i in 1..want.len() {
            want[i].differs_from_first_at = (0..traffic.len()).find(|&r| want[i].decisions[r] != want[0].decisions[r]);
        }
        let got = compare(&lineup, &traffic, span);
        check!("one outcome per entry", got.len(), want.len());
        for (i, (g, w)) in got.into_iter().zip(want).enumerate() {
            check!(format!("entry {i} of {lineup:?}, span {span}, {traffic:?}"), g, w);
        }
    }
}

#[test]
fn an_empty_lineup_and_empty_traffic() {
    check!("no entries", compare(&[], &at(&[(1, 1)]), 10), vec![]);
    let got = compare(&[(Policy::Gcra { burst: 1, interval_ms: 1 }, Scope::Shared)], &[], 10);
    check!("no requests", got, vec![Outcome { decisions: vec![], busiest: 0, clients: BTreeMap::new(), differs_from_first_at: None, held: 0 }]);
}

#[test]
fn the_three_equal_meters_never_differ() {
    let mut rng = anneal_prelude::Rng::new(9216);
    for _ in 0..200 {
        let (burst, interval_ms) = (rng.below(6) as u32, 1 + rng.below(20) as u64);
        let lineup = [
            (Policy::TokenBucket { burst, interval_ms }, Scope::PerClient),
            (Policy::LeakyMeter { burst, interval_ms }, Scope::PerClient),
            (Policy::Gcra { burst, interval_ms }, Scope::PerClient),
        ];
        let mut now = 0;
        let traffic: Vec<Request> = (0..60)
            .map(|_| {
                now += rng.below(9) as u64;
                Request { at_ms: now, client: rng.below(3) as u32 }
            })
            .collect();
        let got = compare(&lineup, &traffic, 100);
        check!(format!("burst {burst}, interval {interval_ms}"), (got[1] == got[0], got[2] == got[0]), (true, true));
    }
}

#[test]
fn compared_with_the_first_not_the_one_before() {
    let one = Policy::FixedWindow { limit: 1, window_ms: 100 };
    let two = Policy::FixedWindow { limit: 2, window_ms: 100 };
    let got = compare(&[(one, Scope::Shared), (two, Scope::Shared), (two, Scope::Shared)], &at(&[(0, 1), (1, 1)]), 10);
    check!("differences", [got[0].differs_from_first_at, got[1].differs_from_first_at, got[2].differs_from_first_at], [None, Some(1), Some(1)]);
}

#[test]
fn entries_do_not_affect_each_other() {
    let policy = Policy::TokenBucket { burst: 1, interval_ms: 100 };
    let traffic = at(&[(0, 1), (0, 2), (50, 1)]);
    let got = compare(&[(policy, Scope::Shared), (policy, Scope::PerClient), (policy, Scope::Shared)], &traffic, 10);
    check!("the same entry twice", got[2].decisions.clone(), got[0].decisions.clone());
    check!("it agrees with entry 0", got[2].differs_from_first_at, None);
    check!("per client", got[1].decisions.clone(), vec![true, true, false]);
}

#[test]
fn a_client_that_was_only_ever_refused_is_listed() {
    let got = compare(&[(Policy::SlidingLog { limit: 0, window_ms: 5 }, Scope::PerClient)], &at(&[(1, 9), (2, 4)]), 3);
    let refused = ClientReport { allowed: 0, denied: 1, busiest: 0 };
    check!("clients", got[0].clients.clone(), BTreeMap::from([(4, refused), (9, refused)]));
    check!("held", got[0].held, 0);
}

#[test]
fn held_is_judged_at_the_last_request() {
    let lineup = [
        (Policy::TokenBucket { burst: 2, interval_ms: 10 }, Scope::PerClient),
        (Policy::LeakyMeter { burst: 2, interval_ms: 10 }, Scope::PerClient),
        (Policy::SlidingCounter { limit: 2, window_ms: 10 }, Scope::PerClient),
    ];
    // client 1 spends one unit at 0; at 9 it is not yet repaid, at 10 it is
    let early = compare(&lineup, &at(&[(0, 1), (9, 2)]), 5);
    check!("at 9", early.iter().map(|o| o.held).collect::<Vec<_>>(), vec![2, 2, 2]);
    let later = compare(&lineup, &at(&[(0, 1), (10, 2)]), 5);
    check!("at 10: the counter still weighs it", later.iter().map(|o| o.held).collect::<Vec<_>>(), vec![1, 1, 2]);
}

#[test]
fn a_large_crowd() {
    // many clients, much traffic, and a span that covers most of it
    let lineup = [
        (Policy::SlidingCounter { limit: 1000, window_ms: 1 << 30 }, Scope::PerClient),
        (Policy::Gcra { burst: 3, interval_ms: 1 << 30 }, Scope::PerClient),
        (Policy::FixedWindow { limit: u32::MAX, window_ms: 7 }, Scope::Shared),
    ];
    let traffic: Vec<Request> = (0..200_000u32).map(|i| Request { at_ms: u64::from(i), client: i % 50_000 }).collect();
    let got = compare(&lineup, &traffic, 150_000);
    check!("per client", (got[0].held, got[1].held, got[0].clients.len()), (50_000, 50_000, 50_000));
    check!("busiest of everybody", got[2].busiest, 150_000);
    check!("gcra admits three of four each", got[1].decisions.iter().filter(|&&d| d).count(), 150_000);
}
