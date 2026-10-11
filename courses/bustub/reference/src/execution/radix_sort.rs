//! A stable LSD radix sort on u32 keys.

/// Sorts `(key, payload)` pairs by key, stably, in four byte-wise counting-sort passes from the least significant byte: linear in the number of
/// rows.
pub fn radix_sort(rows: &mut Vec<(u32, u32)>) {
    // @begin 3g-c5
    let mut buf = vec![(0u32, 0u32); rows.len()];
    for shift in [0u32, 8, 16, 24] {
        let mut count = [0usize; 257];
        for r in rows.iter() {
            count[((r.0 >> shift) & 0xFF) as usize + 1] += 1;
        }
        for i in 0..256 {
            count[i + 1] += count[i];
        }
        for r in rows.iter() {
            let b = ((r.0 >> shift) & 0xFF) as usize;
            buf[count[b]] = *r;
            count[b] += 1;
        }
        std::mem::swap(rows, &mut buf);
    }
    //~ todo!("3g-c5: four stable counting-sort passes, least significant byte first")
    // @end
}
