use solution::*;

#[allow(dead_code, unused_imports)]
fn table(routes: &[(&str, &str)]) -> Table<String> {
    let mut t = Table::new();
    for (i, (method, pattern)) in routes.iter().enumerate() {
        t.add(method, pattern, format!("{i}"));
    }
    t
}

/// "2 id=7" for route number 2, "405 GET,PUT", or "404".
#[allow(dead_code, unused_imports)]
fn ask(t: &Table<String>, method: &str, path: &str, names: &[&str]) -> String {
    match t.find(method, path) {
        Found::Route(value, params) => {
            let mut out = value.clone();
            for n in names {
                if let Some(v) = params.get(n) {
                    out.push_str(&format!(" {n}={v}"));
                }
            }
            out
        }
        Found::WrongMethod(allowed) => format!("405 {}", allowed.join(",")),
        Found::Nothing => "404".to_string(),
    }
}

#[test]
fn the_whole_path_must_match() {
    let t = table(&[("GET", "/a/:x")]);
    for path in ["/a", "/a/", "/a/1/", "/a/1/2", "/b/1", "/", "/A/1"] {
        check!(path, ask(&t, "GET", path, &[]), "404".to_string());
    }
}

#[test]
fn a_trailing_slash_is_part_of_the_pattern() {
    let t = table(&[("GET", "/dir/"), ("GET", "/dir")]);
    check!("both", (ask(&t, "GET", "/dir/", &[]), ask(&t, "GET", "/dir", &[])), ("0".to_string(), "1".to_string()));
}

#[test]
fn the_rest_may_be_empty() {
    let t = table(&[("GET", "/s/*p")]);
    check!("/s/", ask(&t, "GET", "/s/", &["p"]), "0 p=".to_string());
    check!("/s", ask(&t, "GET", "/s", &["p"]), "0 p=".to_string());
    check!("/s//x", ask(&t, "GET", "/s//x", &["p"]), "0 p=/x".to_string());
    check!("/sx", ask(&t, "GET", "/sx", &["p"]), "404".to_string());
}

#[test]
fn earlier_routes_win_among_those_with_the_method() {
    let t = table(&[("GET", "/a/new"), ("GET", "/a/:id")]);
    check!("literal first", ask(&t, "GET", "/a/new", &["id"]), "0".to_string());
    let t = table(&[("GET", "/a/:id"), ("GET", "/a/new")]);
    check!("capture first", ask(&t, "GET", "/a/new", &["id"]), "0 id=new".to_string());
    let t = table(&[("GET", "/a/:id"), ("POST", "/a/new")]);
    check!("a later route with the method", ask(&t, "POST", "/a/new", &["id"]), "1".to_string());
}

#[test]
fn allowed_methods_once_each_in_order_of_adding() {
    let t = table(&[("PUT", "/x/:a"), ("GET", "/x/y"), ("PUT", "/x/y"), ("GET", "/other")]);
    check!("DELETE /x/y", ask(&t, "DELETE", "/x/y", &[]), "405 PUT,GET".to_string());
}

#[test]
fn head_falls_back_to_get() {
    let t = table(&[("GET", "/a/:x"), ("GET", "/a/b")]);
    check!("first GET", ask(&t, "HEAD", "/a/b", &["x"]), "0 x=b".to_string());
    let t = table(&[("GET", "/a"), ("HEAD", "/a")]);
    check!("own HEAD route even if later", ask(&t, "HEAD", "/a", &[]), "1".to_string());
    check!("HEAD where only POST", ask(&table(&[("POST", "/p")]), "HEAD", "/p", &[]), "405 POST".to_string());
    check!("GET does not use HEAD", ask(&table(&[("HEAD", "/p")]), "GET", "/p", &[]), "405 HEAD".to_string());
}

#[test]
fn methods_are_exact() {
    check!("get is not GET", ask(&table(&[("GET", "/a")]), "get", "/a", &[]), "405 GET".to_string());
}

#[test]
fn capture_count_and_unknown_names() {
    let t = table(&[("GET", "/:a/*b")]);
    match t.find("GET", "/1/2/3") {
        Found::Route(_, p) => check!("two captures", (p.len(), p.is_empty(), p.get("a"), p.get("b"), p.get("c")), (2, false, Some("1"), Some("2/3"), None)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_table_holds_any_type() {
    let mut t: Table<Box<dyn Fn(u32) -> u32>> = Table::new();
    t.add("GET", "/double", Box::new(|n| n * 2));
    let k = 5;
    t.add("GET", "/add", Box::new(move |n| n + k));
    for (path, want) in [("/double", 14), ("/add", 12)] {
        match t.find("GET", path) {
            Found::Route(f, _) => check!(path, f(7), want),
            _ => panic!("no route for {path}"),
        }
    }
}

#[test]
fn an_empty_table() {
    check!("nothing", ask(&Table::new(), "GET", "/", &[]), "404".to_string());
}

#[test]
fn random_tables_against_a_model() {
    let mut rng = anneal_prelude::Rng::new(71001);
    for _ in 0..500 {
        let mut routes: Vec<(&str, Vec<&str>)> = Vec::new();
        for _ in 0..rng.below(5) {
            let method = *rng.pick(&["GET", "PUT", "HEAD"]);
            let len = 1 + rng.below(3);
            let mut pattern: Vec<&str> = (0..len).map(|_| *rng.pick(&["a", "b", ":x", ""])).collect();
            if rng.below(4) == 0 {
                pattern.push("*r");
            }
            routes.push((method, pattern));
        }
        let patterns: Vec<(String, String)> = routes.iter().map(|(m, p)| (m.to_string(), format!("/{}", p.join("/")))).collect();
        let mut t = Table::new();
        for (i, (m, p)) in patterns.iter().enumerate() {
            t.add(m, p, i.to_string());
        }
        let method = *rng.pick(&["GET", "PUT", "HEAD", "POST"]);
        let len = 1 + rng.below(4);
        let segs: Vec<&str> = (0..len).map(|_| *rng.pick(&["a", "b", "c", ""])).collect();
        let path = format!("/{}", segs.join("/"));
        // the captures a pattern would make, or None
        let fit = |pattern: &[&str]| -> Option<String> {
            let rest = pattern.last() == Some(&"*r");
            let fixed = &pattern[..pattern.len() - usize::from(rest)];
            if segs.len() < fixed.len() || (!rest && segs.len() != fixed.len()) {
                return None;
            }
            let mut got = String::new();
            for (p, s) in fixed.iter().zip(&segs) {
                if *p == ":x" {
                    if s.is_empty() {
                        return None;
                    }
                    if got.is_empty() {
                        got = format!(" x={s}");
                    }
                } else if p != s {
                    return None;
                }
            }
            if rest {
                got.push_str(&format!(" r={}", segs[fixed.len()..].join("/")));
            }
            Some(got)
        };
        let matching: Vec<(usize, &str, String)> = routes.iter().enumerate().filter_map(|(i, (m, p))| fit(p).map(|c| (i, *m, c))).collect();
        let chosen = matching.iter().find(|(_, m, _)| *m == method).or_else(|| matching.iter().find(|(_, m, _)| method == "HEAD" && *m == "GET"));
        let want = match chosen {
            Some((i, _, captures)) => format!("{i}{captures}"),
            None if matching.is_empty() => "404".to_string(),
            None => {
                let mut allow: Vec<&str> = Vec::new();
                for (_, m, _) in &matching {
                    if !allow.contains(m) {
                        allow.push(m);
                    }
                }
                format!("405 {}", allow.join(","))
            }
        };
        check!(format!("{method} {path} in {patterns:?}"), ask(&t, method, &path, &["x", "r"]), want);
    }
}
