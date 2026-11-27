use std::collections::BTreeMap;
use std::io;
use tokio::sync::{mpsc, oneshot};
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
            totals.extend(touched);
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
