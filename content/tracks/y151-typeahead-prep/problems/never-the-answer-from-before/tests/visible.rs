use solution::*;

#[test]
fn the_second_visitor_gets_the_stored_answer() {
    let shelf: Shelf<u32, String> = Shelf::new();
    check!("first", shelf.get_or_work_out(&1, || "a".to_string()), ("a".to_string(), false));
    check!("second", shelf.get_or_work_out(&1, || "b".to_string()), ("a".to_string(), true));
}

#[test]
fn a_change_throws_out_what_it_touches() {
    let shelf: Shelf<u32, u32> = Shelf::new();
    shelf.get_or_work_out(&1, || 10);
    shelf.get_or_work_out(&2, || 20);
    shelf.changed(|&k| k == 1);
    check!("1 is worked out again", shelf.get_or_work_out(&1, || 11), (11, false));
    check!("2 stayed", shelf.get_or_work_out(&2, || 21), (20, true));
}

#[test]
fn a_change_during_the_work() {
    let shelf: Shelf<u32, u32> = Shelf::new();
    let first = shelf.get_or_work_out(&1, || {
        shelf.changed(|&k| k == 1);
        10
    });
    check!("the caller still gets it", first, (10, false));
    check!("it was not stored", shelf.get_or_work_out(&1, || 11), (11, false));
}

#[test]
fn another_keys_change_is_no_reason_to_drop() {
    let shelf: Shelf<u32, u32> = Shelf::new();
    shelf.get_or_work_out(&1, || {
        shelf.changed(|&k| k == 2);
        10
    });
    check!("stored", shelf.get_or_work_out(&1, || 11), (10, true));
}

#[test]
fn work_may_ask_the_shelf() {
    let shelf: Shelf<u32, u32> = Shelf::new();
    let sum = shelf.get_or_work_out(&2, || shelf.get_or_work_out(&1, || 1).0 + 1);
    check!("built on another answer", sum, (2, false));
    check!("both stored", (shelf.get_or_work_out(&1, || 9), shelf.get_or_work_out(&2, || 9)), ((1, true), (2, true)));
}
