use solution::*;

#[allow(unused_imports)]
use futures::{FutureExt, Stream, StreamExt};
#[allow(unused_imports)]
use std::pin::Pin;
#[allow(unused_imports)]
use std::sync::Arc;
#[allow(unused_imports)]
use std::sync::atomic::{AtomicUsize, Ordering};
#[allow(unused_imports)]
use std::task::{Context, Poll};
#[allow(unused_imports)]
use std::time::Duration;

#[allow(dead_code)]
pub fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap().block_on(f)
}

#[allow(dead_code)]
pub type Follow = Pin<Box<dyn Stream<Item = Entry> + Send>>;

#[allow(dead_code)]
pub fn follow(feed: &Feed, after: u64) -> Follow {
    Box::pin(feed.follow(after))
}

#[allow(dead_code)]
#[derive(Debug, PartialEq)]
pub enum Got {
    Entry(u64, String),
    /// Nothing yet: the stream is waiting.
    Waiting,
    End,
}

#[allow(dead_code)]
/// One look at the stream, without waiting.
pub fn peek(stream: &mut Follow) -> Got {
    match stream.next().now_or_never() {
        None => Got::Waiting,
        Some(None) => Got::End,
        Some(Some(e)) => Got::Entry(e.id, e.text),
    }
}

#[allow(dead_code)]
pub fn entry(id: u64, text: &str) -> Got {
    Got::Entry(id, text.to_string())
}

#[allow(dead_code)]
/// The next entry, waiting for it in virtual time.
pub async fn next(stream: &mut Follow) -> Option<Entry> {
    tokio::time::timeout(Duration::from_secs(3600), stream.next()).await.expect("the follower was never woken")
}

#[allow(dead_code)]
pub async fn rest(mut stream: Follow) -> Vec<u64> {
    let mut ids = Vec::new();
    while let Some(e) = next(&mut stream).await {
        ids.push(e.id);
    }
    ids
}

#[allow(dead_code)]
pub struct Wakes(pub AtomicUsize);

#[allow(dead_code)]
impl futures::task::ArcWake for Wakes {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn history_then_new_entries() {
    run(async {
        let feed = Feed::new();
        check!("first id", feed.publish("build"), Ok(1));
        check!("second id", feed.publish("test"), Ok(2));
        let mut s = follow(&feed, 0);
        check!("first", peek(&mut s), entry(1, "build"));
        check!("second", peek(&mut s), entry(2, "test"));
        check!("caught up", peek(&mut s), Got::Waiting);
        let _ = feed.publish("deploy");
        check!("a new entry", peek(&mut s), entry(3, "deploy"));
    });
}

#[test]
fn rejoin_from_where_you_left() {
    run(async {
        let feed = Feed::new();
        for text in ["a", "b", "c"] {
            let _ = feed.publish(text);
        }
        let mut s = follow(&feed, 2);
        check!("follow(2)", peek(&mut s), entry(3, "c"));
        check!("then", peek(&mut s), Got::Waiting);
    });
}

#[test]
fn closing_ends_the_stream() {
    run(async {
        let feed = Feed::new();
        let _ = feed.publish("only");
        let waiting = tokio::spawn(rest(follow(&feed, 0)));
        tokio::task::yield_now().await;
        feed.close();
        check!("ids until the end", waiting.await.unwrap(), vec![1]);
        check!("publish after close", feed.publish("late"), Err(Closed));
    });
}

#[test]
fn since_does_not_wait() {
    let feed = Feed::new();
    let _ = feed.publish("a");
    let _ = feed.publish("b");
    check!("since(1)", feed.since(1), vec![Entry { id: 2, text: "b".to_string() }]);
    check!("since(2)", feed.since(2), vec![]);
}

#[test]
fn an_empty_feed_is_waiting() {
    run(async {
        let feed = Feed::new();
        let mut s = follow(&feed, 0);
        check!("nothing published yet", peek(&mut s), Got::Waiting);
        check!("since(0)", feed.since(0), vec![]);
    });
}
