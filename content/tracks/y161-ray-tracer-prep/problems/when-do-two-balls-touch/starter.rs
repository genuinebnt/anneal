/// A disc that moves in a straight line at a steady speed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ball {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub radius: f64,
}

/// The first time `t >= 0` at which the two discs touch or overlap.
pub fn first_touch(a: &Ball, b: &Ball) -> Option<f64> {
    todo!()
}
