//! RECIPE 258 (simulation): physics simulation: integrators, fixed timestep, cloth, collisions, N-body.
//! LEVEL: advanced
//! Use in: game physics, particle systems, orbital mechanics demos, scientific simulation, anything that steps a system forward in time.   Run: cargo run --release --example physics_simulation
//! COMMON USES: game engines (fixed timestep + accumulator, broad-phase grids, position-based cloth), molecular dynamics (velocity Verlet), astrophysics (tree codes such as Barnes-Hut), robotics/animation (constraint relaxation), deterministic replays and lockstep netcode.
#![allow(dead_code)]
//!
//! The integrator decides whether a simulation stays believable. Explicit Euler ADDS energy every step
//! (it explodes), semi-implicit (symplectic) Euler and velocity Verlet keep a nearby "shadow" energy
//! bounded forever, and RK4 is the most accurate per step but slowly drifts. We measure all four on a
//! harmonic oscillator (closed form known) and on a Kepler orbit (energy must be constant).
//!
//! Then the engineering around the integrator: a fixed timestep with an accumulator (bit-identical
//! results regardless of frame rate) vs a variable timestep (frame-rate dependent); a position-based
//! cloth (Verlet + distance-constraint relaxation); impulse-based elastic circle collisions (momentum and
//! kinetic energy conserved); a uniform-grid broad phase checked against O(n^2); and a Barnes-Hut quadtree
//! (O(n log n) gravity, error controlled by the opening angle theta).
//! Pictures are ASCII; PPM files are only written when COOKBOOK_GALLERY names a directory.
//! Read: https://en.wikipedia.org/wiki/Verlet_integration
//! Read: https://en.wikipedia.org/wiki/Barnes%E2%80%93Hut_simulation
//! Read: https://floating-point-gui.de/

use std::collections::BTreeSet;
use std::time::Instant;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }
}

type V2 = [f64; 2];
fn add(a: V2, b: V2) -> V2 {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: V2, b: V2) -> V2 {
    [a[0] - b[0], a[1] - b[1]]
}
fn mul(a: V2, s: f64) -> V2 {
    [a[0] * s, a[1] * s]
}
fn dot(a: V2, b: V2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn len(a: V2) -> f64 {
    dot(a, a).sqrt()
}

/// ASCII/PPM canvas over world coordinates.
struct Plot {
    w: usize,
    h: usize,
    cells: Vec<Vec<char>>,
    min: V2,
    max: V2,
}
impl Plot {
    fn new(w: usize, h: usize, min: V2, max: V2) -> Self {
        Plot {
            w,
            h,
            cells: vec![vec![' '; w]; h],
            min,
            max,
        }
    }
    fn put(&mut self, q: V2, ch: char, overwrite: bool) {
        let gx = ((q[0] - self.min[0]) / (self.max[0] - self.min[0]) * (self.w - 1) as f64).round();
        let gy = ((q[1] - self.min[1]) / (self.max[1] - self.min[1]) * (self.h - 1) as f64).round();
        if gx < 0.0 || gy < 0.0 || gx >= self.w as f64 || gy >= self.h as f64 {
            return;
        }
        let (x, y) = (gx as usize, self.h - 1 - gy as usize);
        if overwrite || self.cells[y][x] == ' ' {
            self.cells[y][x] = ch;
        }
    }
    fn line(&mut self, a: V2, b: V2, ch: char) {
        let steps = 2 * self.w.max(self.h);
        for s in 0..=steps {
            self.put(add(a, mul(sub(b, a), s as f64 / steps as f64)), ch, false);
        }
    }
    fn print(&self) {
        for row in &self.cells {
            println!("    {}", row.iter().collect::<String>().trim_end());
        }
    }
    fn gallery(&self, name: &str) {
        let Ok(dir) = std::env::var("COOKBOOK_GALLERY") else {
            return;
        };
        let s = 8;
        let mut out = format!("P6\n{} {}\n255\n", self.w * s, self.h * s).into_bytes();
        for gy in 0..self.h * s {
            for gx in 0..self.w * s {
                out.extend(match self.cells[gy / s][gx / s] {
                    ' ' => [15, 15, 25],
                    '.' | '-' => [90, 100, 140],
                    _ => [250, 200, 70],
                });
            }
        }
        let _ = std::fs::write(format!("{dir}/{name}.ppm"), out);
    }
}

// ---------------- 1. integrators ----------------
#[derive(Clone, Copy, PartialEq, Debug)]
enum Method {
    Euler,
    Symplectic,
    Verlet,
    Rk4,
}
const METHODS: [Method; 4] = [
    Method::Euler,
    Method::Symplectic,
    Method::Verlet,
    Method::Rk4,
];

fn step(m: Method, x: &mut V2, v: &mut V2, dt: f64, acc: &dyn Fn(V2) -> V2) {
    match m {
        Method::Euler => {
            let a = acc(*x);
            *x = add(*x, mul(*v, dt)); // position uses the OLD velocity
            *v = add(*v, mul(a, dt));
        }
        Method::Symplectic => {
            *v = add(*v, mul(acc(*x), dt)); // velocity first,
            *x = add(*x, mul(*v, dt)); // then position uses the NEW velocity
        }
        Method::Verlet => {
            let a = acc(*x);
            *x = add(add(*x, mul(*v, dt)), mul(a, 0.5 * dt * dt));
            let a2 = acc(*x);
            *v = add(*v, mul(add(a, a2), 0.5 * dt));
        }
        Method::Rk4 => {
            let (k1x, k1v) = (*v, acc(*x));
            let (k2x, k2v) = (
                add(*v, mul(k1v, dt / 2.0)),
                acc(add(*x, mul(k1x, dt / 2.0))),
            );
            let (k3x, k3v) = (
                add(*v, mul(k2v, dt / 2.0)),
                acc(add(*x, mul(k2x, dt / 2.0))),
            );
            let (k4x, k4v) = (add(*v, mul(k3v, dt)), acc(add(*x, mul(k3x, dt))));
            for k in 0..2 {
                x[k] += dt / 6.0 * (k1x[k] + 2.0 * k2x[k] + 2.0 * k3x[k] + k4x[k]);
                v[k] += dt / 6.0 * (k1v[k] + 2.0 * k2v[k] + 2.0 * k3v[k] + k4v[k]);
            }
        }
    }
}
fn osc_acc(x: V2) -> V2 {
    [-x[0], 0.0]
}
fn osc_energy(x: V2, v: V2) -> f64 {
    0.5 * (dot(v, v) + dot(x, x))
}
fn kepler_acc(x: V2) -> V2 {
    let r = len(x);
    mul(x, -1.0 / (r * r * r)) // GM = 1
}
fn kepler_energy(x: V2, v: V2) -> f64 {
    0.5 * dot(v, v) - 1.0 / len(x)
}

/// Returns (max relative energy deviation, final energy / initial energy).
fn energy_run(
    m: Method,
    x0: V2,
    v0: V2,
    dt: f64,
    steps: usize,
    acc: &dyn Fn(V2) -> V2,
    energy: &dyn Fn(V2, V2) -> f64,
) -> (f64, f64) {
    let (mut x, mut v) = (x0, v0);
    let e0 = energy(x, v);
    let mut worst = 0f64;
    for _ in 0..steps {
        step(m, &mut x, &mut v, dt, acc);
        worst = worst.max(((energy(x, v) - e0) / e0).abs());
    }
    (worst, energy(x, v) / e0)
}

fn integrators_section() {
    println!("1. integrators: oscillator x''=-x (dt=0.1) and Kepler orbit (dt=0.01, 10 orbits)");
    println!(
        "   {:<11} {:>12} {:>12} {:>14} {:>13}",
        "method", "osc E(100)/E0", "osc max dE/E", "osc err at t=10", "kepler max dE/E"
    );
    let mut res = Vec::new();
    for &m in &METHODS {
        let (worst, ratio) =
            energy_run(m, [1.0, 0.0], [0.0, 0.0], 0.1, 1000, &osc_acc, &osc_energy);
        let (mut x, mut v) = ([1.0, 0.0], [0.0, 0.0]);
        for _ in 0..100 {
            step(m, &mut x, &mut v, 0.1, &osc_acc);
        }
        let err = (x[0] - 10f64.cos()).abs();
        let (kw, _) = energy_run(
            m,
            [1.0, 0.0],
            [0.0, 0.8],
            0.01,
            4000,
            &kepler_acc,
            &kepler_energy,
        );
        println!(
            "   {:<11} {:>12.4} {:>12.2e} {:>14.2e} {:>13.2e}",
            format!("{m:?}"),
            ratio,
            worst,
            err,
            kw
        );
        res.push((worst, ratio, err, kw));
    }
    let [eu, sy, ve, rk] = [res[0], res[1], res[2], res[3]];
    assert!(
        eu.1 > 1e3,
        "explicit Euler explodes: E multiplies by (1+dt^2) every step"
    );
    assert!(
        sy.0 < 0.1 && ve.0 < 0.02,
        "symplectic and Verlet stay bounded"
    );
    assert!(
        rk.2 < ve.2 && ve.2 < sy.2 && sy.2 < eu.2,
        "short-term position error ordering RK4 < Verlet < symplectic < Euler"
    );
    assert!(eu.3 > 10.0 * sy.3, "Euler Kepler drift dwarfs symplectic");
    assert!(
        sy.3 < 0.1 && ve.3 < 5e-3 && rk.3 < ve.3,
        "Kepler: bounded for symplectic/Verlet, RK4 smallest"
    );

    // ASCII orbit from the Verlet run
    let mut pl = Plot::new(61, 21, [-1.5, -1.0], [0.5, 1.0]);
    let (mut x, mut v) = ([1.0, 0.0], [0.0, 0.8]);
    for i in 0..4000 {
        step(Method::Verlet, &mut x, &mut v, 0.01, &kepler_acc);
        if i % 5 == 0 {
            pl.put(x, '.', false);
        }
    }
    pl.put([0.0, 0.0], 'O', true);
    println!(
        "   Kepler orbit (velocity Verlet, 10 revolutions overlap exactly: the ellipse does not precess or spiral):"
    );
    pl.print();
    pl.gallery("r258_orbit");
}

// ---------------- 2. fixed vs variable timestep ----------------
fn pend_step(th: &mut f64, w: &mut f64, dt: f64) {
    *w += -9.81 * th.sin() * dt; // symplectic Euler
    *th += *w * dt;
}
fn frames(rng: &mut Rng, total_us: u64) -> Vec<u64> {
    let (mut v, mut sum) = (Vec::new(), 0);
    while sum < total_us {
        let f = ((rng.range(4_000.0, 40_000.0)) as u64).min(total_us - sum);
        v.push(f);
        sum += f;
    }
    v
}
fn run_fixed(fr: &[u64]) -> (f64, f64) {
    const TICK: u64 = 10_000; // microseconds: integer time, so no float drift in the accumulator
    let (mut th, mut w, mut acc) = (1.0, 0.0, 0u64);
    for &f in fr {
        acc += f;
        while acc >= TICK {
            pend_step(&mut th, &mut w, 0.01);
            acc -= TICK;
        }
    }
    (th, w)
}
fn run_variable(fr: &[u64]) -> (f64, f64) {
    let (mut th, mut w) = (1.0, 0.0);
    for &f in fr {
        pend_step(&mut th, &mut w, f as f64 * 1e-6);
    }
    (th, w)
}
fn timestep_section(rng: &mut Rng) {
    let (a, b) = (frames(rng, 5_000_000), frames(rng, 5_000_000));
    assert_ne!(a.len(), b.len());
    let (fa, fb) = (run_fixed(&a), run_fixed(&b));
    assert_eq!(
        (fa.0.to_bits(), fa.1.to_bits()),
        (fb.0.to_bits(), fb.1.to_bits()),
        "fixed timestep: identical for any frame pacing"
    );
    assert_eq!(run_fixed(&a).0.to_bits(), fa.0.to_bits(), "and repeatable");
    let (va, vb) = (run_variable(&a), run_variable(&b));
    assert!(
        (va.0 - vb.0).abs() > 1e-6,
        "variable timestep depends on the frame pacing"
    );
    println!(
        "2. 5 s pendulum, two different frame pacings ({} vs {} frames):",
        a.len(),
        b.len()
    );
    println!(
        "   fixed step    theta = {:.15} / {:.15}  (bit-identical)",
        fa.0, fb.0
    );
    println!(
        "   variable step theta = {:.15} / {:.15}  (differ by {:.1e})",
        va.0,
        vb.0,
        (va.0 - vb.0).abs()
    );
}

// ---------------- 3. cloth ----------------
struct Cloth {
    pos: Vec<V2>,
    prev: Vec<V2>,
    pinned: Vec<bool>,
    links: Vec<(usize, usize, f64)>,
}
impl Cloth {
    fn new(w: usize, h: usize, pin_gap: f64) -> Self {
        let mut pos = Vec::new();
        let off = (w - 1) as f64 / 2.0;
        for j in 0..h {
            for i in 0..w {
                pos.push([i as f64 - off, -(j as f64)]);
            }
        }
        let mut pinned = vec![false; w * h];
        for (i, q) in pos.iter().enumerate().take(w) {
            pinned[i] = (q[0].abs() - pin_gap / 2.0).abs() < 0.5; // pin two top points, pin_gap apart
        }
        let mut links = Vec::new();
        for j in 0..h {
            for i in 0..w {
                if i + 1 < w {
                    links.push((j * w + i, j * w + i + 1, 1.0));
                }
                if j + 1 < h {
                    links.push((j * w + i, (j + 1) * w + i, 1.0));
                }
            }
        }
        // squeeze the pins together so the net must hang in a curve
        let mut c = Cloth {
            prev: pos.clone(),
            pos,
            pinned,
            links,
        };
        let pins: Vec<usize> = (0..w * h).filter(|&i| c.pinned[i]).collect();
        let (l, r) = (pins[0], pins[pins.len() - 1]);
        c.pos[l][0] = -pin_gap / 2.0;
        c.pos[r][0] = pin_gap / 2.0;
        c.prev = c.pos.clone();
        c
    }
    fn step(&mut self, dt: f64, iters: usize) {
        for i in 0..self.pos.len() {
            if self.pinned[i] {
                continue;
            }
            let vel = mul(sub(self.pos[i], self.prev[i]), 0.99); // damping
            self.prev[i] = self.pos[i];
            self.pos[i] = add(add(self.pos[i], vel), [0.0, -9.81 * dt * dt]); // Verlet: no explicit velocity
        }
        for _ in 0..iters {
            for &(a, b, rest) in &self.links {
                let d = sub(self.pos[b], self.pos[a]);
                let dist = len(d);
                let corr = mul(d, (dist - rest) / dist);
                let (wa, wb) = (
                    if self.pinned[a] { 0.0 } else { 1.0 },
                    if self.pinned[b] { 0.0 } else { 1.0 },
                );
                let tot = wa + wb;
                self.pos[a] = add(self.pos[a], mul(corr, wa / tot));
                self.pos[b] = sub(self.pos[b], mul(corr, wb / tot));
            }
        }
    }
    fn max_stretch(&self) -> f64 {
        self.links
            .iter()
            .map(|&(a, b, rest)| ((len(sub(self.pos[b], self.pos[a])) - rest) / rest).abs())
            .fold(0.0, f64::max)
    }
    fn max_speed(&self, dt: f64) -> f64 {
        (0..self.pos.len())
            .map(|i| len(sub(self.pos[i], self.prev[i])) / dt)
            .fold(0.0, f64::max)
    }
}
fn cloth_section() {
    let (w, h, dt) = (14, 8, 1.0 / 60.0);
    let mut c = Cloth::new(w, h, 9.0);
    let mut early = 0.0;
    for s in 0..3000 {
        c.step(dt, 60);
        if s == 30 {
            early = c.max_speed(dt);
        }
    }
    let (speed, stretch) = (c.max_speed(dt), c.max_stretch());
    println!(
        "3. cloth {w}x{h}, 2 pins 9 apart: speed {early:.2} (t=0.5s) -> {speed:.1e} (t=50s), worst link stretch {:.2}%",
        stretch * 100.0
    );
    assert!(speed < 1e-3 && speed < early, "settles");
    assert!(
        stretch < 0.05,
        "constraint relaxation keeps edges within 5% of rest length under gravity"
    );
    let mut pl = Plot::new(61, 16, [-8.0, -9.5], [8.0, 1.0]);
    for &(a, b, _) in &c.links {
        pl.line(c.pos[a], c.pos[b], '-');
    }
    for q in &c.pos {
        pl.put(*q, 'o', true);
    }
    for (i, q) in c.pos.iter().enumerate() {
        if c.pinned[i] {
            pl.put(*q, 'P', true);
        }
    }
    pl.print();
    pl.gallery("r258_cloth");
}

// ---------------- 4. circle collisions ----------------
#[derive(Clone)]
struct Circle {
    p: V2,
    v: V2,
    m: f64,
    r: f64,
}
/// Impulse resolution along the contact normal; e = 1 is perfectly elastic. Returns true if overlapping.
fn resolve(a: &mut Circle, b: &mut Circle, e: f64) -> bool {
    let d = sub(b.p, a.p);
    let dist = len(d);
    if dist >= a.r + b.r || dist == 0.0 {
        return false;
    }
    let n = mul(d, 1.0 / dist);
    let vn = dot(sub(b.v, a.v), n);
    if vn < 0.0 {
        let j = -(1.0 + e) * vn / (1.0 / a.m + 1.0 / b.m);
        a.v = sub(a.v, mul(n, j / a.m));
        b.v = add(b.v, mul(n, j / b.m));
    }
    // push apart (positional correction does not touch velocities, so it cannot change energy)
    let pen = a.r + b.r - dist;
    a.p = sub(a.p, mul(n, pen * (1.0 / a.m) / (1.0 / a.m + 1.0 / b.m)));
    b.p = add(b.p, mul(n, pen * (1.0 / b.m) / (1.0 / a.m + 1.0 / b.m)));
    true
}
fn kinetic(cs: &[Circle]) -> f64 {
    cs.iter().map(|c| 0.5 * c.m * dot(c.v, c.v)).sum()
}
fn collisions_section(rng: &mut Rng) {
    for _ in 0..10_000 {
        let mut a = Circle {
            p: [0.0, 0.0],
            v: [rng.range(-3.0, 3.0), rng.range(-3.0, 3.0)],
            m: rng.range(0.5, 5.0),
            r: 1.0,
        };
        let ang = rng.range(0.0, std::f64::consts::TAU);
        let mut b = Circle {
            p: [1.9 * ang.cos(), 1.9 * ang.sin()],
            v: [rng.range(-3.0, 3.0), rng.range(-3.0, 3.0)],
            m: rng.range(0.5, 5.0),
            r: 1.0,
        };
        let mom = |a: &Circle, b: &Circle| add(mul(a.v, a.m), mul(b.v, b.m));
        let (p0, k0) = (mom(&a, &b), kinetic(&[a.clone(), b.clone()]));
        let before = dot(sub(b.v, a.v), sub(b.p, a.p));
        resolve(&mut a, &mut b, 1.0);
        let (p1, k1) = (mom(&a, &b), kinetic(&[a.clone(), b.clone()]));
        assert!(len(sub(p1, p0)) < 1e-9 && (k1 - k0).abs() < 1e-9 * k0.max(1.0));
        if before < 0.0 {
            assert!(
                dot(sub(b.v, a.v), sub(b.p, a.p)) >= -1e-12,
                "approaching pair now separates"
            );
        }
    }
    // a box full of circles: walls reflect, collisions elastic -> kinetic energy constant
    let (bw, bh) = (40.0, 30.0);
    let mut cs: Vec<Circle> = Vec::new();
    while cs.len() < 40 {
        let c = Circle {
            p: [rng.range(1.0, bw - 1.0), rng.range(1.0, bh - 1.0)],
            v: [rng.range(-5.0, 5.0), rng.range(-5.0, 5.0)],
            m: rng.range(0.5, 3.0),
            r: rng.range(0.5, 1.0),
        };
        if cs.iter().all(|o| len(sub(o.p, c.p)) > o.r + c.r) {
            cs.push(c);
        }
    }
    let k0 = kinetic(&cs);
    let mut hits = 0;
    for _ in 0..3000 {
        for c in cs.iter_mut() {
            c.p = add(c.p, mul(c.v, 0.01));
            if c.p[0] < c.r || c.p[0] > bw - c.r {
                c.v[0] = -c.v[0];
                c.p[0] = c.p[0].clamp(c.r, bw - c.r);
            }
            if c.p[1] < c.r || c.p[1] > bh - c.r {
                c.v[1] = -c.v[1];
                c.p[1] = c.p[1].clamp(c.r, bh - c.r);
            }
        }
        for i in 0..cs.len() {
            for j in i + 1..cs.len() {
                let (l, r) = cs.split_at_mut(j);
                hits += resolve(&mut l[i], &mut r[0], 1.0) as usize;
            }
        }
    }
    let k1 = kinetic(&cs);
    assert!((k1 - k0).abs() < 1e-9 * k0, "elastic: KE conserved");
    println!(
        "4. impulse collisions: 10000 random pairs conserve momentum+KE to 1e-9; box of 40 circles: {hits} contacts, KE {k0:.6} -> {k1:.6}"
    );
}

// ---------------- 5. broad phase ----------------
fn pairs_brute(cs: &[Circle]) -> BTreeSet<(usize, usize)> {
    let mut s = BTreeSet::new();
    for i in 0..cs.len() {
        for j in i + 1..cs.len() {
            if len(sub(cs[i].p, cs[j].p)) < cs[i].r + cs[j].r {
                s.insert((i, j));
            }
        }
    }
    s
}
/// Uniform grid, cell >= 2 * max radius: a circle can only touch circles in its own or the 8 neighbouring cells.
fn pairs_grid(cs: &[Circle], world: f64) -> BTreeSet<(usize, usize)> {
    let cell = 2.0 * cs.iter().map(|c| c.r).fold(0.0, f64::max);
    let n = (world / cell).ceil() as usize + 1;
    let mut grid: Vec<Vec<usize>> = vec![Vec::new(); n * n];
    let key = |c: &Circle| ((c.p[0] / cell) as usize, (c.p[1] / cell) as usize);
    for (i, c) in cs.iter().enumerate() {
        let (cx, cy) = key(c);
        grid[cy * n + cx].push(i);
    }
    let mut s = BTreeSet::new();
    for (i, c) in cs.iter().enumerate() {
        let (cx, cy) = key(c);
        for ny in cy.saturating_sub(1)..=(cy + 1).min(n - 1) {
            for nx in cx.saturating_sub(1)..=(cx + 1).min(n - 1) {
                for &j in &grid[ny * n + nx] {
                    if j > i && len(sub(c.p, cs[j].p)) < c.r + cs[j].r {
                        s.insert((i, j));
                    }
                }
            }
        }
    }
    s
}
fn broadphase_section(rng: &mut Rng) {
    let world = 300.0;
    let cs: Vec<Circle> = (0..2000)
        .map(|_| Circle {
            p: [rng.range(0.0, world), rng.range(0.0, world)],
            v: [0.0; 2],
            m: 1.0,
            r: rng.range(0.5, 2.0),
        })
        .collect();
    let (a, b) = (pairs_brute(&cs), pairs_grid(&cs, world));
    assert_eq!(a, b);
    assert!(a.len() > 50);
    let reps = 20;
    let t = Instant::now();
    for _ in 0..reps {
        std::hint::black_box(pairs_brute(std::hint::black_box(&cs)));
    }
    let tb = t.elapsed();
    let t = Instant::now();
    for _ in 0..reps {
        std::hint::black_box(pairs_grid(std::hint::black_box(&cs), world));
    }
    let tg = t.elapsed();
    println!(
        "5. broad phase on 2000 circles: {} identical colliding pairs; brute {:?}, grid {:?}, speed-up x{:.1}",
        a.len(),
        tb / reps,
        tg / reps,
        tb.as_secs_f64() / tg.as_secs_f64()
    );
}

// ---------------- 6. Barnes-Hut ----------------
const NONE: usize = usize::MAX;
struct Node {
    cx: f64,
    cy: f64,
    half: f64,
    mass: f64,
    com: V2,
    kids: [usize; 4],
    body: usize,
}
struct QuadTree {
    nodes: Vec<Node>,
}
impl QuadTree {
    fn new_node(cx: f64, cy: f64, half: f64) -> Node {
        Node {
            cx,
            cy,
            half,
            mass: 0.0,
            com: [0.0; 2],
            kids: [NONE; 4],
            body: NONE,
        }
    }
    fn build(pos: &[V2], mass: &[f64]) -> Self {
        let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
        for q in pos {
            for k in 0..2 {
                lo[k] = lo[k].min(q[k]);
                hi[k] = hi[k].max(q[k]);
            }
        }
        let half = (hi[0] - lo[0]).max(hi[1] - lo[1]) / 2.0 * 1.0001 + 1e-9;
        let mut t = QuadTree {
            nodes: vec![Self::new_node(
                (lo[0] + hi[0]) / 2.0,
                (lo[1] + hi[1]) / 2.0,
                half,
            )],
        };
        for i in 0..pos.len() {
            t.insert(0, i, pos);
        }
        t.moments(0, pos, mass);
        t
    }
    fn child(&mut self, node: usize, p: V2) -> usize {
        let n = &self.nodes[node];
        let q = (p[0] >= n.cx) as usize + 2 * (p[1] >= n.cy) as usize;
        if n.kids[q] == NONE {
            let h = n.half / 2.0;
            let (cx, cy) = (
                n.cx + if q & 1 == 1 { h } else { -h },
                n.cy + if q & 2 == 2 { h } else { -h },
            );
            self.nodes.push(Self::new_node(cx, cy, h));
            let id = self.nodes.len() - 1;
            self.nodes[node].kids[q] = id;
        }
        self.nodes[node].kids[q]
    }
    fn insert(&mut self, node: usize, b: usize, pos: &[V2]) {
        let leaf = self.nodes[node].kids == [NONE; 4];
        if leaf && self.nodes[node].body == NONE {
            self.nodes[node].body = b;
            return;
        }
        if leaf {
            let old = std::mem::replace(&mut self.nodes[node].body, NONE);
            let c = self.child(node, pos[old]);
            self.insert(c, old, pos);
        }
        let c = self.child(node, pos[b]);
        self.insert(c, b, pos);
    }
    fn moments(&mut self, node: usize, pos: &[V2], mass: &[f64]) {
        let b = self.nodes[node].body;
        if b != NONE {
            self.nodes[node].mass = mass[b];
            self.nodes[node].com = pos[b];
            return;
        }
        let (mut m, mut c) = (0.0, [0.0; 2]);
        for k in self.nodes[node].kids {
            if k != NONE {
                self.moments(k, pos, mass);
                m += self.nodes[k].mass;
                c = add(c, mul(self.nodes[k].com, self.nodes[k].mass));
            }
        }
        self.nodes[node].mass = m;
        self.nodes[node].com = mul(c, 1.0 / m);
    }
    fn accel(&self, node: usize, i: usize, pos: &[V2], eps2: f64, theta: f64) -> V2 {
        let n = &self.nodes[node];
        if n.mass == 0.0 || n.body == i {
            return [0.0; 2];
        }
        let d = sub(n.com, pos[i]);
        let r2 = dot(d, d) + eps2;
        if n.body != NONE || (2.0 * n.half).powi(2) < theta * theta * r2 {
            return mul(d, n.mass / (r2 * r2.sqrt())); // leaf, or far enough: treat the cell as a point mass
        }
        let mut a = [0.0; 2];
        for k in n.kids {
            if k != NONE {
                a = add(a, self.accel(k, i, pos, eps2, theta));
            }
        }
        a
    }
}
fn accel_brute(pos: &[V2], mass: &[f64], i: usize, eps2: f64) -> V2 {
    let mut a = [0.0; 2];
    for j in 0..pos.len() {
        if j != i {
            let d = sub(pos[j], pos[i]);
            let r2 = dot(d, d) + eps2;
            a = add(a, mul(d, mass[j] / (r2 * r2.sqrt())));
        }
    }
    a
}
fn total_energy(pos: &[V2], vel: &[V2], mass: &[f64], eps2: f64) -> f64 {
    let mut e = 0.0;
    for i in 0..pos.len() {
        e += 0.5 * mass[i] * dot(vel[i], vel[i]);
        for j in i + 1..pos.len() {
            let d = sub(pos[j], pos[i]);
            e -= mass[i] * mass[j] / (dot(d, d) + eps2).sqrt();
        }
    }
    e
}
fn nbody_section(rng: &mut Rng) {
    let n = 600;
    let eps2 = 1e-4;
    let pos: Vec<V2> = (0..n)
        .map(|_| {
            [
                rng.range(0.0, 1.0) + rng.range(0.0, 1.0),
                rng.range(0.0, 1.0),
            ]
        })
        .collect();
    let mass: Vec<f64> = (0..n).map(|_| rng.range(0.5, 1.5) / n as f64).collect();
    let tree = QuadTree::build(&pos, &mass);
    let mut errs = Vec::new();
    println!(
        "6. Barnes-Hut, {n} bodies, {} tree nodes: mean relative force error vs brute force",
        tree.nodes.len()
    );
    for theta in [1.0, 0.5, 0.2, 0.0] {
        let mut sum = 0.0;
        for i in 0..n {
            let (a, b) = (
                tree.accel(0, i, &pos, eps2, theta),
                accel_brute(&pos, &mass, i, eps2),
            );
            sum += len(sub(a, b)) / len(b);
        }
        errs.push(sum / n as f64);
        println!("   theta={theta:<4} error {:.2e}", sum / n as f64);
    }
    assert!(errs[1] < 0.02, "theta=0.5 is accurate to ~1%");
    assert!(
        errs[0] > errs[1] && errs[1] > errs[2] && errs[3] < 1e-12,
        "error shrinks with theta; theta=0 is exact"
    );

    // short leapfrog (kick-drift-kick) run using the tree for forces
    let m = 150;
    let mut pos: Vec<V2> = (0..m)
        .map(|_| {
            loop {
                let q = [rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)];
                if dot(q, q) < 1.0 {
                    break q;
                }
            }
        })
        .collect();
    let mass: Vec<f64> = vec![1.0 / m as f64; m];
    let mut vel: Vec<V2> = pos.iter().map(|q| [-q[1] * 0.4, q[0] * 0.4]).collect(); // gentle rotation
    let (eps2, dt) = (0.05f64.powi(2), 0.002);
    let e0 = total_energy(&pos, &vel, &mass, eps2);
    let accs = |pos: &[V2]| -> Vec<V2> {
        let t = QuadTree::build(pos, &mass);
        (0..m).map(|i| t.accel(0, i, pos, eps2, 0.5)).collect()
    };
    let mut a = accs(&pos);
    for _ in 0..250 {
        for i in 0..m {
            vel[i] = add(vel[i], mul(a[i], dt / 2.0));
            pos[i] = add(pos[i], mul(vel[i], dt));
        }
        a = accs(&pos);
        for i in 0..m {
            vel[i] = add(vel[i], mul(a[i], dt / 2.0));
        }
    }
    let e1 = total_energy(&pos, &vel, &mass, eps2);
    println!(
        "   leapfrog + tree, {m} bodies, 250 steps: energy {e0:.6} -> {e1:.6} (drift {:.2e})",
        ((e1 - e0) / e0).abs()
    );
    assert!(
        ((e1 - e0) / e0).abs() < 0.01,
        "energy approximately conserved"
    );
}

fn main() {
    let mut rng = Rng(0x9E3779B97F4A7C15);
    integrators_section();
    timestep_section(&mut rng);
    cloth_section();
    collisions_section(&mut rng);
    broadphase_section(&mut rng);
    nbody_section(&mut rng);
    println!("all physics checks passed");
}
