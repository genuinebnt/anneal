use solution::*;

#[test]
fn empty_list() {
    check!(r#"nothing there"#, { let mut l: List<u8> = List::new(); (l.pop_front(), l.pop_back(), l.front().copied(), l.back().copied(), l.is_empty(), l.iter().next().copied()) }, (None, None, None, None, true, None));
}

#[test]
fn single_element_both_ways() {
    check!(r#"the one node is both head and tail"#, { let mut l = List::new(); l.push_back(1); let a = l.pop_back(); l.push_front(2); let b = l.pop_front(); l.push_back(3); (a, b, l.pop_back(), l.pop_front(), l.len()) }, (Some(1), Some(2), Some(3), None, 0));
}

#[test]
fn push_front_on_an_empty_list_sets_the_tail() {
    check!(r#"then pop_back and back()"#, { let mut l = List::new(); l.push_front(1); (l.back().copied(), l.iter().rev().count(), l.pop_back(), l.len()) }, (Some(1), 1, Some(1), 0));
}

#[test]
fn ends_meet_in_the_middle() {
    check!(r#"alternate next and next_back"#, { let mut l = List::new(); for x in 1..=5 { l.push_back(x); } let mut it = l.iter(); let v = (it.next().copied(), it.next_back().copied(), it.next().copied(), it.next_back().copied(), it.next().copied(), it.next().copied(), it.next_back().copied()); v }, (Some(1), Some(5), Some(2), Some(4), Some(3), None, None));
}

#[test]
fn iter_mut_edits() {
    check!(r#"double every element"#, { let mut l = List::new(); for x in 1..=4 { l.push_back(x); } for x in l.iter_mut() { *x *= 2; } l.iter().copied().collect::<Vec<_>>() }, vec![2, 4, 6, 8]);
}

#[test]
fn iter_mut_from_the_back() {
    check!(r#"mutate while reversed"#, { let mut l = List::new(); for x in 1..=3 { l.push_back(x); } for (i, x) in l.iter_mut().rev().enumerate() { *x += i * 10; } l.iter().copied().collect::<Vec<_>>() }, vec![21, 12, 3]);
}

#[test]
fn append_to_and_from_empty() {
    check!(r#"empty cases"#, { let mut a: List<i32> = List::new(); let mut b = List::new(); b.push_back(1); a.append(&mut b); let mut c = List::new(); a.append(&mut c); a.push_back(2); (a.iter().copied().collect::<Vec<_>>(), a.back().copied()) }, (vec![1, 2], Some(2)));
}

#[test]
fn retain_everything_and_nothing() {
    check!(r#"all, then none"#, { let mut l = List::new(); for x in 0..5 { l.push_back(x); } l.retain(|_| true); let n = l.len(); l.retain(|_| false); (n, l.len(), l.pop_front(), l.front().copied()) }, (5, 0, None, None));
}

#[test]
fn retain_first_and_last() {
    check!(r#"remove head and tail"#, { let mut l = List::new(); for x in 0..5 { l.push_back(x); } l.retain(|x| *x != 0 && *x != 4); l.push_front(9); l.push_back(8); (l.iter().copied().collect::<Vec<_>>(), l.iter().rev().copied().collect::<Vec<_>>()) }, (vec![9, 1, 2, 3, 8], vec![8, 3, 2, 1, 9]));
}

#[test]
fn drops_every_element_once() {
    use std::cell::Cell;
    use std::rc::Rc;
    struct D(Rc<Cell<usize>>);
    impl Drop for D {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let c = Rc::new(Cell::new(0));
    {
        let mut l = List::new();
        for _ in 0..50 {
            l.push_back(D(c.clone()));
        }
        let a = l.pop_front();
        check!("pop moves out without dropping", c.get(), 0);
        drop(a);
        let mut n = 0;
        l.retain(|_| {
            n += 1;
            n % 2 == 0
        });
        check!("retain dropped 1 + 25 of the 50", c.get(), 1 + 25);
    }
    check!("the list drop took the other 24", c.get(), 50);
}

#[test]
fn a_million_nodes_drop_without_recursion() {
    let mut l = List::new();
    for i in 0..1_000_000u32 {
        l.push_back(i);
    }
    check!("len", l.len(), 1_000_000);
    drop(l);
}

#[test]
fn random_ops_vs_vecdeque() {
    use std::collections::VecDeque;
    let mut rng = anneal_prelude::Rng::new(6602);
    let mut l: List<u32> = List::new();
    let mut m: VecDeque<u32> = VecDeque::new();
    let mut spare: List<u32> = List::new();
    for step in 0..20_000u32 {
        match rng.below(8) {
            0 => { l.push_front(step); m.push_front(step); }
            1 | 2 => { l.push_back(step); m.push_back(step); }
            3 => check!("pop_front", l.pop_front(), m.pop_front()),
            4 => check!("pop_back", l.pop_back(), m.pop_back()),
            5 => {
                for k in 0..rng.below(4) as u32 {
                    spare.push_back(step * 10 + k);
                    m.push_back(step * 10 + k);
                }
                l.append(&mut spare);
            }
            6 if rng.below(40) == 0 => {
                let modulus = 2 + rng.below(3) as u32;
                l.retain(|x| x % modulus != 0);
                m.retain(|x| x % modulus != 0);
            }
            _ => {}
        }
        check!("len", l.len(), m.len());
    }
    check!("contents front to back", l.iter().copied().collect::<Vec<_>>(), m.iter().copied().collect::<Vec<_>>());
    check!("contents back to front", l.iter().rev().copied().collect::<Vec<_>>(), m.iter().rev().copied().collect::<Vec<_>>());
    check!("spare stayed empty", spare.len(), 0);
}
