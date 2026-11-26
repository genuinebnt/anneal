use solution::*;

fn drain<K: std::hash::Hash + Eq + Clone, P: Ord + Clone>(q: &mut UrgencyQueue<K, P>) -> Vec<(K, P)> {
    std::iter::from_fn(|| q.pop()).collect()
}

fn of(entries: &[(&'static str, u32)]) -> UrgencyQueue<&'static str, u32> {
    let mut q = UrgencyQueue::new();
    for &(k, p) in entries {
        q.push(k, p);
    }
    q
}

#[test]
fn core_smallest_priority_first() {
    let mut q = of(&[("backup", 30), ("page", 1), ("report", 10)]);
    check!("peek", q.peek(), Some((&"page", &1)));
    check!("pops", drain(&mut q), vec![("page", 1), ("report", 10), ("backup", 30)]);
}

#[test]
fn core_a_waiting_key_is_not_pushed_twice() {
    let mut q = of(&[("a", 5)]);
    check!("push again", (q.push("a", 1), q.len(), q.priority(&"a").copied()), (false, 1, Some(5)));
}

#[test]
fn core_empty() {
    let mut q: UrgencyQueue<u8, u8> = UrgencyQueue::new();
    check!("(len, pop, priority)", (q.len(), q.pop(), q.priority(&1).copied()), (0, None, None));
}

#[test]
fn part2_change_in_both_directions() {
    let mut q = of(&[("a", 10), ("b", 20), ("c", 30)]);
    check!("raise a", q.change_priority(&"a", 40), Some(10));
    check!("lower c", q.change_priority(&"c", 5), Some(30));
    check!("pops", drain(&mut q), vec![("c", 5), ("b", 20), ("a", 40)]);
}

#[test]
fn part3_cancel_from_the_middle() {
    let mut q = of(&[("a", 1), ("b", 2), ("c", 3)]);
    check!("remove b twice", (q.remove(&"b"), q.remove(&"b")), (Some(2), None));
    check!("pops", drain(&mut q), vec![("a", 1), ("c", 3)]);
}

#[test]
fn part4_ties_go_to_the_earliest_push() {
    let mut q = of(&[("x", 7), ("y", 7), ("z", 7)]);
    check!("pops", drain(&mut q), vec![("x", 7), ("y", 7), ("z", 7)]);
}
