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
fn every_ring() {
    let all = darts(1611);
    for k in 1..10 {
        let r = f64::from(k) / 10.0;
        about(
            &format!("within {r}"),
            all.iter().filter(|&&(x, y)| x.hypot(y) < r).count(),
            r * r,
        );
    }
}

#[test]
fn every_slice() {
    let all = darts(1612);
    for slice in 0..12 {
        let count = all
            .iter()
            .filter(|&&(x, y)| {
                ((y.atan2(x) / std::f64::consts::TAU + 0.5) * 12.0) as usize % 12 == slice
            })
            .count();
        about(&format!("slice {slice}"), count, 1.0 / 12.0);
    }
}

#[test]
fn distance_and_angle_are_not_tied() {
    let all = darts(1613);
    for (sx, sy) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
        let inner = all
            .iter()
            .filter(|&&(x, y)| {
                x * sx > 0.0 && y * sy > 0.0 && x.hypot(y) < std::f64::consts::FRAC_1_SQRT_2
            })
            .count();
        about("inner half of a quarter", inner, 0.125);
        let outer = all
            .iter()
            .filter(|&&(x, y)| {
                x * sx > 0.0 && y * sy > 0.0 && x.hypot(y) >= std::f64::consts::FRAC_1_SQRT_2
            })
            .count();
        about("outer half of a quarter", outer, 0.125);
    }
}

#[test]
fn small_squares_of_the_table() {
    let all = darts(1614);
    let half = 0.2;
    for (cx, cy) in [
        (0.0, 0.0),
        (0.5, 0.5),
        (-0.7, 0.1),
        (0.1, -0.7),
        (-0.5, -0.5),
    ] {
        let count = all
            .iter()
            .filter(|&&(x, y)| (x - cx).abs() < half / 2.0 && (y - cy).abs() < half / 2.0)
            .count();
        about(
            &format!("square at ({cx}, {cy})"),
            count,
            half * half / std::f64::consts::PI,
        );
    }
}

#[test]
fn every_height_on_the_dome() {
    let all = dome(1615);
    for k in 1..10 {
        let h = f64::from(k) / 10.0;
        about(
            &format!("higher than {h}"),
            all.iter().filter(|d| d[2] > h).count(),
            1.0 - h * h,
        );
    }
}

#[test]
fn the_dome_has_no_favourite_side() {
    let all = dome(1616);
    for slice in 0..8 {
        let count = all
            .iter()
            .filter(|d| {
                ((d[1].atan2(d[0]) / std::f64::consts::TAU + 0.5) * 8.0) as usize % 8 == slice
            })
            .count();
        about(&format!("slice {slice}"), count, 0.125);
    }
}

#[test]
fn height_and_side_are_not_tied() {
    let all = dome(1617);
    about(
        "steep and in front",
        all.iter()
            .filter(|d| d[2] > 0.5f64.sqrt() && d[0] > 0.0)
            .count(),
        0.25,
    );
    about(
        "flat and in front",
        all.iter()
            .filter(|d| d[2] <= 0.5f64.sqrt() && d[0] > 0.0)
            .count(),
        0.25,
    );
    about(
        "steep and to the left",
        all.iter()
            .filter(|d| d[2] > 0.5f64.sqrt() && d[1] > 0.0)
            .count(),
        0.25,
    );
}

#[test]
fn averages() {
    let all = dome(1618);
    let n = DRAWS as f64;
    let mean = |f: &dyn Fn(&[f64; 3]) -> f64| all.iter().map(f).sum::<f64>() / n;
    check!(
        format!("mean z {}", mean(&|d| d[2])),
        (mean(&|d| d[2]) - 2.0 / 3.0).abs() < 0.005,
        true
    );
    check!("mean x", mean(&|d| d[0]).abs() < 0.011, true);
    check!("mean y", mean(&|d| d[1]).abs() < 0.011, true);
    check!(
        "mean x squared",
        (mean(&|d| d[0] * d[0]) - 0.25).abs() < 0.006,
        true
    );
}

#[test]
fn the_ends_of_the_rolls() {
    // Rolls near 0 and near 1 must give sound points too.
    let mut k = 0u64;
    let edges = [
        0.0,
        1.0 - 1.0 / (1u64 << 53) as f64,
        0.5,
        0.25,
        0.9999999,
        1e-17,
        0.75,
        0.1,
        0.6,
    ];
    let mut rng = Rng::new(1619);
    for _ in 0..3000 {
        let mut roll = || {
            k += 1;
            if k.is_multiple_of(2) {
                edges[rng.below(edges.len())]
            } else {
                unit(&mut rng)
            }
        };
        let (x, y) = on_table(&mut roll);
        check!(
            format!("dart ({x}, {y})"),
            x * x + y * y <= 1.0 + 1e-12,
            true
        );
        let d = toward_sky(&mut roll);
        check!(
            format!("direction {d:?}"),
            (d.iter().map(|v| v * v).sum::<f64>() - 1.0).abs() < 1e-9 && d[2] >= 0.0,
            true
        );
    }
}
