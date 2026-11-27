use std::ops::{Add, Div, Mul, Neg, Sub};

// ---- vectors ----

/// A point, a direction or a colour.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

pub const fn v3(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3 { x, y, z }
}

impl Vec3 {
    pub fn dot(self, o: Vec3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Vec3) -> Vec3 {
        v3(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }

    /// The same direction with length 1.
    pub fn unit(self) -> Vec3 {
        self / self.length()
    }

    /// Component by component product, used to filter a colour.
    pub fn times(self, o: Vec3) -> Vec3 {
        v3(self.x * o.x, self.y * o.y, self.z * o.z)
    }

    /// Component 0, 1 or 2.
    pub fn axis(self, i: usize) -> f64 {
        [self.x, self.y, self.z][i]
    }

    pub fn max_part(self) -> f64 {
        self.x.max(self.y).max(self.z)
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
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

// ---- rays and surfaces ----

/// The points `origin + t * dir`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    pub dir: Vec3,
}

impl Ray {
    pub fn at(&self, t: f64) -> Vec3 {
        self.origin + self.dir * t
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    Sphere { center: Vec3, radius: f64 },
    Triangle { a: Vec3, b: Vec3, c: Vec3 },
}

impl Shape {
    /// The smallest `t` with `tmin < t < tmax` at which the ray meets the surface.
    pub fn hit(&self, ray: &Ray, tmin: f64, tmax: f64) -> Option<f64> {
        match *self {
            Shape::Sphere { center, radius } => {
                let oc = ray.origin - center;
                let a = ray.dir.dot(ray.dir);
                let b = oc.dot(ray.dir);
                // The part of `oc` at right angles to the ray. Working from it keeps the answer
                // accurate when the sphere is far away compared with its size.
                let side = oc - ray.dir * (b / a);
                let inside = radius * radius - side.dot(side);
                if inside.is_nan() || inside < 0.0 {
                    return None;
                }
                let half = (inside / a).sqrt();
                let middle = -b / a;
                [middle - half, middle + half]
                    .into_iter()
                    .find(|&t| t > tmin && t < tmax)
            }
            Shape::Triangle { a, b, c } => {
                let (e1, e2) = (b - a, c - a);
                let p = ray.dir.cross(e2);
                let det = e1.dot(p);
                // Compared with the sizes involved, so that scale does not matter.
                let size = e1.dot(e1) * e2.dot(e2) * ray.dir.dot(ray.dir);
                if det.abs() < 1e-14 {
                    let _ = size;
                    return None;
                }
                let s = ray.origin - a;
                let u = s.dot(p) / det;
                if !(0.0..=1.0).contains(&u) {
                    return None;
                }
                let q = s.cross(e1);
                let v = ray.dir.dot(q) / det;
                if !(v >= 0.0 && u + v <= 1.0) {
                    return None;
                }
                let t = e2.dot(q) / det;
                (t > tmin && t < tmax).then_some(t)
            }
        }
    }

    /// The unit normal at a point of the surface.
    pub fn normal_at(&self, point: Vec3) -> Vec3 {
        match *self {
            Shape::Sphere { center, radius } => ((point - center) / radius).unit(),
            Shape::Triangle { a, b, c } => (b - a).cross(c - a).unit(),
        }
    }
}

/// A shape with the number of its material.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prim {
    pub shape: Shape,
    pub material: usize,
}

/// The unbounded surface of the points `p` with `normal . p = offset`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    pub normal: Vec3,
    pub offset: f64,
    pub material: usize,
}

impl Plane {
    pub fn hit(&self, ray: &Ray, tmin: f64, tmax: f64) -> Option<f64> {
        let den = self.normal.dot(ray.dir);
        if den * den <= 1e-24 * self.normal.dot(self.normal) * ray.dir.dot(ray.dir) {
            return None;
        }
        let t = (self.offset - self.normal.dot(ray.origin)) / den;
        (t > tmin && t < tmax).then_some(t)
    }
}

// ---- the world ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Prim(usize),
    Plane(usize),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub t: f64,
    pub point: Vec3,
    /// The surface's own unit normal. It is not turned towards the ray.
    pub normal: Vec3,
    pub material: usize,
    pub surface: Surface,
}

/// Everything a ray can hit.
#[derive(Debug, Clone, Default)]
pub struct World {
    prims: Vec<Prim>,
    planes: Vec<Plane>,
}

impl World {
    pub fn new(prims: Vec<Prim>, planes: Vec<Plane>) -> World {
        World { prims, planes }
    }

    pub fn prims(&self) -> &[Prim] {
        &self.prims
    }

    pub fn planes(&self) -> &[Plane] {
        &self.planes
    }

    /// The nearest hit with `tmin < t < tmax`.
    pub fn closest(&self, ray: &Ray, tmin: f64, tmax: f64) -> Option<Hit> {
        let mut best: Option<(f64, Surface)> = None;
        for (i, prim) in self.prims.iter().enumerate() {
            if let Some(t) = prim.shape.hit(ray, tmin, best.map_or(tmax, |b| b.0)) {
                best = Some((t, Surface::Prim(i)));
            }
        }
        for (i, plane) in self.planes.iter().enumerate() {
            if let Some(t) = plane.hit(ray, tmin, best.map_or(tmax, |b| b.0)) {
                best = Some((t, Surface::Plane(i)));
            }
        }
        best.map(|(t, surface)| self.describe(ray, t, surface))
    }

    fn describe(&self, ray: &Ray, t: f64, surface: Surface) -> Hit {
        let point = ray.at(t);
        let (normal, material) = match surface {
            Surface::Prim(i) => (self.prims[i].shape.normal_at(point), self.prims[i].material),
            Surface::Plane(i) => (self.planes[i].normal.unit(), self.planes[i].material),
        };
        Hit {
            t,
            point,
            normal,
            material,
            surface,
        }
    }

    /// Whether anything at all is hit with `tmin < t < tmax`.
    pub fn blocked(&self, ray: &Ray, tmin: f64, tmax: f64) -> bool {
        self.prims
            .iter()
            .any(|p| p.shape.hit(ray, tmin, tmax).is_some())
            || self.planes.iter().any(|p| p.hit(ray, tmin, tmax).is_some())
    }
}
