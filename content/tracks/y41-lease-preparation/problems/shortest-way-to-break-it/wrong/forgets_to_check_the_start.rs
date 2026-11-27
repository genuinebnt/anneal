use std::collections::{HashSet, VecDeque};
use std::hash::Hash;

/// One thing one actor does to the shared state.
pub type Step<S> = fn(&mut S);

/// Runs `schedule`: each entry is the actor that takes its next step. Returns every state on the way, the start
/// first. `None` if the schedule names an actor that does not exist or has no step left.
pub fn replay<S: Clone>(start: &S, actors: &[Vec<Step<S>>], schedule: &[usize]) -> Option<Vec<S>> {
    let mut done = vec![0; actors.len()];
    let mut states = vec![start.clone()];
    for &who in schedule {
        let step = actors.get(who)?.get(done[who])?;
        done[who] += 1;
        let mut next = states[states.len() - 1].clone();
        step(&mut next);
        states.push(next);
    }
    Some(states)
}

/// Where a run stands: how many steps each actor has taken, and the state.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Point<S> {
    done: Vec<usize>,
    state: S,
}

/// The shortest schedule that reaches a state for which `ok` is false; among the shortest, the first in
/// dictionary order. `None` if no schedule reaches such a state.
pub fn shortest_failure<S: Clone + Eq + Hash>(
    start: &S,
    actors: &[Vec<Step<S>>],
    ok: impl Fn(&S) -> bool,
) -> Option<Vec<usize>> {
    let first = Point {
        done: vec![0; actors.len()],
        state: start.clone(),
    };
    // Breadth first, actors in rising order: points are met shortest first, and in dictionary order among
    // equal lengths. So the first way to a point is the one worth keeping.
    let mut seen = HashSet::from([first.clone()]);
    let mut queue = VecDeque::from([(first, Vec::new())]);
    while let Some((point, schedule)) = queue.pop_front() {
        for (who, steps) in actors.iter().enumerate() {
            let Some(step) = steps.get(point.done[who]) else {
                continue;
            };
            let mut next = point.clone();
            next.done[who] += 1;
            step(&mut next.state);
            if seen.contains(&next) {
                continue;
            }
            let mut longer: Vec<usize> = schedule.clone();
            longer.push(who);
            if !ok(&next.state) {
                return Some(longer);
            }
            seen.insert(next.clone());
            queue.push_back((next, longer));
        }
    }
    None
}
