//! CAPSTONE 12 (graphics): a Whitted + path-tracing ray tracer with BVH, glass, soft shadows, depth of field and PNG output
//! LEVEL: capstone
//! Use in: learning how offline renderers work, synthetic image generation, a base for experiments with sampling and acceleration structures.   Run: cargo run --release --example ray_tracer
#![allow(dead_code)]
//! OVERFLOW-CHECKS: the checkerboard regression only fails with them; `check.sh` runs this file a second time with them.
//!
//! WHAT: a self-contained CPU ray tracer. Scene = spheres, an infinite checkerboard plane and triangle meshes (a generated
//! icosphere, boxes, quads). Materials: Lambert, checker, Blinn-Phong, perfect mirror, dielectric glass (Fresnel by
//! Schlick, total internal reflection, optional Beer-Lambert absorption) and emissive. Lights: point (hard shadows) and
//! rectangular area lights (soft shadows by Monte-Carlo sampling). Camera: pinhole with vertical field of view plus an
//! optional thin lens (depth of field). Two integrators: a Whitted ray tracer (recursive mirror/glass + shadow rays) and
//! a unidirectional PATH TRACER (cosine-weighted hemisphere sampling, Russian roulette) used for a Cornell-box scene.
//! WHY: every piece is small, but they interact; the assertions at the end (BVH == brute force, thread count does not
//! change the image, white-furnace energy conservation, RMSE shrinking like 1/sqrt(N)) are the standard ways to know a
//! renderer is correct rather than merely pretty.
//! HOW (plain text):
//!   camera ray: dir = normalize(forward + (2s-1) tan(fov/2) aspect right + (1-2t) tan(fov/2) up); thin lens: pick a point on
//!     the lens disk, aim at the point where the pinhole ray crosses the focus plane, so only that plane stays sharp.
//!   sphere: |o + t d - c|^2 = r^2 -> quadratic. triangle: Moller-Trumbore. plane: t = (k - n.o) / (n.d).
//!   BVH: binary tree of axis-aligned boxes over triangles/spheres, median split on the longest centroid axis, stored in a
//!     flat Vec (depth-first: left child = index + 1, right child index stored), traversal with an explicit stack that
//!     skips boxes beyond the closest hit found so far. Planes are unbounded, so they are tested separately.
//!   determinism: the RNG for pixel (x, y) is seeded from (seed, x, y) only, and a hit tie is broken by primitive index,
//!     so the result does not depend on thread count, band order, or whether the BVH is used.
//!   Whitted shading at a diffuse hit: albedo * (ambient + sum over lights of light * max(0, n.l) / d^2 * visibility)
//!     + Blinn term ks * max(0, n.h)^shininess, with h = normalize(l + v).
//!   path tracing (Lambert): next dir ~ cos(theta)/pi so pdf cancels the cosine and throughput *= albedo; after 3 bounces
//!     Russian roulette keeps the path with probability p = clamp(max(throughput), .05, .95) and divides by p (this step
//!     adds no bias; the hard limit `max_depth` does cut off a little light).
//!     Glass picks reflection with probability R (Fresnel), refraction otherwise, so throughput needs no extra weight.
//!   white furnace: any closed lossless material inside uniform radiance 1 must look exactly 1; a deviation means the
//!     BRDF/refraction code creates or destroys energy. Convergence: RMSE of an N-sample image against a very
//!     high-sample reference falls like 1/sqrt(N).
//! LIMITS: the path tracer sees only the sky and emissive surfaces (it ignores point and area lights) and treats Blinn as
//!   matte. Whitted shadow rays treat glass as opaque. Area lights have no emitter-cosine term. Glass is always assumed to
//!   be surrounded by air. The convergence check tests that the error falls and drops by 2.5x from 1 to 64 samples, not
//!   the exact 1/sqrt(N) rate.
//! COMMON USES: film and product-visualisation offline renderers (the same ideas as PBRT/Cycles), game-engine lightmap
//!   baking and reference images, synthetic data generation for computer vision, teaching sampling/variance.
//! Read: https://raytracing.github.io/ and https://www.pbr-book.org/
use std::collections::HashMap;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

// =====================================================================================
// math
// =====================================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
struct Vec3 {
    x: f64,
    y: f64,
    z: f64,
}
const fn v3(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3 { x, y, z }
}
impl Vec3 {
    const ZERO: Vec3 = v3(0.0, 0.0, 0.0);
    const ONE: Vec3 = v3(1.0, 1.0, 1.0);
    fn dot(self, o: Vec3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    fn cross(self, o: Vec3) -> Vec3 {
        v3(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    fn length(self) -> f64 {
        self.dot(self).sqrt()
    }
    fn norm(self) -> Vec3 {
        self / self.length()
    }
    fn get(self, axis: usize) -> f64 {
        [self.x, self.y, self.z][axis]
    }
    fn mulv(self, o: Vec3) -> Vec3 {
        v3(self.x * o.x, self.y * o.y, self.z * o.z)
    }
    fn min(self, o: Vec3) -> Vec3 {
        v3(self.x.min(o.x), self.y.min(o.y), self.z.min(o.z))
    }
    fn max(self, o: Vec3) -> Vec3 {
        v3(self.x.max(o.x), self.y.max(o.y), self.z.max(o.z))
    }
    fn max_comp(self) -> f64 {
        self.x.max(self.y).max(self.z)
    }
    fn exp(self) -> Vec3 {
        v3(self.x.exp(), self.y.exp(), self.z.exp())
    }
    fn lum(self) -> f64 {
        0.2126 * self.x + 0.7152 * self.y + 0.0722 * self.z
    }
    fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
    fn reflect(self, n: Vec3) -> Vec3 {
        self - n * (2.0 * self.dot(n))
    }
    fn near(self, o: Vec3, tol: f64) -> bool {
        (self - o).length() <= tol
    }
}
impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl AddAssign for Vec3 {
    fn add_assign(&mut self, o: Vec3) {
        *self = *self + o;
    }
}
impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f64> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f64) -> Vec3 {
        v3(self.x * s, self.y * s, self.z * s)
    }
}
impl Div<f64> for Vec3 {
    type Output = Vec3;
    fn div(self, s: f64) -> Vec3 {
        v3(self.x / s, self.y / s, self.z / s)
    }
}
impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        v3(-self.x, -self.y, -self.z)
    }
}

/// splitmix64: tiny, fast, and any seed (even sequential ones) gives a good stream.
#[derive(Clone)]
struct Rng(u64);
impl Rng {
    /// Independent stream for pixel (x, y): depends on nothing but (seed, x, y).
    fn for_pixel(seed: u64, x: usize, y: usize) -> Rng {
        let mut r = Rng(seed
            ^ (x as u64).wrapping_mul(0xD6E8FEB86659FD93)
            ^ (y as u64).wrapping_mul(0xA5A5_5A5A_1234_5679));
        r.next();
        r
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
    fn f(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Orthonormal basis (t, b) around unit n.
fn basis(n: Vec3) -> (Vec3, Vec3) {
    let a = if n.x.abs() > 0.9 {
        v3(0.0, 1.0, 0.0)
    } else {
        v3(1.0, 0.0, 0.0)
    };
    let t = n.cross(a).norm();
    (t, n.cross(t))
}
/// Cosine-weighted direction in the hemisphere around n (pdf = cos(theta) / pi).
fn cosine_dir(n: Vec3, rng: &mut Rng) -> Vec3 {
    let (r1, r2) = (rng.f(), rng.f());
    let (phi, r) = (2.0 * std::f64::consts::PI * r1, r2.sqrt());
    let (t, b) = basis(n);
    (t * (r * phi.cos()) + b * (r * phi.sin()) + n * (1.0 - r2).sqrt()).norm()
}
fn random_in_disk(rng: &mut Rng) -> (f64, f64) {
    loop {
        let (x, y) = (2.0 * rng.f() - 1.0, 2.0 * rng.f() - 1.0);
        if x * x + y * y <= 1.0 {
            return (x, y);
        }
    }
}
/// Schlick's estimate of the reflected share. `cos_i` is measured on the incoming side.
fn schlick(cos_i: f64, n1: f64, n2: f64) -> f64 {
    let mut cos = cos_i;
    if n1 > n2 {
        // The estimate only holds with the angle on the side with the smaller index: here the outgoing one.
        let sin2_t = (n1 / n2).powi(2) * (1.0 - cos_i * cos_i);
        if sin2_t > 1.0 {
            return 1.0;
        }
        cos = (1.0 - sin2_t).sqrt();
    }
    let r0 = ((n1 - n2) / (n1 + n2)).powi(2);
    r0 + (1.0 - r0) * (1.0 - cos).powi(5)
}
/// None = total internal reflection. `d` unit, `n` unit and facing against `d`, eta = n1/n2.
fn refract(d: Vec3, n: Vec3, eta: f64) -> Option<Vec3> {
    let c = -d.dot(n);
    let k = 1.0 - eta * eta * (1.0 - c * c);
    if k < 0.0 {
        None
    } else {
        Some((d * eta + n * (eta * c - k.sqrt())).norm())
    }
}

// =====================================================================================
// scene description
// =====================================================================================
#[derive(Clone, Copy)]
struct Ray {
    o: Vec3,
    d: Vec3,
}
impl Ray {
    fn at(&self, t: f64) -> Vec3 {
        self.o + self.d * t
    }
}

#[derive(Clone, Copy, Debug)]
enum Material {
    Lambert(Vec3),
    /// two albedos alternating on a grid; the number is cells per unit length (only meaningful on planes)
    Checker(Vec3, Vec3, f64),
    /// diffuse albedo + Blinn-Phong specular (ks, shininess)
    Blinn(Vec3, f64, f64),
    Mirror(Vec3),
    /// index of refraction and absorption coefficient per unit length (Beer-Lambert), zero = clear
    Glass(f64, Vec3),
    Emissive(Vec3),
}

#[derive(Clone, Copy)]
enum Shape {
    Sphere { c: Vec3, r: f64 },
    Tri { a: Vec3, b: Vec3, c: Vec3 },
}
#[derive(Clone, Copy)]
struct Prim {
    shape: Shape,
    mat: usize,
}
struct Plane {
    n: Vec3,
    k: f64, // n . p = k
    mat: usize,
}
enum Light {
    Point {
        pos: Vec3,
        color: Vec3,
    },
    /// parallelogram corner + u + v, emitting `color` (radiant intensity folded in), sampled uniformly
    Area {
        corner: Vec3,
        u: Vec3,
        v: Vec3,
        color: Vec3,
    },
}
#[derive(Clone, Copy)]
enum Env {
    Sky,
    Uniform(Vec3),
    Black,
}
impl Env {
    fn radiance(&self, d: Vec3) -> Vec3 {
        match self {
            Env::Sky => {
                let t = 0.5 * (d.y + 1.0);
                v3(1.0, 1.0, 1.0) * (1.0 - t) + v3(0.45, 0.65, 1.0) * t
            }
            Env::Uniform(c) => *c,
            Env::Black => Vec3::ZERO,
        }
    }
}

fn hit_sphere(r: &Ray, c: Vec3, rad: f64, tmin: f64, tmax: f64) -> Option<f64> {
    let oc = r.o - c;
    let hb = oc.dot(r.d);
    // `hb * hb - (oc.oc - rad^2)` subtracts two huge numbers when the origin is far away, and keeps only noise.
    // The part of `oc` at right angles to the ray stays small.
    let side = oc - r.d * hb;
    let disc = rad * rad - side.dot(side);
    if disc < 0.0 {
        return None;
    }
    let sq = disc.sqrt();
    [-hb - sq, -hb + sq]
        .into_iter()
        .find(|&t| t > tmin && t < tmax)
}
fn hit_tri(r: &Ray, a: Vec3, b: Vec3, c: Vec3, tmin: f64, tmax: f64) -> Option<f64> {
    let (e1, e2) = (b - a, c - a);
    let p = r.d.cross(e2);
    let det = e1.dot(p);
    // Compared with the sizes that went in: an absolute limit would make every tiny triangle invisible.
    if det * det <= 1e-24 * e1.dot(e1) * e2.dot(e2) * r.d.dot(r.d) {
        return None;
    }
    let inv = 1.0 / det;
    let s = r.o - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = r.d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    if t > tmin && t < tmax { Some(t) } else { None }
}
impl Prim {
    fn intersect(&self, r: &Ray, tmin: f64, tmax: f64) -> Option<f64> {
        if COUNTING.load(Ordering::Relaxed) {
            PRIM_TESTS.fetch_add(1, Ordering::Relaxed);
        }
        match self.shape {
            Shape::Sphere { c, r: rad } => hit_sphere(r, c, rad, tmin, tmax),
            Shape::Tri { a, b, c } => hit_tri(r, a, b, c, tmin, tmax),
        }
    }
    fn bounds(&self) -> (Vec3, Vec3) {
        let pad = v3(1e-6, 1e-6, 1e-6);
        match self.shape {
            Shape::Sphere { c, r } => {
                let r = r.abs(); // a negative radius is the same surface with the normal pointing inward
                (c - v3(r, r, r) - pad, c + v3(r, r, r) + pad)
            }
            Shape::Tri { a, b, c } => (a.min(b).min(c) - pad, a.max(b).max(c) + pad),
        }
    }
    fn normal_at(&self, p: Vec3) -> Vec3 {
        match self.shape {
            Shape::Sphere { c, r } => (p - c) / r,
            Shape::Tri { a, b, c } => (b - a).cross(c - a).norm(),
        }
    }
}

// =====================================================================================
// BVH (flattened)
// =====================================================================================
#[derive(Clone, Copy)]
struct Node {
    lo: Vec3,
    hi: Vec3,
    /// leaf: first index into `order`; inner: index of the right child (left child = this index + 1)
    a: u32,
    /// leaf: number of prims (> 0); inner: 0
    n: u32,
    axis: u8,
}
struct Bvh {
    nodes: Vec<Node>,
    order: Vec<u32>, // prim indices in leaf order
}
impl Bvh {
    const LEAF: usize = 4;
    fn build(prims: &[Prim]) -> Bvh {
        let bounds: Vec<(Vec3, Vec3)> = prims.iter().map(|p| p.bounds()).collect();
        let cent: Vec<Vec3> = bounds.iter().map(|(l, h)| (*l + *h) * 0.5).collect();
        let mut ids: Vec<u32> = (0..prims.len() as u32).collect();
        let mut bvh = Bvh {
            nodes: vec![],
            order: vec![],
        };
        if !prims.is_empty() {
            bvh.rec(&mut ids, &bounds, &cent);
        }
        bvh
    }
    fn rec(&mut self, ids: &mut [u32], bounds: &[(Vec3, Vec3)], cent: &[Vec3]) -> usize {
        let (mut lo, mut hi) = (
            v3(f64::MAX, f64::MAX, f64::MAX),
            v3(f64::MIN, f64::MIN, f64::MIN),
        );
        let (mut clo, mut chi) = (lo, hi);
        for &i in ids.iter() {
            lo = lo.min(bounds[i as usize].0);
            hi = hi.max(bounds[i as usize].1);
            clo = clo.min(cent[i as usize]);
            chi = chi.max(cent[i as usize]);
        }
        let me = self.nodes.len();
        if ids.len() <= Self::LEAF {
            self.nodes.push(Node {
                lo,
                hi,
                a: self.order.len() as u32,
                n: ids.len() as u32,
                axis: 0,
            });
            self.order.extend_from_slice(ids);
            return me;
        }
        let ext = chi - clo;
        let axis = if ext.x >= ext.y && ext.x >= ext.z {
            0
        } else if ext.y >= ext.z {
            1
        } else {
            2
        };
        // median split: sort by centroid along the axis (index breaks ties so the build is deterministic)
        ids.sort_by(|&p, &q| {
            cent[p as usize]
                .get(axis)
                .total_cmp(&cent[q as usize].get(axis))
                .then(p.cmp(&q))
        });
        self.nodes.push(Node {
            lo,
            hi,
            a: 0,
            n: 0,
            axis: axis as u8,
        });
        let mid = ids.len() / 2;
        let (l, r) = ids.split_at_mut(mid);
        self.rec(l, bounds, cent);
        let right = self.rec(r, bounds, cent);
        self.nodes[me].a = right as u32;
        me
    }
    fn max_depth(&self) -> usize {
        fn go(b: &Bvh, i: usize) -> usize {
            let n = &b.nodes[i];
            if n.n > 0 {
                1
            } else {
                1 + go(b, i + 1).max(go(b, n.a as usize))
            }
        }
        if self.nodes.is_empty() {
            0
        } else {
            go(self, 0)
        }
    }
}
/// Off except while section 6 counts, so that rendering threads do not fight over one counter.
static COUNTING: AtomicBool = AtomicBool::new(false);
/// How many ray-primitive tests have run. A count is the same on every machine; a stopwatch is not.
static PRIM_TESTS: AtomicU64 = AtomicU64::new(0);

/// Slab test against a node box limited to [0, tmax].
fn box_hit(lo: Vec3, hi: Vec3, o: Vec3, inv: Vec3, tmax: f64) -> bool {
    let (mut t0, mut t1) = (0.0f64, tmax);
    for ax in 0..3 {
        let (mut a, mut b) = (
            (lo.get(ax) - o.get(ax)) * inv.get(ax),
            (hi.get(ax) - o.get(ax)) * inv.get(ax),
        );
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        t0 = t0.max(a);
        t1 = t1.min(b);
        if t0 > t1 {
            return false;
        }
    }
    true
}

struct Scene {
    prims: Vec<Prim>,
    planes: Vec<Plane>,
    mats: Vec<Material>,
    lights: Vec<Light>,
    env: Env,
    bvh: Bvh,
}
#[derive(Clone, Copy)]
struct Hit {
    t: f64,
    p: Vec3,
    n: Vec3, // outward geometric normal
    mat: usize,
}
impl Scene {
    fn new(mats: Vec<Material>, env: Env) -> Scene {
        Scene {
            prims: vec![],
            planes: vec![],
            mats,
            lights: vec![],
            env,
            bvh: Bvh {
                nodes: vec![],
                order: vec![],
            },
        }
    }
    fn finish(&mut self) {
        self.bvh = Bvh::build(&self.prims);
    }
    /// Closest primitive: smaller t wins, equal t -> smaller index (same answer with or without the BVH).
    fn closest(
        &self,
        r: &Ray,
        tmin: f64,
        tmax: f64,
        use_bvh: bool,
        any: bool,
    ) -> Option<(f64, usize, bool)> {
        let mut best: Option<(f64, usize)> = None;
        let mut limit = tmax;
        let consider = |t: f64, i: usize, best: &mut Option<(f64, usize)>, limit: &mut f64| {
            if best.is_none_or(|(bt, bi)| t < bt || (t == bt && i < bi)) {
                *best = Some((t, i));
                *limit = t.next_up(); // keep equal-t candidates alive for the tie rule
            }
        };
        if use_bvh {
            if !self.bvh.nodes.is_empty() {
                let inv = v3(1.0 / r.d.x, 1.0 / r.d.y, 1.0 / r.d.z);
                let mut stack = [0u32; 64];
                let mut sp = 1;
                while sp > 0 {
                    sp -= 1;
                    let node = &self.bvh.nodes[stack[sp] as usize];
                    if !box_hit(node.lo, node.hi, r.o, inv, limit) {
                        continue;
                    }
                    if node.n > 0 {
                        for k in node.a..node.a + node.n {
                            let pi = self.bvh.order[k as usize] as usize;
                            if let Some(t) = self.prims[pi].intersect(r, tmin, limit) {
                                consider(t, pi, &mut best, &mut limit);
                                if any {
                                    return Some((t, pi, false));
                                }
                            }
                        }
                    } else {
                        // visit the child on the ray's side first so `limit` shrinks early
                        let me = stack[sp] as usize;
                        let (l, rr) = (me as u32 + 1, node.a);
                        let (first, second) = if r.d.get(node.axis as usize) >= 0.0 {
                            (l, rr)
                        } else {
                            (rr, l)
                        };
                        stack[sp] = second;
                        stack[sp + 1] = first;
                        sp += 2;
                    }
                }
            }
        } else {
            for (i, p) in self.prims.iter().enumerate() {
                if let Some(t) = p.intersect(r, tmin, limit) {
                    consider(t, i, &mut best, &mut limit);
                    if any {
                        return Some((t, i, false));
                    }
                }
            }
        }
        let mut out = best.map(|(t, i)| (t, i, false));
        for (i, pl) in self.planes.iter().enumerate() {
            let den = pl.n.dot(r.d);
            if den.abs() > 1e-12 {
                let t = (pl.k - pl.n.dot(r.o)) / den;
                if t > tmin && t < out.map_or(tmax, |o| o.0) {
                    out = Some((t, i, true));
                }
            }
        }
        out
    }
    fn intersect(&self, r: &Ray, tmin: f64, tmax: f64, use_bvh: bool) -> Option<Hit> {
        let (t, i, is_plane) = self.closest(r, tmin, tmax, use_bvh, false)?;
        let p = r.at(t);
        Some(if is_plane {
            Hit {
                t,
                p,
                n: self.planes[i].n,
                mat: self.planes[i].mat,
            }
        } else {
            Hit {
                t,
                p,
                n: self.prims[i].normal_at(p),
                mat: self.prims[i].mat,
            }
        })
    }
    fn occluded(&self, r: &Ray, tmax: f64, use_bvh: bool) -> bool {
        self.closest(r, 1e-4, tmax, use_bvh, true).is_some()
    }
}
// ---------- mesh helpers ----------
fn add_quad(s: &mut Scene, a: Vec3, b: Vec3, c: Vec3, d: Vec3, mat: usize) {
    s.prims.push(Prim {
        shape: Shape::Tri { a, b, c },
        mat,
    });
    s.prims.push(Prim {
        shape: Shape::Tri { a, b: c, c: d },
        mat,
    });
}
/// Axis-aligned box rotated about the y axis by `yaw` around its base centre.
fn add_box(s: &mut Scene, base: Vec3, size: Vec3, yaw: f64, mat: usize) {
    let (sn, cs) = yaw.sin_cos();
    let p = |x: f64, y: f64, z: f64| {
        let (lx, lz) = ((x - 0.5) * size.x, (z - 0.5) * size.z);
        base + v3(lx * cs + lz * sn, y * size.y, -lx * sn + lz * cs)
    };
    let c = |i: usize| p((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64);
    for q in [
        [0, 1, 3, 2],
        [4, 6, 7, 5],
        [0, 4, 5, 1],
        [2, 3, 7, 6],
        [0, 2, 6, 4],
        [1, 5, 7, 3],
    ] {
        add_quad(s, c(q[0]), c(q[1]), c(q[2]), c(q[3]), mat);
    }
}
/// Icosphere: icosahedron subdivided `levels` times (20 * 4^levels triangles), projected onto the sphere.
fn add_icosphere(s: &mut Scene, centre: Vec3, radius: f64, levels: u32, mat: usize) {
    let t = (1.0 + 5f64.sqrt()) / 2.0;
    let mut verts: Vec<Vec3> = [
        (-1.0, t, 0.0),
        (1.0, t, 0.0),
        (-1.0, -t, 0.0),
        (1.0, -t, 0.0),
        (0.0, -1.0, t),
        (0.0, 1.0, t),
        (0.0, -1.0, -t),
        (0.0, 1.0, -t),
        (t, 0.0, -1.0),
        (t, 0.0, 1.0),
        (-t, 0.0, -1.0),
        (-t, 0.0, 1.0),
    ]
    .iter()
    .map(|&(x, y, z)| v3(x, y, z).norm())
    .collect();
    let mut faces: Vec<[usize; 3]> = vec![
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    for _ in 0..levels {
        let mut cache: HashMap<(usize, usize), usize> = HashMap::new();
        let mut mid = |verts: &mut Vec<Vec3>, a: usize, b: usize| -> usize {
            *cache.entry((a.min(b), a.max(b))).or_insert_with(|| {
                verts.push(((verts[a] + verts[b]) * 0.5).norm());
                verts.len() - 1
            })
        };
        let mut next = Vec::with_capacity(faces.len() * 4);
        for [a, b, c] in faces {
            let (ab, bc, ca) = (
                mid(&mut verts, a, b),
                mid(&mut verts, b, c),
                mid(&mut verts, c, a),
            );
            next.extend([[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]);
        }
        faces = next;
    }
    for [a, b, c] in faces {
        let (pa, mut pb, mut pc) = (verts[a], verts[b], verts[c]);
        if (pb - pa).cross(pc - pa).dot(pa + pb + pc) < 0.0 {
            std::mem::swap(&mut pb, &mut pc); // make the winding outward-facing
        }
        let f = |p: Vec3| centre + p * radius;
        s.prims.push(Prim {
            shape: Shape::Tri {
                a: f(pa),
                b: f(pb),
                c: f(pc),
            },
            mat,
        });
    }
}

// =====================================================================================
// camera
// =====================================================================================
struct Camera {
    pos: Vec3,
    fwd: Vec3,
    right: Vec3,
    up: Vec3,
    half_h: f64,
    aspect: f64,
    aperture: f64,
    focus: f64,
}
impl Camera {
    fn new(
        pos: Vec3,
        target: Vec3,
        fov_deg: f64,
        aspect: f64,
        aperture: f64,
        focus: f64,
    ) -> Camera {
        assert!(
            (target - pos).length() > 0.0,
            "a camera cannot look at the point where it stands"
        );
        let fwd = (target - pos).norm();
        // Looking straight up or down, "up is +y" says nothing about which way the picture is turned: use -z.
        let up = if fwd.cross(v3(0.0, 1.0, 0.0)).length() > 1e-9 {
            v3(0.0, 1.0, 0.0)
        } else {
            v3(0.0, 0.0, -1.0)
        };
        let right = fwd.cross(up).norm();
        Camera {
            pos,
            fwd,
            right,
            up: right.cross(fwd),
            half_h: (fov_deg.to_radians() / 2.0).tan(),
            aspect,
            aperture,
            focus,
        }
    }
    /// (s, t) in [0,1]^2, t = 0 at the top.
    fn ray(&self, s: f64, t: f64, rng: &mut Rng) -> Ray {
        let dir = self.fwd
            + self.right * ((2.0 * s - 1.0) * self.half_h * self.aspect)
            + self.up * ((1.0 - 2.0 * t) * self.half_h);
        if self.aperture <= 0.0 {
            return Ray {
                o: self.pos,
                d: dir.norm(),
            };
        }
        let target = self.pos + dir * self.focus; // where the pinhole ray crosses the focus plane
        let (lx, ly) = random_in_disk(rng);
        let o = self.pos
            + self.right * (lx * self.aperture / 2.0)
            + self.up * (ly * self.aperture / 2.0);
        Ray {
            o,
            d: (target - o).norm(),
        }
    }
}

// =====================================================================================
// integrators
// =====================================================================================
#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    Whitted,
    Path,
}
#[derive(Clone, Copy)]
struct Cfg {
    w: usize,
    h: usize,
    spp: usize,
    mode: Mode,
    use_bvh: bool,
    threads: usize,
    seed: u64,
    jitter: bool,
    max_depth: u32,
    area_samples: usize,
}

fn checker_albedo(a: Vec3, b: Vec3, scale: f64, p: Vec3) -> Vec3 {
    // `as` stops at the ends of i64, so adding the two cells first can overflow. Take each parity on its own.
    let parity = |v: f64| ((v * scale).floor() as i64).rem_euclid(2);
    if parity(p.x) == parity(p.z) { a } else { b }
}
fn diffuse_albedo(m: &Material, p: Vec3) -> Option<Vec3> {
    match *m {
        Material::Lambert(a) | Material::Blinn(a, _, _) => Some(a),
        Material::Checker(a, b, s) => Some(checker_albedo(a, b, s, p)),
        _ => None,
    }
}

/// Whitted-style recursive shading.
fn whitted(sc: &Scene, r: &Ray, depth: u32, rng: &mut Rng, cfg: &Cfg) -> Vec3 {
    let Some(h) = sc.intersect(r, 1e-4, f64::INFINITY, cfg.use_bvh) else {
        return sc.env.radiance(r.d);
    };
    let front = r.d.dot(h.n) < 0.0;
    let n = if front { h.n } else { -h.n };
    let mat = sc.mats[h.mat];
    match mat {
        Material::Emissive(e) => e,
        Material::Mirror(tint) => {
            if depth == 0 {
                return Vec3::ZERO;
            }
            tint.mulv(whitted(
                sc,
                &Ray {
                    o: h.p + n * 1e-6,
                    d: r.d.reflect(n),
                },
                depth - 1,
                rng,
                cfg,
            ))
        }
        Material::Glass(ior, absorb) => {
            if depth == 0 {
                return Vec3::ZERO;
            }
            let (n1, n2) = if front { (1.0, ior) } else { (ior, 1.0) };
            let cos_i = -r.d.dot(n);
            let refl = whitted(
                sc,
                &Ray {
                    o: h.p + n * 1e-6,
                    d: r.d.reflect(n),
                },
                depth - 1,
                rng,
                cfg,
            );
            let col = match refract(r.d, n, n1 / n2) {
                None => refl, // total internal reflection
                Some(td) => {
                    let f = schlick(cos_i, n1, n2);
                    let tr = whitted(
                        sc,
                        &Ray {
                            o: h.p - n * 1e-6,
                            d: td,
                        },
                        depth - 1,
                        rng,
                        cfg,
                    );
                    refl * f + tr * (1.0 - f)
                }
            };
            // light that arrived along this segment travelled through glass if we are inside
            if front {
                col
            } else {
                col.mulv((-absorb * h.t).exp())
            }
        }
        _ => {
            let albedo = diffuse_albedo(&mat, h.p).unwrap();
            let (ks, shin) = if let Material::Blinn(_, ks, s) = mat {
                (ks, s)
            } else {
                (0.0, 1.0)
            };
            let view = -r.d;
            let mut col = albedo * 0.04; // tiny ambient so shadows are not pure black
            for light in &sc.lights {
                let samples = match light {
                    Light::Point { .. } => 1,
                    Light::Area { .. } => cfg.area_samples.max(1),
                };
                let mut acc = Vec3::ZERO;
                for _ in 0..samples {
                    let (lp, lc) = match *light {
                        Light::Point { pos, color } => (pos, color),
                        Light::Area {
                            corner,
                            u,
                            v,
                            color,
                        } => (corner + u * rng.f() + v * rng.f(), color),
                    };
                    let to = lp - h.p;
                    let dist = to.length();
                    let l = to / dist;
                    let ndl = n.dot(l);
                    // A lamp exactly on the point gives 0 / 0: NaN fails every comparison, so ask for it by name.
                    if ndl.is_nan() || ndl <= 0.0 {
                        continue;
                    }
                    let origin = h.p + n * 1e-4;
                    if sc.occluded(&Ray { o: origin, d: l }, dist - 1e-3, cfg.use_bvh) {
                        continue; // hard shadow ray (soft overall for area lights because each sample differs)
                    }
                    let atten = 1.0 / (dist * dist);
                    let hv = (l + view).norm();
                    let spec = if ks > 0.0 {
                        ks * n.dot(hv).max(0.0).powf(shin)
                    } else {
                        0.0
                    };
                    acc += lc.mulv(albedo * ndl + Vec3::ONE * spec) * atten;
                }
                col += acc / samples as f64;
            }
            col
        }
    }
}

/// Unidirectional path tracer: returns an unbiased estimate of the radiance along `r`.
fn path(sc: &Scene, mut r: Ray, rng: &mut Rng, cfg: &Cfg) -> Vec3 {
    let (mut l, mut thr) = (Vec3::ZERO, Vec3::ONE);
    for depth in 0..cfg.max_depth {
        let Some(h) = sc.intersect(&r, 1e-4, f64::INFINITY, cfg.use_bvh) else {
            l += thr.mulv(sc.env.radiance(r.d));
            break;
        };
        let front = r.d.dot(h.n) < 0.0;
        let n = if front { h.n } else { -h.n };
        let mat = sc.mats[h.mat];
        match mat {
            Material::Emissive(e) => {
                l += thr.mulv(e);
                break;
            }
            Material::Mirror(tint) => {
                thr = thr.mulv(tint);
                r = Ray {
                    o: h.p + n * 1e-6,
                    d: r.d.reflect(n),
                };
            }
            Material::Glass(ior, absorb) => {
                if !front {
                    thr = thr.mulv((-absorb * h.t).exp());
                }
                let (n1, n2) = if front { (1.0, ior) } else { (ior, 1.0) };
                let cos_i = -r.d.dot(n);
                match refract(r.d, n, n1 / n2) {
                    Some(td) if rng.f() >= schlick(cos_i, n1, n2) => {
                        r = Ray {
                            o: h.p - n * 1e-6,
                            d: td,
                        }
                    }
                    _ => {
                        r = Ray {
                            o: h.p + n * 1e-6,
                            d: r.d.reflect(n),
                        }
                    } // reflect w.p. R (or TIR); the weight is 1
                }
            }
            _ => {
                // Lambert (also used for checker and the diffuse part of Blinn): pdf cancels cos/pi, weight = albedo
                thr = thr.mulv(diffuse_albedo(&mat, h.p).unwrap());
                r = Ray {
                    o: h.p + n * 1e-6,
                    d: cosine_dir(n, rng),
                };
            }
        }
        if depth >= 3 {
            let p = thr.max_comp().clamp(0.05, 0.95);
            if rng.f() > p {
                break;
            }
            thr = thr / p;
        }
    }
    l
}

fn render_pixel(sc: &Scene, cam: &Camera, cfg: &Cfg, x: usize, y: usize) -> Vec3 {
    let mut rng = Rng::for_pixel(cfg.seed, x, y);
    let n = (cfg.spp as f64).sqrt().round() as usize;
    let strat = n * n == cfg.spp;
    let mut sum = Vec3::ZERO;
    for s in 0..cfg.spp {
        // jittered supersampling: one random point inside each cell of an n x n grid (or fully random if spp is no square)
        let (jx, jy) = if !cfg.jitter {
            (0.5, 0.5)
        } else if strat {
            (
                ((s % n) as f64 + rng.f()) / n as f64,
                ((s / n) as f64 + rng.f()) / n as f64,
            )
        } else {
            (rng.f(), rng.f())
        };
        let ray = cam.ray(
            (x as f64 + jx) / cfg.w as f64,
            (y as f64 + jy) / cfg.h as f64,
            &mut rng,
        );
        sum += match cfg.mode {
            Mode::Whitted => whitted(sc, &ray, cfg.max_depth, &mut rng, cfg),
            Mode::Path => path(sc, ray, &mut rng, cfg),
        };
    }
    sum / cfg.spp as f64
}

/// Renders in bands pulled from a shared queue by `cfg.threads` scoped threads. Every pixel has its own RNG stream, so the
/// output is bit-identical for any thread count.
fn render(sc: &Scene, cam: &Camera, cfg: &Cfg) -> Vec<Vec3> {
    let mut fb = vec![Vec3::ZERO; cfg.w * cfg.h];
    if fb.is_empty() {
        return fb; // chunks_mut(0) would panic
    }
    assert!(cfg.spp > 0, "at least one sample per pixel");
    let band = 4;
    let queue = Mutex::new(fb.chunks_mut(cfg.w * band).enumerate());
    std::thread::scope(|s| {
        for _ in 0..cfg.threads.max(1) {
            s.spawn(|| {
                loop {
                    let item = queue.lock().unwrap().next();
                    let Some((bi, chunk)) = item else { break };
                    for (k, px) in chunk.iter_mut().enumerate() {
                        let (x, y) = (k % cfg.w, bi * band + k / cfg.w);
                        *px = render_pixel(sc, cam, cfg, x, y);
                    }
                }
            });
        }
    });
    fb
}

// =====================================================================================
// image output (minimal PNG writer, same layout as recipe png_ppm_writer: stored deflate blocks, filter 0)
// =====================================================================================
fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB88320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}
fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}
fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}
fn encode_png(w: usize, h: usize, rgb: &[u8]) -> Vec<u8> {
    // PNG has no empty images, and the header must agree with the data.
    assert!(w > 0 && h > 0, "a PNG needs at least one pixel");
    assert!(
        w.checked_mul(h).and_then(|n| n.checked_mul(3)) == Some(rgb.len()),
        "rgb must hold width * height * 3 bytes"
    );
    let mut raw = Vec::with_capacity((w * 3 + 1) * h);
    for row in rgb.chunks(w * 3) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    for (i, c) in raw.chunks(65535).enumerate() {
        z.push(if (i + 1) * 65535 >= raw.len() { 1 } else { 0 });
        z.extend_from_slice(&(c.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
        z.extend_from_slice(c);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = (w as u32).to_be_bytes().to_vec();
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    png_chunk(&mut out, b"IHDR", &ihdr);
    png_chunk(&mut out, b"IDAT", &z);
    png_chunk(&mut out, b"IEND", &[]);
    out
}
fn srgb8(c: f64) -> u8 {
    let c = if c.is_nan() { 0.0 } else { c.clamp(0.0, 1.0) };
    let e = if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (e * 255.0 + 0.5) as u8
}
fn to_rgb8(fb: &[Vec3]) -> Vec<u8> {
    fb.iter()
        .flat_map(|c| [srgb8(c.x), srgb8(c.y), srgb8(c.z)])
        .collect()
}
fn gallery_dir() -> PathBuf {
    let d = std::env::var_os("COOKBOOK_GALLERY")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("flowforge_gallery"));
    std::fs::create_dir_all(&d).expect("create gallery dir");
    d
}
fn save_png(name: &str, w: usize, h: usize, fb: &[Vec3]) {
    assert!(w <= 400 && h <= 300);
    let path = gallery_dir().join(name);
    std::fs::write(&path, encode_png(w, h, &to_rgb8(fb))).expect("write png");
    println!("wrote {}", path.display());
}
fn ascii_preview(fb: &[Vec3], w: usize, h: usize, cols: usize, rows: usize) -> String {
    const RAMP: &[u8] = b" .:-=+*#%@";
    let mut s = String::new();
    for ry in 0..rows {
        for rx in 0..cols {
            let (x0, y0) = (rx * w / cols, ry * h / rows);
            let (x1, y1) = (
                ((rx + 1) * w / cols).max(x0 + 1),
                ((ry + 1) * h / rows).max(y0 + 1),
            );
            let (mut sum, mut cnt) = (0.0, 0.0);
            for y in y0..y1 {
                for x in x0..x1 {
                    let c = fb[y * w + x];
                    sum += srgb8(c.lum()) as f64 / 255.0;
                    cnt += 1.0;
                }
            }
            s.push(RAMP[((sum / cnt) * (RAMP.len() - 1) as f64).round() as usize] as char);
        }
        s.push('\n');
    }
    s
}

// =====================================================================================
// scenes
// =====================================================================================
const RED: usize = 0;
const BLUE_BLINN: usize = 1;
const MIRROR: usize = 2;
const GLASS: usize = 3;
const FLOOR: usize = 4;
const COPPER: usize = 5;
const GREEN: usize = 6;

/// Whitted showcase: checker floor, red diffuse sphere at the look-at point, blue Blinn sphere, mirror sphere, copper icosphere.
fn whitted_scene(ico_levels: u32) -> (Scene, Camera) {
    let mats = vec![
        Material::Lambert(v3(0.8, 0.1, 0.1)),
        Material::Blinn(v3(0.1, 0.2, 0.7), 0.8, 60.0),
        Material::Mirror(v3(0.95, 0.95, 0.95)),
        Material::Glass(1.5, Vec3::ZERO),
        Material::Checker(v3(0.85, 0.85, 0.85), v3(0.2, 0.25, 0.2), 1.0),
        Material::Blinn(v3(0.75, 0.4, 0.2), 0.9, 40.0),
        Material::Lambert(v3(0.1, 0.7, 0.2)),
    ];
    let mut s = Scene::new(mats, Env::Sky);
    s.planes.push(Plane {
        n: v3(0.0, 1.0, 0.0),
        k: 0.0,
        mat: FLOOR,
    });
    s.prims.push(Prim {
        shape: Shape::Sphere {
            c: v3(0.0, 1.0, 0.0),
            r: 1.0,
        },
        mat: RED,
    });
    s.prims.push(Prim {
        shape: Shape::Sphere {
            c: v3(-2.3, 0.7, -0.8),
            r: 0.7,
        },
        mat: BLUE_BLINN,
    });
    s.prims.push(Prim {
        shape: Shape::Sphere {
            c: v3(1.0, 0.6, -2.4),
            r: 0.9,
        },
        mat: MIRROR,
    });
    add_icosphere(&mut s, v3(2.1, 0.8, 0.4), 0.8, ico_levels, COPPER);
    s.lights.push(Light::Point {
        pos: v3(-4.0, 6.0, 4.0),
        color: v3(40.0, 40.0, 36.0),
    });
    s.lights.push(Light::Area {
        corner: v3(2.0, 5.0, 2.0),
        u: v3(1.5, 0.0, 0.0),
        v: v3(0.0, 0.0, 1.5),
        color: v3(35.0, 35.0, 40.0),
    });
    s.finish();
    let cam = Camera::new(
        v3(0.0, 1.8, 6.5),
        v3(0.0, 1.0, 0.0),
        40.0,
        4.0 / 3.0,
        0.0,
        6.0,
    );
    (s, cam)
}

/// Glass + mirror scene with depth of field focused on the glass sphere.
fn glass_scene() -> (Scene, Camera) {
    let mats = vec![
        Material::Lambert(v3(0.8, 0.1, 0.1)),
        Material::Blinn(v3(0.1, 0.2, 0.7), 0.8, 60.0),
        Material::Mirror(v3(0.9, 0.9, 0.9)),
        Material::Glass(1.5, v3(0.02, 0.1, 0.1)), // slightly red-tinted via Beer-Lambert (cyan is absorbed)
        Material::Checker(v3(0.9, 0.9, 0.9), v3(0.15, 0.15, 0.2), 1.0),
        Material::Lambert(v3(0.9, 0.7, 0.1)),
        Material::Lambert(v3(0.1, 0.7, 0.2)),
    ];
    let mut s = Scene::new(mats, Env::Sky);
    s.planes.push(Plane {
        n: v3(0.0, 1.0, 0.0),
        k: 0.0,
        mat: FLOOR,
    });
    s.prims.push(Prim {
        shape: Shape::Sphere {
            c: v3(0.0, 1.0, 0.0),
            r: 1.0,
        },
        mat: GLASS,
    });
    s.prims.push(Prim {
        shape: Shape::Sphere {
            c: v3(2.4, 1.0, -1.5),
            r: 1.0,
        },
        mat: MIRROR,
    });
    s.prims.push(Prim {
        shape: Shape::Sphere {
            c: v3(-2.4, 0.8, -1.0),
            r: 0.8,
        },
        mat: RED,
    });
    s.prims.push(Prim {
        shape: Shape::Sphere {
            c: v3(-1.0, 0.45, 2.2),
            r: 0.45,
        },
        mat: 5,
    });
    s.prims.push(Prim {
        shape: Shape::Sphere {
            c: v3(1.3, 0.45, 2.0),
            r: 0.45,
        },
        mat: 6,
    });
    add_box(
        &mut s,
        v3(0.3, 0.0, -3.5),
        v3(1.2, 1.6, 1.2),
        0.6,
        BLUE_BLINN,
    );
    s.lights.push(Light::Area {
        corner: v3(-2.5, 6.0, 3.0),
        u: v3(2.5, 0.0, 0.0),
        v: v3(0.0, 0.0, 2.5),
        color: v3(110.0, 110.0, 100.0),
    });
    s.finish();
    let cam = Camera::new(
        v3(0.0, 2.0, 7.0),
        v3(0.0, 1.0, 0.0),
        38.0,
        4.0 / 3.0,
        0.12,
        7.0,
    );
    (s, cam)
}

/// Cornell box: x in [-1,1], y in [0,2], z in [-1,1], open towards +z where the camera stands.
fn cornell_scene(light: Vec3) -> (Scene, Camera) {
    let mats = vec![
        Material::Lambert(v3(0.75, 0.75, 0.75)), // 0 white
        Material::Lambert(v3(0.75, 0.15, 0.15)), // 1 red
        Material::Lambert(v3(0.15, 0.6, 0.15)),  // 2 green
        Material::Emissive(light),               // 3 light
        Material::Glass(1.5, Vec3::ZERO),        // 4 glass
        Material::Mirror(v3(0.9, 0.9, 0.9)),     // 5 mirror
    ];
    let mut s = Scene::new(mats, Env::Black);
    let (x0, x1, y0, y1, z0, z1) = (-1.0, 1.0, 0.0, 2.0, -1.0, 1.0);
    add_quad(
        &mut s,
        v3(x0, y0, z1),
        v3(x1, y0, z1),
        v3(x1, y0, z0),
        v3(x0, y0, z0),
        0,
    ); // floor
    add_quad(
        &mut s,
        v3(x0, y1, z0),
        v3(x1, y1, z0),
        v3(x1, y1, z1),
        v3(x0, y1, z1),
        0,
    ); // ceiling
    add_quad(
        &mut s,
        v3(x0, y0, z0),
        v3(x1, y0, z0),
        v3(x1, y1, z0),
        v3(x0, y1, z0),
        0,
    ); // back
    add_quad(
        &mut s,
        v3(x0, y0, z1),
        v3(x0, y0, z0),
        v3(x0, y1, z0),
        v3(x0, y1, z1),
        1,
    ); // left red
    add_quad(
        &mut s,
        v3(x1, y0, z0),
        v3(x1, y0, z1),
        v3(x1, y1, z1),
        v3(x1, y1, z0),
        2,
    ); // right green
    let (l, yl) = (0.35, y1 - 0.002);
    add_quad(
        &mut s,
        v3(-l, yl, -l),
        v3(l, yl, -l),
        v3(l, yl, l),
        v3(-l, yl, l),
        3,
    ); // light panel
    add_box(&mut s, v3(-0.35, 0.0, -0.35), v3(0.6, 1.2, 0.6), 0.35, 0);
    s.prims.push(Prim {
        shape: Shape::Sphere {
            c: v3(0.4, 0.38, 0.3),
            r: 0.38,
        },
        mat: 4,
    });
    s.prims.push(Prim {
        shape: Shape::Sphere {
            c: v3(0.55, 0.2, -0.45),
            r: 0.2,
        },
        mat: 5,
    });
    s.finish();
    let cam = Camera::new(v3(0.0, 1.0, 3.5), v3(0.0, 1.0, 0.0), 44.0, 1.0, 0.0, 3.5);
    (s, cam)
}

fn base_cfg(w: usize, h: usize, mode: Mode, spp: usize) -> Cfg {
    Cfg {
        w,
        h,
        spp,
        mode,
        use_bvh: true,
        threads: std::thread::available_parallelism()
            .map_or(4, |n| n.get())
            .min(16),
        seed: 12345,
        jitter: true,
        max_depth: if mode == Mode::Path { 16 } else { 5 },
        area_samples: 8,
    }
}
fn all_finite(fb: &[Vec3]) -> bool {
    fb.iter()
        .all(|c| c.is_finite() && c.x >= 0.0 && c.y >= 0.0 && c.z >= 0.0)
}
fn rmse(a: &[Vec3], b: &[Vec3]) -> f64 {
    // plain linear radiance: clamping first would bias the estimate and hide the 1/sqrt(N) trend
    let s: f64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| {
            let d = *x - *y;
            d.dot(d)
        })
        .sum();
    (s / (3.0 * a.len() as f64)).sqrt()
}

/// One deterministic check for each bug this file once had.
fn regressions() {
    // Just before total internal reflection almost everything must be reflected, not 4 percent.
    let critical = (1.0f64 - (1.0f64 / 1.5).powi(2)).sqrt();
    assert!(
        schlick(critical + 1e-9, 1.5, 1.0) > 0.99,
        "Schlick from the dense side"
    );
    // ...and the same face gives the same share from both sides.
    let (cos_out, sin_in) = (0.6f64, 0.8 / 1.5);
    let cos_in = (1.0f64 - sin_in * sin_in).sqrt();
    assert!((schlick(cos_out, 1.0, 1.5) - schlick(cos_in, 1.5, 1.0)).abs() < 1e-12);

    // A ray passing 1.5 from the centre of a unit sphere, from 1e8 away, misses it.
    let far = |x: f64| Ray {
        o: v3(x, 0.0, 1e8),
        d: v3(0.0, 0.0, -1.0),
    };
    assert!(
        hit_sphere(&far(1.5), Vec3::ZERO, 1.0, 1e-4, 1e30).is_none(),
        "far sphere, miss"
    );
    let t = hit_sphere(&far(0.6), Vec3::ZERO, 1.0, 1e-4, 1e30).expect("far sphere, hit");
    assert!((t - (1e8 - 0.8)).abs() < 1e-6, "far sphere, distance {t}");

    // A triangle with sides of 1e-8 is still a triangle.
    let e = 1e-8;
    let down = Ray {
        o: v3(e * 0.2, e * 0.2, 1.0),
        d: v3(0.0, 0.0, -1.0),
    };
    assert!(
        hit_tri(
            &down,
            Vec3::ZERO,
            v3(e, 0.0, 0.0),
            v3(0.0, e, 0.0),
            1e-4,
            10.0
        )
        .is_some(),
        "tiny triangle"
    );

    // Spheres with a negative radius: the BVH and brute force must agree.
    let mut s = Scene::new(vec![Material::Lambert(Vec3::ONE)], Env::Black);
    let mut g = Rng(5);
    let spot = |g: &mut Rng| {
        v3(
            g.f() * 40.0 - 20.0,
            g.f() * 40.0 - 20.0,
            g.f() * 40.0 - 20.0,
        )
    };
    for _ in 0..200 {
        let c = spot(&mut g);
        s.prims.push(Prim {
            shape: Shape::Sphere { c, r: -1.5 },
            mat: 0,
        });
    }
    s.finish();
    for _ in 0..500 {
        let o = (spot(&mut g)).norm() * 60.0;
        let r = Ray {
            o,
            d: (spot(&mut g) - o).norm(),
        };
        assert!(
            s.closest(&r, 1e-4, 1e30, true, false) == s.closest(&r, 1e-4, 1e30, false, false),
            "negative radius: BVH differs from brute force"
        );
    }

    // A NaN coordinate must not panic the build.
    let odd: Vec<Prim> = (0..8)
        .map(|i| {
            let x = if i == 3 { f64::NAN } else { i as f64 };
            Prim {
                shape: Shape::Sphere {
                    c: v3(x, 0.0, 0.0),
                    r: 0.1,
                },
                mat: 0,
            }
        })
        .collect();
    assert_eq!(Bvh::build(&odd).order.len(), 8);

    // A camera looking straight down works.
    let down = Camera::new(v3(0.0, 5.0, 0.0), Vec3::ZERO, 40.0, 1.0, 0.0, 5.0);
    let d = down.ray(0.5, 0.5, &mut Rng(1)).d;
    assert!(
        d.near(v3(0.0, -1.0, 0.0), 1e-12),
        "straight-down camera, got {d:?}"
    );

    // The checkerboard does not overflow, however far the point is. This needs overflow checks (a debug build, or the
    // second run in `check.sh`): with wrapping arithmetic the old sum happened to keep the right parity.
    for far in [1e19, -1e19, f64::INFINITY] {
        checker_albedo(Vec3::ZERO, Vec3::ONE, 1.0, v3(far, 0.0, far));
    }

    // A lamp exactly on the shaded point adds nothing, and certainly not NaN.
    let mut s = Scene::new(vec![Material::Lambert(Vec3::ONE)], Env::Black);
    s.planes.push(Plane {
        n: v3(0.0, 1.0, 0.0),
        k: 0.0,
        mat: 0,
    });
    s.lights.push(Light::Point {
        pos: Vec3::ZERO,
        color: Vec3::ONE,
    });
    s.finish();
    let down = Ray {
        o: v3(0.0, 1.0, 0.0),
        d: v3(0.0, -1.0, 0.0),
    };
    let c = whitted(&s, &down, 3, &mut Rng(1), &base_cfg(1, 1, Mode::Whitted, 1));
    assert!(c.is_finite(), "lamp on the surface gave {c:?}");

    // Empty pictures render as empty instead of panicking; the PNG writer refuses them.
    let (sc, cam) = whitted_scene(0);
    assert!(render(&sc, &cam, &base_cfg(0, 4, Mode::Whitted, 1)).is_empty());
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let refused = |w: usize, h: usize, rgb: &[u8]| {
        std::panic::catch_unwind(|| encode_png(w, h, rgb)).is_err()
    };
    let results = [refused(4, 0, &[]), refused(2, 2, &[0; 3])];
    std::panic::set_hook(hook);
    assert_eq!(
        results,
        [true, true],
        "encode_png must refuse h = 0 and short data"
    );
}

fn main() {
    let t_all = Instant::now();

    // ---------- 0. regressions: each line fails without its fix ----------
    regressions();

    // ---------- 1. BVH sanity and equality with brute force ----------
    let (sc3, cam3) = whitted_scene(3);
    assert_eq!(sc3.bvh.order.len(), sc3.prims.len());
    let mut seen = vec![false; sc3.prims.len()];
    for &i in &sc3.bvh.order {
        assert!(
            !std::mem::replace(&mut seen[i as usize], true),
            "each primitive in exactly one leaf"
        );
    }
    println!(
        "BVH: {} prims, {} nodes, depth {}",
        sc3.prims.len(),
        sc3.bvh.nodes.len(),
        sc3.bvh.max_depth()
    );
    let mut c96 = base_cfg(96, 64, Mode::Whitted, 2);
    c96.threads = 4;
    let img_bvh = render(&sc3, &cam3, &c96);
    c96.use_bvh = false;
    let img_brute = render(&sc3, &cam3, &c96);
    assert!(
        img_bvh == img_brute,
        "BVH image must be pixel-identical to brute force (Whitted 96x64)"
    );
    assert!(all_finite(&img_bvh));
    let (cb, camb) = cornell_scene(v3(12.0, 11.0, 9.0));
    let mut cpath = base_cfg(48, 48, Mode::Path, 4);
    let a = render(&cb, &camb, &cpath);
    cpath.use_bvh = false;
    let b = render(&cb, &camb, &cpath);
    assert!(a == b, "BVH == brute force for path tracing too");
    println!("BVH image == brute-force image (Whitted 96x64 at 2 spp, path 48x48 at 4 spp)");

    // ---------- 2. thread-count independence ----------
    let mut ct = base_cfg(96, 64, Mode::Whitted, 4);
    ct.threads = 1;
    let one = render(&sc3, &cam3, &ct);
    ct.threads = 4;
    let four = render(&sc3, &cam3, &ct);
    assert!(
        one == four,
        "1 thread and 4 threads must give identical pixels"
    );
    let mut ctp = base_cfg(32, 32, Mode::Path, 8);
    ctp.threads = 1;
    let p1 = render(&cb, &camb, &ctp);
    ctp.threads = 7;
    assert!(p1 == render(&cb, &camb, &ctp));
    println!("1-thread == 4-thread render (Whitted) and 1 == 7 threads (path)");

    // ---------- 3. known-colour pixel checks ----------
    let mut cc = base_cfg(33, 25, Mode::Whitted, 1);
    cc.jitter = false;
    cc.threads = 1;
    let img = render(&sc3, &cam3, &cc);
    let centre = img[12 * 33 + 16];
    println!(
        "centre pixel (red diffuse sphere): rgb = ({:.3}, {:.3}, {:.3})",
        centre.x, centre.y, centre.z
    );
    assert!(
        centre.x > 2.0 * centre.y && centre.x > 2.0 * centre.z && centre.x > 0.1,
        "centre must be predominantly red"
    );

    // mirror: camera at the origin looks down -z at a mirror sphere; the reflection goes straight back to a green emitter
    let mut ms = Scene::new(
        vec![
            Material::Mirror(v3(0.9, 0.9, 0.9)),
            Material::Emissive(v3(0.0, 1.0, 0.0)),
        ],
        Env::Sky,
    );
    ms.prims.push(Prim {
        shape: Shape::Sphere {
            c: v3(0.0, 0.0, -5.0),
            r: 1.0,
        },
        mat: 0,
    });
    ms.prims.push(Prim {
        shape: Shape::Sphere {
            c: v3(0.0, 0.0, 5.0),
            r: 1.0,
        },
        mat: 1,
    });
    ms.finish();
    let mcam = Camera::new(Vec3::ZERO, v3(0.0, 0.0, -1.0), 30.0, 1.0, 0.0, 5.0);
    let mimg = render(&ms, &mcam, &Cfg { w: 21, h: 21, ..cc });
    let mp = mimg[10 * 21 + 10];
    assert!(
        mp.near(v3(0.0, 0.9, 0.0), 1e-9),
        "mirror centre pixel must be 0.9 * emitter green, got {mp:?}"
    );
    let corner = mimg[0];
    assert!(
        corner.z > 0.5 && corner.y > 0.5,
        "off the mirror we see the sky, got {corner:?}"
    );
    println!(
        "mirror pixel shows the reflected emitter: ({:.2}, {:.2}, {:.2})",
        mp.x, mp.y, mp.z
    );

    // hard shadow: a point on the floor behind the sphere (as seen from the light) vs an unshadowed floor point
    let mut ss = Scene::new(vec![Material::Lambert(v3(0.8, 0.8, 0.8))], Env::Sky);
    ss.planes.push(Plane {
        n: v3(0.0, 1.0, 0.0),
        k: 0.0,
        mat: 0,
    });
    ss.prims.push(Prim {
        shape: Shape::Sphere {
            c: v3(0.0, 1.0, 0.0),
            r: 1.0,
        },
        mat: 0,
    });
    ss.lights.push(Light::Point {
        pos: v3(3.0, 4.0, 0.0),
        color: v3(40.0, 40.0, 40.0),
    });
    ss.finish();
    let sh_cfg = Cfg { threads: 1, ..cc };
    let mut rng = Rng(1);
    let look = |from: Vec3, to: Vec3, rng: &mut Rng| {
        whitted(
            &ss,
            &Ray {
                o: from,
                d: (to - from).norm(),
            },
            3,
            rng,
            &sh_cfg,
        )
    };
    let in_shadow = look(v3(-1.0, 6.0, 6.0), v3(-1.0, 0.0, 0.0), &mut rng);
    let lit = look(v3(-1.0, 6.0, 6.0), v3(1.5, 0.0, 0.0), &mut rng);
    println!(
        "floor luminance: shadow {:.4} vs lit {:.4}",
        in_shadow.lum(),
        lit.lum()
    );
    assert!(
        in_shadow.lum() < 0.2 * lit.lum() && in_shadow.lum() > 0.0,
        "ambient only in shadow"
    );

    // thin lens: every lens sample passes through the same point on the focus plane
    let dof = Camera::new(v3(0.0, 1.0, 5.0), Vec3::ZERO, 40.0, 1.5, 0.5, 4.0);
    let pin = Camera {
        aperture: 0.0,
        ..Camera::new(v3(0.0, 1.0, 5.0), Vec3::ZERO, 40.0, 1.5, 0.0, 4.0)
    };
    let mut rr = Rng(9);
    let (s_, t_) = (0.3, 0.6);
    let r0 = pin.ray(s_, t_, &mut rr);
    let focus_pt = r0.o + r0.d * (4.0 / r0.d.dot(pin.fwd));
    for _ in 0..50 {
        let r = dof.ray(s_, t_, &mut rr);
        let p = r.o + r.d * (4.0 / r.d.dot(dof.fwd));
        assert!(
            p.near(focus_pt, 1e-9),
            "lens rays converge on the focus plane"
        );
    }
    println!("thin-lens rays converge at the focus point");

    // ---------- 4. white furnace ----------
    let furnace = |mat: Material| -> (f64, f64) {
        let mut s = Scene::new(vec![mat], Env::Uniform(Vec3::ONE));
        s.prims.push(Prim {
            shape: Shape::Sphere {
                c: v3(0.0, 0.0, -4.0),
                r: 1.0,
            },
            mat: 0,
        });
        s.finish();
        let cam = Camera::new(Vec3::ZERO, v3(0.0, 0.0, -1.0), 20.0, 1.0, 0.0, 4.0);
        let cfg = Cfg {
            max_depth: 64,
            seed: 7,
            ..base_cfg(24, 24, Mode::Path, 64)
        };
        let fb = render(&s, &cam, &cfg);
        assert!(all_finite(&fb));
        let m = fb.iter().map(|c| c.x).sum::<f64>() / fb.len() as f64;
        let worst = fb.iter().map(|c| (c.x - m).abs()).fold(0.0, f64::max);
        (m, worst)
    };
    let (m1, dev1) = furnace(Material::Lambert(Vec3::ONE));
    assert!(
        (m1 - 1.0).abs() < 1e-9,
        "albedo-1 diffuse sphere is invisible in a white furnace: {m1}"
    );
    let (m2, dev2) = furnace(Material::Glass(1.5, Vec3::ZERO));
    assert!(
        (m2 - 1.0).abs() < 0.01,
        "lossless glass conserves energy: {m2}"
    );
    let (m3, _) = furnace(Material::Lambert(v3(0.5, 0.5, 0.5)));
    assert!(
        (m3 - 0.5).abs() < 0.02,
        "albedo-0.5 sphere fills the frame and returns exactly its albedo: {m3}"
    );
    let (m4, _) = furnace(Material::Mirror(Vec3::ONE));
    assert!((m4 - 1.0).abs() < 1e-9);
    println!(
        "white furnace: diffuse(1) mean {m1:.6} (max dev {dev1:.1e}), glass mean {m2:.4} (max dev {dev2:.3}), mirror {m4:.6}"
    );

    // ---------- 5. path-tracer convergence ----------
    let (cs, ccam) = cornell_scene(v3(12.0, 11.0, 9.0));
    let (rw, rh) = (40, 40);
    let reference = render(
        &cs,
        &ccam,
        &Cfg {
            seed: 999,
            ..base_cfg(rw, rh, Mode::Path, 1024)
        },
    );
    assert!(all_finite(&reference));
    let mut errs = vec![];
    for spp in [1usize, 4, 16, 64] {
        let fb = render(
            &cs,
            &ccam,
            &Cfg {
                seed: 31337,
                ..base_cfg(rw, rh, Mode::Path, spp)
            },
        );
        assert!(all_finite(&fb));
        errs.push((spp, rmse(&fb, &reference)));
    }
    for (spp, e) in &errs {
        println!(
            "path tracer {spp:>3} spp: RMSE vs 1024 spp = {e:.4}   (RMSE * sqrt(spp) = {:.3})",
            e * (*spp as f64).sqrt()
        );
    }
    assert!(
        errs.windows(2).all(|w| w[1].1 < w[0].1),
        "RMSE must fall as samples grow: {errs:?}"
    );
    assert!(
        errs[0].1 / errs[3].1 > 2.5,
        "1 -> 64 spp should cut the error well beyond 2.5x (ideal 8x)"
    );

    // ---------- 6. BVH speed-up on a few thousand triangles ----------
    let (big, bigcam) = whitted_scene(4);
    let tris = big
        .prims
        .iter()
        .filter(|p| matches!(p.shape, Shape::Tri { .. }))
        .count();
    let mut sp = base_cfg(64, 48, Mode::Whitted, 1);
    sp.threads = 1;
    let counted = |cfg: &Cfg| {
        PRIM_TESTS.store(0, Ordering::Relaxed);
        COUNTING.store(true, Ordering::Relaxed);
        let t0 = Instant::now();
        let img = render(&big, &bigcam, cfg);
        let secs = t0.elapsed().as_secs_f64();
        COUNTING.store(false, Ordering::Relaxed);
        (img, PRIM_TESTS.load(Ordering::Relaxed), secs)
    };
    let (fast, n_bvh, t_bvh) = counted(&sp);
    sp.use_bvh = false;
    let (slow, n_brute, t_brute) = counted(&sp);
    assert!(fast == slow);
    println!(
        "BVH on {tris} triangles (64x48, 1 thread): {n_brute} primitive tests brute force, {n_bvh} with the BVH ({t_brute:.3}s and {t_bvh:.3}s)"
    );
    assert!(
        n_bvh * 20 < n_brute,
        "the BVH must test at most a twentieth of the primitives: {n_bvh} of {n_brute}"
    );

    // ---------- 7. final images ----------
    let (w, h) = (320, 240);
    let t0 = Instant::now();
    let fb = render(
        &sc3,
        &cam3,
        &Cfg {
            seed: 1,
            area_samples: 12,
            ..base_cfg(w, h, Mode::Whitted, 9)
        },
    );
    assert!(all_finite(&fb));
    save_png("c12_whitted.png", w, h, &fb);
    let (gs, gcam) = glass_scene();
    let fb2 = render(
        &gs,
        &gcam,
        &Cfg {
            seed: 2,
            area_samples: 12,
            max_depth: 8,
            ..base_cfg(w, h, Mode::Whitted, 16)
        },
    );
    assert!(all_finite(&fb2));
    save_png("c12_glass_mirror.png", w, h, &fb2);
    let (cw, ch) = (160, 160);
    let fb3 = render(
        &cs,
        &ccam,
        &Cfg {
            seed: 3,
            ..base_cfg(cw, ch, Mode::Path, 256)
        },
    );
    assert!(all_finite(&fb3));
    save_png("c12_cornell_path.png", cw, ch, &fb3);
    println!(
        "final images rendered in {:.2}s",
        t0.elapsed().as_secs_f64()
    );
    print!("{}", ascii_preview(&fb3, cw, ch, 56, 24));
    println!("all checks passed in {:.2}s", t_all.elapsed().as_secs_f64());
}
