use std::thread::{self, JoinHandle};

/// Sums `data` using up to `chunks` threads, and returns once they're done.
pub fn parallel_sum(data: &[u64], chunks: usize) -> u64 {
    let size = data.len().div_ceil(chunks.max(1)).max(1);
    let handles: Vec<_> = data
        .chunks(size)
        .map(|chunk| thread::spawn(move || chunk.iter().sum::<u64>()))
        .collect();
    handles
        .into_iter()
        .map(|h| h.join().expect("worker panicked"))
        .sum()
}

/// Starts counting the whitespace-separated words of `docs` in the background. The caller joins later,
/// possibly after dropping `docs`.
pub fn count_later(docs: &[String]) -> JoinHandle<usize> {
    thread::spawn(move || docs.iter().map(|d| d.split_whitespace().count()).sum())
}

/// Runs `job` on a new thread called `name`.
pub fn spawn_named<F, T>(name: &str, job: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send,
    T: Send,
{
    thread::Builder::new()
        .name(name.to_string())
        .spawn(job)
        .expect("spawning a thread")
}
