use solution::*;

#[test]
fn zero_capacity() {
    check!(r#"N = 0"#, { let mut v: ArrayVec<i32, 0> = ArrayVec::new(); (v.try_push(1), v.pop(), v.is_full(), v.is_empty(), v.capacity()) }, (Err(1), None, true, true, 0));
}

#[test]
fn insert_at_len_is_allowed() {
    check!(r#"append through insert"#, { let mut v: ArrayVec<i32, 3> = ArrayVec::new(); v.try_insert(0, 1).unwrap(); v.try_insert(1, 2).unwrap(); (v.to_vec(), v.try_insert(2, 3), v.try_insert(0, 9)) }, (vec![1, 2], Ok(()), Err(9)));
}

#[test]
fn insert_past_len_panics() {
    check!(r#"index > len"#, std::panic::catch_unwind(|| { let mut v: ArrayVec<i32, 4> = ArrayVec::new(); v.try_push(1).unwrap(); let _ = v.try_insert(3, 2); }).is_err(), true);
}

#[test]
fn remove_out_of_range_panics() {
    check!(r#"index == len"#, std::panic::catch_unwind(|| { let mut v: ArrayVec<i32, 4> = ArrayVec::new(); v.try_push(1).unwrap(); v.remove(1); }).is_err(), true);
}

#[test]
fn into_array_when_full() {
    check!(r#"exactly N elements"#, { let mut v: ArrayVec<String, 2> = ArrayVec::new(); v.try_push("a".to_string()).unwrap(); v.try_push("b".to_string()).unwrap(); v.into_array().ok() }, Some(["a".to_string(), "b".to_string()]));
}

#[test]
fn into_array_when_not_full() {
    check!(r#"gives the vector back"#, { let mut v: ArrayVec<i32, 3> = ArrayVec::new(); v.try_push(7).unwrap(); match v.into_array() { Ok(_) => None, Err(back) => Some(back.to_vec()) } }, Some(vec![7]));
}

#[test]
fn strings_move_without_copying_bits_twice() {
    check!(r#"owned heap data"#, { let mut v: ArrayVec<String, 8> = ArrayVec::new(); for i in 0..8 { v.try_push(format!("s{i}")).unwrap(); } let a = v.remove(2); let b = v.swap_remove(0); (a, b, v.len(), v[0].clone()) }, ("s2".to_string(), "s0".to_string(), 6, "s7".to_string()));
}

#[test]
fn is_inline() {
    check!(r#"no heap, size is N elements plus a length"#, std::mem::size_of::<ArrayVec<u32, 16>>() <= 16 * 4 + 8, true);
}

#[test]
fn drops_exactly_the_live_elements() {
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
        let mut v: ArrayVec<D, 8> = ArrayVec::new();
        for _ in 0..8 {
            v.try_push(D(c.clone())).ok().unwrap();
        }
        let rejected = v.try_push(D(c.clone()));
        check!("a rejected push hands the value back, not dropped yet", (rejected.is_err(), c.get()), (true, 0));
        drop(rejected);
        check!("dropping the rejected value drops it once", c.get(), 1);
        drop(v.pop());
        drop(v.remove(0));
        drop(v.swap_remove(0));
        check!("three removed values dropped: 1 + 3", c.get(), 4);
    }
    check!("the five left dropped with the vector: 4 + 5", c.get(), 9);
    let c2 = Rc::new(Cell::new(0));
    let mut full: ArrayVec<D, 3> = ArrayVec::new();
    for _ in 0..3 {
        full.try_push(D(c2.clone())).ok().unwrap();
    }
    let arr = full.into_array().ok().unwrap();
    check!("into_array moves the elements: no drops yet", c2.get(), 0);
    drop(arr);
    check!("each of the three dropped once", c2.get(), 3);
}

#[test]
fn random_ops_vs_std_vec() {
    let mut rng = anneal_prelude::Rng::new(6603);
    let mut mine: ArrayVec<String, 16> = ArrayVec::new();
    let mut model: Vec<String> = Vec::new();
    for step in 0..20_000 {
        match rng.below(5) {
            0 | 1 => {
                let r = mine.try_push(format!("p{step}"));
                if model.len() < 16 {
                    model.push(format!("p{step}"));
                    check!("push accepted below capacity", r.is_ok(), true);
                } else {
                    check!("push rejected at capacity", r, Err(format!("p{step}")));
                }
            }
            2 => check!("pop", mine.pop(), model.pop()),
            3 => {
                let i = rng.below(model.len() + 1);
                let r = mine.try_insert(i, format!("i{step}"));
                if model.len() < 16 {
                    model.insert(i, format!("i{step}"));
                    check!("insert accepted below capacity", r.is_ok(), true);
                } else {
                    check!("insert rejected at capacity", r, Err(format!("i{step}")));
                }
            }
            _ if !model.is_empty() => {
                let i = rng.below(model.len());
                if rng.bool() {
                    check!("remove", mine.remove(i), model.remove(i));
                } else {
                    check!("swap_remove", mine.swap_remove(i), model.swap_remove(i));
                }
            }
            _ => {}
        }
        check!("same contents", mine.to_vec(), model.clone());
    }
}
