//! RECIPE 250 (graphics): 3D math from scratch: Vec3, Mat4, projections, quaternions, gimbal lock, ray intersections
//! LEVEL: intermediate
//! Use in: renderers, game engines, CAD viewers, robotics kinematics, anything that moves points in 3D.   Run: cargo run --release --example vec3_matrix_quaternion
#![allow(dead_code)]
//!
//! WHAT: the small kit every 3D program needs, written with plain f64 and no crates: a `Vec3` with dot/cross/reflect/
//! refract/Schlick, a row-major `Mat4` acting on COLUMN vectors (p' = M * p), look-at and perspective/orthographic
//! projection, unit quaternions with SLERP, Euler angles (and why they gimbal-lock), barycentric coordinates, and the
//! three classic ray tests (Moller-Trumbore triangle, sphere, AABB slab).
//! WHY: these formulas are tiny but easy to get subtly wrong (handedness, row vs column, w division, epsilon). Each is
//! checked here against a known value or a property test with a seeded RNG, so a wrong sign fails loudly.
//! HOW (the math, plain text):
//!   reflect(i, n)  = i - 2 (i.n) n                          (n unit)
//!   refract(i,n,e) = e i + (e c - sqrt(1 - e^2 (1 - c^2))) n, c = -i.n, e = n1/n2; radicand < 0 => total internal reflection
//!   Schlick(c)     = R0 + (1 - R0)(1 - c)^5, R0 = ((n1-n2)/(n1+n2))^2
//!   point vs direction: points use w = 1 (translation applies, then divide by w), directions use w = 0 (no translation).
//!   perspective (OpenGL, right-handed, camera looks down -z, NDC cube [-1,1]^3):
//!       x' = x/(r), y' = y/t scaled by n; z' = (-(f+n) z - 2fn)/(f-n) before dividing by w = -z.
//!       The near-plane corner (r, t, -n) maps to (1, 1, -1); the far-plane corner (r f/n, t f/n, -f) maps to (1, 1, 1).
//!   quaternion q = (cos(a/2), sin(a/2) axis); rotate v: q v q*; SLERP(a,b,t) = sin((1-t)W)/sin(W) a + sin(tW)/sin(W) b,
//!       cos(W) = a.b (negate b if the dot is negative to take the short arc).
//!   Moller-Trumbore: solve o + t d = (1-u-v) a + u b + v c with Cramer's rule: e1 = b-a, e2 = c-a, p = d x e2,
//!       det = e1.p, u = (o-a).p / det, q = (o-a) x e1, v = d.q / det, t = e2.q / det.
//!   GIMBAL LOCK: with R = Rz(yaw) Ry(pitch) Rx(roll), at pitch = 90 degrees R depends only on yaw - roll, so one degree
//!       of freedom is lost: many (yaw, roll) pairs give the SAME rotation. Quaternions have no such singularity.
//!   SHADOW ACNE: a ray starting exactly on a surface can re-hit it at t ~ 1e-7 from rounding; fix with a t_min epsilon
//!       or by offsetting the origin along the normal.
//! COMMON USES: games (cameras, skeletal animation with slerp), CAD/CAM viewers, film VFX and offline renderers, robotics
//!   (orientation interpolation, frame transforms), synthetic image generation for ML datasets.
//! Read: https://raytracing.github.io/ and https://www.pbr-book.org/
use std::ops::{Add, Div, Mul, Neg, Sub};

const EPS: f64 = 1e-9;

// ---------- seeded RNG (splitmix64) ----------
struct Rng(u64);
impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
    /// uniform in [0,1)
    fn f(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.f()
    }
    fn vec(&mut self) -> Vec3 {
        Vec3::new(
            self.range(-5.0, 5.0),
            self.range(-5.0, 5.0),
            self.range(-5.0, 5.0),
        )
    }
    fn unit(&mut self) -> Vec3 {
        loop {
            let v = Vec3::new(
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
            );
            let l = v.length();
            if l > 1e-3 && l <= 1.0 {
                return v / l;
            }
        }
    }
}

// ---------- Vec3 ----------
#[derive(Clone, Copy, Debug, PartialEq)]
struct Vec3 {
    x: f64,
    y: f64,
    z: f64,
}
impl Vec3 {
    const fn new(x: f64, y: f64, z: f64) -> Self {
        Vec3 { x, y, z }
    }
    fn dot(self, o: Vec3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    fn length(self) -> f64 {
        self.dot(self).sqrt()
    }
    fn normalize(self) -> Vec3 {
        let l = self.length();
        if l < EPS { self } else { self / l }
    }
    fn lerp(self, o: Vec3, t: f64) -> Vec3 {
        self * (1.0 - t) + o * t
    }
    fn reflect(self, n: Vec3) -> Vec3 {
        self - n * (2.0 * self.dot(n))
    }
    /// `self` unit incoming, `n` unit normal, `eta` = n1/n2. None = total internal reflection.
    fn refract(self, n: Vec3, eta: f64) -> Option<Vec3> {
        let (n, c) = if self.dot(n) < 0.0 {
            (n, -self.dot(n))
        } else {
            (-n, self.dot(n))
        };
        let k = 1.0 - eta * eta * (1.0 - c * c);
        if k < 0.0 {
            None
        } else {
            Some(self * eta + n * (eta * c - k.sqrt()))
        }
    }
    fn near(self, o: Vec3, tol: f64) -> bool {
        (self - o).length() <= tol
    }
}
impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f64> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f64) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
}
impl Div<f64> for Vec3 {
    type Output = Vec3;
    fn div(self, s: f64) -> Vec3 {
        Vec3::new(self.x / s, self.y / s, self.z / s)
    }
}
impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        Vec3::new(-self.x, -self.y, -self.z)
    }
}

/// Schlick's approximation of Fresnel reflectance; `cos` = cosine of the incident angle.
fn schlick(cos: f64, n1: f64, n2: f64) -> f64 {
    let r0 = ((n1 - n2) / (n1 + n2)).powi(2);
    r0 + (1.0 - r0) * (1.0 - cos).powi(5)
}

// ---------- Mat4 ----------
#[derive(Clone, Copy, Debug)]
struct Mat4 {
    m: [[f64; 4]; 4], // row-major: m[row][col]
}
impl Mat4 {
    fn identity() -> Mat4 {
        let mut m = [[0.0; 4]; 4];
        for (i, row) in m.iter_mut().enumerate() {
            row[i] = 1.0;
        }
        Mat4 { m }
    }
    fn translate(t: Vec3) -> Mat4 {
        let mut r = Mat4::identity();
        r.m[0][3] = t.x;
        r.m[1][3] = t.y;
        r.m[2][3] = t.z;
        r
    }
    fn scale(s: Vec3) -> Mat4 {
        let mut r = Mat4::identity();
        r.m[0][0] = s.x;
        r.m[1][1] = s.y;
        r.m[2][2] = s.z;
        r
    }
    /// Rodrigues rotation about a (normalised here) axis.
    fn rotate(axis: Vec3, angle: f64) -> Mat4 {
        let a = axis.normalize();
        let (s, c) = angle.sin_cos();
        let t = 1.0 - c;
        let mut r = Mat4::identity();
        r.m[0] = [
            t * a.x * a.x + c,
            t * a.x * a.y - s * a.z,
            t * a.x * a.z + s * a.y,
            0.0,
        ];
        r.m[1] = [
            t * a.x * a.y + s * a.z,
            t * a.y * a.y + c,
            t * a.y * a.z - s * a.x,
            0.0,
        ];
        r.m[2] = [
            t * a.x * a.z - s * a.y,
            t * a.y * a.z + s * a.x,
            t * a.z * a.z + c,
            0.0,
        ];
        r
    }
    fn mul(&self, o: &Mat4) -> Mat4 {
        let mut r = [[0.0; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                r[i][j] = (0..4).map(|k| self.m[i][k] * o.m[k][j]).sum();
            }
        }
        Mat4 { m: r }
    }
    fn transpose(&self) -> Mat4 {
        let mut r = [[0.0; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                r[i][j] = self.m[j][i];
            }
        }
        Mat4 { m: r }
    }
    fn det3(&self) -> f64 {
        let m = &self.m;
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    }
    /// Gauss-Jordan with partial pivoting. None if singular.
    fn inverse(&self) -> Option<Mat4> {
        let mut a = self.m;
        let mut inv = Mat4::identity().m;
        for col in 0..4 {
            let piv =
                (col..4).max_by(|&i, &j| a[i][col].abs().partial_cmp(&a[j][col].abs()).unwrap())?;
            if a[piv][col].abs() < 1e-12 {
                return None;
            }
            a.swap(col, piv);
            inv.swap(col, piv);
            let d = a[col][col];
            for j in 0..4 {
                a[col][j] /= d;
                inv[col][j] /= d;
            }
            for i in 0..4 {
                if i != col {
                    let f = a[i][col];
                    for j in 0..4 {
                        a[i][j] -= f * a[col][j];
                        inv[i][j] -= f * inv[col][j];
                    }
                }
            }
        }
        Some(Mat4 { m: inv })
    }
    /// Point: w = 1, then perspective-divide by the resulting w.
    fn transform_point(&self, p: Vec3) -> Vec3 {
        let m = &self.m;
        let x = m[0][0] * p.x + m[0][1] * p.y + m[0][2] * p.z + m[0][3];
        let y = m[1][0] * p.x + m[1][1] * p.y + m[1][2] * p.z + m[1][3];
        let z = m[2][0] * p.x + m[2][1] * p.y + m[2][2] * p.z + m[2][3];
        let w = m[3][0] * p.x + m[3][1] * p.y + m[3][2] * p.z + m[3][3];
        if (w - 1.0).abs() > EPS && w.abs() > EPS {
            Vec3::new(x / w, y / w, z / w)
        } else {
            Vec3::new(x, y, z)
        }
    }
    /// Direction: w = 0, translation ignored.
    fn transform_dir(&self, d: Vec3) -> Vec3 {
        let m = &self.m;
        Vec3::new(
            m[0][0] * d.x + m[0][1] * d.y + m[0][2] * d.z,
            m[1][0] * d.x + m[1][1] * d.y + m[1][2] * d.z,
            m[2][0] * d.x + m[2][1] * d.y + m[2][2] * d.z,
        )
    }
    fn max_abs_diff(&self, o: &Mat4) -> f64 {
        let mut d: f64 = 0.0;
        for i in 0..4 {
            for j in 0..4 {
                d = d.max((self.m[i][j] - o.m[i][j]).abs());
            }
        }
        d
    }
    /// Right-handed view matrix: camera at `eye` looking at `target`, looking down its local -z.
    fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
        let f = (target - eye).normalize();
        let s = f.cross(up).normalize();
        let u = s.cross(f);
        let mut r = Mat4::identity();
        r.m[0] = [s.x, s.y, s.z, -s.dot(eye)];
        r.m[1] = [u.x, u.y, u.z, -u.dot(eye)];
        r.m[2] = [-f.x, -f.y, -f.z, f.dot(eye)];
        r
    }
    fn perspective(fov_y: f64, aspect: f64, n: f64, f: f64) -> Mat4 {
        let t = 1.0 / (fov_y / 2.0).tan();
        let mut r = Mat4 { m: [[0.0; 4]; 4] };
        r.m[0][0] = t / aspect;
        r.m[1][1] = t;
        r.m[2][2] = -(f + n) / (f - n);
        r.m[2][3] = -2.0 * f * n / (f - n);
        r.m[3][2] = -1.0;
        r
    }
    fn orthographic(l: f64, r: f64, b: f64, t: f64, n: f64, f: f64) -> Mat4 {
        let mut m = Mat4::identity();
        m.m[0][0] = 2.0 / (r - l);
        m.m[1][1] = 2.0 / (t - b);
        m.m[2][2] = -2.0 / (f - n);
        m.m[0][3] = -(r + l) / (r - l);
        m.m[1][3] = -(t + b) / (t - b);
        m.m[2][3] = -(f + n) / (f - n);
        m
    }
}

// ---------- Quaternion ----------
#[derive(Clone, Copy, Debug)]
struct Quat {
    w: f64,
    x: f64,
    y: f64,
    z: f64,
}
impl Quat {
    fn from_axis_angle(axis: Vec3, angle: f64) -> Quat {
        let a = axis.normalize();
        let (s, c) = (angle / 2.0).sin_cos();
        Quat {
            w: c,
            x: a.x * s,
            y: a.y * s,
            z: a.z * s,
        }
    }
    fn mul(self, o: Quat) -> Quat {
        Quat {
            w: self.w * o.w - self.x * o.x - self.y * o.y - self.z * o.z,
            x: self.w * o.x + self.x * o.w + self.y * o.z - self.z * o.y,
            y: self.w * o.y - self.x * o.z + self.y * o.w + self.z * o.x,
            z: self.w * o.z + self.x * o.y - self.y * o.x + self.z * o.w,
        }
    }
    fn conj(self) -> Quat {
        Quat {
            w: self.w,
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }
    fn norm(self) -> f64 {
        (self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }
    fn normalize(self) -> Quat {
        let n = self.norm();
        Quat {
            w: self.w / n,
            x: self.x / n,
            y: self.y / n,
            z: self.z / n,
        }
    }
    fn dot(self, o: Quat) -> f64 {
        self.w * o.w + self.x * o.x + self.y * o.y + self.z * o.z
    }
    fn rotate(self, v: Vec3) -> Vec3 {
        let p = Quat {
            w: 0.0,
            x: v.x,
            y: v.y,
            z: v.z,
        };
        let r = self.mul(p).mul(self.conj());
        Vec3::new(r.x, r.y, r.z)
    }
    fn to_mat4(self) -> Mat4 {
        let Quat { w, x, y, z } = self.normalize();
        let mut r = Mat4::identity();
        r.m[0] = [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - w * z),
            2.0 * (x * z + w * y),
            0.0,
        ];
        r.m[1] = [
            2.0 * (x * y + w * z),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - w * x),
            0.0,
        ];
        r.m[2] = [
            2.0 * (x * z - w * y),
            2.0 * (y * z + w * x),
            1.0 - 2.0 * (x * x + y * y),
            0.0,
        ];
        r
    }
    fn slerp(self, mut b: Quat, t: f64) -> Quat {
        let mut d = self.dot(b);
        if d < 0.0 {
            b = Quat {
                w: -b.w,
                x: -b.x,
                y: -b.y,
                z: -b.z,
            };
            d = -d;
        }
        let (ka, kb) = if d > 0.9995 {
            (1.0 - t, t) // nearly parallel: sin(W) ~ 0, fall back to nlerp
        } else {
            let w = d.acos();
            (((1.0 - t) * w).sin() / w.sin(), (t * w).sin() / w.sin())
        };
        Quat {
            w: ka * self.w + kb * b.w,
            x: ka * self.x + kb * b.x,
            y: ka * self.y + kb * b.y,
            z: ka * self.z + kb * b.z,
        }
        .normalize()
    }
}

/// R = Rz(yaw) * Ry(pitch) * Rx(roll)
fn euler_matrix(yaw: f64, pitch: f64, roll: f64) -> Mat4 {
    Mat4::rotate(Vec3::new(0.0, 0.0, 1.0), yaw)
        .mul(&Mat4::rotate(Vec3::new(0.0, 1.0, 0.0), pitch))
        .mul(&Mat4::rotate(Vec3::new(1.0, 0.0, 0.0), roll))
}
/// Extract (yaw, pitch, roll); at pitch = +-90 degrees roll is not recoverable, we set it to 0 (gimbal lock).
fn euler_from_matrix(r: &Mat4) -> (f64, f64, f64, bool) {
    let sp = -r.m[2][0];
    if sp.abs() > 1.0 - 1e-9 {
        let pitch = sp.signum() * std::f64::consts::FRAC_PI_2;
        let yaw = (-r.m[0][1]).atan2(r.m[1][1]);
        (yaw, pitch, 0.0, true)
    } else {
        (
            r.m[1][0].atan2(r.m[0][0]),
            sp.asin(),
            r.m[2][1].atan2(r.m[2][2]),
            false,
        )
    }
}

// ---------- barycentric + ray tests ----------
fn barycentric(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> (f64, f64, f64) {
    let (v0, v1, v2) = (b - a, c - a, p - a);
    let (d00, d01, d11) = (v0.dot(v0), v0.dot(v1), v1.dot(v1));
    let (d20, d21) = (v2.dot(v0), v2.dot(v1));
    let den = d00 * d11 - d01 * d01;
    let v = (d11 * d20 - d01 * d21) / den;
    let w = (d00 * d21 - d01 * d20) / den;
    (1.0 - v - w, v, w)
}

/// Moller-Trumbore. Returns (t, u, v).
fn ray_triangle(o: Vec3, d: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<(f64, f64, f64)> {
    let (e1, e2) = (b - a, c - a);
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < EPS {
        return None; // parallel
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    if t > EPS { Some((t, u, v)) } else { None }
}

/// Nearest positive t with |o + t d - c| = r. `tmin` guards against self-intersection.
fn ray_sphere(o: Vec3, d: Vec3, c: Vec3, r: f64, tmin: f64) -> Option<f64> {
    let oc = o - c;
    let a = d.dot(d);
    let hb = oc.dot(d);
    let disc = hb * hb - a * (oc.dot(oc) - r * r);
    if disc < 0.0 {
        return None;
    }
    let sq = disc.sqrt();
    [(-hb - sq) / a, (-hb + sq) / a]
        .into_iter()
        .find(|&t| t > tmin)
}

/// Slab method: intersect the ray with three pairs of axis planes, keep the overlap of the t-intervals.
fn ray_aabb(o: Vec3, d: Vec3, lo: Vec3, hi: Vec3) -> Option<(f64, f64)> {
    let (mut t0, mut t1) = (0.0_f64, f64::INFINITY);
    for (oo, dd, l, h) in [
        (o.x, d.x, lo.x, hi.x),
        (o.y, d.y, lo.y, hi.y),
        (o.z, d.z, lo.z, hi.z),
    ] {
        let inv = 1.0 / dd; // dd == 0 gives +-inf, which the min/max below handle correctly
        let (mut a, mut b) = ((l - oo) * inv, (h - oo) * inv);
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        t0 = t0.max(a);
        t1 = t1.min(b);
        if t0 > t1 {
            return None;
        }
    }
    Some((t0, t1))
}

/// f32 sphere test used only for the shadow-acne demo (f32 rounding is ~1e-7, so acne is visible).
fn ray_sphere_f32(o: [f32; 3], d: [f32; 3], tmin: f32) -> bool {
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let hb = dot(o, d);
    let disc = hb * hb - dot(d, d) * (dot(o, o) - 1.0);
    if disc < 0.0 {
        return false;
    }
    let sq = disc.sqrt();
    [-hb - sq, -hb + sq].iter().any(|&t| t > tmin)
}

fn main() {
    let mut rng = Rng(0xC0FFEE);
    let pi = std::f64::consts::PI;

    // --- Vec3 basics
    let (x, y, z) = (
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    );
    assert_eq!(x.cross(y), z);
    assert_eq!(Vec3::new(3.0, 4.0, 0.0).length(), 5.0);
    assert!(x.lerp(y, 0.5).near(Vec3::new(0.5, 0.5, 0.0), 1e-12));
    let r = Vec3::new(1.0, -1.0, 0.0).normalize().reflect(y);
    assert!(r.near(Vec3::new(1.0, 1.0, 0.0).normalize(), 1e-12));
    println!("Vec3 basics ok");

    // --- refraction: Snell's law, and total internal reflection leaving glass at a steep angle
    let theta_i = 30f64.to_radians();
    let inc = Vec3::new(theta_i.sin(), -theta_i.cos(), 0.0);
    let t = inc
        .refract(y, 1.0 / 1.5)
        .expect("air->glass always refracts");
    let theta_t = (t.x / t.length()).asin();
    assert!(
        (1.0 * theta_i.sin() - 1.5 * theta_t.sin()).abs() < 1e-12,
        "Snell"
    );
    let steep = Vec3::new(60f64.to_radians().sin(), -60f64.to_radians().cos(), 0.0);
    assert!(
        steep.refract(y, 1.5).is_none(),
        "glass->air at 60 deg exceeds the 41.8 deg critical angle"
    );
    assert!(inc.refract(y, 1.5).is_some());
    let r_normal = schlick(1.0, 1.0, 1.5);
    assert!(
        (r_normal - 0.04).abs() < 1e-12,
        "glass reflects 4% at normal incidence"
    );
    assert!(
        schlick(0.01, 1.0, 1.5) > 0.9,
        "grazing angles are nearly mirrors"
    );
    println!(
        "refract/Schlick ok: R(0 deg) = {r_normal:.3}, R(89.4 deg) = {:.3}",
        schlick(0.01, 1.0, 1.5)
    );

    // --- Mat4: point vs direction, composition order, inverse
    let tr = Mat4::translate(Vec3::new(1.0, 2.0, 3.0));
    assert!(
        tr.transform_point(Vec3::new(1.0, 1.0, 1.0))
            .near(Vec3::new(2.0, 3.0, 4.0), 1e-12)
    );
    assert!(
        tr.transform_dir(Vec3::new(1.0, 1.0, 1.0))
            .near(Vec3::new(1.0, 1.0, 1.0), 1e-12)
    );
    // rotate-then-translate is M = T * R (rightmost applied first)
    let m = tr.mul(&Mat4::rotate(z, pi / 2.0));
    assert!(m.transform_point(x).near(Vec3::new(1.0, 3.0, 3.0), 1e-12));
    assert!(
        Mat4::scale(Vec3::new(2.0, 3.0, 4.0))
            .transform_point(Vec3::new(1.0, 1.0, 1.0))
            .near(Vec3::new(2.0, 3.0, 4.0), 1e-12)
    );
    assert!(
        Mat4 { m: [[1.0; 4]; 4] }.inverse().is_none(),
        "singular matrix has no inverse"
    );

    // --- view + projection: frustum corners map to the NDC cube corners
    let (fov, aspect, n, f) = (90f64.to_radians(), 2.0, 0.5, 50.0);
    let p = Mat4::perspective(fov, aspect, n, f);
    let tt = n * (fov / 2.0).tan();
    let rr = tt * aspect;
    assert!(
        p.transform_point(Vec3::new(rr, tt, -n))
            .near(Vec3::new(1.0, 1.0, -1.0), 1e-9),
        "near corner"
    );
    let k = f / n;
    assert!(
        p.transform_point(Vec3::new(rr * k, tt * k, -f))
            .near(Vec3::new(1.0, 1.0, 1.0), 1e-9),
        "far corner"
    );
    assert!(
        p.transform_point(Vec3::new(-rr * k, -tt * k, -f))
            .near(Vec3::new(-1.0, -1.0, 1.0), 1e-9)
    );
    let o = Mat4::orthographic(-4.0, 4.0, -3.0, 3.0, 1.0, 10.0);
    assert!(
        o.transform_point(Vec3::new(4.0, 3.0, -1.0))
            .near(Vec3::new(1.0, 1.0, -1.0), 1e-12)
    );
    assert!(
        o.transform_point(Vec3::new(-4.0, -3.0, -10.0))
            .near(Vec3::new(-1.0, -1.0, 1.0), 1e-12)
    );
    let view = Mat4::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::new(0.0, 0.0, 0.0), y);
    assert!(
        view.transform_point(Vec3::new(0.0, 0.0, 0.0))
            .near(Vec3::new(0.0, 0.0, -5.0), 1e-12),
        "target is 5 units down -z"
    );
    let clip = p.mul(&view).transform_point(Vec3::new(0.0, 0.0, 0.0));
    assert!(clip.x.abs() < 1e-12 && clip.y.abs() < 1e-12 && clip.z > -1.0 && clip.z < 1.0);
    println!(
        "projection ok: frustum corners -> NDC +-1; target depth in NDC = {:.4}",
        clip.z
    );

    // --- quaternions
    let q = Quat::from_axis_angle(z, pi / 2.0);
    assert!(q.rotate(x).near(y, 1e-12));
    let (qa, qb) = (
        Quat::from_axis_angle(x, 0.3),
        Quat::from_axis_angle(Vec3::new(1.0, 2.0, 3.0), 2.5),
    );
    assert!(
        (qa.slerp(qb, 0.0).dot(qa) - 1.0).abs() < 1e-12
            && (qa.slerp(qb, 1.0).dot(qb).abs() - 1.0).abs() < 1e-12
    );
    let half = Quat::from_axis_angle(z, 0.0).slerp(Quat::from_axis_angle(z, pi / 2.0), 0.5);
    assert!(
        half.rotate(x)
            .near(Vec3::new(0.5f64.sqrt(), 0.5f64.sqrt(), 0.0), 1e-12),
        "halfway = 45 degrees"
    );
    println!("quaternion basics ok");

    // --- gimbal lock
    let a = euler_matrix(0.7, pi / 2.0, 0.2);
    let b = euler_matrix(0.7 + 0.4, pi / 2.0, 0.2 + 0.4); // shift yaw and roll together
    assert!(
        a.max_abs_diff(&b) < 1e-12,
        "at pitch 90 only yaw-roll matters"
    );
    let c = euler_matrix(0.7, 0.4, 0.2);
    let d = euler_matrix(0.7 + 0.4, 0.4, 0.2 + 0.4);
    assert!(
        c.max_abs_diff(&d) > 0.1,
        "away from the pole the same shift changes the rotation"
    );
    let (yw, pt, rl, locked) = euler_from_matrix(&a);
    assert!(locked && rl == 0.0);
    assert!(
        euler_matrix(yw, pt, rl).max_abs_diff(&a) < 1e-9,
        "rotation survives, the (yaw, roll) split is lost"
    );
    let (_, _, _, locked2) = euler_from_matrix(&c);
    assert!(!locked2);
    println!(
        "gimbal lock: (yaw,roll) = (0.7,0.2) and (1.1,0.6) give the identical matrix at pitch 90 degrees"
    );

    // --- barycentric, ray tests
    let (ta, tb, tc) = (
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(4.0, 0.0, 0.0),
        Vec3::new(0.0, 4.0, 0.0),
    );
    let (l0, l1, l2) = barycentric(Vec3::new(1.0, 1.0, 0.0), ta, tb, tc);
    assert!((l0 - 0.5).abs() < 1e-12 && (l1 - 0.25).abs() < 1e-12 && (l2 - 0.25).abs() < 1e-12);
    let (t, u, v) = ray_triangle(Vec3::new(1.0, 1.0, 5.0), -z, ta, tb, tc).unwrap();
    assert!((t - 5.0).abs() < 1e-12 && (u - 0.25).abs() < 1e-12 && (v - 0.25).abs() < 1e-12);
    assert!(
        ray_triangle(Vec3::new(3.0, 3.0, 5.0), -z, ta, tb, tc).is_none(),
        "outside the hypotenuse"
    );
    assert!(
        ray_triangle(Vec3::new(1.0, 1.0, 5.0), z, ta, tb, tc).is_none(),
        "behind the ray"
    );
    assert!(
        ray_triangle(Vec3::new(1.0, 1.0, 5.0), x, ta, tb, tc).is_none(),
        "parallel"
    );
    assert!(
        (ray_sphere(
            Vec3::new(0.0, 0.0, 5.0),
            -z,
            Vec3::new(0.0, 0.0, 0.0),
            1.0,
            0.0
        )
        .unwrap()
            - 4.0)
            .abs()
            < 1e-12
    );
    assert!(
        ray_sphere(
            Vec3::new(0.0, 2.0, 5.0),
            -z,
            Vec3::new(0.0, 0.0, 0.0),
            1.0,
            0.0
        )
        .is_none()
    );
    let (t0, t1) = ray_aabb(
        Vec3::new(-5.0, 0.5, 0.5),
        x,
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 1.0, 1.0),
    )
    .unwrap();
    assert!((t0 - 5.0).abs() < 1e-12 && (t1 - 6.0).abs() < 1e-12);
    assert!(
        ray_aabb(
            Vec3::new(-5.0, 2.0, 0.5),
            x,
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 1.0)
        )
        .is_none()
    );
    println!("barycentric and ray tests ok");

    // --- property tests (seeded)
    let (mut worst_inv, mut worst_orth, mut worst_q) = (0.0f64, 0.0f64, 0.0f64);
    for _ in 0..500 {
        let axis = rng.unit();
        let ang = rng.range(-pi, pi);
        let rm = Mat4::rotate(axis, ang);
        let prod = rm.mul(&rm.transpose());
        worst_orth = worst_orth.max(prod.max_abs_diff(&Mat4::identity()));
        assert!((rm.det3() - 1.0).abs() < 1e-12, "det = 1");
        // general affine matrix: T * R * S
        let g = Mat4::translate(rng.vec())
            .mul(&rm)
            .mul(&Mat4::scale(Vec3::new(
                rng.range(0.5, 3.0),
                rng.range(0.5, 3.0),
                rng.range(0.5, 3.0),
            )));
        worst_inv = worst_inv.max(g.mul(&g.inverse().unwrap()).max_abs_diff(&Mat4::identity()));
        // cross product orthogonality
        let (va, vb) = (rng.vec(), rng.vec());
        let cr = va.cross(vb);
        assert!(cr.dot(va).abs() < 1e-9 && cr.dot(vb).abs() < 1e-9);
        // reflect keeps length
        let nn = rng.unit();
        assert!((va.reflect(nn).length() - va.length()).abs() < 1e-9);
        // quaternion == matrix, and unit length is preserved by products
        let qq = Quat::from_axis_angle(axis, ang);
        let vq = qq.rotate(va);
        worst_q = worst_q.max(
            vq.near(rm.transform_dir(va), 1e-9)
                .then_some(0.0)
                .unwrap_or(1.0),
        );
        assert!((vq.length() - va.length()).abs() < 1e-9);
        let prodq = qq.mul(Quat::from_axis_angle(rng.unit(), rng.range(-pi, pi)));
        assert!((prodq.norm() - 1.0).abs() < 1e-12);
        assert!(qq.to_mat4().max_abs_diff(&rm) < 1e-12);
    }
    assert!(worst_q == 0.0);
    println!(
        "500 random trials: worst |R R^T - I| = {worst_orth:.2e}, worst |M M^-1 - I| = {worst_inv:.2e}, quat == matrix"
    );

    // --- shadow acne: start rays exactly on the surface, point them away from it, count false self-hits
    let (mut acne, mut fixed) = (0, 0);
    for _ in 0..20000 {
        let nrm = rng.unit();
        let o = [nrm.x as f32, nrm.y as f32, nrm.z as f32]; // on the unit sphere up to f32 rounding
        let mut dir = rng.unit();
        if dir.dot(nrm) < 0.0 {
            dir = -dir; // towards the "light", outside the sphere
        }
        let dv = [dir.x as f32, dir.y as f32, dir.z as f32];
        if ray_sphere_f32(o, dv, 0.0) {
            acne += 1;
        }
        if ray_sphere_f32(o, dv, 1e-3) {
            fixed += 1;
        }
    }
    println!(
        "shadow acne: {acne}/20000 outward rays falsely hit their own sphere with t_min = 0; with t_min = 1e-3: {fixed}"
    );
    assert_eq!(fixed, 0);
    assert!(acne > 0, "f32 rounding makes self-intersection visible");
    println!("all checks passed");
}
