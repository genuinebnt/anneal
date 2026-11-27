use solution::*;

#[allow(dead_code, unused_imports)]
use std::sync::Arc;

#[allow(dead_code)]
struct Named(&'static str);
impl Context for Named {
    fn name(&self) -> String {
        self.0.to_string()
    }
}

#[allow(dead_code)]
fn named(n: &'static str) -> Arc<dyn Context> {
    Arc::new(Named(n))
}

#[allow(dead_code)]
fn now() -> Option<String> {
    current().map(|c| c.name())
}

#[test]
fn nothing_is_current_at_first() {
    check!("no guard", now(), None);
}

#[test]
fn enter_and_leave() {
    let guard = enter(named("a"));
    check!("inside", now(), Some("a".into()));
    drop(guard);
    check!("after", now(), None);
}

#[test]
fn nesting_restores_the_outer_one() {
    let _a = enter(named("a"));
    {
        let _b = enter(named("b"));
        check!("inner", now(), Some("b".into()));
    }
    check!("outer again", now(), Some("a".into()));
}

#[test]
fn guards_may_be_dropped_out_of_order() {
    let a = enter(named("a"));
    let b = enter(named("b"));
    drop(a);
    check!("b is still the innermost", now(), Some("b".into()));
    drop(b);
    check!("a does not come back", now(), None);
}

#[test]
fn threads_do_not_share() {
    let _a = enter(named("main"));
    let other = std::thread::spawn(now).join().unwrap();
    check!("a new thread has none", other, None);
}

#[test]
fn with_current_passes_it_in() {
    let _a = enter(named("a"));
    check!("the name", with_current(|c| c.map(|c| c.name())), Some("a".to_string()));
}
