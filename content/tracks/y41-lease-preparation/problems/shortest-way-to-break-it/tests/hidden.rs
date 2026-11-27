use solution::*;

#[allow(dead_code)]
fn add<const N: u64>(s: &mut u64) {
    *s += N;
}

#[allow(dead_code)]
fn double(s: &mut u64) {
    *s = (*s * 2) % 97;
}

#[allow(dead_code)]
fn clear(s: &mut u64) {
    *s = 0;
}

#[test]
fn shortest_wins_over_dictionary_order() {
    // actor 0 can reach 3 in three steps, actor 1 in one
    let actors: Vec<Vec<Step<u64>>> = vec![vec![add::<1>; 3], vec![add::<3>]];
    check!("one step", shortest_failure(&0, &actors, |&s| s < 3), Some(vec![1]));
}

#[test]
fn equal_lengths_go_to_the_smaller_actor_first() {
    let actors: Vec<Vec<Step<u64>>> = vec![vec![add::<1>; 2], vec![add::<5>; 2], vec![add::<5>; 2]];
    check!("0 then 1", shortest_failure(&0, &actors, |&s| s != 6), Some(vec![0, 1]));
    check!("1 then 1", shortest_failure(&0, &actors, |&s| s != 10), Some(vec![1, 1]));
}

#[test]
fn equal_states_at_different_places_are_different() {
    // after [0] and after [1] the state is 0, but what can still happen differs
    let nothing: Step<u64> = |_| {};
    let actors: Vec<Vec<Step<u64>>> = vec![vec![nothing, nothing], vec![nothing, add::<7>]];
    check!("only actor 1 can add", shortest_failure(&0, &actors, |&s| s != 7), Some(vec![1, 1]));
}

#[test]
fn no_actors() {
    let actors: Vec<Vec<Step<u64>>> = Vec::new();
    check!("good start", shortest_failure(&0, &actors, |_| true), None);
    check!("bad start", shortest_failure(&0, &actors, |_| false), Some(vec![]));
    check!("replay nothing", replay(&1, &actors, &[]), Some(vec![1]));
    check!("replay something", replay(&1, &actors, &[0]), None);
}

#[test]
fn a_repaired_state_is_still_a_failure() {
    let actors: Vec<Vec<Step<u64>>> = vec![vec![add::<9>, clear]];
    let found = shortest_failure(&0, &actors, |&s| s < 5);
    check!("found in the middle", found, Some(vec![0]));
}

/// Tries every schedule one by one and keeps the best.
#[allow(dead_code)]
fn every_schedule(
    start: u64,
    actors: &[Vec<Step<u64>>],
    bad: u64,
    schedule: &mut Vec<usize>,
    best: &mut Option<Vec<usize>>,
) {
    let mut done = vec![0; actors.len()];
    let mut state = start;
    for &who in schedule.iter() {
        actors[who][done[who]](&mut state);
        done[who] += 1;
    }
    if state == bad {
        let better = match best {
            Some(b) => (schedule.len(), &*schedule) < (b.len(), b),
            None => true,
        };
        if better {
            *best = Some(schedule.clone());
        }
        return;
    }
    for who in 0..actors.len() {
        if done[who] < actors[who].len() {
            schedule.push(who);
            every_schedule(start, actors, bad, schedule, best);
            schedule.pop();
        }
    }
}

#[test]
fn matches_trying_every_schedule() {
    let mut rng = anneal_prelude::Rng::new(41_301);
    let kinds: [Step<u64>; 4] = [add::<1>, add::<3>, double, clear];
    let mut found_some = 0;
    for round in 0..400 {
        let actors: Vec<Vec<Step<u64>>> = (0..rng.int(1, 3))
            .map(|_| (0..rng.int(0, 3)).map(|_| *rng.pick(&kinds)).collect())
            .collect();
        let bad = rng.int(1, 12) as u64;
        let mut want = None;
        every_schedule(0, &actors, bad, &mut Vec::new(), &mut want);
        let got = shortest_failure(&0, &actors, |&s| s != bad);
        if let Some(schedule) = &got {
            found_some += 1;
            let states = replay(&0, &actors, schedule).expect("a found schedule must run");
            check!(format!("round {round}: it ends badly"), states.last(), Some(&bad));
        }
        check!(format!("round {round}: avoid {bad}"), got, want);
    }
    check!("many rounds had a failure", found_some > 50, true);
}

#[test]
fn six_actors_of_six_steps_with_no_failure() {
    let actors: Vec<Vec<Step<u64>>> = vec![vec![add::<1>; 6]; 6];
    check!("a counter never passes 36", shortest_failure(&0, &actors, |&s| s <= 36), None);
}

#[test]
fn six_actors_with_a_deep_failure() {
    let mut actors: Vec<Vec<Step<u64>>> = vec![vec![add::<1>; 6]; 6];
    actors[5][5] = clear;
    let found = shortest_failure(&0, &actors, |&s| s != 0 || s > 100);
    // the start is 0: it fails at once
    check!("bad start", found, Some(vec![]));
    let deep = shortest_failure(&1, &actors, |&s| s != 0);
    check!("only the last actor's last step clears", deep, Some(vec![5; 6]));
    let counters: Vec<Vec<Step<u64>>> = vec![vec![add::<1>; 6]; 6];
    let far = shortest_failure(&0, &counters, |&s| s != 30);
    let mut want = Vec::new();
    for who in 0..5 {
        want.extend([who; 6]);
    }
    check!("thirty steps, smallest actors first", far, Some(want));
}
