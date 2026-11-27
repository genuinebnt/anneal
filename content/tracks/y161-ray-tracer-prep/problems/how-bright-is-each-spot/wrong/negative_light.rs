/// A thin wall from `a` to `b` on the floor plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wall {
    pub a: (f64, f64),
    pub b: (f64, f64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lamp {
    pub at: (f64, f64),
    pub power: f64,
}

/// Whether the wall cuts the way from `from` to `to`, away from both ends.
fn cuts(wall: &Wall, from: (f64, f64), to: (f64, f64)) -> bool {
    let d = (to.0 - from.0, to.1 - from.1);
    let e = (wall.b.0 - wall.a.0, wall.b.1 - wall.a.1);
    let den = d.0 * e.1 - d.1 * e.0;
    if den == 0.0 {
        return false;
    }
    let w = (wall.a.0 - from.0, wall.a.1 - from.1);
    let along_way = (w.0 * e.1 - w.1 * e.0) / den;
    let along_wall = (w.0 * d.1 - w.1 * d.0) / den;
    let margin = 1e-7 / d.0.hypot(d.1);
    along_way > margin && along_way < 1.0 - margin && (0.0..=1.0).contains(&along_wall)
}

/// The light that falls on a spot at `spot` which faces along `facing`.
pub fn brightness(spot: (f64, f64), facing: (f64, f64), lamps: &[Lamp], walls: &[Wall]) -> f64 {
    let size = facing.0.hypot(facing.1);
    let mut total = 0.0;
    for lamp in lamps {
        let to = (lamp.at.0 - spot.0, lamp.at.1 - spot.1);
        let distance = to.0.hypot(to.1);
        if distance == 0.0 {
            continue;
        }
        let cosine = (to.0 * facing.0 + to.1 * facing.1) / (distance * size);
        if walls.iter().any(|w| cuts(w, spot, lamp.at)) {
            continue;
        }
        total += lamp.power * cosine / (distance * distance);
    }
    total
}
