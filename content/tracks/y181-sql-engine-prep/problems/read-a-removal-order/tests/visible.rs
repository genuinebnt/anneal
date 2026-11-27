use solution::*;

#[allow(dead_code)]
fn bad(wanted: Wanted, start: usize, end: usize) -> Result<Order, Mistake> {
    Err(Mistake { wanted, start, end })
}

#[test]
fn a_full_order() {
    let got = read("move 2 chairs, 1 piano from attic to garage by fri urgent");
    let want = Order {
        items: vec![(2, "chairs".to_string()), (1, "piano".to_string())],
        from: "attic".to_string(),
        to: "garage".to_string(),
        by: Some(Day::Fri),
        urgent: true,
    };
    check!("every part", got, Ok(want));
}

#[test]
fn the_short_form_and_any_capitals() {
    let want = Order {
        items: vec![(3, "boxes".to_string())],
        from: "mon".to_string(),
        to: "shed".to_string(),
        by: None,
        urgent: false,
    };
    check!("day names are fine as places", read("MOVE 3 Boxes FROM Mon To SHED"), Ok(want));
}

#[test]
fn a_wrong_word_is_pointed_at() {
    check!("no count", read("move chairs from a to b"), bad(Wanted::Count, 5, 11));
    check!("`form` for `from`", read("move 1 desk form a to b"), bad(Wanted::From, 12, 16));
    check!("not a day", read("move 1 desk from a to b by noon"), bad(Wanted::Day, 27, 31));
}

#[test]
fn a_message_that_stops_too_early() {
    check!("nothing", read(""), bad(Wanted::Move, 0, 0));
    check!("no second place", read("move 1 desk from a to  "), bad(Wanted::Place, 23, 23));
}

#[test]
fn more_after_the_end() {
    check!("two endings", read("move 1 desk from a to b urgent please"), bad(Wanted::End, 31, 37));
    check!("`by` comes before `urgent`", read("move 1 desk from a to b urgent by mon"), bad(Wanted::End, 31, 33));
}
