use solution::*;

#[test]
fn fifo_order() {
    check!(r#"push 1, 2, 3, pop three"#, { let q = Queue::new(4); q.push(1).unwrap(); q.push(2).unwrap(); q.push(3).unwrap(); (q.pop(), q.pop(), q.pop(), q.pop()) }, (Some(1), Some(2), Some(3), None));
}

#[test]
fn capacity_rounds_up() {
    check!(r#"5 -> 8, 1 -> 2, 8 -> 8"#, (Queue::<u8>::new(5).capacity(), Queue::<u8>::new(1).capacity(), Queue::<u8>::new(8).capacity()), (8, 2, 8));
}

#[test]
fn full_gives_value_back() {
    check!(r#"capacity 2, third push"#, { let q = Queue::new(2); q.push('a').unwrap(); q.push('b').unwrap(); q.push('c') }, Err('c'));
}

#[test]
fn pop_makes_room() {
    check!(r#"fill, pop, push"#, { let q = Queue::new(2); q.push(1).unwrap(); q.push(2).unwrap(); let a = q.pop(); let ok = q.push(3).is_ok(); (a, ok, q.pop(), q.pop()) }, (Some(1), true, Some(2), Some(3)));
}

#[test]
fn empty_pop() {
    check!(r#"new queue"#, Queue::<i32>::new(4).pop(), None);
}
