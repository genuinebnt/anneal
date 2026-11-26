use solution::*;

#[allow(dead_code)]
fn limits() -> Limits {
    Limits::default()
}

#[allow(dead_code)]
fn parse(bytes: &[u8]) -> Parsed {
    parse_request(bytes, &Limits::default())
}

/// The request in `bytes`, which must be exactly one whole request.
#[allow(dead_code)]
fn whole(bytes: &[u8]) -> Request {
    match parse(bytes) {
        Parsed::Done(request, used) => {
            assert_eq!(used, bytes.len(), "bytes used by {:?}", String::from_utf8_lossy(bytes));
            request
        }
        other => panic!("expected a whole request from {:?}, got {other:?}", String::from_utf8_lossy(bytes)),
    }
}

#[allow(dead_code)]
fn bad(status: u16, bytes: &[u8]) {
    let got = match parse(bytes) {
        Parsed::Bad(e) => Some(e.status()),
        _ => None,
    };
    check!(format!("{:?}", String::from_utf8_lossy(&bytes[..bytes.len().min(200)])), got, Some(status));
}

/// A request built by hand, for calling handlers directly.
#[allow(dead_code)]
fn request(method: &str, path: &str, headers: &[(&str, &str)]) -> Request {
    Request {
        method: method.to_string(),
        path: path.to_string(),
        query: String::new(),
        minor: 1,
        headers: headers.iter().map(|(k, v)| (k.to_ascii_lowercase(), v.to_string())).collect(),
        body: Vec::new(),
    }
}

use std::sync::{Arc, Mutex};

fn text(r: &Response) -> String {
    match &r.body {
        Body::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        Body::Empty => String::new(),
        Body::Stream(_) => "<stream>".into(),
    }
}

fn routes(list: &[(&str, &str, &'static str)]) -> Router {
    let mut r = Router::new();
    for &(method, pattern, name) in list {
        r.add(method, pattern, move |_, p| Response::text(200, format!("{name} {} {}", p.get("a").unwrap_or("-"), p.get("b").unwrap_or("-"))));
    }
    r
}

fn run(r: &Router, method: &str, path: &str) -> (u16, String) {
    let out = r.dispatch(&request(method, path, &[]));
    let detail = if out.status == 200 { text(&out) } else { out.header_value("allow").unwrap_or("").to_string() };
    (out.status, detail)
}

#[test]
fn a_whole_path_must_match() {
    let r = routes(&[("GET", "/a/:a", "x")]);
    for path in ["/a", "/a/", "/a/1/2", "/a/1/", "/b/1", "/"] {
        check!(path, run(&r, "GET", path).0, 404);
    }
}

#[test]
fn rest_takes_zero_or_more_segments() {
    let r = routes(&[("GET", "/s/*a", "rest")]);
    check!("/s/x/y.txt", run(&r, "GET", "/s/x/y.txt"), (200, "rest x/y.txt -".to_string()));
    check!("/s/", run(&r, "GET", "/s/"), (200, "rest  -".to_string()));
    check!("/s", run(&r, "GET", "/s"), (200, "rest  -".to_string()));
    check!("/sx", run(&r, "GET", "/sx").0, 404);
}

#[test]
fn the_first_matching_route_wins() {
    let r = routes(&[("GET", "/a/new", "literal"), ("GET", "/a/:a", "param")]);
    check!("/a/new", run(&r, "GET", "/a/new"), (200, "literal - -".to_string()));
    let r = routes(&[("GET", "/a/:a", "param"), ("GET", "/a/new", "literal")]);
    check!("/a/new, the other order", run(&r, "GET", "/a/new"), (200, "param new -".to_string()));
}

#[test]
fn a_later_route_with_the_right_method_still_matches() {
    let r = routes(&[("GET", "/a/:a", "get"), ("POST", "/a/new", "post")]);
    check!("POST /a/new", run(&r, "POST", "/a/new"), (200, "post - -".to_string()));
}

#[test]
fn allow_lists_each_method_once() {
    let r = routes(&[("GET", "/x/y", "1"), ("PUT", "/x/:a", "2"), ("GET", "/x/:b", "3")]);
    check!("DELETE /x/y", run(&r, "DELETE", "/x/y"), (405, "GET, PUT".to_string()));
}

#[test]
fn head_is_served_by_get_unless_there_is_a_head_route() {
    let r = routes(&[("GET", "/h", "get")]);
    check!("HEAD with only GET", run(&r, "HEAD", "/h").0, 200);
    let r = routes(&[("GET", "/h", "get"), ("HEAD", "/h", "head")]);
    check!("HEAD with its own route", run(&r, "HEAD", "/h"), (200, "head - -".to_string()));
    let r = routes(&[("POST", "/h", "post")]);
    check!("HEAD with only POST", run(&r, "HEAD", "/h"), (405, "POST".to_string()));
}

#[test]
fn the_first_step_added_is_the_outermost() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let step = |name: &'static str, log: Arc<Mutex<Vec<String>>>| {
        move |req: &Request, next: Next<'_>| {
            log.lock().unwrap().push(format!("{name} in"));
            let out = next(req);
            log.lock().unwrap().push(format!("{name} out {}", out.status));
            out.header("X-Seen", name)
        }
    };
    let app = App::new(routes(&[("GET", "/", "r")])).wrap(step("a", log.clone())).wrap(step("b", log.clone()));
    let out = app.handle(&request("GET", "/", &[]));
    check!("order of calls", log.lock().unwrap().clone(), ["a in", "b in", "b out 200", "a out 200"].map(String::from).to_vec());
    let seen: Vec<&str> = out.headers.iter().filter(|(k, _)| k == "X-Seen").map(|(_, v)| v.as_str()).collect();
    check!("headers added from the inside out", seen, vec!["b", "a"]);
}

#[test]
fn a_step_may_answer_by_itself_change_the_request_or_go_on_twice() {
    let calls = Arc::new(Mutex::new(0));
    let c = calls.clone();
    let mut r = Router::new();
    r.add("GET", "/real", move |req, _| {
        *c.lock().unwrap() += 1;
        Response::text(200, req.header("x-added").unwrap_or("none").to_string())
    });
    let app = App::new(r)
        .wrap(|req: &Request, next: Next<'_>| if req.path == "/blocked" { Response::new(403) } else { next(req) })
        .wrap(|req: &Request, next: Next<'_>| {
            let mut changed = req.clone();
            changed.path = "/real".into();
            changed.headers.push(("x-added".into(), "yes".into()));
            next(&changed);
            next(&changed)
        });
    check!("short circuit", (app.handle(&request("GET", "/blocked", &[])).status, *calls.lock().unwrap()), (403, 0));
    let out = app.handle(&request("GET", "/other", &[]));
    check!("rewritten and retried", (out.status, text(&out), *calls.lock().unwrap()), (200, "yes".to_string(), 2));
}

#[test]
fn steps_outside_the_catching_one_see_the_500_and_the_app_keeps_working() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s = seen.clone();
    let mut r = Router::new();
    r.add("GET", "/boom", |_, _| panic!("secret detail 1234"));
    r.add("GET", "/ok", |_, _| Response::text(200, "ok"));
    let app = App::new(r)
        .wrap(move |req: &Request, next: Next<'_>| {
            let out = next(req);
            s.lock().unwrap().push(out.status);
            out
        })
        .wrap(catch_panics())
        .wrap(|req: &Request, next: Next<'_>| next(req));
    for _ in 0..3 {
        let out = app.handle(&request("GET", "/boom", &[]));
        check!("500 without the panic message", (out.status, text(&out).contains("1234")), (500, false));
        check!("still serving", app.handle(&request("GET", "/ok", &[])).status, 200);
    }
    check!("what the outer step saw", seen.lock().unwrap().clone(), vec![500, 200, 500, 200, 500, 200]);
}

#[test]
fn a_panic_in_a_step_inside_is_caught_and_outside_is_not() {
    let boom = |req: &Request, next: Next<'_>| {
        if req.path == "/p" {
            panic!("step bug");
        }
        next(req)
    };
    let app = App::new(Router::new()).wrap(catch_panics()).wrap(boom);
    check!("inside", app.handle(&request("GET", "/p", &[])).status, 500);
    let app = App::new(Router::new()).wrap(boom).wrap(catch_panics());
    let outside = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.handle(&request("GET", "/p", &[])).status));
    check!("outside the catching step the panic goes through", outside.is_err(), true);
}

#[test]
fn without_the_catching_step_a_panic_goes_through() {
    let mut r = Router::new();
    r.add("GET", "/boom", |_, _| panic!("x"));
    let app = App::new(r);
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.handle(&request("GET", "/boom", &[])).status));
    check!("no catch_panics", out.is_err(), true);
}

#[test]
fn bad_request_ids_are_replaced_and_new_ones_differ() {
    let app = App::new(Router::new()).wrap(request_ids());
    let long = "a".repeat(65);
    let mut made = std::collections::HashSet::new();
    for bad in ["", "has space", "semi;colon", "new\r\nline", long.as_str(), "é"] {
        let out = app.handle(&request("GET", "/", &[("x-request-id", bad)]));
        let id = out.header_value("x-request-id").unwrap_or("").to_string();
        check!(format!("{bad:?} replaced"), id.starts_with("req-"), true);
        made.insert(id);
    }
    check!("six different ids", made.len(), 6);
    let ok = "b".repeat(64);
    check!("64 characters are fine", app.handle(&request("GET", "/", &[("x-request-id", &ok)])).header_value("x-request-id").map(str::to_string), Some(ok));
}

#[test]
fn an_app_is_shared_between_threads() {
    let app = Arc::new(App::new(routes(&[("GET", "/n/:a", "n")])).wrap(request_ids()).wrap(catch_panics()));
    let handles: Vec<_> = (0..4)
        .map(|t| {
            let app = app.clone();
            std::thread::spawn(move || (0..50).map(|i| text(&app.handle(&request("GET", &format!("/n/{t}-{i}"), &[])))).collect::<Vec<_>>())
        })
        .collect();
    for (t, h) in handles.into_iter().enumerate() {
        check!(format!("thread {t}"), h.join().unwrap(), (0..50).map(|i| format!("n {t}-{i} -")).collect::<Vec<_>>());
    }
}

#[test]
fn random_routes_against_a_model() {
    let mut rng = anneal_prelude::Rng::new(81005);
    let words = ["a", "b", ""];
    for _ in 0..300 {
        let mut table: Vec<(&str, Vec<&str>, String)> = Vec::new();
        let mut router = Router::new();
        for i in 0..rng.below(5) {
            let method = *rng.pick(&["GET", "PUT"]);
            let len = 1 + rng.below(3);
            let mut pattern: Vec<&str> = (0..len).map(|_| *rng.pick(&["a", "b", ":x"])).collect();
            if rng.below(4) == 0 {
                pattern.push("*rest");
            }
            let name = format!("r{i}");
            let n = name.clone();
            router.add(method, &format!("/{}", pattern.join("/")), move |_, _| Response::text(200, n.clone()));
            table.push((method, pattern, name));
        }
        let method = *rng.pick(&["GET", "PUT", "POST"]);
        let len = 1 + rng.below(4);
        let segs: Vec<&str> = (0..len).map(|_| *rng.pick(&words)).collect();
        let path = format!("/{}", segs.join("/"));
        let fits = |pattern: &[&str]| {
            let rest = pattern.last() == Some(&"*rest");
            let fixed = &pattern[..pattern.len() - usize::from(rest)];
            if segs.len() < fixed.len() || (!rest && segs.len() != fixed.len()) {
                return false;
            }
            fixed.iter().zip(&segs).all(|(p, s)| if *p == ":x" { !s.is_empty() } else { p == s })
        };
        let hit = table.iter().find(|(m, p, _)| *m == method && fits(p));
        let want = match hit {
            Some((_, _, name)) => (200, name.clone()),
            None => {
                let mut allow: Vec<&str> = Vec::new();
                for (m, p, _) in &table {
                    if fits(p) && !allow.contains(m) {
                        allow.push(m);
                    }
                }
                if allow.is_empty() { (404, String::new()) } else { (405, allow.join(", ")) }
            }
        };
        let described: Vec<String> = table.iter().map(|(m, p, n)| format!("{m} /{} = {n}", p.join("/"))).collect();
        check!(format!("{method} {path} with {described:?}"), run(&router, method, &path), want);
    }
}
