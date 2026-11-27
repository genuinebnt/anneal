use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::Router;
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;

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

#[derive(Deserialize)]
struct Filter {
    symbol: Option<String>,
}

pub fn router(prices: Prices) -> Router {
    Router::new()
        .route("/prices", get(stream_prices))
        .with_state(prices)
}

async fn stream_prices(
    State(prices): State<Prices>,
    Query(filter): Query<Filter>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Subscribed now, before the response goes out: nothing published later can be missed.
    let receiver: Option<broadcast::Receiver<Quote>> = None;
    let wanted = filter.symbol;
    let events = futures::stream::unfold((receiver, wanted), move |(receiver, wanted)| {
        let prices = prices.clone();
        async move {
        let mut receiver = match receiver {
            Some(r) => r,
            None => prices.subscribe()?,
        };
        loop {
            let event = match receiver.recv().await {
                Ok(quote) => {
                    if wanted.as_ref().is_some_and(|w| *w != quote.symbol) {
                        continue;
                    }
                    let data = json!({"symbol": quote.symbol, "cents": quote.cents});
                    Event::default()
                        .event("price")
                        .id(quote.seq.to_string())
                        .data(data.to_string())
                }
                // Lagging is not the end: say how much was lost and carry on.
                Err(RecvError::Lagged(n)) => Event::default().event("missed").data(n.to_string()),
                Err(RecvError::Closed) => return None,
            };
            return Some((Ok(event), (Some(receiver), wanted)));
        }
        }
    });
    Sse::new(events).keep_alive(KeepAlive::new())
}
