use std::thread;

/// Sums each chunk on its own thread. The totals come back in chunk order.
pub fn sum_chunks(chunks: Vec<Vec<u64>>) -> Vec<u64> {
    let handles: Vec<thread::JoinHandle<u64>> = chunks
        .into_iter()
        .map(|chunk| thread::spawn(move || chunk.iter().sum()))
        .collect();
    handles
        .into_iter()
        .map(|h| h.join().expect("worker panicked"))
        .collect()
}

/// Splits `data` into `parts` contiguous pieces whose lengths differ by at most one (longer pieces first),
/// sums each piece on its own scoped thread, and returns the sums in order. `parts` is at least 1.
pub fn sum_parts(data: &[u64], parts: usize) -> Vec<u64> {
    let (base, extra) = (data.len() / parts, data.len() % parts);
    thread::scope(|s| {
        let mut handles = Vec::with_capacity(parts);
        let mut start = 0;
        for i in 0..parts {
            let len = base + usize::from(i < extra);
            let piece = &data[start..start + len];
            start += len;
            // `move` copies the `&[u64]` into the thread; the data itself stays borrowed from the caller.
            handles.push(s.spawn(move || piece.iter().sum::<u64>()));
        }
        handles
            .into_iter()
            .map(|h| h.join().expect("worker panicked"))
            .collect()
    })
}
