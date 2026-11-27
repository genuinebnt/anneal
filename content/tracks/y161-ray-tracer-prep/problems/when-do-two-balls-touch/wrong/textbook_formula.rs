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
    let disc = toward * toward - speed2 * (px * px + py * py - reach * reach);
    if disc < 0.0 {
        return None;
    }
    Some(((toward - disc.sqrt()) / speed2).max(0.0))
}
