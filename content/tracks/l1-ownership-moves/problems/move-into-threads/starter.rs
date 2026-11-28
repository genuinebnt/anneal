/// Sums each chunk with `sum`, each on its own thread. The totals come back in chunk order.
pub fn sum_chunks(chunks: Vec<Vec<u64>>, sum: fn(&[u64]) -> u64) -> Vec<u64> {
    todo!()
}

/// Splits `data` into `parts` contiguous pieces whose lengths differ by at most one (longer pieces first),
/// sums each piece with `sum` on its own scoped thread, and returns the sums in order. `parts` is at least 1.
pub fn sum_parts(data: &[u64], parts: usize, sum: fn(&[u64]) -> u64) -> Vec<u64> {
    todo!()
}
