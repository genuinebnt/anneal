use solution::*;

#[test]
fn submit_returns_the_result() {
    check!(r#"submit(|| 6 * 7)"#, { let p = ThreadPool::new(2); p.submit(|| 6 * 7).join() }, Ok(42));
}

#[test]
fn all_jobs_run() {
    check!(r#"20 jobs add to a counter; shutdown"#, { let n = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)); let p = ThreadPool::new(3); for _ in 0..20 { let n = n.clone(); p.execute(move || { n.fetch_add(1, std::sync::atomic::Ordering::SeqCst); }); } p.shutdown(); n.load(std::sync::atomic::Ordering::SeqCst) }, 20);
}

#[test]
fn runs_off_the_calling_thread() {
    check!(r#"job thread id differs"#, { let me = std::thread::current().id(); let p = ThreadPool::new(1); p.submit(move || std::thread::current().id() != me).join() }, Ok(true));
}

#[test]
fn single_worker_is_fifo() {
    check!(r#"one worker keeps submit order"#, { let p = ThreadPool::new(1); let hs: Vec<_> = (0..5).map(|i| { p.submit(move || i) }).collect(); let r: Vec<_> = hs.into_iter().map(|h| h.join().unwrap()).collect(); r }, vec![0, 1, 2, 3, 4]);
}

#[test]
fn panic_becomes_err() {
    check!(r#"a panicking task"#, { let p = ThreadPool::new(1); let h = p.submit(|| -> u8 { panic!("boom") }); h.join() }, Err(TaskPanicked));
}
