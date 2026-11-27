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
fn bouncing() {
    check!(
        "off a floor",
        vnear(
            reflect(v3(0.6, -0.8, 0.0), v3(0.0, 1.0, 0.0)),
            v3(0.6, 0.8, 0.0),
            1e-12
        ),
        true
    );
}

#[test]
fn crossing() {
    let straight = refract(v3(0.0, -1.0, 0.0), v3(0.0, 1.0, 0.0), 1.0 / 1.5).unwrap();
    check!(
        "head on goes straight",
        vnear(straight, v3(0.0, -1.0, 0.0), 1e-12),
        true
    );
    // Into glass the ray bends toward the normal: the sine shrinks by 1.5.
    let bent = refract(v3(0.6, -0.8, 0.0), v3(0.0, 1.0, 0.0), 1.0 / 1.5).unwrap();
    check!(
        "bent",
        vnear(bent, v3(0.4, -(1.0f64 - 0.16).sqrt(), 0.0), 1e-12),
        true
    );
    check!(
        "too flat to leave glass",
        refract(v3(0.8, -0.6, 0.0), v3(0.0, 1.0, 0.0), 1.5),
        None
    );
}

#[test]
fn how_much_bounces() {
    check!(
        "head on",
        near(reflectance(1.0, 1.0, 1.5), 0.04, 1e-12),
        true
    );
    check!(
        "head on, leaving",
        near(reflectance(1.0, 1.5, 1.0), 0.04, 1e-12),
        true
    );
    check!(
        "grazing",
        near(reflectance(0.0, 1.0, 1.5), 1.0, 1e-12),
        true
    );
    check!("too flat to leave", reflectance(0.6, 1.5, 1.0), 1.0);
}

#[test]
fn a_mirror_shows_what_is_behind_the_camera() {
    let ball = |z: f64, material: usize| Prim {
        shape: Shape::Sphere {
            center: v3(0.0, 0.0, z),
            radius: 1.0,
        },
        material,
    };
    let s = scene(
        vec![ball(-5.0, 0), ball(5.0, 1)],
        vec![],
        vec![
            Material::Mirror { tint: grey(0.9) },
            Material::Light {
                radiance: v3(0.0, 2.0, 1.0),
            },
        ],
        Sky::Black,
    );
    check!(
        "tint times the glow",
        vnear(
            follow(&s, grey(0.0), v3(0.0, 0.0, -1.0), 3),
            v3(0.0, 1.8, 0.9),
            1e-9
        ),
        true
    );
    check!(
        "with no turns left, black",
        follow(&s, grey(0.0), v3(0.0, 0.0, -1.0), 0),
        grey(0.0)
    );
    check!(
        "the glow itself needs no turns",
        follow(&s, grey(0.0), v3(0.0, 0.0, 1.0), 0),
        v3(0.0, 2.0, 1.0)
    );
}

#[test]
fn clear_glass_in_an_even_sky_is_invisible() {
    let s = slab(CLEAR, Sky::Uniform(grey(1.0)));
    check!(
        "all the light comes through, one way or another",
        vnear(
            follow(&s, v3(0.0, 0.0, 2.0), v3(0.3, 0.0, -1.0), 24),
            grey(1.0),
            1e-9
        ),
        true
    );
}
