use solution::*;

#[allow(dead_code)]
fn stack(values: &[i64]) -> Stack {
    let mut s = Stack::new();
    values.iter().for_each(|&v| s.push(v));
    s
}

#[test]
fn push_and_pop() {
    let mut s = stack(&[1, 2]);
    check!("pop", (s.pop(), s.pop(), s.pop()), (Some(2), Some(1), None));
}

#[test]
fn a_handle_and_its_slot_are_one_value() {
    let mut s = stack(&[10, 20]);
    let h = s.keep(1).unwrap();
    s.slots_mut()[1] = 21;
    check!("read after slot write", s.read(&h), 21);
    s.write(&h, 22);
    check!("slot after write", s.slots().to_vec(), vec![10, 22]);
}

#[test]
fn the_variable_outlives_its_slot() {
    let mut s = stack(&[10, 20]);
    let h = s.keep(1).unwrap();
    s.slots_mut()[1] = 25;
    s.pop();
    check!("after pop", s.read(&h), 25);
    s.write(&h, 30);
    check!("written after pop", s.read(&h), 30);
}

#[test]
fn a_new_variable_in_an_old_slot() {
    let mut s = stack(&[1]);
    let old = s.keep(0).unwrap();
    s.pop();
    s.push(2);
    let new = s.keep(0).unwrap();
    s.write(&new, 3);
    check!(
        "two variables",
        (s.read(&old), s.read(&new), old.same(&new)),
        (1, 3, false)
    );
}

#[test]
fn claiming_twice() {
    let mut s = stack(&[7]);
    let a = s.keep(0).unwrap();
    let b = s.keep(0).unwrap();
    s.pop();
    s.write(&a, 8);
    check!("one variable", (a.same(&b), s.read(&b)), (true, 8));
}
