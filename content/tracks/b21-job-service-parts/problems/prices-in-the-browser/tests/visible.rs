use solution::*;

#[allow(unused_imports)]
use axum::Router;
#[allow(unused_imports)]
use axum::body::{Body, BodyDataStream};
#[allow(unused_imports)]
use axum::http::Request;
#[allow(unused_imports)]
use futures::StreamExt;
#[allow(unused_imports)]
use serde_json::{Value, json};
#[allow(unused_imports)]
use std::time::Duration;
#[allow(unused_imports)]
use tokio::time::timeout;
#[allow(unused_imports)]
use tower::ServiceExt;

#[allow(dead_code)]
pub fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap().block_on(f)
}

#[allow(dead_code)]
/// One block of the stream: `comment` is true when it holds only comment lines.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Block {
    pub event: Option<String>,
    pub id: Option<String>,
    pub data: Option<String>,
    pub comment: bool,
}

#[allow(dead_code)]
pub fn parse(text: &str) -> Block {
    let mut block = Block { comment: true, ..Block::default() };
    for line in text.lines() {
        if line.starts_with(':') || line.is_empty() {
            continue;
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value).to_string();
        block.comment = false;
        match field {
            "event" => block.event = Some(value),
            "id" => block.id = Some(value),
            "data" => block.data = Some(match block.data.take() {
                Some(old) => format!("{old}\n{value}"),
                None => value,
            }),
            _ => {}
        }
    }
    block
}

#[allow(dead_code)]
/// Nothing arrived in time.
#[derive(Debug, PartialEq)]
pub struct Silent;

#[allow(dead_code)]
pub struct Client {
    pub status: u16,
    pub content_type: String,
    body: BodyDataStream,
    buffer: String,
}

#[allow(dead_code)]
#[derive(Debug, PartialEq)]
pub enum Read {
    Price { id: u64, symbol: String, cents: u64 },
    Missed(u64),
    /// Nothing but comments within a second.
    Quiet,
    End,
    Other(String),
}

#[allow(dead_code)]
pub fn price(id: u64, symbol: &str, cents: u64) -> Read {
    Read::Price { id, symbol: symbol.to_string(), cents }
}

#[allow(dead_code)]
pub async fn connect(app: &Router, uri: &str) -> Client {
    let request = Request::builder().uri(uri).body(Body::empty()).unwrap();
    let response = timeout(Duration::from_secs(1), app.clone().oneshot(request)).await.expect("no response: it must not wait for quotes").unwrap();
    let content_type = response.headers().get("content-type").map(|v| v.to_str().unwrap().to_string()).unwrap_or_default();
    Client { status: response.status().as_u16(), content_type, body: response.into_body().into_data_stream(), buffer: String::new() }
}

#[allow(dead_code)]
impl Client {
    /// The next block, or `Ok(None)` at the end of the body. `Err` if nothing arrived within `wait`.
    pub async fn block(&mut self, wait: Duration) -> Result<Option<Block>, Silent> {
        loop {
            if let Some(end) = self.buffer.find("\n\n") {
                let text: String = self.buffer.drain(..end + 2).collect();
                return Ok(Some(parse(&text)));
            }
            match timeout(wait, self.body.next()).await {
                Err(_) => return Err(Silent),
                Ok(None) => return Ok(None),
                Ok(Some(bytes)) => self.buffer.push_str(std::str::from_utf8(&bytes.expect("body error")).expect("utf-8")),
            }
        }
    }

    /// The next event, skipping comments.
    pub async fn read(&mut self) -> Read {
        for _ in 0..1000 {
            let block = match self.block(Duration::from_secs(1)).await {
                Err(Silent) => return Read::Quiet,
                Ok(None) => return Read::End,
                Ok(Some(b)) => b,
            };
            if block.comment {
                continue;
            }
            let data = block.data.clone().unwrap_or_default();
            return match block.event.as_deref() {
                Some("price") => {
                    let value: Value = serde_json::from_str(&data).unwrap_or(Value::Null);
                    let (symbol, cents) = (value["symbol"].as_str().unwrap_or("?").to_string(), value["cents"].as_u64().unwrap_or(0));
                    if value != json!({"symbol": symbol, "cents": cents}) {
                        return Read::Other(data);
                    }
                    match block.id.and_then(|i| i.parse().ok()) {
                        Some(id) => Read::Price { id, symbol, cents },
                        None => Read::Other("a price without a numeric id".to_string()),
                    }
                }
                Some("missed") if block.id.is_none() => data.parse().map(Read::Missed).unwrap_or(Read::Other(data)),
                _ => Read::Other(format!("{block:?}")),
            };
        }
        Read::Other("1000 comments in a row".to_string())
    }
}

#[test]
fn quotes_arrive_as_events() {
    run(async {
        let prices = Prices::new(16);
        let mut client = connect(&router(prices.clone()), "/prices").await;
        check!("status", client.status, 200);
        check!("content type", client.content_type.starts_with("text/event-stream"), true);
        check!("before any quote", client.read().await, Read::Quiet);
        prices.publish("ABC", 1250);
        check!("first", client.read().await, price(1, "ABC", 1250));
        prices.publish("XY", 7);
        check!("second", client.read().await, price(2, "XY", 7));
    });
}

#[test]
fn filtering_by_symbol() {
    run(async {
        let prices = Prices::new(16);
        let mut client = connect(&router(prices.clone()), "/prices?symbol=XY").await;
        prices.publish("ABC", 1);
        prices.publish("XY", 2);
        prices.publish("xy", 3);
        check!("only XY", client.read().await, price(2, "XY", 2));
        check!("nothing else", client.read().await, Read::Quiet);
    });
}

#[test]
fn closing_ends_the_body() {
    run(async {
        let prices = Prices::new(16);
        let mut client = connect(&router(prices.clone()), "/prices").await;
        prices.publish("A", 1);
        prices.close();
        check!("buffered quote", client.read().await, price(1, "A", 1));
        check!("then", client.read().await, Read::End);
    });
}

#[test]
fn a_slow_client_is_told_what_it_missed() {
    run(async {
        let prices = Prices::new(4);
        let mut client = connect(&router(prices.clone()), "/prices").await;
        for cents in 1..=10 {
            prices.publish("A", cents);
        }
        check!("first read", client.read().await, Read::Missed(6));
        check!("then the oldest kept", client.read().await, price(7, "A", 7));
    });
}

#[test]
fn every_client_gets_its_own_copy() {
    run(async {
        let prices = Prices::new(16);
        let app = router(prices.clone());
        let mut a = connect(&app, "/prices").await;
        let mut b = connect(&app, "/prices").await;
        prices.publish("A", 1);
        check!("both clients", (a.read().await, b.read().await), (price(1, "A", 1), price(1, "A", 1)));
    });
}
