#[allow(unused_imports)]
use anneal_prelude::Rng;
use solution::*;

#[allow(dead_code)]
fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[allow(dead_code)]
fn grey(v: f64) -> Vec3 {
    v3(v, v, v)
}

#[allow(dead_code)]
fn camera(from: Vec3, to: Vec3, fov: f64, aspect: f64) -> Camera {
    Camera::new(&CameraSpec {
        position: from,
        look_at: to,
        up: v3(0.0, 1.0, 0.0),
        vfov_degrees: fov,
        aspect,
        lens_diameter: 0.0,
        focus_distance: 1.0,
    })
    .unwrap()
}

#[allow(dead_code)]
fn ahead() -> Camera {
    camera(grey(0.0), v3(0.0, 0.0, -1.0), 90.0, 1.0)
}

#[allow(dead_code)]
fn empty(sky: Sky) -> Scene {
    Scene {
        world: World::new(vec![], vec![]),
        materials: vec![],
        lamps: vec![],
        sky,
    }
}

#[allow(dead_code)]
const WHITTED: Integrator = Integrator::Whitted {
    depth: 6,
    panel_samples: 2,
};
#[allow(dead_code)]
const PATH: Integrator = Integrator::Path { max_bounces: 12 };

#[allow(dead_code)]
fn settings(width: usize, height: usize, samples: usize, integrator: Integrator) -> Settings {
    Settings {
        width,
        height,
        samples,
        seed: 7,
        threads: 3,
        integrator,
    }
}

/// The Cornell box: x from -1 to 1, y from 0 to 2, z from -1 to 1, open towards the camera. The lamp is a glowing square under the ceiling.
#[allow(dead_code)]
fn cornell() -> (Scene, Camera) {
    let materials = vec![
        Material::Matte { albedo: grey(0.73) },
        Material::Matte {
            albedo: v3(0.65, 0.05, 0.05),
        },
        Material::Matte {
            albedo: v3(0.12, 0.45, 0.15),
        },
        Material::Light {
            radiance: grey(5.0),
        },
        Material::Glass {
            index: 1.5,
            absorb: grey(0.0),
        },
        Material::Mirror { tint: grey(0.9) },
    ];
    let mut prims = Vec::new();
    let p = v3;
    prims.extend(quad(
        p(-1.0, 0.0, 1.0),
        p(1.0, 0.0, 1.0),
        p(1.0, 0.0, -1.0),
        p(-1.0, 0.0, -1.0),
        0,
    ));
    prims.extend(quad(
        p(-1.0, 2.0, -1.0),
        p(1.0, 2.0, -1.0),
        p(1.0, 2.0, 1.0),
        p(-1.0, 2.0, 1.0),
        0,
    ));
    prims.extend(quad(
        p(-1.0, 0.0, -1.0),
        p(1.0, 0.0, -1.0),
        p(1.0, 2.0, -1.0),
        p(-1.0, 2.0, -1.0),
        0,
    ));
    prims.extend(quad(
        p(-1.0, 0.0, 1.0),
        p(-1.0, 0.0, -1.0),
        p(-1.0, 2.0, -1.0),
        p(-1.0, 2.0, 1.0),
        1,
    ));
    prims.extend(quad(
        p(1.0, 0.0, -1.0),
        p(1.0, 0.0, 1.0),
        p(1.0, 2.0, 1.0),
        p(1.0, 2.0, -1.0),
        2,
    ));
    prims.extend(quad(
        p(-0.6, 1.99, -0.6),
        p(0.6, 1.99, -0.6),
        p(0.6, 1.99, 0.6),
        p(-0.6, 1.99, 0.6),
        3,
    ));
    prims.extend(cuboid(p(-0.35, 0.0, -0.35), p(0.6, 1.1, 0.6), 0.35, 0));
    prims.push(Prim {
        shape: Shape::Sphere {
            center: p(0.4, 0.38, 0.3),
            radius: 0.38,
        },
        material: 4,
    });
    prims.push(Prim {
        shape: Shape::Sphere {
            center: p(0.55, 0.2, -0.5),
            radius: 0.2,
        },
        material: 5,
    });
    let scene = Scene {
        world: World::new(prims, vec![]),
        materials,
        lamps: vec![],
        sky: Sky::Black,
    };
    (scene, camera(p(0.0, 1.0, 3.5), p(0.0, 1.0, 0.0), 44.0, 1.0))
}

/// A small scene for the sharp renderer: floor, a ball of each kind and a panel lamp.
#[allow(dead_code)]
fn showcase() -> (Scene, Camera) {
    let materials = vec![
        Material::Checker {
            even: grey(0.8),
            odd: grey(0.2),
            cells_per_unit: 1.0,
        },
        Material::Shiny {
            albedo: v3(0.1, 0.2, 0.7),
            strength: 0.8,
            sharpness: 60.0,
        },
        Material::Mirror { tint: grey(0.9) },
        Material::Glass {
            index: 1.5,
            absorb: v3(0.02, 0.1, 0.1),
        },
    ];
    let ball = |x: f64, z: f64, material| Prim {
        shape: Shape::Sphere {
            center: v3(x, 1.0, z),
            radius: 1.0,
        },
        material,
    };
    let mut prims = vec![ball(-2.2, 0.0, 1), ball(0.0, -1.0, 2), ball(2.2, 0.0, 3)];
    prims.extend(icosphere(v3(0.3, 0.5, 1.5), 0.5, 1, 1).unwrap());
    let lamps = vec![Lamp::Panel {
        corner: v3(2.0, 5.0, 2.0),
        edge_u: v3(1.5, 0.0, 0.0),
        edge_v: v3(0.0, 0.0, 1.5),
        power: grey(40.0),
    }];
    let floor = Plane {
        normal: v3(0.0, 1.0, 0.0),
        offset: 0.0,
        material: 0,
    };
    let scene = Scene {
        world: World::new(prims, vec![floor]),
        materials,
        lamps,
        sky: Sky::Gradient,
    };
    (
        scene,
        camera(v3(0.0, 2.0, 7.0), v3(0.0, 1.0, 0.0), 40.0, 1.5),
    )
}

#[allow(dead_code)]
fn bits(picture: &[Vec3]) -> Vec<u64> {
    picture
        .iter()
        .flat_map(|p| [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()])
        .collect()
}

#[test]
fn an_even_sky_gives_an_even_picture() {
    let colour = v3(0.2, 0.4, 0.6);
    for samples in [1, 3, 4] {
        let picture = render(
            &empty(Sky::Uniform(colour)),
            &ahead(),
            &settings(5, 4, samples, PATH),
        )
        .unwrap();
        check!(format!("{samples} samples: 20 pixels"), picture.len(), 20);
        check!(
            format!("{samples} samples: all the sky colour"),
            picture.iter().all(|&p| (p - colour).length() < 1e-12),
            true
        );
    }
}

#[test]
fn rows_from_the_top_pixels_from_the_left() {
    // A glowing ball up and to the left.
    let ball = Prim {
        shape: Shape::Sphere {
            center: v3(-2.0, 2.0, -3.0),
            radius: 0.8,
        },
        material: 0,
    };
    let scene = Scene {
        world: World::new(vec![ball], vec![]),
        materials: vec![Material::Light {
            radiance: grey(1.0),
        }],
        lamps: vec![],
        sky: Sky::Black,
    };
    let picture = render(&scene, &ahead(), &settings(9, 9, 4, WHITTED)).unwrap();
    check!("bright at column 1, row 1", picture[9 + 1].x > 0.9, true);
    check!(
        "dark in the other corners",
        [picture[16].x, picture[64].x, picture[80].x],
        [0.0, 0.0, 0.0]
    );
}

#[test]
fn the_error_between_two_pictures() {
    let a = [grey(0.0), grey(1.0)];
    let b = [grey(0.0), v3(1.0, 1.0, 4.0)];
    check!(
        "sqrt(9 / 6)",
        near(rmse(&a, &b).unwrap(), (1.5f64).sqrt(), 1e-12),
        true
    );
    check!("with itself", rmse(&a, &a), Some(0.0));
    check!("different sizes", rmse(&a, &b[..1]), None);
}

#[test]
fn the_number_of_threads_does_not_change_the_picture() {
    let (scene, camera) = showcase();
    let one = render(
        &scene,
        &camera,
        &Settings {
            threads: 1,
            ..settings(15, 10, 4, WHITTED)
        },
    )
    .unwrap();
    let four = render(
        &scene,
        &camera,
        &Settings {
            threads: 4,
            ..settings(15, 10, 4, WHITTED)
        },
    )
    .unwrap();
    check!("the very same numbers", bits(&one) == bits(&four), true);
}

#[test]
fn settings_that_make_no_picture() {
    let scene = empty(Sky::Black);
    for (w, h, n) in [(0, 4, 1), (4, 0, 1), (4, 4, 0)] {
        check!(
            format!("{w} x {h}, {n} samples"),
            render(&scene, &ahead(), &settings(w, h, n, PATH)).is_none(),
            true
        );
    }
}
