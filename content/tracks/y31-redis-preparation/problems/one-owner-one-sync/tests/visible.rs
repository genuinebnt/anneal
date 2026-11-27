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
fn one_at_a_time() {
    run(async {
        let book = Book::default();
        let (handle, _) = start(book.clone(), 10);
        check!("add", handle.send(add("a", 5)).await, Ok(5));
        check!("add", handle.send(add("a", 2)).await, Ok(7));
        check!("get", handle.send(get("a")).await, Ok(7));
        check!("get of nothing", handle.send(get("z")).await, Ok(0));
        check!("two commits; reads need none", book.commits(), vec![vec![change("a", 5)], vec![change("a", 2)]]);
    });
}

#[test]
fn requests_that_wait_together_are_saved_together() {
    run(async {
        let book = Book::default();
        let (handle, _) = start(book.clone(), 10);
        let answers = all_at_once(&handle, vec![add("a", 1), add("b", 2), get("a"), add("a", 3)]).await;
        check!("answers in order of sending", answers, vec![Ok(1), Ok(2), Ok(1), Ok(4)]);
        check!("one commit", book.commits(), vec![vec![change("a", 1), change("b", 2), change("a", 3)]]);
    });
}

#[test]
fn a_batch_has_a_limit() {
    run(async {
        let book = Book::default();
        let (handle, _) = start(book.clone(), 3);
        all_at_once(&handle, (0..8).map(|i| add("n", i)).collect()).await;
        check!("sizes", book.commits().iter().map(Vec::len).collect::<Vec<_>>(), vec![3, 3, 2]);
    });
}

#[test]
fn a_failed_commit_answers_not_saved_and_stops() {
    run(async {
        let book = Book::failing(1);
        let (handle, task) = start(book.clone(), 10);
        check!("saved", handle.send(add("a", 1)).await, Ok(1));
        let answers = all_at_once(&handle, vec![add("a", 1), get("a")]).await;
        check!("the whole batch, reads too", answers, vec![Err(CallError::NotSaved), Err(CallError::NotSaved)]);
        check!("afterwards", handle.send(get("a")).await, Err(CallError::Stopped));
        check!("the totals as last saved", task.await.unwrap(), Totals::from([("a".to_string(), 1)]));
    });
}

#[test]
fn the_task_ends_when_the_handles_are_gone() {
    run(async {
        let (handle, task) = start(Book::default(), 4);
        let other = handle.clone();
        handle.send(add("a", 2)).await.unwrap();
        drop(handle);
        check!("one handle is left", other.send(add("a", 2)).await, Ok(4));
        drop(other);
        check!("totals", task.await.unwrap(), Totals::from([("a".to_string(), 4)]));
    });
}
