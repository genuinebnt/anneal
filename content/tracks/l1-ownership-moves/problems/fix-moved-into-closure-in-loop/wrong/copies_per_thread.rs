use std::thread;

/// `n` threads each sum `data`; returns every thread's result.
pub fn sum_everywhere(data: Vec<u64>, n: usize) -> Vec<u64> {
    let mut handles = Vec::new();
    for _ in 0..n {
        let mine: Vec<u64> = data.iter().copied().collect();
        handles.push(thread::spawn(move || mine.iter().sum::<u64>()));
    }
    handles.into_iter().map(|h| h.join().expect("worker panicked")).collect()
}
