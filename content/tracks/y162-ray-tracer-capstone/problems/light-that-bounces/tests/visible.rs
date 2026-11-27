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
fn directions_stay_on_their_side() {
    let mut rng = Rng::new(1);
    for normal in [v3(0.0, 0.0, 1.0), v3(1.0, 0.0, 0.0), v3(-0.6, 0.0, 0.8)] {
        for _ in 0..200 {
            let d = cosine_direction(normal, &mut || unit(&mut rng));
            check!(
                format!("{d:?} around {normal:?}"),
                near(d.length(), 1.0, 1e-9) && d.dot(normal) >= -1e-9,
                true
            );
        }
    }
}

#[test]
fn a_grey_ball_under_an_even_sky_shows_its_grey_exactly() {
    // A ray leaving a ball never meets it again, so every single estimate is the albedo times the sky.
    let albedo = v3(0.8, 0.5, 0.25);
    let s = scene(
        vec![ball(v3(0.0, 0.0, -5.0), 1.0, 0)],
        vec![Material::Matte { albedo }],
        Sky::Uniform(grey(1.0)),
    );
    for seed in 0..50 {
        check!(
            format!("seed {seed}"),
            vnear(average(&s, forward(), 8, 1, seed), albedo, 1e-12),
            true
        );
    }
}

#[test]
fn mirrors_and_glowing_things() {
    let s = scene(
        vec![ball(v3(0.0, 0.0, -5.0), 1.0, 0)],
        vec![Material::Mirror { tint: grey(0.9) }],
        Sky::Uniform(grey(1.0)),
    );
    check!(
        "mirror",
        vnear(average(&s, forward(), 8, 1, 1), grey(0.9), 1e-12),
        true
    );
    let s = scene(
        vec![ball(v3(0.0, 0.0, -5.0), 1.0, 0)],
        vec![Material::Light {
            radiance: v3(3.0, 2.0, 1.0),
        }],
        Sky::Black,
    );
    check!("glow", average(&s, forward(), 1, 1, 1), v3(3.0, 2.0, 1.0));
    check!(
        "no surface may be visited",
        average(&s, forward(), 0, 1, 1),
        grey(0.0)
    );
}

#[test]
fn a_miss_gathers_the_sky() {
    let s = scene(vec![], vec![], Sky::Uniform(v3(0.1, 0.2, 0.3)));
    check!(
        "one visit allowed",
        average(&s, forward(), 1, 1, 1),
        v3(0.1, 0.2, 0.3)
    );
}

#[test]
fn light_that_bounced_many_times_arrives() {
    // The wall sees the glowing ball in a quarter of its view and itself in the rest. So wall = albedo * (1/4 * 1 + 3/4 * wall).
    let (s, ray) = lamp_in_a_round_room(0.8);
    let got = average(&s, ray, 200, 40_000, 2);
    check!(
        format!("0.5 within 0.015, got {}", got.x),
        near(got.x, 0.5, 0.015),
        true
    );
}
