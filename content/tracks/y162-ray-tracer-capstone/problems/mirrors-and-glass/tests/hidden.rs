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
const CLEAR: Material = Material::Glass {
    index: 1.5,
    absorb: Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    },
};

/// A plane facing along +z or -z at height `z`.
#[allow(dead_code)]
fn pane(z: f64, facing: f64, material: usize) -> Plane {
    Plane {
        normal: v3(0.0, 0.0, facing),
        offset: facing * z,
        material,
    }
}

#[allow(dead_code)]
fn scene(prims: Vec<Prim>, planes: Vec<Plane>, materials: Vec<Material>, sky: Sky) -> Scene {
    Scene {
        world: World::new(prims, planes),
        materials,
        lamps: vec![],
        sky,
    }
}

#[allow(dead_code)]
fn follow(s: &Scene, origin: Vec3, dir: Vec3, depth: u32) -> Vec3 {
    let mut rng = Rng::new(5);
    trace(s, &Ray { origin, dir }, depth, 1, &mut || {
        (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    })
}

/// A slab of glass from z = -1 to z = 0, each side facing outwards.
#[allow(dead_code)]
fn slab(material: Material, sky: Sky) -> Scene {
    scene(
        vec![],
        vec![pane(0.0, 1.0, 0), pane(-1.0, -1.0, 0)],
        vec![material],
        sky,
    )
}

#[test]
fn random_crossings_obey_the_law() {
    let mut rng = Rng::new(1661);
    for case in 0..2000 {
        let normal = v3(
            rng.int(-9, 9) as f64,
            rng.int(-9, 9) as f64,
            rng.int(1, 9) as f64,
        )
        .unit();
        let mut dir = v3(
            rng.int(-90, 90) as f64,
            rng.int(-90, 90) as f64,
            rng.int(-90, 90) as f64 + 0.5,
        )
        .unit();
        if normal.dot(dir).abs() < 1e-3 {
            continue;
        }
        if normal.dot(dir) > 0.0 {
            dir = -dir;
        }
        let (from, to) = *rng.pick(&[(1.0, 1.5), (1.5, 1.0), (1.33, 1.5), (2.4, 1.0), (1.0, 1.0)]);
        let eta = from / to;
        let cos_in = -dir.dot(normal);
        let sine_out = eta * (1.0 - cos_in * cos_in).max(0.0).sqrt();
        if (sine_out - 1.0).abs() < 1e-6 {
            continue;
        }
        let name = format!("case {case}: dir {dir:?} normal {normal:?} {from} to {to}");
        let got = refract(dir, normal, eta);
        check!(name.clone(), got.is_some(), sine_out < 1.0);
        let share = reflectance(cos_in, from, to);
        check!(name.clone(), (0.0..=1.0).contains(&share), true);
        let Some(out) = got else {
            check!(name, share, 1.0);
            continue;
        };
        check!(
            name.clone(),
            near(out.length(), 1.0, 1e-9) && out.dot(normal) < 0.0,
            true
        );
        // The part along the surface keeps its direction and shrinks or grows by eta.
        let along_in = dir - normal * dir.dot(normal);
        let along_out = out - normal * out.dot(normal);
        check!(name.clone(), vnear(along_out, along_in * eta, 1e-9), true);
        // The same surface met from the other side at the outgoing angle reflects the same share.
        let back = reflectance(-out.dot(normal), to, from);
        check!(
            format!("{name}: {share} one way, {back} the other"),
            near(share, back, 1e-7),
            true
        );
        let mirrored = reflect(dir, normal);
        check!(
            name,
            near(mirrored.dot(normal), cos_in, 1e-9)
                && vnear(mirrored - normal * mirrored.dot(normal), along_in, 1e-9),
            true
        );
    }
}

#[test]
fn the_formula() {
    for cos in [0.0, 0.2, 0.5, 0.9, 1.0] {
        let straight = (0.5f64 / 2.5f64).powi(2);
        let cos: f64 = cos;
        let want = straight + (1.0 - straight) * (1.0 - cos).powi(5);
        check!(
            format!("cos {cos}"),
            near(reflectance(cos, 1.0, 1.5), want, 1e-12),
            true
        );
    }
}

#[test]
fn just_before_nothing_gets_out_almost_everything_bounces() {
    let limit = (1.0f64 - (1.0 / 1.5f64).powi(2)).sqrt();
    let share = reflectance(limit + 1e-9, 1.5, 1.0);
    check!(format!("got {share}"), share > 0.99, true);
}

#[test]
fn parallel_mirrors_stop() {
    let s = scene(
        vec![],
        vec![pane(-1.0, 1.0, 0), pane(1.0, -1.0, 0)],
        vec![Material::Mirror { tint: grey(1.0) }],
        Sky::Uniform(grey(1.0)),
    );
    for depth in [0, 1, 5, 64, 65, 1000, u32::MAX] {
        check!(
            format!("depth {depth}"),
            follow(&s, grey(0.0), v3(0.0, 0.0, 1.0), depth),
            grey(0.0)
        );
    }
}

#[test]
fn each_turn_costs_one() {
    // Two mirrors, then a glowing wall behind the second: exactly two turns are needed.
    let mirror = Material::Mirror { tint: grey(0.5) };
    let glow = Material::Light {
        radiance: grey(8.0),
    };
    let corner = |a: Vec3, b: Vec3, c: Vec3| Prim {
        shape: Shape::Triangle { a, b, c },
        material: 0,
    };
    // A 45 degree mirror at the origin turns +x travel into +y, a second at (0, 5) turns it into +x.
    let first = corner(v3(-1.0, -1.0, -1.0), v3(1.0, 1.0, -1.0), v3(0.0, 0.0, 2.0));
    let second = corner(v3(-1.0, 4.0, -1.0), v3(1.0, 6.0, -1.0), v3(0.0, 5.0, 2.0));
    let wall = Plane {
        normal: v3(1.0, 0.0, 0.0),
        offset: 5.0,
        material: 1,
    };
    let s = scene(
        vec![first, second],
        vec![wall],
        vec![mirror, glow],
        Sky::Black,
    );
    let start = v3(-3.0, 0.0, 0.0);
    check!(
        "two turns",
        vnear(follow(&s, start, v3(1.0, 0.0, 0.0), 2), grey(2.0), 1e-9),
        true
    );
    check!(
        "one is not enough",
        follow(&s, start, v3(1.0, 0.0, 0.0), 1),
        grey(0.0)
    );
}

#[test]
fn a_slab_of_tinted_glass() {
    // Head on, light goes back and forth inside. Summing the endless series gives the two parts.
    let absorb = v3(0.0, 0.5, 2.0);
    let s = slab(
        Material::Glass { index: 1.5, absorb },
        Sky::Uniform(grey(1.0)),
    );
    let got = follow(&s, v3(0.0, 0.0, 3.0), v3(0.0, 0.0, -1.0), 40);
    let r: f64 = 0.04;
    let want = |a: f64| {
        let keep = (-a * 1.0f64).exp();
        let through = (1.0 - r) * (1.0 - r) * keep / (1.0 - r * r * keep * keep);
        let back = r + (1.0 - r) * (1.0 - r) * r * keep * keep / (1.0 - r * r * keep * keep);
        through + back
    };
    check!(
        format!("got {got:?}"),
        vnear(
            got,
            v3(want(absorb.x), want(absorb.y), want(absorb.z)),
            1e-9
        ),
        true
    );
}

#[test]
fn the_length_of_the_ray_given_does_not_matter() {
    let s = slab(
        Material::Glass {
            index: 1.5,
            absorb: grey(0.7),
        },
        Sky::Uniform(grey(1.0)),
    );
    let a = follow(&s, v3(0.0, 0.0, 3.0), v3(0.2, 0.1, -1.0), 30);
    let b = follow(&s, v3(0.0, 0.0, 3.0), v3(2.0, 1.0, -10.0), 30);
    check!(format!("{a:?} and {b:?}"), vnear(a, b, 1e-9), true);
}

#[test]
fn trapped_inside_the_slab() {
    let s = slab(CLEAR, Sky::Uniform(grey(1.0)));
    // 60 degrees from the normal is beyond the limit of about 42.
    let got = follow(&s, v3(0.0, 0.0, -0.5), v3(0.866, 0.0, -0.5), 40);
    check!("it never gets out", got, grey(0.0));
    // 30 degrees gets out, a part at each side.
    let got = follow(&s, v3(0.0, 0.0, -0.5), v3(0.5, 0.0, -0.866), 40);
    check!(
        format!("it gets out: {got:?}"),
        vnear(got, grey(1.0), 1e-9),
        true
    );
}

#[test]
fn a_glass_ball_in_an_even_sky() {
    let ball = Prim {
        shape: Shape::Sphere {
            center: v3(0.0, 0.0, -5.0),
            radius: 1.0,
        },
        material: 0,
    };
    let s = scene(
        vec![ball],
        vec![],
        vec![CLEAR],
        Sky::Uniform(v3(1.0, 0.5, 0.25)),
    );
    for i in -7..=7 {
        let got = follow(&s, grey(0.0), v3(i as f64 * 0.02, 0.01, -1.0), 24);
        check!(
            format!("ray {i}: {got:?}"),
            vnear(got, v3(1.0, 0.5, 0.25), 1e-6),
            true
        );
    }
}

#[test]
fn matte_surfaces_are_shaded_from_the_side_the_ray_came_from() {
    let wall = pane(-2.0, -1.0, 0);
    let mut s = scene(
        vec![],
        vec![wall],
        vec![Material::Matte { albedo: grey(0.5) }],
        Sky::Black,
    );
    s.lamps = vec![Lamp::Point {
        position: v3(0.0, 0.0, 0.0),
        power: grey(16.0),
    }];
    // The wall's own normal points away from the camera. It is lit on the camera's side all the same.
    let got = follow(&s, v3(0.0, 0.0, 0.0), v3(0.0, 0.0, -1.0), 0);
    check!(
        format!("{got:?}"),
        vnear(got, grey(0.02 + 16.0 * 0.5 / 4.0), 1e-9),
        true
    );
}

#[test]
fn a_miss_sees_the_sky() {
    let s = scene(vec![], vec![], vec![], Sky::Gradient);
    check!(
        "straight up",
        vnear(
            follow(&s, grey(0.0), v3(0.0, 3.0, 0.0), 0),
            v3(0.45, 0.65, 1.0),
            1e-12
        ),
        true
    );
}
