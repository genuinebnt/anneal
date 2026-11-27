use solution::*;

#[allow(dead_code)]
fn list(items: Vec<Item>) -> List {
    std::rc::Rc::new(std::cell::RefCell::new(items))
}

#[allow(dead_code)]
fn num(n: i64) -> Item {
    Item::Number(n)
}

#[allow(dead_code)]
fn of(l: &List) -> Item {
    Item::List(l.clone())
}

#[test]
fn numbers_and_lists() {
    let l = list(vec![num(1), Item::List(list(vec![])), num(-2)]);
    check!(
        "[1, [], -2]",
        show(&of(&l), 100),
        Some("[1, [], -2]".to_string())
    );
}

#[test]
fn a_list_inside_itself() {
    let l = list(vec![num(1)]);
    l.borrow_mut().push(of(&l));
    check!(
        "l = [1, l]",
        show(&of(&l), 100),
        Some("[1, [...]]".to_string())
    );
}

#[test]
fn twice_side_by_side_is_not_inside_itself() {
    let inner = list(vec![num(5)]);
    let outer = list(vec![of(&inner), of(&inner)]);
    check!(
        "[a, a]",
        show(&of(&outer), 100),
        Some("[[5], [5]]".to_string())
    );
}

#[test]
fn exactly_at_the_limit() {
    let l = list(vec![num(10), num(20)]);
    check!("limit 8", show(&of(&l), 8), Some("[10, 20]".to_string()));
    check!("limit 7", show(&of(&l), 7), None);
}

#[test]
fn doubling_sixty_times() {
    let mut l = list(vec![num(0)]);
    for _ in 0..60 {
        l = list(vec![of(&l), of(&l)]);
    }
    check!("2^60 zeros", show(&of(&l), 1000), None);
}
