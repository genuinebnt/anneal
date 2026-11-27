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
fn the_loss_follows_the_longer_path() {
    let got = pass_through(1.0, &[sheet(1.5, 4.0, 0.1)], 0.9f64.asin()).unwrap();
    let clear = pass_through(1.0, &[sheet(1.5, 4.0, 0.0)], 0.9f64.asin()).unwrap();
    check!(
        format!("5 units travelled: {} of {}", got.share, clear.share),
        (got.share / clear.share - (-0.5f64).exp()).abs() < 1e-9,
        true
    );
}

#[test]
fn the_faces_at_an_angle() {
    // Outside 1, inside 1.5, sine outside 0.9: the cosines are sqrt(0.19) outside and 0.8 inside. Both faces use the outside one.
    let head_on: f64 = 0.04;
    let r = head_on + (1.0 - head_on) * (1.0 - 0.19f64.sqrt()).powi(5);
    expect(
        "two equal faces",
        pass_through(1.0, &[sheet(1.5, 4.0, 0.0)], 0.9f64.asin()),
        3.0,
        (1.0 - r) * (1.0 - r),
    );
}

#[test]
fn almost_trapped_means_almost_nothing() {
    // Into a thinner sheet just below the limit angle.
    let limit = (1.0 / 1.5f64).asin();
    let got = pass_through(1.5, &[sheet(1.0, 0.001, 0.0)], limit - 1e-9).unwrap();
    check!(format!("{got:?}"), got.share < 0.001, true);
    check!(
        "just beyond",
        pass_through(1.5, &[sheet(1.0, 0.001, 0.0)], limit + 1e-9),
        None
    );
}

#[test]
fn one_bad_sheet_stops_everything() {
    // The middle sheet is too thin optically for this angle.
    let stack = [
        sheet(2.4, 1.0, 0.0),
        sheet(0.8, 1.0, 0.0),
        sheet(2.4, 1.0, 0.0),
    ];
    check!(
        "trapped inside the stack",
        pass_through(1.0, &stack, 1.1),
        None
    );
    check!(
        "steeper gets through",
        pass_through(1.0, &stack, 0.5).is_some(),
        true
    );
}

#[test]
fn a_sheet_like_the_outside_is_invisible() {
    let got = pass_through(1.4, &[sheet(1.4, 2.0, 0.0)], 0.6);
    expect("no faces to see", got, 2.0 * 0.6f64.tan(), 1.0);
}

#[test]
fn bad_input() {
    let pane = [sheet(1.5, 1.0, 0.0)];
    for angle in [-0.1, std::f64::consts::FRAC_PI_2, 2.0, f64::NAN] {
        check!(
            format!("angle {angle}"),
            pass_through(1.0, &pane, angle),
            None
        );
    }
    for outside in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        check!(
            format!("outside {outside}"),
            pass_through(outside, &pane, 0.1),
            None
        );
    }
    for bad in [
        sheet(0.0, 1.0, 0.0),
        sheet(-1.5, 1.0, 0.0),
        sheet(f64::NAN, 1.0, 0.0),
        sheet(1.5, -1.0, 0.0),
        sheet(1.5, f64::INFINITY, 0.0),
        sheet(1.5, 1.0, -0.1),
        sheet(1.5, 1.0, f64::NAN),
    ] {
        check!(
            format!("{bad:?}"),
            pass_through(1.0, &[pane[0], bad], 0.1),
            None
        );
    }
}

#[allow(dead_code)]
fn random_stack(rng: &mut Rng) -> Vec<Sheet> {
    (0..rng.int(1, 6))
        .map(|_| {
            sheet(
                rng.int(100, 250) as f64 / 100.0,
                rng.int(0, 50) as f64 / 10.0,
                rng.int(0, 30) as f64 / 100.0,
            )
        })
        .collect()
}

#[test]
fn turned_around_it_is_the_same() {
    let mut rng = Rng::new(1627);
    for case in 0..2000 {
        let stack = random_stack(&mut rng);
        let mut back = stack.clone();
        back.reverse();
        let outside = *rng.pick(&[1.0, 1.33, 2.0]);
        let angle = rng.int(0, 150) as f64 / 100.0;
        let (a, b) = (
            pass_through(outside, &stack, angle),
            pass_through(outside, &back, angle),
        );
        let name =
            format!("case {case}: {stack:?} outside {outside} angle {angle}: {a:?} and {b:?}");
        match (a, b) {
            (Some(a), Some(b)) => check!(
                name,
                (a.shift - b.shift).abs() < 1e-9
                    && (a.share - b.share).abs() < 1e-9
                    && (0.0..=1.0).contains(&a.share),
                true
            ),
            (a, b) => check!(name, a.is_none() && b.is_none(), true),
        }
    }
}

#[test]
fn each_way_through_a_single_face() {
    // A thick pane and the same angle seen from inside it: a beam entering from the glass side loses the same at each face.
    let mut rng = Rng::new(1628);
    for case in 0..500 {
        let n = rng.int(105, 250) as f64 / 100.0;
        let angle = rng.int(1, 140) as f64 / 100.0;
        let into = pass_through(1.0, &[sheet(n, 1.0, 0.0)], angle).unwrap();
        let inner = (angle.sin() / n).asin();
        let out_of = pass_through(n, &[sheet(1.0, 1.0, 0.0)], inner).unwrap();
        check!(
            format!(
                "case {case}: index {n}, angle {angle}: {} and {}",
                into.share, out_of.share
            ),
            (into.share - out_of.share).abs() < 1e-7,
            true
        );
        check!(
            format!("case {case}: shift"),
            (into.shift - inner.tan()).abs() < 1e-9
                && (out_of.shift - angle.tan()).abs() < 1e-7 * angle.tan().max(1.0),
            true
        );
    }
}

#[test]
fn two_sheets_add_their_shifts_and_losses() {
    let one = sheet(1.5, 1.5, 0.2);
    let single = pass_through(1.0, &[one], 0.8).unwrap();
    let double = pass_through(1.0, &[one, one], 0.8).unwrap();
    check!(
        "twice the shift",
        (double.shift - 2.0 * single.shift).abs() < 1e-9,
        true
    );
    // The face between two equal sheets reflects nothing, so only the second slab of tint is added.
    let tint = (-0.2 * 1.5 / (1.0 - (0.8f64.sin() / 1.5).powi(2)).sqrt()).exp();
    check!(
        format!("{} and {}", double.share, single.share * tint),
        (double.share - single.share * tint).abs() < 1e-9,
        true
    );
}
