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
fn literal_and_unusual_payloads() {
    let log = Log::default();
    let p = Pipeline::new(handler).wrap(log.shield());
    check!("literal", p.run(&call("/lit")), answer(500, "internal error"));
    check!("a number", p.run(&call("/odd")), answer(500, "internal error"));
    check!("reports", log.lines(), vec!["panic: secret literal".to_string(), "panic: unknown panic".to_string()]);
}

#[test]
fn steps_outside_see_an_ordinary_500() {
    let log = Log::default();
    let p = Pipeline::new(handler).wrap(log.step("out")).wrap(log.shield()).wrap(log.step("in"));
    let out = p.run(&call("/lit"));
    check!("answer", out, answer(500, "internal error +out"));
    check!("log", log.lines(), ["out>", "in>", "panic: secret literal", "<out 500"].map(String::from).to_vec());
}

#[test]
fn a_panic_outside_the_shield_goes_through() {
    let boom = |c: &Call, next: Next<'_>| {
        assert!(c.path != "/trip", "outer step");
        next(c)
    };
    let log = Log::default();
    let p = Pipeline::new(handler).wrap(boom).wrap(log.shield());
    let got = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| p.run(&call("/trip"))));
    check!("not caught", got.is_err(), true);
    check!("not reported", log.lines(), Vec::<String>::new());
    check!("pipeline still usable", p.run(&call("/ok")).status, 200);
}

#[test]
fn a_panic_in_a_step_inside_is_caught() {
    let p = Pipeline::new(handler).wrap(Log::default().shield()).wrap(|_: &Call, _: Next<'_>| panic!("inner step"));
    check!("inner step", p.run(&call("/a")), answer(500, "internal error"));
}

#[test]
fn without_a_shield_panics_go_through() {
    let p = Pipeline::new(handler);
    let got = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| p.run(&call("/boom"))));
    check!("no shield", got.is_err(), true);
}

#[test]
fn next_can_be_called_again() {
    let tries = Arc::new(Mutex::new(0));
    let t = tries.clone();
    let p = Pipeline::new(move |_| {
        let mut n = t.lock().unwrap();
        *n += 1;
        if *n < 3 { answer(503, "busy") } else { answer(200, "done") }
    })
    .wrap(|c: &Call, next: Next<'_>| {
        let mut out = next(c);
        for _ in 0..4 {
            if out.status != 503 {
                break;
            }
            out = next(c);
        }
        out
    });
    check!("retried until it worked", (p.run(&call("/")), *tries.lock().unwrap()), (answer(200, "done"), 3));
}

#[test]
fn a_retry_goes_through_the_inner_steps_each_time() {
    let log = Log::default();
    let p = Pipeline::new(handler)
        .wrap(|c: &Call, next: Next<'_>| {
            next(c);
            next(c)
        })
        .wrap(log.step("in"));
    p.run(&call("/r"));
    check!("inner ran twice", log.lines().len(), 4);
}

#[test]
fn the_callers_call_is_left_alone() {
    let log = Log::default();
    let p = Pipeline::new(handler).wrap(log.step("a"));
    let c = call("/c");
    p.run(&c);
    check!("unchanged", c, call("/c"));
}

#[test]
fn calls_run_at_the_same_time() {
    let barrier = Arc::new((Mutex::new(0), std::sync::Condvar::new()));
    let b = barrier.clone();
    // each call waits until all three are inside the handler
    let p = Arc::new(
        Pipeline::new(move |_| {
            let (lock, cv) = &*b;
            let mut n = lock.lock().unwrap();
            *n += 1;
            cv.notify_all();
            let (n, _) = cv.wait_timeout_while(n, std::time::Duration::from_secs(3), |n| *n < 3).unwrap();
            answer(200, n.to_string())
        })
        .wrap(Log::default().shield()),
    );
    let hs: Vec<_> = (0..3).map(|_| { let p = p.clone(); std::thread::spawn(move || p.run(&call("/")).body) }).collect();
    let got: Vec<String> = hs.into_iter().map(|h| h.join().unwrap()).collect();
    check!("all three met inside", got, vec!["3".to_string(); 3]);
}

#[test]
fn random_stacks_against_a_model() {
    let mut rng = anneal_prelude::Rng::new(71101);
    for _ in 0..200 {
        // 0: tag, 1: block "/x", 2: shield, 3: panic on "/y"
        let kinds: Vec<usize> = (0..rng.below(6)).map(|_| rng.below(4)).collect();
        let path = *rng.pick(&["/a", "/x", "/y", "/lit"]);
        let mut p = Pipeline::new(handler);
        for &k in &kinds {
            p = match k {
                0 => p.wrap(|c: &Call, next: Next<'_>| {
                    let out = next(c);
                    answer(out.status, format!("{}.", out.body))
                }),
                1 => p.wrap(|c: &Call, next: Next<'_>| if c.path == "/x" { answer(403, "no") } else { next(c) }),
                2 => p.wrap(shield(|_| {})),
                _ => p.wrap(|c: &Call, next: Next<'_>| {
                    assert!(c.path != "/y", "step");
                    next(c)
                }),
            };
        }
        fn model(kinds: &[usize], path: &str) -> Option<Answer> {
            let Some((&k, rest)) = kinds.split_first() else {
                return (path != "/lit").then(|| answer(200, format!("{path} []")));
            };
            match k {
                0 => model(rest, path).map(|a| answer(a.status, format!("{}.", a.body))),
                1 if path == "/x" => Some(answer(403, "no")),
                2 => Some(model(rest, path).unwrap_or_else(|| answer(500, "internal error"))),
                3 if path == "/y" => None,
                _ => model(rest, path),
            }
        }
        let got = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| p.run(&call(path)))).ok();
        check!(format!("steps {kinds:?} (0 tag, 1 block /x, 2 shield, 3 panic on /y), path {path}"), got, model(&kinds, path));
    }
}
