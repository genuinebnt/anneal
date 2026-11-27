use solution::*;

#[allow(dead_code)]
fn stack(values: &[i64]) -> Stack {
    let mut s = Stack::new();
    values.iter().for_each(|&v| s.push(v));
    s
}

#[test]
fn nothing_there() {
    let mut s = Stack::new();
    check!("empty", (s.keep(0).is_none(), s.pop()), (true, None));
    s.push(1);
    check!(
        "out of range",
        (s.keep(1).is_none(), s.keep(usize::MAX).is_none()),
        (true, true)
    );
    s.truncate(10);
    check!("longer", s.slots().to_vec(), vec![1]);
}

#[test]
fn truncate_lets_go_of_everything_above() {
    let mut s = stack(&[0, 1, 2, 3, 4]);
    let handles: Vec<Handle> = [4, 1, 3, 0, 2]
        .iter()
        .map(|&i| s.keep(i).unwrap())
        .collect();
    s.truncate(2);
    s.push(100);
    s.push(101);
    s.slots_mut()[0] = -1;
    let got: Vec<i64> = handles.iter().map(|h| s.read(h)).collect();
    check!("kept 4 1 3 0 2, truncated to 2", got, vec![4, 1, 3, -1, 2]);
    s.write(&handles[2], 33);
    check!(
        "the new slot 3 is untouched",
        s.slots().to_vec(),
        vec![-1, 1, 100, 101]
    );
}

#[test]
fn a_clone_is_the_same_variable() {
    let mut s = stack(&[5]);
    let a = s.keep(0).unwrap();
    let b = a.clone();
    s.truncate(0);
    s.write(&b, 6);
    check!("clone", (s.read(&a), a.same(&b)), (6, true));
}

#[test]
fn different_slots_are_different_variables() {
    let mut s = stack(&[1, 1]);
    let a = s.keep(0).unwrap();
    let b = s.keep(1).unwrap();
    check!("not the same", a.same(&b), false);
    s.write(&a, 2);
    check!("separate", s.read(&b), 1);
}

#[test]
fn the_value_is_taken_when_the_slot_goes() {
    let mut s = stack(&[1]);
    let h = s.keep(0).unwrap();
    s.write(&h, 2);
    s.slots_mut()[0] = 3;
    s.truncate(0);
    s.push(99);
    check!("3", s.read(&h), 3);
}

#[test]
fn claiming_again_after_other_removals() {
    let mut s = stack(&[1, 2, 3]);
    let low = s.keep(0).unwrap();
    let high = s.keep(2).unwrap();
    s.pop();
    let again = s.keep(0).unwrap();
    check!(
        "still the same",
        (low.same(&again), low.same(&high)),
        (true, false)
    );
}

#[test]
fn many_slots() {
    let mut s = stack(&(0..3000).collect::<Vec<_>>());
    let handles: Vec<Handle> = (0..3000).rev().map(|i| s.keep(i).unwrap()).collect();
    s.truncate(0);
    let got: Vec<i64> = handles.iter().map(|h| s.read(h)).collect();
    check!("3000 handles", got, (0..3000).rev().collect::<Vec<_>>());
}

// Random operations against a model in which every variable has a number of its own and all values sit in one table.

#[test]
fn random_histories() {
    let mut rng = anneal_prelude::Rng::new(7101);
    for _ in 0..1500 {
        let mut s = Stack::new();
        let (mut values, mut slots): (Vec<i64>, Vec<usize>) = (Vec::new(), Vec::new());
        let mut kept: Vec<(Handle, usize)> = Vec::new();
        let mut history = String::new();
        for step in 0..40 {
            let value = step * 10 + 1;
            match rng.below(8) {
                0 | 1 => {
                    history.push_str(&format!("push({value}) "));
                    s.push(value);
                    slots.push(values.len());
                    values.push(value);
                }
                2 => {
                    history.push_str("pop ");
                    let want = slots.pop().map(|v| values[v]);
                    check!(history.clone(), s.pop(), want);
                }
                3 => {
                    let length = rng.below(slots.len() + 2);
                    history.push_str(&format!("truncate({length}) "));
                    s.truncate(length);
                    slots.truncate(length);
                }
                4 if !slots.is_empty() => {
                    let i = rng.below(slots.len());
                    history.push_str(&format!("keep({i}) "));
                    let handle = s.keep(i).expect("the slot exists");
                    for (other, variable) in &kept {
                        check!(history.clone(), handle.same(other), *variable == slots[i]);
                    }
                    kept.push((handle, slots[i]));
                }
                5 if !slots.is_empty() => {
                    let i = rng.below(slots.len());
                    history.push_str(&format!("slot[{i}]={value} "));
                    s.slots_mut()[i] = value;
                    values[slots[i]] = value;
                }
                6 if !kept.is_empty() => {
                    let i = rng.below(kept.len());
                    history.push_str(&format!("write(h{i},{value}) "));
                    s.write(&kept[i].0, value);
                    values[kept[i].1] = value;
                }
                _ => {}
            }
            let want: Vec<i64> = slots.iter().map(|&v| values[v]).collect();
            check!(history.clone(), s.slots().to_vec(), want);
            for (i, (handle, variable)) in kept.iter().enumerate() {
                check!(
                    format!("{history}read(h{i})"),
                    s.read(handle),
                    values[*variable]
                );
            }
        }
    }
}
