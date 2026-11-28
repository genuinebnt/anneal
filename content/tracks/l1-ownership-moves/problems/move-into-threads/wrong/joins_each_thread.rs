use std::thread;

/// Sums each chunk with `sum`, each on its own thread. The totals come back in chunk order.
pub fn sum_chunks(chunks: Vec<Vec<u64>>, sum: fn(&[u64]) -> u64) -> Vec<u64> {
    // Wrong: each thread is joined before the next one starts, so they run one after another.
    chunks
        .into_iter()
        .map(|chunk| thread::spawn(move || sum(&chunk)).join().expect("worker panicked"))
        .collect()
}

/// Splits `data` into `parts` contiguous pieces whose lengths differ by at most one (longer pieces first),
/// sums each piece with `sum` on its own scoped thread, and returns the sums in order. `parts` is at least 1.
pub fn sum_parts(data: &[u64], parts: usize, sum: fn(&[u64]) -> u64) -> Vec<u64> {
    let (base, extra) = (data.len() / parts, data.len() % parts);
    thread::scope(|s| {
        let mut out = Vec::with_capacity(parts);
        let mut start = 0;
        for i in 0..parts {
            let len = base + usize::from(i < extra);
            let piece = &data[start..start + len];
            start += len;
            // Wrong: joined right away, so the next piece waits for this one.
            out.push(s.spawn(move || sum(piece)).join().expect("worker panicked"));
        }
        out
    })
}
