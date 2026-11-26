use solution::*;

use std::cell::Cell;
use std::rc::Rc;

#[test]
fn capacity_zero() {
    let mut c = LruCache::new(0);
    check!("put into nothing", (c.put(1, 2), c.len(), c.get(&1).copied()), (Put::Evicted(1, 2), 0, None));
}

#[test]
fn capacity_one() {
    let mut c = LruCache::new(1);
    c.put('a', 1);
    check!("(replace, evict)", (c.put('a', 2), c.put('b', 3)), (Put::Replaced(1), Put::Evicted('a', 2)));
    check!("recency", c.keys_by_recency(), vec!['b']);
}

#[test]
fn remove_from_each_position() {
    let mut c = LruCache::new(5);
    for i in 0..5 {
        c.put(i, ());
    }
    c.remove(&2);
    check!("middle", c.keys_by_recency(), vec![4, 3, 1, 0]);
    c.remove(&4);
    check!("newest", c.keys_by_recency(), vec![3, 1, 0]);
    c.remove(&0);
    check!("oldest", c.keys_by_recency(), vec![3, 1]);
}

#[test]
fn empty_it_and_start_again() {
    let mut c = LruCache::new(3);
    c.put(1, 1);
    c.remove(&1);
    check!("empty", (c.len(), c.keys_by_recency()), (0, vec![]));
    c.put(2, 2);
    c.put(3, 3);
    check!("refilled", c.keys_by_recency(), vec![3, 2]);
}

#[test]
fn missing_keys() {
    let mut c: LruCache<u8, u8> = LruCache::new(2);
    check!("get, peek, remove on nothing", (c.get(&1).copied(), c.peek(&1).copied(), c.remove(&1)), (None, None, None));
}

#[test]
fn replacing_moves_to_front() {
    let mut c = LruCache::new(3);
    for i in 1..=3 {
        c.put(i, i);
    }
    c.put(1, 100);
    check!("after updating the oldest", c.put(4, 4), Put::Evicted(2, 2));
}

struct Counted(Rc<Cell<usize>>);

impl Drop for Counted {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn values_leave_at_once() {
    let drops = Rc::new(Cell::new(0));
    let mut c = LruCache::new(2);
    c.put(1, Counted(drops.clone()));
    c.put(2, Counted(drops.clone()));
    drop(c.remove(&1));
    check!("after dropping a removed value", drops.get(), 1);
    drop(c.put(3, Counted(drops.clone())));
    drop(c.put(4, Counted(drops.clone())));
    check!("after dropping an evicted value", drops.get(), 2);
    drop(c.put(4, Counted(drops.clone())));
    check!("after dropping a replaced value", drops.get(), 3);
    drop(c);
    check!("after dropping the cache", drops.get(), 5);
}

#[test]
fn values_need_not_be_clone() {
    struct Token(u8);
    let mut c = LruCache::new(1);
    c.put(1, Token(7));
    check!("get a non-Clone value", c.get(&1).map(|t| t.0), Some(7));
}

#[test]
fn scale_operations_are_constant_time() {
    let n = 100_000u64;
    let mut c = LruCache::new(n as usize);
    for i in 0..n {
        c.put(i, i);
    }
    // Uses from the middle of the order, which a scan has to search for.
    let mut sum = 0;
    for i in 0..n {
        sum += *c.get(&((i * 7919) % n)).unwrap();
    }
    for i in n..2 * n {
        c.put(i, i);
    }
    for i in (n..2 * n).step_by(2) {
        c.remove(&i);
    }
    for i in 0..n / 2 {
        c.put(3 * n + i, i);
    }
    check!("(sum of 100 000 gets, final len)", (sum, c.len()), (n * (n - 1) / 2, n as usize));
}

#[test]
fn random_operations_match_a_vec_model() {
    let mut rng = anneal_prelude::Rng::new(8124);
    for _ in 0..400 {
        let capacity = rng.below(5);
        let mut cache = LruCache::new(capacity);
        // Most recently used first.
        let mut model: Vec<(u8, u32)> = Vec::new();
        let mut trace = vec![format!("capacity {capacity}")];
        for step in 0..40u32 {
            let key = rng.below(7) as u8;
            let found = model.iter().position(|e| e.0 == key);
            match rng.below(4) {
                0 => {
                    trace.push(format!("get {key}"));
                    let want = found.map(|i| {
                        let e = model.remove(i);
                        model.insert(0, e);
                        e.1
                    });
                    check!(format!("{trace:?}"), cache.get(&key).copied(), want);
                }
                1 => {
                    trace.push(format!("remove {key}"));
                    let want = found.map(|i| model.remove(i).1);
                    check!(format!("{trace:?}"), cache.remove(&key), want);
                }
                _ => {
                    trace.push(format!("put {key} {step}"));
                    let want = match found {
                        Some(i) => {
                            let old = model.remove(i).1;
                            model.insert(0, (key, step));
                            Put::Replaced(old)
                        }
                        None => {
                            model.insert(0, (key, step));
                            if model.len() > capacity {
                                let (k, v) = model.pop().unwrap();
                                Put::Evicted(k, v)
                            } else {
                                Put::Inserted
                            }
                        }
                    };
                    check!(format!("{trace:?}"), cache.put(key, step), want);
                }
            }
            check!(format!("{trace:?}: (len, recency)"), (cache.len(), cache.keys_by_recency()), (model.len(), model.iter().map(|e| e.0).collect::<Vec<_>>()));
        }
    }
}
