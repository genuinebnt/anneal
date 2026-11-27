use std::collections::BTreeMap;
use std::io;
use tokio::task::JoinHandle;

/// Where changes are made durable. `commit` may block while it writes.
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
