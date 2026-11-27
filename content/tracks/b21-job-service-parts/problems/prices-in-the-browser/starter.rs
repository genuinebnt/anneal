use axum::Router;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Quote {
    pub seq: u64,
    pub symbol: String,
    pub cents: u64,
}

struct Source {
    /// `None` once the feed is closed.
    sender: Option<broadcast::Sender<Quote>>,
    last_seq: u64,
}

/// Given: the live price feed. Each subscriber has a limited buffer; one that falls behind loses the oldest quotes.
#[derive(Clone)]
pub struct Prices(Arc<Mutex<Source>>);

impl Prices {
    /// `buffer` should be a power of two: Tokio rounds other sizes up.
    pub fn new(buffer: usize) -> Self {
        let source = Source {
            sender: Some(broadcast::Sender::new(buffer)),
            last_seq: 0,
        };
        Prices(Arc::new(Mutex::new(source)))
    }

    /// Sends a quote to everyone subscribed now. Quotes are numbered 1, 2, 3... Does nothing once closed.
    pub fn publish(&self, symbol: &str, cents: u64) {
        let mut source = self.0.lock().unwrap();
        let Some(sender) = source.sender.clone() else {
            return;
        };
        source.last_seq += 1;
        let quote = Quote {
            seq: source.last_seq,
            symbol: symbol.to_string(),
            cents,
        };
        let _ = sender.send(quote);
    }

    /// No more quotes. Receivers get what is still buffered, then `Closed`.
    pub fn close(&self) {
        self.0.lock().unwrap().sender = None;
    }

    /// A receiver for every quote published from now on, or `None` once closed.
    pub fn subscribe(&self) -> Option<broadcast::Receiver<Quote>> {
        let source = self.0.lock().unwrap();
        source.sender.as_ref().map(|s| s.subscribe())
    }
}

pub fn router(prices: Prices) -> Router {
    todo!()
}
