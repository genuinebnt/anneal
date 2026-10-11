//! Probe-distance statistics over a Robin Hood hash set.

use crate::primer::robin_hood_hash_set::RobinHoodHashSet;

/// `h[d]` = how many of `keys` that are in `set` sit `d` buckets after their home bucket.
pub fn probe_histogram(set: &RobinHoodHashSet<i32>, keys: &[i32]) -> Vec<usize> {
    todo!("0c-c2: for each found key, the distance from its home bucket to the bucket it is in")
}

/// The mean distance of the found keys.
pub fn mean_probe(set: &RobinHoodHashSet<i32>, keys: &[i32]) -> f64 {
    todo!("0c-c2: the weighted average of the histogram")
}
