//! Extras over a skip list, written against its public view.

use crate::primer::skiplist::SkipList;

/// The views `check_levels` works on: for every level, the keys linked there; the (key, height) list of level 0; the size.
pub fn check_levels(levels: &[Vec<i32>], nodes: &[(i32, usize)], size: usize) -> Result<(), String> {
    todo!("0b-c2: level 0 holds every key in order; each higher level is an ordered subsequence of the one below; heights match the levels a key is on")
}

/// Checks `list` through its public view.
pub fn check_integrity(list: &SkipList<i32>) -> Result<(), String> {
    todo!("0b-c2: collect the levels from the list and check them")
}

/// How many keys `k` satisfy `lo <= k <= hi`.
pub fn range_count(list: &SkipList<i32>, lo: i32, hi: i32) -> usize {
    todo!("0b-c3: binary search both ends of the sorted bottom level")
}

/// The largest key `<= k`.
pub fn floor(list: &SkipList<i32>, k: i32) -> Option<i32> {
    todo!("0b-c3: the last key not above k")
}

/// A new list with every key of `a` and of `b`.
pub fn union(a: &SkipList<i32>, b: &SkipList<i32>) -> SkipList<i32> {
    todo!("0b-c5: insert every key of both lists into a new list")
}
