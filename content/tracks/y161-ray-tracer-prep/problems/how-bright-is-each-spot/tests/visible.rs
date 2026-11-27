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
fn one_lamp_straight_ahead() {
    expect(
        "power over distance squared",
        brightness((0.0, 0.0), (0.0, 1.0), &[lamp(0.0, 2.0, 8.0)], &[]),
        2.0,
    );
}

#[test]
fn at_a_slant() {
    // Distance 5 and cosine 0.8.
    expect(
        "slant",
        brightness((0.0, 0.0), (0.0, 1.0), &[lamp(3.0, 4.0, 125.0)], &[]),
        4.0,
    );
}

#[test]
fn a_lamp_behind_adds_nothing() {
    expect(
        "behind",
        brightness(
            (0.0, 0.0),
            (0.0, 1.0),
            &[lamp(0.0, -2.0, 8.0), lamp(0.0, 1.0, 3.0)],
            &[],
        ),
        3.0,
    );
}

#[test]
fn a_wall_in_the_way() {
    let blocker = [wall(-1.0, 1.0, 1.0, 1.0)];
    expect(
        "blocked",
        brightness((0.0, 0.0), (0.0, 1.0), &[lamp(0.0, 2.0, 8.0)], &blocker),
        0.0,
    );
    expect(
        "beyond the lamp, no matter",
        brightness((0.0, 0.0), (0.0, 1.0), &[lamp(0.0, 0.5, 1.0)], &blocker),
        4.0,
    );
}

#[test]
fn a_seat_on_a_wall_is_not_in_its_own_shade() {
    let floor = [wall(-5.0, 0.0, 5.0, 0.0)];
    expect(
        "lit",
        brightness((1.0, 0.0), (0.0, 1.0), &[lamp(1.0, 2.0, 4.0)], &floor),
        1.0,
    );
}
