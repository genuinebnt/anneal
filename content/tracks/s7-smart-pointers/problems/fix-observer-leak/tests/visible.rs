use solution::*;

use std::rc::Rc;

#[allow(dead_code)]
fn texts(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn delivers_to_everyone() {
    let hub = Hub::new();
    let a = Hub::subscribe(&hub, "a");
    let b = Hub::subscribe(&hub, "b");
    check!("publish to two", (hub.publish("hi"), a.inbox(), b.inbox()), (2, texts(&["hi"]), texts(&["hi"])));
}

#[test]
fn post_skips_the_sender() {
    let hub = Hub::new();
    let a = Hub::subscribe(&hub, "a");
    let b = Hub::subscribe(&hub, "b");
    check!("a posts", (a.post("yo"), a.inbox(), b.inbox()), (Some(1), texts(&[]), texts(&["a: yo"])));
}

#[test]
fn dropped_subscriber_is_released() {
    let hub = Hub::new();
    let a = Hub::subscribe(&hub, "a");
    let watch = Rc::downgrade(&a);
    drop(a);
    check!("still alive after dropping the only handle", watch.upgrade().is_some(), false);
    check!("publish reaches nobody", hub.publish("x"), 0);
}

#[test]
fn dropped_hub_is_released() {
    let hub = Hub::new();
    let a = Hub::subscribe(&hub, "a");
    let watch = Rc::downgrade(&hub);
    drop(hub);
    check!("hub still alive while a subscriber exists", watch.upgrade().is_some(), false);
    check!("post without a hub", a.post("anyone?"), None);
}

#[test]
fn dead_entries_are_removed() {
    let hub = Hub::new();
    let keep = Hub::subscribe(&hub, "keep");
    for i in 0..100 {
        let _gone = Hub::subscribe(&hub, &format!("t{i}"));
    }
    hub.publish("tidy");
    check!("100 short-lived subscribers: entries after a publish", (hub.entries(), keep.inbox().len()), (1, 1));
}
