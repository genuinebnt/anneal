use std::hash::Hash;

/// One thing one actor does to the shared state.
pub type Step<S> = fn(&mut S);

/// Runs `schedule`: each entry is the actor that takes its next step. Returns every state on the way, the start
/// first. `None` if the schedule names an actor that does not exist or has no step left.
pub fn replay<S: Clone>(start: &S, actors: &[Vec<Step<S>>], schedule: &[usize]) -> Option<Vec<S>> {
    todo!()
}

/// The shortest schedule that reaches a state for which `ok` is false; among the shortest, the first in
/// dictionary order. `None` if no schedule reaches such a state.
pub fn shortest_failure<S: Clone + Eq + Hash>(
    start: &S,
    actors: &[Vec<Step<S>>],
    ok: impl Fn(&S) -> bool,
) -> Option<Vec<usize>> {
    todo!()
}
