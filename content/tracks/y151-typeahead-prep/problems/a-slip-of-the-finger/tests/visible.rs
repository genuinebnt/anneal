use solution::*;

fn text(a: &str, b: &str, max: usize) -> Option<usize> {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    slips(&a, &b, max)
}

#[test]
fn each_kind_of_slip_counts_once() {
    check!("added", text("cat", "cart", 2), Some(1));
    check!("left out", text("cart", "cat", 2), Some(1));
    check!("replaced", text("cat", "cut", 2), Some(1));
    check!("swapped", text("form", "from", 2), Some(1));
    check!("same", text("cat", "cat", 0), Some(0));
}

#[test]
fn too_far_is_none() {
    check!("3 slips, 2 allowed", text("kitten", "sitting", 2), None);
    check!("3 slips, 3 allowed", text("kitten", "sitting", 3), Some(3));
}

#[test]
fn no_item_is_touched_twice() {
    check!("ca to abc", text("ca", "abc", 5), Some(3));
}

#[test]
fn nothing_against_something() {
    check!("empty to three", text("", "abc", 3), Some(3));
}

#[test]
fn any_items_will_do() {
    check!("numbers", slips(&[10, 20, 30], &[10, 30, 20], 1), Some(1));
}
