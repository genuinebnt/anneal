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
fn quotes_published_before_the_first_read_are_kept() {
    run(async {
        let prices = Prices::new(8);
        prices.publish("OLD", 1);
        let mut client = connect(&router(prices.clone()), "/prices").await;
        prices.publish("A", 2);
        prices.publish("B", 3);
        tokio::time::sleep(Duration::from_secs(1)).await;
        check!("published after the response, before reading", (client.read().await, client.read().await, client.read().await), (price(2, "A", 2), price(3, "B", 3), Read::Quiet));
    });
}

#[test]
fn it_carries_on_after_lagging() {
    run(async {
        let prices = Prices::new(2);
        let mut client = connect(&router(prices.clone()), "/prices").await;
        for cents in 1..=5 {
            prices.publish("A", cents);
        }
        check!("lag", (client.read().await, client.read().await, client.read().await), (Read::Missed(3), price(4, "A", 4), price(5, "A", 5)));
        prices.publish("A", 6);
        check!("still live", client.read().await, price(6, "A", 6));
        for cents in 7..=11 {
            prices.publish("A", cents);
        }
        check!("a second lag", client.read().await, Read::Missed(3));
    });
}

#[test]
fn missed_counts_ignore_the_filter() {
    run(async {
        let prices = Prices::new(2);
        let mut client = connect(&router(prices.clone()), "/prices?symbol=A").await;
        for _ in 0..4 {
            prices.publish("B", 1);
        }
        prices.publish("A", 2);
        check!("dropped quotes of another symbol", (client.read().await, client.read().await), (Read::Missed(3), price(5, "A", 2)));
    });
}

#[test]
fn awkward_symbols() {
    run(async {
        let prices = Prices::new(8);
        let mut client = connect(&router(prices.clone()), "/prices?symbol=a%20%26%22b").await;
        prices.publish("a &\"b", 5);
        check!("a symbol with a space, & and a quote", client.read().await, price(1, "a &\"b", 5));
        let mut all = connect(&router(prices.clone()), "/prices").await;
        prices.publish("two\nlines\rhere", u64::MAX);
        check!("a symbol with line breaks", all.read().await, price(2, "two\nlines\rhere", u64::MAX));
    });
}

#[test]
fn a_quiet_stream_sends_comments() {
    run(async {
        let prices = Prices::new(8);
        let mut client = connect(&router(prices.clone()), "/prices").await;
        for round in 0..3 {
            let got = client.block(Duration::from_millis(15_100)).await;
            check!(format!("quiet period {round}: a comment within 15 s"), got.map(|b| b.map(|b| b.comment)), Ok(Some(true)));
        }
        prices.publish("A", 1);
        check!("and quotes still arrive", client.read().await, price(1, "A", 1));
    });
}

#[test]
fn keep_alive_also_when_the_filter_hides_everything() {
    run(async {
        let prices = Prices::new(8);
        let mut client = connect(&router(prices.clone()), "/prices?symbol=NONE").await;
        let publisher = {
            let prices = prices.clone();
            tokio::spawn(async move {
                for _ in 0..40 {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    prices.publish("A", 1);
                }
            })
        };
        let got = client.block(Duration::from_millis(15_100)).await;
        check!("a comment within 15 s", got.map(|b| b.map(|b| b.comment)), Ok(Some(true)));
        publisher.abort();
    });
}

#[test]
fn a_request_after_the_close() {
    run(async {
        let prices = Prices::new(8);
        prices.publish("A", 1);
        prices.close();
        let mut client = connect(&router(prices), "/prices").await;
        check!("status", client.status, 200);
        check!("content type", client.content_type.starts_with("text/event-stream"), true);
        check!("body", client.block(Duration::from_secs(1)).await, Ok(None));
    });
}

#[test]
fn a_waiting_client_hears_the_close() {
    run(async {
        let prices = Prices::new(8);
        let app = router(prices.clone());
        let mut client = connect(&app, "/prices").await;
        let reader = tokio::spawn(async move { client.read().await });
        tokio::time::sleep(Duration::from_millis(100)).await;
        prices.close();
        check!("the waiting read", reader.await.unwrap(), Read::End);
    });
}

#[test]
fn unknown_routes_stay_unknown() {
    run(async {
        let client = connect(&router(Prices::new(8)), "/price").await;
        check!("GET /price", client.status, 404);
    });
}

/// (client, its filter, the quotes waiting for it, the number dropped)
type Listener = (Client, Option<&'static str>, std::collections::VecDeque<(u64, &'static str)>, u64);

#[test]
fn random_sessions_match_a_model() {
    let mut rng = anneal_prelude::Rng::new(21307);
    for _ in 0..150 {
        run(async {
            let prices = Prices::new(4);
            let app = router(prices.clone());
            let mut clients: Vec<Listener> = Vec::new();
            let mut seq = 0u64;
            let mut log = Vec::new();
            for _ in 0..30 {
                match rng.below(5) {
                    0 if clients.len() < 3 => {
                        let filter = *rng.pick(&[None, Some("A"), Some("B")]);
                        let uri = match filter {
                            Some(s) => format!("/prices?symbol={s}"),
                            None => "/prices".to_string(),
                        };
                        log.push(format!("client {} connects to {uri}", clients.len()));
                        clients.push((connect(&app, &uri).await, filter, Default::default(), 0));
                    }
                    1 | 2 => {
                        let symbol = *rng.pick(&["A", "B"]);
                        seq += 1;
                        log.push(format!("publish {symbol} #{seq}"));
                        prices.publish(symbol, seq * 10);
                        for (_, _, waiting, dropped) in &mut clients {
                            waiting.push_back((seq, symbol));
                            if waiting.len() > 4 {
                                waiting.pop_front();
                                *dropped += 1;
                            }
                        }
                    }
                    _ if !clients.is_empty() => {
                        let i = rng.below(clients.len());
                        let (client, filter, waiting, dropped) = &mut clients[i];
                        log.push(format!("client {i} reads"));
                        let want = if *dropped > 0 {
                            Read::Missed(std::mem::take(dropped))
                        } else {
                            loop {
                                match waiting.pop_front() {
                                    None => break Read::Quiet,
                                    Some((n, s)) if filter.is_none_or(|f| f == s) => break price(n, s, n * 10),
                                    Some(_) => {}
                                }
                            }
                        };
                        check!(format!("{log:?}"), client.read().await, want);
                    }
                    _ => {}
                }
            }
        });
    }
}
