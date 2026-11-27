use solution::*;
use anneal_prelude::Rng;

#[allow(dead_code)]
fn dot(n: u32) -> Shape {
    Shape::Dot(n)
}
#[allow(dead_code)]
fn group(parts: Vec<Shape>) -> Shape {
    Shape::Group(parts)
}
#[allow(dead_code)]
fn moved(x: i64, y: i64, s: Shape) -> Shape {
    Shape::Move(x, y, Box::new(s))
}
#[allow(dead_code)]
fn hidden(s: Shape) -> Shape {
    Shape::Hidden(Box::new(s))
}

/// What a shape draws: each dot with where it lands, in drawing order.
#[allow(dead_code)]
fn paint(s: &Shape, x: i64, y: i64, out: &mut Vec<(u32, i64, i64)>) {
    match s {
        Shape::Dot(n) => out.push((*n, x, y)),
        Shape::Group(parts) => parts.iter().for_each(|p| paint(p, x, y, out)),
        Shape::Move(a, b, inner) => paint(inner, x + a, y + b, out),
        Shape::Hidden(_) => {}
    }
}

#[allow(dead_code)]
fn drawing(s: &Shape) -> Vec<(u32, i64, i64)> {
    let mut out = Vec::new();
    paint(s, 0, 0, &mut out);
    out
}

/// The first rule that still applies somewhere.
#[allow(dead_code)]
fn untidy(s: &Shape) -> Option<&'static str> {
    match s {
        Shape::Dot(_) => None,
        Shape::Hidden(_) => Some("something is still hidden"),
        Shape::Move(x, y, inner) => match &**inner {
            _ if (*x, *y) == (0, 0) => Some("a move by nothing"),
            Shape::Move(..) => Some("a move directly inside a move"),
            Shape::Group(p) if p.is_empty() => Some("a move of nothing"),
            other => untidy(other),
        },
        Shape::Group(parts) => {
            if parts.iter().any(|p| matches!(p, Shape::Group(_))) {
                return Some("a group directly inside a group");
            }
            if parts.len() == 1 {
                return Some("a group of one");
            }
            let moves: Vec<(i64, i64)> = parts.iter().filter_map(|p| match p {
                Shape::Move(x, y, _) => Some((*x, *y)),
                _ => None,
            }).collect();
            if parts.len() > 1 && moves.len() == parts.len() && moves.iter().all(|m| *m == moves[0]) {
                return Some("every part of a group makes the same move");
            }
            parts.iter().find_map(untidy)
        }
    }
}

#[allow(dead_code)]
fn random_shape(rng: &mut Rng, depth: usize) -> Shape {
    // few different moves, so that equal ones and cancelling ones are common
    let step = |rng: &mut Rng| (rng.int(-1, 1), rng.int(-1, 0));
    match rng.below(if depth == 0 { 1 } else { 8 }) {
        0 => dot(rng.below(9) as u32),
        1 => hidden(random_shape(rng, depth - 1)),
        2 | 3 => {
            let (x, y) = step(rng);
            moved(x, y, random_shape(rng, depth - 1))
        }
        4 => {
            // parts that share a move
            let (x, y) = step(rng);
            let n = rng.below(4);
            group((0..n).map(|_| moved(x, y, random_shape(rng, depth - 1))).collect())
        }
        _ => {
            let n = rng.below(4);
            group((0..n).map(|_| random_shape(rng, depth - 1)).collect())
        }
    }
}

#[test]
fn random_shapes_end_up_tidy_and_draw_the_same() {
    let mut rng = Rng::new(9131);
    for _ in 0..3000 {
        let shape = random_shape(&mut rng, 5);
        let result = tidy(shape.clone());
        let input = format!("tidy({shape:?}) = {result:?}");
        check!(format!("{input}: drawing"), drawing(&result), drawing(&shape));
        check!(format!("{input}: rules that still apply"), untidy(&result), None);
        check!(format!("{input}: tidying twice"), tidy(result.clone()), result.clone());
    }
}

#[test]
fn one_rule_makes_room_for_another() {
    // after the shared move comes out, the two inner moves are the same and come out too
    let before = group(vec![moved(1, 0, moved(0, 2, dot(1))), moved(1, 0, moved(0, 2, dot(2)))]);
    check!(format!("{before:?}"), tidy(before.clone()), moved(1, 2, group(vec![dot(1), dot(2)])));
    // taking a move out leaves groups that have to be opened
    let before = group(vec![moved(5, 5, group(vec![dot(1), dot(2)])), moved(5, 5, group(vec![dot(3), dot(4)]))]);
    check!(format!("{before:?}"), tidy(before.clone()), moved(5, 5, group(vec![dot(1), dot(2), dot(3), dot(4)])));
    // the moves left inside add up with the one taken out
    let before = moved(-3, 0, group(vec![moved(3, 0, dot(1)), moved(3, 0, dot(2))]));
    check!(format!("{before:?}"), tidy(before.clone()), group(vec![dot(1), dot(2)]));
}

#[test]
fn one_part_with_a_move_stays_a_move() {
    check!("a group of one move", tidy(group(vec![moved(1, 1, dot(7))])), moved(1, 1, dot(7)));
}

#[test]
fn a_move_by_nothing_made_by_adding() {
    let before = group(vec![dot(0), moved(4, 4, moved(-4, -4, group(vec![dot(1), dot(2)])))]);
    check!(format!("{before:?}"), tidy(before.clone()), group(vec![dot(0), dot(1), dot(2)]));
}

#[test]
fn two_hundred_levels() {
    let mut shape = dot(1);
    for i in 0..200 {
        shape = if i % 2 == 0 { moved(1, 0, shape) } else { group(vec![shape, hidden(dot(2))]) };
    }
    check!("200 levels", tidy(shape), moved(100, 0, dot(1)));
}

#[test]
fn a_very_wide_group() {
    let shape = group((0..3000).map(|i| group(vec![moved(2, 2, dot(i))])).collect());
    let result = tidy(shape);
    let ok = matches!(&result, Shape::Move(2, 2, inner) if matches!(&**inner, Shape::Group(p) if p.len() == 3000));
    check!("3000 parts that share a move", ok, true);
}

#[test]
fn single_dots_and_empty_groups() {
    check!("a dot", tidy(dot(3)), dot(3));
    check!("nothing", tidy(group(vec![])), group(vec![]));
    check!("groups of nothing", tidy(group(vec![group(vec![group(vec![])]), group(vec![])])), group(vec![]));
    check!("hidden", tidy(hidden(hidden(dot(1)))), group(vec![]));
}

#[test]
fn one_part_with_a_different_move_blocks_nothing_else() {
    let before = group(vec![moved(1, 0, dot(1)), moved(1, 0, dot(2)), moved(2, 0, dot(3))]);
    check!(format!("{before:?}"), tidy(before.clone()), before.clone());
    let mixed = group(vec![moved(1, 0, dot(1)), dot(2)]);
    check!(format!("{mixed:?}"), tidy(mixed.clone()), mixed.clone());
}
