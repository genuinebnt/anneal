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
fn one_triangle() {
    let got = judge("one", 3, &[[0, 1, 2]]);
    check!("three new points", got.halfway.len(), 3);
}

#[test]
fn two_triangles_share_a_middle() {
    let got = judge("two", 4, &[[0, 1, 2], [2, 1, 3]]);
    check!("five edges, five new points", got.halfway.len(), 5);
}

#[test]
fn nothing_to_cut() {
    check!(
        "empty",
        cut_in_four(10, &[]),
        Some(Cut {
            halfway: vec![],
            triangles: vec![]
        })
    );
}

#[test]
fn a_pyramid() {
    let got = judge("tetrahedron", 4, &TETRAHEDRON);
    check!("still closed", closed(&got.triangles), true);
}

#[test]
fn bad_numbers() {
    check!("no such point", cut_in_four(3, &[[0, 1, 3]]), None);
    check!("a corner twice", cut_in_four(3, &[[0, 1, 1]]), None);
}
