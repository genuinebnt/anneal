use solution::*;

#[test]
fn evicts_the_least_recently_used() {
    let mut c = LruCache::new(2);
    check!("two inserts", (c.put(1, "a"), c.put(2, "b")), (Put::Inserted, Put::Inserted));
    check!("a third key", c.put(3, "c"), Put::Evicted(1, "a"));
    check!("recency", c.keys_by_recency(), vec![3, 2]);
}

#[test]
fn get_counts_as_a_use() {
    let mut c = LruCache::new(2);
    c.put(1, 10);
    c.put(2, 20);
    check!("get(1)", c.get(&1).copied(), Some(10));
    check!("then a new key evicts 2", c.put(3, 30), Put::Evicted(2, 20));
}

#[test]
fn peek_changes_nothing() {
    let mut c = LruCache::new(2);
    c.put(1, 10);
    c.put(2, 20);
    check!("peek(1)", c.peek(&1).copied(), Some(10));
    check!("1 is still the oldest", c.put(3, 30), Put::Evicted(1, 10));
}

#[test]
fn replacing_never_evicts() {
    let mut c = LruCache::new(2);
    c.put(1, 10);
    c.put(2, 20);
    check!("put(1, 11)", c.put(1, 11), Put::Replaced(10));
    check!("(len, recency)", (c.len(), c.keys_by_recency()), (2, vec![1, 2]));
}

#[test]
fn remove_frees_room() {
    let mut c = LruCache::new(2);
    c.put("x", 1);
    c.put("y", 2);
    check!("remove x", (c.remove(&"x"), c.remove(&"x")), (Some(1), None));
    check!("room for z", (c.put("z", 3), c.keys_by_recency()), (Put::Inserted, vec!["z", "y"]));
}
