//! RECIPE 257 (geometry): computational geometry: exact predicates, hulls, triangulations, collision.
//! LEVEL: advanced
//! Use in: GIS, games, CAD, mesh generation, collision detection, anything that asks "which side of the line?".   Run: cargo run --release --example computational_geometry
//! COMMON USES: convex hulls for outlier/bounding shapes, point-in-polygon for map lookups, triangulation for rendering concave polygons, Delaunay meshes for terrain and interpolation, Voronoi diagrams for nearest-facility regions, SAT/GJK for physics engines, closest-pair for clustering and collision pruning.
#![allow(dead_code)]
//!
//! Everything rests on ONE predicate: orientation(a, b, c) = sign of the cross product (b-a) x (c-a).
//! With f64 the sign of a near-zero cross product is noise (rounding), and a wrong sign makes hulls
//! non-convex and triangulations overlap. With i64 coordinates (|c| < ~1e9) computed in i128 the predicate
//! is EXACT, so every algorithm below is checked against a brute-force oracle with `assert_eq!`, no epsilons.
//! The same trick gives an exact in-circle test (a 3x3 determinant in i128, |c| <= 3e8) for Delaunay. The hull needs
//! care there too: the usual far-away "super triangle" silently loses thin hull triangles, so this one uses "ghost"
//! triangles with a corner at infinity. Each insertion scans every triangle, so n points cost O(n^2).
//!
//! Sections: 1 float-vs-exact orientation, 2 hull (monotone chain vs gift wrapping), 3 segment intersection,
//! 4 point in polygon (ray casting vs winding), 5 shoelace area/centroid, 6 closest pair (divide and conquer),
//! 7 ear clipping, 8 Bowyer-Watson Delaunay + Voronoi dual, 9 rotating calipers, 10 SAT convex collision.
//! Optional picture files are written only if the env var COOKBOOK_GALLERY names a directory.
//! Read: https://cp-algorithms.com/geometry/basic-geometry.html
//! Read: https://cp-algorithms.com/geometry/convex-hull.html
//! Read: https://en.wikipedia.org/wiki/Bowyer%E2%80%93Watson_algorithm
//! Read: https://floating-point-gui.de/

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo) as u64) as i64
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Pt {
    x: i64,
    y: i64,
}
fn p(x: i64, y: i64) -> Pt {
    Pt { x, y }
}

/// Exact cross product (b-a) x (c-a) in i128: >0 left turn (CCW), <0 right turn, 0 collinear.
fn cross(a: Pt, b: Pt, c: Pt) -> i128 {
    (b.x - a.x) as i128 * (c.y - a.y) as i128 - (b.y - a.y) as i128 * (c.x - a.x) as i128
}
fn d2(a: Pt, b: Pt) -> i128 {
    let (dx, dy) = ((a.x - b.x) as i128, (a.y - b.y) as i128);
    dx * dx + dy * dy
}
/// c is collinear with a-b; is it inside the closed segment?
fn on_seg(a: Pt, b: Pt, c: Pt) -> bool {
    c.x >= a.x.min(b.x) && c.x <= a.x.max(b.x) && c.y >= a.y.min(b.y) && c.y <= a.y.max(b.y)
}

// ---------- 2. convex hull ----------
/// Andrew's monotone chain: CCW hull starting at the lexicographically smallest point, no collinear points.
fn convex_hull(pts: &[Pt]) -> Vec<Pt> {
    let mut v = pts.to_vec();
    v.sort();
    v.dedup();
    if v.len() < 3 {
        return v;
    }
    let mut h: Vec<Pt> = Vec::new();
    for &q in &v {
        while h.len() >= 2 && cross(h[h.len() - 2], h[h.len() - 1], q) <= 0 {
            h.pop();
        }
        h.push(q);
    }
    let lower = h.len() + 1;
    for &q in v.iter().rev().skip(1) {
        while h.len() >= lower && cross(h[h.len() - 2], h[h.len() - 1], q) <= 0 {
            h.pop();
        }
        h.push(q);
    }
    h.pop();
    h
}

/// Jarvis march, O(nh): the oracle. Ties (collinear) go to the farthest point.
fn gift_wrap(pts: &[Pt]) -> Vec<Pt> {
    let mut v = pts.to_vec();
    v.sort();
    v.dedup();
    if v.len() < 3 {
        return v;
    }
    let start = v[0];
    let (mut h, mut cur) = (Vec::new(), start);
    loop {
        h.push(cur);
        assert!(h.len() <= v.len() + 1);
        let mut q = if cur == v[0] { v[1] } else { v[0] };
        for &r in &v {
            if r == cur {
                continue;
            }
            let c = cross(cur, q, r);
            if c < 0 || (c == 0 && d2(cur, r) > d2(cur, q)) {
                q = r;
            }
        }
        cur = q;
        if cur == start {
            break;
        }
    }
    h
}

// ---------- 3. segments ----------
fn seg_intersect(a: Pt, b: Pt, c: Pt, d: Pt) -> bool {
    let (d1, d2_, d3, d4) = (
        cross(a, b, c).signum(),
        cross(a, b, d).signum(),
        cross(c, d, a).signum(),
        cross(c, d, b).signum(),
    );
    if d1 * d2_ < 0 && d3 * d4 < 0 {
        return true;
    }
    (d1 == 0 && on_seg(a, b, c))
        || (d2_ == 0 && on_seg(a, b, d))
        || (d3 == 0 && on_seg(c, d, a))
        || (d4 == 0 && on_seg(c, d, b))
}
fn seg_kind(a: Pt, b: Pt, c: Pt, d: Pt) -> &'static str {
    if !seg_intersect(a, b, c, d) {
        return "none";
    }
    if cross(a, b, c) == 0 && cross(a, b, d) == 0 {
        let lo = a.min(b).max(c.min(d));
        let hi = a.max(b).min(c.max(d));
        return if lo == hi { "touch" } else { "overlap" };
    }
    let touches = [
        cross(a, b, c),
        cross(a, b, d),
        cross(c, d, a),
        cross(c, d, b),
    ]
    .contains(&0);
    if touches { "touch" } else { "proper" }
}
/// Different formula as an oracle: solve p + t r = q + u s with rationals and test 0<=t,u<=1.
fn seg_oracle(a: Pt, b: Pt, c: Pt, d: Pt) -> bool {
    let (rx, ry) = ((b.x - a.x) as i128, (b.y - a.y) as i128);
    let (sx, sy) = ((d.x - c.x) as i128, (d.y - c.y) as i128);
    let (qx, qy) = ((c.x - a.x) as i128, (c.y - a.y) as i128);
    let den = rx * sy - ry * sx;
    let in01 = |n: i128, dd: i128| {
        let (n, dd) = if dd < 0 { (-n, -dd) } else { (n, dd) };
        0 <= n && n <= dd
    };
    if den != 0 {
        in01(qx * sy - qy * sx, den) && in01(qx * ry - qy * rx, den)
    } else {
        if qx * ry - qy * rx != 0 {
            return false; // parallel, not collinear
        }
        a.x.min(b.x) <= c.x.max(d.x)
            && c.x.min(d.x) <= a.x.max(b.x)
            && a.y.min(b.y) <= c.y.max(d.y)
            && c.y.min(d.y) <= a.y.max(b.y)
    }
}

// ---------- 4/5. polygons ----------
fn on_boundary(poly: &[Pt], q: Pt) -> bool {
    let n = poly.len();
    (0..n)
        .any(|i| cross(poly[i], poly[(i + 1) % n], q) == 0 && on_seg(poly[i], poly[(i + 1) % n], q))
}
/// Crossing-number test (valid when q is not on the boundary).
fn ray_cast(poly: &[Pt], q: Pt) -> bool {
    let n = poly.len();
    let mut inside = false;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > q.y) != (b.y > q.y) {
            let c = cross(a, b, q);
            if (b.y > a.y && c > 0) || (b.y < a.y && c < 0) {
                inside = !inside;
            }
        }
    }
    inside
}
fn winding(poly: &[Pt], q: Pt) -> i32 {
    let n = poly.len();
    let mut wn = 0;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if a.y <= q.y {
            if b.y > q.y && cross(a, b, q) > 0 {
                wn += 1;
            }
        } else if b.y <= q.y && cross(a, b, q) < 0 {
            wn -= 1;
        }
    }
    wn
}
/// Twice the signed area (shoelace): exact integer, positive for CCW.
fn area2(poly: &[Pt]) -> i128 {
    let n = poly.len();
    (0..n)
        .map(|i| {
            poly[i].x as i128 * poly[(i + 1) % n].y as i128
                - poly[(i + 1) % n].x as i128 * poly[i].y as i128
        })
        .sum()
}
fn centroid(poly: &[Pt]) -> (f64, f64) {
    let n = poly.len();
    let (mut sx, mut sy) = (0f64, 0f64);
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let c = (a.x as i128 * b.y as i128 - b.x as i128 * a.y as i128) as f64;
        sx += (a.x + b.x) as f64 * c;
        sy += (a.y + b.y) as f64 * c;
    }
    let a6 = 3.0 * area2(poly) as f64;
    (sx / a6, sy / a6)
}
fn is_simple(poly: &[Pt]) -> bool {
    let n = poly.len();
    for i in 0..n {
        for j in i + 1..n {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            if seg_intersect(poly[i], poly[(i + 1) % n], poly[j], poly[(j + 1) % n]) {
                return false;
            }
        }
    }
    true
}
/// Random star-shaped (usually concave) CCW polygon with no collinear consecutive vertices.
fn random_polygon(rng: &mut Rng, n: usize) -> Vec<Pt> {
    loop {
        let mut ang: Vec<f64> = (0..n).map(|_| rng.unit() * std::f64::consts::TAU).collect();
        ang.sort_by(|a, b| a.total_cmp(b));
        let poly: Vec<Pt> = ang
            .iter()
            .map(|&t| {
                let r = 200.0 + 800.0 * rng.unit();
                p((r * t.cos()).round() as i64, (r * t.sin()).round() as i64)
            })
            .collect();
        let ok = (0..n).all(|i| cross(poly[i], poly[(i + 1) % n], poly[(i + 2) % n]) != 0)
            && area2(&poly) > 0
            && is_simple(&poly);
        if ok {
            return poly;
        }
    }
}

// ---------- 6. closest pair ----------
fn closest_pair_dc(pts: &[Pt]) -> i128 {
    let mut v = pts.to_vec();
    v.sort();
    fn rec(v: &mut [Pt]) -> i128 {
        let n = v.len();
        if n <= 3 {
            let mut best = i128::MAX;
            for i in 0..n {
                for j in i + 1..n {
                    best = best.min(d2(v[i], v[j]));
                }
            }
            v.sort_by_key(|q| q.y);
            return best;
        }
        let mid = n / 2;
        let midx = v[mid].x;
        let (l, r) = v.split_at_mut(mid);
        let d = rec(l).min(rec(r));
        let mut merged = Vec::with_capacity(n); // merge by y (this is what keeps it O(n log n))
        let (mut i, mut j) = (0, 0);
        while i < l.len() || j < r.len() {
            if j >= r.len() || (i < l.len() && l[i].y <= r[j].y) {
                merged.push(l[i]);
                i += 1;
            } else {
                merged.push(r[j]);
                j += 1;
            }
        }
        v.copy_from_slice(&merged);
        let mut d = d;
        let strip: Vec<Pt> = v
            .iter()
            .copied()
            .filter(|q| ((q.x - midx) as i128).pow(2) < d)
            .collect();
        for i in 0..strip.len() {
            for j in i + 1..strip.len() {
                if ((strip[j].y - strip[i].y) as i128).pow(2) >= d {
                    break;
                }
                d = d.min(d2(strip[i], strip[j]));
            }
        }
        d
    }
    rec(&mut v)
}
fn closest_pair_brute(pts: &[Pt]) -> i128 {
    let mut best = i128::MAX;
    for i in 0..pts.len() {
        for j in i + 1..pts.len() {
            best = best.min(d2(pts[i], pts[j]));
        }
    }
    best
}

// ---------- 7. ear clipping ----------
fn in_tri(a: Pt, b: Pt, c: Pt, q: Pt) -> bool {
    cross(a, b, q) >= 0 && cross(b, c, q) >= 0 && cross(c, a, q) >= 0
}
fn ear_clip(poly: &[Pt]) -> Vec<[Pt; 3]> {
    let mut idx: Vec<usize> = (0..poly.len()).collect();
    let mut tris = Vec::new();
    while idx.len() > 3 {
        let m = idx.len();
        let mut found = false;
        for i in 0..m {
            let (a, b, c) = (
                poly[idx[(i + m - 1) % m]],
                poly[idx[i]],
                poly[idx[(i + 1) % m]],
            );
            let cr = cross(a, b, c);
            if cr < 0 {
                continue; // reflex vertex
            }
            if cr == 0 {
                idx.remove(i); // degenerate sliver: just drop the vertex
                found = true;
                break;
            }
            if !idx
                .iter()
                .any(|&k| poly[k] != a && poly[k] != b && poly[k] != c && in_tri(a, b, c, poly[k]))
            {
                tris.push([a, b, c]);
                idx.remove(i);
                found = true;
                break;
            }
        }
        assert!(
            found,
            "a simple polygon always has an ear (two-ears theorem)"
        );
    }
    if idx.len() == 3 && cross(poly[idx[0]], poly[idx[1]], poly[idx[2]]) > 0 {
        tris.push([poly[idx[0]], poly[idx[1]], poly[idx[2]]]);
    }
    tris
}

// ---------- 8. Delaunay (Bowyer-Watson) ----------
/// > 0 iff d is strictly inside the circumcircle of CCW triangle abc. Exact in i128 for |coord| <= ~3e8.
fn incircle(a: Pt, b: Pt, c: Pt, d: Pt) -> i128 {
    let f = |q: Pt| ((q.x - d.x) as i128, (q.y - d.y) as i128);
    let ((ax, ay), (bx, by), (cx, cy)) = (f(a), f(b), f(c));
    (ax * ax + ay * ay) * (bx * cy - cx * by) - (bx * bx + by * by) * (ax * cy - cx * ay)
        + (cx * cx + cy * cy) * (ax * by - bx * ay)
}
/// The vertex "at infinity". A GHOST triangle stands for the outside of one hull edge a->b (in cyclic order a, b,
/// GHOST), with the outside on the LEFT of a->b. A finite "super triangle" is the common shortcut and is wrong: a thin
/// triangle on the hull has a huge circumcircle that reaches any fixed far-away corner, and the triangle is lost.
const GHOST: usize = usize::MAX;
/// Does inserting d destroy triangle t? A real one: d strictly inside its circumcircle. A ghost: d strictly outside
/// its hull edge, or on that edge between the ends (the limit of a circle whose third corner goes to infinity).
fn conflicts(v: &[Pt], t: [usize; 3], d: Pt) -> bool {
    match t.iter().position(|&k| k == GHOST) {
        None => incircle(v[t[0]], v[t[1]], v[t[2]], d) > 0,
        Some(g) => {
            let (a, b) = (v[t[(g + 1) % 3]], v[t[(g + 2) % 3]]);
            let side = cross(a, b, d);
            side > 0 || (side == 0 && d != a && d != b && on_seg(a, b, d))
        }
    }
}
/// CCW triangles of the Delaunay triangulation; empty if all points are on one line. A repeated point is used once.
fn delaunay(pts: &[Pt]) -> Vec<[usize; 3]> {
    assert!(
        pts.iter()
            .all(|q| q.x.unsigned_abs() <= 300_000_000 && q.y.unsigned_abs() <= 300_000_000),
        "incircle is exact only for |coord| <= 3e8"
    );
    // the first triangle with an area
    let Some(second) = (1..pts.len()).find(|&j| pts[j] != pts[0]) else {
        return vec![];
    };
    let Some(third) = (1..pts.len()).find(|&k| cross(pts[0], pts[second], pts[k]) != 0) else {
        return vec![];
    };
    let (a, b, c) = if cross(pts[0], pts[second], pts[third]) > 0 {
        (0, second, third)
    } else {
        (0, third, second)
    };
    let mut tris: Vec<[usize; 3]> = vec![[a, b, c], [b, a, GHOST], [c, b, GHOST], [a, c, GHOST]];
    for i in (0..pts.len()).filter(|&i| i != a && i != b && i != c) {
        let (bad, good): (Vec<[usize; 3]>, Vec<[usize; 3]>) = tris
            .iter()
            .copied()
            .partition(|&t| conflicts(pts, t, pts[i]));
        let edges: Vec<(usize, usize)> = bad
            .iter()
            .flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])])
            .collect();
        tris = good;
        for &(a, b) in &edges {
            if !edges.contains(&(b, a)) {
                tris.push([a, b, i]); // boundary edge of the cavity: re-triangulate towards the new point
            }
        }
    }
    tris.retain(|t| !t.contains(&GHOST));
    tris
}
/// Empty circumcircles and the Euler count 2n - 2 - h for DISTINCT, not all collinear points. Returns (triangles, h).
fn check_delaunay(pts: &[Pt]) -> (Vec<[usize; 3]>, usize) {
    let tris = delaunay(pts);
    for t in &tris {
        assert!(
            cross(pts[t[0]], pts[t[1]], pts[t[2]]) > 0,
            "CCW, non-degenerate"
        );
        for (k, &q) in pts.iter().enumerate() {
            if !t.contains(&k) {
                assert!(
                    incircle(pts[t[0]], pts[t[1]], pts[t[2]], q) <= 0,
                    "empty circumcircle"
                );
            }
        }
    }
    let hull = convex_hull(pts);
    let h = pts
        .iter()
        .filter(|&&q| {
            (0..hull.len()).any(|i| {
                cross(hull[i], hull[(i + 1) % hull.len()], q) == 0
                    && on_seg(hull[i], hull[(i + 1) % hull.len()], q)
            })
        })
        .count();
    assert_eq!(
        tris.len(),
        2 * pts.len() - 2 - h,
        "Euler: triangles = 2n - 2 - h for {pts:?}"
    );
    (tris, h)
}
fn circumcenter(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> (f64, f64) {
    let d = 2.0 * (a.0 * (b.1 - c.1) + b.0 * (c.1 - a.1) + c.0 * (a.1 - b.1));
    let (a2, b2, c2) = (
        a.0 * a.0 + a.1 * a.1,
        b.0 * b.0 + b.1 * b.1,
        c.0 * c.0 + c.1 * c.1,
    );
    (
        (a2 * (b.1 - c.1) + b2 * (c.1 - a.1) + c2 * (a.1 - b.1)) / d,
        (a2 * (c.0 - b.0) + b2 * (a.0 - c.0) + c2 * (b.0 - a.0)) / d,
    )
}

// ---------- 9. rotating calipers ----------
fn diameter2_calipers(h: &[Pt]) -> i128 {
    let m = h.len();
    if m < 3 {
        return if m == 2 { d2(h[0], h[1]) } else { 0 };
    }
    let (mut best, mut j) = (0, 1);
    for i in 0..m {
        let ni = (i + 1) % m;
        while cross(h[i], h[ni], h[(j + 1) % m]).abs() > cross(h[i], h[ni], h[j]).abs() {
            j = (j + 1) % m;
        }
        best = best.max(d2(h[i], h[j])).max(d2(h[ni], h[j]));
    }
    best
}

// ---------- 10. SAT ----------
/// Separating Axis Theorem for CONVEX CCW polygons (closed sets: touching counts as colliding).
fn sat_collide(a: &[Pt], b: &[Pt]) -> bool {
    for poly in [a, b] {
        let n = poly.len();
        for i in 0..n {
            let (e0, e1) = (poly[i], poly[(i + 1) % n]);
            let (nx, ny) = (-(e1.y - e0.y) as i128, (e1.x - e0.x) as i128);
            let proj = |s: &[Pt]| {
                let v: Vec<i128> = s
                    .iter()
                    .map(|q| q.x as i128 * nx + q.y as i128 * ny)
                    .collect();
                (*v.iter().min().unwrap(), *v.iter().max().unwrap())
            };
            let ((amin, amax), (bmin, bmax)) = (proj(a), proj(b));
            if amax < bmin || bmax < amin {
                return false; // found a separating axis
            }
        }
    }
    true
}
fn brute_collide(a: &[Pt], b: &[Pt]) -> bool {
    let inside = |poly: &[Pt], q: Pt| on_boundary(poly, q) || winding(poly, q) != 0;
    if a.iter().any(|&q| inside(b, q)) || b.iter().any(|&q| inside(a, q)) {
        return true;
    }
    (0..a.len()).any(|i| {
        (0..b.len()).any(|j| seg_intersect(a[i], a[(i + 1) % a.len()], b[j], b[(j + 1) % b.len()]))
    })
}

// ---------- ASCII canvas ----------
struct Canvas {
    w: usize,
    h: usize,
    cells: Vec<Vec<char>>,
    min: (f64, f64),
    max: (f64, f64),
}
impl Canvas {
    fn new(w: usize, h: usize, pts: &[Pt]) -> Self {
        let min = (
            pts.iter().map(|q| q.x).min().unwrap() as f64,
            pts.iter().map(|q| q.y).min().unwrap() as f64,
        );
        let max = (
            pts.iter().map(|q| q.x).max().unwrap() as f64,
            pts.iter().map(|q| q.y).max().unwrap() as f64,
        );
        Canvas {
            w,
            h,
            cells: vec![vec![' '; w]; h],
            min,
            max,
        }
    }
    fn map(&self, q: Pt) -> (i64, i64) {
        let gx =
            (q.x as f64 - self.min.0) / (self.max.0 - self.min.0).max(1.0) * (self.w - 1) as f64;
        let gy =
            (q.y as f64 - self.min.1) / (self.max.1 - self.min.1).max(1.0) * (self.h - 1) as f64;
        (gx.round() as i64, (self.h - 1) as i64 - gy.round() as i64)
    }
    fn put(&mut self, g: (i64, i64), ch: char) {
        self.cells[g.1 as usize][g.0 as usize] = ch;
    }
    fn line(&mut self, a: Pt, b: Pt, ch: char) {
        let ((mut x, mut y), (x1, y1)) = (self.map(a), self.map(b));
        let (dx, dy) = ((x1 - x).abs(), -(y1 - y).abs());
        let (sx, sy) = (if x < x1 { 1 } else { -1 }, if y < y1 { 1 } else { -1 });
        let mut err = dx + dy;
        loop {
            if self.cells[y as usize][x as usize] == ' ' {
                self.put((x, y), ch);
            }
            if (x, y) == (x1, y1) {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }
    fn point(&mut self, q: Pt, ch: char) {
        let g = self.map(q);
        self.put(g, ch);
    }
    fn print(&self) {
        for row in &self.cells {
            println!("    {}", row.iter().collect::<String>().trim_end());
        }
    }
    /// Only if COOKBOOK_GALLERY names a directory: write a small PPM (nothing is written otherwise).
    fn gallery(&self, name: &str) {
        let Ok(dir) = std::env::var("COOKBOOK_GALLERY") else {
            return;
        };
        let s = 8;
        let mut out = format!("P6\n{} {}\n255\n", self.w * s, self.h * s).into_bytes();
        for gy in 0..self.h * s {
            for gx in 0..self.w * s {
                let ch = self.cells[gy / s][gx / s];
                let rgb = match ch {
                    ' ' => [20, 20, 28],
                    '.' => [90, 90, 120],
                    '*' | '#' => [240, 180, 60],
                    _ => [120, 220, 140],
                };
                out.extend(rgb);
            }
        }
        let _ = std::fs::write(format!("{dir}/{name}.ppm"), out);
    }
}

fn main() {
    let mut rng = Rng(0x2545F4914F6CDD1D);

    // 1. orientation: float failure vs exact integers
    let (mut float_wrong, mut total) = (0, 0);
    for i in 1..60i64 {
        for j in 1..60i64 {
            // three points on the line y = 3x, written as decimals: (0.1k, 0.3k)
            let f = |k: i64| (k as f64 * 0.1, k as f64 * 0.3);
            let (a, b, c) = (f(1), f(i), f(j));
            let fc = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
            let exact = cross(p(1, 3), p(i, 3 * i), p(j, 3 * j)); // same points scaled by 10 as integers
            assert_eq!(exact, 0);
            total += 1;
            if fc != 0.0 {
                float_wrong += 1;
            }
        }
    }
    println!(
        "1. collinear triples where the f64 cross product is NOT zero: {float_wrong}/{total} (integers: 0/{total})"
    );
    assert!(float_wrong > 0);

    // 2. hull
    let mut max_h = 0;
    for trial in 0..300 {
        let n = 3 + (trial % 40);
        let range = if trial % 3 == 0 { 6 } else { 1000 }; // tiny ranges force many collinear points and duplicates
        let pts: Vec<Pt> = (0..n)
            .map(|_| p(rng.range(0, range), rng.range(0, range)))
            .collect();
        let h = convex_hull(&pts);
        assert_eq!(h, gift_wrap(&pts), "monotone chain == gift wrapping");
        let m = h.len();
        if m >= 3 {
            assert!(
                (0..m).all(|i| cross(h[i], h[(i + 1) % m], h[(i + 2) % m]) > 0),
                "strictly convex"
            );
            assert!(
                pts.iter()
                    .all(|&q| (0..m).all(|i| cross(h[i], h[(i + 1) % m], q) >= 0)),
                "all points inside or on"
            );
        }
        max_h = max_h.max(m);
    }
    let pts: Vec<Pt> = (0..60)
        .map(|_| p(rng.range(0, 60), rng.range(0, 24)))
        .collect();
    let hull = convex_hull(&pts);
    println!(
        "2. hull OK on 300 random sets (largest hull {max_h}); picture: {} points, hull has {} vertices (H)",
        pts.len(),
        hull.len()
    );
    let mut cv = Canvas::new(61, 25, &pts);
    for &q in &pts {
        cv.point(q, '.');
    }
    for i in 0..hull.len() {
        cv.line(hull[i], hull[(i + 1) % hull.len()], '*');
    }
    for &q in &hull {
        cv.point(q, 'H');
    }
    cv.print();
    cv.gallery("r257_hull");

    // 3. segments
    let mut kinds = std::collections::BTreeMap::new();
    for _ in 0..200_000 {
        let mut q = || p(rng.range(0, 6), rng.range(0, 6));
        let (a, b, c, d) = (q(), q(), q(), q());
        if a == b || c == d {
            continue;
        }
        let got = seg_intersect(a, b, c, d);
        assert_eq!(got, seg_oracle(a, b, c, d), "{a:?} {b:?} {c:?} {d:?}");
        assert_eq!(got, seg_kind(a, b, c, d) != "none");
        *kinds.entry(seg_kind(a, b, c, d)).or_insert(0) += 1;
    }
    assert!(
        ["none", "proper", "touch", "overlap"]
            .iter()
            .all(|k| kinds.get(k).copied().unwrap_or(0) > 100)
    );
    println!("3. segment intersection == rational oracle on 200k pairs; kinds seen: {kinds:?}");

    // 4/5. polygons
    let l_shape = [p(0, 0), p(4, 0), p(4, 2), p(2, 2), p(2, 4), p(0, 4)];
    assert_eq!(area2(&l_shape), 24);
    assert!(
        winding(&l_shape, p(1, 3)) != 0
            && winding(&l_shape, p(3, 3)) == 0
            && ray_cast(&l_shape, p(1, 3))
            && !ray_cast(&l_shape, p(3, 3))
    );
    let mut tested = 0;
    for _ in 0..200 {
        let sz = 6 + rng.range(0, 8) as usize;
        let poly = random_polygon(&mut rng, sz);
        for _ in 0..200 {
            let q = p(rng.range(-1000, 1001), rng.range(-1000, 1001));
            if on_boundary(&poly, q) {
                continue;
            }
            assert_eq!(ray_cast(&poly, q), winding(&poly, q) != 0);
            tested += 1;
        }
    }
    let (cx, cy) = centroid(&[p(0, 0), p(6, 0), p(6, 3), p(0, 3)]);
    assert!((cx - 3.0).abs() < 1e-12 && (cy - 1.5).abs() < 1e-12);
    println!(
        "4. ray casting == winding number on {tested} random queries; 5. shoelace area/centroid exact on a rectangle"
    );

    // 6. closest pair
    for trial in 0..200 {
        let n = 2 + trial * 3;
        let range = if trial % 2 == 0 { 50 } else { 100_000 };
        let pts: Vec<Pt> = (0..n)
            .map(|_| p(rng.range(0, range), rng.range(0, range)))
            .collect();
        assert_eq!(closest_pair_dc(&pts), closest_pair_brute(&pts));
    }
    let big: Vec<Pt> = (0..200_000)
        .map(|_| p(rng.range(0, 1_000_000_000), rng.range(0, 1_000_000_000)))
        .collect();
    let t = std::time::Instant::now();
    let d = closest_pair_dc(&big);
    println!(
        "6. closest pair D&C == O(n^2) on 200 sets; 200000 points in {:?} (min dist^2 = {d})",
        t.elapsed()
    );

    // 7. ear clipping
    let mut shown = false;
    for _ in 0..300 {
        let sz = 5 + rng.range(0, 12) as usize;
        let poly = random_polygon(&mut rng, sz);
        let tris = ear_clip(&poly);
        let total: i128 = tris.iter().map(|t| area2(t)).sum();
        assert_eq!(
            total,
            area2(&poly),
            "triangle areas sum to the polygon area"
        );
        assert!(tris.len() <= poly.len() - 2 && tris.iter().all(|t| area2(t) > 0));
        let scaled: Vec<Pt> = poly.iter().map(|q| p(q.x * 3, q.y * 3)).collect();
        for t in &tris {
            let c3 = p(t[0].x + t[1].x + t[2].x, t[0].y + t[1].y + t[2].y); // 3 * centroid, stays integral
            assert!(
                !on_boundary(&scaled, c3) && winding(&scaled, c3) != 0,
                "triangle lies inside"
            );
        }
        // centroid via triangles (area-weighted) == polygon centroid
        let (mut sx, mut sy) = (0.0, 0.0);
        for t in &tris {
            let (c, a) = (centroid(t), area2(t) as f64);
            sx += c.0 * a;
            sy += c.1 * a;
        }
        let pc = centroid(&poly);
        assert!((sx / total as f64 - pc.0).abs() < 1e-6 && (sy / total as f64 - pc.1).abs() < 1e-6);
        if !shown && poly.len() >= 10 {
            shown = true;
            println!(
                "7. ear clipping: {} vertices -> {} triangles (areas sum exactly)",
                poly.len(),
                tris.len()
            );
            let mut cv = Canvas::new(61, 25, &poly);
            for i in 0..poly.len() {
                cv.line(poly[i], poly[(i + 1) % poly.len()], '#');
            }
            for t in &tris {
                for i in 0..3 {
                    cv.line(t[i], t[(i + 1) % 3], '.');
                }
            }
            cv.print();
            cv.gallery("r257_triangulation");
        }
    }

    // 8. Delaunay + Voronoi
    for trial in 0..30 {
        let n = 10 + trial * 6;
        let mut pts: Vec<Pt> = (0..n)
            .map(|_| p(rng.range(0, 1000), rng.range(0, 1000)))
            .collect();
        pts.sort();
        pts.dedup();
        let n = pts.len();
        let (tris, h) = check_delaunay(&pts);
        // Voronoi dual: each triangle <-> a Voronoi vertex = circumcenter, equidistant to its 3 sites, no site closer
        let f = |k: usize| (pts[k].x as f64, pts[k].y as f64);
        for t in &tris {
            let c = circumcenter(f(t[0]), f(t[1]), f(t[2]));
            let dist = |k: usize| ((f(k).0 - c.0).powi(2) + (f(k).1 - c.1).powi(2)).sqrt();
            let r = dist(t[0]);
            assert!(
                (dist(t[1]) - r).abs() < 1e-6 * r.max(1.0)
                    && (dist(t[2]) - r).abs() < 1e-6 * r.max(1.0)
            );
            assert!((0..n).all(|k| dist(k) >= r - 1e-6 * r.max(1.0)));
        }
        if trial == 29 {
            println!(
                "8. Delaunay on {n} points: {} triangles = 2n-2-h with h={h}; empty circumcircles verified exactly",
                tris.len()
            );
            println!(
                "   Voronoi dual: {} Voronoi vertices (one per triangle); cells of the {h} hull sites are unbounded",
                tris.len()
            );
        }
    }

    // hull triangles so thin that their circles are enormous, and the largest allowed coordinates
    check_delaunay(&[p(0, 0), p(1_000_000, 0), p(500_000, 1), p(500_000, 500_000)]);
    const M: i64 = 300_000_000;
    check_delaunay(&[p(-M, -M), p(M, -M), p(-M, M), p(M - 1, M - 1), p(0, 1)]);
    // not sorted: the last point lands ON a hull edge that already exists
    check_delaunay(&[p(0, 0), p(4, 0), p(0, 4), p(2, 0)]);
    // small sets: tiny ranges give collinear and cocircular points, wide ones give thin hull triangles
    for trial in 0..4000 {
        let range = if trial % 2 == 0 { 5 } else { 100_000_000 };
        let mut pts: Vec<Pt> = (0..8)
            .map(|_| p(rng.range(0, range), rng.range(0, range)))
            .collect();
        pts.sort();
        pts.dedup();
        if convex_hull(&pts).len() >= 3 {
            let (tris, _) = check_delaunay(&pts);
            let mut doubled = pts.clone();
            doubled.extend_from_slice(&pts);
            assert_eq!(delaunay(&doubled).len(), tris.len(), "repeats are ignored");
        }
    }
    assert!(delaunay(&[p(0, 0), p(1, 1), p(2, 2)]).is_empty() && delaunay(&[]).is_empty());

    // 9. rotating calipers
    for trial in 0..300 {
        let n = 1 + trial % 50;
        let pts: Vec<Pt> = (0..n)
            .map(|_| p(rng.range(-500, 500), rng.range(-500, 500)))
            .collect();
        let brute = (0..n)
            .flat_map(|i| (0..n).map(move |j| (i, j)))
            .map(|(i, j)| d2(pts[i], pts[j]))
            .max()
            .unwrap();
        assert_eq!(diameter2_calipers(&convex_hull(&pts)), brute);
    }
    println!("9. rotating calipers diameter == brute force on 300 sets");

    // 10. SAT
    let (mut hit, mut miss) = (0, 0);
    for _ in 0..3000 {
        let off = rng.range(0, 90);
        let mut shape = |cx: i64| {
            let pts: Vec<Pt> = (0..8)
                .map(|_| p(cx + rng.range(-30, 30), rng.range(-30, 30)))
                .collect();
            convex_hull(&pts)
        };
        let a = shape(0);
        let b = shape(off);
        if a.len() < 3 || b.len() < 3 {
            continue;
        }
        let r = sat_collide(&a, &b);
        assert_eq!(r, brute_collide(&a, &b));
        if r { hit += 1 } else { miss += 1 }
    }
    assert!(hit > 100 && miss > 100);
    println!(
        "10. SAT == brute-force overlap on {} random convex pairs ({hit} colliding, {miss} separated)",
        hit + miss
    );
    println!("all geometry checks passed");
}
