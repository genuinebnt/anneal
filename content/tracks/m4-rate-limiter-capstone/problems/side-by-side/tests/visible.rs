use solution::*;

fn at(list: &[(u64, u32)]) -> Vec<Request> {
    list.iter().map(|&(at_ms, client)| Request { at_ms, client }).collect()
}

#[test]
fn the_edge_again_for_two_policies() {
    let lineup = [
        (Policy::SlidingLog { limit: 2, window_ms: 100 }, Scope::Shared),
        (Policy::FixedWindow { limit: 2, window_ms: 100 }, Scope::Shared),
    ];
    let got = compare(&lineup, &at(&[(99, 1), (99, 1), (100, 1), (100, 1)]), 100);
    check!("busiest", (got[0].busiest, got[1].busiest), (2, 4));
    check!("first difference", (got[0].differs_from_first_at, got[1].differs_from_first_at), (None, Some(2)));
}

#[test]
fn a_noisy_client_and_a_quiet_one() {
    let mut list: Vec<(u64, u32)> = (0..100).map(|i| (i * 10, 0)).collect();
    list.extend([(505, 7), (905, 7)]);
    list.sort();
    let bucket = Policy::TokenBucket { burst: 2, interval_ms: 100 };
    let got = compare(&[(bucket, Scope::Shared), (bucket, Scope::PerClient)], &at(&list), 1000);
    check!("shared: the quiet client suffers", got[0].clients[&7], ClientReport { allowed: 0, denied: 2, busiest: 0 });
    check!("per client: it does not", got[1].clients[&7], ClientReport { allowed: 2, denied: 0, busiest: 2 });
}

#[test]
fn limiters_still_held_at_the_end() {
    let policy = Policy::LeakyMeter { burst: 1, interval_ms: 50 };
    let got = compare(&[(policy, Scope::PerClient)], &at(&[(0, 1), (60, 2), (70, 3)]), 10);
    check!("client 1 is long paid off", got[0].held, 2);
}

#[test]
fn an_earlier_time_counts_across_clients() {
    let policy = Policy::FixedWindow { limit: 1, window_ms: 100 };
    let got = compare(&[(policy, Scope::PerClient)], &at(&[(150, 1), (20, 2), (160, 2)]), 1);
    check!("client 2 first counts at 150", (got[0].decisions.clone(), got[0].busiest), (vec![true, true, false], 2));
}

#[test]
fn every_policy_can_be_named() {
    let lineup = [
        (Policy::FixedWindow { limit: 1, window_ms: 10 }, Scope::Shared),
        (Policy::SlidingLog { limit: 1, window_ms: 10 }, Scope::Shared),
        (Policy::SlidingCounter { limit: 1, window_ms: 10 }, Scope::Shared),
        (Policy::TokenBucket { burst: 1, interval_ms: 10 }, Scope::Shared),
        (Policy::LeakyMeter { burst: 1, interval_ms: 10 }, Scope::Shared),
        (Policy::Gcra { burst: 1, interval_ms: 10 }, Scope::Shared),
    ];
    for outcome in compare(&lineup, &at(&[(5, 0), (5, 0)]), 10) {
        check!("one passes", outcome.decisions, vec![true, false]);
    }
}
