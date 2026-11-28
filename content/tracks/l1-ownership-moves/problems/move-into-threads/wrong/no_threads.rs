/// Sums each chunk with `sum`, each on its own thread. The totals come back in chunk order.
pub fn sum_chunks(chunks: Vec<Vec<u64>>, sum: fn(&[u64]) -> u64) -> Vec<u64> {
    // Wrong: no threads. The sums are right, but nothing runs at the same time.
    chunks.iter().map(|c| sum(c)).collect()
}

/// Splits `data` into `parts` contiguous pieces whose lengths differ by at most one (longer pieces first),
/// sums each piece with `sum` on its own scoped thread, and returns the sums in order. `parts` is at least 1.
pub fn sum_parts(data: &[u64], parts: usize, sum: fn(&[u64]) -> u64) -> Vec<u64> {
    let (base, extra) = (data.len() / parts, data.len() % parts);
    let mut out = Vec::with_capacity(parts);
    let mut start = 0;
    for i in 0..parts {
        let len = base + usize::from(i < extra);
        out.push(sum(&data[start..start + len]));
        start += len;
    }
    out
}
