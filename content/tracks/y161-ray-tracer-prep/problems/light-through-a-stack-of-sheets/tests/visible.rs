#[allow(unused_imports)]
use anneal_prelude::Rng;
use solution::*;

#[allow(dead_code)]
fn sheet(index: f64, thickness: f64, absorb: f64) -> Sheet {
    Sheet {
        index,
        thickness,
        absorb,
    }
}

#[allow(dead_code)]
fn expect(name: &str, got: Option<Passage>, shift: f64, share: f64) {
    let ok = got.is_some_and(|p| {
        (p.shift - shift).abs() <= 1e-9 * shift.max(1.0) && (p.share - share).abs() <= 1e-9
    });
    check!(
        format!("{name}: got {got:?}, want shift {shift}, share {share}"),
        ok,
        true
    );
}

#[test]
fn nothing_in_the_way() {
    expect("no sheets", pass_through(1.0, &[], 0.7), 0.0, 1.0);
}

#[test]
fn straight_through_a_window() {
    // Head on, each face reflects 4 percent.
    expect(
        "a pane",
        pass_through(1.0, &[sheet(1.5, 2.0, 0.0)], 0.0),
        0.0,
        0.96 * 0.96,
    );
}

#[test]
fn tinted_glass_head_on() {
    expect(
        "loses with thickness",
        pass_through(1.0, &[sheet(1.5, 2.0, 0.25)], 0.0),
        0.0,
        0.96 * 0.96 * (-0.5f64).exp(),
    );
}

#[test]
fn at_an_angle_the_beam_moves_sideways() {
    // sin(angle) = 0.9 and index 1.5: inside the sine is 0.6 and the cosine 0.8.
    let got = pass_through(1.0, &[sheet(1.5, 4.0, 0.0)], 0.9f64.asin()).unwrap();
    check!(
        format!("shift 3, got {}", got.shift),
        (got.shift - 3.0).abs() < 1e-9,
        true
    );
}

#[test]
fn trapped() {
    // From water into air at a flat angle, nothing gets through.
    check!(
        "no passage",
        pass_through(1.33, &[sheet(1.0, 1.0, 0.0)], 1.2),
        None
    );
}
