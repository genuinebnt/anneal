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
fn absurd_sizes_are_refused_before_any_work() {
    let scene = empty(Sky::Black);
    for (w, h, n) in [
        (usize::MAX, 2, 1),
        (2, usize::MAX, 1),
        (1 << 40, 1 << 40, 1),
        (MAX_PIXELS + 1, 1, 1),
        (4097, 4096, 1),
        (1, 1, MAX_SAMPLES + 1),
        (1, 1, usize::MAX),
    ] {
        check!(
            format!("{w} x {h}, {n} samples"),
            render(&scene, &ahead(), &settings(w, h, n, PATH)).is_none(),
            true
        );
    }
    let picture = render(&scene, &ahead(), &settings(1, 1, MAX_SAMPLES, PATH));
    check!(
        "the largest sample count is allowed",
        picture.is_some(),
        true
    );
}

#[test]
fn any_thread_count_gives_the_very_same_numbers() {
    let (sharp, sharp_camera) = showcase();
    let (boxed, boxed_camera) = cornell();
    for (scene, camera, integrator) in [(sharp, sharp_camera, WHITTED), (boxed, boxed_camera, PATH)]
    {
        let base = Settings {
            threads: 1,
            ..settings(17, 13, 5, integrator)
        };
        let want = bits(&render(&scene, &camera, &base).unwrap());
        for threads in [0, 2, 3, 7, 13, 64, 1000, usize::MAX] {
            let got = render(&scene, &camera, &Settings { threads, ..base }).unwrap();
            check!(
                format!("{integrator:?} with {threads} threads"),
                bits(&got) == want,
                true
            );
        }
        let again = render(&scene, &camera, &base).unwrap();
        check!("twice the same", bits(&again) == want, true);
        let other = render(&scene, &camera, &Settings { seed: 8, ..base }).unwrap();
        check!("another seed, another noise", bits(&other) != want, true);
    }
}

#[test]
fn every_pixel_has_its_own_luck() {
    // Inside a round room every pixel has the same true colour, so their noise should differ.
    let materials = vec![
        Material::Matte { albedo: grey(0.8) },
        Material::Light {
            radiance: grey(1.0),
        },
    ];
    let ball = |radius, material| Prim {
        shape: Shape::Sphere {
            center: grey(0.0),
            radius,
        },
        material,
    };
    let scene = Scene {
        world: World::new(vec![ball(2.0, 0), ball(0.5, 1)], vec![]),
        materials,
        lamps: vec![],
        sky: Sky::Black,
    };
    let look = camera(v3(0.0, 1.5, 0.0), v3(0.0, 2.0, 0.0001), 20.0, 1.0);
    for threads in [1, 4] {
        let picture = render(
            &scene,
            &look,
            &Settings {
                threads,
                ..settings(16, 16, 16, PATH)
            },
        )
        .unwrap();
        let mut seen: Vec<u64> = picture.iter().map(|p| p.x.to_bits()).collect();
        seen.sort();
        seen.dedup();
        check!(
            format!("many different pixels, got {}", seen.len()),
            seen.len() > 20,
            true
        );
        let rows: Vec<Vec<u64>> = picture.chunks(16).map(bits).collect();
        let columns: Vec<Vec<u64>> = (0..16)
            .map(|x| {
                bits(
                    &picture
                        .iter()
                        .skip(x)
                        .step_by(16)
                        .copied()
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        for list in [rows, columns] {
            let mut unique = list.clone();
            unique.sort();
            unique.dedup();
            check!("no two rows or columns alike", unique.len(), list.len());
        }
    }
}

#[test]
fn edges_are_smooth() {
    // A glowing floor edge: the left part of each row is sky 0, the right part glows. The edge crosses column 4 at a third.
    let width = 9.0;
    let edge = (4.0 + 1.0 / 3.0) / width * 2.0 - 1.0;
    let sheet = quad(
        v3(edge, -3.0, -1.0),
        v3(3.0, -3.0, -1.0),
        v3(3.0, 3.0, -1.0),
        v3(edge, 3.0, -1.0),
        0,
    );
    let scene = Scene {
        world: World::new(sheet.to_vec(), vec![]),
        materials: vec![Material::Light {
            radiance: grey(1.0),
        }],
        lamps: vec![],
        sky: Sky::Black,
    };
    for integrator in [WHITTED, PATH] {
        let picture = render(&scene, &ahead(), &settings(9, 9, 256, integrator)).unwrap();
        for y in 0..9 {
            let row = &picture[y * 9..y * 9 + 9];
            check!(
                format!("row {y}: dark left of the edge"),
                row[..4].iter().all(|p| p.x == 0.0),
                true
            );
            check!(
                format!("row {y}: lit right of it"),
                row[5..].iter().all(|p| (p.x - 1.0).abs() < 1e-12),
                true
            );
            check!(
                format!("row {y}: two thirds lit, got {}", row[4].x),
                near(row[4].x, 2.0 / 3.0, 0.05),
                true
            );
        }
    }
}

#[test]
fn the_lens_blurs_what_is_not_in_focus() {
    let sheet = quad(
        v3(0.0, -9.0, -5.0),
        v3(9.0, -9.0, -5.0),
        v3(9.0, 9.0, -5.0),
        v3(0.0, 9.0, -5.0),
        0,
    );
    let scene = Scene {
        world: World::new(sheet.to_vec(), vec![]),
        materials: vec![Material::Light {
            radiance: grey(1.0),
        }],
        lamps: vec![],
        sky: Sky::Black,
    };
    let spec = |lens_diameter, focus_distance| CameraSpec {
        position: grey(0.0),
        look_at: v3(0.0, 0.0, -1.0),
        up: v3(0.0, 1.0, 0.0),
        vfov_degrees: 40.0,
        aspect: 1.0,
        lens_diameter,
        focus_distance,
    };
    let partly = |s: CameraSpec| {
        let picture = render(
            &scene,
            &Camera::new(&s).unwrap(),
            &settings(16, 1, 64, WHITTED),
        )
        .unwrap();
        picture.iter().filter(|p| p.x > 0.05 && p.x < 0.95).count()
    };
    check!("pinhole: sharp", partly(spec(0.0, 1.0)), 0);
    check!(
        "lens focused on the sheet: sharp",
        partly(spec(1.0, 5.0)),
        0
    );
    check!(
        "lens focused far in front: a soft edge",
        partly(spec(1.0, 1.0)) >= 4,
        true
    );
}

#[test]
fn more_samples_less_noise_in_the_cornell_box() {
    let (scene, camera) = cornell();
    let base = Settings {
        threads: 4,
        ..settings(12, 12, 256, Integrator::Path { max_bounces: 8 })
    };
    let reference = render(&scene, &camera, &Settings { seed: 999, ..base }).unwrap();
    check!(
        "finite and never negative",
        reference
            .iter()
            .all(|p| [p.x, p.y, p.z].iter().all(|v| v.is_finite() && *v >= 0.0)),
        true
    );
    let error = |samples| {
        let picture = render(
            &scene,
            &camera,
            &Settings {
                samples,
                seed: 31337,
                ..base
            },
        )
        .unwrap();
        rmse(&picture, &reference).unwrap()
    };
    let (one, sixteen) = (error(1), error(16));
    // 16 times the samples should give about a quarter of the error.
    check!(
        format!("1 sample: {one}, 16 samples: {sixteen}"),
        sixteen < 0.5 * one,
        true
    );
}

#[test]
fn the_red_wall_is_on_the_left() {
    let (scene, camera) = cornell();
    let picture = render(&scene, &camera, &settings(12, 12, 36, PATH)).unwrap();
    // The light of six pixels at each side, added up, to even out the noise.
    let side = |x: usize| (3..9).fold(grey(0.0), |sum, y| sum + picture[y * 12 + x]);
    let (left, right) = (side(0), side(11));
    check!(
        format!("left {left:?} right {right:?}"),
        left.x > 2.0 * left.y && right.y > 1.5 * right.x,
        true
    );
}

#[test]
fn random_errors() {
    let mut rng = Rng::new(1691);
    for _ in 0..200 {
        let n = rng.below(20) + 1;
        let mut make = |_| {
            v3(
                rng.int(-50, 50) as f64,
                rng.int(-50, 50) as f64,
                rng.int(-50, 50) as f64,
            ) / 10.0
        };
        let a: Vec<Vec3> = (0..n).map(&mut make).collect();
        let b: Vec<Vec3> = (0..n).map(&mut make).collect();
        let sum: f64 = a
            .iter()
            .zip(&b)
            .map(|(p, q)| (p.x - q.x).powi(2) + (p.y - q.y).powi(2) + (p.z - q.z).powi(2))
            .sum();
        check!(
            format!("{a:?} {b:?}"),
            near(rmse(&a, &b).unwrap(), (sum / (3 * n) as f64).sqrt(), 1e-12),
            true
        );
    }
    check!("no pixels", rmse(&[], &[]), None);
}

#[test]
fn from_scene_to_file() {
    let (scene, camera) = showcase();
    let picture = render(&scene, &camera, &settings(30, 20, 4, WHITTED)).unwrap();
    let png = encode_png(30, 20, &to_rgb8(&picture)).unwrap();
    check!(
        "a PNG of the right size",
        (&png[1..4], &png[16..24]),
        (&b"PNG"[..], &[0, 0, 0, 30, 0, 0, 0, 20][..])
    );
}
