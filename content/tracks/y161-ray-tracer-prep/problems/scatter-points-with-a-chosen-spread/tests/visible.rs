#[allow(unused_imports)]
use anneal_prelude::Rng;
use solution::*;

#[allow(dead_code)]
const DRAWS: usize = 60_000;

#[allow(dead_code)]
fn unit(rng: &mut Rng) -> f64 {
    (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64
}

#[allow(dead_code)]
fn darts(seed: u64) -> Vec<(f64, f64)> {
    let mut rng = Rng::new(seed);
    (0..DRAWS)
        .map(|_| on_table(&mut || unit(&mut rng)))
        .collect()
}

#[allow(dead_code)]
fn dome(seed: u64) -> Vec<[f64; 3]> {
    let mut rng = Rng::new(seed);
    (0..DRAWS)
        .map(|_| toward_sky(&mut || unit(&mut rng)))
        .collect()
}

/// The measured share must be within five standard deviations of `want`.
#[allow(dead_code)]
fn about(name: &str, count: usize, want: f64) {
    let got = count as f64 / DRAWS as f64;
    let room = 5.0 * (want * (1.0 - want) / DRAWS as f64).sqrt();
    check!(
        format!("{name}: share {got}, want {want} +- {room}"),
        (got - want).abs() <= room,
        true
    );
}

#[test]
fn darts_stay_on_the_table() {
    check!(
        "all inside",
        darts(1).iter().all(|&(x, y)| x * x + y * y <= 1.0 + 1e-12),
        true
    );
}

#[test]
fn the_inner_half_gets_a_quarter() {
    let near = darts(2).iter().filter(|&&(x, y)| x.hypot(y) < 0.5).count();
    about("within radius 0.5", near, 0.25);
}

#[test]
fn left_and_right_alike() {
    about("x < 0", darts(3).iter().filter(|p| p.0 < 0.0).count(), 0.5);
}

#[test]
fn directions_have_length_one_and_point_up() {
    check!(
        "unit and up",
        dome(4)
            .iter()
            .all(|d| (d[0] * d[0] + d[1] * d[1] + d[2] * d[2] - 1.0).abs() < 1e-9 && d[2] >= 0.0),
        true
    );
}

#[test]
fn most_directions_are_steep() {
    // A quarter of the directions lie within 30 degrees of straight up: where z > cos 30.
    about(
        "within 30 degrees",
        dome(5).iter().filter(|d| d[2] > 0.75f64.sqrt()).count(),
        0.25,
    );
}
