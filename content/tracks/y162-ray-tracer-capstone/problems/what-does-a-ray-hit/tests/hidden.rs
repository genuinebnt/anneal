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
fn ray(o: (f64, f64, f64), d: (f64, f64, f64)) -> Ray {
    Ray {
        origin: v3(o.0, o.1, o.2),
        dir: v3(d.0, d.1, d.2),
    }
}

#[allow(dead_code)]
fn ball(x: f64, y: f64, z: f64, radius: f64) -> Shape {
    Shape::Sphere {
        center: v3(x, y, z),
        radius,
    }
}

#[allow(dead_code)]
fn prim(shape: Shape, material: usize) -> Prim {
    Prim { shape, material }
}

#[allow(dead_code)]
fn hits_at(got: Option<f64>, want: f64) -> bool {
    got.is_some_and(|t| near(t, want, 1e-9 * want.abs().max(1.0)))
}

#[test]
fn from_inside_a_ball_the_far_side() {
    let r = ray((0.5, 0.0, 0.0), (1.0, 0.0, 0.0));
    check!(
        "inside",
        hits_at(ball(0.0, 0.0, 0.0, 2.0).hit(&r, 1e-4, 100.0), 1.5),
        true
    );
}

#[test]
fn the_interval_is_open() {
    let r = ray((0.0, 0.0, 0.0), (1.0, 0.0, 0.0));
    let s = ball(5.0, 0.0, 0.0, 1.0);
    check!(
        "tmin exactly on the first side",
        hits_at(s.hit(&r, 4.0, 100.0), 6.0),
        true
    );
    check!("tmax exactly on the first side", s.hit(&r, 1e-4, 4.0), None);
    let wall = Plane {
        normal: v3(1.0, 0.0, 0.0),
        offset: 4.0,
        material: 0,
    };
    check!("plane at tmax", wall.hit(&r, 1e-4, 4.0), None);
    check!("plane at tmin", wall.hit(&r, 4.0, 9.0), None);
}

#[test]
fn a_negative_radius_turns_the_normal_inward() {
    let s = ball(0.0, 0.0, -5.0, -1.0);
    let r = ray((0.0, 0.0, 0.0), (0.0, 0.0, -1.0));
    check!(
        "the same surface",
        hits_at(s.hit(&r, 1e-4, 100.0), 4.0),
        true
    );
    check!(
        "normal points in",
        vnear(s.normal_at(v3(0.0, 0.0, -4.0)), v3(0.0, 0.0, -1.0), 1e-9),
        true
    );
}

#[test]
fn a_ball_very_far_away() {
    for far in [1e8, 3e8] {
        let s = ball(0.0, 0.0, 0.0, 1.0);
        let miss = ray((1.5, 0.0, far), (0.0, 0.0, -1.0));
        check!(
            format!("passes 1.5 from the centre, {far:e} away"),
            s.hit(&miss, 1e-4, 1e12),
            None
        );
        let miss = ray((0.0, 1.01, far), (0.0, 0.0, -1.0));
        check!(
            format!("passes 1.01 from the centre, {far:e} away"),
            s.hit(&miss, 1e-4, 1e12),
            None
        );
        let through = ray((0.6, 0.0, far), (0.0, 0.0, -1.0));
        let got = s.hit(&through, 1e-4, 1e12);
        check!(
            format!("passes 0.6 from the centre, {far:e} away"),
            got.is_some_and(|t| near(t, far - 0.8, 1e-6)),
            true
        );
    }
}

#[test]
fn small_and_large_triangles() {
    for size in [1e-8, 1e-3, 1.0, 1e6] {
        let tri = Shape::Triangle {
            a: v3(0.0, 0.0, 0.0),
            b: v3(size, 0.0, 0.0),
            c: v3(0.0, size, 0.0),
        };
        let r = ray((0.25 * size, 0.25 * size, 1.0), (0.0, 0.0, -1.0));
        check!(
            format!("side {size:e}"),
            hits_at(tri.hit(&r, 1e-4, 100.0), 1.0),
            true
        );
        let r = ray((0.75 * size, 0.75 * size, 1.0), (0.0, 0.0, -1.0));
        check!(
            format!("side {size:e}, outside"),
            tri.hit(&r, 1e-4, 100.0),
            None
        );
    }
}

#[test]
fn flat_and_broken_input_is_never_hit() {
    let r = ray((0.0, 0.0, 1.0), (0.0, 0.0, -1.0));
    let line = Shape::Triangle {
        a: v3(-1.0, -1.0, 0.0),
        b: v3(0.0, 0.0, 0.0),
        c: v3(1.0, 1.0, 0.0),
    };
    check!("three corners on a line", line.hit(&r, 1e-4, 100.0), None);
    let dot = Shape::Triangle {
        a: v3(0.0, 0.0, 0.0),
        b: v3(0.0, 0.0, 0.0),
        c: v3(0.0, 0.0, 0.0),
    };
    check!("three equal corners", dot.hit(&r, 1e-4, 100.0), None);
    let edge_on = Shape::Triangle {
        a: v3(0.0, -1.0, 0.0),
        b: v3(0.0, 1.0, 0.0),
        c: v3(0.0, 0.0, 5.0),
    };
    check!("seen edge on", edge_on.hit(&r, 1e-4, 100.0), None);
    let nan = ball(f64::NAN, 0.0, 0.0, 1.0);
    check!("a NaN centre", nan.hit(&r, 1e-4, 100.0), None);
    let nan = Shape::Triangle {
        a: v3(f64::NAN, 0.0, 0.0),
        b: v3(1.0, 0.0, 0.0),
        c: v3(0.0, 1.0, 0.0),
    };
    check!("a NaN corner", nan.hit(&r, 1e-4, 100.0), None);
}

#[test]
fn random_rays_at_balls() {
    let mut rng = Rng::new(1611);
    for case in 0..600 {
        let mut c = || rng.int(-2000, 2000) as f64 / 100.0;
        let centre = v3(c(), c(), c());
        let origin = v3(c(), c(), c());
        let aim = v3(c(), c(), c());
        let radius = rng.int(1, 900) as f64 / 100.0;
        let dir = aim - origin;
        if dir.length() < 0.1 {
            continue;
        }
        let r = Ray { origin, dir };
        // How close the whole line comes to the centre.
        let along = (centre - origin).dot(dir) / dir.dot(dir);
        let gap = (r.at(along) - centre).length();
        let got = Shape::Sphere {
            center: centre,
            radius,
        }
        .hit(&r, 0.0, f64::INFINITY);
        let inside = (origin - centre).length() < radius;
        let name = format!("case {case}: centre {centre:?} radius {radius} {r:?} got {got:?}");
        if (gap - radius).abs() < 1e-6 || ((origin - centre).length() - radius).abs() < 1e-6 {
            continue;
        }
        let expected = inside || (gap < radius && along > 0.0);
        check!(name.clone(), got.is_some(), expected);
        if let Some(t) = got {
            let distance = (r.at(t) - centre).length();
            check!(name.clone(), near(distance, radius, 1e-7), true);
            // The first meeting: halfway there the ray is on the same side of the surface as at the start.
            check!(name, (r.at(t * 0.5) - centre).length() < radius, inside);
        }
    }
}

#[test]
fn random_rays_at_triangles() {
    let mut rng = Rng::new(1612);
    let mut tried = 0;
    for case in 0..600 {
        let mut c = || rng.int(-500, 500) as f64 / 100.0;
        let a = v3(c(), c(), c());
        let b = v3(c(), c(), c());
        let cc = v3(c(), c(), c());
        let origin = v3(c(), c(), c() + 20.0);
        let normal = (b - a).cross(cc - a);
        if normal.length() < 0.5 {
            continue;
        }
        // Aim at a point with chosen shares of the three corners.
        let u = rng.int(-40, 140) as f64 / 100.0;
        let v = rng.int(-40, 140) as f64 / 100.0;
        let target = a + (b - a) * u + (cc - a) * v;
        let dir = (target - origin) * 0.5;
        if (normal.unit().dot(dir.unit())).abs() < 0.05
            || u.abs() < 0.005
            || v.abs() < 0.005
            || (u + v - 1.0).abs() < 0.005
        {
            continue;
        }
        tried += 1;
        let got = Shape::Triangle { a, b, c: cc }.hit(&Ray { origin, dir }, 1e-4, 1000.0);
        let inside = u > 0.0 && v > 0.0 && u + v < 1.0;
        let name = format!("case {case}: {a:?} {b:?} {cc:?} aimed with shares {u} {v}");
        check!(name.clone(), got.is_some(), inside);
        if let Some(t) = got {
            check!(name, near(t, 2.0, 1e-7), true);
        }
    }
    check!("enough cases", tried > 300, true);
}

#[test]
fn a_tie_goes_to_the_lower_number() {
    let s = ball(0.0, 0.0, -5.0, 1.0);
    let world = World::new(
        vec![
            prim(ball(9.0, 0.0, 0.0, 1.0), 0),
            prim(s, 1),
            prim(s, 2),
            prim(s, 3),
        ],
        vec![],
    );
    let r = ray((0.0, 0.0, 0.0), (0.0, 0.0, -1.0));
    check!(
        "of three equal balls",
        world.closest(&r, 1e-4, 100.0).map(|h| h.surface),
        Some(Surface::Prim(1))
    );
    let p = Plane {
        normal: v3(0.0, 0.0, 1.0),
        offset: -3.0,
        material: 0,
    };
    let world = World::new(vec![], vec![p, p]);
    check!(
        "of two equal planes",
        world.closest(&r, 1e-4, 100.0).map(|h| h.surface),
        Some(Surface::Plane(0))
    );
}

#[test]
fn planes_and_prims_compete() {
    let r = ray((0.0, 0.0, 0.0), (0.0, 0.0, -1.0));
    let near_wall = Plane {
        normal: v3(0.0, 0.0, 3.0),
        offset: -6.0,
        material: 4,
    };
    let world = World::new(vec![prim(ball(0.0, 0.0, -5.0, 1.0), 1)], vec![near_wall]);
    let hit = world.closest(&r, 1e-4, 100.0).unwrap();
    check!(
        "the plane is nearer",
        (hit.surface, hit.material),
        (Surface::Plane(0), 4)
    );
    check!(
        "its normal has length 1",
        vnear(hit.normal, v3(0.0, 0.0, 1.0), 1e-12),
        true
    );
    check!("only a plane can block", world.blocked(&r, 1e-4, 3.0), true);
    check!("beyond both", world.blocked(&r, 7.0, 100.0), false);
    check!(
        "nothing in an empty world",
        World::new(vec![], vec![]).closest(&r, 1e-4, 100.0),
        None
    );
}

#[test]
fn the_normal_is_not_turned_to_the_ray() {
    let world = World::new(vec![prim(ball(0.0, 0.0, 0.0, 2.0), 0)], vec![]);
    let hit = world
        .closest(&ray((0.0, 0.0, 0.0), (1.0, 0.0, 0.0)), 1e-4, 100.0)
        .unwrap();
    check!(
        "from inside it still points out",
        vnear(hit.normal, v3(1.0, 0.0, 0.0), 1e-9),
        true
    );
}
