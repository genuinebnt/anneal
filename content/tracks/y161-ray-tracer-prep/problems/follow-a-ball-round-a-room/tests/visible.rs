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
fn straight_to_and_fro() {
    expect(
        "along x",
        bounces(&square(), (0.0, 0.5), (1.0, 0.0), 3),
        &[(0, 1.0, 0.5), (2, -1.0, 0.5), (0, 1.0, 0.5)],
    );
}

#[test]
fn off_at_the_same_angle() {
    expect(
        "up and right",
        bounces(&square(), (0.0, 0.0), (2.0, 1.0), 2),
        &[(0, 1.0, 0.5), (1, 0.0, 1.0)],
    );
}

#[test]
fn into_a_corner() {
    expect(
        "the diagonal",
        bounces(&square(), (0.0, 0.0), (1.0, 1.0), 4),
        &[
            (0, 1.0, 1.0),
            (1, 1.0, 1.0),
            (2, -1.0, -1.0),
            (3, -1.0, -1.0),
        ],
    );
}

#[test]
fn nothing_asked_nothing_given() {
    check!(
        "count 0",
        bounces(&square(), (0.0, 0.0), (1.0, 0.3), 0),
        Vec::new()
    );
    check!(
        "standing still",
        bounces(&square(), (0.0, 0.0), (0.0, 0.0), 5),
        Vec::new()
    );
}

#[test]
fn a_room_with_a_side_missing() {
    let walls = vec![
        wall(1.0, 0.0, 1.0),
        wall(0.0, 1.0, 1.0),
        wall(0.0, 1.0, -1.0),
    ];
    expect(
        "out through the left",
        bounces(&walls, (0.0, 0.0), (1.0, 0.0), 10),
        &[(0, 1.0, 0.0)],
    );
}
