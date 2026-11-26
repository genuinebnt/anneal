#[cfg(not(loom))]
fn main() {
    use acquire_release_ordering::{countdown, litmus, publish};
    use std::sync::atomic::Ordering;
    use std::thread;

    let (publisher, reader) = publish::slot();
    let seen = thread::scope(|s| {
        s.spawn(move || publisher.publish(42));
        loop {
            if let Some(value) = reader.try_read() {
                break value;
            }
            thread::yield_now();
        }
    });
    println!("publication: reader saw {seen}");

    let [a, b] = countdown::pair();
    let (x, y) = thread::scope(|s| {
        let a = s.spawn(move || a.finish(2));
        let b = s.spawn(move || b.finish(3));
        (a.join().unwrap(), b.join().unwrap())
    });
    println!("last one out: sum = {:?}", x.or(y));

    // Miri interprets the program, so it gets far fewer rounds.
    let rounds = if cfg!(miri) { 100 } else { 100_000 };
    let strong = litmus::count_stale_reads(rounds, Ordering::Release, Ordering::Acquire);
    println!("release/acquire: {strong} stale reads in {rounds} rounds (the language says 0)");
    let weak = litmus::count_stale_reads(rounds, Ordering::Relaxed, Ordering::Relaxed);
    println!("relaxed/relaxed: {weak} stale reads in {rounds} rounds (any number is allowed)");
}

#[cfg(loom)]
fn main() {
    println!("built with --cfg loom: run `cargo test --release --test model` instead");
}
