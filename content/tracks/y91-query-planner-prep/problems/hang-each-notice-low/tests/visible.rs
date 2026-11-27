use solution::*;

#[allow(dead_code)]
fn room(n: u8) -> Place {
    Place::Room(n)
}
#[allow(dead_code)]
fn wing(a: Place, b: Place) -> Place {
    Place::Wing(Box::new(a), Box::new(b))
}

/// ((0 1) (2 (3 4)))
#[allow(dead_code)]
fn school() -> Place {
    wing(wing(room(0), room(1)), wing(room(2), wing(room(3), room(4))))
}

#[test]
fn as_low_as_it_can_hang() {
    use Turn::{Left, Right};
    let notices = vec![vec![1], vec![4, 3], vec![3, 2], vec![0, 4]];
    check!("rooms [1], [4, 3], [3, 2], [0, 4]", hang(&school(), &notices), vec![
        Some(vec![Left, Right]),
        Some(vec![Right, Right]),
        Some(vec![Right]),
        Some(vec![]),
    ]);
}

#[test]
fn unknown_rooms_and_empty_notices() {
    check!("room 9 does not exist", hang(&school(), &[vec![1, 9]]), vec![None]);
    check!("a notice for no room stays at the entrance", hang(&school(), &[vec![]]), vec![Some(vec![])]);
}

#[test]
fn a_building_of_one_room() {
    check!("one room", hang(&room(7), &[vec![7, 7], vec![6]]), vec![Some(vec![]), None]);
}

#[test]
fn a_whole_part_of_the_building() {
    use Turn::Right;
    check!("rooms 2, 3 and 4 fill the right part", hang(&school(), &[vec![2, 3, 4]]), vec![Some(vec![Right])]);
}

#[test]
fn naming_a_room_twice() {
    use Turn::{Left, Right};
    check!("room 3 named twice", hang(&school(), &[vec![3, 3]]), vec![Some(vec![Right, Right, Left])]);
}
