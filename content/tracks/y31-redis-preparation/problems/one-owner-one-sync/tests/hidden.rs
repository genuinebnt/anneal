use solution::*;


use std::future::Future;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[allow(dead_code)]
fn run<F: std::future::Future<Output = ()>>(body: F) {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap();
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(60), body).await.expect("the test hung");
    });
}

/// Keeps every commit it was given. The commit with number `fail_at` (from 0) fails.
#[allow(dead_code)]
#[derive(Clone, Default)]
struct Book {
    commits: Arc<Mutex<Vec<Vec<(String, i64)>>>>,
    fail_at: Option<usize>,
}
#[allow(dead_code)]
impl Book {
    fn failing(at: usize) -> Book {
        Book { fail_at: Some(at), ..Book::default() }
    }
    fn commits(&self) -> Vec<Vec<(String, i64)>> {
        self.commits.lock().unwrap().clone()
    }
}
impl Journal for Book {
    fn commit(&mut self, changes: &[(String, i64)]) -> io::Result<()> {
        let mut commits = self.commits.lock().unwrap();
        let n = commits.len();
        commits.push(changes.to_vec());
        if self.fail_at == Some(n) {
            return Err(io::Error::from(io::ErrorKind::StorageFull));
        }
        Ok(())
    }
}

#[allow(dead_code)]
fn add(name: &str, delta: i64) -> Request {
    Request::Add(name.to_string(), delta)
}
#[allow(dead_code)]
fn get(name: &str) -> Request {
    Request::Get(name.to_string())
}
#[allow(dead_code)]
fn change(name: &str, delta: i64) -> (String, i64) {
    (name.to_string(), delta)
}

/// Starts all the requests before any of them can be answered, and returns the answers in order.
#[allow(dead_code)]
async fn all_at_once(handle: &Handle, requests: Vec<Request>) -> Vec<Result<i64, CallError>> {
    let mut calls: Vec<_> = requests.into_iter().map(|r| Box::pin(handle.send(r))).collect();
    let mut answers: Vec<Option<Result<i64, CallError>>> = vec![None; calls.len()];
    std::future::poll_fn(|cx| {
        let mut waiting = false;
        for (call, answer) in calls.iter_mut().zip(answers.iter_mut()) {
            if answer.is_none() {
                match call.as_mut().poll(cx) {
                    std::task::Poll::Ready(a) => *answer = Some(a),
                    std::task::Poll::Pending => waiting = true,
                }
            }
        }
        if waiting { std::task::Poll::Pending } else { std::task::Poll::Ready(()) }
    })
    .await;
    answers.into_iter().map(|a| a.expect("answered")).collect()
}

#[test]
fn a_batch_of_reads_commits_nothing() {
    run(async {
        let book = Book::failing(0);
        let (handle, _) = start(book.clone(), 5);
        check!("reads", all_at_once(&handle, vec![get("a"); 12]).await, vec![Ok(0); 12]);
        check!("no commit", book.commits().len(), 0);
    });
}

#[test]
fn the_limit_counts_requests_not_changes() {
    run(async {
        let book = Book::default();
        let (handle, _) = start(book.clone(), 2);
        all_at_once(&handle, vec![add("a", 1), get("a"), add("a", 2), add("a", 3), get("a"), get("a"), add("a", 4)]).await;
        check!("commits", book.commits(), vec![vec![change("a", 1)], vec![change("a", 2), change("a", 3)], vec![change("a", 4)]]);
    });
}

#[test]
fn a_limit_of_one() {
    run(async {
        let book = Book::default();
        let (handle, _) = start(book.clone(), 1);
        all_at_once(&handle, (0..5).map(|_| add("a", 1)).collect()).await;
        check!("five commits", book.commits().len(), 5);
    });
}

#[test]
fn the_owner_does_not_wait_to_fill_a_batch() {
    run(async {
        let book = Book::default();
        let (handle, _) = start(book.clone(), 100);
        let started = tokio::time::Instant::now();
        handle.send(add("a", 1)).await.unwrap();
        // the clock is paused: any timer the owner waits on would move it
        check!("no time passed", started.elapsed(), Duration::ZERO);
    });
}

#[test]
fn requests_queued_behind_a_failed_batch_are_stopped() {
    run(async {
        let book = Book::failing(0);
        let (handle, task) = start(book.clone(), 2);
        let answers = all_at_once(&handle, (0..5).map(|_| add("a", 1)).collect()).await;
        let mut want = vec![Err(CallError::NotSaved); 2];
        want.extend(vec![Err(CallError::Stopped); 3]);
        check!("answers", answers, want);
        check!("only one commit was tried", book.commits().len(), 1);
        check!("nothing saved", task.await.unwrap(), Totals::new());
    });
}

#[test]
fn a_failure_does_not_leak_into_the_totals() {
    run(async {
        let (handle, task) = start(Book::failing(2), 10);
        handle.send(add("a", 1)).await.unwrap();
        handle.send(add("b", 1)).await.unwrap();
        let _ = all_at_once(&handle, vec![add("a", 10), add("c", 10)]).await;
        check!("as saved", task.await.unwrap(), Totals::from([("a".to_string(), 1), ("b".to_string(), 1)]));
    });
}

#[test]
fn a_caller_that_gives_up_still_has_its_change_made() {
    run(async {
        let book = Book::default();
        let (handle, task) = start(book.clone(), 10);
        {
            let call = handle.send(add("a", 5));
            let mut call = std::pin::pin!(call);
            // poll it once, so that it is sent, then drop it
            std::future::poll_fn(|cx| {
                let _ = call.as_mut().poll(cx);
                std::task::Poll::Ready(())
            })
            .await;
        }
        check!("the next caller sees it", handle.send(add("a", 1)).await, Ok(6));
        drop(handle);
        check!("totals", task.await.unwrap(), Totals::from([("a".to_string(), 6)]));
    });
}

#[test]
fn requests_sent_before_the_handle_drops_are_served() {
    run(async {
        let book = Book::default();
        let (handle, task) = start(book.clone(), 3);
        for _ in 0..10 {
            let clone = handle.clone();
            tokio::spawn(async move { clone.send(add("a", 1)).await });
        }
        tokio::task::yield_now().await;
        drop(handle);
        check!("all ten", task.await.unwrap(), Totals::from([("a".to_string(), 10)]));
    });
}

#[test]
fn totals_wrap() {
    run(async {
        let (handle, _) = start(Book::default(), 10);
        let answers = all_at_once(&handle, vec![add("a", i64::MAX), add("a", 1)]).await;
        check!("wraps", answers, vec![Ok(i64::MAX), Ok(i64::MIN)]);
    });
}

#[test]
fn many_tasks_on_many_threads() {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_time().build().unwrap();
    let body = async {
        let book = Book::default();
        let (handle, task) = start(book.clone(), 16);
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..20 {
            let handle = handle.clone();
            tasks.spawn(async move {
                let mut seen = Vec::new();
                for _ in 0..200 {
                    seen.push(handle.send(add("n", 1)).await.unwrap());
                }
                seen
            });
        }
        let mut all = Vec::new();
        while let Some(seen) = tasks.join_next().await {
            let seen = seen.unwrap();
            check!("each task sees the number only grow", seen.windows(2).all(|w| w[0] < w[1]), true);
            all.extend(seen);
        }
        all.sort_unstable();
        check!("every total once", all, (1..=4000).collect::<Vec<i64>>());
        drop(handle);
        check!("totals", task.await.unwrap(), Totals::from([("n".to_string(), 4000)]));
        check!("no batch over the limit", book.commits().iter().all(|c| !c.is_empty() && c.len() <= 16), true);
    };
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(60), body).await.expect("the test hung");
    });
}
