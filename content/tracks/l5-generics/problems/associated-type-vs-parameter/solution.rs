use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;

pub trait Graph {
    /// Each graph has one node type, so it's an associated type, not a parameter.
    type Node;

    fn neighbors(&self, node: &Self::Node) -> Vec<Self::Node>;
}

/// Nodes are 0..edges.len(); `edges[i]` lists the nodes `i` has an edge to. Edges are one-way.
pub struct AdjList {
    pub edges: Vec<Vec<usize>>,
}

/// Cells are `.` (open) or `#` (wall). Nodes are the (row, col) of open cells; moves go up, down, left or right.
pub struct Grid {
    cells: Vec<Vec<u8>>,
}

impl Graph for AdjList {
    type Node = usize;

    fn neighbors(&self, node: &usize) -> Vec<usize> {
        self.edges[*node].clone()
    }
}

impl Graph for Grid {
    type Node = (usize, usize);

    fn neighbors(&self, node: &(usize, usize)) -> Vec<(usize, usize)> {
        let (r, c) = *node;
        let mut out = Vec::new();
        for (dr, dc) in [(0, 1), (1, 0), (0, -1), (-1, 0)] {
            let (nr, nc) = (r as isize + dr, c as isize + dc);
            if nr < 0 || nc < 0 {
                continue;
            }
            let (nr, nc) = (nr as usize, nc as usize);
            if self.cells.get(nr).and_then(|row| row.get(nc)) == Some(&b'.') {
                out.push((nr, nc));
            }
        }
        out
    }
}

/// Rows separated by newlines.
impl From<&str> for Grid {
    fn from(text: &str) -> Self {
        Grid::from(text.lines().collect::<Vec<_>>())
    }
}

/// One string per row.
impl From<Vec<&str>> for Grid {
    fn from(rows: Vec<&str>) -> Self {
        Grid {
            cells: rows.iter().map(|r| r.as_bytes().to_vec()).collect(),
        }
    }
}

/// How many nodes can be reached from `start`, counting `start` itself.
pub fn reachable<G>(graph: &G, start: G::Node) -> usize
where
    G: Graph,
    G::Node: Eq + Hash + Clone,
{
    let mut seen: HashSet<G::Node> = HashSet::new();
    seen.insert(start.clone());
    let mut stack = vec![start];
    while let Some(node) = stack.pop() {
        for next in graph.neighbors(&node) {
            if !seen.contains(&next) {
                seen.insert(next.clone());
                stack.push(next);
            }
        }
    }
    seen.len()
}

/// The nodes on a shortest path from `from` to `to`, both included. None if `to` can't be reached.
pub fn shortest_path<G>(graph: &G, from: G::Node, to: G::Node) -> Option<Vec<G::Node>>
where
    G: Graph,
    G::Node: Eq + Hash + Clone,
{
    // BFS; `parent` doubles as the seen set. The start maps to itself.
    let mut parent: HashMap<G::Node, G::Node> = HashMap::new();
    parent.insert(from.clone(), from.clone());
    let mut queue = VecDeque::from([from.clone()]);
    while let Some(node) = queue.pop_front() {
        if node == to {
            let mut path = vec![node];
            while *path.last().unwrap() != from {
                let prev = parent[path.last().unwrap()].clone();
                path.push(prev);
            }
            path.reverse();
            return Some(path);
        }
        for next in graph.neighbors(&node) {
            if !parent.contains_key(&next) {
                parent.insert(next.clone(), node.clone());
                queue.push_back(next);
            }
        }
    }
    None
}
