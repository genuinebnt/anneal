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
fn the_middle_looks_straight_ahead() {
    let camera = Camera::new(&spec()).unwrap();
    let ray = camera.ray(0.5, 0.5, (0.0, 0.0));
    check!("origin", vnear(ray.origin, v3(0.0, 0.0, 0.0), 1e-12), true);
    check!("direction", vnear(ray.dir, v3(0.0, 0.0, -1.0), 1e-12), true);
}

#[test]
fn right_is_right_and_top_is_up() {
    let camera = Camera::new(&spec()).unwrap();
    // 90 degrees from bottom to top and twice as wide: the corners are at (±2, ±1, -1).
    let top_right = camera.ray(1.0, 0.0, (0.0, 0.0));
    check!(
        "top right",
        vnear(top_right.dir, v3(2.0, 1.0, -1.0).unit(), 1e-12),
        true
    );
    let bottom_left = camera.ray(0.0, 1.0, (0.0, 0.0));
    check!(
        "bottom left",
        vnear(bottom_left.dir, v3(-2.0, -1.0, -1.0).unit(), 1e-12),
        true
    );
}

#[test]
fn what_is_not_a_camera() {
    let bad = CameraSpec {
        look_at: v3(0.0, 0.0, 0.0),
        ..spec()
    };
    check!("looking at itself", Camera::new(&bad).is_none(), true);
    let bad = CameraSpec {
        look_at: v3(0.0, -5.0, 0.0),
        ..spec()
    };
    check!("looking along up", Camera::new(&bad).is_none(), true);
}

#[test]
fn nine_samples_fill_nine_cells() {
    let mut rng = Rng::new(1);
    let offsets = pixel_offsets(9, &mut || unit(&mut rng));
    let mut cells: Vec<(usize, usize)> = offsets
        .iter()
        .map(|&(x, y)| ((x * 3.0) as usize, (y * 3.0) as usize))
        .collect();
    cells.sort();
    cells.dedup();
    check!("samples", offsets.len(), 9);
    check!("different cells", cells.len(), 9);
}

#[test]
fn lens_rays_meet_on_the_sharp_plane() {
    let camera = Camera::new(&CameraSpec {
        lens_diameter: 0.5,
        focus_distance: 4.0,
        ..spec()
    })
    .unwrap();
    for lens in [(0.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.3, 0.4)] {
        let ray = camera.ray(0.5, 0.5, lens);
        let t = (-4.0 - ray.origin.z) / ray.dir.z;
        check!(
            format!("lens {lens:?}"),
            vnear(ray.at(t), v3(0.0, 0.0, -4.0), 1e-9),
            true
        );
    }
    let edge = camera.ray(0.5, 0.5, (1.0, 0.0));
    check!(
        "the lens point (1, 0) is half the diameter to the right",
        vnear(edge.origin, v3(0.25, 0.0, 0.0), 1e-12),
        true
    );
}
