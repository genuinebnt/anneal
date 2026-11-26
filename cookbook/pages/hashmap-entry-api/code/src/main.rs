use hashmap_entry_api::*;
use std::collections::{BTreeMap, HashMap};

fn main() {
    // `HashMap` iteration order is not specified, so sort before printing.
    let counts: BTreeMap<_, _> = count_words("the cat the dog the end").into_iter().collect();
    println!("count_words : {counts:?}");

    let groups: BTreeMap<_, _> = group_by_len(&["a", "bb", "c", "dd", "eee"])
        .into_iter()
        .collect();
    println!("group_by_len: {groups:?}");

    let mut stock = HashMap::from([("bolt".to_string(), 2)]);
    for _ in 0..3 {
        let left = take_one(&mut stock, "bolt".to_string());
        println!("take_one    : {left:?}, keys left: {}", stock.len());
    }

    // `entry` alone inserts nothing, but a miss makes room for one item.
    let mut full: HashMap<u32, u32> = HashMap::with_capacity(3);
    let mut next = 0;
    while full.len() < full.capacity() {
        full.insert(next, next);
        next += 1;
    }
    let before = full.capacity();
    let _ = full.entry(next);
    println!(
        "entry miss  : len stays {}, capacity {before} -> {}",
        full.len(),
        full.capacity()
    );
}
