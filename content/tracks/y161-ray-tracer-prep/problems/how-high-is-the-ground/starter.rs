/// The ground under `(x, z)`: the lowest number of a triangle that covers the place when seen from
/// above, and the height there. A point is `[x, y, z]` with `y` the height.
pub fn height_at(
    points: &[[f64; 3]],
    triangles: &[[usize; 3]],
    x: f64,
    z: f64,
) -> Option<(usize, f64)> {
    todo!()
}
