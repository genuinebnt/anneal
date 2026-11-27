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
fn into_a_box() {
    let (low, high) = (v3(1.0, -1.0, -1.0), v3(3.0, 1.0, 1.0));
    let forward = ray(v3(0.0, 0.0, 0.0), v3(1.0, 0.0, 0.0));
    check!(
        "comes in at 1",
        box_entry(low, high, &forward, 0.0, 100.0),
        Some(1.0)
    );
    check!(
        "starting inside: at tmin",
        box_entry(low, high, &forward, 2.0, 100.0),
        Some(2.0)
    );
    check!("too far", box_entry(low, high, &forward, 0.0, 0.5), None);
    check!("behind", box_entry(low, high, &forward, 4.0, 100.0), None);
    check!(
        "beside",
        box_entry(
            low,
            high,
            &ray(v3(0.0, 2.0, 0.0), v3(1.0, 0.0, 0.0)),
            0.0,
            100.0
        ),
        None
    );
}

#[test]
fn bounds_of_shapes() {
    check!(
        "ball",
        ball(v3(1.0, 2.0, 3.0), 2.0).bounds(),
        (v3(-1.0, 0.0, 1.0), v3(3.0, 4.0, 5.0))
    );
    check!(
        "triangle",
        tri(v3(0.0, 5.0, 1.0), v3(4.0, 0.0, 1.0), v3(2.0, 2.0, -1.0)).bounds(),
        (v3(0.0, 0.0, -1.0), v3(4.0, 5.0, 1.0))
    );
}

#[test]
fn nothing_at_all() {
    let tree = Finder::build(&[]);
    let mut probe = RayProbe {
        prims: &[],
        ray: ray(v3(0.0, 0.0, 0.0), v3(1.0, 0.0, 0.0)),
    };
    check!("closest", tree.closest(&mut probe, 0.0, 10.0), None);
    check!("any", tree.any(&mut probe, 0.0, 10.0), false);
}

#[test]
fn the_same_answer_as_trying_everything() {
    let mut rng = Rng::new(1671);
    let prims: Vec<Prim> = (0..50)
        .map(|_| ball(point(&mut rng, 10), rng.int(1, 3) as f64))
        .collect();
    let rays: Vec<Ray> = (0..100)
        .map(|_| {
            let from = point(&mut rng, 20);
            ray(from, point(&mut rng, 8) - from + v3(0.5, 0.5, 0.5))
        })
        .collect();
    compare("balls", &prims, &rays);
}

#[test]
fn a_long_row_asks_little() {
    let prims = line_of_balls(0, 4096);
    let tree = tree_of(&prims);
    let (got, asked) = counted(
        &tree,
        &prims,
        ray(v3(-10.0, 0.0, 0.0), v3(1.0, 0.0, 0.0)),
        f64::INFINITY,
    );
    check!("the first ball", got.map(|g| g.1), Some(0));
    check!(
        format!("at most 150 questions for 4096 balls, asked {asked}"),
        asked <= 150,
        true
    );
}
