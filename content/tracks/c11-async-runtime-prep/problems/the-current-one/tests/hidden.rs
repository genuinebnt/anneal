use solution::*;

#[allow(dead_code, unused_imports)]
use std::panic::{catch_unwind, AssertUnwindSafe};
#[allow(dead_code, unused_imports)]
use std::sync::{Arc, Mutex};

#[allow(dead_code)]
struct Named(String);
impl Context for Named {
    fn name(&self) -> String {
        self.0.clone()
    }
}

#[allow(dead_code)]
fn named(n: &str) -> Arc<dyn Context> {
    Arc::new(Named(n.to_string()))
}

#[allow(dead_code)]
fn now() -> Option<String> {
    current().map(|c| c.name())
}

#[test]
fn current_is_the_same_arc() {
    let mine = named("a");
    let _g = enter(mine.clone());
    check!("same allocation", Arc::ptr_eq(&mine, &current().unwrap()), true);
}

#[test]
fn a_panic_restores_the_outer_one() {
    let _a = enter(named("a"));
    let r = catch_unwind(AssertUnwindSafe(|| {
        let _b = enter(named("b"));
        panic!("x");
    }));
    check!("panicked", r.is_err(), true);
    check!("outer is back", now(), Some("a".into()));
}

#[test]
fn the_arc_is_released_with_the_guard() {
    let a = named("a");
    let b = named("b");
    let ga = enter(a.clone());
    let gb = enter(b.clone());
    drop(ga);
    check!("a released although b was entered after it", Arc::strong_count(&a), 1);
    drop(gb);
    check!("b released", Arc::strong_count(&b), 1);
}

#[test]
fn the_same_context_may_be_entered_twice() {
    let a = named("a");
    let b = named("b");
    let first = enter(a.clone());
    let middle = enter(b);
    let again = enter(a.clone());
    drop(first);
    check!("the newest a is current", now(), Some("a".into()));
    drop(again);
    check!("then b", now(), Some("b".into()));
    drop(middle);
    check!("then nothing", now(), None);
}

/// Looks at the current context from its destructor, and enters another.
struct Nosy(Arc<Mutex<Vec<Option<String>>>>);
impl Context for Nosy {
    fn name(&self) -> String {
        "nosy".into()
    }
}
impl Drop for Nosy {
    fn drop(&mut self) {
        let mut log = self.0.lock().unwrap();
        log.push(now());
        let temp = enter(named("temp"));
        log.push(now());
        drop(temp);
        log.push(now());
    }
}

#[test]
fn a_destructor_may_use_the_functions() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let _outer = enter(named("outer"));
    let guard = enter(Arc::new(Nosy(log.clone())));
    drop(guard);
    check!("seen from the destructor", log.lock().unwrap().clone(), vec![Some("outer".into()), Some("temp".into()), Some("outer".into())]);
}

#[test]
fn with_current_allows_reentry() {
    let guard = enter(named("a"));
    let mut slot = Some(guard);
    let seen = with_current(|ctx| {
        let inner = enter(named("b"));
        let during = with_current(|c| c.map(|c| c.name()));
        drop(inner);
        drop(slot.take()); // the guard of the very context we hold
        (ctx.map(|c| c.name()), during, now())
    });
    check!("held context, nested one, after dropping its guard", seen, (Some("a".into()), Some("b".into()), None));
}

#[test]
fn with_current_without_a_context() {
    check!("none", with_current(|c| c.is_none()), true);
}

#[test]
fn many_threads_at_once() {
    let handles: Vec<_> = (0..8)
        .map(|t| {
            std::thread::spawn(move || {
                for i in 0..500 {
                    let name = format!("{t}-{i}");
                    let _g = enter(named(&name));
                    std::thread::yield_now();
                    assert_eq!(now(), Some(name));
                }
                assert_eq!(now(), None);
            })
        })
        .collect();
    handles.into_iter().for_each(|h| h.join().unwrap());
}

#[test]
fn random_enters_and_drops() {
    let mut rng = anneal_prelude::Rng::new(110_601);
    for round in 0..300 {
        let mut live: Vec<(String, Entered)> = Vec::new();
        let mut history = Vec::new();
        for step in 0..40 {
            if live.is_empty() || rng.below(5) < 3 {
                let name = format!("c{step}");
                history.push(format!("enter {name}"));
                live.push((name.clone(), enter(named(&name))));
            } else {
                let i = rng.below(live.len());
                let (name, guard) = live.remove(i);
                history.push(format!("drop {name}"));
                drop(guard);
            }
            check!(format!("round {round} {history:?}"), now(), live.last().map(|(n, _)| n.clone()));
        }
        // leave the thread clean for the next round
        live.clear();
        check!(format!("round {round}: all gone"), now(), None);
    }
}

#[test]
fn works_under_a_deep_stack() {
    let guards: Vec<_> = (0..10_000).map(|i| enter(named(&i.to_string()))).collect();
    check!("the newest", now(), Some("9999".into()));
    drop(guards); // a Vec drops its items front to back: the oldest guard goes first
    check!("empty", now(), None);
}
