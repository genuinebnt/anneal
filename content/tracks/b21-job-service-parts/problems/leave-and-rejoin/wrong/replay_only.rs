use futures::Stream;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: u64,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Closed;

#[derive(Default)]
struct Log {
    lines: Vec<String>,
    closed: bool,
}

impl Log {
    /// The first entry with an id above `after`. Ids are positions plus one.
    fn next(&self, after: u64) -> Option<Entry> {
        let index = usize::try_from(after).ok()?;
        let text = self.lines.get(index)?.clone();
        Some(Entry {
            id: after + 1,
            text,
        })
    }
}

/// Only feed handles hold the sender, so followers can tell when all of them are gone.
#[derive(Clone)]
pub struct Feed {
    log: Arc<Mutex<Log>>,
    changed: Arc<watch::Sender<()>>,
}

impl Default for Feed {
    fn default() -> Self {
        Self::new()
    }
}

impl Feed {
    pub fn new() -> Self {
        Feed {
            log: Arc::default(),
            changed: Arc::new(watch::Sender::new(())),
        }
    }

    pub fn publish(&self, text: &str) -> Result<u64, Closed> {
        let id = {
            let mut log = self.log.lock().unwrap();
            if log.closed {
                return Err(Closed);
            }
            log.lines.push(text.to_string());
            log.lines.len() as u64
        };
        self.changed.send_replace(());
        Ok(id)
    }

    pub fn close(&self) {
        self.log.lock().unwrap().closed = true;
        self.changed.send_replace(());
    }

    pub fn since(&self, after: u64) -> Vec<Entry> {
        let log = self.log.lock().unwrap();
        let mut found = Vec::new();
        let mut last = after;
        while let Some(entry) = log.next(last) {
            last = entry.id;
            found.push(entry);
        }
        found
    }

    pub fn follow(&self, after: u64) -> impl Stream<Item = Entry> + Send + 'static {
        let state = (self.log.clone(), self.changed.subscribe(), after, false);
        futures::stream::unfold(
            state,
            |(log, mut changed, after, mut orphaned)| async move {
                loop {
                    // Mark the signal as seen BEFORE reading, so a publish in between still wakes us.
                    changed.borrow_and_update();
                    let (entry, closed) = {
                        let log = log.lock().unwrap();
                        (log.next(after), log.closed)
                    };
                    if let Some(entry) = entry {
                        let after = entry.id;
                        return Some((entry, (log, changed, after, orphaned)));
                    }
                    if true || closed || orphaned {
                        return None;
                    }
                    // An error means every handle is gone: read once more, then end.
                    orphaned = changed.changed().await.is_err();
                }
            },
        )
    }
}
