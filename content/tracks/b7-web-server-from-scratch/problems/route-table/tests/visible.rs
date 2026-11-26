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
fn literal_paths() {
    let t = table(&[("GET", "/"), ("GET", "/about")]);
    check!("/ and /about", (ask(&t, "GET", "/", &[]), ask(&t, "GET", "/about", &[])), ("0".to_string(), "1".to_string()));
}

#[test]
fn captured_values() {
    let t = table(&[("GET", "/users/:id/posts/:post")]);
    check!("/users/7/posts/42", ask(&t, "GET", "/users/7/posts/42", &["id", "post"]), "0 id=7 post=42".to_string());
}

#[test]
fn the_rest_of_a_path() {
    let t = table(&[("GET", "/files/*path")]);
    check!("/files/a/b.txt", ask(&t, "GET", "/files/a/b.txt", &["path"]), "0 path=a/b.txt".to_string());
}

#[test]
fn nothing_matches() {
    check!("/nope", ask(&table(&[("GET", "/a")]), "GET", "/nope", &[]), "404".to_string());
}

#[test]
fn the_path_exists_but_not_for_this_method() {
    let t = table(&[("GET", "/a"), ("POST", "/a")]);
    check!("DELETE /a", ask(&t, "DELETE", "/a", &[]), "405 GET,POST".to_string());
}

#[test]
fn the_query_is_ignored() {
    let t = table(&[("GET", "/a/:x")]);
    check!("/a/1?next=/b/2", ask(&t, "GET", "/a/1?next=/b/2", &["x"]), "0 x=1".to_string());
}
