use solution::*;

#[test]
fn values_survive_reallocation() {
    check!(r#"strings with their own heap data"#, { let mut v = MyVec::new(); for i in 0..1000 { v.push(format!("item-{i}")); } (v[0].clone(), v[999].clone(), v.iter().map(|s| s.len()).sum::<usize>()) }, ("item-0".to_string(), "item-999".to_string(), 7890));
}

#[test]
fn zero_sized_types() {
    check!(r#"never allocates"#, { let mut v = MyVec::new(); for _ in 0..10_000 { v.push(()); } let n = v.len(); (n, v.capacity() == usize::MAX, v.pop(), v.len()) }, (10_000, true, Some(()), 9_999));
}

#[test]
fn remove_at_the_ends() {
    check!(r#"first and last"#, { let mut v = MyVec::new(); for x in 1..=4 { v.push(x); } let a = v.remove(0); let b = v.remove(2); (a, b, v.to_vec()) }, (1, 4, vec![2, 3]));
}

#[test]
fn truncate_and_clear() {
    check!(r#"shorten then empty"#, { let mut v = MyVec::new(); for x in 0..10 { v.push(x); } v.truncate(20); let a = v.len(); v.truncate(4); let b = v.to_vec(); v.clear(); (a, b, v.len()) }, (10, vec![0, 1, 2, 3], 0));
}

#[test]
fn insert_past_the_end_panics() {
    check!(r#"index > len"#, std::panic::catch_unwind(|| { let mut v = MyVec::new(); v.push(1); v.insert(3, 2); }).is_err(), true);
}

#[test]
fn remove_out_of_range_panics() {
    check!(r#"index == len"#, std::panic::catch_unwind(|| { let mut v = MyVec::new(); v.push(1); v.remove(1); }).is_err(), true);
}

#[test]
fn deref_mut_writes() {
    check!(r#"mutate through the slice"#, { let mut v = MyVec::new(); for x in 0..4 { v.push(x); } for x in v.iter_mut() { *x *= 10; } v.to_vec() }, vec![0, 10, 20, 30]);
}

#[test]
fn is_send_and_sync() {
    check!(r#"owning a Send type"#, { fn needs<X: Send + Sync>() {} needs::<MyVec<u32>>(); true }, true);
}

#[test]
fn drops_every_element_exactly_once() {
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
        let mut v = MyVec::new();
        for _ in 0..10 {
            v.push(D(c.clone()));
        }
        check!("nothing dropped while they are in the vector", c.get(), 0);
        let popped = v.pop().unwrap();
        check!("pop moves out without dropping", c.get(), 0);
        drop(popped);
        check!("the popped value dropped once", c.get(), 1);
        let removed = v.remove(3);
        check!("remove moves out without dropping", c.get(), 1);
        drop(removed);
        v.truncate(5);
        check!("truncate to 5 dropped the tail: 1 + 1 + 3", c.get(), 5);
        v.insert(0, D(c.clone()));
        v.clear();
        check!("clear dropped the remaining 6: 5 + 6", c.get(), 11);
        v.push(D(c.clone()));
    }
    check!("dropping the vector dropped the last element", c.get(), 12);
}

#[test]
fn random_ops_vs_std_vec() {
    let mut rng = anneal_prelude::Rng::new(6601);
    let mut mine = MyVec::new();
    let mut model: Vec<String> = Vec::new();
    for step in 0..20_000 {
        match rng.below(6) {
            0 | 1 => {
                mine.push(format!("p{step}"));
                model.push(format!("p{step}"));
            }
            2 => check!("pop", mine.pop(), model.pop()),
            3 => {
                let i = rng.below(model.len() + 1);
                mine.insert(i, format!("i{step}"));
                model.insert(i, format!("i{step}"));
            }
            4 if !model.is_empty() => {
                let i = rng.below(model.len());
                check!("remove", mine.remove(i), model.remove(i));
            }
            _ if rng.below(50) == 0 => {
                let n = rng.below(model.len() + 1);
                mine.truncate(n);
                model.truncate(n);
            }
            _ => {}
        }
        check!("same length", mine.len(), model.len());
    }
    check!("same contents", mine.to_vec(), model);
}
