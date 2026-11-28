#[allow(unused_imports)]
use anneal_prelude::Rng;
use solution::*;
use std::collections::{HashMap, HashSet};

/// Checks one answer against the rules. Positions are compared exactly: each triangle is laid out with its
/// own corners at (0, 0), (2, 0) and (0, 2).
#[allow(dead_code)]
fn judge(name: &str, points: usize, triangles: &[[usize; 3]]) -> Cut {
    let got = cut_in_four(points, triangles);
    check!(format!("{name}: accepted"), got.is_some(), true);
    let got = got.unwrap();
    let edges: HashSet<(usize, usize)> = triangles
        .iter()
        .flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])])
        .map(|(a, b)| (a.min(b), a.max(b)))
        .collect();
    let listed: Vec<(usize, usize)> = got
        .halfway
        .iter()
        .map(|&(a, b)| (a.min(b), a.max(b)))
        .collect();
    check!(
        format!("{name}: one new point for each edge"),
        listed.len(),
        edges.len()
    );
    check!(
        format!("{name}: and no edge twice or made up"),
        listed.iter().copied().collect::<HashSet<_>>() == edges,
        true
    );
    let number: HashMap<(usize, usize), usize> = listed
        .iter()
        .enumerate()
        .map(|(i, &e)| (e, points + i))
        .collect();
    check!(
        format!("{name}: four pieces each"),
        got.triangles.len(),
        4 * triangles.len()
    );
    for (k, &[a, b, c]) in triangles.iter().enumerate() {
        let m = |p: usize, q: usize| number[&(p.min(q), p.max(q))];
        let place: HashMap<usize, (i32, i32)> = [
            (a, (0, 0)),
            (b, (2, 0)),
            (c, (0, 2)),
            (m(a, b), (1, 0)),
            (m(b, c), (1, 1)),
            (m(c, a), (0, 1)),
        ]
        .into_iter()
        .collect();
        let want: HashSet<[usize; 3]> = [
            [a, m(a, b), m(c, a)],
            [b, m(b, c), m(a, b)],
            [c, m(c, a), m(b, c)],
            [m(a, b), m(b, c), m(c, a)],
        ]
        .into_iter()
        .map(|mut t| {
            t.sort();
            t
        })
        .collect();
        let pieces = &got.triangles[4 * k..4 * k + 4];
        let have: HashSet<[usize; 3]> = pieces
            .iter()
            .map(|&p| {
                let mut t = p;
                t.sort();
                t
            })
            .collect();
        check!(
            format!("{name}: the pieces of triangle {k}: {pieces:?}"),
            have == want,
            true
        );
        for piece in pieces {
            let [p, q, r] = piece.map(|v| place[&v]);
            let turn = (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0);
            check!(
                format!("{name}: piece {piece:?} of triangle {k} turns the same way as its parent"),
                turn > 0,
                true
            );
        }
    }
    got
}

#[allow(dead_code)]
const TETRAHEDRON: [[usize; 3]; 4] = [[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]];

/// Every edge of a closed mesh is walked once in each direction.
#[allow(dead_code)]
fn closed(triangles: &[[usize; 3]]) -> bool {
    let mut seen = HashSet::new();
    let fresh = triangles
        .iter()
        .all(|t| (0..3).all(|k| seen.insert((t[k], t[(k + 1) % 3]))));
    fresh && seen.iter().all(|&(a, b)| seen.contains(&(b, a)))
}

#[test]
fn cut_again_and_again() {
    let mut points = 4;
    let mut mesh = TETRAHEDRON.to_vec();
    for round in 1..=4 {
        let got = judge(&format!("round {round}"), points, &mesh);
        points += got.halfway.len();
        mesh = got.triangles;
        check!(format!("round {round}: closed"), closed(&mesh), true);
        // A closed surface without handles: corners - edges + faces = 2.
        let edges = mesh.len() * 3 / 2;
        check!(
            format!(
                "round {round}: {points} points, {edges} edges, {} faces",
                mesh.len()
            ),
            points + mesh.len(),
            edges + 2
        );
    }
    check!("faces", mesh.len(), 4 * 4usize.pow(4));
}

#[test]
fn every_triangle_is_looked_at() {
    check!(
        "bad one last",
        cut_in_four(6, &[[0, 1, 2], [3, 4, 5], [0, 1, 6]]),
        None
    );
    for bad in [
        [1, 1, 2],
        [2, 1, 2],
        [usize::MAX, 0, 1],
        [0, usize::MAX, 1],
        [0, 1, usize::MAX],
    ] {
        check!(format!("{bad:?}"), cut_in_four(3, &[[0, 1, 2], bad]), None);
    }
}

#[test]
fn numbers_near_the_top() {
    // The old points fill almost every number: there is room for three more, then none.
    let top = usize::MAX - 3;
    let t = [top - 3, top - 2, top - 1];
    check!(
        "three new points still fit",
        cut_in_four(top, &[t]).map(|c| c.halfway.len()),
        Some(3)
    );
    check!("six do not", cut_in_four(top, &[t, [0, 1, 2]]), None);
}

#[test]
fn the_same_triangle_twice() {
    let got = judge("twice", 3, &[[0, 1, 2], [0, 1, 2]]);
    check!("still three edges", got.halfway.len(), 3);
}

#[test]
fn a_fan() {
    let fan: Vec<[usize; 3]> = (1..500).map(|i| [0, i, i + 1]).collect();
    judge("fan", 501, &fan);
}

#[test]
fn a_large_sheet() {
    let n = 60;
    let mut mesh = Vec::new();
    for row in 0..n {
        for column in 0..n {
            let a = row * (n + 1) + column;
            mesh.push([a, a + 1, a + n + 2]);
            mesh.push([a, a + n + 2, a + n + 1]);
        }
    }
    let got = judge("sheet", (n + 1) * (n + 1), &mesh);
    check!("new points", got.halfway.len(), 3 * n * n + 2 * n);
}

#[test]
fn random_soup() {
    let mut rng = Rng::new(1628);
    for case in 0..500 {
        let points = rng.int(3, 12) as usize;
        let count = rng.int(1, 12) as usize;
        let mesh: Vec<[usize; 3]> = (0..count)
            .map(|_| {
                let mut all: Vec<usize> = (0..points).collect();
                rng.shuffle(&mut all);
                [all[0], all[1], all[2]]
            })
            .collect();
        judge(
            &format!("case {case}: {points} points, {mesh:?}"),
            points,
            &mesh,
        );
    }
}

#[test]
fn unused_points_are_left_alone() {
    let got = judge("gaps", 100, &[[10, 50, 99]]);
    check!(
        "new numbers start after all old ones",
        got.triangles
            .iter()
            .flatten()
            .all(|&v| [10, 50, 99].contains(&v) || (100..103).contains(&v)),
        true
    );
}

#[test]
fn the_very_last_number_is_used() {
    // The three new points get the numbers usize::MAX - 2, usize::MAX - 1 and usize::MAX. All of them fit.
    let got = cut_in_four(usize::MAX - 2, &[[0, 1, 2]]);
    check!(
        "three new points, the last one numbered usize::MAX",
        got.as_ref().map(|c| (
            c.halfway.len(),
            c.triangles.iter().flatten().copied().max()
        )),
        Some((3, Some(usize::MAX)))
    );
}
