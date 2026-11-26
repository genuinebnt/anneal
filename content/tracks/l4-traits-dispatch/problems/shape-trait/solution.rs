use std::f64::consts::PI;

pub struct Circle {
    pub r: f64,
}

pub struct Rect {
    pub w: f64,
    pub h: f64,
}

/// Another trait with a `name` method. Don't change it.
pub trait Label {
    fn name(&self) -> String;
}

impl Label for Rect {
    fn name(&self) -> String {
        "box".to_string()
    }
}

pub trait Named {
    fn name(&self) -> String;
}

pub trait Shape: Named {
    /// Number of straight sides (0 for a circle).
    const SIDES: u32;

    fn area(&self) -> f64;
    fn perimeter(&self) -> f64;

    /// "<name>, <SIDES> sides, area <area to 2 decimals>".
    fn describe(&self) -> String {
        format!(
            "{}, {} sides, area {:.2}",
            self.name(),
            Self::SIDES,
            self.area()
        )
    }
}

impl Named for Circle {
    fn name(&self) -> String {
        "circle".to_string()
    }
}

impl Shape for Circle {
    const SIDES: u32 = 0;

    fn area(&self) -> f64 {
        PI * self.r * self.r
    }

    fn perimeter(&self) -> f64 {
        2.0 * PI * self.r
    }
}

impl Named for Rect {
    fn name(&self) -> String {
        "rect".to_string()
    }
}

impl Shape for Rect {
    const SIDES: u32 = 4;

    fn area(&self) -> f64 {
        self.w * self.h
    }

    fn perimeter(&self) -> f64 {
        2.0 * (self.w + self.h)
    }
}

/// Total number of sides across the shapes.
pub fn total_sides<S: Shape>(shapes: &[S]) -> u32 {
    S::SIDES * shapes.len() as u32
}

/// "<Named name>/<Label name>" for a rect: "rect/box".
pub fn both_names(r: &Rect) -> String {
    format!("{}/{}", Named::name(r), <Rect as Label>::name(r))
}
