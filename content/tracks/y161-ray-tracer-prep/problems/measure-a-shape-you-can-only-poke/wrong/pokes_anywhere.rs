/// A number in `[0, 1)` made from a raw 64-bit random value.
pub fn unit_from(raw: u64) -> f64 {
    (raw >> 11) as f64 / (1u64 << 53) as f64
}

/// The largest `side` that `covered_share` accepts.
pub const MAX_SIDE: usize = 2048;

/// Estimates which share of the unit square `inside` covers, with `side * side` pokes.
pub fn covered_share(
    side: usize,
    inside: &mut dyn FnMut(f64, f64) -> bool,
    raw: &mut dyn FnMut() -> u64,
) -> Option<f64> {
    if side == 0 || side > MAX_SIDE {
        return None;
    }
    let mut yes = 0usize;
    for row in 0..side {
        for column in 0..side {
            let _ = (row, column);
            let x = unit_from(raw());
            let y = unit_from(raw());
            yes += usize::from(inside(x, y));
        }
    }
    Some(yes as f64 / (side * side) as f64)
}
