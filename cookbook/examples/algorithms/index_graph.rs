//! RECIPE 04 (graphs): index-based graph, interning names once, iterative BFS.
//! LEVEL: intermediate
//! Use in: stages 5, 7, 8 (representation, no recursion, no String hashing in hot loops).
//! COMMON USES: road networks, dependency graphs, social graphs: intern names once, then run algorithms on dense integer ids.
//! Run: cargo run --example index_graph
//!
//! Domain is a transit map and the algorithm is BFS shortest-hops, NOT topological sort: study the
//! *shape* (intern -> dense ids -> Vec<Vec<id>> -> queue + visited vector) and apply it yourself.
//! Real code: petgraph -> https://github.com/petgraph/petgraph
#![allow(dead_code)]
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StopId(u32);

#[derive(Default)]
struct Network {
    names: Vec<String>,
    lookup: HashMap<String, StopId>,
    next: Vec<Vec<StopId>>, // adjacency list: next[a] = stops reachable from a
}

impl Network {
    /// Turn a string into a dense id exactly once; everything after uses integers.
    fn intern(&mut self, name: &str) -> StopId {
        if let Some(&id) = self.lookup.get(name) {
            return id;
        }
        let id = StopId(self.names.len() as u32);
        self.names.push(name.to_string());
        self.lookup.insert(name.to_string(), id);
        self.next.push(Vec::new());
        id
    }

    fn connect(&mut self, from: &str, to: &str) {
        let (a, b) = (self.intern(from), self.intern(to));
        self.next[a.0 as usize].push(b);
    }

    fn fewest_hops(&self, from: StopId, to: StopId) -> Option<Vec<StopId>> {
        let n = self.names.len();
        let mut seen = vec![false; n];
        let mut prev: Vec<Option<StopId>> = vec![None; n];
        let mut queue = VecDeque::new();
        seen[from.0 as usize] = true;
        queue.push_back(from);
        while let Some(cur) = queue.pop_front() {
            if cur == to {
                let mut path = vec![to];
                let mut at = to;
                while let Some(p) = prev[at.0 as usize] {
                    path.push(p);
                    at = p;
                }
                path.reverse();
                return Some(path);
            }
            for &nb in &self.next[cur.0 as usize] {
                if !seen[nb.0 as usize] {
                    seen[nb.0 as usize] = true;
                    prev[nb.0 as usize] = Some(cur);
                    queue.push_back(nb);
                }
            }
        }
        None
    }
}

fn main() {
    let mut net = Network::default();
    for (a, b) in [
        ("home", "bus"),
        ("bus", "metro"),
        ("home", "walk"),
        ("walk", "metro"),
        ("metro", "work"),
    ] {
        net.connect(a, b);
    }
    let (home, work) = (net.lookup["home"], net.lookup["work"]);
    let path = net.fewest_hops(home, work).unwrap();
    let names: Vec<&str> = path
        .iter()
        .map(|id| net.names[id.0 as usize].as_str())
        .collect();
    println!("{}", names.join(" -> "));
    println!("unreachable: {:?}", net.fewest_hops(work, home));
}
