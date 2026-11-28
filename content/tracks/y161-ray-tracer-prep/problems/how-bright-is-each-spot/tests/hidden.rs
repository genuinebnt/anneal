#[allow(unused_imports)]
use anneal_prelude::Rng;
use solution::*;

#[allow(dead_code)]
fn lamp(x: f64, y: f64, power: f64) -> Lamp {
    Lamp { at: (x, y), power }
}

#[allow(dead_code)]
fn wall(ax: f64, ay: f64, bx: f64, by: f64) -> Wall {
    Wall {
        a: (ax, ay),
        b: (bx, by),
    }
}

#[allow(dead_code)]
fn expect(name: &str, got: f64, want: f64) {
    check!(
        format!("{name}: got {got}, want {want}"),
        (got - want).abs() <= 1e-9 * want.max(1.0),
        true
    );
}

#[test]
fn the_length_of_facing_does_not_matter() {
    for size in [0.001, 1.0, 250.0] {
        expect(
            &format!("facing of length {size}"),
            brightness((0.0, 0.0), (0.0, size), &[lamp(3.0, 4.0, 125.0)], &[]),
            4.0,
        );
    }
}

#[test]
fn edge_on_light_adds_nothing() {
    expect(
        "sideways",
        brightness((0.0, 0.0), (0.0, 1.0), &[lamp(7.0, 0.0, 99.0)], &[]),
        0.0,
    );
}

#[test]
fn walls_that_do_not_reach() {
    let lamps = [lamp(0.0, 4.0, 16.0)];
    for w in [
        wall(0.5, 2.0, 3.0, 2.0),
        wall(-3.0, 2.0, -0.5, 2.0),
        wall(1.0, 0.0, 1.0, 5.0),
        wall(-1.0, -1.0, 1.0, -1.0),
        wall(-1.0, 4.5, 1.0, 4.5),
    ] {
        expect(
            &format!("{w:?}"),
            brightness((0.0, 0.0), (0.0, 1.0), &lamps, &[w]),
            1.0,
        );
    }
}

#[test]
fn walls_along_the_way_never_block() {
    let lamps = [lamp(0.0, 4.0, 16.0)];
    expect(
        "lying along the way",
        brightness((0.0, 0.0), (0.0, 1.0), &lamps, &[wall(0.0, 1.0, 0.0, 2.0)]),
        1.0,
    );
    expect(
        "beside it",
        brightness((0.0, 0.0), (0.0, 1.0), &lamps, &[wall(1.0, 1.0, 1.0, 2.0)]),
        1.0,
    );
    expect(
        "a wall of no length",
        brightness((0.0, 0.0), (0.0, 1.0), &lamps, &[wall(3.0, 3.0, 3.0, 3.0)]),
        1.0,
    );
}

#[test]
fn a_lamp_on_the_spot() {
    expect(
        "adds nothing",
        brightness(
            (1.0, 1.0),
            (0.0, 1.0),
            &[lamp(1.0, 1.0, 5.0), lamp(1.0, 2.0, 2.0)],
            &[],
        ),
        2.0,
    );
    expect(
        "no lamps",
        brightness((1.0, 1.0), (0.0, 1.0), &[], &[]),
        0.0,
    );
}

#[test]
fn spots_worked_out_along_slanted_walls() {
    // The spot is computed on the wall, so rounding puts it a hair to either side.
    let mut rng = Rng::new(1625);
    for case in 0..3000 {
        let mut c = || rng.int(-90, 90) as f64 / 7.0;
        let w = wall(c(), c(), c(), c());
        let (dx, dy) = (w.b.0 - w.a.0, w.b.1 - w.a.1);
        if dx.hypot(dy) < 1.0 {
            continue;
        }
        let share = rng.int(1, 12) as f64 / 13.0;
        let spot = (w.a.0 + dx * share, w.a.1 + dy * share);
        let facing = (-dy, dx);
        let out = rng.int(1, 40) as f64 / 10.0;
        let aside = rng.int(-30, 30) as f64 / 10.0;
        let at = (
            spot.0 + (facing.0 * out + dx * aside) / dx.hypot(dy),
            spot.1 + (facing.1 * out + dy * aside) / dx.hypot(dy),
        );
        let lamps = [Lamp { at, power: 10.0 }];
        let got = brightness(spot, facing, &lamps, &[w]);
        check!(
            format!("case {case}: spot {spot:?} on {w:?}, lamp {at:?}: {got}"),
            got > 0.0,
            true
        );
    }
}

#[test]
fn very_close_walls_still_block() {
    let lamps = [lamp(0.0, 2.0, 8.0)];
    // 0.0015 from the spot or the lamp: farther than 0.001, so it must block.
    for y in [0.0015, 1.9985] {
        expect(
            &format!("wall at {y}"),
            brightness((0.0, 0.0), (0.0, 1.0), &lamps, &[wall(-1.0, y, 1.0, y)]),
            0.0,
        );
    }
}

/// Which side of the line through `a` and `b` the point is on.
#[allow(dead_code)]
fn side(a: (f64, f64), b: (f64, f64), p: (f64, f64)) -> f64 {
    ((b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)) / (b.0 - a.0).hypot(b.1 - a.1)
}

#[test]
fn random_rooms() {
    let mut rng = Rng::new(1626);
    let (mut tried, mut shaded) = (0, 0);
    for case in 0..3000 {
        let mut c = || rng.int(-100, 100) as f64 / 10.0 + 0.0137;
        let spot = (c(), c() + 0.0059);
        let facing = (c(), c() + 0.0071);
        let lamps: Vec<Lamp> = (0..rng.int(1, 4))
            .map(|_| Lamp {
                at: (
                    rng.int(-100, 100) as f64 / 10.0 + 0.0311,
                    rng.int(-100, 100) as f64 / 10.0,
                ),
                power: rng.int(1, 90) as f64,
            })
            .collect();
        let walls: Vec<Wall> = (0..rng.int(0, 5))
            .map(|_| {
                let mut c = || rng.int(-100, 100) as f64 / 10.0;
                Wall {
                    a: (c(), c()),
                    b: (c() + 0.0173, c()),
                }
            })
            .collect();
        let mut want = 0.0;
        let mut doubtful = false;
        for l in &lamps {
            let to = (l.at.0 - spot.0, l.at.1 - spot.1);
            let d = to.0.hypot(to.1);
            let cos = (to.0 * facing.0 + to.1 * facing.1) / (d * facing.0.hypot(facing.1));
            let mut blocked = false;
            for w in &walls {
                // Two segments cross when each has the ends of the other on both sides.
                let s = [
                    side(w.a, w.b, spot),
                    side(w.a, w.b, l.at),
                    side(spot, l.at, w.a),
                    side(spot, l.at, w.b),
                ];
                doubtful |= s.iter().any(|v| v.abs() < 1e-4);
                if s[0] * s[1] < 0.0 && s[2] * s[3] < 0.0 {
                    blocked = true;
                }
            }
            if cos > 0.0 {
                if blocked {
                    shaded += 1;
                } else {
                    want += l.power * cos / (d * d);
                }
            }
        }
        if doubtful {
            continue;
        }
        tried += 1;
        expect(
            &format!("case {case}: {spot:?} {facing:?} {lamps:?} {walls:?}"),
            brightness(spot, facing, &lamps, &walls),
            want,
        );
    }
    check!(
        format!("enough cases ({tried}) with shade ({shaded})"),
        tried > 2500 && shaded > 300,
        true
    );
}

#[test]
fn close_to_a_near_lamp_does_not_block() {
    // The lamp is only 0.01 away. Walls 5e-8 from either end are within 1e-7 of it, so they do not block.
    let lamps = [lamp(0.0, 0.01, 1.0)];
    for y in [5e-8, 0.01 - 5e-8] {
        expect(
            &format!("wall at {y}"),
            brightness((0.0, 0.0), (0.0, 1.0), &lamps, &[wall(-1.0, y, 1.0, y)]),
            10_000.0,
        );
    }
}
