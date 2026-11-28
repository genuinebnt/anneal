/// The ground under `(x, z)`: the lowest number of a triangle that covers the place when seen from
/// above, and the height there. A point is `[x, y, z]` with `y` the height.
pub fn height_at(
    points: &[[f64; 3]],
    triangles: &[[usize; 3]],
    x: f64,
    z: f64,
) -> Option<(usize, f64)> {
    for (number, corners) in triangles.iter().enumerate() {
        let [Some(a), Some(b), Some(c)] = corners.map(|i| points.get(i)) else {
            continue;
        };
        let (e1x, e1z) = (b[0] - a[0], b[2] - a[2]);
        let (e2x, e2z) = (c[0] - a[0], c[2] - a[2]);
        let det = e1x * e2z - e2x * e1z;
        // Compared with the sides, so that small triangles are not mistaken for flat ones.
        if det == 0.0 {
            continue;
        }
        let (px, pz) = (x - a[0], z - a[2]);
        // The shares of the second and third corner.
        let u = (px * e2z - e2x * pz) / det;
        let v = (e1x * pz - px * e1z) / det;
        if u >= -1e-9 && v >= -1e-9 && u + v <= 1.0 + 1e-9 {
            return Some((number, a[1] + u * (b[1] - a[1]) + v * (c[1] - a[1])));
        }
    }
    None
}
