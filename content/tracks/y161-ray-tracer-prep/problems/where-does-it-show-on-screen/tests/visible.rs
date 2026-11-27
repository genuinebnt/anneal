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
fn straight_ahead_is_the_middle() {
    expect(
        "middle",
        viewer(),
        [0.0, 0.0, -3.0],
        Some((100.0, 50.0, 3.0)),
    );
}

#[test]
fn right_and_up() {
    // At depth 10 the picture reaches 10 up and down from the middle, and 20 to each side.
    expect(
        "right",
        viewer(),
        [10.0, 0.0, -10.0],
        Some((150.0, 50.0, 10.0)),
    );
    expect("up", viewer(), [0.0, 5.0, -10.0], Some((100.0, 25.0, 10.0)));
}

#[test]
fn depth_is_not_distance() {
    expect(
        "off to a corner",
        viewer(),
        [18.0, 9.0, -10.0],
        Some((190.0, 5.0, 10.0)),
    );
}

#[test]
fn behind_the_eye() {
    expect("behind", viewer(), [0.0, 0.0, 3.0], None);
    expect("behind and to the side", viewer(), [1.0, 1.0, 10.0], None);
}

#[test]
fn outside_the_picture() {
    expect("too far right", viewer(), [21.0, 0.0, -10.0], None);
    expect("too high", viewer(), [0.0, 11.0, -10.0], None);
}
