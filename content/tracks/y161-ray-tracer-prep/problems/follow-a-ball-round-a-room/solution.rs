/// A straight wall: the points with `nx * x + ny * y = offset`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wall {
    pub nx: f64,
    pub ny: f64,
    pub offset: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounce {
    pub wall: usize,
    pub x: f64,
    pub y: f64,
}

/// The first `count` bounces of a ball that starts strictly inside a convex room.
pub fn bounces(
    walls: &[Wall],
    start: (f64, f64),
    velocity: (f64, f64),
    count: usize,
) -> Vec<Bounce> {
    // Which side of each wall is the room: 1 where `n . p - offset` is positive inside.
    let inside: Vec<f64> = walls
        .iter()
        .map(|w| (w.nx * start.0 + w.ny * start.1 - w.offset).signum())
        .collect();
    let (mut x, mut y) = start;
    let (mut vx, mut vy) = velocity;
    let mut found = Vec::new();
    while found.len() < count {
        let mut best: Option<(f64, usize)> = None;
        for (i, w) in walls.iter().enumerate() {
            let toward = w.nx * vx + w.ny * vy;
            // Only a wall the ball is getting closer to can be hit. This also rules out the wall
            // it has just left, however the rounding fell.
            if toward * inside[i] >= 0.0 {
                continue;
            }
            // A corner can leave the ball a hair beyond the second wall: that is distance zero.
            let t = ((w.offset - w.nx * x - w.ny * y) / toward).max(0.0);
            if best.is_none_or(|b| t < b.0) {
                best = Some((t, i));
            }
        }
        let Some((t, i)) = best else { break };
        x += vx * t;
        y += vy * t;
        let w = walls[i];
        let k = 2.0 * (w.nx * vx + w.ny * vy) / (w.nx * w.nx + w.ny * w.ny);
        vx -= k * w.nx;
        vy -= k * w.ny;
        found.push(Bounce { wall: i, x, y });
    }
    found
}
