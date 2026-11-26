//! RECIPE 260 (algorithms): procedural puzzles with ASCII pictures: mazes, Sudoku, queens, WFC, tours, Hanoi, Gray codes.
//! LEVEL: advanced
//! Use in: game level generation, constraint solving, combinatorial search, test-data generation, algorithm interviews.   Run: cargo run --release --example procedural_puzzles
//! COMMON USES: roguelike/maze level generators, puzzle apps and solvers (exact cover), constraint propagation in procedural content (wave function collapse), enumerating permutations/subsets in brute-force search, Gray codes in rotary encoders and Karnaugh maps, flood fill/connected components in image processing and paint tools.
#![allow(dead_code)]
//!
//! Each section is a small, classic algorithm with a SELF-CHECK that does not trust the algorithm:
//! mazes (4 generators) are verified to be spanning trees; solvers are verified shortest and unique; Sudoku
//! solutions are validated and solver counts are cross-checked between Dancing Links and a backtracker;
//! N-queens counts must equal the known sequence; WFC output is re-checked cell by cell against the
//! adjacency rules; flood fill is cross-checked against union-find; knight's tours are re-walked;
//! Hanoi, Gray codes and permutation generators are checked against their defining properties.
//! Read: https://en.wikipedia.org/wiki/Maze_generation_algorithm
//! Read: https://en.wikipedia.org/wiki/Dancing_Links
//! Read: https://en.wikipedia.org/wiki/Wave_function_collapse
//! Read: https://cp-algorithms.com/data_structures/disjoint_set_union.html

use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap, VecDeque};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            v.swap(i, self.below(i + 1));
        }
    }
}

struct Dsu {
    parent: Vec<usize>,
    sets: usize,
}
impl Dsu {
    fn new(n: usize) -> Self {
        Dsu {
            parent: (0..n).collect(),
            sets: n,
        }
    }
    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }
    fn union(&mut self, a: usize, b: usize) -> bool {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return false;
        }
        self.parent[rb] = ra;
        self.sets -= 1;
        true
    }
}

// ---------------- 1. mazes ----------------
struct Maze {
    w: usize,
    h: usize,
    open: BTreeSet<(usize, usize)>, // passages, stored as (smaller, larger) cell index
}
impl Maze {
    fn new(w: usize, h: usize) -> Self {
        Maze {
            w,
            h,
            open: BTreeSet::new(),
        }
    }
    fn grid_neighbors(&self, c: usize) -> Vec<usize> {
        let (x, y) = (c % self.w, c / self.w);
        let mut v = Vec::new();
        if y > 0 {
            v.push(c - self.w);
        }
        if x + 1 < self.w {
            v.push(c + 1);
        }
        if y + 1 < self.h {
            v.push(c + self.w);
        }
        if x > 0 {
            v.push(c - 1);
        }
        v
    }
    fn carve(&mut self, a: usize, b: usize) {
        self.open.insert((a.min(b), a.max(b)));
    }
    fn is_open(&self, a: usize, b: usize) -> bool {
        self.open.contains(&(a.min(b), a.max(b)))
    }
    fn moves(&self, c: usize) -> Vec<usize> {
        self.grid_neighbors(c)
            .into_iter()
            .filter(|&n| self.is_open(c, n))
            .collect()
    }
    fn render(&self, path: &[usize]) -> String {
        let on_path: BTreeSet<usize> = path.iter().copied().collect();
        let mut s = String::new();
        for y in 0..self.h {
            for x in 0..self.w {
                s += "+";
                s += if y > 0 && self.is_open(y * self.w + x, (y - 1) * self.w + x) {
                    "   "
                } else {
                    "---"
                };
            }
            s += "+\n";
            for x in 0..self.w {
                let c = y * self.w + x;
                s += if x > 0 && self.is_open(c, c - 1) {
                    " "
                } else {
                    "|"
                };
                s += if on_path.contains(&c) { " . " } else { "   " };
            }
            s += "|\n";
        }
        s + &"+---".repeat(self.w) + "+\n"
    }
    fn dead_ends(&self) -> usize {
        (0..self.w * self.h)
            .filter(|&c| self.moves(c).len() == 1)
            .count()
    }
}

fn backtracker(w: usize, h: usize, rng: &mut Rng) -> Maze {
    let mut m = Maze::new(w, h);
    let mut seen = vec![false; w * h];
    let mut stack = vec![rng.below(w * h)];
    seen[stack[0]] = true;
    while let Some(&cur) = stack.last() {
        let free: Vec<usize> = m
            .grid_neighbors(cur)
            .into_iter()
            .filter(|&n| !seen[n])
            .collect();
        if free.is_empty() {
            stack.pop(); // dead end: backtrack
        } else {
            let n = free[rng.below(free.len())];
            m.carve(cur, n);
            seen[n] = true;
            stack.push(n);
        }
    }
    m
}
fn kruskal_maze(w: usize, h: usize, rng: &mut Rng) -> Maze {
    let mut m = Maze::new(w, h);
    let mut edges: Vec<(usize, usize)> = (0..w * h)
        .flat_map(|c| {
            m.grid_neighbors(c)
                .into_iter()
                .filter(move |&n| n > c)
                .map(move |n| (c, n))
        })
        .collect();
    rng.shuffle(&mut edges);
    let mut d = Dsu::new(w * h);
    for (a, b) in edges {
        if d.union(a, b) {
            m.carve(a, b);
        }
    }
    m
}
fn prim_maze(w: usize, h: usize, rng: &mut Rng) -> Maze {
    let mut m = Maze::new(w, h);
    let mut seen = vec![false; w * h];
    let start = rng.below(w * h);
    seen[start] = true;
    let mut frontier: Vec<(usize, usize)> = m
        .grid_neighbors(start)
        .into_iter()
        .map(|n| (start, n))
        .collect();
    while !frontier.is_empty() {
        let i = rng.below(frontier.len());
        let (a, b) = frontier.swap_remove(i);
        if seen[b] {
            continue;
        }
        m.carve(a, b);
        seen[b] = true;
        frontier.extend(
            m.grid_neighbors(b)
                .into_iter()
                .filter(|&n| !seen[n])
                .map(|n| (b, n)),
        );
    }
    m
}
/// Wilson: loop-erased random walks => a UNIFORMLY random spanning tree.
fn wilson_maze(w: usize, h: usize, rng: &mut Rng) -> Maze {
    let mut m = Maze::new(w, h);
    let n = w * h;
    let mut in_tree = vec![false; n];
    in_tree[rng.below(n)] = true;
    let mut next = vec![usize::MAX; n];
    for start in 0..n {
        let mut cur = start;
        while !in_tree[cur] {
            let nb = m.grid_neighbors(cur);
            next[cur] = nb[rng.below(nb.len())]; // overwriting `next` erases loops
            cur = next[cur];
        }
        cur = start;
        while !in_tree[cur] {
            m.carve(cur, next[cur]);
            in_tree[cur] = true;
            cur = next[cur];
        }
    }
    m
}
fn assert_spanning_tree(m: &Maze) {
    let n = m.w * m.h;
    assert_eq!(m.open.len(), n - 1, "n-1 passages");
    let mut d = Dsu::new(n);
    for &(a, b) in &m.open {
        assert!(d.union(a, b), "no cycle");
    }
    assert_eq!(d.sets, 1, "connected");
}
fn bfs_path(m: &Maze, s: usize, t: usize) -> (Vec<usize>, usize) {
    let mut par = vec![usize::MAX; m.w * m.h];
    par[s] = s;
    let (mut q, mut expanded) = (VecDeque::from([s]), 0);
    while let Some(c) = q.pop_front() {
        expanded += 1;
        if c == t {
            break;
        }
        for nb in m.moves(c) {
            if par[nb] == usize::MAX {
                par[nb] = c;
                q.push_back(nb);
            }
        }
    }
    (unwind(&par, s, t), expanded)
}
fn unwind(par: &[usize], s: usize, t: usize) -> Vec<usize> {
    let mut p = vec![t];
    while *p.last().unwrap() != s {
        p.push(par[*p.last().unwrap()]);
    }
    p.reverse();
    p
}
fn astar_path(m: &Maze, s: usize, t: usize) -> (Vec<usize>, usize) {
    let h = |c: usize| (c % m.w).abs_diff(t % m.w) + (c / m.w).abs_diff(t / m.w);
    let mut g = vec![usize::MAX; m.w * m.h];
    let mut par = vec![usize::MAX; m.w * m.h];
    g[s] = 0;
    let (mut heap, mut expanded) = (BinaryHeap::from([Reverse((h(s), s))]), 0);
    while let Some(Reverse((_, c))) = heap.pop() {
        expanded += 1;
        if c == t {
            break;
        }
        for nb in m.moves(c) {
            if g[c] + 1 < g[nb] {
                g[nb] = g[c] + 1;
                par[nb] = c;
                heap.push(Reverse((g[nb] + h(nb), nb)));
            }
        }
    }
    (unwind(&par, s, t), expanded)
}
fn count_simple_paths(m: &Maze, cur: usize, t: usize, seen: &mut Vec<bool>) -> usize {
    if cur == t {
        return 1;
    }
    seen[cur] = true;
    let mut total = 0;
    for n in m.moves(cur) {
        if !seen[n] {
            total += count_simple_paths(m, n, t, seen);
        }
    }
    seen[cur] = false;
    total
}
fn mazes_section(rng: &mut Rng) {
    type Gen = fn(usize, usize, &mut Rng) -> Maze;
    let gens: [(&str, Gen); 4] = [
        ("backtracker", backtracker),
        ("kruskal", kruskal_maze),
        ("prim", prim_maze),
        ("wilson", wilson_maze),
    ];
    println!(
        "1. mazes: every generator must yield a spanning tree (n-1 passages, connected, acyclic)"
    );
    for (name, g) in gens {
        let mut dead = 0.0;
        for i in 0..60 {
            let (w, h) = (2 + i % 14, 2 + (i * 7) % 11);
            assert_spanning_tree(&g(w, h, rng));
        }
        for _ in 0..20 {
            dead += g(20, 20, rng).dead_ends() as f64 / 400.0;
        }
        println!(
            "   {name:<12} 60 random sizes OK; dead-end fraction on 20x20: {:.2}",
            dead / 20.0
        );
    }
    let m = backtracker(14, 6, rng);
    let t = 14 * 6 - 1;
    let (pb, eb) = bfs_path(&m, 0, t);
    let (pa, ea) = astar_path(&m, 0, t);
    let mut seen = vec![false; 14 * 6];
    assert_eq!(
        count_simple_paths(&m, 0, t, &mut seen),
        1,
        "perfect maze: exactly one route"
    );
    assert_eq!(pb, pa, "unique path, so BFS and A* agree");
    print!(
        "{}",
        m.render(&pb)
            .lines()
            .map(|l| format!("   {l}\n"))
            .collect::<String>()
    );
    println!(
        "   solved: path length {} (BFS expanded {eb} cells, A* {ea})",
        pb.len() - 1
    );
    // braid: knock out 8 walls -> loops; now shortest != only path, A* must still match BFS
    let mut loops = backtracker(20, 10, rng);
    for _ in 0..8 {
        let c = rng.below(200);
        let nb = loops.grid_neighbors(c);
        loops.carve(c, nb[rng.below(nb.len())]);
    }
    let (p1, p2) = (bfs_path(&loops, 0, 199).0, astar_path(&loops, 0, 199).0);
    assert_eq!(
        p1.len(),
        p2.len(),
        "A* with an admissible heuristic is optimal"
    );
    assert!(p1.windows(2).all(|w| loops.is_open(w[0], w[1])));
}

// ---------------- 2. sudoku ----------------
/// Dancing Links (Knuth's Algorithm X) for exact cover.
struct Dlx {
    l: Vec<usize>,
    r: Vec<usize>,
    u: Vec<usize>,
    d: Vec<usize>,
    col: Vec<usize>,
    size: Vec<usize>,
    row: Vec<usize>,
}
impl Dlx {
    fn new(cols: usize) -> Self {
        let mut x = Dlx {
            l: vec![],
            r: vec![],
            u: vec![],
            d: vec![],
            col: vec![],
            size: vec![0; cols + 1],
            row: vec![],
        };
        for i in 0..=cols {
            x.l.push(if i == 0 { cols } else { i - 1 });
            x.r.push(if i == cols { 0 } else { i + 1 });
            x.u.push(i);
            x.d.push(i);
            x.col.push(i);
            x.row.push(usize::MAX);
        }
        x
    }
    fn add_row(&mut self, id: usize, cols: &[usize]) {
        let mut first = usize::MAX;
        for &c in cols {
            let (n, h) = (self.l.len(), c + 1);
            self.col.push(h);
            self.row.push(id);
            self.u.push(self.u[h]);
            self.d.push(h);
            let up = self.u[h];
            self.d[up] = n;
            self.u[h] = n;
            self.size[h] += 1;
            if first == usize::MAX {
                self.l.push(n);
                self.r.push(n);
                first = n;
            } else {
                let last = self.l[first];
                self.l.push(last);
                self.r.push(first);
                self.r[last] = n;
                self.l[first] = n;
            }
        }
    }
    fn cover(&mut self, c: usize) {
        let (lc, rc) = (self.l[c], self.r[c]);
        self.r[lc] = rc;
        self.l[rc] = lc;
        let mut i = self.d[c];
        while i != c {
            let mut j = self.r[i];
            while j != i {
                let (uj, dj) = (self.u[j], self.d[j]);
                self.d[uj] = dj;
                self.u[dj] = uj;
                self.size[self.col[j]] -= 1;
                j = self.r[j];
            }
            i = self.d[i];
        }
    }
    fn uncover(&mut self, c: usize) {
        let mut i = self.u[c];
        while i != c {
            let mut j = self.l[i];
            while j != i {
                self.size[self.col[j]] += 1;
                let (uj, dj) = (self.u[j], self.d[j]);
                self.d[uj] = j;
                self.u[dj] = j;
                j = self.l[j];
            }
            i = self.u[i];
        }
        let (lc, rc) = (self.l[c], self.r[c]);
        self.r[lc] = c;
        self.l[rc] = c;
    }
    fn search(
        &mut self,
        sol: &mut Vec<usize>,
        limit: usize,
        count: &mut usize,
        first: &mut Option<Vec<usize>>,
    ) {
        if self.r[0] == 0 {
            *count += 1;
            first.get_or_insert_with(|| sol.clone());
            return;
        }
        let (mut c, mut j) = (self.r[0], self.r[0]);
        while j != 0 {
            if self.size[j] < self.size[c] {
                c = j; // Knuth's S heuristic: branch on the column with fewest candidates
            }
            j = self.r[j];
        }
        if self.size[c] == 0 {
            return;
        }
        self.cover(c);
        let mut rr = self.d[c];
        while rr != c {
            sol.push(self.row[rr]);
            let mut j = self.r[rr];
            while j != rr {
                self.cover(self.col[j]);
                j = self.r[j];
            }
            self.search(sol, limit, count, first);
            let mut j = self.l[rr];
            while j != rr {
                self.uncover(self.col[j]);
                j = self.l[j];
            }
            sol.pop();
            if *count >= limit {
                break;
            }
            rr = self.d[rr];
        }
        self.uncover(c);
    }
}
type Grid = [u8; 81];
fn parse(s: &str) -> Grid {
    let mut g = [0; 81];
    for (i, ch) in s.bytes().enumerate() {
        g[i] = ch - b'0';
    }
    g
}
/// Returns (number of solutions up to `limit`, first solution).
fn sudoku_dlx(g: &Grid, limit: usize) -> (usize, Option<Grid>) {
    let mut x = Dlx::new(324);
    for r in 0..9 {
        for c in 0..9 {
            for d in 0..9 {
                let given = g[r * 9 + c];
                if given == 0 || given as usize == d + 1 {
                    x.add_row(
                        r * 81 + c * 9 + d,
                        &[
                            r * 9 + c,
                            81 + r * 9 + d,
                            162 + c * 9 + d,
                            243 + ((r / 3) * 3 + c / 3) * 9 + d,
                        ],
                    );
                }
            }
        }
    }
    let (mut count, mut first) = (0, None);
    x.search(&mut Vec::new(), limit, &mut count, &mut first);
    let grid = first.map(|rows| {
        let mut out = [0u8; 81];
        for id in rows {
            out[id / 9] = (id % 9) as u8 + 1;
        }
        out
    });
    (count, grid)
}
/// Plain backtracking with candidate bitmasks and "fewest candidates first".
fn sudoku_bt(g: &mut Grid, limit: usize, count: &mut usize, first: &mut Option<Grid>) {
    let mut best = (usize::MAX, 10u32, 0u16);
    for i in 0..81 {
        if g[i] == 0 {
            let mut used = 0u16;
            for k in 0..81 {
                let (same_row, same_col) = (k / 9 == i / 9, k % 9 == i % 9);
                let same_box = (k / 27, (k % 9) / 3) == (i / 27, (i % 9) / 3);
                if g[k] != 0 && (same_row || same_col || same_box) {
                    used |= 1 << g[k];
                }
            }
            let cand = !used & 0x3FE;
            if cand.count_ones() < best.1 {
                best = (i, cand.count_ones(), cand);
            }
        }
    }
    if best.0 == usize::MAX {
        *count += 1;
        first.get_or_insert(*g);
        return;
    }
    for d in 1..=9u8 {
        if best.2 >> d & 1 == 1 {
            g[best.0] = d;
            sudoku_bt(g, limit, count, first);
            g[best.0] = 0;
            if *count >= limit {
                return;
            }
        }
    }
}
fn valid_solution(g: &Grid, puzzle: &Grid) -> bool {
    let full = |it: &mut dyn Iterator<Item = usize>| it.fold(0u16, |m, i| m | 1 << g[i]) == 0x3FE;
    (0..9).all(|k| {
        full(&mut (0..9).map(|c| k * 9 + c))
            && full(&mut (0..9).map(|r| r * 9 + k))
            && full(&mut (0..9).map(|i| (k / 3 * 3 + i / 3) * 9 + k % 3 * 3 + i % 3))
    }) && (0..81).all(|i| puzzle[i] == 0 || puzzle[i] == g[i])
}
fn show_grid(g: &Grid) {
    for r in 0..9 {
        if r % 3 == 0 {
            println!("   +-------+-------+-------+");
        }
        let row: Vec<String> = (0..9)
            .map(|c| format!("{}{}", if c % 3 == 0 { "| " } else { "" }, g[r * 9 + c]))
            .collect();
        println!("   {} |", row.join(" "));
    }
    println!("   +-------+-------+-------+");
}
fn sudoku_section() {
    let puzzles = [
        "530070000600195000098000060800060003400803001700020006060000280000419005000080079",
        "800000000003600000070090200050007000000045700000100030001000068008500010090000400",
    ];
    let mut solved = Vec::new();
    for (k, s) in puzzles.iter().enumerate() {
        let p = parse(s);
        let (n, sol) = sudoku_dlx(&p, 2);
        let (mut count, mut first, mut q) = (0, None, p);
        sudoku_bt(&mut q, 2, &mut count, &mut first);
        let sol = sol.unwrap();
        assert!(n == 1 && count == 1, "unique solution by both solvers");
        assert!(
            valid_solution(&sol, &p) && valid_solution(&first.unwrap(), &p) && first == Some(sol)
        );
        println!(
            "2. sudoku #{k}: DLX and backtracker agree; solution valid ({} givens)",
            p.iter().filter(|&&v| v != 0).count()
        );
        solved.push(sol);
    }
    show_grid(&solved[0]);
    // a "deadly rectangle": blank four cells that can be swapped -> exactly two solutions
    let mut tested = false;
    'outer: for sol in &solved {
        for r0 in 0..9 {
            for r1 in r0 + 1..(r0 / 3 * 3 + 3) {
                for c0 in 0..9 {
                    for c1 in c0 + 1..9 {
                        if sol[r0 * 9 + c0] == sol[r1 * 9 + c1]
                            && sol[r0 * 9 + c1] == sol[r1 * 9 + c0]
                        {
                            let mut p = *sol;
                            for k in [r0 * 9 + c0, r0 * 9 + c1, r1 * 9 + c0, r1 * 9 + c1] {
                                p[k] = 0;
                            }
                            let (n, _) = sudoku_dlx(&p, 100);
                            let (mut cnt, mut first, mut q) = (0, None, p);
                            sudoku_bt(&mut q, 100, &mut cnt, &mut first);
                            assert_eq!((n, cnt), (2, 2));
                            println!(
                                "   deadly rectangle at rows {r0},{r1} cols {c0},{c1}: both solvers count exactly {n} solutions"
                            );
                            tested = true;
                            break 'outer;
                        }
                    }
                }
            }
        }
    }
    assert!(tested);
    let (n, g) = sudoku_dlx(&[0; 81], 1000);
    assert!(n == 1000 && valid_solution(&g.unwrap(), &[0; 81]));
    println!(
        "   empty grid: DLX stops at the 1000-solution cap (real count 6.67e21), first one valid"
    );
}

// ---------------- 3. n-queens ----------------
fn queens(
    n: usize,
    cols: u32,
    d1: u32,
    d2: u32,
    row: usize,
    placed: &mut Vec<usize>,
    first: &mut Option<Vec<usize>>,
) -> u64 {
    if row == n {
        first.get_or_insert_with(|| placed.clone());
        return 1;
    }
    let mut free = !(cols | d1 | d2) & ((1 << n) - 1);
    let mut total = 0;
    while free != 0 {
        let bit = free & free.wrapping_neg(); // lowest set bit
        free ^= bit;
        placed.push(bit.trailing_zeros() as usize);
        total += queens(
            n,
            cols | bit,
            (d1 | bit) << 1,
            (d2 | bit) >> 1,
            row + 1,
            placed,
            first,
        );
        placed.pop();
    }
    total
}
fn queens_section() {
    let known = [1u64, 0, 0, 2, 10, 4, 40, 92, 352, 724];
    for (i, &want) in known.iter().enumerate() {
        assert_eq!(queens(i + 1, 0, 0, 0, 0, &mut Vec::new(), &mut None), want);
    }
    let (mut first, c12) = (None, queens(12, 0, 0, 0, 0, &mut Vec::new(), &mut None));
    assert_eq!(c12, 14200);
    queens(8, 0, 0, 0, 0, &mut Vec::new(), &mut first);
    let q = first.unwrap();
    assert!((0..8).all(|i| (i + 1..8).all(|j| q[i] != q[j] && q[i].abs_diff(q[j]) != j - i)));
    println!("3. n-queens counts n=1..10 = {known:?}, n=12 -> {c12}; first 8-queens solution:");
    for r in 0..8 {
        println!(
            "   {}",
            (0..8)
                .map(|c| if q[r] == c {
                    "Q"
                } else if (r + c) % 2 == 0 {
                    "."
                } else {
                    " "
                })
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
}

// ---------------- 4. wave function collapse ----------------
// sockets [N, E, S, W]: 1 = a pipe leaves through that side
const TILES: [(char, [u8; 4], u32); 12] = [
    (' ', [0, 0, 0, 0], 5),
    ('─', [0, 1, 0, 1], 2),
    ('│', [1, 0, 1, 0], 2),
    ('┌', [0, 1, 1, 0], 1),
    ('┐', [0, 0, 1, 1], 1),
    ('└', [1, 1, 0, 0], 1),
    ('┘', [1, 0, 0, 1], 1),
    ('├', [1, 1, 1, 0], 1),
    ('┤', [1, 0, 1, 1], 1),
    ('┬', [0, 1, 1, 1], 1),
    ('┴', [1, 1, 0, 1], 1),
    ('┼', [1, 1, 1, 1], 1),
];
const DIRS: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)]; // N E S W
fn compat() -> [[u16; 12]; 4] {
    let mut c = [[0u16; 12]; 4];
    for d in 0..4 {
        for t in 0..12 {
            for u in 0..12 {
                if TILES[t].1[d] == TILES[u].1[(d + 2) % 4] {
                    c[d][t] |= 1 << u;
                }
            }
        }
    }
    c
}
fn propagate(
    dom: &mut [u16],
    w: usize,
    h: usize,
    stack: &mut Vec<usize>,
    comp: &[[u16; 12]; 4],
) -> bool {
    while let Some(cell) = stack.pop() {
        for (d, (dx, dy)) in DIRS.iter().enumerate() {
            let (nx, ny) = ((cell % w) as i32 + dx, (cell / w) as i32 + dy);
            if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                continue;
            }
            let nb = ny as usize * w + nx as usize;
            let allowed = (0..12)
                .filter(|&t| dom[cell] >> t & 1 == 1)
                .fold(0u16, |m, t| m | comp[d][t]);
            let new = dom[nb] & allowed;
            if new != dom[nb] {
                if new == 0 {
                    return false; // contradiction
                }
                dom[nb] = new;
                stack.push(nb);
            }
        }
    }
    true
}
/// Returns the tiles and the number of restarts it took.
fn wfc(w: usize, h: usize, rng: &mut Rng) -> (Vec<usize>, usize) {
    let comp = compat();
    'restart: for restarts in 0.. {
        let mut dom = vec![0xFFFu16; w * h];
        for (cell, dm) in dom.iter_mut().enumerate() {
            let (x, y) = (cell % w, cell / w);
            for (d, edge) in [(0, y == 0), (1, x == w - 1), (2, y == h - 1), (3, x == 0)] {
                if edge {
                    *dm &= (0..12)
                        .filter(|&t| TILES[t].1[d] == 0)
                        .fold(0u16, |m, t| m | 1 << t);
                }
            }
        }
        if !propagate(&mut dom, w, h, &mut (0..w * h).collect(), &comp) {
            continue;
        }
        loop {
            // lowest-entropy cell (fewest options), random tie-break
            let open: Vec<usize> = (0..w * h).filter(|&i| dom[i].count_ones() > 1).collect();
            let Some(min) = open.iter().map(|&i| dom[i].count_ones()).min() else {
                return (
                    dom.iter().map(|d| d.trailing_zeros() as usize).collect(),
                    restarts,
                );
            };
            let cands: Vec<usize> = open
                .into_iter()
                .filter(|&i| dom[i].count_ones() == min)
                .collect();
            let cell = cands[rng.below(cands.len())];
            let opts: Vec<usize> = (0..12).filter(|&t| dom[cell] >> t & 1 == 1).collect();
            let total: u32 = opts.iter().map(|&t| TILES[t].2).sum();
            let mut roll = rng.below(total as usize) as u32;
            let pick = *opts
                .iter()
                .find(|&&t| {
                    if roll < TILES[t].2 {
                        true
                    } else {
                        roll -= TILES[t].2;
                        false
                    }
                })
                .unwrap();
            dom[cell] = 1 << pick;
            if !propagate(&mut dom, w, h, &mut vec![cell], &comp) {
                continue 'restart;
            }
        }
    }
    unreachable!()
}
fn wfc_section(rng: &mut Rng) {
    let (mut restarts, mut last) = (0, (Vec::new(), 0, 0));
    for i in 0..60 {
        let (w, h) = (6 + i % 12, 4 + i % 7);
        let (t, r) = wfc(w, h, rng);
        restarts += r;
        for y in 0..h {
            for x in 0..w {
                let s = TILES[t[y * w + x]].1;
                assert!(
                    x + 1 == w && s[1] == 0 || x + 1 < w && s[1] == TILES[t[y * w + x + 1]].1[3],
                    "E-W socket match"
                );
                assert!(
                    y + 1 == h && s[2] == 0 || y + 1 < h && s[2] == TILES[t[(y + 1) * w + x]].1[0],
                    "N-S socket match"
                );
                assert!(x > 0 || s[3] == 0);
                assert!(y > 0 || s[0] == 0);
            }
        }
        last = (t, w, h);
    }
    println!(
        "4. wave function collapse: 60 pipe maps (sockets re-verified independently), {restarts} restarts after contradictions; last map:"
    );
    for y in 0..last.2 {
        println!(
            "   {}",
            (0..last.1)
                .map(|x| TILES[last.0[y * last.1 + x]].0)
                .collect::<String>()
        );
    }
}

// ---------------- 5. islands ----------------
fn islands_bfs(g: &[Vec<bool>]) -> (usize, Vec<Vec<usize>>) {
    let (h, w) = (g.len(), g[0].len());
    let mut label = vec![vec![0usize; w]; h];
    let mut n = 0;
    for y in 0..h {
        for x in 0..w {
            if g[y][x] && label[y][x] == 0 {
                n += 1;
                label[y][x] = n;
                let mut q = VecDeque::from([(x, y)]);
                while let Some((cx, cy)) = q.pop_front() {
                    for (dx, dy) in [(0i32, 1i32), (1, 0), (0, -1), (-1, 0)] {
                        let (nx, ny) = (cx as i32 + dx, cy as i32 + dy);
                        if nx >= 0
                            && ny >= 0
                            && (nx as usize) < w
                            && (ny as usize) < h
                            && g[ny as usize][nx as usize]
                            && label[ny as usize][nx as usize] == 0
                        {
                            label[ny as usize][nx as usize] = n;
                            q.push_back((nx as usize, ny as usize));
                        }
                    }
                }
            }
        }
    }
    (n, label)
}
fn islands_dsu(g: &[Vec<bool>]) -> usize {
    let (h, w) = (g.len(), g[0].len());
    let mut d = Dsu::new(w * h);
    let mut land = 0;
    for y in 0..h {
        for x in 0..w {
            if g[y][x] {
                land += 1;
                if x + 1 < w && g[y][x + 1] {
                    d.union(y * w + x, y * w + x + 1);
                }
                if y + 1 < h && g[y + 1][x] {
                    d.union(y * w + x, (y + 1) * w + x);
                }
            }
        }
    }
    d.sets - (w * h - land) // every water cell is its own singleton set
}
fn islands_section(rng: &mut Rng) {
    for _ in 0..300 {
        let (w, h) = (1 + rng.below(30), 1 + rng.below(20));
        let g: Vec<Vec<bool>> = (0..h)
            .map(|_| (0..w).map(|_| rng.below(100) < 45).collect())
            .collect();
        assert_eq!(islands_bfs(&g).0, islands_dsu(&g));
    }
    let g: Vec<Vec<bool>> = (0..10)
        .map(|_| (0..44).map(|_| rng.below(100) < 42).collect())
        .collect();
    let (n, label) = islands_bfs(&g);
    assert_eq!(n, islands_dsu(&g));
    println!("5. flood fill == union-find on 300 random grids; this 44x10 map has {n} islands:");
    for row in &label {
        println!(
            "   {}",
            row.iter()
                .map(|&l| if l == 0 {
                    '~'
                } else {
                    (b'a' + ((l - 1) % 26) as u8) as char
                })
                .collect::<String>()
        );
    }
}

// ---------------- 6. knight's tour ----------------
fn knight_moves(p: (i32, i32), n: i32) -> Vec<(i32, i32)> {
    [
        (1, 2),
        (2, 1),
        (2, -1),
        (1, -2),
        (-1, -2),
        (-2, -1),
        (-2, 1),
        (-1, 2),
    ]
    .iter()
    .map(|d| (p.0 + d.0, p.1 + d.1))
    .filter(|q| q.0 >= 0 && q.1 >= 0 && q.0 < n && q.1 < n)
    .collect()
}
fn warnsdorff(n: i32, start: (i32, i32), rng: &mut Rng) -> Option<Vec<(i32, i32)>> {
    let mut seen = vec![vec![false; n as usize]; n as usize];
    let (mut cur, mut path) = (start, vec![start]);
    seen[start.1 as usize][start.0 as usize] = true;
    for _ in 1..n * n {
        let opts: Vec<((i32, i32), usize)> = knight_moves(cur, n)
            .into_iter()
            .filter(|q| !seen[q.1 as usize][q.0 as usize])
            .map(|q| {
                (
                    q,
                    knight_moves(q, n)
                        .iter()
                        .filter(|r| !seen[r.1 as usize][r.0 as usize])
                        .count(),
                )
            })
            .collect();
        let best = opts.iter().map(|o| o.1).min()?; // None => stuck
        let ties: Vec<_> = opts.iter().filter(|o| o.1 == best).collect();
        cur = ties[rng.below(ties.len())].0;
        seen[cur.1 as usize][cur.0 as usize] = true;
        path.push(cur);
    }
    Some(path)
}
fn knight_section(rng: &mut Rng) {
    let mut total_tries = 0;
    let mut sample = None;
    for sx in 0..8 {
        for sy in 0..8 {
            let tour = loop {
                total_tries += 1;
                if let Some(t) = warnsdorff(8, (sx, sy), rng) {
                    break t;
                }
            };
            let set: BTreeSet<_> = tour.iter().collect();
            assert!(
                tour.len() == 64
                    && set.len() == 64
                    && tour
                        .windows(2)
                        .all(|w| knight_moves(w[0], 8).contains(&w[1]))
            );
            if (sx, sy) == (0, 0) {
                sample = Some(tour);
            }
        }
    }
    println!(
        "6. knight's tour (Warnsdorff, fewest-onward-moves first): all 64 start squares tour the board ({total_tries} attempts total); from a1:"
    );
    let mut b = [[0; 8]; 8];
    for (i, p) in sample.unwrap().iter().enumerate() {
        b[p.1 as usize][p.0 as usize] = i + 1;
    }
    for row in b {
        println!(
            "   {}",
            row.iter().map(|v| format!("{v:>3}")).collect::<String>()
        );
    }
}

// ---------------- 7. hanoi, 8. gray ----------------
fn hanoi(n: u32, from: usize, to: usize, via: usize, out: &mut Vec<(u32, usize, usize)>) {
    if n == 0 {
        return;
    }
    hanoi(n - 1, from, via, to, out);
    out.push((n, from, to));
    hanoi(n - 1, via, to, from, out);
}
fn draw_pegs(p: &[Vec<u32>; 3], n: u32) {
    for level in (0..n as usize).rev() {
        let row: Vec<String> = p
            .iter()
            .map(|peg| {
                let d = peg.get(level).copied().unwrap_or(0) as usize;
                format!(
                    "{:^width$}",
                    format!("{}|{}", "=".repeat(d), "=".repeat(d)),
                    width = 2 * n as usize + 3
                )
            })
            .collect();
        println!("   {}", row.join(" "));
    }
}
fn hanoi_gray_section() {
    for n in 1..=16u32 {
        let mut moves = Vec::new();
        hanoi(n, 0, 2, 1, &mut moves);
        assert_eq!(moves.len(), (1usize << n) - 1);
        let mut pegs: [Vec<u32>; 3] = [(1..=n).rev().collect(), vec![], vec![]];
        for &(d, f, t) in &moves {
            assert_eq!(pegs[f].pop(), Some(d), "moves the top disk");
            assert!(
                pegs[t].last().is_none_or(|&top| top > d),
                "never a big disk onto a small one"
            );
            pegs[t].push(d);
        }
        assert!(pegs[0].is_empty() && pegs[1].is_empty() && pegs[2].len() == n as usize);
    }
    println!(
        "7. Hanoi: 2^n-1 legal moves verified for n=1..16. Start and finish for n=4 (15 moves):"
    );
    let mut pegs: [Vec<u32>; 3] = [vec![4, 3, 2, 1], vec![], vec![]];
    draw_pegs(&pegs, 4);
    let mut m = Vec::new();
    hanoi(4, 0, 2, 1, &mut m);
    for (d, f, t) in m {
        pegs[f].pop();
        pegs[t].push(d);
    }
    println!("   {}", "-".repeat(3 * (2 * 4 + 3) + 2));
    draw_pegs(&pegs, 4);

    for bits in 1..=16u32 {
        let codes: Vec<u32> = (0..1u32 << bits).map(|i| i ^ (i >> 1)).collect();
        assert!(codes.windows(2).all(|w| (w[0] ^ w[1]).count_ones() == 1));
        assert_eq!(
            (codes[0] ^ codes[codes.len() - 1]).count_ones(),
            1,
            "cyclic"
        );
        assert_eq!(codes.iter().collect::<BTreeSet<_>>().len(), codes.len());
        for (i, &g) in codes.iter().enumerate() {
            let (mut v, mut s) = (g, g >> 1);
            while s != 0 {
                v ^= s;
                s >>= 1;
            }
            assert_eq!(v as usize, i, "inverse by prefix xor");
        }
    }
    println!(
        "8. Gray codes: single-bit steps, cyclic, bijective, invertible for 1..16 bits. 3-bit: {}",
        (0..8u32)
            .map(|i| format!("{:03b}", i ^ (i >> 1)))
            .collect::<Vec<_>>()
            .join(" ")
    );
}

// ---------------- 9. permutations & combinations ----------------
fn heap_perms(n: usize) -> Vec<(Vec<usize>, Option<(usize, usize)>)> {
    let mut a: Vec<usize> = (0..n).collect();
    let mut out = vec![(a.clone(), None)];
    let mut c = vec![0; n];
    let mut i = 0;
    while i < n {
        if c[i] < i {
            let j = if i % 2 == 0 { 0 } else { c[i] };
            a.swap(j, i);
            out.push((a.clone(), Some((j, i))));
            c[i] += 1;
            i = 0;
        } else {
            c[i] = 0;
            i += 1;
        }
    }
    out
}
fn next_permutation<T: Ord>(v: &mut [T]) -> bool {
    let Some(i) = (1..v.len()).rev().find(|&i| v[i - 1] < v[i]) else {
        v.reverse(); // last permutation: wrap to the first, like C++ std::next_permutation
        return false;
    };
    let j = (i..v.len()).rev().find(|&j| v[j] > v[i - 1]).unwrap();
    v.swap(i - 1, j);
    v[i..].reverse();
    true
}
/// Gosper's hack: next integer with the same popcount.
fn next_combination(x: u64) -> u64 {
    let c = x & x.wrapping_neg();
    let r = x + c;
    (((r ^ x) >> 2) / c) | r
}
fn perms_section() {
    let fact = |n: usize| (1..=n).product::<usize>();
    for n in 1..=7 {
        let hp = heap_perms(n);
        assert_eq!(hp.len(), fact(n));
        assert_eq!(
            hp.iter().map(|p| &p.0).collect::<BTreeSet<_>>().len(),
            fact(n),
            "all distinct"
        );
        assert!(
            hp.windows(2)
                .all(|w| w[0].0.iter().zip(&w[1].0).filter(|(a, b)| a != b).count() == 2),
            "each step is one swap"
        );
        let mut lex = Vec::new();
        let mut v: Vec<usize> = (0..n).collect();
        loop {
            lex.push(v.clone());
            if !next_permutation(&mut v) {
                break;
            }
        }
        assert_eq!(v, (0..n).collect::<Vec<_>>(), "wraps to the sorted order");
        let mut sorted: Vec<Vec<usize>> = hp.into_iter().map(|p| p.0).collect();
        sorted.sort();
        assert_eq!(lex, sorted, "next_permutation enumerates in sorted order");
    }
    let mut ms = vec![1, 1, 2, 2, 3];
    let mut count = 1;
    while next_permutation(&mut ms) {
        count += 1;
    }
    assert_eq!(count, 30, "5!/(2!2!1!) distinct arrangements of a multiset");
    for n in 1..=16u32 {
        for k in 0..=n {
            let mut x = (1u64 << k) - 1;
            let mut cnt = 0u64;
            while x < 1 << n {
                assert_eq!(x.count_ones(), k);
                cnt += 1;
                if k == 0 {
                    break;
                }
                x = next_combination(x);
            }
            let binom = (0..k as u64).fold(1u64, |b, i| b * (n as u64 - i) / (i + 1));
            assert_eq!(cnt, binom, "C({n},{k})");
        }
    }
    println!(
        "9. permutations: Heap's (one swap per step, all n! distinct) and lexicographic next_permutation agree for n<=7; multiset count 30; Gosper's hack counts C(n,k) for n<=16"
    );
    let h = heap_perms(3);
    println!(
        "   Heap's algorithm n=3: {}",
        h.iter()
            .map(|(p, s)| format!(
                "{}{}",
                p.iter().map(|d| d.to_string()).collect::<String>(),
                s.map_or(String::new(), |(a, b)| format!("(swap {a},{b})"))
            ))
            .collect::<Vec<_>>()
            .join(" -> ")
    );
    println!("   3-subsets of 5 as bitmasks: {}", {
        let mut x = 0b111u64;
        let mut v = vec![];
        while x < 32 {
            v.push(format!("{x:05b}"));
            x = next_combination(x);
        }
        v.join(" ")
    });
}

fn main() {
    let mut rng = Rng(0xA0761D6478BD642F);
    mazes_section(&mut rng);
    sudoku_section();
    queens_section();
    wfc_section(&mut rng);
    islands_section(&mut rng);
    knight_section(&mut rng);
    hanoi_gray_section();
    perms_section();
    println!("all puzzle checks passed");
}
