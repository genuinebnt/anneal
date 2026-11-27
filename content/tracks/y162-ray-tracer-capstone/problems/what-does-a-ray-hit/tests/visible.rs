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
fn vectors_add_up() {
    let (a, b) = (v3(1.0, 2.0, 3.0), v3(4.0, -5.0, 6.0));
    check!(
        "a + b, a - b, -a",
        (a + b, a - b, -a),
        (
            v3(5.0, -3.0, 9.0),
            v3(-3.0, 7.0, -3.0),
            v3(-1.0, -2.0, -3.0)
        )
    );
    check!(
        "a * 2, a / 2",
        (a * 2.0, a / 2.0),
        (v3(2.0, 4.0, 6.0), v3(0.5, 1.0, 1.5))
    );
    check!("dot", a.dot(b), 12.0);
    check!("cross", a.cross(b), v3(27.0, 6.0, -13.0));
    check!("times", a.times(b), v3(4.0, -10.0, 18.0));
    check!(
        "axis and max_part",
        (b.axis(0), b.axis(1), b.axis(2), b.max_part()),
        (4.0, -5.0, 6.0, 6.0)
    );
    check!(
        "unit of (3, 0, 4)",
        vnear(v3(3.0, 0.0, 4.0).unit(), v3(0.6, 0.0, 0.8), 1e-12),
        true
    );
    check!(
        "at",
        ray((1.0, 0.0, 0.0), (0.0, 2.0, 0.0)).at(1.5),
        v3(1.0, 3.0, 0.0)
    );
}

#[test]
fn a_ball_in_front() {
    let r = ray((0.0, 0.0, 0.0), (0.0, 0.0, -1.0));
    check!(
        "front side at 4",
        hits_at(ball(0.0, 0.0, -5.0, 1.0).hit(&r, 1e-4, 100.0), 4.0),
        true
    );
    check!(
        "past the front side: the far side at 6",
        hits_at(ball(0.0, 0.0, -5.0, 1.0).hit(&r, 4.5, 100.0), 6.0),
        true
    );
    check!(
        "nothing before 3",
        ball(0.0, 0.0, -5.0, 1.0).hit(&r, 1e-4, 3.0),
        None
    );
    check!(
        "behind the ray",
        ball(0.0, 0.0, 5.0, 1.0).hit(&r, 1e-4, 100.0),
        None
    );
}

#[test]
fn the_direction_need_not_have_length_one() {
    let r = ray((0.0, 0.0, 0.0), (0.0, 0.0, -2.0));
    check!(
        "t counts in lengths of dir",
        hits_at(ball(0.0, 0.0, -5.0, 1.0).hit(&r, 1e-4, 100.0), 2.0),
        true
    );
}

#[test]
fn a_triangle_and_a_plane() {
    let tri = Shape::Triangle {
        a: v3(0.0, 0.0, -3.0),
        b: v3(4.0, 0.0, -3.0),
        c: v3(0.0, 4.0, -3.0),
    };
    check!(
        "through the inside",
        hits_at(
            tri.hit(&ray((1.0, 1.0, 0.0), (0.0, 0.0, -1.0)), 1e-4, 100.0),
            3.0
        ),
        true
    );
    check!(
        "from the back too",
        hits_at(
            tri.hit(&ray((1.0, 1.0, -5.0), (0.0, 0.0, 1.0)), 1e-4, 100.0),
            2.0
        ),
        true
    );
    check!(
        "beside it",
        tri.hit(&ray((3.0, 3.0, 0.0), (0.0, 0.0, -1.0)), 1e-4, 100.0),
        None
    );
    check!(
        "normal",
        vnear(tri.normal_at(v3(1.0, 1.0, -3.0)), v3(0.0, 0.0, 1.0), 1e-12),
        true
    );
    let floor = Plane {
        normal: v3(0.0, 2.0, 0.0),
        offset: -2.0,
        material: 0,
    };
    check!(
        "the plane 2y = -2",
        hits_at(
            floor.hit(&ray((0.0, 0.0, 0.0), (0.0, -1.0, 0.0)), 1e-4, 100.0),
            1.0
        ),
        true
    );
    check!(
        "parallel to it",
        floor.hit(&ray((0.0, 0.0, 0.0), (1.0, 0.0, 0.0)), 1e-4, 100.0),
        None
    );
}

#[test]
fn the_world_gives_the_nearest() {
    let world = World::new(
        vec![
            prim(ball(0.0, 0.0, -9.0, 1.0), 7),
            prim(ball(0.0, 0.0, -5.0, 1.0), 3),
        ],
        vec![Plane {
            normal: v3(0.0, 0.0, 1.0),
            offset: -20.0,
            material: 9,
        }],
    );
    let r = ray((0.0, 0.0, 0.0), (0.0, 0.0, -1.0));
    let hit = world.closest(&r, 1e-4, 1000.0);
    check!("something is hit", hit.is_some(), true);
    let hit = hit.unwrap();
    check!("which", (hit.surface, hit.material), (Surface::Prim(1), 3));
    check!(
        "where",
        near(hit.t, 4.0, 1e-9) && vnear(hit.point, v3(0.0, 0.0, -4.0), 1e-9),
        true
    );
    check!("normal", vnear(hit.normal, v3(0.0, 0.0, 1.0), 1e-9), true);
    check!("blocked before 10", world.blocked(&r, 1e-4, 10.0), true);
    check!("not blocked before 4", world.blocked(&r, 1e-4, 3.9), false);
}
