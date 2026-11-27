/// A thin wall from `a` to `b` on the floor plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wall {
    pub a: (f64, f64),
    pub b: (f64, f64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lamp {
    pub at: (f64, f64),
    pub power: f64,
}

/// The light that falls on a spot at `spot` which faces along `facing`.
pub fn brightness(spot: (f64, f64), facing: (f64, f64), lamps: &[Lamp], walls: &[Wall]) -> f64 {
    todo!()
}
