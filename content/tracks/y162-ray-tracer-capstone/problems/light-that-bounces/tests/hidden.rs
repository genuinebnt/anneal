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
fn unit(rng: &mut Rng) -> f64 {
    (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64
}

#[allow(dead_code)]
fn ball(center: Vec3, radius: f64, material: usize) -> Prim {
    Prim {
        shape: Shape::Sphere { center, radius },
        material,
    }
}

#[allow(dead_code)]
fn scene(prims: Vec<Prim>, materials: Vec<Material>, sky: Sky) -> Scene {
    Scene {
        world: World::new(prims, vec![]),
        materials,
        lamps: vec![],
        sky,
    }
}

/// The average of `n` estimates along one ray.
#[allow(dead_code)]
fn average(s: &Scene, ray: Ray, bounces: u32, n: usize, seed: u64) -> Vec3 {
    let mut rng = Rng::new(seed);
    let mut sum = grey(0.0);
    for _ in 0..n {
        sum = sum + path(s, &ray, bounces, &mut || unit(&mut rng));
    }
    sum / n as f64
}

#[allow(dead_code)]
fn forward() -> Ray {
    Ray {
        origin: grey(0.0),
        dir: v3(0.0, 0.0, -1.0),
    }
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

/// A glowing ball of radius 1 in the middle of a grey ball room of radius 2. The ray looks at the wall.
#[allow(dead_code)]
fn lamp_in_a_round_room(albedo: f64) -> (Scene, Ray) {
    let materials = vec![
        Material::Matte {
            albedo: grey(albedo),
        },
        Material::Light {
            radiance: grey(1.0),
        },
    ];
    let s = scene(
        vec![ball(grey(0.0), 2.0, 0), ball(grey(0.0), 1.0, 1)],
        materials,
        Sky::Black,
    );
    (
        s,
        Ray {
            origin: v3(0.0, 1.5, 0.0),
            dir: v3(0.0, 1.0, 0.0),
        },
    )
}

#[test]
fn directions_have_the_right_spread() {
    let mut rng = Rng::new(1681);
    let n = 40_000;
    for normal in [
        v3(0.0, 0.0, 1.0),
        v3(1.0, 0.0, 0.0),
        v3(0.0, -1.0, 0.0),
        v3(0.48, -0.6, 0.64),
        v3(0.95, 0.0, (1.0f64 - 0.9025).sqrt()),
    ] {
        let a = normal
            .cross(if normal.x.abs() > 0.5 {
                v3(0.0, 1.0, 0.0)
            } else {
                v3(1.0, 0.0, 0.0)
            })
            .unit();
        let b = normal.cross(a);
        let (mut up, mut high, mut sa, mut sb, mut qa, mut qb) = (0.0, 0, 0.0, 0.0, 0.0, 0.0);
        for _ in 0..n {
            let d = cosine_direction(normal, &mut || unit(&mut rng));
            check!(
                "unit and on the side of the normal",
                near(d.length(), 1.0, 1e-9) && d.dot(normal) >= -1e-9,
                true
            );
            up += d.dot(normal);
            high += usize::from(d.dot(normal) > 0.5);
            sa += d.dot(a);
            sb += d.dot(b);
            qa += d.dot(a).powi(2);
            qb += d.dot(b).powi(2);
        }
        let n = n as f64;
        // Five standard deviations of each average.
        check!(
            format!("{normal:?}: mean cosine {}", up / n),
            near(up / n, 2.0 / 3.0, 0.006),
            true
        );
        check!(
            format!(
                "{normal:?}: share above 60 degrees from the plane {}",
                high as f64 / n
            ),
            near(high as f64 / n, 0.75, 0.011),
            true
        );
        check!(
            format!("{normal:?}: no lean ({}, {})", sa / n, sb / n),
            near(sa / n, 0.0, 0.0125) && near(sb / n, 0.0, 0.0125),
            true
        );
        check!(
            format!("{normal:?}: round ({}, {})", qa / n, qb / n),
            near(qa / n, 0.25, 0.008) && near(qb / n, 0.25, 0.008),
            true
        );
    }
}

#[test]
fn other_albedos_in_the_round_room() {
    for (albedo, tolerance) in [(0.3, 0.012), (0.6, 0.014), (0.95, 0.02), (1.0, 0.022)] {
        let (s, ray) = lamp_in_a_round_room(albedo);
        let want = albedo * 0.25 / (1.0 - albedo * 0.75);
        let got = average(&s, ray, 400, 40_000, 1682).x;
        check!(
            format!("albedo {albedo}: {want} within {tolerance}, got {got}"),
            near(got, want, tolerance),
            true
        );
    }
}

#[test]
fn the_limit_on_bounces_is_a_limit() {
    let (s, ray) = lamp_in_a_round_room(0.8);
    check!(
        "only the wall itself",
        average(&s, ray, 1, 2000, 3),
        grey(0.0)
    );
    // With two visits only the first bounce can find the lamp: 0.8 / 4.
    let got = average(&s, ray, 2, 40_000, 4).x;
    check!(
        format!("two visits: 0.2, got {got}"),
        near(got, 0.2, 0.01),
        true
    );
}

#[test]
fn clear_glass_keeps_all_the_light() {
    let s = scene(
        vec![ball(v3(0.0, 0.0, -5.0), 1.0, 0)],
        vec![CLEAR],
        Sky::Uniform(grey(1.0)),
    );
    for i in -3..=3 {
        let ray = Ray {
            origin: grey(0.0),
            dir: v3(i as f64 * 0.04, 0.02, -1.0),
        };
        let got = average(&s, ray, 64, 8000, 1683).x;
        check!(
            format!("ray {i}: 1 within 0.01, got {got}"),
            near(got, 1.0, 0.01),
            true
        );
    }
}

#[test]
fn tinted_glass_loses_what_it_should() {
    let absorb = v3(0.0, 0.5, 2.0);
    let materials = vec![Material::Glass { index: 1.5, absorb }];
    let pane = |z: f64, facing: f64| Plane {
        normal: v3(0.0, 0.0, facing),
        offset: facing * z,
        material: 0,
    };
    let s = Scene {
        world: World::new(vec![], vec![pane(0.0, 1.0), pane(-1.0, -1.0)]),
        materials,
        lamps: vec![],
        sky: Sky::Uniform(grey(1.0)),
    };
    let got = average(
        &s,
        Ray {
            origin: v3(0.0, 0.0, 3.0),
            dir: v3(0.0, 0.0, -5.0),
        },
        200,
        40_000,
        1684,
    );
    let r: f64 = 0.04;
    let want = |a: f64| {
        let keep = (-a).exp();
        let through = (1.0 - r) * (1.0 - r) * keep / (1.0 - r * r * keep * keep);
        r + (1.0 - r) * (1.0 - r) * r * keep * keep / (1.0 - r * r * keep * keep) + through
    };
    let want = v3(want(absorb.x), want(absorb.y), want(absorb.z));
    check!(
        format!("{want:?} within 0.012, got {got:?}"),
        vnear(got, want, 0.012),
        true
    );
}

#[test]
fn all_matte_looks_alike_and_lamps_are_not_used() {
    let sky = Sky::Uniform(grey(1.0));
    let at = v3(0.0, 0.0, -5.0);
    let shiny = Material::Shiny {
        albedo: grey(0.5),
        strength: 3.0,
        sharpness: 5.0,
    };
    let board = Material::Checker {
        even: grey(0.5),
        odd: grey(0.5),
        cells_per_unit: 2.0,
    };
    for material in [shiny, board] {
        let mut s = scene(vec![ball(at, 1.0, 0)], vec![material], sky);
        s.lamps = vec![Lamp::Point {
            position: grey(0.0),
            power: grey(100.0),
        }];
        check!(
            format!("{material:?}"),
            vnear(average(&s, forward(), 8, 1, 5), grey(0.5), 1e-12),
            true
        );
    }
}

/// How many numbers the average path asks for.
#[allow(dead_code)]
fn appetite(s: &Scene, ray: Ray) -> f64 {
    let mut rng = Rng::new(1685);
    let mut calls = 0u64;
    for _ in 0..200 {
        path(s, &ray, 10_000, &mut || {
            calls += 1;
            assert!(
                calls <= 200 * 200,
                "the paths ask for more than 200 numbers each"
            );
            unit(&mut rng)
        });
    }
    calls as f64 / 200.0
}

#[test]
fn a_path_with_little_left_stops() {
    for albedo in [0.0, 0.5, 0.9] {
        let s = scene(
            vec![ball(grey(0.0), 3.0, 0)],
            vec![Material::Matte {
                albedo: grey(albedo),
            }],
            Sky::Black,
        );
        let asked = appetite(&s, forward());
        check!(
            format!("grey room {albedo}: {asked} numbers per path, at most 200"),
            asked <= 200.0,
            true
        );
    }
}

#[test]
fn even_a_perfect_room_lets_go() {
    // Nothing is lost at these walls. The paths must end anyway, long before 10,000 visits.
    let s = scene(
        vec![ball(grey(0.0), 3.0, 0)],
        vec![Material::Matte { albedo: grey(1.0) }],
        Sky::Black,
    );
    let asked = appetite(&s, forward());
    check!(
        format!("{asked} numbers per path, at most 200"),
        asked <= 200.0,
        true
    );
}

#[test]
fn faint_paths_are_not_simply_dropped() {
    // Two dim mirrors, then a very bright wall. After the second mirror only 4 percent of the light is
    // left, yet that is all there is to see: 0.2 * 0.2 * 100 = 4.
    let mirror = Material::Mirror { tint: grey(0.2) };
    let glow = Material::Light {
        radiance: grey(100.0),
    };
    let corner = |a: Vec3, b: Vec3, c: Vec3| Prim {
        shape: Shape::Triangle { a, b, c },
        material: 0,
    };
    // A 45 degree mirror at the origin turns +x travel into +y, a second at (0, 5) turns it back into +x.
    let first = corner(v3(-1.0, -1.0, -1.0), v3(1.0, 1.0, -1.0), v3(0.0, 0.0, 2.0));
    let second = corner(v3(-1.0, 4.0, -1.0), v3(1.0, 6.0, -1.0), v3(0.0, 5.0, 2.0));
    let wall = Plane {
        normal: v3(1.0, 0.0, 0.0),
        offset: 5.0,
        material: 1,
    };
    let s = Scene {
        world: World::new(vec![first, second], vec![wall]),
        materials: vec![mirror, glow],
        lamps: vec![],
        sky: Sky::Black,
    };
    let ray = Ray {
        origin: v3(-3.0, 0.0, 0.0),
        dir: v3(1.0, 0.0, 0.0),
    };
    let got = average(&s, ray, 100, 40_000, 1686).x;
    check!(
        format!("4 within 0.8, got {got}"),
        near(got, 4.0, 0.8),
        true
    );
}
