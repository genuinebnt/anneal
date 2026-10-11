//! A Grace hash join: partition both sides, then join each pair of partitions in memory.

use std::collections::HashMap;

pub type Row = (i64, u32);

/// The bucket of `key` among `k` buckets (`k >= 1`).
pub fn bucket_of(key: i64, k: usize) -> usize {
    let mut x = key as u64 ^ 0x9E37_79B9_7F4A_7C15;
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    ((x ^ (x >> 31)) % k as u64) as usize
}

/// Splits `rows` into `k` buckets by the hash of their key, so rows with equal keys land in the same bucket, in input order.
pub fn partition(rows: &[Row], k: usize) -> Vec<Vec<Row>> {
    // @begin 3f-c3
    let mut buckets = vec![Vec::new(); k];
    for &r in rows {
        buckets[bucket_of(r.0, k)].push(r);
    }
    buckets
    //~ todo!("3f-c3: send each row to the bucket of its key")
    // @end
}

/// The equi-join of two row sets on their keys, as `(left payload, right payload)` pairs. Both sides are partitioned into `k` buckets and each pair
/// of buckets is joined in memory, so only one bucket's hash table exists at a time.
pub fn grace_join(left: &[Row], right: &[Row], k: usize) -> Vec<(u32, u32)> {
    // @begin 3f-c3
    let (lp, rp) = (partition(left, k), partition(right, k));
    let mut out = Vec::new();
    for (l, r) in lp.iter().zip(&rp) {
        let mut table: HashMap<i64, Vec<u32>> = HashMap::new();
        for &(key, payload) in l {
            table.entry(key).or_default().push(payload);
        }
        for &(key, rp) in r {
            if let Some(ls) = table.get(&key) {
                out.extend(ls.iter().map(|&lp| (lp, rp)));
            }
        }
    }
    out
    //~ todo!("3f-c3: for each pair of buckets, build a table on the left and probe with the right")
    // @end
}
