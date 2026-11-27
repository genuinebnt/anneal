from y17_parts.common import broken

TYPES = r'''/// Where changes are made durable. `commit` may block while it writes.
pub trait Journal: Send + 'static {
    /// Saves these changes, in order. On an error, none of them can be relied on.
    fn commit(&mut self, changes: &[(String, i64)]) -> io::Result<()>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Add(String, i64),
    Get(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallError {
    /// The batch this request was in could not be saved.
    NotSaved,
    /// The owner has stopped.
    Stopped,
}

pub type Totals = BTreeMap<String, i64>;
'''

SOLUTION = r'''use std::collections::BTreeMap;
use std::io;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

''' + TYPES + r'''
struct Call {
    request: Request,
    reply: oneshot::Sender<Result<i64, CallError>>,
}

/// A way to send requests to the owner. Cheap to clone.
#[derive(Clone)]
pub struct Handle {
    calls: mpsc::UnboundedSender<Call>,
}

impl Handle {
    pub async fn send(&self, request: Request) -> Result<i64, CallError> {
        let (reply, answer) = oneshot::channel();
        self.calls
            .send(Call { request, reply })
            .map_err(|_| CallError::Stopped)?;
        answer.await.unwrap_or(Err(CallError::Stopped))
    }
}

/// Starts the task that owns the totals.
pub fn start<J: Journal>(journal: J, max_batch: usize) -> (Handle, JoinHandle<Totals>) {
    let (calls, inbox) = mpsc::unbounded_channel();
    let task = tokio::spawn(own(journal, max_batch, inbox));
    (Handle { calls }, task)
}

async fn own<J: Journal>(
    mut journal: J,
    max_batch: usize,
    mut inbox: mpsc::UnboundedReceiver<Call>,
) -> Totals {
    let mut totals = Totals::new();
    while let Some(first) = inbox.recv().await {
        // take what is already waiting, without waiting for more
        let mut batch = vec![first];
        while batch.len() < max_batch {
            let Ok(call) = inbox.try_recv() else {
                break;
            };
            batch.push(call);
        }
        // apply to a copy of what changes, so that a failed commit leaves the totals as saved
        let mut touched = Totals::new();
        let mut changes = Vec::new();
        let mut answers = Vec::with_capacity(batch.len());
        for call in batch {
            let answer = match call.request {
                Request::Add(name, delta) => {
                    let old = touched.get(&name).or_else(|| totals.get(&name));
                    let new = old.copied().unwrap_or(0).wrapping_add(delta);
                    touched.insert(name.clone(), new);
                    changes.push((name, delta));
                    new
                }
                Request::Get(name) => {
                    let value = touched.get(&name).or_else(|| totals.get(&name));
                    value.copied().unwrap_or(0)
                }
            };
            answers.push((call.reply, answer));
        }
        if !changes.is_empty() && journal.commit(&changes).is_err() {
            // closed first, so that a caller who hears of the failure cannot get another request in
            inbox.close();
            for (reply, _) in answers {
                let _ = reply.send(Err(CallError::NotSaved));
            }
            break;
        }
        totals.extend(touched);
        for (reply, answer) in answers {
            // the caller may have given up; its change stands
            let _ = reply.send(Ok(answer));
        }
    }
    totals
}
'''

STARTER = r'''use std::collections::BTreeMap;
use std::io;
use tokio::task::JoinHandle;

''' + TYPES + r'''
/// A way to send requests to the owner. Cheap to clone.
#[derive(Clone)]
pub struct Handle {
    // add what you need
}

impl Handle {
    /// Sends a request and waits for its answer: the total of its name.
    pub async fn send(&self, request: Request) -> Result<i64, CallError> {
        todo!()
    }
}

/// Starts the task that owns the totals. It saves at most `max_batch` requests' changes at once.
pub fn start<J: Journal>(journal: J, max_batch: usize) -> (Handle, JoinHandle<Totals>) {
    todo!()
}
'''

HELPERS = r'''
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
'''

VISIBLE = [
    HELPERS,
    r'''
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
''',
]

HIDDEN = [
    HELPERS,
    r'''
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
''',
]

STATEMENT = r'''
A points service must not lose a change it has confirmed, so each change is saved to disk before the caller hears "done". Saving is slow, and the cost is mostly **per save**, not per change. A thousand callers should not need a thousand saves.

Write `start(journal, max_batch)`. It spawns a Tokio task that **owns** the totals, and returns a `Handle` and the task's `JoinHandle`, which gives the final totals. `max_batch` is at least 1.

**`Handle::send(request)`** sends a request and waits for its answer. `Add(name, delta)` adds to a total (wrapping) and answers the new total. `Get(name)` answers the total, 0 if none. `Handle` can be cloned.

Each request is sent when its future is **first polled**, and requests are served in the order sent.

**Batches.** When the owner is free and requests are waiting, it takes the oldest, plus all that are **already waiting**, up to `max_batch` requests in all. It never waits for more to arrive.

- It applies the batch in order. A `Get` sees the changes before it in the same batch.
- If the batch has any `Add`, it calls `journal.commit` **once**, with all the batch's `Add`s in order. A batch of `Get`s commits nothing.
- It answers only **after** the commit has returned.

**Failure.** If `commit` fails, every request in that batch, reads included, gets `NotSaved`. The owner then stops.

Waiting and later requests get `Stopped`, including any sent after a caller learns of the failure. The final totals hold **only what was saved**: an entry for each name with a saved `Add`, and no other.

**Ending.** When every `Handle` is gone, the owner serves what was already sent, then ends. A caller that drops its future after sending does not undo its request.

`commit` is an ordinary blocking function; you may call it from the owner's task. The tests run mostly on a single-threaded runtime with its clock paused, and send many requests before the owner gets to run.
'''

PROBLEM = dict(
    slug="one-owner-one-sync", title="Many callers, slow saves", level="medium", stage="network",
    tags=["actor", "mpsc and oneshot", "group commit", "batching"],
    statement=STATEMENT, starter=STARTER, solution=SOLUTION, visible=VISIBLE, hidden=HIDDEN,
    crates=["tokio"],
    teaches=[
        "A task that owns the data, reached by messages with a oneshot for the answer, needs no lock.",
        "Draining what already waits gives batches that grow with the load, and add no delay when idle.",
        "Answer only after the commit, and never let an unsaved change be seen as saved.",
    ],
    follow_up="1) `commit` blocks a runtime worker. When does that matter, and how would `spawn_blocking` change the loop? 2) How does batching change latency and throughput as load rises? 3) Why can a failed `fsync` not simply be retried?",
    hints=[
        ("approach", "Give each request a one-use channel for its answer, and put both in one queue. The owner takes one request, then whatever else is in the queue, before it saves anything."),
        ("rust", "`mpsc::UnboundedSender::send` is not async, so the request goes out on the first poll. `try_recv` drains without waiting. A dropped `oneshot::Sender` shows up at the receiver as an error you can map to `Stopped`."),
        ("edge", "Where do the changes live until the commit succeeds? In which order must the owner close its queue and send the failures, so that nobody can slip a request in between?"),
    ],
    notes=("Callers send a request with a oneshot sender. The owner takes one, drains what is already queued, applies it to a scratch copy of the touched totals, commits once and only then publishes the totals and the answers. On failure it closes its queue before answering.",
           "O(batch) per commit", "O(names + waiting requests)"),
    wrong={
        "one_commit_per_change": broken(SOLUTION, ("        while batch.len() < max_batch {", "        while batch.len() < 1 {")),
        "batch_limit_ignored": broken(SOLUTION, ("        while batch.len() < max_batch {", "        let _ = max_batch;\n        loop {")),
        "answers_before_the_commit": broken(SOLUTION, (
            "        if !changes.is_empty() && journal.commit(&changes).is_err() {\n            // closed first, so that a caller who hears of the failure cannot get another request in\n            inbox.close();\n            for (reply, _) in answers {\n                let _ = reply.send(Err(CallError::NotSaved));\n            }\n            break;\n        }\n        totals.extend(touched);\n        for (reply, answer) in answers {\n            // the caller may have given up; its change stands\n            let _ = reply.send(Ok(answer));\n        }",
            "        for (reply, answer) in answers {\n            let _ = reply.send(Ok(answer));\n        }\n        if !changes.is_empty() && journal.commit(&changes).is_err() {\n            break;\n        }\n        totals.extend(touched);")),
        "keeps_going_after_a_failure": broken(SOLUTION, (
            "            inbox.close();\n            for (reply, _) in answers {\n                let _ = reply.send(Err(CallError::NotSaved));\n            }\n            break;",
            "            for (reply, _) in answers {\n                let _ = reply.send(Err(CallError::NotSaved));\n            }\n            continue;")),
        "unsaved_changes_stay_in_the_totals": broken(SOLUTION, (
            "            for (reply, _) in answers {\n                let _ = reply.send(Err(CallError::NotSaved));\n            }\n            break;",
            "            for (reply, _) in answers {\n                let _ = reply.send(Err(CallError::NotSaved));\n            }\n            totals.extend(touched);\n            break;")),
        "reads_ignore_the_batch": broken(SOLUTION, (
            "                Request::Get(name) => {\n                    let value = touched.get(&name).or_else(|| totals.get(&name));",
            "                Request::Get(name) => {\n                    let value = totals.get(&name);")),
        "waits_a_moment_to_fill_the_batch": broken(SOLUTION, (
            "        let mut batch = vec![first];\n", "        let mut batch = vec![first];\n        tokio::time::sleep(std::time::Duration::from_millis(1)).await;\n")),
    },
)
