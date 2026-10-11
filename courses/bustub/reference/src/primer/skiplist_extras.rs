//! Extras over a skip list, written against its public view.

use crate::primer::skiplist::SkipList;

/// The views `check_levels` works on: for every level, the keys linked there; the (key, height) list of level 0; the size.
pub fn check_levels(levels: &[Vec<i32>], nodes: &[(i32, usize)], size: usize) -> Result<(), String> {
    // @begin 0b-c2
    let Some(bottom) = levels.first() else {
        return if size == 0 && nodes.is_empty() { Ok(()) } else { Err("no levels but the list is not empty".into()) };
    };
    if bottom.len() != size || nodes.len() != size {
        return Err(format!("level 0 has {} keys, nodes() has {}, size() is {size}", bottom.len(), nodes.len()));
    }
    for (l, keys) in levels.iter().enumerate() {
        if let Some(w) = keys.windows(2).find(|w| w[0] >= w[1]) {
            return Err(format!("level {l} is not strictly increasing at {} then {}", w[0], w[1]));
        }
        if l > 0 {
            let below = &levels[l - 1];
            let mut it = below.iter();
            for k in keys {
                if !it.any(|b| b == k) {
                    return Err(format!("key {k} is on level {l} but not (in order) on level {}", l - 1));
                }
            }
        }
    }
    for (k, h) in nodes {
        let appears = levels.iter().filter(|lv| lv.contains(k)).count();
        if *h < 1 || *h != appears {
            return Err(format!("key {k} has height {h} but appears on {appears} levels"));
        }
    }
    Ok(())
    //~ todo!("0b-c2: level 0 holds every key in order; each higher level is an ordered subsequence of the one below; heights match the levels a key is on")
    // @end
}

/// Checks `list` through its public view.
pub fn check_integrity(list: &SkipList<i32>) -> Result<(), String> {
    // @begin 0b-c2
    let nodes = list.nodes();
    let top = nodes.iter().map(|n| n.1).max().unwrap_or(0);
    let levels: Vec<Vec<i32>> = (0..top).map(|l| list.level(l)).collect();
    check_levels(&levels, &nodes, list.size())
    //~ todo!("0b-c2: collect the levels from the list and check them")
    // @end
}

/// How many keys `k` satisfy `lo <= k <= hi`.
pub fn range_count(list: &SkipList<i32>, lo: i32, hi: i32) -> usize {
    // @begin 0b-c3
    if hi < lo {
        return 0;
    }
    let keys = list.level(0);
    keys.partition_point(|&k| k <= hi) - keys.partition_point(|&k| k < lo)
    //~ todo!("0b-c3: binary search both ends of the sorted bottom level")
    // @end
}

/// The largest key `<= k`.
pub fn floor(list: &SkipList<i32>, k: i32) -> Option<i32> {
    // @begin 0b-c3
    let keys = list.level(0);
    let at = keys.partition_point(|&x| x <= k);
    at.checked_sub(1).map(|i| keys[i])
    //~ todo!("0b-c3: the last key not above k")
    // @end
}

/// A new list with every key of `a` and of `b`.
pub fn union(a: &SkipList<i32>, b: &SkipList<i32>) -> SkipList<i32> {
    // @begin 0b-c5
    let out = SkipList::new();
    for k in a.level(0).into_iter().chain(b.level(0)) {
        out.insert(&k);
    }
    out
    //~ todo!("0b-c5: insert every key of both lists into a new list")
    // @end
}
