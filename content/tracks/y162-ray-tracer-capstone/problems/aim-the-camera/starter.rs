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
                if det * det <= 1e-24 * size {
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

// ---- meshes ----

/// The two triangles `(a, b, c)` and `(a, c, d)` of a flat four-cornered face.
pub fn quad(a: Vec3, b: Vec3, c: Vec3, d: Vec3, material: usize) -> [Prim; 2] {
    let triangle = |a, b, c| Prim {
        shape: Shape::Triangle { a, b, c },
        material,
    };
    [triangle(a, b, c), triangle(a, c, d)]
}

/// A box standing on `base` (the middle of its bottom face), turned by `yaw` about the upright axis.
pub fn cuboid(base: Vec3, size: Vec3, yaw: f64, material: usize) -> Vec<Prim> {
    let (sin, cos) = yaw.sin_cos();
    // Corner `i`: bit 0 picks the high x side, bit 1 the top, bit 2 the high z side.
    let corner = |i: usize| {
        let x = ((i & 1) as f64 - 0.5) * size.x;
        let y = ((i >> 1) & 1) as f64 * size.y;
        let z = (((i >> 2) & 1) as f64 - 0.5) * size.z;
        base + v3(x * cos + z * sin, y, z * cos - x * sin)
    };
    // Each face anticlockwise as seen from outside.
    let faces = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    faces
        .iter()
        .flat_map(|f| {
            quad(
                corner(f[0]),
                corner(f[1]),
                corner(f[2]),
                corner(f[3]),
                material,
            )
        })
        .collect()
}

/// The largest number of subdivision rounds `icosphere` accepts.
pub const MAX_LEVELS: u32 = 6;

/// A ball made of `20 * 4^levels` triangles. `None` for too many levels or a radius that is not a
/// positive number.
pub fn icosphere(center: Vec3, radius: f64, levels: u32, material: usize) -> Option<Vec<Prim>> {
    if levels > MAX_LEVELS || !(radius > 0.0 && radius.is_finite()) {
        return None;
    }
    let g = (1.0 + 5f64.sqrt()) / 2.0;
    let mut points: Vec<Vec3> = [
        (-1.0, g, 0.0),
        (1.0, g, 0.0),
        (-1.0, -g, 0.0),
        (1.0, -g, 0.0),
        (0.0, -1.0, g),
        (0.0, 1.0, g),
        (0.0, -1.0, -g),
        (0.0, 1.0, -g),
        (g, 0.0, -1.0),
        (g, 0.0, 1.0),
        (-g, 0.0, -1.0),
        (-g, 0.0, 1.0),
    ]
    .iter()
    .map(|&(x, y, z)| v3(x, y, z).unit())
    .collect();
    #[rustfmt::skip]
    let mut faces: Vec<[usize; 3]> = vec![
        [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11],
        [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
        [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9],
        [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
    ];
    for _ in 0..levels {
        let mut middles: std::collections::HashMap<(usize, usize), usize> = Default::default();
        let mut finer = Vec::with_capacity(faces.len() * 4);
        for [a, b, c] in faces {
            let [ab, bc, ca] = [(a, b), (b, c), (c, a)].map(|(p, q)| {
                *middles.entry((p.min(q), p.max(q))).or_insert_with(|| {
                    points.push((points[p] + points[q]).unit());
                    points.len() - 1
                })
            });
            finer.extend([[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]);
        }
        faces = finer;
    }
    let place = |i: usize| center + points[i] * radius;
    let triangle = |&[a, b, c]: &[usize; 3]| Prim {
        shape: Shape::Triangle {
            a: place(a),
            b: place(b),
            c: place(c),
        },
        material,
    };
    Some(faces.iter().map(triangle).collect())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraSpec {
    pub position: Vec3,
    pub look_at: Vec3,
    /// Which way is up in the picture, roughly.
    pub up: Vec3,
    /// The angle seen from the bottom edge of the picture to the top edge.
    pub vfov_degrees: f64,
    /// Width divided by height.
    pub aspect: f64,
    /// 0 for a pinhole.
    pub lens_diameter: f64,
    /// The distance, along the view direction, of the plane that stays sharp.
    pub focus_distance: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct Camera {}

impl Camera {
    /// `None` if the numbers do not describe a camera.
    pub fn new(spec: &CameraSpec) -> Option<Camera> {
        todo!()
    }

    /// The ray through picture position `(s, t)`: `s` runs from 0 (left) to 1 (right), `t` from 0
    /// (top) to 1 (bottom). `lens` is a point of the unit disk.
    pub fn ray(&self, s: f64, t: f64, lens: (f64, f64)) -> Ray {
        todo!()
    }
}

/// Where inside a pixel to take `samples` samples. `rng` returns numbers in `[0, 1)`.
pub fn pixel_offsets(samples: usize, rng: &mut dyn FnMut() -> f64) -> Vec<(f64, f64)> {
    todo!()
}

/// A point spread evenly over the disk of radius 1.
pub fn unit_disk(rng: &mut dyn FnMut() -> f64) -> (f64, f64) {
    todo!()
}
