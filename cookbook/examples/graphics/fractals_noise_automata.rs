//! RECIPE 256 (graphics): generative graphics: fractals, noise, L-systems, cellular automata, reaction-diffusion, contours.
//! LEVEL: intermediate
//! Use in: procedural textures and terrain, game worlds, generative art, simulation toys, map contouring.   Run: cargo run --release --example fractals_noise_automata
#![allow(dead_code)]
//!
//! MANDELBROT: iterate z -> z^2 + c from z=0; c is in the set if |z| stays bounded. SMOOTH COLOURING removes the
//! visible bands of integer escape counts: mu = n + 1 - log2(ln|z_n|) is continuous. Points in the main CARDIOID or the
//! period-2 BULB never escape, so a closed-form test skips their (max-iteration) work; we assert identical results.
//! PERLIN (gradient) noise: random unit gradients on lattice points, dot with the offset vector, blend with the quintic
//! fade 6t^5-15t^4+10t^3 (zero first AND second derivative at lattice points). It is 0 at the lattice, bounded by
//! sqrt(2)/2 in 2D, deterministic from a seed, and CONTINUOUS. VALUE noise interpolates random lattice values instead.
//! fBm sums octaves: frequency x2, amplitude x0.5 each. WORLEY noise: one random feature point per cell; F1/F2 are the
//! distances to the nearest/second nearest point, so only the 3x3 neighbouring cells need checking.
//! L-SYSTEMS: parallel string rewriting then a TURTLE (F forward, +/- turn, [ ] push/pop state); fit to the canvas
//! using the bounding box. LIFE: a cell lives with 3 neighbours, or survives with 2. ELEMENTARY CA: the next cell is
//! bit (4l+2c+r) of the rule number; Rule 90 = left XOR right = Pascal's triangle mod 2 = Sierpinski triangle.
//! GRAY-SCOTT: du = Du lap(u) - u v^2 + F(1-u), dv = Dv lap(v) + u v^2 - (F+k) v. MARCHING SQUARES: classify each grid
//! cell's 4 corners as above/below the level (16 cases), interpolate the crossing on each edge; every crossing edge is
//! shared by exactly two cells, so contours close (we pad the field below the level to guarantee no open ends).
//! COMMON USES: games (terrain, caves, clouds, textures from noise), film VFX (procedural materials), GIS (contour lines,
//! isobars), medical imaging (iso-surfaces use marching cubes, the 3D cousin), generative art, biology/chemistry toy models.
//! Read: https://www.redblobgames.com/ and https://github.com/ssloy/tinyrenderer/wiki

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn f(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}
fn hash2(x: i64, y: i64, seed: u64) -> u64 {
    let mut z = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ seed.wrapping_mul(0x1656_67B1_9E37_79F9);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

// ---------- Mandelbrot / Julia ----------
const BAILOUT2: f64 = 65536.0; // large bailout makes the smooth formula accurate
/// Returns (iteration count, smooth value). Interior points return (max_iter, max_iter).
fn escape(mut x: f64, mut y: f64, cx: f64, cy: f64, max_iter: u32) -> (u32, f64) {
    for n in 0..max_iter {
        let (x2, y2) = (x * x, y * y);
        if x2 + y2 > BAILOUT2 {
            let mu = n as f64 + 1.0 - (0.5 * (x2 + y2).ln()).ln() / std::f64::consts::LN_2;
            return (n, mu);
        }
        y = 2.0 * x * y + cy;
        x = x2 - y2 + cx;
    }
    (max_iter, max_iter as f64)
}
fn in_cardioid_or_bulb(x: f64, y: f64) -> bool {
    let q = (x - 0.25) * (x - 0.25) + y * y;
    q * (q + (x - 0.25)) < 0.25 * y * y || (x + 1.0) * (x + 1.0) + y * y < 0.0625
}
fn mandelbrot(cx: f64, cy: f64, max_iter: u32, shortcut: bool) -> (u32, f64) {
    if shortcut && in_cardioid_or_bulb(cx, cy) {
        return (max_iter, max_iter as f64);
    }
    escape(0.0, 0.0, cx, cy, max_iter)
}
fn julia(x: f64, y: f64, c: (f64, f64), max_iter: u32) -> (u32, f64) {
    escape(x, y, c.0, c.1, max_iter)
}

// ---------- noise ----------
fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}
struct Noise {
    seed: u64,
}
impl Noise {
    fn grad(&self, ix: i64, iy: i64) -> (f64, f64) {
        let a = (hash2(ix, iy, self.seed) % 8) as f64 * std::f64::consts::FRAC_PI_4; // 8 unit directions
        (a.cos(), a.sin())
    }
    fn perlin(&self, x: f64, y: f64) -> f64 {
        let (xi, yi) = (x.floor(), y.floor());
        let (xf, yf) = (x - xi, y - yi);
        let (xi, yi) = (xi as i64, yi as i64);
        let corner = |dx: i64, dy: i64| {
            let g = self.grad(xi + dx, yi + dy);
            g.0 * (xf - dx as f64) + g.1 * (yf - dy as f64)
        };
        let (u, v) = (fade(xf), fade(yf));
        lerp(
            lerp(corner(0, 0), corner(1, 0), u),
            lerp(corner(0, 1), corner(1, 1), u),
            v,
        )
    }
    fn value(&self, x: f64, y: f64) -> f64 {
        let (xi, yi) = (x.floor(), y.floor());
        let (u, v) = (fade(x - xi), fade(y - yi));
        let (xi, yi) = (xi as i64, yi as i64);
        let at = |dx: i64, dy: i64| {
            (hash2(xi + dx, yi + dy, self.seed ^ 0x55) >> 11) as f64 / (1u64 << 53) as f64
        };
        lerp(lerp(at(0, 0), at(1, 0), u), lerp(at(0, 1), at(1, 1), u), v)
    }
    /// fBm of Perlin noise, normalised by the total amplitude so |result| <= sqrt(2)/2.
    fn fbm(&self, x: f64, y: f64, octaves: u32) -> f64 {
        let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
        for o in 0..octaves {
            sum += amp * self.perlin(x * freq + 17.3 * o as f64, y * freq - 9.1 * o as f64);
            norm += amp;
            amp *= 0.5;
            freq *= 2.0;
        }
        sum / norm
    }
    fn cell_point(&self, cx: i64, cy: i64) -> (f64, f64) {
        let h = hash2(cx, cy, self.seed ^ 0xABCD);
        (
            cx as f64 + (h & 0xFFFF_FFFF) as f64 / 4294967296.0,
            cy as f64 + (h >> 32) as f64 / 4294967296.0,
        )
    }
    /// Worley: (F1, F2) distances to the nearest and second-nearest feature points, from the 3x3 neighbourhood.
    fn worley(&self, x: f64, y: f64) -> (f64, f64) {
        self.worley_r(x, y, 1)
    }
    fn worley_r(&self, x: f64, y: f64, r: i64) -> (f64, f64) {
        let (cx, cy) = (x.floor() as i64, y.floor() as i64);
        let (mut f1, mut f2) = (f64::MAX, f64::MAX);
        for j in -r..=r {
            for i in -r..=r {
                let p = self.cell_point(cx + i, cy + j);
                let d = ((p.0 - x).powi(2) + (p.1 - y).powi(2)).sqrt();
                if d < f1 {
                    f2 = f1;
                    f1 = d;
                } else if d < f2 {
                    f2 = d;
                }
            }
        }
        (f1, f2)
    }
}

// ---------- L-systems ----------
fn expand(axiom: &str, rules: &[(char, &str)], n: u32) -> String {
    let mut s = axiom.to_string();
    for _ in 0..n {
        s = s
            .chars()
            .map(|c| {
                rules
                    .iter()
                    .find(|r| r.0 == c)
                    .map_or(c.to_string(), |r| r.1.to_string())
            })
            .collect();
    }
    s
}
type Seg = (f64, f64, f64, f64);
/// Turtle interpretation: F/G forward (F draws), +/- turn, [ ] push/pop. Heading 0 = up. Returns segments and end position.
fn turtle(s: &str, angle_deg: f64) -> (Vec<Seg>, (f64, f64)) {
    let (mut x, mut y, mut a) = (0.0f64, 0.0f64, 90.0f64);
    let (mut stack, mut segs) = (vec![], vec![]);
    for c in s.chars() {
        match c {
            'F' | 'G' => {
                let (nx, ny) = (x + a.to_radians().cos(), y + a.to_radians().sin());
                if c == 'F' {
                    segs.push((x, y, nx, ny));
                }
                x = nx;
                y = ny;
            }
            '+' => a += angle_deg,
            '-' => a -= angle_deg,
            '[' => stack.push((x, y, a)),
            ']' => {
                let t = stack.pop().expect("balanced brackets");
                x = t.0;
                y = t.1;
                a = t.2;
            }
            _ => {}
        }
    }
    (segs, (x, y))
}
/// Bounding-box auto-fit: uniform scale + centring into w x h (y flipped to screen coordinates).
fn fit(segs: &[Seg], w: i32, h: i32, margin: f64) -> Vec<(i32, i32, i32, i32)> {
    let xs = segs.iter().flat_map(|s| [s.0, s.2]);
    let ys = segs.iter().flat_map(|s| [s.1, s.3]);
    let (x0, x1) = (
        xs.clone().fold(f64::MAX, f64::min),
        xs.fold(f64::MIN, f64::max),
    );
    let (y0, y1) = (
        ys.clone().fold(f64::MAX, f64::min),
        ys.fold(f64::MIN, f64::max),
    );
    let sc = ((w as f64 - 2.0 * margin) / (x1 - x0).max(1e-9))
        .min((h as f64 - 2.0 * margin) / (y1 - y0).max(1e-9));
    let (ox, oy) = (
        (w as f64 - (x1 - x0) * sc) / 2.0,
        (h as f64 - (y1 - y0) * sc) / 2.0,
    );
    let map = |x: f64, y: f64| {
        (
            ((x - x0) * sc + ox).round() as i32,
            (h as f64 - ((y - y0) * sc + oy)).round() as i32,
        )
    };
    segs.iter()
        .map(|s| {
            let (a, b) = (map(s.0, s.1), map(s.2, s.3));
            (a.0, a.1, b.0, b.1)
        })
        .collect()
}

// ---------- cellular automata ----------
#[derive(Clone, PartialEq)]
struct Life {
    w: usize,
    h: usize,
    c: Vec<bool>,
}
impl Life {
    fn new(w: usize, h: usize, cells: &[(usize, usize)]) -> Life {
        let mut l = Life {
            w,
            h,
            c: vec![false; w * h],
        };
        for &(x, y) in cells {
            l.c[y * w + x] = true;
        }
        l
    }
    fn step(&self) -> Life {
        let mut n = Life {
            w: self.w,
            h: self.h,
            c: vec![false; self.w * self.h],
        };
        for y in 0..self.h {
            for x in 0..self.w {
                let mut cnt = 0;
                for dy in [self.h - 1, 0, 1] {
                    for dx in [self.w - 1, 0, 1] {
                        if (dx, dy) != (0, 0)
                            && self.c[((y + dy) % self.h) * self.w + (x + dx) % self.w]
                        {
                            cnt += 1;
                        }
                    }
                }
                n.c[y * self.w + x] = matches!((self.c[y * self.w + x], cnt), (true, 2) | (_, 3));
            }
        }
        n
    }
    fn cells(&self) -> Vec<(usize, usize)> {
        (0..self.w * self.h)
            .filter(|&i| self.c[i])
            .map(|i| (i % self.w, i / self.w))
            .collect()
    }
}
/// One step of a 1D elementary CA on a ZERO-padded (non-wrapping) row.
fn eca_step(row: &[u8], rule: u8) -> Vec<u8> {
    (0..row.len())
        .map(|i| {
            let l = if i > 0 { row[i - 1] } else { 0 };
            let r = if i + 1 < row.len() { row[i + 1] } else { 0 };
            (rule >> (l << 2 | row[i] << 1 | r)) & 1
        })
        .collect()
}
fn eca_run(width: usize, rows: usize, rule: u8) -> Vec<Vec<u8>> {
    let mut cur = vec![0u8; width];
    cur[width / 2] = 1;
    let mut out = vec![cur.clone()];
    for _ in 1..rows {
        cur = eca_step(&cur, rule);
        out.push(cur.clone());
    }
    out
}

// ---------- Gray-Scott ----------
struct GrayScott {
    n: usize,
    u: Vec<f64>,
    v: Vec<f64>,
    f: f64,
    k: f64,
}
fn laplacian(g: &[f64], n: usize, x: usize, y: usize) -> f64 {
    let at = |x: usize, y: usize| g[(y % n) * n + x % n];
    at(x + 1, y) + at(x + n - 1, y) + at(x, y + 1) + at(x, y + n - 1) - 4.0 * at(x, y)
}
impl GrayScott {
    fn new(n: usize, f: f64, k: f64, seed: u64) -> GrayScott {
        let (mut u, mut v) = (vec![1.0; n * n], vec![0.0; n * n]);
        let mut r = Rng(seed);
        for y in 0..n {
            for x in 0..n {
                let (dx, dy) = (x as f64 - n as f64 / 2.0, y as f64 - n as f64 / 2.0);
                if dx.abs() < n as f64 / 8.0 && dy.abs() < n as f64 / 8.0 {
                    u[y * n + x] = 0.5;
                    v[y * n + x] = 0.25 + 0.02 * r.f();
                }
            }
        }
        GrayScott { n, u, v, f, k }
    }
    fn step(&mut self) {
        let (n, dt, du, dv) = (self.n, 1.0, 0.16, 0.08);
        let (mut nu, mut nv) = (self.u.clone(), self.v.clone());
        for y in 0..n {
            for x in 0..n {
                let i = y * n + x;
                let (u, v) = (self.u[i], self.v[i]);
                let uvv = u * v * v;
                nu[i] = u + dt * (du * laplacian(&self.u, n, x, y) - uvv + self.f * (1.0 - u));
                nv[i] = v + dt * (dv * laplacian(&self.v, n, x, y) + uvv - (self.f + self.k) * v);
            }
        }
        self.u = nu;
        self.v = nv;
    }
}

// ---------- marching squares ----------
type Edge = (bool, usize, usize); // (horizontal?, i, j): horizontal edge joins vertex (i,j)-(i+1,j); vertical (i,j)-(i,j+1)
struct Contours {
    segs: Vec<(Edge, Edge)>,
}
fn marching_squares(f: &[f64], w: usize, h: usize, iso: f64) -> Contours {
    let at = |i: usize, j: usize| f[j * w + i];
    let mut segs = vec![];
    for j in 0..h - 1 {
        for i in 0..w - 1 {
            let inside = |v: f64| (v >= iso) as usize;
            let idx = inside(at(i, j))
                | inside(at(i + 1, j)) << 1
                | inside(at(i + 1, j + 1)) << 2
                | inside(at(i, j + 1)) << 3;
            let (t, b, l, r): (Edge, Edge, Edge, Edge) = (
                (true, i, j),
                (true, i, j + 1),
                (false, i, j),
                (false, i + 1, j),
            );
            let centre = (at(i, j) + at(i + 1, j) + at(i + 1, j + 1) + at(i, j + 1)) / 4.0 >= iso;
            match idx {
                0 | 15 => {}
                1 | 14 => segs.push((l, t)),
                2 | 13 => segs.push((t, r)),
                3 | 12 => segs.push((l, r)),
                4 | 11 => segs.push((r, b)),
                6 | 9 => segs.push((t, b)),
                7 | 8 => segs.push((b, l)),
                5 | 10 => {
                    // saddle: the centre value decides whether the inside corners are joined
                    let join_inside = centre;
                    let a_in = idx == 5; // corners a,c inside when 5; b,d inside when 10
                    if join_inside == a_in {
                        segs.extend([(t, r), (b, l)]);
                    } else {
                        segs.extend([(l, t), (r, b)]);
                    }
                }
                _ => unreachable!(),
            }
        }
    }
    Contours { segs }
}
fn edge_point(e: Edge, f: &[f64], w: usize, iso: f64) -> (f64, f64) {
    let (a, b) = if e.0 {
        ((e.1, e.2), (e.1 + 1, e.2))
    } else {
        ((e.1, e.2), (e.1, e.2 + 1))
    };
    let (fa, fb) = (f[a.1 * w + a.0], f[b.1 * w + b.0]);
    let t = (iso - fa) / (fb - fa);
    (
        a.0 as f64 + (b.0 - a.0) as f64 * t,
        a.1 as f64 + (b.1 - a.1) as f64 * t,
    )
}

// ---------- helpers for the mosaic ----------
fn palette(t: f64, phase: [f64; 3]) -> [u8; 3] {
    [0, 1, 2].map(|i| ((0.5 + 0.5 * (std::f64::consts::TAU * (t + phase[i])).cos()) * 255.0) as u8)
}
struct Pane {
    w: usize,
    h: usize,
    px: Vec<[u8; 3]>,
}
impl Pane {
    fn new(w: usize, h: usize) -> Pane {
        Pane {
            w,
            h,
            px: vec![[8, 10, 16]; w * h],
        }
    }
    fn put(&mut self, x: i32, y: i32, c: [u8; 3]) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            let w = self.w;
            self.px[y as usize * w + x as usize] = c;
        }
    }
    fn line(&mut self, a: (i32, i32), b: (i32, i32), c: [u8; 3]) {
        let (mut x, mut y) = a;
        let (dx, dy) = ((b.0 - x).abs(), -(b.1 - y).abs());
        let (sx, sy, mut e) = (
            if x < b.0 { 1 } else { -1 },
            if y < b.1 { 1 } else { -1 },
            dx + dy,
        );
        loop {
            self.put(x, y, c);
            if (x, y) == b {
                break;
            }
            let e2 = 2 * e;
            if e2 >= dy {
                e += dy;
                x += sx;
            }
            if e2 <= dx {
                e += dx;
                y += sy;
            }
        }
    }
}

// ---------- tests ----------
fn test_mandelbrot() {
    let mut saved = 0u64;
    let mut checked = 0;
    for j in 0..150 {
        for i in 0..200 {
            let (cx, cy) = (-2.2 + 3.0 * i as f64 / 200.0, -1.1 + 2.2 * j as f64 / 150.0);
            let (a, _) = mandelbrot(cx, cy, 300, true);
            let (b, _) = mandelbrot(cx, cy, 300, false);
            assert_eq!(a, b, "shortcut changed the result at c = ({cx}, {cy})");
            if in_cardioid_or_bulb(cx, cy) {
                saved += 300;
            }
            checked += 1;
        }
    }
    println!(
        "mandelbrot: cardioid/bulb shortcut identical to brute force on {checked} points (skips {saved} iterations)"
    );
    // smooth colouring: across a tiny step the integer count jumps by 1 but mu moves smoothly
    let (mut max_dn, mut max_dmu) = (0i64, 0.0f64);
    let mut prev = mandelbrot(0.5, 0.7, 500, true);
    for i in 1..4000 {
        let cur = mandelbrot(0.5 + i as f64 * 2e-5, 0.7, 500, true);
        max_dn = max_dn.max((cur.0 as i64 - prev.0 as i64).abs());
        max_dmu = max_dmu.max((cur.1 - prev.1).abs());
        prev = cur;
    }
    assert!(max_dn >= 1, "the integer count must show banding steps");
    assert!(
        max_dmu < 0.1,
        "smooth value is continuous across band edges: max step {max_dmu}"
    );
    println!(
        "smooth colouring: integer count jumps by {max_dn}, smooth value never moves more than {max_dmu:.4} per step"
    );
    let (n, mu) = julia(1.4, 1.2, (-0.8, 0.156), 200);
    assert!(n < 200 && mu > n as f64 - 4.0 && mu <= n as f64 + 2.0);
}

fn test_noise() {
    let (a, b) = (Noise { seed: 1 }, Noise { seed: 2 });
    let mut r = Rng(31337);
    let (mut diff_seeds, mut max_slope, mut max_fbm_slope) = (0, 0.0f64, 0.0f64);
    for _ in 0..50_000 {
        let (x, y) = (r.f() * 200.0 - 100.0, r.f() * 200.0 - 100.0);
        let (p, q) = (a.perlin(x, y), a.value(x, y));
        assert!(
            p.abs() <= std::f64::consts::FRAC_1_SQRT_2 + 1e-12,
            "perlin bounded: {p}"
        );
        assert!((0.0..=1.0).contains(&q), "value noise in [0,1]");
        assert_eq!(
            p,
            Noise { seed: 1 }.perlin(x, y),
            "deterministic from the seed"
        );
        if (p - b.perlin(x, y)).abs() > 1e-9 {
            diff_seeds += 1;
        }
        let d = 1e-3;
        max_slope = max_slope.max((a.perlin(x + d, y + d) - p).abs() / (d * 2f64.sqrt()));
        max_fbm_slope =
            max_fbm_slope.max((a.fbm(x + d, y + d, 5) - a.fbm(x, y, 5)).abs() / (d * 2f64.sqrt()));
        assert!(a.fbm(x, y, 6).abs() <= std::f64::consts::FRAC_1_SQRT_2 + 1e-12);
    }
    assert!(diff_seeds > 49_000, "different seeds give different fields");
    assert!(
        max_slope < 3.0 && max_fbm_slope < 12.0,
        "nearby points give nearby values (slopes {max_slope:.2}, {max_fbm_slope:.2})"
    );
    for (i, j) in [(0.0, 0.0), (3.0, -7.0), (12.0, 5.0)] {
        assert!(
            a.perlin(i, j).abs() < 1e-12,
            "gradient noise is 0 at lattice points"
        );
    }
    let lat = (hash2(5, 9, 1 ^ 0x55) >> 11) as f64 / (1u64 << 53) as f64;
    assert!(
        (a.value(5.0, 9.0) - lat).abs() < 1e-12,
        "value noise equals the lattice value there"
    );
    println!(
        "noise: bounded, deterministic, continuous (max slope perlin {max_slope:.2}, fbm {max_fbm_slope:.2}) over 50k points"
    );
    // worley: 3x3 search equals a wider brute-force search for F1
    let (mut f2_ok, n) = (0, 3000);
    for _ in 0..n {
        let (x, y) = (r.f() * 50.0, r.f() * 50.0);
        let (f1, f2) = a.worley(x, y);
        let (g1, g2) = a.worley_r(x, y, 3);
        assert!(
            (f1 - g1).abs() < 1e-12 && f1 <= f2,
            "F1 from the 3x3 neighbourhood is exact"
        );
        if (f2 - g2).abs() < 1e-12 {
            f2_ok += 1;
        }
    }
    println!(
        "worley: F1 exact on {n} points; F2 matches wide search on {:.1}%",
        100.0 * f2_ok as f64 / n as f64
    );
}

fn test_lsystems() -> Vec<(&'static str, Vec<Seg>)> {
    let koch = expand("F++F++F", &[('F', "F-F++F-F")], 4);
    assert_eq!(koch.matches('F').count(), 3 * 4usize.pow(4));
    let (ks, end) = turtle(&koch, 60.0);
    assert!(
        end.0.abs() < 1e-6 && end.1.abs() < 1e-6,
        "the Koch snowflake is a closed curve"
    );
    for (x0, y0, x1, y1) in fit(&ks, 133, 100, 4.0) {
        assert!(
            (0..133).contains(&x0)
                && (0..133).contains(&x1)
                && (0..100).contains(&y0)
                && (0..100).contains(&y1),
            "auto-fit stays on canvas"
        );
    }
    let dragon = expand("FX", &[('X', "X+YF+"), ('Y', "-FX-Y")], 11);
    assert_eq!(
        dragon.matches('F').count(),
        1 << 11,
        "dragon curve has 2^n segments"
    );
    let (ds, _) = turtle(&dragon, 90.0);
    let plant = expand("X", &[('X', "F+[[X]-X]-F[-FX]+X"), ('F', "FF")], 5);
    let (ps, _) = turtle(&plant, 25.0);
    assert!(plant.matches('[').count() == plant.matches(']').count() && ps.len() > 500);
    println!(
        "l-systems: koch {} segments (closed), dragon {} segments, plant {} segments",
        ks.len(),
        ds.len(),
        ps.len()
    );
    vec![("koch", ks), ("dragon", ds), ("plant", ps)]
}

fn test_life_and_eca() {
    let blinker = Life::new(8, 8, &[(3, 4), (4, 4), (5, 4)]);
    assert!(
        blinker.step() != blinker && blinker.step().step() == blinker,
        "blinker has period 2"
    );
    let block = Life::new(8, 8, &[(2, 2), (3, 2), (2, 3), (3, 3)]);
    assert!(block.step() == block, "block is a still life");
    let glider_cells = [(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)];
    let g0 = Life::new(20, 20, &glider_cells);
    let g4 = (0..4).fold(g0.clone(), |g, _| g.step());
    let moved: Vec<(usize, usize)> = glider_cells.iter().map(|&(x, y)| (x + 1, y + 1)).collect();
    assert!(
        g4 == Life::new(20, 20, &moved),
        "glider moves by (1,1) every 4 generations"
    );
    let g80 = (0..80).fold(g0.clone(), |g, _| g.step());
    assert!(
        g80 == g0,
        "on a 20x20 torus the glider returns home after 80 generations"
    );
    println!("life: blinker p2, block still, glider (+1,+1)/4 gens verified; glider after 4 gens:");
    for y in 0..6 {
        println!(
            "    {}",
            (0..8)
                .map(|x| if g4.c[y * 20 + x] { '#' } else { '.' })
                .collect::<String>()
        );
    }
    // Rule 90 == Pascal's triangle mod 2 (single seed, zero-padded)
    let t = 64;
    let width = 2 * t + 3;
    let rows = eca_run(width, t, 90);
    let mut pascal = vec![vec![1u8]];
    for n in 1..t {
        let p = &pascal[n - 1];
        pascal.push(
            (0..=n)
                .map(|k| (if k > 0 { p[k - 1] } else { 0 } + if k < n { p[k] } else { 0 }) % 2)
                .collect(),
        );
    }
    let c = width / 2;
    for n in 0..t {
        for k in 0..=n {
            // C(n,k) sits at column c + (2k - n)
            assert_eq!(
                rows[n][(c as i64 + 2 * k as i64 - n as i64) as usize],
                pascal[n][k],
                "rule 90 row {n} col {k}"
            );
        }
    }
    for n in [1usize, 3, 7, 15, 31] {
        assert_eq!(
            rows[n].iter().map(|&v| v as usize).sum::<usize>(),
            n + 1,
            "row 2^k-1 is all odd"
        );
    }
    // generic rule sanity: 204 identity, 170 copies the right neighbour, 240 the left, 0 clears, 255 fills
    let mut r = Rng(5);
    let row: Vec<u8> = (0..40).map(|_| (r.next() & 1) as u8).collect();
    assert_eq!(eca_step(&row, 204), row);
    assert_eq!(&eca_step(&row, 170)[..39], &row[1..]);
    assert_eq!(&eca_step(&row, 240)[1..], &row[..39]);
    assert!(
        eca_step(&row, 0).iter().all(|&v| v == 0) && eca_step(&row, 255).iter().all(|&v| v == 1)
    );
    println!(
        "eca: rule 90 equals Pascal's triangle mod 2 for {t} rows; rules 204/170/240/0/255 behave as specified"
    );
}

fn test_gray_scott() -> GrayScott {
    let mut r = Rng(3);
    let field: Vec<f64> = (0..32 * 32).map(|_| r.f()).collect();
    let total: f64 = (0..32)
        .flat_map(|y| (0..32).map(move |x| (x, y)))
        .map(|(x, y)| laplacian(&field, 32, x, y))
        .sum();
    assert!(
        total.abs() < 1e-9,
        "periodic Laplacian sums to zero (diffusion conserves mass)"
    );
    let mut gs = GrayScott::new(64, 0.035, 0.065, 11);
    for s in 0..4000 {
        gs.step();
        if s % 500 == 0 {
            assert!(
                gs.u.iter()
                    .chain(&gs.v)
                    .all(|&x| (-1e-9..=1.0 + 1e-9).contains(&x)),
                "concentrations stay in [0,1]"
            );
        }
    }
    let m = gs.v.iter().sum::<f64>() / gs.v.len() as f64;
    let var = gs.v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / gs.v.len() as f64;
    assert!(
        var > 1e-4 && m > 0.0,
        "a pattern formed (variance {var:.4})"
    );
    println!("gray-scott: 4000 steps on 64x64, v mean {m:.3}, variance {var:.4} (pattern present)");
    gs
}

fn contour_field(n: &Noise, w: usize, h: usize) -> Vec<f64> {
    let mut f = vec![0.0; w * h];
    for j in 0..h {
        for i in 0..w {
            f[j * w + i] = if i == 0 || j == 0 || i == w - 1 || j == h - 1 {
                -1.0
            } else {
                n.fbm(i as f64 * 0.17, j as f64 * 0.17, 3)
            };
        }
    }
    f
}
fn test_marching(n: &Noise) {
    let (w, h) = (34, 26);
    let f = contour_field(n, w, h);
    for iso in [-0.1, 0.0, 0.15] {
        let c = marching_squares(&f, w, h, iso);
        let mut deg = std::collections::HashMap::new();
        for (a, b) in &c.segs {
            *deg.entry(*a).or_insert(0) += 1;
            *deg.entry(*b).or_insert(0) += 1;
        }
        // independent count of sign-changing grid edges
        let mut crossing = 0;
        for j in 0..h {
            for i in 0..w {
                for (di, dj) in [(1, 0), (0, 1)] {
                    if i + di < w
                        && j + dj < h
                        && (f[j * w + i] >= iso) != (f[(j + dj) * w + i + di] >= iso)
                    {
                        crossing += 1;
                    }
                }
            }
        }
        assert_eq!(
            deg.len(),
            crossing,
            "every crossing edge is used, and only those"
        );
        assert!(
            deg.values().all(|&d| d == 2),
            "each crossing edge is shared by exactly two segments: contours are closed"
        );
        for e in deg.keys() {
            let p = edge_point(*e, &f, w, iso);
            assert!(p.0 >= 0.0 && p.1 >= 0.0 && p.0 <= (w - 1) as f64 && p.1 <= (h - 1) as f64);
        }
        // count closed loops by union-find over segments
        let mut parent: std::collections::HashMap<Edge, Edge> =
            deg.keys().map(|&e| (e, e)).collect();
        fn find(p: &mut std::collections::HashMap<Edge, Edge>, e: Edge) -> Edge {
            let q = p[&e];
            if q == e {
                e
            } else {
                let r = find(p, q);
                p.insert(e, r);
                r
            }
        }
        for (a, b) in &c.segs {
            let (ra, rb) = (find(&mut parent, *a), find(&mut parent, *b));
            parent.insert(ra, rb);
        }
        let keys: Vec<Edge> = parent.keys().cloned().collect();
        let loops = keys
            .iter()
            .map(|&e| find(&mut parent, e))
            .collect::<std::collections::HashSet<_>>()
            .len();
        println!(
            "marching squares iso={iso:>5}: {} segments, {} crossing edges all degree 2, {} closed loops",
            c.segs.len(),
            crossing,
            loops
        );
    }
}

fn main() {
    println!("== generative graphics ==");
    test_mandelbrot();
    test_noise();
    let ls = test_lsystems();
    test_life_and_eca();
    let gs = test_gray_scott();
    let noise = Noise { seed: 2024 };
    test_marching(&noise);

    // mosaic: 3x3 panes of 133x100
    let (pw, ph) = (133usize, 100usize);
    let mut panes: Vec<Pane> = vec![];
    // 1 mandelbrot
    let mut p = Pane::new(pw, ph);
    for y in 0..ph {
        for x in 0..pw {
            let (cx, cy) = (
                -2.2 + 3.0 * x as f64 / pw as f64,
                -1.13 + 2.26 * y as f64 / ph as f64,
            );
            let (n, mu) = mandelbrot(cx, cy, 200, true);
            p.px[y * pw + x] = if n >= 200 {
                [0, 0, 0]
            } else {
                palette(mu * 0.03, [0.0, 0.1, 0.2])
            };
        }
    }
    panes.push(p);
    // 2 julia
    let mut p = Pane::new(pw, ph);
    for y in 0..ph {
        for x in 0..pw {
            let (zx, zy) = (
                -1.6 + 3.2 * x as f64 / pw as f64,
                -1.2 + 2.4 * y as f64 / ph as f64,
            );
            let (n, mu) = julia(zx, zy, (-0.8, 0.156), 200);
            p.px[y * pw + x] = if n >= 200 {
                [0, 0, 0]
            } else {
                palette(mu * 0.04 + 0.5, [0.0, 0.33, 0.67])
            };
        }
    }
    panes.push(p);
    // 3 perlin fbm terrain colours
    let mut p = Pane::new(pw, ph);
    for y in 0..ph {
        for x in 0..pw {
            let v = noise.fbm(x as f64 * 0.04, y as f64 * 0.04, 5) * 1.4 + 0.5;
            p.px[y * pw + x] = if v < 0.42 {
                [20, 50, (110.0 + 200.0 * v) as u8]
            } else if v < 0.5 {
                [210, 195, 140]
            } else if v < 0.75 {
                [40, (90.0 + 160.0 * (v - 0.5)) as u8, 50]
            } else {
                let g = (150.0 + 200.0 * (v - 0.75)).min(255.0) as u8;
                [g, g, g]
            };
        }
    }
    panes.push(p);
    // 4 worley
    let mut p = Pane::new(pw, ph);
    for y in 0..ph {
        for x in 0..pw {
            let (f1, f2) = noise.worley(x as f64 / 14.0, y as f64 / 14.0);
            let e = (f2 - f1 * 1.0).clamp(0.0, 1.0);
            let c = palette(f1 * 0.5, [0.55, 0.65, 0.8]);
            let k = (e * 3.0).min(1.0);
            p.px[y * pw + x] = c.map(|v| (v as f64 * k) as u8);
        }
    }
    panes.push(p);
    // 5 koch, 6 plant
    for (name, col) in [("koch", [120, 200, 255]), ("plant", [110, 220, 110])] {
        let mut p = Pane::new(pw, ph);
        let segs = &ls.iter().find(|s| s.0 == name).unwrap().1;
        for (x0, y0, x1, y1) in fit(segs, pw as i32, ph as i32, 4.0) {
            p.line((x0, y0), (x1, y1), col);
        }
        panes.push(p);
    }
    // 7 rule 30 | rule 90
    let mut p = Pane::new(pw, ph);
    for (k, rule) in [30u8, 90].into_iter().enumerate() {
        let rows = eca_run(2 * ph + 3, ph, rule);
        for y in 0..ph {
            for x in 0..pw / 2 {
                if rows[y][ph + 1 - pw / 4 + x] == 1 {
                    p.put(
                        (k * (pw / 2) + x) as i32,
                        y as i32,
                        if k == 0 {
                            [255, 180, 80]
                        } else {
                            [140, 160, 255]
                        },
                    );
                }
            }
        }
    }
    panes.push(p);
    // 8 gray-scott
    let mut p = Pane::new(pw, ph);
    for y in 0..ph {
        for x in 0..pw {
            let (gx, gy) = ((x * 64 / pw).min(63), (y * 64 / ph).min(63));
            p.px[y * pw + x] = palette(0.65 - gs.v[gy * 64 + gx] * 1.2, [0.0, 0.1, 0.2]);
        }
    }
    panes.push(p);
    // 9 marching squares
    let (w, h) = (34, 26);
    let f = contour_field(&noise, w, h);
    let mut p = Pane::new(pw, ph);
    for (iso, col) in [
        (-0.1, [60, 90, 200]),
        (0.0, [90, 200, 240]),
        (0.15, [250, 230, 120]),
    ] {
        let c = marching_squares(&f, w, h, iso);
        for (a, b) in &c.segs {
            let (pa, pb) = (edge_point(*a, &f, w, iso), edge_point(*b, &f, w, iso));
            p.line(
                ((pa.0 * 4.0) as i32 - 2, (pa.1 * 4.0) as i32 - 2),
                ((pb.0 * 4.0) as i32 - 2, (pb.1 * 4.0) as i32 - 2),
                col,
            );
        }
    }
    panes.push(p);

    let (w, h) = (399usize, 300usize);
    let mut rgb = vec![0u8; w * h * 3];
    for (i, p) in panes.iter().enumerate() {
        let (ox, oy) = ((i % 3) * pw, (i / 3) * ph);
        for y in 0..ph {
            for x in 0..pw {
                let (px, o) = (p.px[y * pw + x], ((oy + y) * w + ox + x) * 3);
                // thin separators between panes
                rgb[o..o + 3].copy_from_slice(&if x == 0 || y == 0 { [40, 44, 56] } else { px });
            }
        }
    }
    assert!(rgb.iter().any(|&b| b > 200) && panes.len() == 9);
    println!(
        "mosaic: mandelbrot | julia | perlin fBm || worley | koch | plant || rule 30/90 | gray-scott | contours"
    );
    save_image("r256_mosaic", w, h, &rgb);
    ascii_preview(w, h, &rgb, 90);
    println!("all generative-graphics assertions passed");
}

// ---------- image output helpers (copied into each recipe so every file stays standalone) ----------
fn gallery_dir() -> std::path::PathBuf {
    let d = match std::env::var_os("COOKBOOK_GALLERY") {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::temp_dir().join("flowforge_gallery"),
    };
    std::fs::create_dir_all(&d).expect("create gallery dir");
    d
}

/// Binary PPM (P6): the simplest image format there is: a text header then raw RGB bytes.
fn write_ppm(path: &std::path::Path, w: usize, h: usize, rgb: &[u8]) {
    let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
    out.extend_from_slice(rgb);
    std::fs::write(path, out).expect("write ppm");
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// Minimal PNG: signature, IHDR, one IDAT holding a zlib stream of STORED (uncompressed) deflate blocks, IEND.
fn write_png(path: &std::path::Path, w: usize, h: usize, rgb: &[u8]) {
    assert_eq!(rgb.len(), w * h * 3);
    let mut raw = Vec::with_capacity((w * 3 + 1) * h);
    for y in 0..h {
        raw.push(0); // filter type 0 = none
        raw.extend_from_slice(&rgb[y * w * 3..(y + 1) * w * 3]);
    }
    let mut z = vec![0x78, 0x01]; // zlib header, no compression
    let mut chunks = raw.chunks(65535).peekable();
    while let Some(c) = chunks.next() {
        z.push(if chunks.peek().is_none() { 1 } else { 0 }); // BFINAL, BTYPE=00 (stored)
        z.extend_from_slice(&(c.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
        z.extend_from_slice(c);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        png.extend_from_slice(&body);
        png.extend_from_slice(&crc32(&body).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit, truecolour RGB
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    std::fs::write(path, png).expect("write png");
}

/// Save both PPM and PNG next to each other; prints the PNG path.
fn save_image(name: &str, w: usize, h: usize, rgb: &[u8]) {
    assert!(
        w <= 400 && h <= 300,
        "gallery images are limited to 400x300"
    );
    let dir = gallery_dir();
    write_ppm(&dir.join(format!("{name}.ppm")), w, h, rgb);
    let p = dir.join(format!("{name}.png"));
    write_png(&p, w, h, rgb);
    println!("wrote {} ({w}x{h})", p.display());
}

/// Terminal preview: average each cell's luminance and map it through a character ramp.
fn ascii_preview(w: usize, h: usize, rgb: &[u8], cols: usize) {
    let ramp = b" .:-=+*#%@";
    let cw = (w as f32 / cols as f32).max(1.0);
    let ch = cw * 2.0; // terminal cells are about twice as tall as wide
    let rows = (h as f32 / ch).floor() as usize;
    for r in 0..rows {
        let mut line = String::new();
        for c in 0..cols {
            let (x0, x1) = (
                (c as f32 * cw) as usize,
                (((c + 1) as f32 * cw) as usize).min(w),
            );
            let (y0, y1) = (
                (r as f32 * ch) as usize,
                (((r + 1) as f32 * ch) as usize).min(h),
            );
            let (mut s, mut n) = (0.0f32, 0.0f32);
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = (y * w + x) * 3;
                    s += 0.2126 * rgb[i] as f32
                        + 0.7152 * rgb[i + 1] as f32
                        + 0.0722 * rgb[i + 2] as f32;
                    n += 1.0;
                }
            }
            let l = if n > 0.0 { s / n / 255.0 } else { 0.0 };
            line.push(ramp[((l * 9.0).round() as usize).min(9)] as char);
        }
        println!("{line}");
    }
}
