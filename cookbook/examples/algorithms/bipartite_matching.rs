//! RECIPE 70 (graphs): bipartite matching (Kuhn's augmenting paths): assign each worker to at most one job.
//! LEVEL: intermediate
//! Use in: worker-to-task assignment where each side takes at most one partner (resource-aware scheduling roadmap).   Run: cargo run --release --example bipartite_matching
//! COMMON USES: assigning workers to jobs, ride matching, staff rosters; flow generalises it.
#![allow(dead_code)]
//!
//! Try to match each left node. If its preferred right node is taken, ask the CURRENT owner to move to another option
//! (recursively): that chain is an "augmenting path". Each success grows the matching by one. O(V*E).
//! For big inputs use Hopcroft-Karp O(E sqrt V); for COSTS use the Hungarian algorithm / min-cost flow.
//! Read: https://cp-algorithms.com/graph/kuhn_maximum_bipartite_matching.html
fn try_assign(
    worker: usize,
    can_do: &[Vec<usize>],
    visited: &mut [bool],
    owner: &mut [Option<usize>],
) -> bool {
    for &job in &can_do[worker] {
        if visited[job] {
            continue;
        }
        visited[job] = true;
        // free job, or its owner can be moved elsewhere
        if owner[job].is_none() || try_assign(owner[job].unwrap(), can_do, visited, owner) {
            owner[job] = Some(worker);
            return true;
        }
    }
    false
}

fn max_matching(can_do: &[Vec<usize>], jobs: usize) -> (usize, Vec<Option<usize>>) {
    let mut owner = vec![None; jobs]; // owner[job] = worker
    let mut matched = 0;
    for w in 0..can_do.len() {
        let mut visited = vec![false; jobs];
        if try_assign(w, can_do, &mut visited, &mut owner) {
            matched += 1;
        }
    }
    (matched, owner)
}

fn main() {
    let workers = ["ana", "bo", "cy", "di"];
    let jobs = ["weld", "paint", "wire"];
    // who can do what
    let can_do = vec![vec![0, 1], vec![0], vec![1, 2], vec![0]];
    let (n, owner) = max_matching(&can_do, jobs.len());
    for (j, o) in owner.iter().enumerate() {
        println!("{:>6} -> {}", jobs[j], o.map_or("nobody", |w| workers[w]));
    }
    println!(
        "matched {n} of {} workers / {} jobs",
        workers.len(),
        jobs.len()
    );
    assert_eq!(n, 3); // everyone cannot be matched: 4 workers, 3 jobs; greedy would get stuck at 2
    // every assigned worker really is allowed to do that job, and no worker is used twice
    let mut used = vec![false; workers.len()];
    for (j, o) in owner.iter().enumerate() {
        if let Some(w) = o {
            assert!(can_do[*w].contains(&j) && !std::mem::replace(&mut used[*w], true));
        }
    }
}
