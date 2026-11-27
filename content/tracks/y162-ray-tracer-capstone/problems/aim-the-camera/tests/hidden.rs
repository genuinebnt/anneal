#[allow(unused_imports)]
use anneal_prelude::Rng;
use solution::*;

#[allow(dead_code)]
fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[allow(dead_code)]
fn vnear(a: Vec3, b: Vec3, tol: f64) -> bool {
    near(a.x, b.x, tol) && near(a.y, b.y, tol) && near(a.z, b.z, tol)
}

#[allow(dead_code)]
fn spec() -> CameraSpec {
    CameraSpec {
        position: v3(0.0, 0.0, 0.0),
        look_at: v3(0.0, 0.0, -1.0),
        up: v3(0.0, 1.0, 0.0),
        vfov_degrees: 90.0,
        aspect: 2.0,
        lens_diameter: 0.0,
        focus_distance: 1.0,
    }
}

/// A source of numbers in [0, 1).
#[allow(dead_code)]
fn unit(rng: &mut Rng) -> f64 {
    (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64
}

#[test]
fn a_tilted_up_is_straightened() {
    let tilted = CameraSpec {
        up: v3(0.0, 5.0, -4.0),
        ..spec()
    };
    let camera = Camera::new(&tilted).unwrap();
    check!(
        "still straight",
        vnear(
            camera.ray(0.5, 0.0, (0.0, 0.0)).dir,
            v3(0.0, 1.0, -1.0).unit(),
            1e-12
        ),
        true
    );
}

#[test]
fn looking_straight_down_with_another_up() {
    let down = CameraSpec {
        position: v3(0.0, 9.0, 0.0),
        look_at: v3(0.0, 0.0, 0.0),
        up: v3(0.0, 0.0, -1.0),
        ..spec()
    };
    let camera = Camera::new(&down).unwrap();
    check!(
        "middle",
        vnear(
            camera.ray(0.5, 0.5, (0.0, 0.0)).dir,
            v3(0.0, -1.0, 0.0),
            1e-12
        ),
        true
    );
    check!(
        "right edge is +x",
        camera.ray(1.0, 0.5, (0.0, 0.0)).dir.x > 0.5,
        true
    );
}

#[test]
fn more_things_that_are_not_cameras() {
    let nan = f64::NAN;
    let bad = [
        CameraSpec {
            up: v3(0.0, 0.0, 0.0),
            ..spec()
        },
        CameraSpec {
            up: v3(0.0, 0.0, 3.0),
            ..spec()
        },
        CameraSpec {
            up: v3(0.0, 1e-12, 1.0),
            ..spec()
        },
        CameraSpec {
            position: v3(nan, 0.0, 0.0),
            ..spec()
        },
        CameraSpec {
            look_at: v3(0.0, f64::INFINITY, -1.0),
            ..spec()
        },
        CameraSpec {
            vfov_degrees: 0.0,
            ..spec()
        },
        CameraSpec {
            vfov_degrees: 180.0,
            ..spec()
        },
        CameraSpec {
            vfov_degrees: nan,
            ..spec()
        },
        CameraSpec {
            aspect: 0.0,
            ..spec()
        },
        CameraSpec {
            aspect: -1.0,
            ..spec()
        },
        CameraSpec {
            aspect: f64::INFINITY,
            ..spec()
        },
        CameraSpec {
            lens_diameter: -0.1,
            ..spec()
        },
        CameraSpec {
            lens_diameter: nan,
            ..spec()
        },
        CameraSpec {
            focus_distance: 0.0,
            ..spec()
        },
        CameraSpec {
            focus_distance: f64::INFINITY,
            ..spec()
        },
    ];
    for s in bad {
        check!(format!("{s:?}"), Camera::new(&s).is_none(), true);
    }
    let short_up = CameraSpec {
        up: v3(0.0, 1e-12, 0.0),
        ..spec()
    };
    check!(
        "a short up that points the right way is fine",
        Camera::new(&short_up).is_some(),
        true
    );
}

#[test]
fn random_cameras() {
    let mut rng = Rng::new(1631);
    let mut tried = 0;
    for case in 0..500 {
        let mut c = || rng.int(-90, 90) as f64 / 10.0;
        let position = v3(c(), c(), c());
        let look_at = v3(c(), c(), c());
        let up = v3(c(), c(), c());
        let view = look_at - position;
        if view.length() < 1.0
            || view.unit().cross(up).length() < 0.1 * up.length()
            || up.length() < 0.5
        {
            continue;
        }
        tried += 1;
        let fov = rng.int(10, 150) as f64;
        let aspect = rng.int(3, 30) as f64 / 10.0;
        let focus = rng.int(1, 90) as f64 / 10.0;
        let diameter = rng.int(0, 20) as f64 / 10.0;
        let s = CameraSpec {
            position,
            look_at,
            up,
            vfov_degrees: fov,
            aspect,
            lens_diameter: diameter,
            focus_distance: focus,
        };
        let name = format!("case {case}: {s:?}");
        let camera = Camera::new(&s).unwrap();
        let forward = view.unit();
        let (across, down) = (
            rng.int(0, 100) as f64 / 100.0,
            rng.int(0, 100) as f64 / 100.0,
        );
        let pin = camera.ray(across, down, (0.0, 0.0));
        check!(
            name.clone(),
            vnear(pin.origin, position, 1e-9) && near(pin.dir.length(), 1.0, 1e-9),
            true
        );
        // On the plane at distance 1 ahead, the offsets from the middle are given by the angles.
        let ahead = pin.dir / pin.dir.dot(forward) - forward;
        let half = (fov.to_radians() / 2.0).tan();
        let right = forward.cross(up).unit();
        let upright = right.cross(forward);
        check!(
            name.clone(),
            near(ahead.dot(right), (2.0 * across - 1.0) * half * aspect, 1e-8),
            true
        );
        check!(
            name.clone(),
            near(ahead.dot(upright), (1.0 - 2.0 * down) * half, 1e-8),
            true
        );
        // Any lens point: it starts on the lens and passes through the same point of the sharp plane.
        let (a, r) = (
            rng.int(0, 628) as f64 / 100.0,
            rng.int(0, 100) as f64 / 100.0,
        );
        let lens = (r * a.cos(), r * a.sin());
        let through_lens = camera.ray(across, down, lens);
        let from = through_lens.origin - position;
        check!(
            name.clone(),
            vnear(
                from,
                (right * lens.0 + upright * lens.1) * (diameter / 2.0),
                1e-9
            ),
            true
        );
        let sharp = pin.at(focus / pin.dir.dot(forward));
        let reach = (sharp - through_lens.origin).dot(forward) / through_lens.dir.dot(forward);
        check!(
            name.clone(),
            vnear(through_lens.at(reach), sharp, 1e-7),
            true
        );
        check!(name, near(through_lens.dir.length(), 1.0, 1e-9), true);
    }
    check!("enough cases", tried > 250, true);
}

#[test]
fn square_counts_use_one_sample_per_cell() {
    for side in [1usize, 2, 4, 5, 8, 16] {
        for seed in 0..20 {
            let mut rng = Rng::new(seed * 31 + side as u64);
            let offsets = pixel_offsets(side * side, &mut || unit(&mut rng));
            let mut seen = vec![0; side * side];
            for &(x, y) in &offsets {
                check!(
                    format!("side {side}: inside the pixel ({x}, {y})"),
                    (0.0..1.0).contains(&x) && (0.0..1.0).contains(&y),
                    true
                );
                seen[(y * side as f64) as usize * side + (x * side as f64) as usize] += 1;
            }
            check!(
                format!("side {side}, seed {seed}"),
                seen.iter().all(|&n| n == 1),
                true
            );
        }
    }
}

#[test]
fn samples_move_inside_their_cells() {
    let mut rng = Rng::new(7);
    let offsets = pixel_offsets(1024, &mut || unit(&mut rng));
    // Where each sample sits inside its own cell, from 0 to 1.
    let inside: Vec<(f64, f64)> = offsets
        .iter()
        .map(|&(x, y)| ((x * 32.0).fract(), (y * 32.0).fract()))
        .collect();
    let left = inside.iter().filter(|p| p.0 < 0.5).count();
    let low = inside.iter().filter(|p| p.1 < 0.5).count();
    let below = inside.iter().filter(|p| p.0 < p.1).count();
    for (name, n) in [
        ("left half", left),
        ("upper half", low),
        ("one side of the diagonal", below),
    ] {
        check!(
            format!("{name}: about half of 1024, got {n}"),
            (410..=614).contains(&n),
            true
        );
    }
}

#[test]
fn other_counts() {
    let mut rng = Rng::new(9);
    check!(
        "no samples",
        pixel_offsets(0, &mut || unit(&mut rng)).len(),
        0
    );
    for n in [2usize, 3, 7, 50, 200] {
        let offsets = pixel_offsets(n, &mut || unit(&mut rng));
        check!(format!("{n}: how many"), offsets.len(), n);
        check!(
            format!("{n}: inside"),
            offsets
                .iter()
                .all(|&(x, y)| (0.0..1.0).contains(&x) && (0.0..1.0).contains(&y)),
            true
        );
        let mut xs: Vec<u64> = offsets.iter().map(|p| p.0.to_bits()).collect();
        xs.sort();
        xs.dedup();
        check!(format!("{n}: not all in one place"), xs.len() > n / 2, true);
    }
}

#[test]
fn the_disk_is_filled_evenly() {
    let mut rng = Rng::new(1633);
    let n = 40_000;
    let mut inner = 0;
    let mut sectors = [0u32; 8];
    for _ in 0..n {
        let (x, y) = unit_disk(&mut || unit(&mut rng));
        let r = (x * x + y * y).sqrt();
        check!(format!("({x}, {y}) is in the disk"), r <= 1.0 + 1e-12, true);
        if r <= 0.5 {
            inner += 1;
        }
        let turn = y.atan2(x) / std::f64::consts::TAU + 0.5;
        sectors[(turn * 8.0) as usize % 8] += 1;
    }
    // A quarter of the area lies within half the radius. The bounds are five standard deviations.
    let share = f64::from(inner) / f64::from(n);
    check!(
        format!("share within radius 0.5: {share}"),
        near(share, 0.25, 0.011),
        true
    );
    for (i, &count) in sectors.iter().enumerate() {
        let share = f64::from(count) / f64::from(n);
        check!(
            format!("sector {i}: {share}"),
            near(share, 0.125, 0.0083),
            true
        );
    }
}
