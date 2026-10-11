//! Probe-distance statistics over a Robin Hood hash set.

use crate::primer::robin_hood_hash_set::RobinHoodHashSet;

/// `h[d]` = how many of `keys` that are in `set` sit `d` buckets after their home bucket.
pub fn probe_histogram(set: &RobinHoodHashSet<i32>, keys: &[i32]) -> Vec<usize> {
    // @begin 0c-c2
    let mut h: Vec<usize> = Vec::new();
    for k in keys {
        if set.contains(k) {
            let d = set.probe_distance(set.home_bucket(k), set.get_bucket(k));
            if h.len() <= d {
                h.resize(d + 1, 0);
            }
            h[d] += 1;
        }
    }
    h
    //~ todo!("0c-c2: for each found key, the distance from its home bucket to the bucket it is in")
    // @end
}

/// The mean distance of the found keys.
pub fn mean_probe(set: &RobinHoodHashSet<i32>, keys: &[i32]) -> f64 {
    // @begin 0c-c2
    let h = probe_histogram(set, keys);
    let n: usize = h.iter().sum();
    if n == 0 {
        return 0.0;
    }
    h.iter().enumerate().map(|(d, c)| d * c).sum::<usize>() as f64 / n as f64
    //~ todo!("0c-c2: the weighted average of the histogram")
    // @end
}
