//! RECIPE 277 (geometry): spatial search structures: k-d tree, quadtree, STR R-tree, grid hash, BK-tree, VP-tree.
//! LEVEL: advanced
//! Use in: GIS and map queries, game collision/neighbour search, nearest-neighbour ML features, fuzzy matching and spell checking.   Run: cargo run --release --example spatial_search_structures
#![allow(dead_code)]
//!
//! WHAT: six indexes that answer "what is near this?" without scanning everything. Every one is verified against a
//! brute-force oracle on seeded random data, and the work done is COUNTED (nodes visited / distance evaluations) so the
//! speed-up is a number, not a claim.
//!  1. K-D TREE: median split on a cycling axis (select_nth, implicit balanced layout in one index array). Nearest neighbour
//!     with branch pruning (skip the far side when the splitting plane is farther than the best so far), k-nearest with a
//!     bounded max-heap, orthogonal range (box) query. The CURSE OF DIMENSIONALITY table (d = 2, 5, 10, 20) shows pruning
//!     collapsing: in 2-D a query touches a tiny fraction of n, in 20-D nearly every node, because in high dimensions all
//!     points are almost equally far away and a sphere around the query crosses almost every splitting plane.
//!  2. REGION QUADTREE for 2-D points: a leaf splits into four equal quadrants at a capacity; range and nearest queries.
//!  3. R-TREE bulk-loaded with SORT-TILE-RECURSIVE (STR): sort by x into vertical slices, sort each slice by y, pack M per
//!     node, repeat on the node boxes. Stores rectangles (not points); window query equals a linear scan.
//!  4. UNIFORM GRID / SPATIAL HASH with cell size = the query radius: a fixed-radius neighbour query inspects 3x3 cells.
//!  5. BK-TREE for a discrete metric (Levenshtein): children keyed by distance to the parent; the triangle inequality
//!     lets a radius-r query visit only children whose key is within r of the query's distance. "Words within 2 edits".
//!  6. VP-TREE (vantage point): split by distance to a pivot (inside / outside radius mu); needs only a metric, so the same
//!     code indexes Euclidean vectors and strings.
//!
//! WHICH INDEX FOR WHICH WORKLOAD (printed at the end too): low-dimensional static points -> k-d tree; points inserted
//! over time in 2-D/3-D maps -> quadtree/octree; rectangles/polygons on disk -> R-tree (STR for static, R*-tree for
//! dynamic); many moving objects with a known interaction radius (games, particles) -> grid; strings/edit distance ->
//! BK-tree; arbitrary metric, moderate intrinsic dimension -> VP-tree; high-dimensional embeddings (50-1000 dims) ->
//! approximate indexes, see ivf_flat (IVF-Flat), hnsw_lite (HNSW), vamana_diskann_lite (Vamana/DiskANN), product_quantization (product quantization).
//!
//! COMMON USES: GIS bounding-box queries and spatial joins in geospatial databases (R-tree family, as in PostGIS GiST
//! indexes and SQLite's R*Tree module); collision culling and neighbour lookup in games and particle simulations (grids and
//! quadtrees); nearest-neighbour lookups in point-cloud processing and k-NN classifiers (k-d trees, as in scikit-learn
//! and SciPy); spell-check suggestions and near-duplicate detection (BK-trees); metric-space search over fingerprints.
//! Read: The Art of Computer Programming Vol 3 (Sorting and Searching), 6.5 "Retrieval on Secondary Keys"
//! Read: The Algorithm Design Manual (Skiena), catalog entries "Kd-Trees" and "Nearest Neighbor Search"
//! Read: https://en.wikipedia.org/wiki/K-d_tree
//! Read: https://en.wikipedia.org/wiki/R-tree
//! Read: https://en.wikipedia.org/wiki/BK-tree
//! Read: https://en.wikipedia.org/wiki/Vantage-point_tree
use std::collections::{BinaryHeap, HashMap, HashSet};

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn dist2(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum()
}
fn random_points(rng: &mut Rng, n: usize, dim: usize) -> Vec<f64> {
    (0..n * dim).map(|_| rng.unit()).collect()
}

// ================================================================= 1. k-d tree
struct KdTree<'a> {
    pts: &'a [f64],
    dim: usize,
    idx: Vec<usize>, // idx[mid] of every slice is that subtree's node
}
impl<'a> KdTree<'a> {
    fn build(pts: &'a [f64], dim: usize) -> Self {
        let n = pts.len() / dim;
        let mut t = KdTree {
            pts,
            dim,
            idx: (0..n).collect(),
        };
        t.split(0, n, 0);
        t
    }
    fn p(&self, i: usize) -> &[f64] {
        &self.pts[i * self.dim..(i + 1) * self.dim]
    }
    fn split(&mut self, lo: usize, hi: usize, depth: usize) {
        if hi - lo <= 1 {
            return;
        }
        let (axis, mid, pts, dim) = (depth % self.dim, (lo + hi) / 2, self.pts, self.dim);
        self.idx[lo..hi].select_nth_unstable_by(mid - lo, |&a, &b| {
            pts[a * dim + axis].total_cmp(&pts[b * dim + axis])
        });
        self.split(lo, mid, depth + 1);
        self.split(mid + 1, hi, depth + 1);
    }
    fn nearest(&self, q: &[f64]) -> (f64, usize, u64) {
        let mut best = (f64::INFINITY, usize::MAX);
        let mut visited = 0;
        self.nn(0, self.idx.len(), 0, q, &mut best, &mut visited);
        (best.0, best.1, visited)
    }
    fn nn(
        &self,
        lo: usize,
        hi: usize,
        depth: usize,
        q: &[f64],
        best: &mut (f64, usize),
        visited: &mut u64,
    ) {
        if lo >= hi {
            return;
        }
        let (axis, mid) = (depth % self.dim, (lo + hi) / 2);
        let i = self.idx[mid];
        *visited += 1;
        let d = dist2(self.p(i), q);
        if d < best.0 {
            *best = (d, i);
        }
        let diff = q[axis] - self.p(i)[axis];
        let ((nl, nh), (fl, fh)) = if diff < 0.0 {
            ((lo, mid), (mid + 1, hi))
        } else {
            ((mid + 1, hi), (lo, mid))
        };
        self.nn(nl, nh, depth + 1, q, best, visited);
        if diff * diff < best.0 {
            self.nn(fl, fh, depth + 1, q, best, visited);
        }
    }
    /// k nearest as (dist2, index), nearest first, plus nodes visited.
    fn knn(&self, q: &[f64], k: usize) -> (Vec<(f64, usize)>, u64) {
        let mut heap: BinaryHeap<(u64, usize)> = BinaryHeap::new(); // max-heap on dist2 bits (positive f64 sorts like u64)
        let mut visited = 0;
        self.knn_rec(0, self.idx.len(), 0, q, k, &mut heap, &mut visited);
        let mut v: Vec<(f64, usize)> = heap
            .into_iter()
            .map(|(b, i)| (f64::from_bits(b), i))
            .collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        (v, visited)
    }
    #[allow(clippy::too_many_arguments)]
    fn knn_rec(
        &self,
        lo: usize,
        hi: usize,
        depth: usize,
        q: &[f64],
        k: usize,
        heap: &mut BinaryHeap<(u64, usize)>,
        visited: &mut u64,
    ) {
        if lo >= hi {
            return;
        }
        let (axis, mid) = (depth % self.dim, (lo + hi) / 2);
        let i = self.idx[mid];
        *visited += 1;
        let d = dist2(self.p(i), q);
        if heap.len() < k {
            heap.push((d.to_bits(), i));
        } else if d < f64::from_bits(heap.peek().unwrap().0) {
            heap.pop();
            heap.push((d.to_bits(), i));
        }
        let diff = q[axis] - self.p(i)[axis];
        let ((nl, nh), (fl, fh)) = if diff < 0.0 {
            ((lo, mid), (mid + 1, hi))
        } else {
            ((mid + 1, hi), (lo, mid))
        };
        self.knn_rec(nl, nh, depth + 1, q, k, heap, visited);
        let bound = if heap.len() < k {
            f64::INFINITY
        } else {
            f64::from_bits(heap.peek().unwrap().0)
        };
        if diff * diff < bound {
            self.knn_rec(fl, fh, depth + 1, q, k, heap, visited);
        }
    }
    /// All points with lo[a] <= x[a] <= hi[a] on every axis.
    fn range(&self, qlo: &[f64], qhi: &[f64]) -> (Vec<usize>, u64) {
        let (mut out, mut visited) = (vec![], 0);
        self.range_rec(0, self.idx.len(), 0, qlo, qhi, &mut out, &mut visited);
        (out, visited)
    }
    #[allow(clippy::too_many_arguments)]
    fn range_rec(
        &self,
        lo: usize,
        hi: usize,
        depth: usize,
        qlo: &[f64],
        qhi: &[f64],
        out: &mut Vec<usize>,
        visited: &mut u64,
    ) {
        if lo >= hi {
            return;
        }
        let (axis, mid) = (depth % self.dim, (lo + hi) / 2);
        let i = self.idx[mid];
        *visited += 1;
        let p = self.p(i);
        if (0..self.dim).all(|a| qlo[a] <= p[a] && p[a] <= qhi[a]) {
            out.push(i);
        }
        if qlo[axis] <= p[axis] {
            self.range_rec(lo, mid, depth + 1, qlo, qhi, out, visited);
        }
        if qhi[axis] >= p[axis] {
            self.range_rec(mid + 1, hi, depth + 1, qlo, qhi, out, visited);
        }
    }
}
fn brute_knn(pts: &[f64], dim: usize, q: &[f64], k: usize) -> Vec<(f64, usize)> {
    let mut v: Vec<(f64, usize)> = (0..pts.len() / dim)
        .map(|i| (dist2(&pts[i * dim..(i + 1) * dim], q), i))
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v.truncate(k);
    v
}

// ================================================================= 2. quadtree
struct QNode {
    x: f64,
    y: f64,
    s: f64,
    kids: [usize; 4], // 0 = none (the root is never a child)
    pts: Vec<usize>,
    depth: u32,
}
struct Quadtree<'a> {
    pts: &'a [(f64, f64)],
    nodes: Vec<QNode>,
}
const Q_CAP: usize = 8;
const Q_MAX_DEPTH: u32 = 24;
impl<'a> Quadtree<'a> {
    fn new(pts: &'a [(f64, f64)]) -> Self {
        let mut t = Quadtree {
            pts,
            nodes: vec![QNode {
                x: 0.0,
                y: 0.0,
                s: 1.0,
                kids: [0; 4],
                pts: vec![],
                depth: 0,
            }],
        };
        for i in 0..pts.len() {
            t.insert(i);
        }
        t
    }
    fn quadrant(n: &QNode, p: (f64, f64)) -> usize {
        let h = n.s / 2.0;
        (p.0 >= n.x + h) as usize + 2 * (p.1 >= n.y + h) as usize
    }
    fn insert(&mut self, id: usize) {
        let p = self.pts[id];
        let mut cur = 0;
        loop {
            if self.nodes[cur].kids[0] != 0 {
                cur = self.nodes[cur].kids[Self::quadrant(&self.nodes[cur], p)];
                continue;
            }
            self.nodes[cur].pts.push(id);
            if self.nodes[cur].pts.len() > Q_CAP && self.nodes[cur].depth < Q_MAX_DEPTH {
                let (x, y, h, d) = (
                    self.nodes[cur].x,
                    self.nodes[cur].y,
                    self.nodes[cur].s / 2.0,
                    self.nodes[cur].depth,
                );
                for q in 0..4 {
                    let (cx, cy) = (x + h * (q % 2) as f64, y + h * (q / 2) as f64);
                    self.nodes.push(QNode {
                        x: cx,
                        y: cy,
                        s: h,
                        kids: [0; 4],
                        pts: vec![],
                        depth: d + 1,
                    });
                    let nid = self.nodes.len() - 1;
                    self.nodes[cur].kids[q] = nid;
                }
                for moved in std::mem::take(&mut self.nodes[cur].pts) {
                    let q = Self::quadrant(&self.nodes[cur], self.pts[moved]);
                    let kid = self.nodes[cur].kids[q];
                    self.nodes[kid].pts.push(moved);
                }
            }
            return;
        }
    }
    fn range(&self, x0: f64, y0: f64, x1: f64, y1: f64) -> (Vec<usize>, u64) {
        let (mut out, mut visited, mut stack) = (vec![], 0, vec![0usize]);
        while let Some(c) = stack.pop() {
            let n = &self.nodes[c];
            if n.x > x1 || n.x + n.s < x0 || n.y > y1 || n.y + n.s < y0 {
                continue;
            }
            visited += 1;
            for &i in &n.pts {
                let p = self.pts[i];
                if p.0 >= x0 && p.0 <= x1 && p.1 >= y0 && p.1 <= y1 {
                    out.push(i);
                }
            }
            if n.kids[0] != 0 {
                stack.extend(n.kids);
            }
        }
        (out, visited)
    }
    fn box_dist2(n: &QNode, q: (f64, f64)) -> f64 {
        let dx = (n.x - q.0).max(0.0).max(q.0 - (n.x + n.s));
        let dy = (n.y - q.1).max(0.0).max(q.1 - (n.y + n.s));
        dx * dx + dy * dy
    }
    fn nearest(&self, q: (f64, f64)) -> (f64, usize, u64) {
        let mut best = (f64::INFINITY, usize::MAX);
        let mut visited = 0;
        self.nn(0, q, &mut best, &mut visited);
        (best.0, best.1, visited)
    }
    fn nn(&self, c: usize, q: (f64, f64), best: &mut (f64, usize), visited: &mut u64) {
        let n = &self.nodes[c];
        *visited += 1;
        for &i in &n.pts {
            let p = self.pts[i];
            let d = (p.0 - q.0).powi(2) + (p.1 - q.1).powi(2);
            if d < best.0 {
                *best = (d, i);
            }
        }
        if n.kids[0] != 0 {
            let mut order: Vec<(f64, usize)> = n
                .kids
                .iter()
                .map(|&k| (Self::box_dist2(&self.nodes[k], q), k))
                .collect();
            order.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (bd, k) in order {
                if bd < best.0 {
                    self.nn(k, q, best, visited);
                }
            }
        }
    }
}

// ================================================================= 3. STR R-tree
#[derive(Clone, Copy, Debug)]
struct Rect {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}
impl Rect {
    fn union(a: Rect, b: Rect) -> Rect {
        Rect {
            x0: a.x0.min(b.x0),
            y0: a.y0.min(b.y0),
            x1: a.x1.max(b.x1),
            y1: a.y1.max(b.y1),
        }
    }
    fn intersects(&self, o: &Rect) -> bool {
        self.x0 <= o.x1 && o.x0 <= self.x1 && self.y0 <= o.y1 && o.y0 <= self.y1
    }
    fn cx(&self) -> f64 {
        (self.x0 + self.x1) / 2.0
    }
    fn cy(&self) -> f64 {
        (self.y0 + self.y1) / 2.0
    }
}
struct RNode {
    mbr: Rect,
    kids: Vec<usize>, // rect ids if leaf, else node ids
    leaf: bool,
}
struct RTree {
    rects: Vec<Rect>,
    nodes: Vec<RNode>,
    root: usize,
}
impl RTree {
    /// Sort-Tile-Recursive packing, fan-out m.
    fn bulk_load(rects: Vec<Rect>, m: usize) -> Self {
        let mut t = RTree {
            rects,
            nodes: vec![],
            root: 0,
        };
        let mut level: Vec<(Rect, usize)> = t
            .rects
            .iter()
            .copied()
            .enumerate()
            .map(|(i, r)| (r, i))
            .collect();
        let mut leaf = true;
        loop {
            let packed = t.pack(level, m, leaf);
            leaf = false;
            if packed.len() == 1 {
                t.root = packed[0].1;
                return t;
            }
            level = packed;
        }
    }
    fn pack(&mut self, mut items: Vec<(Rect, usize)>, m: usize, leaf: bool) -> Vec<(Rect, usize)> {
        let nodes_needed = items.len().div_ceil(m).max(1);
        let slices = (nodes_needed as f64).sqrt().ceil() as usize;
        items.sort_by(|a, b| a.0.cx().total_cmp(&b.0.cx()));
        let mut out = vec![];
        for slice in items.chunks_mut(slices * m) {
            slice.sort_by(|a, b| a.0.cy().total_cmp(&b.0.cy()));
            for group in slice.chunks(m) {
                let mbr = group
                    .iter()
                    .skip(1)
                    .fold(group[0].0, |acc, g| Rect::union(acc, g.0));
                self.nodes.push(RNode {
                    mbr,
                    kids: group.iter().map(|g| g.1).collect(),
                    leaf,
                });
                out.push((mbr, self.nodes.len() - 1));
            }
        }
        out
    }
    fn window(&self, w: &Rect) -> (Vec<usize>, u64) {
        let (mut out, mut visited, mut stack) = (vec![], 0, vec![self.root]);
        while let Some(c) = stack.pop() {
            let n = &self.nodes[c];
            visited += 1;
            if !n.mbr.intersects(w) {
                continue;
            }
            for &k in &n.kids {
                if n.leaf {
                    if self.rects[k].intersects(w) {
                        out.push(k);
                    }
                } else {
                    stack.push(k);
                }
            }
        }
        (out, visited)
    }
    fn height(&self) -> usize {
        let (mut h, mut c) = (1, self.root);
        while !self.nodes[c].leaf {
            c = self.nodes[c].kids[0];
            h += 1;
        }
        h
    }
}

// ================================================================= 4. grid / spatial hash
struct Grid<'a> {
    pts: &'a [(f64, f64)],
    cell: f64,
    cells: HashMap<(i64, i64), Vec<usize>>,
}
impl<'a> Grid<'a> {
    fn new(pts: &'a [(f64, f64)], cell: f64) -> Self {
        let mut cells: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, p) in pts.iter().enumerate() {
            cells.entry(Self::key(*p, cell)).or_default().push(i);
        }
        Grid { pts, cell, cells }
    }
    fn key(p: (f64, f64), cell: f64) -> (i64, i64) {
        ((p.0 / cell).floor() as i64, (p.1 / cell).floor() as i64)
    }
    /// All points within distance r <= cell of q (r must not exceed the cell size).
    fn within(&self, q: (f64, f64), r: f64) -> Vec<usize> {
        assert!(r <= self.cell);
        let (cx, cy) = Self::key(q, self.cell);
        let mut out = vec![];
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(v) = self.cells.get(&(cx + dx, cy + dy)) {
                    out.extend(v.iter().copied().filter(|&i| {
                        (self.pts[i].0 - q.0).powi(2) + (self.pts[i].1 - q.1).powi(2) <= r * r
                    }));
                }
            }
        }
        out
    }
}

// ================================================================= 5. BK-tree
fn levenshtein(a: &[u8], b: &[u8]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, &ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + (ca != cb) as usize)
                .min(prev[j + 1] + 1)
                .min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}
struct BkTree {
    words: Vec<String>,
    kids: Vec<Vec<(usize, usize)>>, // per node: (distance to parent, child)
}
impl BkTree {
    fn new() -> Self {
        BkTree {
            words: vec![],
            kids: vec![],
        }
    }
    fn insert(&mut self, w: &str) {
        self.words.push(w.to_string());
        self.kids.push(vec![]);
        let id = self.words.len() - 1;
        if id == 0 {
            return;
        }
        let mut cur = 0;
        loop {
            let d = levenshtein(w.as_bytes(), self.words[cur].as_bytes());
            if d == 0 {
                self.words.pop();
                self.kids.pop();
                return; // duplicate
            }
            match self.kids[cur].iter().find(|&&(kd, _)| kd == d) {
                Some(&(_, child)) => cur = child,
                None => {
                    self.kids[cur].push((d, id));
                    return;
                }
            }
        }
    }
    /// All words within edit distance r; returns (matches, distance evaluations).
    fn query(&self, q: &str, r: usize) -> (Vec<String>, u64) {
        let (mut out, mut calls, mut stack) = (vec![], 0, vec![0usize]);
        while let Some(c) = stack.pop() {
            let d = levenshtein(q.as_bytes(), self.words[c].as_bytes());
            calls += 1;
            if d <= r {
                out.push(self.words[c].clone());
            }
            for &(kd, child) in &self.kids[c] {
                if kd + r >= d && kd <= d + r {
                    stack.push(child); // triangle inequality: |kd - d| <= r
                }
            }
        }
        (out, calls)
    }
}

// ================================================================= 6. VP-tree
const NONE: usize = usize::MAX;
struct VpNode {
    item: usize,
    mu: f64,
    left: usize,  // distance <= mu
    right: usize, // distance >= mu
}
struct VpTree {
    nodes: Vec<VpNode>,
    root: usize,
}
impl VpTree {
    fn build(items: Vec<usize>, dist: &impl Fn(usize, usize) -> f64, rng: &mut Rng) -> Self {
        let mut t = VpTree {
            nodes: vec![],
            root: NONE,
        };
        t.root = t.rec(items, dist, rng);
        t
    }
    fn rec(
        &mut self,
        mut items: Vec<usize>,
        dist: &impl Fn(usize, usize) -> f64,
        rng: &mut Rng,
    ) -> usize {
        if items.is_empty() {
            return NONE;
        }
        let pick = rng.below(items.len() as u64) as usize;
        let vp = items.swap_remove(pick);
        if items.is_empty() {
            self.nodes.push(VpNode {
                item: vp,
                mu: 0.0,
                left: NONE,
                right: NONE,
            });
            return self.nodes.len() - 1;
        }
        let mut ds: Vec<(f64, usize)> = items.iter().map(|&i| (dist(vp, i), i)).collect();
        let m = ds.len() / 2;
        ds.select_nth_unstable_by(m, |a, b| a.0.total_cmp(&b.0));
        let mu = ds[m].0;
        let right: Vec<usize> = ds[m..].iter().map(|x| x.1).collect();
        let left: Vec<usize> = ds[..m].iter().map(|x| x.1).collect();
        let (l, r) = (self.rec(left, dist, rng), self.rec(right, dist, rng));
        self.nodes.push(VpNode {
            item: vp,
            mu,
            left: l,
            right: r,
        });
        self.nodes.len() - 1
    }
    /// k nearest by the query-distance closure; returns (sorted (dist,item), distance evaluations).
    fn knn(&self, qd: &impl Fn(usize) -> f64, k: usize) -> (Vec<(f64, usize)>, u64) {
        let mut heap: BinaryHeap<(u64, usize)> = BinaryHeap::new();
        let mut calls = 0;
        self.search(self.root, qd, k, &mut heap, &mut calls);
        let mut v: Vec<(f64, usize)> = heap
            .into_iter()
            .map(|(b, i)| (f64::from_bits(b), i))
            .collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        (v, calls)
    }
    fn search(
        &self,
        n: usize,
        qd: &impl Fn(usize) -> f64,
        k: usize,
        heap: &mut BinaryHeap<(u64, usize)>,
        calls: &mut u64,
    ) {
        if n == NONE {
            return;
        }
        let node = &self.nodes[n];
        let d = qd(node.item);
        *calls += 1;
        if heap.len() < k {
            heap.push((d.to_bits(), node.item));
        } else if d < f64::from_bits(heap.peek().unwrap().0) {
            heap.pop();
            heap.push((d.to_bits(), node.item));
        }
        let tau = |h: &BinaryHeap<(u64, usize)>| {
            if h.len() < k {
                f64::INFINITY
            } else {
                f64::from_bits(h.peek().unwrap().0)
            }
        };
        if d < node.mu {
            if d - tau(heap) <= node.mu {
                self.search(node.left, qd, k, heap, calls);
            }
            if d + tau(heap) >= node.mu {
                self.search(node.right, qd, k, heap, calls);
            }
        } else {
            if d + tau(heap) >= node.mu {
                self.search(node.right, qd, k, heap, calls);
            }
            if d - tau(heap) <= node.mu {
                self.search(node.left, qd, k, heap, calls);
            }
        }
    }
}

fn main() {
    let mut rng = Rng::new(277);

    // ---- 1. k-d tree
    println!("== k-d tree: nearest neighbour (n = 20000 uniform points, 200 queries)");
    println!("   dim | avg nodes visited | % of n | k=10 visited | box query visited / hits");
    let n = 20_000;
    let mut fractions = vec![];
    for dim in [2usize, 5, 10, 20] {
        let pts = random_points(&mut rng, n, dim);
        let tree = KdTree::build(&pts, dim);
        let (mut vis_nn, mut vis_k, mut vis_r, mut hits) = (0u64, 0u64, 0u64, 0usize);
        for _ in 0..200 {
            let q: Vec<f64> = (0..dim).map(|_| rng.unit()).collect();
            let (d, i, v) = tree.nearest(&q);
            let want = brute_knn(&pts, dim, &q, 1)[0];
            assert!((d - want.0).abs() < 1e-12 && (dist2(tree.p(i), &q) - want.0).abs() < 1e-12);
            vis_nn += v;
            let (kn, v) = tree.knn(&q, 10);
            let wk = brute_knn(&pts, dim, &q, 10);
            assert_eq!(kn.len(), 10);
            assert!(
                kn.iter().zip(&wk).all(|(a, b)| (a.0 - b.0).abs() < 1e-12),
                "knn dim {dim}"
            );
            vis_k += v;
            // a box with side 0.3 around the query clipped to the cube
            let (lo, hi): (Vec<f64>, Vec<f64>) = q
                .iter()
                .map(|&c| ((c - 0.15).max(0.0), (c + 0.15).min(1.0)))
                .unzip();
            let (mut got, v) = tree.range(&lo, &hi);
            got.sort();
            let want: Vec<usize> = (0..n)
                .filter(|&i| {
                    (0..dim).all(|a| lo[a] <= pts[i * dim + a] && pts[i * dim + a] <= hi[a])
                })
                .collect();
            assert_eq!(got, want, "range dim {dim}");
            vis_r += v;
            hits += got.len();
        }
        let avg = vis_nn as f64 / 200.0;
        fractions.push(avg / n as f64);
        println!(
            "  {dim:>4} | {avg:>17.0} | {:>5.1}% | {:>12.0} | {:>6.0} / {:.1}",
            100.0 * avg / n as f64,
            vis_k as f64 / 200.0,
            vis_r as f64 / 200.0,
            hits as f64 / 200.0
        );
    }
    assert!(
        fractions[0] < 0.01,
        "2-D pruning must touch < 1% of the points, got {}",
        fractions[0]
    );
    assert!(
        fractions.windows(2).all(|w| w[0] < w[1]),
        "work grows with dimension: {fractions:?}"
    );
    assert!(
        fractions[3] > 10.0 * fractions[0] && fractions[3] > 0.25,
        "20-D pruning should collapse: {fractions:?}"
    );

    // ---- 2. quadtree
    let pts2: Vec<(f64, f64)> = (0..50_000)
        .map(|i| {
            if i % 5 == 0 {
                (rng.unit() * 0.1 + 0.4, rng.unit() * 0.1 + 0.4)
            } else {
                (rng.unit(), rng.unit())
            }
        }) // a dense hot spot
        .collect();
    let qt = Quadtree::new(&pts2);
    let (mut vr, mut vn) = (0, 0);
    for _ in 0..200 {
        let (x, y) = (rng.unit() * 0.9, rng.unit() * 0.9);
        let (mut got, v) = qt.range(x, y, x + 0.05, y + 0.05);
        got.sort();
        let want: Vec<usize> = (0..pts2.len())
            .filter(|&i| {
                pts2[i].0 >= x && pts2[i].0 <= x + 0.05 && pts2[i].1 >= y && pts2[i].1 <= y + 0.05
            })
            .collect();
        assert_eq!(got, want);
        vr += v;
        let q = (rng.unit(), rng.unit());
        let (d, _, v) = qt.nearest(q);
        let wd = pts2
            .iter()
            .map(|p| (p.0 - q.0).powi(2) + (p.1 - q.1).powi(2))
            .fold(f64::INFINITY, f64::min);
        assert!((d - wd).abs() < 1e-15);
        vn += v;
    }
    let maxdepth = qt.nodes.iter().map(|n| n.depth).max().unwrap();
    println!(
        "== quadtree: {} points, {} nodes, max depth {maxdepth}; avg nodes visited: range {:.0}, nearest {:.0}",
        pts2.len(),
        qt.nodes.len(),
        vr as f64 / 200.0,
        vn as f64 / 200.0
    );
    assert!(vn as f64 / 200.0 < 200.0);

    // ---- 3. STR R-tree
    let rects: Vec<Rect> = (0..40_000)
        .map(|_| {
            let (x, y) = (rng.unit(), rng.unit());
            Rect {
                x0: x,
                y0: y,
                x1: x + rng.unit() * 0.01,
                y1: y + rng.unit() * 0.01,
            }
        })
        .collect();
    let rt = RTree::bulk_load(rects.clone(), 16);
    let mut vis = 0;
    for _ in 0..300 {
        let (x, y) = (rng.unit(), rng.unit());
        let w = Rect {
            x0: x,
            y0: y,
            x1: x + 0.03,
            y1: y + 0.03,
        };
        let (mut got, v) = rt.window(&w);
        got.sort();
        let want: Vec<usize> = (0..rects.len())
            .filter(|&i| rects[i].intersects(&w))
            .collect();
        assert_eq!(got, want);
        vis += v;
    }
    println!(
        "== STR R-tree: {} rects, fan-out 16, {} nodes, height {}; window query visits {:.0} nodes (scan = {})",
        rects.len(),
        rt.nodes.len(),
        rt.height(),
        vis as f64 / 300.0,
        rects.len()
    );
    assert!((vis as f64 / 300.0) < 400.0);

    // ---- 4. grid
    let gp: Vec<(f64, f64)> = (0..20_000).map(|_| (rng.unit(), rng.unit())).collect();
    let r = 0.01;
    let grid = Grid::new(&gp, r);
    let mut pairs = 0usize;
    for (i, &p) in gp.iter().enumerate().take(500) {
        let mut got = grid.within(p, r);
        got.sort();
        let want: Vec<usize> = (0..gp.len())
            .filter(|&j| (gp[j].0 - p.0).powi(2) + (gp[j].1 - p.1).powi(2) <= r * r)
            .collect();
        assert_eq!(got, want);
        pairs += got.len() - 1; // minus itself
        assert!(got.contains(&i));
    }
    println!(
        "== grid hash: {} cells for {} points; 500 radius-{r} queries equal brute force ({:.1} neighbours each)",
        grid.cells.len(),
        gp.len(),
        pairs as f64 / 500.0
    );

    // ---- 5. BK-tree
    let alphabet = b"abcdefghij";
    let mut set = HashSet::new();
    while set.len() < 30_000 {
        let len = 5 + rng.below(4) as usize;
        let w: String = (0..len)
            .map(|_| alphabet[rng.below(10) as usize] as char)
            .collect();
        set.insert(w);
    }
    let mut words: Vec<String> = set.into_iter().collect();
    words.sort();
    let mut bk = BkTree::new();
    for w in &words {
        bk.insert(w);
    }
    assert_eq!(bk.words.len(), words.len());
    let (mut calls, mut matches) = (0u64, 0usize);
    for qn in 0..100 {
        let mut q = words[rng.below(words.len() as u64) as usize]
            .clone()
            .into_bytes();
        if qn % 2 == 1 {
            let p = rng.below(q.len() as u64) as usize; // a typo: substitute one letter
            q[p] = alphabet[rng.below(10) as usize];
        }
        let q = String::from_utf8(q).unwrap();
        let (mut got, c) = bk.query(&q, 2);
        got.sort();
        let want: Vec<String> = words
            .iter()
            .filter(|w| levenshtein(q.as_bytes(), w.as_bytes()) <= 2)
            .cloned()
            .collect();
        assert_eq!(got, want, "query {q}");
        calls += c;
        matches += got.len();
    }
    println!(
        "== BK-tree: {} words, radius 2: {:.0} distance evals per query vs {} brute force ({:.1} matches avg)",
        words.len(),
        calls as f64 / 100.0,
        words.len(),
        matches as f64 / 100.0
    );
    assert!((calls as f64 / 100.0) < words.len() as f64 * 0.8);
    assert_eq!(levenshtein(b"kitten", b"sitting"), 3);

    // ---- 6. VP-tree: Euclidean clusters in 8-D, and words under edit distance
    let dim = 8;
    let centers: Vec<Vec<f64>> = (0..12)
        .map(|_| (0..dim).map(|_| rng.unit() * 10.0).collect())
        .collect();
    let mut vecs = vec![];
    for i in 0..15_000 {
        let c = &centers[i % 12];
        for &cc in c.iter() {
            vecs.push(cc + (rng.unit() + rng.unit() + rng.unit() - 1.5) * 0.7);
        }
    }
    let vn_items = vecs.len() / dim;
    let ed = |a: usize, b: usize| {
        dist2(&vecs[a * dim..(a + 1) * dim], &vecs[b * dim..(b + 1) * dim]).sqrt()
    };
    let vp = VpTree::build((0..vn_items).collect(), &ed, &mut rng);
    let mut calls = 0;
    for _ in 0..100 {
        let q: Vec<f64> = centers[rng.below(12) as usize]
            .iter()
            .map(|&c| c + (rng.unit() - 0.5) * 2.0)
            .collect();
        let qd = |i: usize| dist2(&vecs[i * dim..(i + 1) * dim], &q).sqrt();
        let (got, c) = vp.knn(&qd, 5);
        let mut all: Vec<(f64, usize)> = (0..vn_items).map(|i| (qd(i), i)).collect();
        all.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert!(got.iter().zip(&all).all(|(a, b)| (a.0 - b.0).abs() < 1e-12));
        calls += c;
    }
    println!(
        "== VP-tree (8-D clustered vectors, n = {vn_items}): 5-NN uses {:.0} distance evals vs {vn_items} brute force",
        calls as f64 / 100.0
    );
    assert!((calls as f64 / 100.0) < vn_items as f64 * 0.5);
    let sub: Vec<&String> = words.iter().take(5000).collect();
    let wd = |a: usize, b: usize| levenshtein(sub[a].as_bytes(), sub[b].as_bytes()) as f64;
    let vpw = VpTree::build((0..sub.len()).collect(), &wd, &mut rng);
    let mut wcalls = 0;
    for _ in 0..50 {
        let q = sub[rng.below(5000) as usize].as_bytes();
        let qd = |i: usize| levenshtein(q, sub[i].as_bytes()) as f64;
        let (got, c) = vpw.knn(&qd, 3);
        let mut all: Vec<f64> = (0..sub.len()).map(qd).collect();
        all.sort_by(|a, b| a.total_cmp(b));
        assert!(got.iter().zip(&all).all(|(a, b)| a.0 == *b));
        wcalls += c;
    }
    println!(
        "   same VP-tree code over strings (edit distance, n = 5000): 3-NN {:.0} distance evals per query",
        wcalls as f64 / 50.0
    );

    println!("\n  workload                                    -> index");
    for (w, i) in [
        ("low-dim static points (<= ~10 dims)", "k-d tree"),
        ("2-D/3-D points inserted over time", "quadtree / octree"),
        (
            "rectangles/polygons, disk-based GIS",
            "R-tree (STR bulk load if static)",
        ),
        (
            "moving objects with known radius (games)",
            "uniform grid / spatial hash",
        ),
        ("spell check, edit-distance search", "BK-tree"),
        ("any metric, modest intrinsic dimension", "VP-tree"),
        (
            "embeddings with 100+ dims",
            "approximate: r140-r143 (IVF, HNSW, DiskANN, PQ)",
        ),
    ] {
        println!("  {w:<43} -> {i}");
    }
    println!("OK");
}
