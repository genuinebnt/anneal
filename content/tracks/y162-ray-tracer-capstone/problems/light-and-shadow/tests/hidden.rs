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
fn grey(v: f64) -> Vec3 {
    v3(v, v, v)
}

#[allow(dead_code)]
fn up() -> Vec3 {
    v3(0.0, 1.0, 0.0)
}

#[allow(dead_code)]
fn scene(prims: Vec<Prim>, planes: Vec<Plane>, lamps: Vec<Lamp>) -> Scene {
    Scene {
        world: World::new(prims, planes),
        materials: vec![Material::Matte { albedo: grey(0.5) }],
        lamps,
        sky: Sky::Black,
    }
}

#[allow(dead_code)]
fn point(x: f64, y: f64, z: f64, power: f64) -> Lamp {
    Lamp::Point {
        position: v3(x, y, z),
        power: grey(power),
    }
}

#[allow(dead_code)]
fn unit(rng: &mut Rng) -> f64 {
    (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64
}

/// The shade at `p` on a surface facing up, seen from straight above.
#[allow(dead_code)]
fn lit(s: &Scene, p: Vec3, material: Material, samples: usize, seed: u64) -> Vec3 {
    let mut rng = Rng::new(seed);
    shade_matte(s, p, up(), up(), &material, samples, &mut || unit(&mut rng))
}

#[allow(dead_code)]
const HALF: Material = Material::Matte {
    albedo: Vec3 {
        x: 0.5,
        y: 0.5,
        z: 0.5,
    },
};

#[test]
fn colours_and_lamps_add_up() {
    let material = Material::Matte {
        albedo: v3(1.0, 0.5, 0.25),
    };
    let a = Lamp::Point {
        position: v3(0.0, 1.0, 0.0),
        power: v3(1.0, 0.0, 2.0),
    };
    let b = Lamp::Point {
        position: v3(0.0, 2.0, 0.0),
        power: v3(0.0, 4.0, 4.0),
    };
    let s = scene(vec![], vec![], vec![a, b]);
    let want = v3(1.0, 0.5, 0.25) * 0.04 + v3(1.0, 0.0, 0.5) + v3(0.0, 0.5, 0.25);
    check!(
        "sum",
        vnear(lit(&s, grey(0.0), material, 1, 1), want, 1e-12),
        true
    );
}

#[test]
fn the_highlight() {
    let s = scene(vec![], vec![], vec![point(3.0, 4.0, 0.0, 25.0)]);
    let m = Material::Shiny {
        albedo: grey(0.5),
        strength: 2.0,
        sharpness: 10.0,
    };
    // Toward the lamp (0.6, 0.8, 0), toward the eye (0, 1, 0): halfway is (0.6, 1.8, 0), made a unit vector.
    let half = v3(0.6, 1.8, 0.0).unit();
    let want = 0.02 + 0.4 + 2.0 * half.y.powf(10.0);
    check!(
        "matte plus glint",
        vnear(lit(&s, grey(0.0), m, 1, 1), grey(want), 1e-12),
        true
    );
}

#[test]
fn the_glint_needs_the_lamp_in_front() {
    let m = Material::Shiny {
        albedo: grey(0.5),
        strength: 2.0,
        sharpness: 1.0,
    };
    let s = scene(vec![], vec![], vec![point(3.0, -0.1, 0.0, 25.0)]);
    check!(
        "lamp just below the surface",
        vnear(lit(&s, grey(0.0), m, 1, 1), grey(0.02), 1e-12),
        true
    );
}

#[test]
fn what_is_beyond_the_lamp_casts_no_shadow() {
    let ceiling = Plane {
        normal: up(),
        offset: 3.0,
        material: 0,
    };
    let s = scene(vec![], vec![ceiling], vec![point(0.0, 2.0, 0.0, 4.0)]);
    check!(
        "lit",
        vnear(lit(&s, grey(0.0), HALF, 1, 1), grey(0.52), 1e-12),
        true
    );
}

#[test]
fn close_to_either_end_but_not_within_a_thousandth() {
    for gap in [0.002, 0.01, 1.99, 1.998] {
        let sheet = Plane {
            normal: up(),
            offset: gap,
            material: 0,
        };
        let s = scene(vec![], vec![sheet], vec![point(0.0, 2.0, 0.0, 4.0)]);
        check!(
            format!("a sheet at height {gap}"),
            vnear(lit(&s, grey(0.0), HALF, 1, 1), grey(0.02), 1e-12),
            true
        );
    }
}

#[test]
fn a_surface_never_shadows_itself() {
    // Points found by hitting a floor and a very large ball: each is a little above or below the true surface.
    let floor = Plane {
        normal: v3(0.1, 1.0, 0.07),
        offset: 0.3,
        material: 0,
    };
    let big = Prim {
        shape: Shape::Sphere {
            center: v3(0.0, -1000.0, 0.0),
            radius: 1000.0,
        },
        material: 0,
    };
    let worlds = [(vec![], vec![floor]), (vec![big], vec![])];
    let mut rng = Rng::new(1651);
    for (which, (prims, planes)) in worlds.into_iter().enumerate() {
        let s = scene(prims, planes, vec![point(2.0, 30.0, 1.0, 5000.0)]);
        for case in 0..2000 {
            let origin = v3(
                rng.int(-900, 900) as f64 / 100.0,
                rng.int(500, 2000) as f64 / 100.0,
                rng.int(-900, 900) as f64 / 100.0,
            );
            let aim = v3(
                rng.int(-900, 900) as f64 / 100.0,
                rng.int(-100, 100) as f64 / 100.0,
                rng.int(-900, 900) as f64 / 100.0,
            );
            let ray = Ray {
                origin,
                dir: (aim - origin).unit(),
            };
            let Some(hit) = s.world.closest(&ray, 1e-4, 1e6) else {
                continue;
            };
            let normal = if hit.normal.dot(ray.dir) < 0.0 {
                hit.normal
            } else {
                -hit.normal
            };
            if normal.dot(v3(2.0, 30.0, 1.0) - hit.point) <= 0.5 {
                continue;
            }
            let mut r = Rng::new(1);
            let got = shade_matte(&s, hit.point, normal, -ray.dir, &HALF, 1, &mut || {
                unit(&mut r)
            });
            check!(
                format!("world {which}, case {case}: point {:?}", hit.point),
                got.x > 0.1,
                true
            );
        }
    }
}

#[test]
fn a_lamp_exactly_on_the_point() {
    let s = scene(
        vec![],
        vec![],
        vec![point(0.0, 0.0, 0.0, 4.0), point(0.0, 2.0, 0.0, 4.0)],
    );
    check!(
        "the other lamp still counts",
        vnear(lit(&s, grey(0.0), HALF, 1, 1), grey(0.52), 1e-12),
        true
    );
}

#[test]
fn a_panel_of_no_size_is_a_point() {
    let panel = Lamp::Panel {
        corner: v3(3.0, 4.0, 0.0),
        edge_u: grey(0.0),
        edge_v: grey(0.0),
        power: grey(25.0),
    };
    let s = scene(vec![], vec![], vec![panel]);
    for samples in [0, 1, 7, 100] {
        check!(
            format!("{samples} samples"),
            vnear(lit(&s, grey(0.0), HALF, samples, 2), grey(0.42), 1e-9),
            true
        );
    }
}

/// A 2 by 2 panel, 100 above the point, and a screen halfway up that hides the part with x > `from`.
#[allow(dead_code)]
fn screened(from: f64) -> Scene {
    let panel = Lamp::Panel {
        corner: v3(-1.0, 100.0, -1.0),
        edge_u: v3(2.0, 0.0, 0.0),
        edge_v: v3(0.0, 0.0, 2.0),
        power: grey(20_000.0),
    };
    let x = from / 2.0;
    let c = [
        v3(x, 50.0, -5.0),
        v3(5.0, 50.0, -5.0),
        v3(5.0, 50.0, 5.0),
        v3(x, 50.0, 5.0),
    ];
    let screen = [
        Shape::Triangle {
            a: c[0],
            b: c[1],
            c: c[2],
        },
        Shape::Triangle {
            a: c[0],
            b: c[2],
            c: c[3],
        },
    ];
    scene(
        screen
            .iter()
            .map(|&shape| Prim { shape, material: 0 })
            .collect(),
        vec![],
        vec![panel],
    )
}

#[test]
fn a_panel_gives_soft_shadows() {
    // The whole panel gives 20000 * 0.5 / 100^2 = 1, within 0.02 %.
    for (from, visible) in [
        (1.0, 1.0),
        (0.5, 0.75),
        (0.0, 0.5),
        (-0.5, 0.25),
        (-1.0, 0.0),
    ] {
        for seed in 0..3 {
            let got = lit(&screened(from), grey(0.0), HALF, 4000, 1652 + seed).x - 0.02;
            // Five standard deviations of 4000 draws are at most 0.04.
            check!(
                format!("visible share {visible}: got {got}"),
                near(got, visible, 0.041),
                true
            );
        }
    }
}

#[test]
fn panel_samples_move_along_both_edges() {
    // Only the quarter of the panel with x < 0 and z < 0 stays visible.
    let mut s = screened(0.0);
    let c = [
        v3(-5.0, 50.0, 0.0),
        v3(0.0, 50.0, 0.0),
        v3(0.0, 50.0, 5.0),
        v3(-5.0, 50.0, 5.0),
    ];
    let mut prims = s.world.prims().to_vec();
    prims.push(Prim {
        shape: Shape::Triangle {
            a: c[0],
            b: c[1],
            c: c[2],
        },
        material: 0,
    });
    prims.push(Prim {
        shape: Shape::Triangle {
            a: c[0],
            b: c[2],
            c: c[3],
        },
        material: 0,
    });
    s.world = World::new(prims, vec![]);
    let got = lit(&s, grey(0.0), HALF, 4000, 1653).x - 0.02;
    check!(
        format!("a quarter: got {got}"),
        near(got, 0.25, 0.041),
        true
    );
}

#[test]
fn huge_coordinates_do_not_panic() {
    let board = Material::Checker {
        even: grey(1.0),
        odd: grey(0.0),
        cells_per_unit: 3.0,
    };
    for x in [
        1e19,
        -1e19,
        1e300,
        -1e300,
        f64::INFINITY,
        f64::NAN,
        f64::MAX,
    ] {
        for z in [1e19, -1e19, 0.5, f64::NAN] {
            check!(
                format!("({x}, {z})"),
                surface_colour(&board, v3(x, 0.0, z)).is_some(),
                true
            );
        }
    }
}

#[test]
fn random_checker_cells() {
    let mut rng = Rng::new(1654);
    for _ in 0..2000 {
        let size = *rng.pick(&[0.5, 1.0, 2.0, 4.0]);
        let (i, k) = (rng.int(-1000, 1000), rng.int(-1000, 1000));
        let board = Material::Checker {
            even: grey(1.0),
            odd: grey(0.0),
            cells_per_unit: size,
        };
        let p = v3(
            (i as f64 + 0.5) / size,
            rng.int(-9, 9) as f64,
            (k as f64 + 0.25) / size,
        );
        let want = if (i + k).rem_euclid(2) == 0 { 1.0 } else { 0.0 };
        check!(
            format!("cell ({i}, {k}) at {size} per unit"),
            surface_colour(&board, p).unwrap().x,
            want
        );
    }
}
