use solution::*;

#[test]
fn wraps_many_times() {
    check!(r#"capacity 3, push 0..10"#, { let mut r = Ring::with_capacity(3); for i in 0..10 { r.push(i); } (r.len(), r.iter().copied().collect::<Vec<_>>()) }, (3, vec![7, 8, 9]));
}

#[test]
fn capacity_one() {
    check!(r#"capacity 1, push "a", "b""#, { let mut r = Ring::with_capacity(1); r.push("a"); (r.push("b"), r.iter().copied().collect::<Vec<_>>()) }, (Some("a"), vec!["b"]));
}

#[test]
fn empty_ring() {
    check!(r#"capacity 3, nothing pushed"#, { let r = Ring::<u8>::with_capacity(3); (r.len(), r.iter().count()) }, (0, 0));
}

#[test]
fn zero_capacity_panics() {
    check!(r#"capacity 0"#, std::panic::catch_unwind(|| Ring::<u8>::with_capacity(0)).is_err(), true);
}

#[test]
fn strings() {
    check!(r#"capacity 2, push "a", "b", "c""#, { let mut r = Ring::with_capacity(2); r.push("a".to_string()); r.push("b".to_string()); (r.push("c".to_string()), r.iter().cloned().collect::<Vec<_>>()) }, (Some("a".to_string()), vec!["b".to_string(), "c".to_string()]));
}

#[test]
fn big() {
    check!(r#"capacity 1000, push 0..2500"#, { let mut r = Ring::with_capacity(1000); let evicted = (0..2500).filter_map(|i| r.push(i)).count(); let v: Vec<i32> = r.iter().copied().collect(); (evicted, v.len(), v[0], v[999]) }, (1500, 1000, 1500, 2499));
}

#[test]
fn iter_twice() {
    check!(r#"capacity 3, push 1..=4, iterate twice"#, { let mut r = Ring::with_capacity(3); for i in 1..=4 { r.push(i); } (r.iter().copied().collect::<Vec<_>>(), r.iter().copied().collect::<Vec<_>>()) }, (vec![2, 3, 4], vec![2, 3, 4]));
}

#[test]
fn push_moves_no_other_value() {
    for cap in [2usize, 3, 8] {
        let mut r = Ring::with_capacity(cap);
        let mut before: Vec<(u32, *const u32)> = Vec::new();
        for x in 0..4 * cap as u32 {
            r.push(x);
            let now: Vec<(u32, *const u32)> = r.iter().map(|v| (*v, v as *const u32)).collect();
            for &(value, slot) in &before {
                if let Some(&(_, new_slot)) = now.iter().find(|&&(v, _)| v == value) {
                    check!(format!("capacity {cap}: after push({x}), is {value} still in the same slot?"), new_slot == slot, true);
                }
            }
            before = now;
        }
    }
}

#[test]
fn random_vs_model() {
    let mut rng = anneal_prelude::Rng::new(2318);
    for _ in 0..300 {
        let cap = rng.below(5) + 1;
        let mut r = Ring::with_capacity(cap);
        let mut model = std::collections::VecDeque::new();
        let mut pushed = Vec::new();
        for _ in 0..rng.below(15) {
            let x = rng.below(100) as u32;
            pushed.push(x);
            model.push_back(x);
            let want = if model.len() > cap { model.pop_front() } else { None };
            check!(format!("capacity {cap}, push {pushed:?}"), r.push(x), want);
        }
        check!(format!("capacity {cap}, push {pushed:?}, then iter"), (r.len(), r.iter().copied().collect::<Vec<_>>()), (model.len(), model.into_iter().collect::<Vec<_>>()));
    }
}

#[test]
fn scale_300k_pushes() {
    let mut r = Ring::with_capacity(150_000);
    let mut evicted = 0u64;
    for i in 0..300_000u64 {
        evicted += r.push(i).unwrap_or(0);
    }
    let first = r.iter().next().copied();
    check!("capacity 150000, push 0..300000", (r.len(), first, evicted), (150_000, Some(150_000), 11_249_925_000));
}
