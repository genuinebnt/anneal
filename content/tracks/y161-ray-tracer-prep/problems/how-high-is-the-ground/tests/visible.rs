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
fn inside_each_half() {
    let (p, t) = tile();
    // In the first triangle the height is 2 z.
    expect("first", height_at(&p, &t, 3.0, 1.0), Some((0, 2.0)));
    expect("second", height_at(&p, &t, 1.0, 3.0), Some((1, 2.0)));
}

#[test]
fn off_the_tile() {
    let (p, t) = tile();
    expect("left of it", height_at(&p, &t, -0.5, 1.0), None);
    expect("beyond it", height_at(&p, &t, 4.5, 4.5), None);
}

#[test]
fn the_shared_edge_goes_to_the_lower_number() {
    let (p, t) = tile();
    expect(
        "on the diagonal",
        height_at(&p, &t, 1.0, 1.0),
        Some((0, 2.0)),
    );
}

#[test]
fn corners_and_outer_edges_count() {
    let (p, t) = tile();
    expect("corner", height_at(&p, &t, 4.0, 4.0), Some((0, 8.0)));
    expect("outer edge", height_at(&p, &t, 0.0, 2.0), Some((1, 0.0)));
}

#[test]
fn no_ground() {
    expect("nothing", height_at(&[], &[], 0.0, 0.0), None);
}
