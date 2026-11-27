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
fn a_bare_number() {
    check!(
        "limit 20",
        show(&num(i64::MIN), 20),
        Some("-9223372036854775808".to_string())
    );
    check!("limit 19", show(&num(i64::MIN), 19), None);
}

#[test]
fn limits_of_nothing() {
    check!(
        "empty list, 2",
        show(&Item::List(list(vec![])), 2),
        Some("[]".to_string())
    );
    check!("empty list, 1", show(&Item::List(list(vec![])), 1), None);
    check!("a number, 0", show(&num(1), 0), None);
}

#[test]
fn the_closing_brackets_count() {
    let l = list(vec![Item::List(list(vec![Item::List(list(vec![num(1)]))]))]);
    check!("[[[1]]] 7", show(&of(&l), 7), Some("[[[1]]]".to_string()));
    check!("[[[1]]] 6", show(&of(&l), 6), None);
}

#[test]
fn two_lists_that_hold_each_other() {
    let a = list(vec![num(1)]);
    let b = list(vec![num(2), of(&a)]);
    a.borrow_mut().push(of(&b));
    check!("a", show(&of(&a), 100), Some("[1, [2, [...]]]".to_string()));
    check!("b", show(&of(&b), 100), Some("[2, [1, [...]]]".to_string()));
    a.borrow_mut().clear();
}

#[test]
fn after_leaving_a_list_it_may_be_written_again() {
    let shared = list(vec![num(3)]);
    let a = list(vec![of(&shared)]);
    let top = list(vec![of(&a), of(&shared), of(&a)]);
    check!(
        "[a, s, a]",
        show(&of(&top), 100),
        Some("[[[3]], [3], [[3]]]".to_string())
    );
}

#[test]
fn equal_contents_are_not_the_same_list() {
    let inner = list(vec![]);
    let outer = list(vec![of(&inner)]);
    let top = list(vec![of(&outer)]);
    check!("[[[]]]", show(&of(&top), 100), Some("[[[]]]".to_string()));
}

#[test]
fn nothing_is_left_borrowed_or_changed() {
    let l = list(vec![num(1)]);
    l.borrow_mut().push(of(&l));
    let _ = show(&of(&l), 100);
    let _ = show(&of(&l), 3);
    check!(
        "can borrow for writing",
        l.try_borrow_mut().map(|v| v.len()).ok(),
        Some(2)
    );
    l.borrow_mut().clear();
}

#[test]
fn tens_of_thousands_deep_on_a_small_stack() {
    let thread = std::thread::Builder::new().stack_size(256 << 10);
    let work = || {
        // Frees the chain from the outside in, so that dropping never recurses, also when `show` panics.
        struct Chain(Vec<List>);
        impl Drop for Chain {
            fn drop(&mut self) {
                while self.0.pop().is_some() {}
            }
        }
        let mut chain = Chain(vec![list(vec![])]);
        for _ in 0..10_000 {
            let next = list(vec![of(chain.0.last().unwrap())]);
            chain.0.push(next);
        }
        let top = of(chain.0.last().unwrap());
        let full = show(&top, 100_000).map(|t| t == "[".repeat(10_001) + &"]".repeat(10_001));
        let cut = show(&top, 100);
        (full, cut)
    };
    let got = thread
        .spawn(work)
        .expect("spawn")
        .join()
        .expect("show panicked");
    check!("10 000 lists", got, (Some(true), None));
}

#[test]
fn a_wide_list_that_holds_itself_everywhere() {
    let l = list(vec![]);
    for _ in 0..1000 {
        l.borrow_mut().push(of(&l));
    }
    let want = format!("[{}]", vec!["[...]"; 1000].join(", "));
    check!("1000 times itself", show(&of(&l), 10_000), Some(want));
    l.borrow_mut().clear();
}

// Random graphs of lists, compared with a plain recursive writer that has no limit.

#[allow(dead_code)]
fn plain(item: &Item, inside: &mut Vec<*const std::cell::RefCell<Vec<Item>>>) -> String {
    match item {
        Item::Number(n) => n.to_string(),
        Item::List(l) => {
            let id = std::rc::Rc::as_ptr(l);
            if inside.contains(&id) {
                return "[...]".to_string();
            }
            inside.push(id);
            let parts: Vec<String> = l.borrow().iter().map(|i| plain(i, inside)).collect();
            inside.pop();
            format!("[{}]", parts.join(", "))
        }
    }
}

#[test]
fn random_tangles() {
    let mut rng = anneal_prelude::Rng::new(6301);
    for _ in 0..1500 {
        let lists: Vec<List> = (0..1 + rng.below(5)).map(|_| list(vec![])).collect();
        let mut drawn = String::new();
        for (i, l) in lists.iter().enumerate() {
            for _ in 0..rng.below(4) {
                if rng.bool() {
                    let n = rng.int(-9, 99);
                    l.borrow_mut().push(num(n));
                    drawn.push_str(&format!("L{i}<-{n} "));
                } else {
                    let j = rng.below(lists.len());
                    l.borrow_mut().push(of(&lists[j]));
                    drawn.push_str(&format!("L{i}<-L{j} "));
                }
            }
        }
        let want = plain(&of(&lists[0]), &mut Vec::new());
        let limit = rng.below(want.len() * 2 + 2);
        let expected = if want.len() <= limit {
            Some(want)
        } else {
            None
        };
        check!(
            format!("{drawn} limit {limit}"),
            show(&of(&lists[0]), limit),
            expected
        );
        lists.iter().for_each(|l| l.borrow_mut().clear());
    }
}
