//! RECIPE 44 (collections): Vec, HashMap, HashSet, BTreeMap, VecDeque: the methods that save you loops.
//! LEVEL: simple
//! Use in: every stage; choosing the right container is half of performance.   Run: cargo run --example collections_tour
//! COMMON USES: choosing the right container: counting, grouping, deduping, sorted ranges, queues.
#![allow(dead_code)]
//!
//! Choosing: Vec (default) | VecDeque (queue/both ends) | HashMap/HashSet (lookup, unordered) | BTreeMap/BTreeSet
//! (sorted, range queries, deterministic iteration) | BinaryHeap (priority) | never LinkedList.
//! Docs: https://doc.rust-lang.org/std/collections/index.html
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

fn main() {
    // ---- Vec ----
    let mut v = vec![5, 3, 8, 3, 1, 8, 9];
    v.sort_unstable();
    v.dedup(); // removes CONSECUTIVE duplicates: sort first
    println!("{v:?} {:?} {:?}", v.binary_search(&8), v.binary_search(&4)); // Err(pos) = insertion point
    v.retain(|&x| x != 5);
    let drained: Vec<i32> = v.drain(..2).collect(); // remove + yield a range
    println!("{drained:?} {v:?}");
    let tail = v.split_off(1);
    println!("{v:?} {tail:?} {:?} {:?}", v.first(), tail.last());
    let mut w = vec![1, 2, 3, 4, 5];
    w.swap_remove(0); // O(1) remove, does not keep order
    w.insert(1, 99);
    w.rotate_left(1);
    w.truncate(4);
    w.extend_from_slice(&[7, 7]);
    println!(
        "{w:?} {:?} {}",
        w.iter().rev().take(2).collect::<Vec<_>>(),
        vec![0u8; 3].len()
    );
    println!("{:?}", [1, 2, 3, 4, 5].chunks(2).collect::<Vec<_>>());
    let mut people = vec![("bo", 30), ("al", 30), ("cy", 25)];
    people.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0))); // multi-key sort
    println!("{people:?} contains={}", people.iter().any(|p| p.0 == "al"));

    // ---- HashMap ----
    let mut wc: HashMap<&str, u32> = HashMap::new();
    for w in "a b a c b a".split(' ') {
        *wc.entry(w).or_insert(0) += 1;
    }
    wc.entry("z").and_modify(|n| *n += 100).or_insert(1);
    let mut items: Vec<_> = wc.iter().collect();
    items.sort(); // HashMap order is random: sort before printing
    println!("{items:?}");
    let mut groups: HashMap<usize, Vec<&str>> = HashMap::new();
    for word in ["a", "bb", "cc", "d"] {
        groups.entry(word.len()).or_default().push(word); // group-by idiom
    }
    let mut keys: Vec<_> = groups.keys().copied().collect();
    keys.sort();
    println!("{keys:?} {:?}", groups[&2]);
    wc.retain(|_, n| *n > 1);
    println!("{} left, remove -> {:?}", wc.len(), wc.remove("a"));
    let from_pairs: HashMap<_, _> = [("k1", 1), ("k2", 2)].into_iter().collect();
    println!("{}", from_pairs.values().sum::<i32>());

    // ---- sets ----
    let a: HashSet<_> = [1, 2, 3, 4].into();
    let b: HashSet<_> = [3, 4, 5].into();
    let mut inter: Vec<_> = a.intersection(&b).copied().collect();
    inter.sort();
    let mut diff: Vec<_> = a.difference(&b).copied().collect();
    diff.sort();
    println!("{inter:?} {diff:?} subset={}", a.is_subset(&b));
    let mut seen = HashSet::new();
    println!(
        "insert returns true when new: {} then {}",
        seen.insert("x"),
        seen.insert("x")
    ); // free "seen before?"

    // ---- BTreeMap: sorted, range queries ----
    let mut events: BTreeMap<u64, &str> = BTreeMap::new();
    for (t, e) in [(10, "start"), (20, "mid"), (30, "end"), (45, "late")] {
        events.insert(t, e);
    }
    println!("{:?}", events.range(15..=35).collect::<Vec<_>>());
    println!(
        "{:?} {:?}",
        events.first_key_value(),
        events.last_key_value()
    );
    println!("{:?}", events.range(..25).next_back()); // greatest key < 25
    let s: BTreeSet<_> = ["pear", "apple", "fig"].into_iter().collect();
    println!("{s:?}");

    // ---- VecDeque: efficient at both ends ----
    let mut q: VecDeque<i32> = VecDeque::new();
    q.push_back(1);
    q.push_back(2);
    q.push_front(0);
    q.rotate_left(1);
    println!("{q:?}");
    let front = q.pop_front();
    println!("popped {front:?}, back is {:?}, now {q:?}", q.back());
}
