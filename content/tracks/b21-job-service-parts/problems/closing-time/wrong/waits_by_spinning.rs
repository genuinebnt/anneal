use std::sync::Arc;
use tokio::sync::watch;

#[derive(Default)]
struct State {
    closed: bool,
    inside: usize,
    admitted: u64,
}

#[derive(Clone)]
pub struct Door {
    state: Arc<watch::Sender<State>>,
}

pub struct Visit {
    door: Door,
}

impl Drop for Visit {
    fn drop(&mut self) {
        self.door.state.send_modify(|s| s.inside -= 1);
    }
}

impl Default for Door {
    fn default() -> Self {
        Self::new()
    }
}

impl Door {
    pub fn new() -> Self {
        Door {
            state: Arc::new(watch::Sender::new(State::default())),
        }
    }

    pub fn enter(&self) -> Option<Visit> {
        // Asking "is it open?" and counting the visit are one change: closing cannot slip between them.
        let admitted = self.state.send_if_modified(|s| {
            if !s.closed {
                s.inside += 1;
                s.admitted += 1;
            }
            !s.closed
        });
        admitted.then(|| Visit { door: self.clone() })
    }

    pub fn inside(&self) -> usize {
        self.state.borrow().inside
    }

    pub fn is_closed(&self) -> bool {
        self.state.borrow().closed
    }

    pub fn close(&self) {
        self.state.send_modify(|s| s.closed = true);
    }

    pub async fn shutdown(&self) -> u64 {
        self.close();
        // Waiting only reads the state, so any number may wait and any of them may give up.
        while self.inside() > 0 {
            tokio::task::yield_now().await;
        }
        self.state.borrow().admitted
    }
}
