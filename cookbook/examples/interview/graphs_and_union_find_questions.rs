//! RECIPE 622 (interview): graphs, grids, BFS/DFS, union-find, shortest paths and MST: the most-asked coding questions in Rust.
//! LEVEL: advanced
//! Use in: preparing for coding interviews; recognising the graph model (grid as graph, implicit state graph, union-find, Dijkstra) behind a question.   Run: cargo run --release --example graphs_and_union_find_questions
//! COMMON USES: phone screens and on-sites (NeetCode / Blind 75 / Grind 75 style), routing, connectivity, clustering, puzzle solvers.
#![allow(dead_code)]
//! Read: https://www.techinterviewhandbook.org/grind75 , https://neetcode.io/practice , https://interviewing.io/rust-interview-questions , https://cp-algorithms.com/
//!
//! OMITTED ON PURPOSE (these are the learner's own exercises elsewhere): topological sort / Kahn's algorithm, Course Schedule I/II,
//! Alien Dictionary, cycle detection in dependency graphs, critical path and longest path in a task DAG. (Cycle detection in
//! UNDIRECTED graphs appears only as union-find: Redundant Connection and Graph Valid Tree.)
//!
//! QUESTION BANK (numbering restarts in each interview file; a generator reads these lines)
//! Q1. Number of Islands (DFS with an explicit stack, BFS, union-find) and Max Area of Island. [Medium] [asked: very common]
//! Q2. Flood Fill. [Easy] [asked: common]
//! Q3. Clone Graph (arena indices and the `Rc<RefCell<..>>` version). [Medium] [asked: very common]
//! Q4. Rotting Oranges (multi-source BFS by levels). [Medium] [asked: very common]
//! Q5. 01 Matrix (BFS from all zeros) and Walls and Gates. [Medium] [asked: common]
//! Q6. Pacific Atlantic Water Flow (reverse search from the borders). [Medium] [asked: common]
//! Q7. Surrounded Regions. [Medium] [asked: common]
//! Q8. Word Ladder (BFS and bidirectional BFS). [Hard] [asked: very common]
//! Q9. Shortest Path in a Binary Matrix (8 directions). [Medium] [asked: common]
//! Q10. Open the Lock (BFS over an implicit state graph). [Medium] [asked: common]
//! Q11. Snakes and Ladders (BFS with board numbering). [Medium] [asked: common]
//! Q12. Shortest Bridge (DFS to find an island, then BFS outward). [Medium] [asked: occasional]
//! Q13. Is Graph Bipartite and Possible Bipartition (2-colouring). [Medium] [asked: common]
//! Q14. Number of Connected Components and Graph Valid Tree. [Medium] [asked: very common]
//! Q15. Redundant Connection (union-find). [Medium] [asked: very common]
//! Q16. Accounts Merge (union-find over strings). [Medium] [asked: common]
//! Q17. Evaluate Division (weighted union-find). [Medium] [asked: common]
//! Q18. Network Delay Time (Dijkstra). [Medium] [asked: very common]
//! Q19. Cheapest Flights Within K Stops (Bellman-Ford rounds). [Medium] [asked: very common]
//! Q20. Path With Minimum Effort and Swim in Rising Water (minimax Dijkstra). [Medium] [asked: common]
//! Q21. Min Cost to Connect All Points (Prim's MST). [Medium] [asked: common]
//! Q22. Critical Connections in a Network (bridges via low-link, iterative). [Hard] [asked: common]
//! Q23. Find the Town Judge and Find Center of Star Graph (degree tricks). [Easy] [asked: common]
//! Q24. Reconstruct Itinerary (Eulerian path, Hierholzer). [Hard] [asked: common]
//! Q25. Graph representations: adjacency list vs matrix vs edge list vs CSR, and the grid-as-graph neighbour helper with `checked_add_signed`. [Easy] [asked: very common]
//!
//! Every question: statement, approach (brute force then optimal, complexity, pattern), Rust wrinkle, solution(s), a brute-force oracle
//! with 2000 seeded xorshift cases plus edge cases, and what the interviewer is really testing.
//!
//! GRAPH CHECKLIST (say it first in the interview): is it directed? weighted (non-negative? negative?) ? connected? how big are V
//! and E? "shortest" with unit edges -> BFS; non-negative weights -> Dijkstra; at most k edges -> Bellman-Ford rounds; "are these
//! connected" with merges -> union-find; implicit states (locks, words, boards) -> BFS over generated neighbours.

use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet, VecDeque};
use std::rc::Rc;

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn int(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i + 1);
            v.swap(i, j);
        }
    }
}
const CASES: usize = 2000;

// ---------- shared helpers: grid-as-graph and union-find ----------
type Grid = Vec<Vec<u8>>;
const DIRS: [(isize, isize); 4] = [(0, 1), (1, 0), (0, -1), (-1, 0)];
const DIRS8: [(isize, isize); 8] = [
    (0, 1),
    (1, 0),
    (0, -1),
    (-1, 0),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];
fn dims<T>(g: &[Vec<T>]) -> (usize, usize) {
    (g.len(), g.first().map_or(0, |r| r.len()))
}
/// In-bounds neighbours of (r, c) without ever forming a negative `usize`: `checked_add_signed` returns `None` on underflow.
fn neighbors(
    r: usize,
    c: usize,
    rows: usize,
    cols: usize,
    dirs: &'static [(isize, isize)],
) -> impl Iterator<Item = (usize, usize)> {
    dirs.iter().filter_map(move |&(dr, dc)| {
        let (nr, nc) = (r.checked_add_signed(dr)?, c.checked_add_signed(dc)?);
        (nr < rows && nc < cols).then_some((nr, nc))
    })
}
fn n4(r: usize, c: usize, rows: usize, cols: usize) -> impl Iterator<Item = (usize, usize)> {
    neighbors(r, c, rows, cols, &DIRS)
}
fn rand_grid(rng: &mut Rng, max_dim: usize, ones_in: usize) -> Grid {
    let (r, c) = (1 + rng.below(max_dim), 1 + rng.below(max_dim));
    (0..r)
        .map(|_| (0..c).map(|_| u8::from(rng.below(ones_in) == 0)).collect())
        .collect()
}

struct Uf {
    p: Vec<usize>,
    size: Vec<usize>,
    comps: usize,
}
impl Uf {
    fn new(n: usize) -> Self {
        Uf {
            p: (0..n).collect(),
            size: vec![1; n],
            comps: n,
        }
    }
    fn find(&mut self, mut x: usize) -> usize {
        while self.p[x] != x {
            self.p[x] = self.p[self.p[x]]; // path halving
            x = self.p[x];
        }
        x
    }
    /// Returns false when a and b were already connected (that edge closes a cycle).
    fn union(&mut self, a: usize, b: usize) -> bool {
        let (mut a, mut b) = (self.find(a), self.find(b));
        if a == b {
            return false;
        }
        if self.size[a] < self.size[b] {
            std::mem::swap(&mut a, &mut b);
        }
        self.p[b] = a;
        self.size[a] += self.size[b];
        self.comps -= 1;
        true
    }
}

// ================================================================================================
/// Q1. NUMBER OF ISLANDS (count 4-connected groups of 1s in a grid) and MAX AREA OF ISLAND.
///   [[1,1,0],[0,1,0],[0,0,1]] -> 2 islands, max area 3
///
/// APPROACH. Treat each cell as a node and 4 neighbours as edges. Scan the grid; each unvisited land cell starts a new island:
/// flood it (DFS or BFS) so it is never counted again. O(mn) time and space. Three equivalent engines: DFS with an EXPLICIT STACK
/// (recursion can overflow on a 1000 x 1000 all-land grid), BFS with a `VecDeque`, and UNION-FIND (islands = land cells minus
/// successful unions; the right choice when land is added over time: Number of Islands II). Max area = largest flood size.
/// RUST. Do not recurse on big grids: Rust has no tail-call guarantee and the default main-thread stack is ~8 MB. Mark visited
/// when you PUSH (not when you pop) or cells enter the stack many times. Borrowing: reading `g` while mutating `seen` is fine since
/// they are separate allocations; mutating the input grid to mark cells needs `&mut` and loses the caller's data.
/// INTERVIEWER IS TESTING: visited-at-push, bounds without underflow, and whether you can swap DFS/BFS/union-find on demand.
fn islands_dfs(g: &Grid) -> usize {
    let (rows, cols) = dims(g);
    let mut seen = vec![vec![false; cols]; rows];
    let mut count = 0;
    for r in 0..rows {
        for c in 0..cols {
            if g[r][c] == 1 && !seen[r][c] {
                count += 1;
                seen[r][c] = true;
                let mut stack = vec![(r, c)];
                while let Some((cr, cc)) = stack.pop() {
                    for (nr, nc) in n4(cr, cc, rows, cols) {
                        if g[nr][nc] == 1 && !seen[nr][nc] {
                            seen[nr][nc] = true;
                            stack.push((nr, nc));
                        }
                    }
                }
            }
        }
    }
    count
}
fn islands_bfs(g: &Grid) -> usize {
    let (rows, cols) = dims(g);
    let mut seen = vec![vec![false; cols]; rows];
    let mut count = 0;
    for r in 0..rows {
        for c in 0..cols {
            if g[r][c] == 1 && !seen[r][c] {
                count += 1;
                seen[r][c] = true;
                let mut q = VecDeque::from([(r, c)]);
                while let Some((cr, cc)) = q.pop_front() {
                    for (nr, nc) in n4(cr, cc, rows, cols) {
                        if g[nr][nc] == 1 && !seen[nr][nc] {
                            seen[nr][nc] = true;
                            q.push_back((nr, nc));
                        }
                    }
                }
            }
        }
    }
    count
}
fn islands_uf(g: &Grid) -> usize {
    let (rows, cols) = dims(g);
    let mut uf = Uf::new(rows * cols);
    let mut land = 0;
    let mut merges = 0;
    for r in 0..rows {
        for c in 0..cols {
            if g[r][c] == 1 {
                land += 1;
                // only look right and down so each edge is considered once
                for (nr, nc) in [(r + 1, c), (r, c + 1)] {
                    if nr < rows
                        && nc < cols
                        && g[nr][nc] == 1
                        && uf.union(r * cols + c, nr * cols + nc)
                    {
                        merges += 1;
                    }
                }
            }
        }
    }
    land - merges
}
fn max_area_of_island(g: &Grid) -> usize {
    let (rows, cols) = dims(g);
    let mut seen = vec![vec![false; cols]; rows];
    let mut best = 0;
    for r in 0..rows {
        for c in 0..cols {
            if g[r][c] == 1 && !seen[r][c] {
                seen[r][c] = true;
                let (mut stack, mut area) = (vec![(r, c)], 0);
                while let Some((cr, cc)) = stack.pop() {
                    area += 1;
                    for (nr, nc) in n4(cr, cc, rows, cols) {
                        if g[nr][nc] == 1 && !seen[nr][nc] {
                            seen[nr][nc] = true;
                            stack.push((nr, nc));
                        }
                    }
                }
                best = best.max(area);
            }
        }
    }
    best
}
fn sink_rec(g: &mut Grid, r: usize, c: usize) -> usize {
    if g[r][c] != 1 {
        return 0;
    }
    g[r][c] = 2;
    let (rows, cols) = dims(g);
    1 + n4(r, c, rows, cols)
        .collect::<Vec<_>>()
        .into_iter()
        .map(|(nr, nc)| sink_rec(g, nr, nc))
        .sum::<usize>()
}
fn q1() {
    let mut rng = Rng::new(1);
    for _ in 0..CASES {
        let g = rand_grid(&mut rng, 7, 2);
        let mut copy = g.clone();
        let (rows, cols) = dims(&g);
        let (mut want, mut want_area) = (0, 0);
        for r in 0..rows {
            for c in 0..cols {
                let a = sink_rec(&mut copy, r, c); // recursive oracle
                if a > 0 {
                    want += 1;
                    want_area = want_area.max(a);
                }
            }
        }
        assert_eq!(
            (islands_dfs(&g), islands_bfs(&g), islands_uf(&g)),
            (want, want, want),
            "{g:?}"
        );
        assert_eq!(max_area_of_island(&g), want_area, "{g:?}");
    }
    assert_eq!(
        islands_dfs(&vec![vec![1, 1, 0], vec![0, 1, 0], vec![0, 0, 1]]),
        2
    );
    assert_eq!(islands_uf(&vec![]), 0);
}

// ================================================================================================
/// Q2. FLOOD FILL. Starting at (sr, sc) recolour the 4-connected region of the same colour to `color` (paint-bucket tool).
///   [[1,1,1],[1,1,0],[1,0,1]], (1,1), 2 -> [[2,2,2],[2,2,0],[2,0,1]]
///
/// APPROACH. Graph search from the start over cells equal to the ORIGINAL colour: O(mn). The famous trap: if the new colour equals the
/// old one, a "visited = colour changed" scheme loops forever; return early. Using the recolour itself as the visited mark
/// keeps O(1) extra space apart from the stack.
/// RUST. `image: &mut Vec<Vec<i32>>` is modified in place; capture `old = image[sr][sc]` first because the cell changes
/// during the search. Explicit stack again to avoid deep recursion.
/// INTERVIEWER IS TESTING: the same-colour infinite loop, empty images, and out-of-range start.
fn flood_fill(image: &mut Vec<Vec<i32>>, sr: usize, sc: usize, color: i32) {
    let (rows, cols) = dims(image);
    if sr >= rows || sc >= cols || image[sr][sc] == color {
        return;
    }
    let old = image[sr][sc];
    image[sr][sc] = color;
    let mut stack = vec![(sr, sc)];
    while let Some((r, c)) = stack.pop() {
        for (nr, nc) in n4(r, c, rows, cols) {
            if image[nr][nc] == old {
                image[nr][nc] = color;
                stack.push((nr, nc));
            }
        }
    }
}
fn flood_rec(image: &mut Vec<Vec<i32>>, r: usize, c: usize, old: i32, color: i32) {
    if image[r][c] != old {
        return;
    }
    image[r][c] = color;
    let (rows, cols) = dims(image);
    for (nr, nc) in n4(r, c, rows, cols).collect::<Vec<_>>() {
        flood_rec(image, nr, nc, old, color);
    }
}
fn q2() {
    let mut rng = Rng::new(2);
    for _ in 0..CASES {
        let (rows, cols) = (1 + rng.below(6), 1 + rng.below(6));
        let img: Vec<Vec<i32>> = (0..rows)
            .map(|_| (0..cols).map(|_| rng.below(3) as i32).collect())
            .collect();
        let (sr, sc, color) = (rng.below(rows), rng.below(cols), rng.below(4) as i32);
        let mut a = img.clone();
        flood_fill(&mut a, sr, sc, color);
        let mut b = img.clone();
        if img[sr][sc] != color {
            let old = img[sr][sc];
            flood_rec(&mut b, sr, sc, old, color);
        }
        assert_eq!(a, b, "{img:?}");
    }
    let mut img = vec![vec![1, 1, 1], vec![1, 1, 0], vec![1, 0, 1]];
    flood_fill(&mut img, 1, 1, 2);
    assert_eq!(img, vec![vec![2, 2, 2], vec![2, 2, 0], vec![2, 0, 1]]);
}

// ================================================================================================
/// Q3. CLONE GRAPH. Deep-copy a connected undirected graph given a reference to one node (each node: a value and a list of neighbours).
///   adjacency [[2,4],[1,3],[2,4],[1,3]] -> an independent copy with the same shape
///
/// APPROACH. Traverse (BFS/DFS) and keep a MAP from original node -> its clone. On first sight create the clone and enqueue the
/// original; for every edge look up (or create) the neighbour's clone and link it. The map doubles as the visited set and
/// is what makes cycles terminate. O(V + E) time and space.
/// RUST. Two idioms. ARENA: nodes are indices into `Vec<Vec<usize>>`; cloning is allocating a new arena and a `Vec<Option<usize>>`
/// map: no lifetimes, no cycles, the idiomatic Rust answer. POINTER style: `Rc<Node>` with `RefCell<Vec<Rc<Node>>>` neighbours
/// (shown below, keyed by node value): `Rc` cycles are never freed (leak); real code would use `Weak` for back edges or
/// an arena. Mention this unprompted.
/// INTERVIEWER IS TESTING: using the map as the visited set, not cloning twice, and your ownership story for cycles.
fn clone_arena(adj: &[Vec<usize>], start: usize) -> (Vec<Vec<usize>>, Vec<Option<usize>>) {
    let mut map: Vec<Option<usize>> = vec![None; adj.len()];
    let mut out: Vec<Vec<usize>> = vec![];
    map[start] = Some(0);
    out.push(vec![]);
    let mut q = VecDeque::from([start]);
    while let Some(u) = q.pop_front() {
        let cu = map[u].unwrap();
        for &v in &adj[u] {
            let cv = match map[v] {
                Some(x) => x,
                None => {
                    map[v] = Some(out.len());
                    out.push(vec![]);
                    q.push_back(v);
                    out.len() - 1
                }
            };
            out[cu].push(cv);
        }
    }
    (out, map)
}
struct RcNode {
    val: i32,
    nbrs: RefCell<Vec<Rc<RcNode>>>,
}
fn clone_rc(start: &Rc<RcNode>) -> Rc<RcNode> {
    let mut map: HashMap<i32, Rc<RcNode>> = HashMap::new();
    let root = Rc::new(RcNode {
        val: start.val,
        nbrs: RefCell::new(vec![]),
    });
    map.insert(start.val, root.clone());
    let mut q = VecDeque::from([start.clone()]);
    while let Some(u) = q.pop_front() {
        let cu = map[&u.val].clone();
        for v in u.nbrs.borrow().iter() {
            let cv = match map.get(&v.val) {
                Some(c) => c.clone(),
                None => {
                    let c = Rc::new(RcNode {
                        val: v.val,
                        nbrs: RefCell::new(vec![]),
                    });
                    map.insert(v.val, c.clone());
                    q.push_back(v.clone());
                    c
                }
            };
            cu.nbrs.borrow_mut().push(cv);
        }
    }
    root
}
fn q3() {
    let mut rng = Rng::new(3);
    for _ in 0..CASES {
        let n = 1 + rng.below(8);
        let mut adj = vec![vec![]; n];
        for _ in 0..rng.below(14) {
            let (a, b) = (rng.below(n), rng.below(n));
            adj[a].push(b);
            adj[b].push(a);
        }
        let start = rng.below(n);
        let (clone, map) = clone_arena(&adj, start);
        // reachable set from start
        let mut seen = vec![false; n];
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(u) = stack.pop() {
            for &v in &adj[u] {
                if !seen[v] {
                    seen[v] = true;
                    stack.push(v);
                }
            }
        }
        assert_eq!(clone.len(), seen.iter().filter(|&&s| s).count());
        for u in 0..n {
            assert_eq!(map[u].is_some(), seen[u]);
            if let Some(cu) = map[u] {
                let mapped: Vec<usize> = adj[u].iter().map(|&v| map[v].unwrap()).collect();
                assert_eq!(
                    clone[cu], mapped,
                    "neighbour lists must correspond under the map"
                );
            }
        }
    }
    // Rc version on the 4-cycle 1-2-3-4
    let nodes: Vec<Rc<RcNode>> = (1..=4)
        .map(|v| {
            Rc::new(RcNode {
                val: v,
                nbrs: RefCell::new(vec![]),
            })
        })
        .collect();
    for (a, b) in [(0, 1), (1, 2), (2, 3), (3, 0)] {
        nodes[a].nbrs.borrow_mut().push(nodes[b].clone());
        nodes[b].nbrs.borrow_mut().push(nodes[a].clone());
    }
    let copy = clone_rc(&nodes[0]);
    assert!(!Rc::ptr_eq(&copy, &nodes[0]));
    let vals: Vec<i32> = copy.nbrs.borrow().iter().map(|n| n.val).collect();
    assert_eq!(vals, vec![2, 4]);
    assert!(
        copy.nbrs
            .borrow()
            .iter()
            .all(|n| !nodes.iter().any(|o| Rc::ptr_eq(o, n)))
    );
    // (the Rc cycles above are leaked at exit: demonstrates why real code prefers arenas or Weak back-references)
}

// ================================================================================================
/// Q4. ROTTING ORANGES. Grid: 0 empty, 1 fresh, 2 rotten. Each minute rotten oranges rot their 4-neighbours. Minutes until none is
/// fresh, or -1 if impossible.   [[2,1,1],[1,1,0],[0,1,1]] -> 4;   [[2,1,1],[0,1,1],[1,0,1]] -> -1
///
/// APPROACH. Simulating minute by minute is O(mn * time). MULTI-SOURCE BFS: enqueue ALL rotten oranges at time 0 and process in LEVELS
/// (`for _ in 0..queue.len()`); each level is one minute. Count fresh oranges up front and decrement when rotted; at the end any
/// fresh left means unreachable. O(mn). The same shape solves "distance to the nearest source" questions (Q5).
/// RUST. Capture `queue.len()` before the inner loop (it changes while you push). Return `Option<usize>` instead of a -1 magic number.
/// Do not add a minute for the last, empty level.
/// INTERVIEWER IS TESTING: multi-source BFS and the off-by-one on the final level.
fn rotting_oranges(g: &Grid) -> Option<usize> {
    let (rows, cols) = dims(g);
    let mut grid = g.clone();
    let mut q = VecDeque::new();
    let mut fresh = 0;
    for r in 0..rows {
        for c in 0..cols {
            match grid[r][c] {
                2 => q.push_back((r, c)),
                1 => fresh += 1,
                _ => {}
            }
        }
    }
    let mut minutes = 0;
    while fresh > 0 && !q.is_empty() {
        for _ in 0..q.len() {
            let (r, c) = q.pop_front().unwrap();
            for (nr, nc) in n4(r, c, rows, cols) {
                if grid[nr][nc] == 1 {
                    grid[nr][nc] = 2;
                    fresh -= 1;
                    q.push_back((nr, nc));
                }
            }
        }
        minutes += 1;
    }
    (fresh == 0).then_some(minutes)
}
fn rotting_oracle(g: &Grid) -> Option<usize> {
    let (rows, cols) = dims(g);
    let mut grid = g.clone();
    let mut minutes = 0;
    loop {
        let mut next = grid.clone();
        let mut changed = false;
        for r in 0..rows {
            for c in 0..cols {
                if grid[r][c] == 1 && n4(r, c, rows, cols).any(|(nr, nc)| grid[nr][nc] == 2) {
                    next[r][c] = 2;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
        grid = next;
        minutes += 1;
    }
    grid.iter()
        .all(|row| row.iter().all(|&x| x != 1))
        .then_some(minutes)
}
fn q4() {
    let mut rng = Rng::new(4);
    for _ in 0..CASES {
        let (rows, cols) = (1 + rng.below(6), 1 + rng.below(6));
        let g: Grid = (0..rows)
            .map(|_| (0..cols).map(|_| [0, 1, 1, 1, 2][rng.below(5)]).collect())
            .collect();
        assert_eq!(rotting_oranges(&g), rotting_oracle(&g), "{g:?}");
    }
    assert_eq!(
        rotting_oranges(&vec![vec![2, 1, 1], vec![1, 1, 0], vec![0, 1, 1]]),
        Some(4)
    );
    assert_eq!(
        rotting_oranges(&vec![vec![2, 1, 1], vec![0, 1, 1], vec![1, 0, 1]]),
        None
    );
    assert_eq!(rotting_oranges(&vec![vec![0]]), Some(0));
}

// ================================================================================================
/// Q5. 01 MATRIX (distance of each cell to the nearest 0) and WALLS AND GATES (fill each empty room with the distance to its
/// nearest gate; walls block).
///   [[0,0,0],[0,1,0],[1,1,1]] -> [[0,0,0],[0,1,0],[1,2,1]];   rooms use INF = 2147483647, wall -1, gate 0
///
/// APPROACH. Brute force: BFS from every cell, O((mn)^2). Correct trick: REVERSE THE SEARCH. Seed one BFS queue with ALL zeros (gates)
/// at distance 0 and expand outward; the first time a cell is reached is its shortest distance. O(mn). Walls and Gates is the
/// same with walls as obstacles and unreachable rooms left at INF. (Alternative for 01 Matrix: two DP sweeps, top-left then bottom-right.)
/// RUST. Use `usize::MAX` / `i32::MAX` as INF and never add to it. Use the distance matrix itself as visited (INF = unvisited).
/// INTERVIEWER IS TESTING: do you flip to multi-source BFS instead of searching from each cell.
fn zero_one_matrix(g: &Grid) -> Vec<Vec<usize>> {
    let (rows, cols) = dims(g);
    let mut dist = vec![vec![usize::MAX; cols]; rows];
    let mut q = VecDeque::new();
    for r in 0..rows {
        for c in 0..cols {
            if g[r][c] == 0 {
                dist[r][c] = 0;
                q.push_back((r, c));
            }
        }
    }
    while let Some((r, c)) = q.pop_front() {
        for (nr, nc) in n4(r, c, rows, cols) {
            if dist[nr][nc] == usize::MAX {
                dist[nr][nc] = dist[r][c] + 1;
                q.push_back((nr, nc));
            }
        }
    }
    dist
}
const ROOM_INF: i32 = i32::MAX;
fn walls_and_gates(rooms: &mut Vec<Vec<i32>>) {
    let (rows, cols) = dims(rooms);
    let mut q: VecDeque<(usize, usize)> = (0..rows)
        .flat_map(|r| (0..cols).map(move |c| (r, c)))
        .filter(|&(r, c)| rooms[r][c] == 0)
        .collect();
    while let Some((r, c)) = q.pop_front() {
        for (nr, nc) in n4(r, c, rows, cols) {
            if rooms[nr][nc] == ROOM_INF {
                rooms[nr][nc] = rooms[r][c] + 1;
                q.push_back((nr, nc));
            }
        }
    }
}
fn q5() {
    let mut rng = Rng::new(5);
    for _ in 0..CASES {
        let g = rand_grid(&mut rng, 6, 2);
        let (rows, cols) = dims(&g);
        let got = zero_one_matrix(&g);
        for r in 0..rows {
            for c in 0..cols {
                // no obstacles: BFS distance equals the Manhattan distance to the nearest zero
                let want = (0..rows)
                    .flat_map(|i| (0..cols).map(move |j| (i, j)))
                    .filter(|&(i, j)| g[i][j] == 0)
                    .map(|(i, j)| i.abs_diff(r) + j.abs_diff(c))
                    .min()
                    .unwrap_or(usize::MAX);
                assert_eq!(got[r][c], want, "{g:?}");
            }
        }
        let rooms0: Vec<Vec<i32>> = (0..rows)
            .map(|_| {
                (0..cols)
                    .map(|_| [ROOM_INF, ROOM_INF, ROOM_INF, -1, 0][rng.below(5)])
                    .collect()
            })
            .collect();
        let mut rooms = rooms0.clone();
        walls_and_gates(&mut rooms);
        let mut oracle = rooms0.clone(); // relax to a fixpoint (Bellman-Ford style)
        loop {
            let mut changed = false;
            for r in 0..rows {
                for c in 0..cols {
                    if oracle[r][c] == -1 {
                        continue;
                    }
                    for (nr, nc) in n4(r, c, rows, cols) {
                        if oracle[nr][nc] != -1
                            && oracle[nr][nc] != ROOM_INF
                            && oracle[nr][nc] + 1 < oracle[r][c]
                        {
                            oracle[r][c] = oracle[nr][nc] + 1;
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        assert_eq!(rooms, oracle, "{rooms0:?}");
    }
    assert_eq!(
        zero_one_matrix(&vec![vec![0, 0, 0], vec![0, 1, 0], vec![1, 1, 1]]),
        vec![vec![0, 0, 0], vec![0, 1, 0], vec![1, 2, 1]]
    );
}

// ================================================================================================
/// Q6. PACIFIC ATLANTIC WATER FLOW. Heights grid; water flows to 4-neighbours with height <= current. The Pacific touches the top and left
/// edges, the Atlantic the bottom and right. List the cells from which water can reach BOTH oceans.
///   [[1,2,2,3,5],[3,2,3,4,4],[2,4,5,3,1],[6,7,1,4,5],[5,1,1,2,4]] -> 7 cells
///
/// APPROACH. Brute force: from each cell search downhill to see which borders it reaches, O((mn)^2). Reverse it: start a BFS from
/// each ocean's border cells and move UPHILL (to neighbours with height >= current); the cells reached are exactly those that drain to that
/// ocean. Answer = intersection of the two reached sets. O(mn).
/// RUST. Return a `Vec<(usize, usize)>` in row-major order so tests are deterministic. A small closure/function that takes the
/// start cells keeps the two searches identical.
/// INTERVIEWER IS TESTING: reversing the direction of the search, and corner cells belonging to both borders.
fn reach(h: &[Vec<u8>], starts: Vec<(usize, usize)>) -> Vec<Vec<bool>> {
    let (rows, cols) = dims(h);
    let mut seen = vec![vec![false; cols]; rows];
    let mut q = VecDeque::new();
    for (r, c) in starts {
        if !seen[r][c] {
            seen[r][c] = true;
            q.push_back((r, c));
        }
    }
    while let Some((r, c)) = q.pop_front() {
        for (nr, nc) in n4(r, c, rows, cols) {
            if !seen[nr][nc] && h[nr][nc] >= h[r][c] {
                seen[nr][nc] = true;
                q.push_back((nr, nc));
            }
        }
    }
    seen
}
fn pacific_atlantic(h: &[Vec<u8>]) -> Vec<(usize, usize)> {
    let (rows, cols) = dims(h);
    if rows == 0 {
        return vec![];
    }
    let pac: Vec<_> = (0..rows)
        .map(|r| (r, 0))
        .chain((0..cols).map(|c| (0, c)))
        .collect();
    let atl: Vec<_> = (0..rows)
        .map(|r| (r, cols - 1))
        .chain((0..cols).map(|c| (rows - 1, c)))
        .collect();
    let (p, a) = (reach(h, pac), reach(h, atl));
    (0..rows)
        .flat_map(|r| (0..cols).map(move |c| (r, c)))
        .filter(|&(r, c)| p[r][c] && a[r][c])
        .collect()
}
fn flows_to_borders(h: &[Vec<u8>], r0: usize, c0: usize) -> (bool, bool) {
    let (rows, cols) = dims(h);
    let mut seen = vec![vec![false; cols]; rows];
    let mut stack = vec![(r0, c0)];
    seen[r0][c0] = true;
    let (mut pac, mut atl) = (false, false);
    while let Some((r, c)) = stack.pop() {
        pac |= r == 0 || c == 0;
        atl |= r == rows - 1 || c == cols - 1;
        for (nr, nc) in n4(r, c, rows, cols) {
            if !seen[nr][nc] && h[nr][nc] <= h[r][c] {
                seen[nr][nc] = true;
                stack.push((nr, nc));
            }
        }
    }
    (pac, atl)
}
fn q6() {
    let mut rng = Rng::new(6);
    for _ in 0..CASES {
        let (rows, cols) = (1 + rng.below(5), 1 + rng.below(5));
        let h: Vec<Vec<u8>> = (0..rows)
            .map(|_| (0..cols).map(|_| rng.below(5) as u8).collect())
            .collect();
        let want: Vec<(usize, usize)> = (0..rows)
            .flat_map(|r| (0..cols).map(move |c| (r, c)))
            .filter(|&(r, c)| flows_to_borders(&h, r, c) == (true, true))
            .collect();
        assert_eq!(pacific_atlantic(&h), want, "{h:?}");
    }
    let h: Vec<Vec<u8>> = vec![
        vec![1, 2, 2, 3, 5],
        vec![3, 2, 3, 4, 4],
        vec![2, 4, 5, 3, 1],
        vec![6, 7, 1, 4, 5],
        vec![5, 1, 1, 2, 4],
    ];
    assert_eq!(pacific_atlantic(&h).len(), 7);
}

// ================================================================================================
/// Q7. SURROUNDED REGIONS. Flip every region of 'O' that is completely surrounded by 'X' (not connected to a border) into 'X'.
///   XXXX/XOOX/XXOX/XOXX -> XXXX/XXXX/XXXX/XOXX
///
/// APPROACH. Searching each region and asking "does it touch the border" works but is clumsy. Invert: BFS from every BORDER 'O' and mark
/// everything reachable as SAFE; then flip every unsafe 'O' to 'X'. O(mn) time, O(mn) space (or reuse the board by marking safe cells
/// with a temporary letter).
/// RUST. A `safe` bool matrix keeps the input untouched until the final pass. Seed the queue by iterating the 4 border lines;
/// 1-row or 1-column boards make every cell a border cell.
/// INTERVIEWER IS TESTING: the "mark the survivors" inversion and tiny boards.
fn surrounded_regions(b: &mut Grid) {
    let (rows, cols) = dims(b);
    let mut safe = vec![vec![false; cols]; rows];
    let mut q = VecDeque::new();
    for r in 0..rows {
        for c in 0..cols {
            if (r == 0 || c == 0 || r == rows - 1 || c == cols - 1) && b[r][c] == b'O' {
                safe[r][c] = true;
                q.push_back((r, c));
            }
        }
    }
    while let Some((r, c)) = q.pop_front() {
        for (nr, nc) in n4(r, c, rows, cols) {
            if b[nr][nc] == b'O' && !safe[nr][nc] {
                safe[nr][nc] = true;
                q.push_back((nr, nc));
            }
        }
    }
    for r in 0..rows {
        for c in 0..cols {
            if b[r][c] == b'O' && !safe[r][c] {
                b[r][c] = b'X';
            }
        }
    }
}
fn surrounded_oracle(b: &Grid) -> Grid {
    let (rows, cols) = dims(b);
    let mut out = b.clone();
    let mut done = vec![vec![false; cols]; rows];
    for r in 0..rows {
        for c in 0..cols {
            if b[r][c] == b'O' && !done[r][c] {
                let (mut stack, mut cells, mut touches) = (vec![(r, c)], vec![], false);
                done[r][c] = true;
                while let Some((cr, cc)) = stack.pop() {
                    cells.push((cr, cc));
                    touches |= cr == 0 || cc == 0 || cr == rows - 1 || cc == cols - 1;
                    for (nr, nc) in n4(cr, cc, rows, cols) {
                        if b[nr][nc] == b'O' && !done[nr][nc] {
                            done[nr][nc] = true;
                            stack.push((nr, nc));
                        }
                    }
                }
                if !touches {
                    cells.iter().for_each(|&(i, j)| out[i][j] = b'X');
                }
            }
        }
    }
    out
}
fn q7() {
    let mut rng = Rng::new(7);
    for _ in 0..CASES {
        let (rows, cols) = (1 + rng.below(6), 1 + rng.below(6));
        let b: Grid = (0..rows)
            .map(|_| (0..cols).map(|_| b"XOO"[rng.below(3)]).collect())
            .collect();
        let mut got = b.clone();
        surrounded_regions(&mut got);
        assert_eq!(got, surrounded_oracle(&b), "{b:?}");
    }
    let mut b: Grid = ["XXXX", "XOOX", "XXOX", "XOXX"]
        .iter()
        .map(|s| s.bytes().collect())
        .collect();
    surrounded_regions(&mut b);
    let rows: Vec<String> = b
        .iter()
        .map(|r| String::from_utf8(r.clone()).unwrap())
        .collect();
    assert_eq!(rows, vec!["XXXX", "XXXX", "XXXX", "XOXX"]);
}

// ================================================================================================
/// Q8. WORD LADDER. Shortest transformation from `begin` to `end` changing one letter at a time, every intermediate in the word list.
/// Return the NUMBER OF WORDS in the shortest sequence (0 if none).
///   hit -> cog, ["hot","dot","dog","lot","log","cog"] -> 5 (hit,hot,dot,dog,cog)
///
/// APPROACH. Words are nodes, one-letter difference is an edge; unit edges so BFS gives the shortest path. Comparing all pairs
/// is O(N^2 L). Faster: bucket words by WILDCARD PATTERN (`h*t`, `*ot`, `ho*`): words sharing a pattern are neighbours, so each
/// word touches only L buckets: O(N L^2). BIDIRECTIONAL BFS expands the smaller of two frontiers (one from each end) and stops when
/// they meet: the effective branching factor drops dramatically, from b^d to 2 b^(d/2).
/// RUST. Hold `&str` slices (lifetime `'a` tying begin, end and the list together) instead of cloning Strings; patterns are the only owned
/// strings. `HashSet<&str>` frontiers; swap the two with `std::mem::swap`. Remember the begin word need not be in the list but the end word must.
/// INTERVIEWER IS TESTING: pattern buckets, correct level counting, and bidirectional BFS as the follow-up.
fn patterns(w: &str) -> Vec<String> {
    (0..w.len())
        .map(|i| format!("{}*{}", &w[..i], &w[i + 1..]))
        .collect()
}
fn word_buckets<'a>(begin: &'a str, words: &[&'a str]) -> HashMap<String, Vec<&'a str>> {
    let mut b: HashMap<String, Vec<&'a str>> = HashMap::new();
    for &w in words.iter().chain(std::iter::once(&begin)) {
        for p in patterns(w) {
            b.entry(p).or_default().push(w);
        }
    }
    b
}
fn ladder_bfs<'a>(begin: &'a str, end: &'a str, words: &[&'a str]) -> usize {
    if !words.contains(&end) {
        return 0;
    }
    let buckets = word_buckets(begin, words);
    let mut seen: HashSet<&str> = HashSet::from([begin]);
    let mut q = VecDeque::from([(begin, 1usize)]);
    while let Some((w, d)) = q.pop_front() {
        if w == end {
            return d;
        }
        for p in patterns(w) {
            for &nb in buckets.get(&p).into_iter().flatten() {
                if seen.insert(nb) {
                    q.push_back((nb, d + 1));
                }
            }
        }
    }
    0
}
fn ladder_bidir<'a>(begin: &'a str, end: &'a str, words: &[&'a str]) -> usize {
    if !words.contains(&end) {
        return 0;
    }
    let buckets = word_buckets(begin, words);
    let (mut front, mut back): (HashSet<&str>, HashSet<&str>) =
        (HashSet::from([begin]), HashSet::from([end]));
    let mut seen: HashSet<&str> = HashSet::from([begin, end]);
    let mut steps = 1;
    while !front.is_empty() && !back.is_empty() {
        if front.len() > back.len() {
            std::mem::swap(&mut front, &mut back);
        }
        let mut next = HashSet::new();
        for &w in &front {
            for p in patterns(w) {
                for &nb in buckets.get(&p).into_iter().flatten() {
                    if back.contains(nb) {
                        return steps + 1;
                    }
                    if seen.insert(nb) {
                        next.insert(nb);
                    }
                }
            }
        }
        front = next;
        steps += 1;
    }
    0
}
fn ladder_oracle(begin: &str, end: &str, words: &[&str]) -> usize {
    let mut nodes: Vec<&str> = vec![begin];
    for w in words {
        if !nodes.contains(w) {
            nodes.push(w);
        }
    }
    let Some(target) = nodes.iter().position(|&w| w == end) else {
        return 0;
    };
    let diff1 = |a: &str, b: &str| a.bytes().zip(b.bytes()).filter(|(x, y)| x != y).count() == 1;
    let mut dist = vec![usize::MAX; nodes.len()];
    dist[0] = 1;
    loop {
        let mut changed = false;
        for i in 0..nodes.len() {
            for j in 0..nodes.len() {
                if dist[i] != usize::MAX && diff1(nodes[i], nodes[j]) && dist[i] + 1 < dist[j] {
                    dist[j] = dist[i] + 1;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    if dist[target] == usize::MAX {
        0
    } else {
        dist[target]
    }
}
fn q8() {
    let mut rng = Rng::new(8);
    let all: Vec<String> = (0..27)
        .map(|i| {
            [i / 9, (i / 3) % 3, i % 3]
                .iter()
                .map(|&d| (b'a' + d as u8) as char)
                .collect()
        })
        .collect();
    for _ in 0..CASES {
        let (b, e) = (rng.below(27), rng.below(27));
        if b == e {
            continue;
        }
        let mut list: Vec<&str> = all
            .iter()
            .map(String::as_str)
            .filter(|_| rng.below(3) != 0)
            .collect();
        rng.shuffle(&mut list);
        let (begin, end) = (all[b].as_str(), all[e].as_str());
        let want = ladder_oracle(begin, end, &list);
        assert_eq!(
            ladder_bfs(begin, end, &list),
            want,
            "{begin} {end} {list:?}"
        );
        assert_eq!(
            ladder_bidir(begin, end, &list),
            want,
            "{begin} {end} {list:?}"
        );
    }
    let list = ["hot", "dot", "dog", "lot", "log", "cog"];
    assert_eq!(ladder_bfs("hit", "cog", &list), 5);
    assert_eq!(ladder_bidir("hit", "cog", &list), 5);
    assert_eq!(
        ladder_bidir("hit", "cog", &["hot", "dot", "dog", "lot", "log"]),
        0
    );
}

// ================================================================================================
/// Q9. SHORTEST PATH IN A BINARY MATRIX. n x n grid of 0 (open) / 1 (blocked); move in 8 directions from the top-left to the bottom-right;
/// return the path length in cells, or -1.   [[0,1],[1,0]] -> 2;   [[0,0,0],[1,1,0],[1,1,0]] -> 4
///
/// APPROACH. Unit-cost moves -> BFS from the start, level = number of cells. O(n^2). Check the start and goal are open first.
/// Pattern: BFS shortest path on a grid; only the direction table changes between problems.
/// RUST. `DIRS8` const array plus `neighbors(.., &DIRS8)`. A 1 x 1 open grid has length 1 (start == goal): handle it BEFORE the loop
/// if you test the goal on expansion, or test on dequeue as here.
/// INTERVIEWER IS TESTING: start/goal blocked, the 1 x 1 case, and counting cells vs moves.
fn shortest_binary(g: &Grid) -> Option<usize> {
    let n = g.len();
    if n == 0 || g[0][0] == 1 || g[n - 1][n - 1] == 1 {
        return None;
    }
    let mut dist = vec![vec![usize::MAX; n]; n];
    dist[0][0] = 1;
    let mut q = VecDeque::from([(0, 0)]);
    while let Some((r, c)) = q.pop_front() {
        if (r, c) == (n - 1, n - 1) {
            return Some(dist[r][c]);
        }
        for (nr, nc) in neighbors(r, c, n, n, &DIRS8) {
            if g[nr][nc] == 0 && dist[nr][nc] == usize::MAX {
                dist[nr][nc] = dist[r][c] + 1;
                q.push_back((nr, nc));
            }
        }
    }
    None
}
fn q9() {
    let mut rng = Rng::new(9);
    for _ in 0..CASES {
        let n = 1 + rng.below(6);
        let g: Grid = (0..n)
            .map(|_| (0..n).map(|_| u8::from(rng.below(3) == 0)).collect())
            .collect();
        let mut d = vec![vec![usize::MAX; n]; n];
        if g[0][0] == 0 {
            d[0][0] = 1;
        }
        loop {
            let mut changed = false;
            for r in 0..n {
                for c in 0..n {
                    if g[r][c] == 0 {
                        for (nr, nc) in neighbors(r, c, n, n, &DIRS8) {
                            if d[nr][nc] != usize::MAX && d[nr][nc] + 1 < d[r][c] {
                                d[r][c] = d[nr][nc] + 1;
                                changed = true;
                            }
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        assert_eq!(
            shortest_binary(&g),
            (d[n - 1][n - 1] != usize::MAX).then_some(d[n - 1][n - 1]),
            "{g:?}"
        );
    }
    assert_eq!(shortest_binary(&vec![vec![0, 1], vec![1, 0]]), Some(2));
    assert_eq!(
        shortest_binary(&vec![vec![0, 0, 0], vec![1, 1, 0], vec![1, 1, 0]]),
        Some(4)
    );
    assert_eq!(shortest_binary(&vec![vec![0]]), Some(1));
    assert_eq!(shortest_binary(&vec![vec![1]]), None);
}

// ================================================================================================
/// Q10. OPEN THE LOCK. A lock has w wheels (digits 0-9, wrapping). Start at "000..0"; one move turns one wheel one step either way.
/// Some combinations are DEADENDS (the lock jams if reached). Fewest moves to `target`, or -1.
///   deadends ["0201","0101","0102","1212","2002"], target "0202" -> 6;   deadends ["8888"], target "0009" -> 1
///
/// APPROACH. The state space is an implicit graph of 10^w nodes with 2w neighbours each. BFS from the start over states that are not
/// deadends; O(10^w * w). Encode a state as an integer (`digit * 10^i`) so visited is a flat `Vec<bool>`, not a hash set of strings.
/// Start itself a deadend -> impossible. Bidirectional BFS is a follow-up.
/// RUST. Rotating a digit: `(d + 1) % 10` and `(d + 9) % 10` (never `d - 1` on unsigned). `ns = s - d * p + nd * p`.
/// INTERVIEWER IS TESTING: implicit graph modelling, deadend start, target == start.
fn open_lock(deadends: &[&str], target: &str) -> Option<usize> {
    let w = target.len();
    let total = 10usize.pow(w as u32);
    let parse = |s: &str| s.parse::<usize>().unwrap();
    let mut dead = vec![false; total];
    for d in deadends {
        dead[parse(d)] = true;
    }
    if dead[0] {
        return None;
    }
    let t = parse(target);
    let mut dist = vec![usize::MAX; total];
    dist[0] = 0;
    let mut q = VecDeque::from([0usize]);
    while let Some(s) = q.pop_front() {
        if s == t {
            return Some(dist[s]);
        }
        for i in 0..w {
            let p = 10usize.pow(i as u32);
            let digit = (s / p) % 10;
            for nd in [(digit + 1) % 10, (digit + 9) % 10] {
                let ns = s - digit * p + nd * p;
                if !dead[ns] && dist[ns] == usize::MAX {
                    dist[ns] = dist[s] + 1;
                    q.push_back(ns);
                }
            }
        }
    }
    None
}
fn q10() {
    let mut rng = Rng::new(10);
    for _ in 0..CASES {
        // two wheels (100 states) so the relaxation oracle is cheap
        let dead_n = rng.below(40);
        let dead_s: Vec<String> = (0..dead_n)
            .map(|_| format!("{:02}", rng.below(100)))
            .collect();
        let dead: Vec<&str> = dead_s.iter().map(String::as_str).collect();
        let target = format!("{:02}", rng.below(100));
        let mut is_dead = [false; 100];
        dead.iter()
            .for_each(|d| is_dead[d.parse::<usize>().unwrap()] = true);
        let mut d = [usize::MAX; 100];
        if !is_dead[0] {
            d[0] = 0;
        }
        loop {
            let mut changed = false;
            for s in 0..100usize {
                if d[s] == usize::MAX {
                    continue;
                }
                let (tens, ones) = (s / 10, s % 10);
                for ns in [
                    ((tens + 1) % 10) * 10 + ones,
                    ((tens + 9) % 10) * 10 + ones,
                    tens * 10 + (ones + 1) % 10,
                    tens * 10 + (ones + 9) % 10,
                ] {
                    if !is_dead[ns] && d[s] + 1 < d[ns] {
                        d[ns] = d[s] + 1;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let t = target.parse::<usize>().unwrap();
        assert_eq!(
            open_lock(&dead, &target),
            (d[t] != usize::MAX).then_some(d[t]),
            "{dead:?} {target}"
        );
    }
    assert_eq!(
        open_lock(&["0201", "0101", "0102", "1212", "2002"], "0202"),
        Some(6)
    );
    assert_eq!(open_lock(&["8888"], "0009"), Some(1));
    assert_eq!(
        open_lock(
            &[
                "8887", "8889", "8878", "8898", "8788", "8988", "7888", "9888"
            ],
            "8888"
        ),
        None
    );
    assert_eq!(open_lock(&["0000"], "8888"), None);
}

// ================================================================================================
/// Q11. SNAKES AND LADDERS. n x n board numbered 1..n^2 in BOUSTROPHEDON order starting at the bottom-left (rows alternate direction).
/// Each cell holds -1 or a destination square. From square s roll 1-6 and move; landing on a snake/ladder takes you to its destination
/// (once; no chaining). Fewest rolls to reach square n^2, or -1.
///   the LeetCode 6x6 example -> 4
///
/// APPROACH. Squares are nodes, the six dice outcomes are edges: BFS over square numbers, O(n^2 * 6). The only hard part is mapping a
/// square number to (row, col): `r = (s - 1) / n` counted from the bottom, `c = (s - 1) % n`, and if r is odd the row runs
/// right-to-left (`c = n - 1 - c`); finally flip the row index to the top-based one.
/// RUST. Precompute a flat `Vec<i32>` `jump[s]` once so BFS never touches 2-D indices. Compare squares with `min(s + 6, n * n)`.
/// INTERVIEWER IS TESTING: the numbering conversion (draw it) and that a jump is applied exactly once.
fn square_to_cell(s: usize, n: usize) -> (usize, usize) {
    let from_bottom = (s - 1) / n;
    let mut col = (s - 1) % n;
    if from_bottom % 2 == 1 {
        col = n - 1 - col;
    }
    (n - 1 - from_bottom, col)
}
fn snakes_and_ladders(board: &[Vec<i32>]) -> Option<usize> {
    let n = board.len();
    let goal = n * n;
    let mut jump = vec![0usize; goal + 1];
    for s in 1..=goal {
        let (r, c) = square_to_cell(s, n);
        jump[s] = if board[r][c] == -1 {
            s
        } else {
            board[r][c] as usize
        };
    }
    let mut dist = vec![usize::MAX; goal + 1];
    dist[1] = 0;
    let mut q = VecDeque::from([1usize]);
    while let Some(s) = q.pop_front() {
        if s == goal {
            return Some(dist[s]);
        }
        for next in s + 1..=(s + 6).min(goal) {
            let dest = jump[next];
            if dist[dest] == usize::MAX {
                dist[dest] = dist[s] + 1;
                q.push_back(dest);
            }
        }
    }
    (dist[goal] != usize::MAX).then_some(dist[goal])
}
fn q11() {
    let mut rng = Rng::new(11);
    for _ in 0..CASES {
        let n = 2 + rng.below(4);
        let mut board = vec![vec![-1i32; n]; n];
        for s in 2..n * n {
            if rng.below(4) == 0 {
                let (r, c) = square_to_cell(s, n);
                board[r][c] = 1 + rng.below(n * n) as i32;
            }
        }
        let goal = n * n;
        let mut d = vec![usize::MAX; goal + 1];
        d[1] = 0;
        loop {
            let mut changed = false;
            for s in 1..=goal {
                if d[s] == usize::MAX {
                    continue;
                }
                for roll in 1..=6 {
                    if s + roll > goal {
                        continue;
                    }
                    let (r, c) = square_to_cell(s + roll, n);
                    let dest = if board[r][c] == -1 {
                        s + roll
                    } else {
                        board[r][c] as usize
                    };
                    if d[s] + 1 < d[dest] {
                        d[dest] = d[s] + 1;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        assert_eq!(
            snakes_and_ladders(&board),
            (d[goal] != usize::MAX).then_some(d[goal]),
            "{board:?}"
        );
    }
    let board = vec![
        vec![-1, -1, -1, -1, -1, -1],
        vec![-1, -1, -1, -1, -1, -1],
        vec![-1, -1, -1, -1, -1, -1],
        vec![-1, 35, -1, -1, 13, -1],
        vec![-1, -1, -1, -1, -1, -1],
        vec![-1, 15, -1, -1, -1, -1],
    ];
    assert_eq!(snakes_and_ladders(&board), Some(4));
    assert_eq!(square_to_cell(1, 6), (5, 0));
    assert_eq!(square_to_cell(7, 6), (4, 5));
}

// ================================================================================================
/// Q12. SHORTEST BRIDGE. A 0/1 grid contains exactly two islands. Flip the fewest 0s to 1 to connect them.
///   [[0,1],[1,0]] -> 1;   [[1,1,1,1,1],[1,0,0,0,1],[1,0,1,0,1],[1,0,0,0,1],[1,1,1,1,1]] -> 1
///
/// APPROACH. Step 1: DFS (explicit stack) from the first land cell to collect and mark ALL cells of island A. Step 2: multi-source BFS
/// outward from every cell of A over water, one ring per level; the first time the frontier touches an unmarked land cell (island B)
/// the number of completed rings is the number of zeros to flip. O(mn). Oracle: minimum Manhattan distance between a cell of A and a cell of B, minus 1.
/// RUST. Reuse a single `seen` matrix for both phases (island A marked in step 1 is the BFS's starting layer). Rings: capture the
/// queue length per level as in Q4.
/// INTERVIEWER IS TESTING: composing two traversals and the off-by-one between distance and flips.
fn shortest_bridge(g: &Grid) -> usize {
    let (rows, cols) = dims(g);
    let mut seen = vec![vec![false; cols]; rows];
    let start = (0..rows)
        .flat_map(|r| (0..cols).map(move |c| (r, c)))
        .find(|&(r, c)| g[r][c] == 1)
        .unwrap();
    let (mut stack, mut q) = (vec![start], VecDeque::new());
    seen[start.0][start.1] = true;
    while let Some((r, c)) = stack.pop() {
        q.push_back((r, c));
        for (nr, nc) in n4(r, c, rows, cols) {
            if g[nr][nc] == 1 && !seen[nr][nc] {
                seen[nr][nc] = true;
                stack.push((nr, nc));
            }
        }
    }
    let mut flips = 0;
    while !q.is_empty() {
        for _ in 0..q.len() {
            let (r, c) = q.pop_front().unwrap();
            for (nr, nc) in n4(r, c, rows, cols) {
                if seen[nr][nc] {
                    continue;
                }
                if g[nr][nc] == 1 {
                    return flips;
                }
                seen[nr][nc] = true;
                q.push_back((nr, nc));
            }
        }
        flips += 1;
    }
    unreachable!("the problem guarantees two islands")
}
fn q12() {
    let mut rng = Rng::new(12);
    let mut tested = 0;
    while tested < CASES {
        let g = rand_grid(&mut rng, 7, 3);
        if islands_dfs(&g) != 2 {
            continue; // the question promises exactly two islands: resample until we get one
        }
        tested += 1;
        // label cells by island with the union-find helper
        let (rows, cols) = dims(&g);
        let mut uf = Uf::new(rows * cols);
        for r in 0..rows {
            for c in 0..cols {
                if g[r][c] == 1 {
                    for (nr, nc) in n4(r, c, rows, cols) {
                        if g[nr][nc] == 1 {
                            uf.union(r * cols + c, nr * cols + nc);
                        }
                    }
                }
            }
        }
        let land: Vec<(usize, usize)> = (0..rows)
            .flat_map(|r| (0..cols).map(move |c| (r, c)))
            .filter(|&(r, c)| g[r][c] == 1)
            .collect();
        let mut best = usize::MAX;
        for &(r1, c1) in &land {
            for &(r2, c2) in &land {
                if uf.find(r1 * cols + c1) != uf.find(r2 * cols + c2) {
                    best = best.min(r1.abs_diff(r2) + c1.abs_diff(c2) - 1);
                }
            }
        }
        assert_eq!(shortest_bridge(&g), best, "{g:?}");
    }
    assert_eq!(shortest_bridge(&vec![vec![0, 1], vec![1, 0]]), 1);
    let ring: Grid = ["11111", "10001", "10101", "10001", "11111"]
        .iter()
        .map(|s| s.bytes().map(|b| b - b'0').collect())
        .collect();
    assert_eq!(shortest_bridge(&ring), 1);
}

// ================================================================================================
/// Q13. IS GRAPH BIPARTITE (adjacency list, may be disconnected) and POSSIBLE BIPARTITION (n people, pairs that dislike each other; split into
/// two groups with no dislike inside a group).
///   [[1,2,3],[0,2],[0,1,3],[0,2]] -> false (odd cycle);   n=4, dislikes [[1,2],[1,3],[2,4]] -> true
///
/// APPROACH. A graph is bipartite iff it has NO ODD CYCLE iff it is 2-colourable. BFS/DFS colour each component: start with colour 0,
/// colour neighbours with the opposite colour; meeting a neighbour with the SAME colour proves an odd cycle. Loop over ALL start nodes
/// (graphs can be disconnected: the classic bug). O(V + E). Union-find with an "enemy" link also works. Brute force: try all 2^V colourings.
/// RUST. `Vec<Option<bool>>` or `Vec<i8>` for colours; a `for start in 0..n` driver around the BFS. Possible Bipartition is 1-indexed:
/// allocate `n + 1` slots. Self-loops make a graph non-bipartite (the colour check catches it).
/// INTERVIEWER IS TESTING: disconnected components, self loops, 1-based ids.
fn is_bipartite(adj: &[Vec<usize>]) -> bool {
    let mut colour: Vec<Option<bool>> = vec![None; adj.len()];
    for s in 0..adj.len() {
        if colour[s].is_some() {
            continue;
        }
        colour[s] = Some(false);
        let mut q = VecDeque::from([s]);
        while let Some(u) = q.pop_front() {
            for &v in &adj[u] {
                match colour[v] {
                    None => {
                        colour[v] = Some(!colour[u].unwrap());
                        q.push_back(v);
                    }
                    Some(c) if c == colour[u].unwrap() => return false,
                    Some(_) => {}
                }
            }
        }
    }
    true
}
fn possible_bipartition(n: usize, dislikes: &[(usize, usize)]) -> bool {
    let mut adj = vec![vec![]; n + 1];
    for &(a, b) in dislikes {
        adj[a].push(b);
        adj[b].push(a);
    }
    is_bipartite(&adj)
}
fn q13() {
    let mut rng = Rng::new(13);
    for _ in 0..CASES {
        let n = 1 + rng.below(8);
        let edges: Vec<(usize, usize)> = (0..rng.below(10))
            .map(|_| (rng.below(n), rng.below(n)))
            .collect();
        let mut adj = vec![vec![]; n];
        for &(a, b) in &edges {
            adj[a].push(b);
            adj[b].push(a);
        }
        let want = (0u32..1 << n).any(|m| edges.iter().all(|&(a, b)| (m >> a & 1) != (m >> b & 1)));
        assert_eq!(is_bipartite(&adj), want, "{edges:?}");
        let dislikes: Vec<(usize, usize)> = edges.iter().map(|&(a, b)| (a + 1, b + 1)).collect();
        assert_eq!(possible_bipartition(n, &dislikes), want);
    }
    assert!(!is_bipartite(&[
        vec![1, 2, 3],
        vec![0, 2],
        vec![0, 1, 3],
        vec![0, 2]
    ]));
    assert!(possible_bipartition(4, &[(1, 2), (1, 3), (2, 4)]));
    assert!(!possible_bipartition(3, &[(1, 2), (1, 3), (2, 3)]));
}

// ================================================================================================
/// Q14. NUMBER OF CONNECTED COMPONENTS in an undirected graph, and GRAPH VALID TREE (are n nodes and these edges a tree?).
///   n=5, [[0,1],[1,2],[3,4]] -> 2 components;   n=5, [[0,1],[0,2],[0,3],[1,4]] -> tree;   [[0,1],[1,2],[2,3],[1,3],[1,4]] -> not a tree
///
/// APPROACH. Components: DFS/BFS flood from each unvisited node, or union-find (`components = n - successful unions`), O(V + E) /
/// O((V + E) alpha). Valid tree: a tree on n nodes has EXACTLY n - 1 edges and is connected (equivalently acyclic and connected). With
/// union-find: any edge whose endpoints are already joined is a cycle -> false; at the end require a single component. Check
/// `edges.len() == n - 1` first for a free early exit.
/// RUST. `Uf::union` returns a bool precisely so cycle detection is one `if !uf.union(a, b) { return false }`. Careful with `n - 1`
/// when `n == 0` (usize underflow): treat n == 0 separately.
/// INTERVIEWER IS TESTING: the n - 1 edge fact, and picking union-find when edges arrive incrementally.
fn count_components(n: usize, edges: &[(usize, usize)]) -> usize {
    let mut uf = Uf::new(n);
    for &(a, b) in edges {
        uf.union(a, b);
    }
    uf.comps
}
fn valid_tree(n: usize, edges: &[(usize, usize)]) -> bool {
    if n == 0 || edges.len() != n - 1 {
        return false;
    }
    let mut uf = Uf::new(n);
    edges.iter().all(|&(a, b)| uf.union(a, b)) && uf.comps == 1
}
fn q14() {
    let mut rng = Rng::new(14);
    for _ in 0..CASES {
        let n = 1 + rng.below(7);
        let edges: Vec<(usize, usize)> = (0..rng.below(8))
            .map(|_| (rng.below(n), rng.below(n)))
            .filter(|(a, b)| a != b)
            .collect();
        // oracle: transitive closure by Floyd-Warshall on a reachability matrix
        let mut reach = vec![vec![false; n]; n];
        (0..n).for_each(|i| reach[i][i] = true);
        for &(a, b) in &edges {
            reach[a][b] = true;
            reach[b][a] = true;
        }
        for k in 0..n {
            for i in 0..n {
                for j in 0..n {
                    reach[i][j] |= reach[i][k] && reach[k][j];
                }
            }
        }
        let comps = (0..n).filter(|&i| (0..i).all(|j| !reach[i][j])).count();
        assert_eq!(count_components(n, &edges), comps, "{edges:?}");
        // a graph is a tree iff connected with n-1 distinct edges; duplicate edges would make union() fail, matching "cycle"
        let distinct: BTreeSet<(usize, usize)> =
            edges.iter().map(|&(a, b)| (a.min(b), a.max(b))).collect();
        let tree = comps == 1 && edges.len() == n - 1 && distinct.len() == edges.len();
        assert_eq!(valid_tree(n, &edges), tree, "{edges:?}");
    }
    assert_eq!(count_components(5, &[(0, 1), (1, 2), (3, 4)]), 2);
    assert!(valid_tree(5, &[(0, 1), (0, 2), (0, 3), (1, 4)]));
    assert!(!valid_tree(5, &[(0, 1), (1, 2), (2, 3), (1, 3), (1, 4)]));
    assert!(valid_tree(1, &[]));
}

// ================================================================================================
/// Q15. REDUNDANT CONNECTION. A tree of n nodes (1..n) had ONE extra edge added; given the edges, return the edge that can be removed so the
/// result is a tree. If several qualify, return the one that appears LAST in the input.
///   [[1,2],[1,3],[2,3]] -> [2,3];   [[1,2],[2,3],[3,4],[1,4],[1,5]] -> [1,4]
///
/// APPROACH. Process edges in order with union-find; the first edge whose endpoints are ALREADY connected closes the cycle and is the answer
/// (it is the last edge of the cycle in input order). O(n alpha(n)). Brute force: for each edge from the end, remove it and test whether the
/// remainder is a tree, O(n^2). Directed variant (Redundant Connection II) needs in-degree reasoning: not covered here.
/// RUST. Node ids are 1-based: size the union-find `n + 1`. Return `Option<(usize, usize)>` (None for input without a cycle).
/// INTERVIEWER IS TESTING: why the first cycle-closing edge in input order is also the last candidate that is removable.
fn redundant_connection(edges: &[(usize, usize)]) -> Option<(usize, usize)> {
    let mut uf = Uf::new(edges.len() + 2);
    edges.iter().copied().find(|&(a, b)| !uf.union(a, b))
}
fn q15() {
    let mut rng = Rng::new(15);
    for _ in 0..CASES {
        let n = 3 + rng.below(6);
        let mut edges: Vec<(usize, usize)> = (2..=n).map(|v| (1 + rng.below(v - 1), v)).collect(); // random tree on 1..=n
        let extra = loop {
            let (a, b) = (1 + rng.below(n), 1 + rng.below(n));
            if a != b
                && !edges
                    .iter()
                    .any(|&(x, y)| (x, y) == (a, b) || (x, y) == (b, a))
            {
                break (a, b);
            }
        };
        edges.push(extra);
        rng.shuffle(&mut edges);
        let want = (0..edges.len()).rev().find(|&skip| {
            let rest: Vec<(usize, usize)> = edges
                .iter()
                .enumerate()
                .filter(|&(i, _)| i != skip)
                .map(|(_, &e)| e)
                .collect();
            let relabelled: Vec<(usize, usize)> =
                rest.iter().map(|&(a, b)| (a - 1, b - 1)).collect();
            valid_tree(n, &relabelled)
        });
        let got = redundant_connection(&edges);
        assert_eq!(
            got.map(|e| edges.iter().rposition(|&x| x == e).unwrap()),
            want,
            "{edges:?}"
        );
    }
    assert_eq!(
        redundant_connection(&[(1, 2), (1, 3), (2, 3)]),
        Some((2, 3))
    );
    assert_eq!(
        redundant_connection(&[(1, 2), (2, 3), (3, 4), (1, 4), (1, 5)]),
        Some((1, 4))
    );
    assert_eq!(redundant_connection(&[(1, 2), (2, 3)]), None);
}

// ================================================================================================
/// Q16. ACCOUNTS MERGE. Each account is a name plus a list of emails. Two accounts belong to the same person if they share ANY email. Merge them:
/// output each person once as name followed by their SORTED unique emails.
///   [["John","a","b"],["John","a","c"],["Mary","m"],["John","x"]] -> [["John","a","b","c"],["John","x"],["Mary","m"]]
///
/// APPROACH. Treat accounts as nodes; the first time an email is seen record its owner, and when it appears again UNION the two accounts.
/// Afterwards group emails by the root of their owner, sort each group, prepend the name. O(N L alpha + total log) for sorting. DFS over an
/// email graph is equivalent. Brute force: repeatedly merge any two sets that intersect until stable.
/// RUST. `HashMap<&str, usize>` email -> first account (borrow the strings; no cloning until output). Group with `BTreeMap<usize, BTreeSet<&str>>`
/// so sorting and dedup are free. Final order of people is by root: normalise (sort) before comparing in tests.
/// INTERVIEWER IS TESTING: choosing accounts (not emails) as union-find nodes and not merging by NAME (names collide between different people).
fn accounts_merge(accounts: &[(String, Vec<String>)]) -> Vec<(String, Vec<String>)> {
    let mut uf = Uf::new(accounts.len());
    let mut owner: HashMap<&str, usize> = HashMap::new();
    for (i, (_, emails)) in accounts.iter().enumerate() {
        for e in emails {
            match owner.get(e.as_str()) {
                Some(&j) => {
                    uf.union(i, j);
                }
                None => {
                    owner.insert(e, i);
                }
            }
        }
    }
    let mut groups: BTreeMap<usize, BTreeSet<&str>> = BTreeMap::new();
    for (i, (_, emails)) in accounts.iter().enumerate() {
        let root = uf.find(i);
        groups
            .entry(root)
            .or_default()
            .extend(emails.iter().map(String::as_str));
    }
    let mut out: Vec<(String, Vec<String>)> = groups
        .into_iter()
        .map(|(root, set)| {
            (
                accounts[root].0.clone(),
                set.into_iter().map(String::from).collect(),
            )
        })
        .collect();
    out.sort();
    out
}
fn accounts_oracle(accounts: &[(String, Vec<String>)]) -> BTreeSet<Vec<String>> {
    let mut sets: Vec<BTreeSet<String>> = accounts
        .iter()
        .map(|(_, e)| e.iter().cloned().collect())
        .collect();
    loop {
        let pair = (0..sets.len()).find_map(|i| {
            (i + 1..sets.len())
                .find(|&j| !sets[i].is_disjoint(&sets[j]))
                .map(|j| (i, j))
        });
        match pair {
            Some((i, j)) => {
                let other = sets.remove(j);
                sets[i].extend(other);
            }
            None => break,
        }
    }
    sets.into_iter().map(|s| s.into_iter().collect()).collect()
}
fn q16() {
    let mut rng = Rng::new(16);
    for _ in 0..CASES {
        let n = rng.below(7);
        let accounts: Vec<(String, Vec<String>)> = (0..n)
            .map(|_| {
                let emails: Vec<String> = (0..1 + rng.below(3))
                    .map(|_| format!("e{}", rng.below(8)))
                    .collect();
                ("Name".to_string(), emails)
            })
            .collect();
        let got: BTreeSet<Vec<String>> = accounts_merge(&accounts)
            .into_iter()
            .map(|(_, e)| e)
            .collect();
        assert_eq!(got, accounts_oracle(&accounts), "{accounts:?}");
    }
    let s = |x: &str| x.to_string();
    let acc = vec![
        (s("John"), vec![s("a"), s("b")]),
        (s("John"), vec![s("a"), s("c")]),
        (s("Mary"), vec![s("m")]),
        (s("John"), vec![s("x")]),
    ];
    let merged = accounts_merge(&acc);
    assert_eq!(merged.len(), 3);
    assert!(merged.contains(&(s("John"), vec![s("a"), s("b"), s("c")])));
    assert!(merged.contains(&(s("John"), vec![s("x")])));
}

// ================================================================================================
/// Q17. EVALUATE DIVISION. Given equations `a / b = value` and queries `x / y`, answer each query (-1.0 if undetermined).
///   a/b = 2, b/c = 3; queries a/c -> 6, b/a -> 0.5, a/e -> -1, a/a -> 1, x/x -> -1 (x unknown)
///
/// APPROACH. Variables are nodes, an equation is an edge with weight `value` (and reverse weight `1/value`). A query is the product of weights along any
/// path: DFS/BFS per query, O(Q (V + E)); or Floyd-Warshall on the ratios, O(V^3). WEIGHTED UNION-FIND: keep `w[x]` = x / parent[x];
/// `find` with path compression multiplies weights up to the root so `w[x]` = x / root; query `a / b = w[a] / w[b]` if the roots match.
/// Union of `a / b = v` links roots with `w[ra] = v * w[b] / w[a]`. Near-constant per operation.
/// RUST. Map names to ids with `HashMap<&str, usize>` using the entry API (`let n = ids.len(); *ids.entry(s).or_insert(n)`). Compare floats with an
/// epsilon in tests. The recursive `find` mutates `w[x]` after the recursive call returns: read `p[x]` into a local first.
/// INTERVIEWER IS TESTING: modelling ratios as a graph, unknown variables, and float comparison.
struct WeightedUf {
    p: Vec<usize>,
    w: Vec<f64>,
}
impl WeightedUf {
    fn new(n: usize) -> Self {
        WeightedUf {
            p: (0..n).collect(),
            w: vec![1.0; n],
        }
    }
    fn find(&mut self, x: usize) -> usize {
        let parent = self.p[x];
        if parent != x {
            let root = self.find(parent);
            self.w[x] *= self.w[parent]; // x/parent * parent/root
            self.p[x] = root;
        }
        self.p[x]
    }
    fn union(&mut self, a: usize, b: usize, v: f64) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.p[ra] = rb;
            self.w[ra] = v * self.w[b] / self.w[a];
        }
    }
    fn ratio(&mut self, a: usize, b: usize) -> Option<f64> {
        (self.find(a) == self.find(b)).then(|| self.w[a] / self.w[b])
    }
}
fn calc_equation(equations: &[(&str, &str, f64)], queries: &[(&str, &str)]) -> Vec<f64> {
    let mut ids: HashMap<&str, usize> = HashMap::new();
    for &(a, b, _) in equations {
        for s in [a, b] {
            let n = ids.len();
            ids.entry(s).or_insert(n);
        }
    }
    let mut uf = WeightedUf::new(ids.len());
    for &(a, b, v) in equations {
        uf.union(ids[a], ids[b], v);
    }
    queries
        .iter()
        .map(|&(a, b)| match (ids.get(a), ids.get(b)) {
            (Some(&x), Some(&y)) => uf.ratio(x, y).unwrap_or(-1.0),
            _ => -1.0,
        })
        .collect()
}
fn q17() {
    let mut rng = Rng::new(17);
    let names = ["a", "b", "c", "d", "e", "f", "g"];
    for _ in 0..CASES {
        let truth: Vec<f64> = (0..names.len())
            .map(|_| (1 + rng.below(6)) as f64)
            .collect();
        let eqs: Vec<(&str, &str, f64)> = (0..rng.below(7))
            .map(|_| {
                let (i, j) = (rng.below(5), rng.below(5));
                (names[i], names[j], truth[i] / truth[j])
            })
            .collect();
        // oracle: closure over the variables that appear, by repeated multiplication (Floyd-Warshall on ratios)
        let mut known = [[None::<f64>; 7]; 7];
        for i in 0..7 {
            known[i][i] = None;
        }
        let mut present = [false; 7];
        for &(a, b, v) in &eqs {
            let (i, j) = (
                names.iter().position(|&x| x == a).unwrap(),
                names.iter().position(|&x| x == b).unwrap(),
            );
            present[i] = true;
            present[j] = true;
            known[i][j] = Some(v);
            known[j][i] = Some(1.0 / v);
        }
        for i in 0..7 {
            if present[i] {
                known[i][i] = Some(1.0);
            }
        }
        for k in 0..7 {
            for i in 0..7 {
                for j in 0..7 {
                    if known[i][j].is_none() {
                        if let (Some(x), Some(y)) = (known[i][k], known[k][j]) {
                            known[i][j] = Some(x * y);
                        }
                    }
                }
            }
        }
        let queries: Vec<(&str, &str)> = (0..4)
            .map(|_| (names[rng.below(7)], names[rng.below(7)]))
            .collect();
        let got = calc_equation(&eqs, &queries);
        for (q, g) in queries.iter().zip(got) {
            let (i, j) = (
                names.iter().position(|&x| x == q.0).unwrap(),
                names.iter().position(|&x| x == q.1).unwrap(),
            );
            match known[i][j] {
                Some(v) => assert!((g - v).abs() < 1e-9, "{eqs:?} {q:?}: {g} vs {v}"),
                None => assert_eq!(g, -1.0, "{eqs:?} {q:?}"),
            }
        }
    }
    let got = calc_equation(
        &[("a", "b", 2.0), ("b", "c", 3.0)],
        &[("a", "c"), ("b", "a"), ("a", "e"), ("a", "a"), ("x", "x")],
    );
    assert_eq!(got, vec![6.0, 0.5, -1.0, 1.0, -1.0]);
}

// ================================================================================================
/// Q18. NETWORK DELAY TIME. Directed weighted graph, nodes 1..n, `times[i] = (u, v, w)`. A signal starts at node k. How long until all nodes
/// receive it? -1 if some never do.   times [[2,1,1],[2,3,1],[3,4,1]], n = 4, k = 2 -> 2
///
/// APPROACH. Single-source shortest paths with NON-NEGATIVE weights -> DIJKSTRA with a binary heap: O((V + E) log V). Answer = max distance, or
/// -1 if any node is unreachable. Bellman-Ford is O(VE) and also handles negative weights (used as the oracle). BFS only works for unit weights.
/// RUST. `BinaryHeap` is a MAX-heap: wrap entries in `Reverse((dist, node))`. Skip stale entries (`if d > dist[u] { continue }`) instead of
/// a decrease-key, which std does not offer. `u64` distances with `u64::MAX` as infinity; never add to it.
/// INTERVIEWER IS TESTING: the Reverse trick, stale-entry skipping, and that Dijkstra fails with negative edges.
fn network_delay(n: usize, times: &[(usize, usize, u64)], k: usize) -> Option<u64> {
    let mut adj = vec![vec![]; n + 1];
    for &(u, v, w) in times {
        adj[u].push((v, w));
    }
    let mut dist = vec![u64::MAX; n + 1];
    dist[k] = 0;
    let mut heap = BinaryHeap::from([Reverse((0u64, k))]);
    while let Some(Reverse((d, u))) = heap.pop() {
        if d > dist[u] {
            continue;
        }
        for &(v, w) in &adj[u] {
            if d + w < dist[v] {
                dist[v] = d + w;
                heap.push(Reverse((dist[v], v)));
            }
        }
    }
    let worst = *dist[1..].iter().max()?;
    (worst != u64::MAX).then_some(worst)
}
fn q18() {
    let mut rng = Rng::new(18);
    for _ in 0..CASES {
        let n = 1 + rng.below(7);
        let times: Vec<(usize, usize, u64)> = (0..rng.below(14))
            .map(|_| (1 + rng.below(n), 1 + rng.below(n), rng.below(9) as u64))
            .collect();
        let k = 1 + rng.below(n);
        let mut d = vec![u64::MAX; n + 1];
        d[k] = 0;
        for _ in 0..n {
            for &(u, v, w) in &times {
                if d[u] != u64::MAX && d[u] + w < d[v] {
                    d[v] = d[u] + w;
                }
            }
        }
        let worst = *d[1..].iter().max().unwrap();
        assert_eq!(
            network_delay(n, &times, k),
            (worst != u64::MAX).then_some(worst),
            "{times:?} k={k}"
        );
    }
    assert_eq!(
        network_delay(4, &[(2, 1, 1), (2, 3, 1), (3, 4, 1)], 2),
        Some(2)
    );
    assert_eq!(network_delay(2, &[(1, 2, 1)], 2), None);
    assert_eq!(network_delay(1, &[], 1), Some(0));
}

// ================================================================================================
/// Q19. CHEAPEST FLIGHTS WITHIN K STOPS. `flights = (from, to, price)`; cheapest price from src to dst with AT MOST k intermediate stops (so at most k + 1 flights).
///   n = 4, flights [[0,1,100],[1,2,100],[2,0,100],[1,3,600],[2,3,200]], src 0, dst 3, k = 1 -> 700
///
/// APPROACH. Plain Dijkstra is wrong because the cheapest path may use too many hops; state would have to include the hop count. BELLMAN-FORD for
/// exactly k + 1 ROUNDS: round i computes the best cost using at most i flights; each round must read from a COPY of the previous
/// round's distances, otherwise one round can chain several flights. O(k * E), O(V). Alternatives: BFS/Dijkstra over (node, stops) states.
/// RUST. `let prev = dist.clone()` per round (the borrow checker will not stop you from reading the array you mutate by index; the copy is a
/// correctness requirement, not a borrow workaround). Use `i64::MAX / 2` as infinity so adding a price never overflows.
/// INTERVIEWER IS TESTING: why Dijkstra alone fails and the copy-per-round detail.
fn cheapest_flights(
    n: usize,
    flights: &[(usize, usize, i64)],
    src: usize,
    dst: usize,
    k: usize,
) -> Option<i64> {
    let inf = i64::MAX / 2;
    let mut dist = vec![inf; n];
    dist[src] = 0;
    for _ in 0..=k {
        let prev = dist.clone();
        for &(u, v, p) in flights {
            if prev[u] < inf && prev[u] + p < dist[v] {
                dist[v] = prev[u] + p;
            }
        }
    }
    (dist[dst] < inf).then_some(dist[dst])
}
fn flights_brute(
    flights: &[(usize, usize, i64)],
    u: usize,
    dst: usize,
    left: usize,
    cost: i64,
    best: &mut Option<i64>,
) {
    if u == dst {
        *best = Some(best.map_or(cost, |b| b.min(cost)));
    }
    if left == 0 {
        return;
    }
    for &(a, b, p) in flights {
        if a == u {
            flights_brute(flights, b, dst, left - 1, cost + p, best);
        }
    }
}
fn q19() {
    let mut rng = Rng::new(19);
    for _ in 0..CASES {
        let n = 2 + rng.below(5);
        let flights: Vec<(usize, usize, i64)> = (0..rng.below(10))
            .map(|_| (rng.below(n), rng.below(n), 1 + rng.below(9) as i64))
            .collect();
        let (src, dst, k) = (rng.below(n), rng.below(n), rng.below(4));
        let mut best = None;
        flights_brute(&flights, src, dst, k + 1, 0, &mut best);
        assert_eq!(
            cheapest_flights(n, &flights, src, dst, k),
            best,
            "{flights:?} {src}->{dst} k={k}"
        );
    }
    let f = [
        (0, 1, 100),
        (1, 2, 100),
        (2, 0, 100),
        (1, 3, 600),
        (2, 3, 200),
    ];
    assert_eq!(cheapest_flights(4, &f, 0, 3, 1), Some(700));
    assert_eq!(cheapest_flights(4, &f, 0, 3, 2), Some(400));
    assert_eq!(cheapest_flights(4, &f, 0, 3, 0), None);
}

// ================================================================================================
/// Q20. PATH WITH MINIMUM EFFORT (effort of a path = max absolute height difference between consecutive cells; minimise it from top-left to
/// bottom-right) and SWIM IN RISING WATER (cell values are the time it floods; minimise the max value on a path).
///   [[1,2,2],[3,8,2],[5,3,5]] -> effort 2;   swim [[0,2],[1,3]] -> 3
///
/// APPROACH. Both are MINIMAX path problems: the cost of a path is its worst edge/cell, not the sum. Dijkstra still works because
/// `max(cost_so_far, step)` is monotone: pop the node with the smallest bottleneck so far, relax with `max`. O(mn log mn). Alternative:
/// BINARY SEARCH the answer + BFS feasibility ("can I reach the end using only steps <= x?"), O(mn log range), or union-find adding
/// edges in increasing order until start and end connect. The oracle here is the linear threshold search.
/// RUST. Heap of `Reverse((cost, r, c))`; neighbours via the shared `n4` helper; `abs_diff` on unsigned heights avoids the signed subtraction.
/// INTERVIEWER IS TESTING: recognising that sum-Dijkstra is the wrong model and swapping `+` for `max`.
fn min_effort(h: &[Vec<u32>]) -> u32 {
    let (rows, cols) = dims(h);
    let mut best = vec![vec![u32::MAX; cols]; rows];
    best[0][0] = 0;
    let mut heap = BinaryHeap::from([Reverse((0u32, 0usize, 0usize))]);
    while let Some(Reverse((e, r, c))) = heap.pop() {
        if e > best[r][c] {
            continue;
        }
        if (r, c) == (rows - 1, cols - 1) {
            return e;
        }
        for (nr, nc) in n4(r, c, rows, cols) {
            let ne = e.max(h[r][c].abs_diff(h[nr][nc]));
            if ne < best[nr][nc] {
                best[nr][nc] = ne;
                heap.push(Reverse((ne, nr, nc)));
            }
        }
    }
    best[rows - 1][cols - 1]
}
fn swim(g: &[Vec<u32>]) -> u32 {
    let n = g.len();
    let mut best = vec![vec![u32::MAX; n]; n];
    best[0][0] = g[0][0];
    let mut heap = BinaryHeap::from([Reverse((g[0][0], 0usize, 0usize))]);
    while let Some(Reverse((t, r, c))) = heap.pop() {
        if t > best[r][c] {
            continue;
        }
        if (r, c) == (n - 1, n - 1) {
            return t;
        }
        for (nr, nc) in n4(r, c, n, n) {
            let nt = t.max(g[nr][nc]);
            if nt < best[nr][nc] {
                best[nr][nc] = nt;
                heap.push(Reverse((nt, nr, nc)));
            }
        }
    }
    best[n - 1][n - 1]
}
fn reachable_with(
    rows: usize,
    cols: usize,
    ok_step: impl Fn((usize, usize), (usize, usize)) -> bool,
    ok_cell: impl Fn(usize, usize) -> bool,
) -> bool {
    if !ok_cell(0, 0) {
        return false;
    }
    let mut seen = vec![vec![false; cols]; rows];
    seen[0][0] = true;
    let mut stack = vec![(0, 0)];
    while let Some((r, c)) = stack.pop() {
        for (nr, nc) in n4(r, c, rows, cols) {
            if !seen[nr][nc] && ok_cell(nr, nc) && ok_step((r, c), (nr, nc)) {
                seen[nr][nc] = true;
                stack.push((nr, nc));
            }
        }
    }
    seen[rows - 1][cols - 1]
}
fn q20() {
    let mut rng = Rng::new(20);
    for _ in 0..CASES {
        let (rows, cols) = (1 + rng.below(5), 1 + rng.below(5));
        let h: Vec<Vec<u32>> = (0..rows)
            .map(|_| (0..cols).map(|_| rng.below(10) as u32).collect())
            .collect();
        let want = (0..=9)
            .find(|&e| {
                reachable_with(
                    rows,
                    cols,
                    |a, b| h[a.0][a.1].abs_diff(h[b.0][b.1]) <= e,
                    |_, _| true,
                )
            })
            .unwrap();
        assert_eq!(min_effort(&h), want, "{h:?}");
        let n = 1 + rng.below(5);
        let mut vals: Vec<u32> = (0..(n * n) as u32).collect();
        rng.shuffle(&mut vals);
        let g: Vec<Vec<u32>> = vals.chunks(n).map(|c| c.to_vec()).collect();
        let want = (0..(n * n) as u32)
            .find(|&t| reachable_with(n, n, |_, _| true, |r, c| g[r][c] <= t))
            .unwrap();
        assert_eq!(swim(&g), want, "{g:?}");
    }
    assert_eq!(
        min_effort(&[vec![1, 2, 2], vec![3, 8, 2], vec![5, 3, 5]]),
        2
    );
    assert_eq!(swim(&[vec![0, 2], vec![1, 3]]), 3);
}

// ================================================================================================
/// Q21. MIN COST TO CONNECT ALL POINTS. Points in the plane, edge cost = Manhattan distance between any two points. Minimum total cost to connect all
/// points (a minimum spanning tree).   [[0,0],[2,2],[3,10],[5,2],[7,0]] -> 20
///
/// APPROACH. The graph is COMPLETE (E = V^2), so use DENSE PRIM: grow a tree from point 0, keep `best[v]` = cheapest edge from the tree to v, repeatedly add
/// the closest outside vertex and relax: O(V^2) time, O(V) space, no edge list and no heap. KRUSKAL (sort all V^2/2 edges, union-find) costs
/// O(V^2 log V): shown as the oracle. For sparse graphs prefer Kruskal or heap-based Prim: O(E log V).
/// RUST. Distances as `i64` from `abs_diff` casts (`abs_diff` returns unsigned): `(a.0.abs_diff(b.0) + a.1.abs_diff(b.1)) as i64`. Guard the empty input.
/// INTERVIEWER IS TESTING: choosing the dense variant for a complete graph, and the cut property in one sentence.
fn manhattan(a: (i64, i64), b: (i64, i64)) -> i64 {
    (a.0.abs_diff(b.0) + a.1.abs_diff(b.1)) as i64
}
fn mst_prim(p: &[(i64, i64)]) -> i64 {
    let n = p.len();
    if n == 0 {
        return 0;
    }
    let (mut in_tree, mut best, mut total) = (vec![false; n], vec![i64::MAX; n], 0);
    best[0] = 0;
    for _ in 0..n {
        let u = (0..n)
            .filter(|&i| !in_tree[i])
            .min_by_key(|&i| best[i])
            .unwrap();
        in_tree[u] = true;
        total += best[u];
        for v in 0..n {
            if !in_tree[v] {
                best[v] = best[v].min(manhattan(p[u], p[v]));
            }
        }
    }
    total
}
fn mst_kruskal(p: &[(i64, i64)]) -> i64 {
    let mut edges: Vec<(i64, usize, usize)> = vec![];
    for i in 0..p.len() {
        for j in i + 1..p.len() {
            edges.push((manhattan(p[i], p[j]), i, j));
        }
    }
    edges.sort_unstable();
    let mut uf = Uf::new(p.len());
    edges
        .into_iter()
        .filter(|&(_, a, b)| uf.union(a, b))
        .map(|e| e.0)
        .sum()
}
fn q21() {
    let mut rng = Rng::new(21);
    for _ in 0..CASES {
        let pts: Vec<(i64, i64)> = (0..rng.below(9))
            .map(|_| (rng.int(-6, 6), rng.int(-6, 6)))
            .collect();
        assert_eq!(mst_prim(&pts), mst_kruskal(&pts), "{pts:?}");
    }
    assert_eq!(mst_prim(&[(0, 0), (2, 2), (3, 10), (5, 2), (7, 0)]), 20);
    assert_eq!(mst_prim(&[(3, 12), (-2, 5), (-4, 1)]), 18);
    assert_eq!(mst_prim(&[(0, 0)]), 0);
}

// ================================================================================================
/// Q22. CRITICAL CONNECTIONS IN A NETWORK (find all BRIDGES: edges whose removal disconnects the graph).
///   n = 4, [[0,1],[1,2],[2,0],[1,3]] -> [[1,3]]
///
/// APPROACH. Brute force: remove each edge and recount components, O(E (V + E)). Optimal: Tarjan's LOW-LINK DFS. `tin[v]` = discovery time, `low[v]` = earliest discovery time
/// reachable from v's subtree using at most ONE back edge. A tree edge (u, v) is a bridge iff `low[v] > tin[u]` (v's subtree cannot climb above u). Skip the
/// edge you came from by EDGE ID (not by parent vertex) so parallel edges are handled. O(V + E). Needs an ITERATIVE DFS for big inputs:
/// the stack holds `(node, parent_edge, next_neighbour_index)`.
/// RUST. Re-borrowing: read the stack top into locals, bump its index with `stack[top].2 += 1`, then push or pop: no reference lives across the mutation.
/// Normalise each bridge as `(min, max)` and sort for stable output.
/// INTERVIEWER IS TESTING: the `low[v] > tin[u]` condition (not `>=`, which finds articulation points) and the parent-edge skip.
fn bridges(n: usize, edges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut adj = vec![vec![]; n];
    for (id, &(u, v)) in edges.iter().enumerate() {
        adj[u].push((v, id));
        adj[v].push((u, id));
    }
    let (mut tin, mut low, mut timer, mut out) = (vec![usize::MAX; n], vec![0usize; n], 0, vec![]);
    for s in 0..n {
        if tin[s] != usize::MAX {
            continue;
        }
        tin[s] = timer;
        low[s] = timer;
        timer += 1;
        let mut stack = vec![(s, usize::MAX, 0usize)];
        while !stack.is_empty() {
            let top = stack.len() - 1;
            let (u, parent_edge, i) = stack[top];
            if i < adj[u].len() {
                stack[top].2 += 1;
                let (v, id) = adj[u][i];
                if id == parent_edge {
                    continue;
                }
                if tin[v] == usize::MAX {
                    tin[v] = timer;
                    low[v] = timer;
                    timer += 1;
                    stack.push((v, id, 0));
                } else {
                    low[u] = low[u].min(tin[v]); // back edge
                }
            } else {
                stack.pop();
                if let Some(&(p, _, _)) = stack.last() {
                    low[p] = low[p].min(low[u]);
                    if low[u] > tin[p] {
                        out.push((p.min(u), p.max(u)));
                    }
                }
            }
        }
    }
    out.sort_unstable();
    out
}
fn q22() {
    let mut rng = Rng::new(22);
    for _ in 0..CASES {
        let n = 1 + rng.below(8);
        let mut edges: Vec<(usize, usize)> = vec![];
        for _ in 0..rng.below(11) {
            let (a, b) = (rng.below(n), rng.below(n));
            if a != b
                && !edges
                    .iter()
                    .any(|&(x, y)| (x, y) == (a, b) || (x, y) == (b, a))
            {
                edges.push((a, b));
            }
        }
        let base = count_components(n, &edges);
        let mut want: Vec<(usize, usize)> = (0..edges.len())
            .filter(|&skip| {
                let rest: Vec<_> = edges
                    .iter()
                    .enumerate()
                    .filter(|&(i, _)| i != skip)
                    .map(|(_, &e)| e)
                    .collect();
                count_components(n, &rest) > base
            })
            .map(|i| (edges[i].0.min(edges[i].1), edges[i].0.max(edges[i].1)))
            .collect();
        want.sort_unstable();
        assert_eq!(bridges(n, &edges), want, "{edges:?}");
    }
    assert_eq!(bridges(4, &[(0, 1), (1, 2), (2, 0), (1, 3)]), vec![(1, 3)]);
    assert_eq!(bridges(2, &[(0, 1), (0, 1)]), vec![]); // parallel edges: neither is a bridge
}

// ================================================================================================
/// Q23. FIND THE TOWN JUDGE (the judge trusts nobody and everyone else trusts the judge) and FIND CENTER OF STAR GRAPH (the node adjacent to all others).
///   n = 3, trust [[1,3],[2,3]] -> 3;   star edges [[1,2],[2,3],[4,2]] -> 2
///
/// APPROACH. Judge: this is a directed-graph degree question. `score[x] = in_degree - out_degree`; the judge is the node with score n - 1 (all others trust them and they trust nobody;
/// nobody else can reach n - 1). O(n + E), O(n). Brute force: test each candidate. Star: the centre appears in EVERY edge, so it is the common endpoint
/// of the first two edges: O(1). Lesson: look for a degree/invariant before building a traversal.
/// RUST. Signed scores (`Vec<i64>`) so `-=` cannot underflow; ids are 1-based. Intersect endpoints with simple comparisons, no sets needed.
/// INTERVIEWER IS TESTING: n = 1 with no trust edges (the lone person is the judge) and spotting the O(1) trick.
fn find_judge(n: usize, trust: &[(usize, usize)]) -> Option<usize> {
    let mut score = vec![0i64; n + 1];
    for &(a, b) in trust {
        score[a] -= 1;
        score[b] += 1;
    }
    (1..=n).find(|&i| score[i] == n as i64 - 1)
}
fn star_center(edges: &[(usize, usize)]) -> usize {
    let (a, b) = edges[0];
    if edges[1].0 == a || edges[1].1 == a {
        a
    } else {
        b
    }
}
fn q23() {
    let mut rng = Rng::new(23);
    for _ in 0..CASES {
        let n = 1 + rng.below(6);
        let mut trust: BTreeSet<(usize, usize)> = BTreeSet::new();
        for _ in 0..rng.below(10) {
            let (a, b) = (1 + rng.below(n), 1 + rng.below(n));
            if a != b {
                trust.insert((a, b));
            }
        }
        if rng.below(2) == 0 {
            let j = 1 + rng.below(n);
            trust.retain(|&(a, _)| a != j);
            trust.extend((1..=n).filter(|&i| i != j).map(|i| (i, j)));
        }
        let t: Vec<(usize, usize)> = trust.iter().copied().collect();
        let want = (1..=n).find(|&j| {
            t.iter().all(|&(a, _)| a != j)
                && (1..=n).filter(|&i| i != j).all(|i| t.contains(&(i, j)))
        });
        assert_eq!(find_judge(n, &t), want, "n={n} {t:?}");
        let m = 3 + rng.below(6);
        let c = 1 + rng.below(m);
        let mut edges: Vec<(usize, usize)> = (1..=m)
            .filter(|&i| i != c)
            .map(|i| if rng.below(2) == 0 { (c, i) } else { (i, c) })
            .collect();
        rng.shuffle(&mut edges);
        assert_eq!(star_center(&edges), c, "{edges:?}");
    }
    assert_eq!(find_judge(3, &[(1, 3), (2, 3)]), Some(3));
    assert_eq!(find_judge(1, &[]), Some(1));
    assert_eq!(find_judge(3, &[(1, 3), (2, 3), (3, 1)]), None);
    assert_eq!(star_center(&[(1, 2), (2, 3), (4, 2)]), 2);
}

// ================================================================================================
/// Q24. RECONSTRUCT ITINERARY. Given airline tickets [from, to], rebuild the itinerary starting at "JFK" that uses EVERY ticket exactly once; if several
/// exist return the lexicographically smallest. (A valid itinerary is guaranteed.)
///   [["MUC","LHR"],["JFK","MUC"],["SFO","SJC"],["LHR","SFO"]] -> JFK MUC LHR SFO SJC
///
/// APPROACH. Using every edge exactly once = an EULERIAN PATH. Brute force: backtracking over tickets in lexical order (exponential, the oracle). HIERHOLZER: DFS that
/// always takes the smallest unused outgoing edge, and appends a node to the route only when it is STUCK (no unused edges left); the route is built in reverse, so
/// reverse at the end. The "append on exit" step splices dead-end detours correctly. O(E log E) with sorted adjacency, O(E) after.
/// RUST. Airports are interned to small ids here (id order = lexical order) so each adjacency list is a `Vec<usize>` sorted DESCENDING: `pop()` then yields
/// the smallest remaining destination in O(1). Iterative with an explicit stack, as always. For string airports sort the `Vec<&str>` the same way.
/// INTERVIEWER IS TESTING: knowing it is an Euler path, the post-order append + reverse, and why the dead-end branch must be emitted last.
fn find_itinerary(n: usize, tickets: &[(usize, usize)]) -> Vec<usize> {
    let mut adj = vec![vec![]; n];
    for &(a, b) in tickets {
        adj[a].push(b);
    }
    adj.iter_mut()
        .for_each(|l| l.sort_unstable_by(|x, y| y.cmp(x)));
    let (mut stack, mut route) = (vec![0usize], vec![]);
    while let Some(&u) = stack.last() {
        if let Some(v) = adj[u].pop() {
            stack.push(v);
        } else {
            route.push(u);
            stack.pop();
        }
    }
    route.reverse();
    route
}
fn itinerary_oracle(n: usize, tickets: &[(usize, usize)]) -> Option<Vec<usize>> {
    fn go(u: usize, cnt: &mut Vec<Vec<usize>>, path: &mut Vec<usize>, total: usize) -> bool {
        if path.len() == total + 1 {
            return true;
        }
        for v in 0..cnt.len() {
            if cnt[u][v] > 0 {
                cnt[u][v] -= 1;
                path.push(v);
                if go(v, cnt, path, total) {
                    return true;
                }
                path.pop();
                cnt[u][v] += 1;
            }
        }
        false
    }
    let mut cnt = vec![vec![0usize; n]; n];
    tickets.iter().for_each(|&(a, b)| cnt[a][b] += 1);
    let mut path = vec![0];
    go(0, &mut cnt, &mut path, tickets.len()).then_some(path)
}
fn q24() {
    let mut rng = Rng::new(24);
    for _ in 0..CASES {
        let n = 1 + rng.below(5);
        // a random walk guarantees an Eulerian path from airport 0 exists
        let (mut cur, mut tickets) = (0usize, vec![]);
        for _ in 0..rng.below(9) {
            let next = rng.below(n);
            tickets.push((cur, next));
            cur = next;
        }
        rng.shuffle(&mut tickets);
        let want = itinerary_oracle(n, &tickets).unwrap();
        assert_eq!(find_itinerary(n, &tickets), want, "{tickets:?}");
    }
    // JFK = 0, LHR = 1, MUC = 2, SFO = 3, SJC = 4 (ids chosen in lexical order after JFK)
    assert_eq!(
        find_itinerary(5, &[(2, 1), (0, 2), (3, 4), (1, 3)]),
        vec![0, 2, 1, 3, 4]
    );
    // the dead-end-detour case: JFK->KUL, JFK->NRT, NRT->JFK  =>  JFK NRT JFK KUL
    assert_eq!(
        find_itinerary(3, &[(0, 1), (0, 2), (2, 0)]),
        vec![0, 2, 0, 1]
    );
}

// ================================================================================================
/// Q25. GRAPH REPRESENTATIONS: when to use an ADJACENCY LIST, ADJACENCY MATRIX, EDGE LIST or CSR, and how to treat a GRID as a graph safely.
///   interviewer: "n = 10^5 nodes, m = 3 * 10^5 edges, run many BFS: what do you store?"
///
/// APPROACH. Matrix: O(V^2) space, O(1) edge test, O(V) to enumerate neighbours: good only for dense or tiny graphs (V <= ~5000) and Floyd-Warshall.
/// Adjacency list (`Vec<Vec<usize>>`): O(V + E) space, O(deg) enumeration, O(deg) edge test: the default. Edge list (`Vec<(u, v, w)>`): O(E), perfect for
/// Kruskal, Bellman-Ford and input parsing, bad for neighbour queries. CSR (compressed sparse row: one `offsets` array of V + 1 and one flat
/// `targets` array of E): the list's complexity with ONE allocation and sequential memory, fastest to traverse, immutable once built. Grids need no
/// explicit graph at all: neighbours are computed from coordinates with a direction table.
/// RUST. Build CSR by counting sort: count degrees, prefix-sum into offsets, then place targets. For grids use `r.checked_add_signed(dr)` (returns `None`
/// if it would go below 0) then `< rows`; casting to `i32`/`isize` and back, or `r - 1` on `usize`, is the classic panic/wraparound bug.
/// INTERVIEWER IS TESTING: that you can justify the choice from V, E and the operation mix, and that grid code never underflows.
struct Graphs {
    n: usize,
    edges: Vec<(usize, usize)>,
}
impl Graphs {
    fn adj_list(&self) -> Vec<Vec<usize>> {
        let mut a = vec![vec![]; self.n];
        for &(u, v) in &self.edges {
            a[u].push(v);
            a[v].push(u);
        }
        a
    }
    fn matrix(&self) -> Vec<Vec<bool>> {
        let mut m = vec![vec![false; self.n]; self.n];
        for &(u, v) in &self.edges {
            m[u][v] = true;
            m[v][u] = true;
        }
        m
    }
    fn csr(&self) -> (Vec<usize>, Vec<usize>) {
        let mut offsets = vec![0usize; self.n + 1];
        for &(u, v) in &self.edges {
            offsets[u + 1] += 1;
            offsets[v + 1] += 1;
        }
        for i in 0..self.n {
            offsets[i + 1] += offsets[i];
        }
        let mut next = offsets.clone();
        let mut targets = vec![0usize; offsets[self.n]];
        for &(u, v) in &self.edges {
            targets[next[u]] = v;
            next[u] += 1;
            targets[next[v]] = u;
            next[v] += 1;
        }
        (offsets, targets)
    }
}
fn q25() {
    let mut rng = Rng::new(25);
    for _ in 0..CASES {
        let n = 1 + rng.below(8);
        let mut edges: Vec<(usize, usize)> = vec![];
        for _ in 0..rng.below(12) {
            let (a, b) = (rng.below(n), rng.below(n));
            if a != b
                && !edges
                    .iter()
                    .any(|&(x, y)| (x, y) == (a, b) || (x, y) == (b, a))
            {
                edges.push((a, b));
            }
        }
        let g = Graphs { n, edges };
        let (list, matrix, (offsets, targets)) = (g.adj_list(), g.matrix(), g.csr());
        for u in 0..n {
            let from_list: BTreeSet<usize> = list[u].iter().copied().collect();
            let from_matrix: BTreeSet<usize> = (0..n).filter(|&v| matrix[u][v]).collect();
            let from_csr: BTreeSet<usize> = targets[offsets[u]..offsets[u + 1]]
                .iter()
                .copied()
                .collect();
            assert_eq!(from_list, from_matrix);
            assert_eq!(from_list, from_csr);
            assert_eq!(
                list[u].len(),
                offsets[u + 1] - offsets[u],
                "degree must agree"
            );
        }
        assert_eq!(targets.len(), 2 * g.edges.len());
        // grid neighbour helper against signed arithmetic
        let (rows, cols) = (1 + rng.below(5), 1 + rng.below(5));
        let (r, c) = (rng.below(rows), rng.below(cols));
        let got: BTreeSet<(usize, usize)> = n4(r, c, rows, cols).collect();
        let want: BTreeSet<(usize, usize)> = DIRS
            .iter()
            .map(|&(dr, dc)| (r as i64 + dr as i64, c as i64 + dc as i64))
            .filter(|&(a, b)| a >= 0 && b >= 0 && a < rows as i64 && b < cols as i64)
            .map(|(a, b)| (a as usize, b as usize))
            .collect();
        assert_eq!(got, want);
    }
    assert_eq!(n4(0, 0, 1, 1).count(), 0);
    assert_eq!(n4(0, 0, 2, 2).count(), 2);
    assert_eq!(n4(1, 1, 3, 3).count(), 4);
    assert_eq!(0usize.checked_add_signed(-1), None); // the underflow that `r - 1` would turn into a panic
}

fn main() {
    let qs: [(&str, fn()); 25] = [
        ("Q1", q1),
        ("Q2", q2),
        ("Q3", q3),
        ("Q4", q4),
        ("Q5", q5),
        ("Q6", q6),
        ("Q7", q7),
        ("Q8", q8),
        ("Q9", q9),
        ("Q10", q10),
        ("Q11", q11),
        ("Q12", q12),
        ("Q13", q13),
        ("Q14", q14),
        ("Q15", q15),
        ("Q16", q16),
        ("Q17", q17),
        ("Q18", q18),
        ("Q19", q19),
        ("Q20", q20),
        ("Q21", q21),
        ("Q22", q22),
        ("Q23", q23),
        ("Q24", q24),
        ("Q25", q25),
    ];
    for (name, q) in qs {
        q();
        println!("{name}: ok");
    }
}
