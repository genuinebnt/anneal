use std::iter::Sum;
use std::ops::{Add, AddAssign, Mul, Neg, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Vec2 {
    pub x: i64,
    pub y: i64,
}

impl Add for Vec2 {
    type Output = Vec2;

    fn add(self, o: Vec2) -> Vec2 {
        Vec2 {
            x: self.x + o.x,
            y: self.y + o.y,
        }
    }
}

impl Sub for Vec2 {
    type Output = Vec2;

    fn sub(self, o: Vec2) -> Vec2 {
        Vec2 {
            x: self.x - o.x,
            y: self.y - o.y,
        }
    }
}

impl Neg for Vec2 {
    type Output = Vec2;

    fn neg(self) -> Vec2 {
        Vec2 {
            x: -self.x,
            y: -self.y,
        }
    }
}

impl AddAssign for Vec2 {
    fn add_assign(&mut self, o: Vec2) {
        self.x += o.x;
        self.y += o.y;
    }
}

/// Scaling: v * k.
impl Mul<i64> for Vec2 {
    type Output = Vec2;

    fn mul(self, k: i64) -> Vec2 {
        Vec2 {
            x: self.x * k,
            y: self.y * k,
        }
    }
}

/// Scaling: k * v. Allowed although i64 is foreign, because Vec2 is local.
impl Mul<Vec2> for i64 {
    type Output = Vec2;

    fn mul(self, v: Vec2) -> Vec2 {
        v * self
    }
}

/// Dot product.
impl Mul for Vec2 {
    type Output = i64;

    fn mul(self, o: Vec2) -> i64 {
        self.x * o.x + self.y * o.y
    }
}

impl Sum for Vec2 {
    fn sum<I: Iterator<Item = Vec2>>(iter: I) -> Vec2 {
        iter.fold(Vec2::default(), |a, b| a + b)
    }
}

impl<'a> Sum<&'a Vec2> for Vec2 {
    fn sum<I: Iterator<Item = &'a Vec2>>(iter: I) -> Vec2 {
        iter.copied().sum()
    }
}

/// The sum of all the points.
pub fn total(points: &[Vec2]) -> Vec2 {
    points.iter().sum()
}

/// Start at the origin and take each step `count` times.
pub fn walk(steps: &[(Vec2, i64)]) -> Vec2 {
    let mut p = Vec2::default();
    for &(d, count) in steps {
        p += d * count;
    }
    p
}
