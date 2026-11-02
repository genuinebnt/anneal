use solution::*;

#[test]
fn lifo_reuse() {
    check!(r#"free 0 then 2, next two inserts take 2 then 0"#, { let mut s = Slab::new(); let k: Vec<Key> = (0..3).map(|i| s.insert(i)).collect(); s.remove(k[0]); s.remove(k[2]); let a = s.insert(10); let b = s.insert(11); let c = s.insert(12); (a.idx, b.idx, c.idx, s.slots()) }, (2, 0, 3, 4));
}

#[test]
fn generation_bumps_every_reuse() {
    check!(r#"same slot four times"#, { let mut s = Slab::new(); let mut gens = vec![]; for i in 0..4 { let k = s.insert(i); gens.push(k.gen); s.remove(k); } gens }, vec![0, 1, 2, 3]);
}

#[test]
fn unknown_key() {
    check!(r#"a key from nowhere"#, { let mut s: Slab<i32> = Slab::new(); let k = Key { idx: 9, gen: 0 }; (s.get(k).copied(), s.get_mut(k).is_none(), s.remove(k)) }, (None, true, None));
}

#[test]
fn double_remove_cannot_hit_the_new_value() {
    check!(r#"stale remove after reuse"#, { let mut s = Slab::new(); let a = s.insert(1); s.remove(a); let _b = s.insert(2); (s.remove(a), s.len()) }, (None, 1));
}

#[test]
fn iter_in_slot_order() {
    check!(r#"live entries only"#, { let mut s = Slab::new(); let k: Vec<Key> = (0..5).map(|i| s.insert(i * 10)).collect(); s.remove(k[1]); s.remove(k[3]); s.iter().map(|(_, v)| *v).collect::<Vec<_>>() }, vec![0, 20, 40]);
}

#[test]
fn iter_keys_work() {
    check!(r#"keys from iter reach the values"#, { let mut s = Slab::new(); for i in 0..4 { s.insert(i); } let ok = s.iter().all(|(k, v)| s.get(k) == Some(v)); ok }, true);
}

#[test]
fn drops_removed_values_once() {
    check!(r#"an Rc's count tracks ownership"#, { let rc = std::rc::Rc::new(()); let mut s = Slab::new(); let k = s.insert(rc.clone()); let k2 = s.insert(rc.clone()); let before = std::rc::Rc::strong_count(&rc); drop(s.remove(k)); let mid = std::rc::Rc::strong_count(&rc); let _ = k2; drop(s); (before, mid, std::rc::Rc::strong_count(&rc)) }, (3, 2, 1));
}

#[test]
fn churn_does_not_grow() {
    let mut s = Slab::new();
    let mut live = Vec::new();
    for i in 0..1_000 {
        live.push(s.insert(i));
    }
    for round in 0..200 {
        for _ in 0..500 {
            let k = live.swap_remove((round * 7) % live.len());
            s.remove(k);
        }
        for i in 0..500 {
            live.push(s.insert(i));
        }
    }
    check!("a thousand live values never need more than a thousand slots", (s.len(), s.slots()), (1_000, 1_000));
}

#[test]
fn random_vs_model() {
    use std::collections::HashMap;
    let mut rng = anneal_prelude::Rng::new(6502);
    let mut s: Slab<u64> = Slab::new();
    let mut model: HashMap<Key, u64> = HashMap::new();
    let mut dead: Vec<Key> = Vec::new();
    for step in 0..20_000u64 {
        match rng.below(4) {
            0 | 1 => {
                let k = s.insert(step);
                check!("a fresh key was not alive before", model.insert(k, step).is_none(), true);
            }
            2 if !model.is_empty() => {
                let keys: Vec<Key> = model.keys().copied().collect();
                let k = keys[rng.below(keys.len()) as usize];
                check!("remove returns what was stored", s.remove(k), model.remove(&k));
                dead.push(k);
            }
            _ if !dead.is_empty() => {
                let k = dead[rng.below(dead.len()) as usize];
                check!("a stale key finds nothing", (s.get(k).is_none(), s.remove(k)), (true, None));
            }
            _ => {}
        }
        check!("len matches the model", s.len(), model.len());
    }
    for (k, v) in &model {
        check!("every live key still reads its value", s.get(*k), Some(v));
    }
}
