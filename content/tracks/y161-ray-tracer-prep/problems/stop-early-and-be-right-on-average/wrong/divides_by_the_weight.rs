/// One random estimate of all the light that comes from a row of rooms, entered at room 0.
/// `room(i)` gives `(glow, passes)` for room `i`, or `None` after the last one.
pub fn one_estimate(
    room: &mut dyn FnMut(usize) -> Option<(f64, f64)>,
    roll: &mut dyn FnMut() -> f64,
) -> f64 {
    let mut total = 0.0;
    // What one unit of light from the next room is worth.
    let mut worth = 1.0;
    let mut i = 0;
    while let Some((glow, passes)) = room(i) {
        total += worth * glow;
        worth *= passes;
        if i >= 2 {
            let chance = f64::clamp(worth, 0.05, 0.95);
            if roll() >= chance {
                break;
            }
            // Those who go on also stand for those who stopped here.
            worth /= worth.max(0.05);
        }
        i += 1;
    }
    total
}
