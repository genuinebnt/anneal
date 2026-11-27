#[allow(unused_imports)]
use anneal_prelude::Rng;
use solution::*;

#[allow(dead_code)]
fn expect(name: &str, got: Option<(usize, f64)>, want: Option<(usize, f64)>) {
    let ok = match (got, want) {
        (Some(g), Some(w)) => g.0 == w.0 && (g.1 - w.1).abs() < 1e-9,
        (g, w) => g == w,
    };
    check!(format!("{name}: got {got:?}, want {want:?}"), ok, true);
}

/// A square from (0, 0) to (4, 4) cut along a diagonal. The corner (4, 4) is raised to height 8.
#[allow(dead_code)]
fn tile() -> (Vec<[f64; 3]>, Vec<[usize; 3]>) {
    (
        vec![
            [0.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [4.0, 8.0, 4.0],
            [0.0, 0.0, 4.0],
        ],
        vec![[0, 1, 2], [0, 2, 3]],
    )
}

#[test]
fn corners_may_be_listed_either_way_round() {
    let (p, _) = tile();
    let turned = vec![[2, 1, 0], [3, 2, 0]];
    expect("first", height_at(&p, &turned, 3.0, 1.0), Some((0, 2.0)));
    expect("second", height_at(&p, &turned, 1.0, 3.0), Some((1, 2.0)));
}

#[test]
fn a_wall_or_a_sliver_covers_nothing() {
    // A wall standing on the line z = 0, then a triangle with three corners in a line, then real ground below.
    let p = vec![
        [0.0, 0.0, 0.0],
        [4.0, 0.0, 0.0],
        [2.0, 9.0, 0.0],
        [8.0, 1.0, 0.0],
        [0.0, 1.0, -4.0],
        [8.0, 1.0, -4.0],
        [0.0, 1.0, 4.0],
    ];
    let t = vec![[0, 1, 2], [0, 1, 3], [4, 5, 6]];
    expect(
        "on the line of the wall",
        height_at(&p, &t, 2.0, 0.0),
        Some((2, 1.0)),
    );
    expect(
        "only flat triangles",
        height_at(&p, &t[..2], 2.0, 0.0),
        None,
    );
    let same = vec![[0, 0, 0]];
    expect(
        "one point three times",
        height_at(&p, &same, 0.0, 0.0),
        None,
    );
}

#[test]
fn very_small_and_very_large_ground() {
    for size in [1e-7, 1e-3, 1e6] {
        let p = vec![[0.0, 5.0, 0.0], [size, 5.0, 0.0], [0.0, 5.0 + size, size]];
        let t = vec![[0, 1, 2]];
        let got = height_at(&p, &t, 0.25 * size, 0.25 * size);
        expect(&format!("size {size}"), got, Some((0, 5.0 + 0.25 * size)));
        expect(
            &format!("size {size}, outside"),
            height_at(&p, &t, 0.75 * size, 0.75 * size),
            None,
        );
    }
}

#[test]
fn edges_that_do_not_divide_evenly() {
    // Sides of length 3 give shares such as 1/3 and 2/3, which have no exact form.
    for (ox, oz) in [(0.0, 0.0), (7.0, -11.0), (1000.0, 3000.0)] {
        let p = vec![[ox, 0.0, oz], [ox + 3.0, 3.0, oz], [ox, 6.0, oz + 3.0]];
        let t = vec![[0, 1, 2]];
        for k in 0..=3 {
            let k = k as f64;
            expect(
                &format!("long side, step {k}"),
                height_at(&p, &t, ox + 3.0 - k, oz + k),
                Some((0, 3.0 + k)),
            );
            expect(
                &format!("bottom side, step {k}"),
                height_at(&p, &t, ox + k, oz),
                Some((0, k)),
            );
            expect(
                &format!("left side, step {k}"),
                height_at(&p, &t, ox, oz + k),
                Some((0, 2.0 * k)),
            );
        }
    }
}

#[test]
fn a_hair_outside_is_outside() {
    let (p, t) = tile();
    for (x, z) in [(-0.001, 2.0), (2.0, -0.001), (4.001, 2.0), (2.0, 4.001)] {
        expect(&format!("({x}, {z})"), height_at(&p, &t, x, z), None);
    }
}

#[test]
fn broken_input() {
    let (p, _) = tile();
    let t = vec![[0, 1, 99], [usize::MAX, 1, 2], [0, 1, 2]];
    expect(
        "missing points are skipped",
        height_at(&p, &t, 3.0, 1.0),
        Some((2, 2.0)),
    );
    expect("NaN place", height_at(&p, &t, f64::NAN, 1.0), None);
    let bad = vec![[f64::NAN, 0.0, 0.0], [4.0, 0.0, 0.0], [0.0, 0.0, 4.0]];
    expect("NaN corner", height_at(&bad, &[[0, 1, 2]], 1.0, 1.0), None);
}

#[test]
fn places_computed_on_an_edge() {
    // The place is worked out in floating point along one side, so rounding may put it on either side of the edge.
    let mut rng = Rng::new(1605);
    let mut tried = 0;
    for case in 0..3000 {
        let mut c = || rng.int(-90, 90) as f64 / 10.0;
        let p = vec![[c(), 1.0, c()], [c(), 1.0, c()], [c(), 1.0, c()]];
        let (a, b, cc) = (p[0], p[1], p[2]);
        let area = (b[0] - a[0]) * (cc[2] - a[2]) - (cc[0] - a[0]) * (b[2] - a[2]);
        if area.abs() < 2.0 {
            continue;
        }
        tried += 1;
        let side = rng.below(3);
        let share = rng.int(0, 70) as f64 / 70.0;
        let (from, to) = (p[side], p[(side + 1) % 3]);
        let x = from[0] + (to[0] - from[0]) * share;
        let z = from[2] + (to[2] - from[2]) * share;
        expect(
            &format!("case {case}: {p:?} at ({x}, {z})"),
            height_at(&p, &[[0, 1, 2]], x, z),
            Some((0, 1.0)),
        );
    }
    check!("enough cases", tried > 2000, true);
}

#[test]
fn rolling_hills() {
    let mut rng = Rng::new(1604);
    for case in 0..30 {
        // A grid of points whose places are jittered and whose heights are random.
        let n = 6usize;
        let mut points = Vec::new();
        for row in 0..=n {
            for column in 0..=n {
                let jitter = |rng: &mut Rng, k: usize| {
                    if k == 0 || k == n {
                        0.0
                    } else {
                        rng.int(-25, 25) as f64 / 100.0
                    }
                };
                let x = column as f64 + jitter(&mut rng, column);
                let z = row as f64 + jitter(&mut rng, row);
                points.push([x, rng.int(-9, 9) as f64, z]);
            }
        }
        let mut triangles = Vec::new();
        for row in 0..n {
            for column in 0..n {
                let a = row * (n + 1) + column;
                let (b, c, d) = (a + 1, a + n + 2, a + n + 1);
                triangles.push([a, b, c]);
                triangles.push([a, d, c]);
            }
        }
        rng.shuffle(&mut triangles);
        for _ in 0..200 {
            let x = rng.int(1, 599) as f64 / 100.0 + 0.001234;
            let z = rng.int(1, 599) as f64 / 100.0 + 0.000567;
            let got = height_at(&points, &triangles, x, z);
            let name = format!("case {case} at ({x}, {z}): {got:?}");
            check!(name.clone(), got.is_some(), true);
            let (number, y) = got.unwrap();
            // The signed areas against each side all have one sign exactly when the place is inside.
            let sides = |t: &[usize; 3]| {
                [0, 1, 2].map(|k| {
                    let (p, q) = (points[t[k]], points[t[(k + 1) % 3]]);
                    (q[0] - p[0]) * (z - p[2]) - (q[2] - p[2]) * (x - p[0])
                })
            };
            let covers = |s: [f64; 3], slack: f64| {
                s.iter().all(|&v| v >= -slack) || s.iter().all(|&v| v <= slack)
            };
            check!(name.clone(), covers(sides(&triangles[number]), 1e-6), true);
            check!(
                name.clone(),
                triangles[..number].iter().all(|t| !covers(sides(t), -1e-6)),
                true
            );
            // The place and its height lie in the plane of the three corners.
            let [a, b, c] = triangles[number].map(|i| points[i]);
            let (u, v, w) = (
                [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
                [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
                [x - a[0], y - a[1], z - a[2]],
            );
            let volume = w[0] * (u[1] * v[2] - u[2] * v[1])
                + w[1] * (u[2] * v[0] - u[0] * v[2])
                + w[2] * (u[0] * v[1] - u[1] * v[0]);
            check!(name, volume.abs() < 1e-8, true);
        }
    }
}
