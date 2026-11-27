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
fn corners(p: &Prim) -> [Vec3; 3] {
    match p.shape {
        Shape::Triangle { a, b, c } => [a, b, c],
        Shape::Sphere { .. } => panic!("a mesh is made of triangles"),
    }
}

#[allow(dead_code)]
fn outward_normal(p: &Prim) -> Vec3 {
    let [a, b, c] = corners(p);
    (b - a).cross(c - a)
}

/// The corners of all triangles with equal points (within 1e-9) merged, and the triangles as numbers.
#[allow(dead_code)]
fn welded(mesh: &[Prim]) -> (Vec<Vec3>, Vec<[usize; 3]>) {
    let mut points: Vec<Vec3> = Vec::new();
    let faces = mesh
        .iter()
        .map(|p| {
            corners(p).map(|c| match points.iter().position(|&q| vnear(q, c, 1e-9)) {
                Some(i) => i,
                None => {
                    points.push(c);
                    points.len() - 1
                }
            })
        })
        .collect();
    (points, faces)
}

/// A surface without holes whose triangles all turn the same way: every edge is walked once in each direction.
#[allow(dead_code)]
fn is_closed(faces: &[[usize; 3]]) -> bool {
    let mut edges = std::collections::HashMap::new();
    for f in faces {
        for k in 0..3 {
            *edges.entry((f[k], f[(k + 1) % 3])).or_insert(0) += 1;
        }
    }
    edges
        .iter()
        .all(|(&(a, b), &n)| a != b && n == 1 && edges.get(&(b, a)) == Some(&1))
}

#[allow(dead_code)]
fn area(mesh: &[Prim]) -> f64 {
    mesh.iter().map(|p| outward_normal(p).length() / 2.0).sum()
}

#[allow(dead_code)]
fn volume(mesh: &[Prim]) -> f64 {
    mesh.iter()
        .map(|p| {
            let [a, b, c] = corners(p);
            a.dot(b.cross(c)) / 6.0
        })
        .sum()
}

#[test]
fn absurd_levels_are_refused_at_once() {
    for levels in [7, 20, 31, 32, u32::MAX] {
        check!(
            format!("levels {levels}"),
            icosphere(v3(0.0, 0.0, 0.0), 1.0, levels, 0).is_none(),
            true
        );
    }
    for radius in [-1.0, f64::NAN, f64::INFINITY] {
        check!(
            format!("radius {radius}"),
            icosphere(v3(0.0, 0.0, 0.0), radius, 1, 0).is_none(),
            true
        );
    }
}

#[test]
fn the_first_twelve_corners() {
    let mesh = icosphere(v3(0.0, 0.0, 0.0), 1.0, 0, 0).unwrap();
    let (points, _) = welded(&mesh);
    let g = (1.0 + 5f64.sqrt()) / 2.0;
    let s = (1.0 + g * g).sqrt();
    for a in [-1.0, 1.0] {
        for b in [-g, g] {
            for want in [
                v3(a / s, b / s, 0.0),
                v3(0.0, a / s, b / s),
                v3(b / s, 0.0, a / s),
            ] {
                check!(
                    format!("{want:?}"),
                    points.iter().any(|&p| vnear(p, want, 1e-9)),
                    true
                );
            }
        }
    }
    let side = 2.0 / s;
    check!(
        "twenty equal sides",
        mesh.iter().all(|p| {
            let [x, y, z] = corners(p);
            [(x - y).length(), (y - z).length(), (z - x).length()]
                .iter()
                .all(|&l| near(l, side, 1e-9))
        }),
        true
    );
}

#[test]
fn finer_balls_stay_closed_and_face_out() {
    let centre = v3(-3.0, 0.5, 7.0);
    let mut last_area = 0.0;
    for levels in 0..4u32 {
        let mesh = icosphere(centre, 0.5, levels, 0).unwrap();
        let n = 4usize.pow(levels);
        check!(format!("level {levels}: triangles"), mesh.len(), 20 * n);
        let (points, faces) = welded(&mesh);
        check!(
            format!("level {levels}: shared corners"),
            points.len(),
            10 * n + 2
        );
        check!(format!("level {levels}: closed"), is_closed(&faces), true);
        check!(
            format!("level {levels}: on the ball"),
            points
                .iter()
                .all(|&p| near((p - centre).length(), 0.5, 1e-9)),
            true
        );
        check!(
            format!("level {levels}: facing out"),
            mesh.iter()
                .all(|p| outward_normal(p).dot(corners(p)[0] - centre) > 0.0),
            true
        );
        // The mesh grows towards the true ball.
        let now = area(&mesh);
        check!(
            format!("level {levels}: area grows"),
            now > last_area && now < std::f64::consts::PI,
            true
        );
        last_area = now;
    }
}

#[test]
fn six_rounds() {
    let mesh = icosphere(v3(0.0, 0.0, 0.0), 1.0, 6, 0).unwrap();
    check!("triangles", mesh.len(), 20 * 4096);
}

#[test]
fn turning_a_box() {
    // A quarter turn swaps the footprint.
    let mesh = cuboid(
        v3(0.0, 0.0, 0.0),
        v3(2.0, 1.0, 6.0),
        std::f64::consts::FRAC_PI_2,
        0,
    );
    let (points, _) = welded(&mesh);
    check!(
        "footprint",
        points
            .iter()
            .all(|p| near(p.x.abs(), 3.0, 1e-9) && near(p.z.abs(), 1.0, 1e-9)),
        true
    );
    // The direction of the turn: the corner (+1, 0, 0) of a thin rod swings towards -z.
    let mesh = cuboid(v3(0.0, 0.0, 0.0), v3(10.0, 1.0, 0.0001), 0.3, 0);
    let (points, _) = welded(&mesh);
    let far_end = points.iter().find(|p| p.x > 0.0).unwrap();
    check!("a positive yaw takes +x towards -z", far_end.z < -1.0, true);
}

#[test]
fn random_boxes() {
    let mut rng = Rng::new(1621);
    for case in 0..200 {
        let base = v3(
            rng.int(-50, 50) as f64,
            rng.int(-50, 50) as f64,
            rng.int(-50, 50) as f64,
        );
        let size = v3(
            rng.int(1, 9) as f64,
            rng.int(1, 9) as f64,
            rng.int(1, 9) as f64,
        );
        let yaw = rng.int(-700, 700) as f64 / 100.0;
        let mesh = cuboid(base, size, yaw, 3);
        let name = format!("case {case}: base {base:?} size {size:?} yaw {yaw}");
        let (points, faces) = welded(&mesh);
        check!(
            name.clone(),
            (mesh.len(), points.len(), is_closed(&faces)),
            (12, 8, true)
        );
        check!(
            name.clone(),
            near(
                volume(&shifted(&mesh, base)),
                size.x * size.y * size.z,
                1e-7
            ),
            true
        );
        check!(name.clone(), mesh.iter().all(|p| p.material == 3), true);
        // In the box's own directions every corner is at a corner of the size.
        let (sin, cos) = yaw.sin_cos();
        for p in points {
            let d = p - base;
            let own = (d.x * cos - d.z * sin, d.y, d.x * sin + d.z * cos);
            let ok = near(own.0.abs(), size.x / 2.0, 1e-9)
                && near(own.2.abs(), size.z / 2.0, 1e-9)
                && (near(own.1, 0.0, 1e-9) || near(own.1, size.y, 1e-9));
            check!(name.clone(), ok, true);
        }
    }
}

fn shifted(mesh: &[Prim], by: Vec3) -> Vec<Prim> {
    mesh.iter()
        .map(|p| {
            let [a, b, c] = corners(p).map(|v| v - by);
            Prim {
                shape: Shape::Triangle { a, b, c },
                material: p.material,
            }
        })
        .collect()
}

#[test]
fn a_moved_and_scaled_ball_keeps_the_shape() {
    let unit = icosphere(v3(0.0, 0.0, 0.0), 1.0, 2, 0).unwrap();
    let moved = icosphere(v3(10.0, -20.0, 30.0), 4.0, 2, 0).unwrap();
    check!(
        "area scales by 16",
        near(area(&moved), 16.0 * area(&unit), 1e-6),
        true
    );
    let (points, _) = welded(&moved);
    check!(
        "on the moved ball",
        points
            .iter()
            .all(|&p| near((p - v3(10.0, -20.0, 30.0)).length(), 4.0, 1e-9)),
        true
    );
}

#[test]
fn rays_see_the_mesh() {
    let mesh = icosphere(v3(0.0, 0.0, -5.0), 1.0, 2, 4).unwrap();
    let world = World::new(mesh, vec![]);
    let mut rng = Rng::new(1622);
    for case in 0..100 {
        let aim = v3(
            rng.int(-60, 60) as f64 / 100.0,
            rng.int(-60, 60) as f64 / 100.0,
            -5.0,
        );
        let ray = Ray {
            origin: v3(0.0, 0.0, 0.0),
            dir: aim,
        };
        let hit = world.closest(&ray, 1e-4, 100.0);
        let name = format!("case {case}: aim {aim:?}");
        check!(name.clone(), hit.is_some(), true);
        let hit = hit.unwrap();
        check!(
            name,
            (hit.point - v3(0.0, 0.0, -5.0)).length() > 0.97 && hit.normal.dot(ray.dir) < 0.0,
            true
        );
    }
}
