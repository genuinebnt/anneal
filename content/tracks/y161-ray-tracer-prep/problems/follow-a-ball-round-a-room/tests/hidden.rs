#[allow(unused_imports)]
use anneal_prelude::Rng;
use solution::*;

#[allow(dead_code)]
fn wall(nx: f64, ny: f64, offset: f64) -> Wall {
    Wall { nx, ny, offset }
}

/// The square from -1 to 1: right, top, left, bottom.
#[allow(dead_code)]
fn square() -> Vec<Wall> {
    vec![
        wall(1.0, 0.0, 1.0),
        wall(0.0, 1.0, 1.0),
        wall(1.0, 0.0, -1.0),
        wall(0.0, 1.0, -1.0),
    ]
}

#[allow(dead_code)]
fn same(got: &[Bounce], want: &[(usize, f64, f64)]) -> bool {
    got.len() == want.len()
        && got
            .iter()
            .zip(want)
            .all(|(g, w)| g.wall == w.0 && (g.x - w.1).abs() < 1e-9 && (g.y - w.2).abs() < 1e-9)
}

#[allow(dead_code)]
fn expect(name: &str, got: Vec<Bounce>, want: &[(usize, f64, f64)]) {
    check!(
        format!("{name}: got {got:?}, want {want:?}"),
        same(&got, want),
        true
    );
}

#[test]
fn the_wall_may_be_written_any_way() {
    // The same square with normals of other lengths and signs.
    let walls = vec![
        wall(-4.0, 0.0, -4.0),
        wall(0.0, 0.5, 0.5),
        wall(-1.0, 0.0, 1.0),
        wall(0.0, -7.0, 7.0),
    ];
    expect(
        "up and right",
        bounces(&walls, (0.0, 0.0), (2.0, 1.0), 3),
        &[(0, 1.0, 0.5), (1, 0.0, 1.0), (2, -1.0, 0.5)],
    );
}

#[test]
fn a_slanted_wall() {
    // x + y = 2 closes a triangle with the two axes.
    let walls = vec![
        wall(1.0, 1.0, 2.0),
        wall(1.0, 0.0, 0.0),
        wall(0.0, 1.0, 0.0),
    ];
    expect(
        "off the slope",
        bounces(&walls, (0.5, 0.5), (1.0, 0.0), 2),
        &[(0, 1.5, 0.5), (2, 1.5, 0.0)],
    );
}

#[test]
fn the_speed_does_not_matter() {
    for speed in [1e-6, 1.0, 1e6] {
        expect(
            &format!("speed {speed}"),
            bounces(&square(), (0.0, 0.0), (2.0 * speed, speed), 2),
            &[(0, 1.0, 0.5), (1, 0.0, 1.0)],
        );
    }
}

#[test]
fn two_walls_at_one_corner_in_either_order() {
    let walls = vec![
        wall(0.0, 1.0, 1.0),
        wall(1.0, 0.0, 1.0),
        wall(1.0, 0.0, -1.0),
        wall(0.0, 1.0, -1.0),
    ];
    expect(
        "the lower number first",
        bounces(&walls, (0.5, 0.5), (0.5, 0.5), 2),
        &[(0, 1.0, 1.0), (1, 1.0, 1.0)],
    );
}

/// Replays the rules on the answer: every bounce on its wall, inside the room, on the path that the bounces before allow.
#[allow(dead_code)]
fn follow(
    name: &str,
    walls: &[Wall],
    inner: (f64, f64),
    start: (f64, f64),
    velocity: (f64, f64),
    got: &[Bounce],
) {
    let side: Vec<f64> = walls
        .iter()
        .map(|w| (w.nx * inner.0 + w.ny * inner.1 - w.offset).signum())
        .collect();
    let (mut at, mut v) = (start, velocity);
    for (n, b) in got.iter().enumerate() {
        let w = walls[b.wall];
        let length = w.nx.hypot(w.ny);
        let off = (w.nx * b.x + w.ny * b.y - w.offset) / length;
        check!(
            format!("{name}, bounce {n} {b:?}: on its wall (off by {off})"),
            off.abs() < 1e-7,
            true
        );
        for (i, other) in walls.iter().enumerate() {
            let depth = side[i] * (other.nx * b.x + other.ny * b.y - other.offset)
                / other.nx.hypot(other.ny);
            check!(
                format!("{name}, bounce {n}: inside wall {i} ({depth})"),
                depth > -1e-7,
                true
            );
        }
        // It lies ahead on the straight line from the last point.
        let speed = v.0.hypot(v.1);
        let step = (b.x - at.0, b.y - at.1);
        let along = (step.0 * v.0 + step.1 * v.1) / speed;
        let aside = (step.0 * v.1 - step.1 * v.0) / speed;
        check!(
            format!("{name}, bounce {n}: straight ahead (aside {aside}, along {along})"),
            aside.abs() < 1e-7 && along > -1e-7,
            true
        );
        let toward = side[b.wall] * (w.nx * v.0 + w.ny * v.1);
        check!(
            format!("{name}, bounce {n}: the ball was heading into that wall"),
            toward < 0.0,
            true
        );
        let k = 2.0 * (w.nx * v.0 + w.ny * v.1) / (length * length);
        v = (v.0 - k * w.nx, v.1 - k * w.ny);
        at = (b.x, b.y);
    }
}

#[test]
fn random_rooms() {
    let mut rng = Rng::new(1601);
    for case in 0..300 {
        // A convex room: walls touching a circle at random angles, with corners.
        let sides = rng.int(3, 9) as usize;
        let walls: Vec<Wall> = (0..sides)
            .map(|k| {
                let angle = std::f64::consts::TAU * (k as f64 + rng.int(0, 60) as f64 / 100.0)
                    / sides as f64;
                let radius = rng.int(2, 9) as f64;
                let scale = rng.int(1, 50) as f64 / 10.0 * if rng.bool() { 1.0 } else { -1.0 };
                wall(angle.cos() * scale, angle.sin() * scale, radius * scale)
            })
            .collect();
        let start = (
            rng.int(-70, 70) as f64 / 100.0,
            rng.int(-70, 70) as f64 / 100.0,
        );
        let velocity = (
            rng.int(-90, 90) as f64 + 0.37,
            rng.int(-90, 90) as f64 + 0.11,
        );
        let got = bounces(&walls, start, velocity, 60);
        let name = format!("case {case}: walls {walls:?} start {start:?} velocity {velocity:?}");
        if sides >= 4 {
            check!(format!("{name}: a closed room never ends"), got.len(), 60);
        }
        follow(&name, &walls, (0.0, 0.0), start, velocity, &got);
    }
}

#[test]
fn a_very_long_game() {
    let walls = vec![
        wall(1.0, 0.3, 5.0),
        wall(-0.2, 1.0, 4.0),
        wall(-1.0, 0.1, 3.0),
        wall(0.1, -1.0, 6.0),
    ];
    let got = bounces(&walls, (0.1, 0.2), (0.7771, 0.4533), 20_000);
    check!("all of them", got.len(), 20_000);
    follow(
        "long",
        &walls,
        (0.0, 0.0),
        (0.1, 0.2),
        (0.7771, 0.4533),
        &got[..200],
    );
    let last = got[19_999];
    check!(
        "still in the room",
        last.x.abs() < 10.0 && last.y.abs() < 10.0,
        true
    );
}

#[test]
fn a_room_far_from_zero() {
    // The square from 10 to 12 in both directions.
    let walls = vec![
        wall(1.0, 0.0, 12.0),
        wall(0.0, 1.0, 12.0),
        wall(1.0, 0.0, 10.0),
        wall(0.0, 1.0, 10.0),
    ];
    expect(
        "moved square",
        bounces(&walls, (11.0, 11.0), (2.0, 1.0), 3),
        &[(0, 12.0, 11.5), (1, 11.0, 12.0), (2, 10.0, 11.5)],
    );
}

#[test]
fn no_walls() {
    check!(
        "flies away",
        bounces(&[], (0.0, 0.0), (1.0, 1.0), 3),
        Vec::new()
    );
}
