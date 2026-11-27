#[allow(unused_imports)]
use anneal_prelude::Rng;
use solution::*;

#[allow(dead_code)]
fn viewer() -> Viewer {
    Viewer {
        eye: [0.0, 0.0, 0.0],
        target: [0.0, 0.0, -5.0],
        up: [0.0, 1.0, 0.0],
        vfov_degrees: 90.0,
        width: 200,
        height: 100,
    }
}

#[allow(dead_code)]
fn expect(name: &str, v: Viewer, point: [f64; 3], want: Option<(f64, f64, f64)>) {
    let got = on_screen(&v, point);
    let ok = match (got, want) {
        (Some(g), Some(w)) => {
            (g.across - w.0).abs() < 1e-7
                && (g.down - w.1).abs() < 1e-7
                && (g.depth - w.2).abs() < 1e-7
        }
        (g, w) => g.is_none() && w.is_none(),
    };
    check!(
        format!("{name}: {point:?} gave {got:?}, want {want:?}"),
        ok,
        true
    );
}

#[test]
fn a_tilted_up_is_straightened() {
    let v = Viewer {
        up: [0.0, 3.0, 2.5],
        ..viewer()
    };
    expect("middle", v, [0.0, 0.0, -3.0], Some((100.0, 50.0, 3.0)));
    expect("up", v, [0.0, 5.0, -10.0], Some((100.0, 25.0, 10.0)));
}

#[test]
fn looking_the_other_way() {
    let v = Viewer {
        target: [0.0, 0.0, 5.0],
        ..viewer()
    };
    expect(
        "what was left is now right",
        v,
        [-10.0, 0.0, 10.0],
        Some((150.0, 50.0, 10.0)),
    );
}

#[test]
fn tall_pictures() {
    let v = Viewer {
        width: 100,
        height: 400,
        vfov_degrees: 90.0,
        ..viewer()
    };
    expect(
        "right edge",
        v,
        [2.4, 0.0, -10.0],
        Some((98.0, 200.0, 10.0)),
    );
    expect("past it", v, [2.6, 0.0, -10.0], None);
    expect(
        "near the top",
        v,
        [0.0, 9.0, -10.0],
        Some((50.0, 20.0, 10.0)),
    );
}

#[test]
fn at_the_eye() {
    expect("the eye itself", viewer(), [0.0, 0.0, 0.0], None);
    expect("level with the eye", viewer(), [1.0, 0.0, 0.0], None);
}

#[test]
fn viewers_that_cannot_see() {
    let point = [0.0, 0.0, -3.0];
    let bad = [
        Viewer {
            width: 0,
            ..viewer()
        },
        Viewer {
            height: 0,
            ..viewer()
        },
        Viewer {
            target: [0.0, 0.0, 0.0],
            ..viewer()
        },
        Viewer {
            up: [0.0, 0.0, 0.0],
            ..viewer()
        },
        Viewer {
            up: [0.0, 0.0, -2.0],
            ..viewer()
        },
        Viewer {
            up: [0.0, 0.0, 1.0],
            ..viewer()
        },
        Viewer {
            vfov_degrees: 0.0,
            ..viewer()
        },
        Viewer {
            vfov_degrees: 180.0,
            ..viewer()
        },
        Viewer {
            vfov_degrees: 200.0,
            ..viewer()
        },
        Viewer {
            vfov_degrees: f64::NAN,
            ..viewer()
        },
        Viewer {
            eye: [f64::NAN, 0.0, 0.0],
            ..viewer()
        },
    ];
    for v in bad {
        check!(format!("{v:?}"), on_screen(&v, point), None);
    }
    check!(
        "a NaN point",
        on_screen(&viewer(), [f64::NAN, 0.0, -1.0]),
        None
    );
}

#[test]
fn just_inside_and_just_outside_each_edge() {
    for (x, y, shows) in [
        (19.99, 0.0, true),
        (20.01, 0.0, false),
        (-19.99, 0.0, true),
        (-20.01, 0.0, false),
        (0.0, 9.99, true),
        (0.0, 10.01, false),
        (0.0, -9.99, true),
        (0.0, -10.01, false),
    ] {
        check!(
            format!("({x}, {y}, -10)"),
            on_screen(&viewer(), [x, y, -10.0]).is_some(),
            shows
        );
    }
}

#[test]
fn only_the_direction_to_the_target_matters() {
    for far in [0.001, 1.0, 1000.0] {
        let v = Viewer {
            target: [0.0, 0.0, -far],
            up: [0.0, far, 0.0],
            ..viewer()
        };
        expect(
            &format!("target at {far}"),
            v,
            [10.0, 5.0, -10.0],
            Some((150.0, 25.0, 10.0)),
        );
    }
}

#[test]
fn random_viewers() {
    let mut rng = Rng::new(1606);
    let mut tried = 0;
    for case in 0..3000 {
        let mut c = || rng.int(-90, 90) as f64 / 10.0;
        let eye = [c(), c(), c()];
        let target = [c(), c(), c()];
        let up = [c(), c(), c()];
        let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let unit = |a: [f64; 3]| {
            let n = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
            a.map(|v| v / n)
        };
        let cross = |a: [f64; 3], b: [f64; 3]| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let long = |a: [f64; 3]| (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
        let view = sub(target, eye);
        if long(view) < 1.0 || long(up) < 1.0 || long(cross(unit(view), unit(up))) < 0.1 {
            continue;
        }
        tried += 1;
        let forward = unit(view);
        let right = unit(cross(forward, up));
        let upright = cross(right, forward);
        let v = Viewer {
            eye,
            target,
            up,
            vfov_degrees: rng.int(5, 160) as f64,
            width: rng.int(1, 4000) as u32,
            height: rng.int(1, 4000) as u32,
        };
        // Choose where it should show, then build the point there.
        let share_across = rng.int(-30, 130) as f64 / 100.0 + 0.0013;
        let share_down = rng.int(-30, 130) as f64 / 100.0 + 0.0017;
        let depth = rng.int(1, 500) as f64 / 10.0 * if rng.below(8) == 0 { -1.0 } else { 1.0 };
        let half = (v.vfov_degrees.to_radians() / 2.0).tan();
        let x = (2.0 * share_across - 1.0) * half * f64::from(v.width) / f64::from(v.height);
        let y = (1.0 - 2.0 * share_down) * half;
        let point =
            [0, 1, 2].map(|k| eye[k] + depth * (forward[k] + x * right[k] + y * upright[k]));
        let shows =
            depth > 0.0 && (0.0..=1.0).contains(&share_across) && (0.0..=1.0).contains(&share_down);
        let want = shows.then(|| {
            (
                share_across * f64::from(v.width),
                share_down * f64::from(v.height),
                depth,
            )
        });
        let got = on_screen(&v, point);
        let ok = match (got, want) {
            (Some(g), Some(w)) => {
                (g.across - w.0).abs() < 1e-6
                    && (g.down - w.1).abs() < 1e-6
                    && (g.depth - w.2).abs() < 1e-7
            }
            (g, w) => g.is_none() && w.is_none(),
        };
        check!(
            format!("case {case}: {v:?} point {point:?} gave {got:?}, want {want:?}"),
            ok,
            true
        );
    }
    check!("enough cases", tried > 2000, true);
}
