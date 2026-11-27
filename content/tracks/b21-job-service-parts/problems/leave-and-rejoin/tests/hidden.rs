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
fn a_slow_follower_loses_nothing() {
    run(async {
        let feed = Feed::new();
        let mut slow = follow(&feed, 0);
        check!("empty feed", peek(&mut slow), Got::Waiting);
        for i in 0..20_000 {
            let _ = feed.publish(&i.to_string());
        }
        feed.close();
        let ids = rest(slow).await;
        check!("20,000 entries published before the follower looked again", (ids.len(), ids == (1..=20_000).collect::<Vec<u64>>()), (20_000, true));
    });
}

#[test]
fn beyond_the_end() {
    run(async {
        let feed = Feed::new();
        let mut s = follow(&feed, 3);
        for text in ["a", "b", "c"] {
            let _ = feed.publish(text);
            check!(format!("after publishing {text}, follow(3)"), peek(&mut s), Got::Waiting);
        }
        let _ = feed.publish("d");
        check!("the fourth", peek(&mut s), entry(4, "d"));
        let mut far = follow(&feed, u64::MAX);
        check!("follow(u64::MAX)", peek(&mut far), Got::Waiting);
        check!("since(u64::MAX)", feed.since(u64::MAX), vec![]);
        feed.close();
        check!("closed", peek(&mut far), Got::End);
    });
}

#[test]
fn a_closed_feed() {
    run(async {
        let feed = Feed::new();
        let _ = feed.publish("a");
        feed.close();
        feed.close();
        check!("follow a closed feed", rest(follow(&feed, 0)).await, vec![1]);
        check!("from its end", rest(follow(&feed, 1)).await, vec![]);
        check!("since still works", feed.since(0).len(), 1);
    });
}

#[test]
fn clones_are_one_feed() {
    run(async {
        let feed = Feed::new();
        let other = feed.clone();
        let _ = feed.publish("a");
        check!("id from the clone", other.publish("b"), Ok(2));
        other.close();
        check!("closed through the clone", feed.publish("c"), Err(Closed));
    });
}

#[test]
fn dropping_every_handle_ends_the_stream() {
    run(async {
        let feed = Feed::new();
        let other = feed.clone();
        let mut s = follow(&feed, 0);
        drop(feed);
        check!("one handle is left", peek(&mut s), Got::Waiting);
        let _ = other.publish("last words");
        drop(other);
        check!("the entry", next(&mut s).await.map(|e| e.id), Some(1));
        check!("then the end", next(&mut s).await.map(|e| e.id), None);
    });
}

#[test]
fn a_sleeping_follower_is_woken_by_the_drop() {
    run(async {
        let feed = Feed::new();
        let waiting = tokio::spawn(rest(follow(&feed, 0)));
        tokio::time::sleep(Duration::from_millis(5)).await;
        drop(feed);
        check!("ids", waiting.await.unwrap(), vec![]);
    });
}

#[test]
fn waiting_does_not_spin() {
    run(async {
        let feed = Feed::new();
        let mut s = follow(&feed, 0);
        let wakes = Arc::new(Wakes(AtomicUsize::new(0)));
        let waker = futures::task::waker(wakes.clone());
        let mut cx = Context::from_waker(&waker);
        let first = matches!(s.as_mut().poll_next(&mut cx), Poll::Pending);
        for _ in 0..50 {
            tokio::task::yield_now().await;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
        check!("quiet feed: (pending, wake-ups in 5 s)", (first, wakes.0.load(Ordering::SeqCst)), (true, 0));
        let _ = feed.publish("x");
        check!("woken by the publish", wakes.0.load(Ordering::SeqCst) > 0, true);
        check!("and the entry is there", matches!(s.as_mut().poll_next(&mut cx), Poll::Ready(Some(_))), true);
        let before = wakes.0.load(Ordering::SeqCst);
        let again = matches!(s.as_mut().poll_next(&mut cx), Poll::Pending);
        tokio::time::sleep(Duration::from_secs(5)).await;
        check!("quiet again: (pending, new wake-ups)", (again, wakes.0.load(Ordering::SeqCst) - before), (true, 0));
        feed.close();
        check!("woken by the close", wakes.0.load(Ordering::SeqCst) > before, true);
    });
}

#[test]
fn the_stream_can_move_to_another_thread() {
    let feed = Feed::new();
    let stream = feed.follow(0);
    let reader = std::thread::spawn(move || futures::executor::block_on(async move { Box::pin(stream).map(|e| e.id).collect::<Vec<_>>().await }));
    let _ = feed.publish("a");
    let _ = feed.publish("b");
    feed.close();
    check!("read on another thread, without tokio", reader.join().unwrap(), vec![1, 2]);
}

#[test]
fn nothing_is_lost_while_threads_publish() {
    for round in 0..60 {
        let feed = Feed::new();
        let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(3).enable_time().build().unwrap();
        runtime.block_on(async {
            let writer = {
                let feed = feed.clone();
                tokio::spawn(async move {
                    for i in 0..300u64 {
                        let _ = feed.publish("x");
                        if i % 7 == 0 {
                            tokio::task::yield_now().await;
                        }
                    }
                    feed.close();
                })
            };
            let mut readers = Vec::new();
            for _ in 0..6 {
                let stream = follow(&feed, 0);
                readers.push(tokio::spawn(async move {
                    let mut stream = stream;
                    let mut ids = Vec::new();
                    while let Some(e) = tokio::time::timeout(Duration::from_secs(5), stream.next()).await.expect("a follower missed its wake-up") {
                        ids.push(e.id);
                    }
                    ids
                }));
                tokio::task::yield_now().await;
            }
            for r in readers {
                let ids = r.await.unwrap();
                let whole = ids == (1..=300).collect::<Vec<u64>>();
                check!(format!("round {round}: a follower got {} entries, in order and complete", ids.len()), whole, true);
            }
            writer.await.unwrap();
        });
    }
}

#[test]
fn random_sessions_match_a_model() {
    let mut rng = anneal_prelude::Rng::new(21306);
    for _ in 0..300 {
        run(async {
            let feed = Feed::new();
            let mut published = 0u64;
            let mut closed = false;
            // (stream, the last id it has had)
            let mut followers: Vec<(Follow, u64)> = Vec::new();
            let mut log = Vec::new();
            for _ in 0..30 {
                match rng.below(6) {
                    0 | 1 => {
                        let want = if closed {
                            Err(Closed)
                        } else {
                            published += 1;
                            Ok(published)
                        };
                        log.push("publish".to_string());
                        check!(format!("{log:?}"), feed.publish(&format!("t{published}")), want);
                    }
                    2 => {
                        let after = rng.below(6) as u64;
                        log.push(format!("follower {} = follow({after})", followers.len()));
                        followers.push((follow(&feed, after), after));
                    }
                    3 if rng.below(4) == 0 => {
                        log.push("close".to_string());
                        feed.close();
                        closed = true;
                    }
                    _ if !followers.is_empty() => {
                        let i = rng.below(followers.len());
                        let (stream, had) = &mut followers[i];
                        log.push(format!("read follower {i}"));
                        let want = if *had < published {
                            *had += 1;
                            entry(*had, &format!("t{}", *had))
                        } else if closed {
                            Got::End
                        } else {
                            Got::Waiting
                        };
                        let ended = want == Got::End;
                        check!(format!("{log:?}"), peek(stream), want);
                        if ended {
                            drop(followers.remove(i));
                        }
                    }
                    _ => {}
                }
            }
        });
    }
}
