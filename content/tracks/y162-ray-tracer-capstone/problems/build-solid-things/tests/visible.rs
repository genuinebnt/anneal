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
fn a_quad_is_two_triangles() {
    let (a, b, c, d) = (
        v3(0.0, 0.0, 0.0),
        v3(1.0, 0.0, 0.0),
        v3(1.0, 1.0, 0.0),
        v3(0.0, 1.0, 0.0),
    );
    let q = quad(a, b, c, d, 5);
    check!("first", corners(&q[0]), [a, b, c]);
    check!("second", corners(&q[1]), [a, c, d]);
    check!("materials", (q[0].material, q[1].material), (5, 5));
}

#[test]
fn a_box_has_twelve_triangles_facing_out() {
    let mesh = cuboid(v3(0.0, 0.0, 0.0), v3(2.0, 3.0, 4.0), 0.0, 1);
    check!("count", mesh.len(), 12);
    let (points, faces) = welded(&mesh);
    check!("corners", points.len(), 8);
    check!("closed, all turning one way", is_closed(&faces), true);
    check!(
        "facing out: positive volume",
        near(volume(&mesh), 24.0, 1e-9),
        true
    );
    check!("area", near(area(&mesh), 52.0, 1e-9), true);
    for x in [-1.0, 1.0] {
        for y in [0.0, 3.0] {
            for z in [-2.0, 2.0] {
                check!(
                    format!("corner ({x}, {y}, {z})"),
                    points.iter().any(|&p| vnear(p, v3(x, y, z), 1e-9)),
                    true
                );
            }
        }
    }
}

#[test]
fn the_smallest_ball() {
    let mesh = icosphere(v3(0.0, 0.0, 0.0), 1.0, 0, 2).unwrap();
    check!("triangles", mesh.len(), 20);
    check!("material", mesh.iter().all(|p| p.material == 2), true);
    let (points, faces) = welded(&mesh);
    check!("corners", points.len(), 12);
    check!("closed", is_closed(&faces), true);
}

#[test]
fn one_round_of_cutting() {
    let mesh = icosphere(v3(1.0, 2.0, 3.0), 2.0, 1, 0).unwrap();
    check!("triangles", mesh.len(), 80);
    let (points, faces) = welded(&mesh);
    check!("corners", points.len(), 42);
    check!("closed", is_closed(&faces), true);
    check!(
        "all on the ball",
        points
            .iter()
            .all(|&p| near((p - v3(1.0, 2.0, 3.0)).length(), 2.0, 1e-9)),
        true
    );
}

#[test]
fn bad_requests() {
    check!(
        "too many rounds",
        icosphere(v3(0.0, 0.0, 0.0), 1.0, 7, 0).is_none(),
        true
    );
    check!(
        "radius 0",
        icosphere(v3(0.0, 0.0, 0.0), 0.0, 1, 0).is_none(),
        true
    );
}
