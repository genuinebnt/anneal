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
fn one_lamp_overhead() {
    let s = scene(vec![], vec![], vec![point(0.0, 2.0, 0.0, 4.0)]);
    // 0.5 * 0.04 of ambient light, and 4 * 0.5 * 1 / 2^2 from the lamp.
    check!(
        "0.02 + 0.5",
        vnear(lit(&s, grey(0.0), HALF, 1, 1), grey(0.52), 1e-12),
        true
    );
}

#[test]
fn slanted_light_is_weaker() {
    let s = scene(vec![], vec![], vec![point(3.0, 4.0, 0.0, 25.0)]);
    // Distance 5, cosine 0.8: 25 * 0.5 * 0.8 / 25.
    check!(
        "0.02 + 0.4",
        vnear(lit(&s, grey(0.0), HALF, 1, 1), grey(0.42), 1e-12),
        true
    );
}

#[test]
fn a_lamp_behind_the_surface() {
    let s = scene(vec![], vec![], vec![point(0.0, -2.0, 0.0, 100.0)]);
    check!(
        "ambient only",
        vnear(lit(&s, grey(0.0), HALF, 1, 1), grey(0.02), 1e-12),
        true
    );
}

#[test]
fn something_in_the_way() {
    let ball = Prim {
        shape: Shape::Sphere {
            center: v3(0.0, 2.0, 0.0),
            radius: 0.5,
        },
        material: 0,
    };
    let s = scene(vec![ball], vec![], vec![point(0.0, 4.0, 0.0, 100.0)]);
    check!(
        "in shadow",
        vnear(lit(&s, grey(0.0), HALF, 1, 1), grey(0.02), 1e-12),
        true
    );
    check!(
        "to the side, lit",
        lit(&s, v3(3.0, 0.0, 0.0), HALF, 1, 1).x > 1.0,
        true
    );
}

#[test]
fn checker_squares() {
    let board = Material::Checker {
        even: grey(1.0),
        odd: grey(0.0),
        cells_per_unit: 1.0,
    };
    let colour = |x: f64, z: f64| surface_colour(&board, v3(x, 0.0, z)).unwrap().x;
    check!(
        "neighbours differ, left of zero too",
        [
            colour(0.5, 0.5),
            colour(1.5, 0.5),
            colour(-0.5, 0.5),
            colour(-0.5, -0.5),
            colour(-1.5, 0.5)
        ],
        [1.0, 0.0, 0.0, 1.0, 1.0]
    );
}

#[test]
fn materials_without_a_matte_colour() {
    let others = [
        Material::Mirror { tint: grey(1.0) },
        Material::Glass {
            index: 1.5,
            absorb: grey(0.0),
        },
        Material::Light {
            radiance: grey(1.0),
        },
    ];
    let s = scene(vec![], vec![], vec![point(0.0, 2.0, 0.0, 4.0)]);
    for m in others {
        check!(
            format!("{m:?}: no colour"),
            surface_colour(&m, grey(0.0)),
            None
        );
        check!(
            format!("{m:?}: no shade"),
            lit(&s, grey(0.0), m, 1, 1),
            grey(0.0)
        );
    }
}
