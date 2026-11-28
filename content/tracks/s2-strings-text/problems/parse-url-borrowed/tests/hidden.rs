use solution::*;

#[test]
fn empty_host() {
    check!(r#""ftp://:21/""#, parse_url("ftp://:21/"), None);
}

#[test]
fn port_too_big() {
    check!(r#""http://h:99999/""#, parse_url("http://h:99999/"), None);
}

#[test]
fn no_scheme() {
    check!(r#""example.com/x""#, parse_url("example.com/x"), None);
}

#[test]
fn flag_query() {
    check!(r#""http://h?flag&&a=""#, parse_url("http://h?flag&&a=").map(|u| (u.path, u.query)), Some(("/", vec![("flag", ""), ("a", "")])));
}

#[test]
fn borrows_input() {
    let s = String::from("http://host/p");
    let u = parse_url(&s).unwrap();
    let same = std::ptr::eq(u.host.as_ptr(), s[7..].as_ptr());
    check!(r#"host points into the input"#, same, true);
}

#[test]
fn empty_scheme() {
    check!(r#""://h""#, parse_url("://h"), None);
}

#[test]
fn empty_port() {
    check!(r#""http://h:/""#, parse_url("http://h:/"), None);
}

#[test]
fn port_bounds() {
    check!(r#""http://h:0", "http://h:65535", "http://h:65536""#, (parse_url("http://h:0").map(|u| u.port), parse_url("http://h:65535").map(|u| u.port), parse_url("http://h:65536")), (Some(Some(0)), Some(Some(65535)), None));
}

#[test]
fn query_without_path() {
    check!(r#""http://h?a=1""#, parse_url("http://h?a=1").map(|u| (u.path, u.query)), Some(("/", vec![("a", "1")])));
}

#[test]
fn value_with_equals() {
    check!(r#""http://h/?a=b=c""#, parse_url("http://h/?a=b=c").map(|u| u.query), Some(vec![("a", "b=c")]));
}

#[test]
fn empty_query() {
    check!(r#""http://h/p?""#, parse_url("http://h/p?").map(|u| (u.path, u.query)), Some(("/p", vec![])));
}

#[test]
fn question_mark_in_query() {
    check!(r#""http://h?a=?&b""#, parse_url("http://h?a=?&b").map(|u| u.query), Some(vec![("a", "?"), ("b", "")]));
}

#[test]
fn trailing_slash_path() {
    check!(r#""http://h/a/""#, parse_url("http://h/a/").map(|u| u.path), Some("/a/"));
}

#[test]
fn random_vs_brute_force() {
    let mut rng = anneal_prelude::Rng::new(2216);
    let schemes = ["http", "ftp", "a"];
    let hosts = ["h", "ex.org", "1.2.3.4"];
    let paths = ["", "/", "/a", "/a/b.c"];
    let keys = ["k", "x"];
    let values = ["", "1", "v=w"];
    for _ in 0..300 {
        let (scheme, host, path) = (*rng.pick(&schemes), *rng.pick(&hosts), *rng.pick(&paths));
        let port = if rng.bool() { Some(rng.below(65_536) as u16) } else { None };
        let mut s = format!("{scheme}://{host}");
        if let Some(p) = port {
            s += &format!(":{p}");
        }
        s += path;
        let mut query = Vec::new();
        if rng.bool() {
            let mut parts = Vec::new();
            for _ in 0..rng.below(4) {
                let (k, v) = (*rng.pick(&keys), *rng.pick(&values));
                query.push((k, v));
                parts.push(if v.is_empty() && rng.bool() { k.to_string() } else { format!("{k}={v}") });
            }
            s += "?";
            s += &parts.join("&");
        }
        let want = Url { scheme, host, port, path: if path.is_empty() { "/" } else { path }, query };
        check!(format!("s = {s:?}"), parse_url(&s), Some(want));
    }
}

#[test]
fn slash_inside_query() {
    check!(r#""http://h?next=/a/b""#, parse_url("http://h?next=/a/b").map(|u| (u.host, u.path, u.query)), Some(("h", "/", vec![("next", "/a/b")])));
}
