/// A straight wall: the points with `nx * x + ny * y = offset`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wall {
    pub nx: f64,
    pub ny: f64,
    pub offset: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounce {
    pub wall: usize,
    pub x: f64,
    pub y: f64,
}

/// The first `count` bounces of a ball that starts strictly inside a convex room.
pub fn bounces(
    walls: &[Wall],
    start: (f64, f64),
    velocity: (f64, f64),
    count: usize,
) -> Vec<Bounce> {
    todo!()
}
