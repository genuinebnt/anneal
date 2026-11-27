use std::ops::{Add, Div, Mul, Neg, Sub};

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
        todo!()
    }

    pub fn cross(self, o: Vec3) -> Vec3 {
        todo!()
    }

    pub fn length(self) -> f64 {
        todo!()
    }

    /// The same direction with length 1.
    pub fn unit(self) -> Vec3 {
        todo!()
    }

    /// Component by component product, used to filter a colour.
    pub fn times(self, o: Vec3) -> Vec3 {
        todo!()
    }

    /// Component 0, 1 or 2.
    pub fn axis(self, i: usize) -> f64 {
        todo!()
    }

    pub fn max_part(self) -> f64 {
        todo!()
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        todo!()
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        todo!()
    }
}

impl Mul<f64> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f64) -> Vec3 {
        todo!()
    }
}

impl Div<f64> for Vec3 {
    type Output = Vec3;
    fn div(self, s: f64) -> Vec3 {
        todo!()
    }
}

impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        todo!()
    }
}

/// The points `origin + t * dir`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    pub dir: Vec3,
}

impl Ray {
    pub fn at(&self, t: f64) -> Vec3 {
        todo!()
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
        todo!()
    }

    /// The unit normal at a point of the surface.
    pub fn normal_at(&self, point: Vec3) -> Vec3 {
        todo!()
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
        todo!()
    }
}

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
        todo!()
    }

    /// Whether anything at all is hit with `tmin < t < tmax`.
    pub fn blocked(&self, ray: &Ray, tmin: f64, tmax: f64) -> bool {
        todo!()
    }
}
