use solution::*;

#[test]
fn fifo() {
    check!(r#"push 1, 2, 3 then pop three"#, { let q = BlockingQueue::new(3); q.push(1).unwrap(); q.push(2).unwrap(); q.push(3).unwrap(); (q.pop(), q.pop(), q.pop()) }, (Some(1), Some(2), Some(3)));
}

#[test]
fn try_push_full() {
    check!(r#"capacity 1, second try_push"#, { let q = BlockingQueue::new(1); q.try_push('a').unwrap(); q.try_push('b') }, Err('b'));
}

#[test]
fn try_pop_empty() {
    check!(r#"nothing queued"#, BlockingQueue::<u8>::new(2).try_pop(), None);
}

#[test]
fn closed_drains_then_none() {
    check!(r#"push 1, 2, close, pop thrice"#, { let q = BlockingQueue::new(4); q.push(1).unwrap(); q.push(2).unwrap(); q.close(); (q.pop(), q.pop(), q.pop()) }, (Some(1), Some(2), None));
}

#[test]
fn push_after_close() {
    check!(r#"close, then push"#, { let q = BlockingQueue::new(4); q.close(); (q.push(7), q.try_push(8)) }, (Err(7), Err(8)));
}
