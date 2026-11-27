use solution::*;

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

#[test]
fn groups_inside_groups_open_up() {
    let before = group(vec![group(vec![dot(1), group(vec![])]), hidden(dot(2)), dot(3)]);
    check!(format!("{before:?}"), tidy(before.clone()), group(vec![dot(1), dot(3)]));
}

#[test]
fn moves_add_up_and_can_cancel() {
    let before = moved(2, 0, moved(-2, 5, moved(0, -5, dot(1))));
    check!(format!("{before:?}"), tidy(before.clone()), dot(1));
}

#[test]
fn a_move_that_every_part_makes_is_made_once() {
    let before = group(vec![moved(1, 1, dot(1)), moved(1, 1, dot(2))]);
    check!(format!("{before:?}"), tidy(before.clone()), moved(1, 1, group(vec![dot(1), dot(2)])));
}

#[test]
fn nothing_left() {
    check!("a moved group of hidden things", tidy(moved(3, 3, group(vec![hidden(dot(1))]))), group(vec![]));
}

#[test]
fn a_tidy_shape_stays_as_it_is() {
    let shape = group(vec![moved(1, 0, dot(1)), moved(0, 1, group(vec![dot(2), dot(3)]))]);
    check!(format!("{shape:?}"), tidy(shape.clone()), shape.clone());
}
