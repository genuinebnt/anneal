use solution::*;

fn drain<T>(q: &mut PriorityQueue<T>) -> Vec<T> {
    std::iter::from_fn(|| q.pop()).collect()
}

#[test]
fn core_greatest_comes_out_first() {
    let mut q = PriorityQueue::new();
    for x in [3, 9, 1, 7] {
        q.push(x);
    }
    check!("peek", q.peek().copied(), Some(9));
    check!("all pops", drain(&mut q), vec![9, 7, 3, 1]);
}

#[test]
fn core_empty_queue() {
    let mut q: PriorityQueue<u8> = PriorityQueue::new();
    check!("(len, empty, peek, pop)", (q.len(), q.is_empty(), q.peek().copied(), q.pop()), (0, true, None, None));
}

#[test]
fn core_equal_items_are_all_kept() {
    let mut q = PriorityQueue::new();
    for x in [5, 5, 2, 5] {
        q.push(x);
    }
    check!("len", q.len(), 4);
    check!("all pops", drain(&mut q), vec![5, 5, 5, 2]);
}

#[test]
fn part2_from_vec() {
    let mut q = PriorityQueue::from_vec(vec![4, 8, 15, 16, 23, 42, 0]);
    check!("(len, pops)", (q.len(), drain(&mut q)), (7, vec![42, 23, 16, 15, 8, 4, 0]));
}

#[test]
fn part3_smallest_first() {
    let mut q = PriorityQueue::with_comparator(|a: &i32, b: &i32| b.cmp(a));
    for x in [3, 9, 1] {
        q.push(x);
    }
    check!("all pops", drain(&mut q), vec![1, 3, 9]);
}

#[test]
fn part4_top_three() {
    check!("top 3", top_k(vec![5, 1, 9, 3, 7], 3), vec![9, 7, 5]);
    check!("k larger than the stream", top_k(vec![2, 1], 10), vec![2, 1]);
}
