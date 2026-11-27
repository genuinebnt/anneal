/// A point of the disk `x^2 + y^2 <= 1`, every area of it as likely as any other of the same size.
pub fn on_table(roll: &mut dyn FnMut() -> f64) -> (f64, f64) {
    // The disk within radius r has an area that grows with r squared.
    let radius = roll().sqrt();
    let angle = std::f64::consts::TAU * roll();
    (radius * angle.cos(), radius * angle.sin())
}

/// A direction `[x, y, z]` of length 1 with `z >= 0`, with a density in proportion to `z`.
pub fn toward_sky(roll: &mut dyn FnMut() -> f64) -> [f64; 3] {
    // An even spread over the disk, lifted straight up onto the dome.
    let v = [2.0 * roll() - 1.0, 2.0 * roll() - 1.0, roll()];
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-300);
    [v[0] / length, v[1] / length, v[2] / length]
}
