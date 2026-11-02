use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, Wake, Waker};

type BoxFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

struct Shared {
    queue: Mutex<VecDeque<Arc<Task>>>, // tasks that have been woken and need a poll, oldest first
    cv: Condvar,
    pending: AtomicUsize, // spawned and not yet finished
}

struct Task {
    future: Mutex<Option<BoxFuture>>, // None once finished
    shared: Arc<Shared>,
}

impl Wake for Task {
    fn wake(self: Arc<Self>) {
        let shared = self.shared.clone();
        shared.queue.lock().unwrap().push_back(self);
        shared.cv.notify_one();
    }
}

struct JoinState<T> {
    result: Option<T>,
    waker: Option<Waker>,
}

/// Awaiting a `JoinHandle` gives the task's output. Dropping it does not cancel the task.
pub struct JoinHandle<T> {
    state: Arc<Mutex<JoinState<T>>>,
}

impl<T> JoinHandle<T> {
    /// The output, if the task has finished and nobody took it yet.
    pub fn take(&self) -> Option<T> {
        self.state.lock().unwrap().result.take()
    }

    pub fn is_finished(&self) -> bool {
        self.state.lock().unwrap().result.is_some()
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let mut s = self.state.lock().unwrap();
        match s.result.take() {
            Some(v) => Poll::Ready(v),
            None => {
                s.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

/// A future that is pending exactly once and asks to be polled again at once: it sends the task to the back of the queue.
pub struct YieldNow(bool);

pub fn yield_now() -> YieldNow {
    YieldNow(false)
}

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

#[derive(Clone)]
pub struct Executor {
    shared: Arc<Shared>,
}

impl Executor {
    pub fn new() -> Self {
        Executor { shared: Arc::new(Shared { queue: Mutex::new(VecDeque::new()), cv: Condvar::new(), pending: AtomicUsize::new(0) }) }
    }

    pub fn spawn<F>(&self, fut: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let state = Arc::new(Mutex::new(JoinState { result: None, waker: None }));
        let st = state.clone();
        let wrapped = async move {
            let out = fut.await;
            let joiner = {
                let mut s = st.lock().unwrap();
                s.result = Some(out);
                s.waker.take()
            };
            if let Some(w) = joiner {
                w.wake();
            }
        };
        self.shared.pending.fetch_add(1, Ordering::SeqCst);
        let task = Arc::new(Task { future: Mutex::new(Some(Box::pin(wrapped))), shared: self.shared.clone() });
        self.shared.queue.lock().unwrap().push_back(task);
        self.shared.cv.notify_one();
        JoinHandle { state }
    }

    /// Runs tasks until every spawned task has finished (including ones spawned while running). Sleeps when nothing is
    /// ready but tasks are still waiting for a wake-up from elsewhere.
    pub fn run(&self) {
        loop {
            let task = {
                let mut q = self.shared.queue.lock().unwrap();
                loop {
                    if let Some(t) = q.pop_back() {
                        break t;
                    }
                    if self.shared.pending.load(Ordering::SeqCst) == 0 {
                        return;
                    }
                    q = self.shared.cv.wait(q).unwrap();
                }
            };
            let waker = Waker::from(task.clone());
            let mut cx = Context::from_waker(&waker);
            let mut slot = task.future.lock().unwrap();
            if let Some(mut fut) = slot.take() {
                match fut.as_mut().poll(&mut cx) {
                    Poll::Pending => *slot = Some(fut),
                    Poll::Ready(()) => {
                        self.shared.pending.fetch_sub(1, Ordering::SeqCst);
                    }
                }
            }
        }
    }
}
