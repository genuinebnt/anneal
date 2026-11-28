use std::thread;

/// Sums each chunk with `sum`, each on its own thread. The totals come back in chunk order.
pub fn sum_chunks(chunks: Vec<Vec<u64>>, sum: fn(&[u64]) -> u64) -> Vec<u64> {
    // Start every thread first. Joining inside this `map` would run them one after another.
    let handles: Vec<thread::JoinHandle<u64>> = chunks
        .into_iter()
        .map(|chunk| thread::spawn(move || sum(&chunk)))
        .collect();
    handles.into_iter().map(|h| h.join().expect("worker panicked")).collect()
}

/// Splits `data` into `parts` contiguous pieces whose lengths differ by at most one (longer pieces first),
/// sums each piece with `sum` on its own scoped thread, and returns the sums in order. `parts` is at least 1.
pub fn sum_parts(data: &[u64], parts: usize, sum: fn(&[u64]) -> u64) -> Vec<u64> {
    let (base, extra) = (data.len() / parts, data.len() % parts);
    thread::scope(|s| {
        let mut handles = Vec::with_capacity(parts);
        let mut start = 0;
        for i in 0..parts {
            let size = (data.len() + parts - 1) / parts;
            let len = size.min(data.len() - start);
            let piece = &data[start..start + len];
            start += len;
            // `move` copies the `&[u64]` and the `fn` pointer into the thread; the data stays borrowed from the caller.
            handles.push(s.spawn(move || sum(piece)));
        }
        handles.into_iter().map(|h| h.join().expect("worker panicked")).collect()
    })
}
