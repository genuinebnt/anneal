use std::thread;

/// `n` threads each sum `data`; returns every thread's result.
pub fn sum_everywhere(data: Vec<u64>, n: usize) -> Vec<u64> {
    let mut handles = Vec::new();
    for _ in 0..n {
        handles.push(thread::spawn(move || data.iter().sum::<u64>()));
    }
    handles
        .into_iter()
        .map(|h| h.join().expect("worker panicked"))
        .collect()
}
