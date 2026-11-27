/// One random estimate of all the light that comes from a row of rooms, entered at room 0.
/// `room(i)` gives `(glow, passes)` for room `i`, or `None` after the last one.
pub fn one_estimate(
    room: &mut dyn FnMut(usize) -> Option<(f64, f64)>,
    roll: &mut dyn FnMut() -> f64,
) -> f64 {
    todo!()
}
