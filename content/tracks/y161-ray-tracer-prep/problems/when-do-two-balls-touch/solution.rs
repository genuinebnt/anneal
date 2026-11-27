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
    // Seen from `a`, a point moves and must come within `reach` of the origin.
    let (px, py) = (b.x - a.x, b.y - a.y);
    let (vx, vy) = (b.vx - a.vx, b.vy - a.vy);
    let reach = a.radius + b.radius;
    if px.hypot(py) <= reach {
        return Some(0.0);
    }
    let speed2 = vx * vx + vy * vy;
    let toward = -(px * vx + py * vy);
    if speed2 == 0.0 || toward <= 0.0 {
        return None;
    }
    let closest = toward / speed2;
    // The offset at the closest moment, found without subtracting two huge squares.
    let (cx, cy) = (px + vx * closest, py + vy * closest);
    let spare = reach * reach - (cx * cx + cy * cy);
    if spare < 0.0 {
        return None;
    }
    Some((closest - (spare / speed2).sqrt()).max(0.0))
}
