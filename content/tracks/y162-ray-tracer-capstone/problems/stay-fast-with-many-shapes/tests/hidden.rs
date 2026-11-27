#[allow(unused_imports)]
use anneal_prelude::Rng;
use solution::*;

#[allow(dead_code)]
fn ball(center: Vec3, radius: f64) -> Prim {
    Prim {
        shape: Shape::Sphere { center, radius },
        material: 0,
    }
}

#[allow(dead_code)]
fn tri(a: Vec3, b: Vec3, c: Vec3) -> Prim {
    Prim {
        shape: Shape::Triangle { a, b, c },
        material: 0,
    }
}

#[allow(dead_code)]
fn ray(origin: Vec3, dir: Vec3) -> Ray {
    Ray { origin, dir }
}

/// Counts every question the tree asks.
#[allow(dead_code)]
struct Counting<'a> {
    inner: RayProbe<'a>,
    boxes: usize,
    solids: usize,
}

impl Probe for Counting<'_> {
    fn enter(&mut self, low: Vec3, high: Vec3, tmin: f64, tmax: f64) -> Option<f64> {
        self.boxes += 1;
        self.inner.enter(low, high, tmin, tmax)
    }

    fn hit(&mut self, i: usize, tmin: f64, tmax: f64) -> Option<f64> {
        self.solids += 1;
        self.inner.hit(i, tmin, tmax)
    }
}

#[allow(dead_code)]
fn tree_of(prims: &[Prim]) -> Finder {
    let boxes: Vec<(Vec3, Vec3)> = prims.iter().map(Prim::bounds).collect();
    Finder::build(&boxes)
}

/// The nearest hit and how many questions were asked.
#[allow(dead_code)]
fn counted(tree: &Finder, prims: &[Prim], r: Ray, tmax: f64) -> (Option<(f64, usize)>, usize) {
    let mut probe = Counting {
        inner: RayProbe { prims, ray: r },
        boxes: 0,
        solids: 0,
    };
    let got = tree.closest(&mut probe, 1e-4, tmax);
    (got, probe.boxes + probe.solids)
}

/// The same question answered by trying every solid.
#[allow(dead_code)]
fn every_one(prims: &[Prim], r: &Ray, tmin: f64, tmax: f64) -> Option<(f64, usize)> {
    let mut best: Option<(f64, usize)> = None;
    for (i, p) in prims.iter().enumerate() {
        if let Some(t) = p.shape.hit(r, tmin, tmax) {
            if best.is_none_or(|b| t < b.0) {
                best = Some((t, i));
            }
        }
    }
    best
}

#[allow(dead_code)]
fn compare(name: &str, prims: &[Prim], rays: &[Ray]) -> usize {
    let tree = tree_of(prims);
    let mut hits = 0;
    for (case, r) in rays.iter().enumerate() {
        for tmax in [f64::INFINITY, 25.0] {
            let want = every_one(prims, r, 1e-4, tmax);
            let label = format!("{name}, ray {case}: {r:?} up to {tmax}");
            let mut probe = RayProbe { prims, ray: *r };
            check!(label.clone(), tree.closest(&mut probe, 1e-4, tmax), want);
            check!(label, tree.any(&mut probe, 1e-4, tmax), want.is_some());
            hits += usize::from(want.is_some());
        }
    }
    hits
}

#[allow(dead_code)]
fn line_of_balls(axis: usize, n: usize) -> Vec<Prim> {
    (0..n)
        .map(|i| {
            let mut c = [0.0; 3];
            c[axis] = 3.0 * i as f64;
            ball(v3(c[0], c[1], c[2]), 1.0)
        })
        .collect()
}

#[allow(dead_code)]
fn axis_vector(axis: usize, length: f64) -> Vec3 {
    let mut c = [0.0; 3];
    c[axis] = length;
    v3(c[0], c[1], c[2])
}

#[allow(dead_code)]
fn point(rng: &mut Rng, reach: i64) -> Vec3 {
    v3(
        rng.int(-reach, reach) as f64,
        rng.int(-reach, reach) as f64,
        rng.int(-reach, reach) as f64,
    )
}

#[test]
fn rows_along_every_axis_and_from_both_ends() {
    for axis in 0..3 {
        let prims = line_of_balls(axis, 4096);
        let tree = tree_of(&prims);
        let forward = ray(axis_vector(axis, -10.0), axis_vector(axis, 1.0));
        let backward = ray(
            axis_vector(axis, 3.0 * 4095.0 + 10.0),
            axis_vector(axis, -1.0),
        );
        for (r, first) in [(forward, 0), (backward, 4095)] {
            let (got, asked) = counted(&tree, &prims, r, f64::INFINITY);
            check!(
                format!("axis {axis}, toward ball {first}"),
                got.map(|g| g.1),
                Some(first)
            );
            check!(
                format!("axis {axis}, toward ball {first}: asked {asked}, at most 150"),
                asked <= 150,
                true
            );
            let mut probe = Counting {
                inner: RayProbe {
                    prims: &prims,
                    ray: r,
                },
                boxes: 0,
                solids: 0,
            };
            check!("any", tree.any(&mut probe, 1e-4, f64::INFINITY), true);
            check!(
                format!("any: asked {}, at most 150", probe.boxes + probe.solids),
                probe.boxes + probe.solids <= 150,
                true
            );
        }
        let (got, asked) = counted(
            &tree,
            &prims,
            ray(axis_vector(axis, -10.0), axis_vector(axis, -1.0)),
            f64::INFINITY,
        );
        check!(
            format!("axis {axis}, away from all: asked {asked}, at most 10"),
            (got, asked <= 10),
            (None, true)
        );
        let (got, asked) = counted(
            &tree,
            &prims,
            ray(axis_vector(axis, -10.0), axis_vector(axis, 1.0)),
            5.0,
        );
        check!(
            format!("axis {axis}, window ends before the row: asked {asked}, at most 10"),
            (got, asked <= 10),
            (None, true)
        );
    }
}

#[test]
fn a_cloud_asks_little() {
    let mut rng = Rng::new(1672);
    let prims: Vec<Prim> = (0..4096)
        .map(|_| ball(point(&mut rng, 1000) / 20.0, 0.4))
        .collect();
    let tree = tree_of(&prims);
    let mut total = 0;
    let mut hits = 0;
    for case in 0..300 {
        let from = point(&mut rng, 100);
        let toward = prims[rng.below(prims.len())].bounds().0 + v3(0.4, 0.4, 0.4);
        let r = ray(from, toward - from);
        let (got, asked) = counted(&tree, &prims, r, f64::INFINITY);
        if case < 100 {
            check!(
                format!("ray {case}"),
                got,
                every_one(&prims, &r, 1e-4, f64::INFINITY)
            );
        }
        total += asked;
        hits += usize::from(got.is_some());
    }
    check!("most rays hit", hits > 250, true);
    check!(
        format!(
            "average questions: {} (at most 400 of 4096 balls)",
            total / 300
        ),
        total / 300 <= 400,
        true
    );
}

#[test]
fn the_tree_only_believes_the_probe() {
    /// Says that no box is ever entered.
    struct Blind(usize);
    impl Probe for Blind {
        fn enter(&mut self, _: Vec3, _: Vec3, _: f64, _: f64) -> Option<f64> {
            None
        }
        fn hit(&mut self, _: usize, _: f64, _: f64) -> Option<f64> {
            self.0 += 1;
            Some(1.0)
        }
    }
    let tree = tree_of(&line_of_balls(0, 100));
    let mut blind = Blind(0);
    check!("no box, no hit", tree.closest(&mut blind, 0.0, 10.0), None);
    check!("and no solid is asked about", blind.0, 0);
}

#[test]
fn mixed_shapes_and_inward_balls() {
    let mut rng = Rng::new(1673);
    for round in 0..8 {
        let n = *rng.pick(&[1usize, 2, 5, 9, 33, 120]);
        let prims: Vec<Prim> = (0..n)
            .map(|_| {
                if rng.bool() {
                    let radius = rng.int(1, 4) as f64;
                    ball(
                        point(&mut rng, 12),
                        if rng.bool() { radius } else { -radius },
                    )
                } else {
                    let a = point(&mut rng, 12);
                    let b = a + point(&mut rng, 6);
                    tri(a, b, a + point(&mut rng, 6))
                }
            })
            .collect();
        let rays: Vec<Ray> = (0..150)
            .map(|_| {
                let from = point(&mut rng, 24);
                ray(from, point(&mut rng, 12) - from + v3(0.25, 0.0, 0.0))
            })
            .collect();
        compare(&format!("round {round} with {n} shapes"), &prims, &rays);
    }
}

#[test]
fn flat_walls_and_rays_along_faces() {
    // Axis-parallel squares on whole numbers, and rays along the axes through corners and edges.
    let mut rng = Rng::new(1674);
    let mut prims = Vec::new();
    for _ in 0..80 {
        let at = point(&mut rng, 6);
        let axis = rng.below(3);
        let (u, v) = (
            axis_vector((axis + 1) % 3, 2.0),
            axis_vector((axis + 2) % 3, 2.0),
        );
        prims.extend(quad(at, at + u, at + u + v, at + v, 0));
    }
    let mut rays = Vec::new();
    for _ in 0..600 {
        let axis = rng.below(3);
        let sign = *rng.pick(&[1.0, -1.0]);
        let mut dir = axis_vector(axis, sign);
        if rng.below(4) == 0 {
            dir = -axis_vector(axis, -sign); // the zeros are now -0.0
        }
        rays.push(ray(
            point(&mut rng, 8) - axis_vector(axis, 20.0 * sign),
            dir,
        ));
    }
    let hits = compare("walls", &prims, &rays);
    check!("enough of them hit", hits > 100, true);
}

#[test]
fn a_ray_running_along_a_face_counts() {
    let (low, high) = (v3(0.0, 0.0, 0.0), v3(2.0, 2.0, 0.0));
    for zero in [0.0, -0.0] {
        for x in [0.0, 2.0] {
            let r = ray(v3(x, 1.0, 5.0), v3(zero, zero, -1.0));
            check!(
                format!("x = {x}, zeros {zero}"),
                box_entry(low, high, &r, 0.0, 100.0),
                Some(5.0)
            );
        }
        let r = ray(v3(2.5, 1.0, 5.0), v3(zero, zero, -1.0));
        check!("beside it", box_entry(low, high, &r, 0.0, 100.0), None);
    }
}

#[test]
fn from_very_far_away() {
    let mut rng = Rng::new(1675);
    let prims: Vec<Prim> = (0..200)
        .map(|_| ball(point(&mut rng, 40) / 2.0, 0.5))
        .collect();
    let rays: Vec<Ray> = (0..300)
        .map(|_| {
            let from = (point(&mut rng, 100) + v3(0.5, 0.5, 0.5)).unit() * 1e8;
            ray(from, (point(&mut rng, 40) / 2.0 - from).unit())
        })
        .collect();
    compare("far", &prims, &rays);
}

#[test]
fn many_in_one_place() {
    let prims = vec![ball(v3(5.0, 0.0, 0.0), 1.0); 20_000];
    let tree = tree_of(&prims);
    let r = ray(v3(0.0, 0.0, 0.0), v3(1.0, 0.0, 0.0));
    let mut probe = RayProbe {
        prims: &prims,
        ray: r,
    };
    check!(
        "of equal hits, the lowest number",
        tree.closest(&mut probe, 1e-4, 100.0),
        Some((4.0, 0))
    );
}

#[test]
fn equal_hits_go_to_the_lowest_number() {
    // Two copies of 40 balls, the second set listed in reverse.
    let mut prims: Vec<Prim> = line_of_balls(0, 40);
    let mut again = prims.clone();
    again.reverse();
    prims.extend(again);
    let tree = tree_of(&prims);
    for i in 0..40 {
        let r = ray(v3(3.0 * i as f64, 10.0, 0.0), v3(0.0, -1.0, 0.0));
        let mut probe = RayProbe {
            prims: &prims,
            ray: r,
        };
        check!(
            format!("ball {i}"),
            tree.closest(&mut probe, 1e-4, 100.0).map(|g| g.1),
            Some(i)
        );
    }
}

#[test]
fn broken_numbers_do_not_panic() {
    let nan = f64::NAN;
    let mut prims = line_of_balls(0, 64);
    for i in [3, 17, 18, 40, 63] {
        prims[i] = ball(v3(nan, 0.0, 0.0), 1.0);
    }
    prims.push(tri(v3(nan, nan, nan), v3(nan, 0.0, 0.0), v3(0.0, nan, 0.0)));
    let rays: Vec<Ray> = (0..64)
        .map(|i| ray(v3(3.0 * i as f64, 10.0, 0.0), v3(0.0, -1.0, 0.0)))
        .collect();
    let hits = compare("with NaN", &prims, &rays);
    check!("the sound balls are still found", hits, 59 * 2);
}

#[test]
fn the_world_still_answers_the_same() {
    let mut rng = Rng::new(1676);
    let prims: Vec<Prim> = (0..60)
        .map(|i| Prim {
            material: i,
            ..ball(point(&mut rng, 10), 1.0)
        })
        .collect();
    let floor = Plane {
        normal: v3(0.0, 1.0, 0.0),
        offset: -12.0,
        material: 99,
    };
    let world = World::new(prims.clone(), vec![floor]);
    check!(
        "prims stay in order",
        world.prims() == prims.as_slice(),
        true
    );
    for case in 0..300 {
        let from = point(&mut rng, 25);
        let r = ray(from, point(&mut rng, 10) - from + v3(0.5, 0.0, 0.0));
        let prim = every_one(&prims, &r, 1e-4, 500.0);
        let wall = floor.hit(&r, 1e-4, prim.map_or(500.0, |p| p.0));
        let want = match (wall, prim) {
            (Some(t), _) => Some((t, Surface::Plane(0))),
            (None, Some((t, i))) => Some((t, Surface::Prim(i))),
            (None, None) => None,
        };
        check!(
            format!("ray {case}: {r:?}"),
            world.closest(&r, 1e-4, 500.0).map(|h| (h.t, h.surface)),
            want
        );
        check!(
            format!("ray {case}: blocked"),
            world.blocked(&r, 1e-4, 500.0),
            want.is_some()
        );
    }
}
