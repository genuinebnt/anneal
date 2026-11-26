use solution::*;

#[allow(dead_code, unused_imports)]
use std::sync::{Arc, Mutex};

#[allow(dead_code, unused_imports)]
fn call(path: &str) -> Call {
    Call { path: path.into(), notes: Vec::new() }
}

#[allow(dead_code, unused_imports)]
fn answer(status: u16, body: impl Into<String>) -> Answer {
    Answer { status, body: body.into() }
}

/// Echoes the path and notes; `/boom` panics with a formatted message, `/lit` with a literal, `/odd` with a number.
#[allow(dead_code, unused_imports)]
fn handler(c: &Call) -> Answer {
    match c.path.as_str() {
        "/boom" => panic!("secret {}", 4000 + 242),
        "/lit" => panic!("secret literal"),
        "/odd" => std::panic::panic_any(7u64),
        _ => answer(200, format!("{} [{}]", c.path, c.notes.join(","))),
    }
}

#[derive(Clone, Default)]
#[allow(dead_code, unused_imports)]
struct Log(Arc<Mutex<Vec<String>>>);

#[allow(dead_code, unused_imports)]
impl Log {
    fn add(&self, line: impl Into<String>) {
        self.0.lock().unwrap().push(line.into());
    }
    fn lines(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
    fn step(&self, name: &'static str) -> impl Fn(&Call, Next<'_>) -> Answer + Send + Sync + 'static {
        let log = self.clone();
        move |c, next| {
            log.add(format!("{name}>"));
            let mut inner = c.clone();
            inner.notes.push(name.to_string());
            let out = next(&inner);
            log.add(format!("<{name} {}", out.status));
            answer(out.status, format!("{} +{name}", out.body))
        }
    }
    fn shield(&self) -> impl Fn(&Call, Next<'_>) -> Answer + Send + Sync + 'static {
        let log = self.clone();
        shield(move |message| log.add(format!("panic: {message}")))
    }
}

#[test]
fn no_steps() {
    check!("/a", Pipeline::new(handler).run(&call("/a")), answer(200, "/a []"));
}

#[test]
fn the_first_step_is_outermost() {
    let log = Log::default();
    let p = Pipeline::new(handler).wrap(log.step("a")).wrap(log.step("b"));
    let out = p.run(&call("/x"));
    check!("answer", out, answer(200, "/x [a,b] +b +a"));
    check!("order", log.lines(), ["a>", "b>", "<b 200", "<a 200"].map(String::from).to_vec());
}

#[test]
fn a_step_may_answer_itself() {
    let p = Pipeline::new(handler).wrap(|c: &Call, next: Next<'_>| if c.path == "/private" { answer(403, "no") } else { next(c) });
    check!("blocked and not", (p.run(&call("/private")).status, p.run(&call("/open")).status), (403, 200));
}

#[test]
fn the_shield_turns_a_panic_into_500() {
    let log = Log::default();
    let p = Pipeline::new(handler).wrap(log.shield());
    check!("/boom", p.run(&call("/boom")), answer(500, "internal error"));
    check!("reported", log.lines(), vec!["panic: secret 4242".to_string()]);
}

#[test]
fn it_works_after_a_panic() {
    let p = Pipeline::new(handler).wrap(Log::default().shield());
    p.run(&call("/boom"));
    check!("afterwards", p.run(&call("/fine")).status, 200);
}
