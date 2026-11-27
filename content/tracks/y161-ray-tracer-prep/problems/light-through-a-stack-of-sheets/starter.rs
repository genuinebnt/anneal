/// A flat sheet of clear material.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sheet {
    /// Index of refraction.
    pub index: f64,
    pub thickness: f64,
    /// The share of light lost per unit of length travelled is `1 - e^(-absorb)`.
    pub absorb: f64,
}

/// What becomes of the part of the beam that goes straight through.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Passage {
    /// How far along the faces the beam comes out from where it went in.
    pub shift: f64,
    /// The share of the light that comes out.
    pub share: f64,
}

/// Follows a beam that meets a stack of sheets at `angle` radians from the straight-on direction.
/// `outside` is the index of what surrounds the stack, on both sides.
pub fn pass_through(outside: f64, sheets: &[Sheet], angle: f64) -> Option<Passage> {
    todo!()
}
