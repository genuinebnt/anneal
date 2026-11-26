use solution::*;

use std::rc::Rc;

#[allow(dead_code)]
fn texts(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn empty_hub() {
    check!("publish on a new hub", (Hub::new().publish("x"), Hub::new().entries()), (0, 0));
}

#[test]
fn a_clone_of_the_handle_keeps_the_subscription() {
    let hub = Hub::new();
    let a = Hub::subscribe(&hub, "a");
    let second = a.clone();
    drop(a);
    check!("one of two handles dropped", (hub.publish("x"), second.inbox()), (1, texts(&["x"])));
}

#[test]
fn the_hub_adds_no_strong_reference() {
    let hub = Hub::new();
    let a = Hub::subscribe(&hub, "a");
    hub.publish("x");
    a.post("y");
    check!("strong counts of (subscriber, hub) after use", (Rc::strong_count(&a), Rc::strong_count(&hub)), (1, 1));
}

#[test]
fn post_also_tidies() {
    let hub = Hub::new();
    let a = Hub::subscribe(&hub, "a");
    let b = Hub::subscribe(&hub, "b");
    drop(b);
    check!("a posts after b left", (a.post("hi"), hub.entries()), (Some(0), 1));
}

#[test]
fn same_names_are_different_subscribers() {
    let hub = Hub::new();
    let a = Hub::subscribe(&hub, "twin");
    let b = Hub::subscribe(&hub, "twin");
    check!("one twin posts", (a.post("hi"), a.inbox(), b.inbox()), (Some(1), texts(&[]), texts(&["twin: hi"])));
}

#[test]
fn alone_in_the_hub() {
    let hub = Hub::new();
    let a = Hub::subscribe(&hub, "a");
    check!("post with no one else", a.post("echo"), Some(0));
}

#[test]
fn order_survives_departures() {
    let hub = Hub::new();
    let a = Hub::subscribe(&hub, "a");
    let b = Hub::subscribe(&hub, "b");
    let c = Hub::subscribe(&hub, "c");
    drop(b);
    c.post("1");
    hub.publish("2");
    a.post("3");
    check!("a's inbox", a.inbox(), texts(&["c: 1", "2"]));
    check!("c's inbox", c.inbox(), texts(&["2", "a: 3"]));
}

#[test]
fn everyone_leaves_then_the_hub() {
    let hub = Hub::new();
    let subs: Vec<_> = (0..10).map(|i| Hub::subscribe(&hub, &i.to_string())).collect();
    let watches: Vec<_> = subs.iter().map(Rc::downgrade).collect();
    let hub_watch = Rc::downgrade(&hub);
    drop(subs);
    drop(hub);
    check!("anything still alive", (watches.iter().filter(|w| w.upgrade().is_some()).count(), hub_watch.upgrade().is_some()), (0, false));
}

#[test]
fn random_sessions_match_a_model() {
    let mut rng = anneal_prelude::Rng::new(8113);
    for _ in 0..300 {
        let hub = Hub::new();
        let mut live: Vec<(Rc<Subscriber>, String, Vec<String>)> = Vec::new();
        let mut trace = Vec::new();
        for step in 0..25 {
            match rng.below(4) {
                0 => {
                    let name = format!("s{step}");
                    trace.push(format!("subscribe {name}"));
                    live.push((Hub::subscribe(&hub, &name), name, Vec::new()));
                }
                1 if !live.is_empty() => {
                    let i = rng.below(live.len());
                    let (handle, name, _) = live.remove(i);
                    trace.push(format!("drop {name}"));
                    let watch = Rc::downgrade(&handle);
                    drop(handle);
                    check!(format!("{trace:?}: dropped subscriber still alive"), watch.upgrade().is_some(), false);
                }
                2 if !live.is_empty() => {
                    let i = rng.below(live.len());
                    trace.push(format!("{} posts m{step}", live[i].1));
                    let message = format!("{}: m{step}", live[i].1);
                    let got = live[i].0.post(&format!("m{step}"));
                    for (j, entry) in live.iter_mut().enumerate() {
                        if j != i {
                            entry.2.push(message.clone());
                        }
                    }
                    check!(format!("{trace:?}: (post result, entries)"), (got, hub.entries()), (Some(live.len() - 1), live.len()));
                }
                _ => {
                    trace.push(format!("publish p{step}"));
                    let got = hub.publish(&format!("p{step}"));
                    for entry in &mut live {
                        entry.2.push(format!("p{step}"));
                    }
                    check!(format!("{trace:?}: (publish result, entries)"), (got, hub.entries()), (live.len(), live.len()));
                }
            }
        }
        for (handle, name, want) in &live {
            check!(format!("{trace:?}: inbox of {name}"), handle.inbox(), want.clone());
        }
    }
}
