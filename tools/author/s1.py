from author import T, write_track

P = []

CONFIG_HEAD = """
        use std::collections::HashMap;

        #[derive(Debug, PartialEq, Eq)]
        pub struct Server {
            pub host: String,
            pub port: u16,
            pub tls: bool,
            pub name: String,
            pub workers: usize,
        }
"""

CONFIG_SOL = CONFIG_HEAD + """
        pub fn server(cfg: &HashMap<String, String>) -> Result<Server, String> {
            // An empty value counts as unset, so filter each key before falling back.
            let set = |key: &str| cfg.get(key).filter(|v| !v.is_empty());
            let host = set("host").or_else(|| set("bind")).map_or("127.0.0.1", String::as_str).to_string();
            let port = cfg
                .get("port")
                .ok_or_else(|| "missing port".to_string())
                .and_then(|v| v.parse::<u16>().ok().filter(|&p| p != 0).ok_or_else(|| format!("invalid port: {v}")))?;
            let workers = match cfg.get("workers") {
                None => 1,
                Some(v) => v.parse::<usize>().ok().filter(|&w| w > 0).ok_or_else(|| format!("invalid workers: {v}"))?,
            };
            Ok(Server {
                host,
                port,
                tls: cfg.get("tls").is_some_and(|v| v == "on"),
                name: cfg.get("name").cloned().unwrap_or_default(),
                workers,
            })
        }
"""

CONFIG_TESTS = """
        use std::collections::HashMap;

        fn cfg(pairs: &[(&str, &str)]) -> HashMap<String, String> {
            pairs.iter().map(|&(k, v)| (k.to_string(), v.to_string())).collect()
        }

        fn srv(host: &str, port: u16, tls: bool, name: &str, workers: usize) -> Result<Server, String> {
            Ok(Server { host: host.to_string(), port, tls, name: name.to_string(), workers })
        }
"""

P.append(dict(
    slug="config-port", title="Read a server config", level="easy", stage="use-it", tags=["ok_or_else", "or_else", "is_some_and", "unwrap_or_default"],
    teaches=[
        "`ok_or_else` + `and_then` + `?` turn \"missing\" and \"malformed\" into two different errors.",
        "`filter` before `or_else`: an empty value must fall through to the next key, not stop the chain.",
        "Absent means default, present-but-bad is an error: `.parse().ok().unwrap_or(d)` silently merges the two.",
        "`is_some_and`, `cloned().unwrap_or_default()` for the one-liners.",
    ],
    statement="""
        Build a `Server` from a string map `cfg`:

        - `host`: the `"host"` value, else the `"bind"` value, else `"127.0.0.1"`. An empty value counts as unset.
        - `port`: required. `Err("missing port")` if absent, `Err("invalid port: <value>")` unless it's a `u16` other than 0.
        - `tls`: true only when `"tls"` is exactly `"on"`.
        - `name`: the `"name"` value, or `""`.
        - `workers`: 1 if absent, otherwise a `usize` of at least 1, else `Err("invalid workers: <value>")`.

        Check the port before the workers. Values are parsed as they are: no trimming.
    """,
    examples=[("cfg = {port: \"80\"}", "Ok(Server { host: \"127.0.0.1\", port: 80, tls: false, name: \"\", workers: 1 })"),
              ("cfg = {host: \"\", bind: \"0.0.0.0\", port: \"0\"}", "Err(\"invalid port: 0\")")],
    starter=CONFIG_HEAD + """
        pub fn server(cfg: &HashMap<String, String>) -> Result<Server, String> {
            todo!()
        }
    """,
    solution=CONFIG_SOL,
    visible=[
        CONFIG_TESTS,
        T("everything_set", "host = \"db.local\", port = \"5432\", tls = \"on\", name = \"primary\", workers = \"8\"",
          'server(&cfg(&[("host", "db.local"), ("port", "5432"), ("tls", "on"), ("name", "primary"), ("workers", "8")]))', 'srv("db.local", 5432, true, "primary", 8)'),
        T("only_a_port", "port = \"80\"", 'server(&cfg(&[("port", "80")]))', 'srv("127.0.0.1", 80, false, "", 1)'),
        T("missing_port", "host = \"a\"", 'server(&cfg(&[("host", "a")]))', 'Err("missing port".to_string())'),
        T("empty_host_falls_back_to_bind", "host = \"\", bind = \"0.0.0.0\", port = \"80\"", 'server(&cfg(&[("host", ""), ("bind", "0.0.0.0"), ("port", "80")]))', 'srv("0.0.0.0", 80, false, "", 1)'),
        T("port_zero_is_invalid", "port = \"0\"", 'server(&cfg(&[("port", "0")]))', 'Err("invalid port: 0".to_string())'),
    ],
    hidden=[
        CONFIG_TESTS,
        T("host_beats_bind", "host = \"a\", bind = \"b\", port = \"1\"", 'server(&cfg(&[("host", "a"), ("bind", "b"), ("port", "1")]))', 'srv("a", 1, false, "", 1)'),
        T("bind_only", "bind = \"b\", port = \"1\"", 'server(&cfg(&[("bind", "b"), ("port", "1")]))', 'srv("b", 1, false, "", 1)'),
        T("host_and_bind_both_empty", "host = \"\", bind = \"\", port = \"1\"", 'server(&cfg(&[("host", ""), ("bind", ""), ("port", "1")]))', 'srv("127.0.0.1", 1, false, "", 1)'),
        T("port_bounds", "port = \"65535\", then port = \"65536\"", '(server(&cfg(&[("port", "65535")])), server(&cfg(&[("port", "65536")])))', '(srv("127.0.0.1", 65535, false, "", 1), Err("invalid port: 65536".to_string()))'),
        T("port_not_trimmed_or_signed", "port = \" 80\", then port = \"-1\"", '(server(&cfg(&[("port", " 80")])), server(&cfg(&[("port", "-1")])))', '(Err("invalid port:  80".to_string()), Err("invalid port: -1".to_string()))'),
        T("empty_port_is_invalid_not_missing", "port = \"\"", 'server(&cfg(&[("port", "")]))', 'Err("invalid port: ".to_string())'),
        T("workers_zero", "port = \"80\", workers = \"0\"", 'server(&cfg(&[("port", "80"), ("workers", "0")]))', 'Err("invalid workers: 0".to_string())'),
        T("workers_garbage_is_not_the_default", "port = \"80\", workers = \"many\"", 'server(&cfg(&[("port", "80"), ("workers", "many")]))', 'Err("invalid workers: many".to_string())'),
        T("empty_workers_is_invalid", "port = \"80\", workers = \"\"", 'server(&cfg(&[("port", "80"), ("workers", "")]))', 'Err("invalid workers: ".to_string())'),
        T("port_checked_before_workers", "port = \"x\", workers = \"0\"", 'server(&cfg(&[("port", "x"), ("workers", "0")]))', 'Err("invalid port: x".to_string())'),
        T("missing_port_before_workers", "workers = \"0\"", 'server(&cfg(&[("workers", "0")]))', 'Err("missing port".to_string())'),
        T("tls_only_exactly_on", "tls = \"ON\" / \"true\" / \"on\"",
          '[server(&cfg(&[("port", "1"), ("tls", "ON")])), server(&cfg(&[("port", "1"), ("tls", "true")])), server(&cfg(&[("port", "1"), ("tls", "on")]))].map(|r| r.map(|s| s.tls))', "[Ok(false), Ok(false), Ok(true)]"),
        T("unicode_values", "host = \"прокси\", name = \"café ☕\", port = \"443\"", 'server(&cfg(&[("host", "прокси"), ("name", "café ☕"), ("port", "443")]))', 'srv("прокси", 443, false, "café ☕", 1)'),
        T("empty_config", "{}", "server(&HashMap::new())", 'Err("missing port".to_string())'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(7101);
            let keys = ["host", "bind", "port", "tls", "name", "workers"];
            let values = ["", "a", "on", "0", "1", "80", "65535", "65536", "-1", " 8", "x"];
            for _ in 0..400 {
                let mut pairs: Vec<(&str, &str)> = Vec::new();
                for &k in &keys {
                    if rng.below(3) > 0 {
                        pairs.push((k, *rng.pick(&values)));
                    }
                }
                let get = |k: &str| pairs.iter().find(|(pk, _)| *pk == k).map(|(_, v)| *v);
                let want = (|| {
                    let port = match get("port") {
                        None => return Err("missing port".to_string()),
                        Some(v) => match v.parse::<u16>() {
                            Ok(p) if p > 0 => p,
                            _ => return Err(format!("invalid port: {v}")),
                        },
                    };
                    let workers = match get("workers") {
                        None => 1,
                        Some(v) => match v.parse::<usize>() {
                            Ok(w) if w > 0 => w,
                            _ => return Err(format!("invalid workers: {v}")),
                        },
                    };
                    let host = match (get("host"), get("bind")) {
                        (Some(h), _) if !h.is_empty() => h,
                        (_, Some(b)) if !b.is_empty() => b,
                        _ => "127.0.0.1",
                    };
                    srv(host, port, get("tls") == Some("on"), get("name").unwrap_or(""), workers)
                })();
                check!(format!("cfg = {pairs:?}"), server(&cfg(&pairs)), want);
            }
        }
        """,
    ],
    wrong=dict(
        filter_after_fallback=CONFIG_SOL.replace('let host = set("host").or_else(|| set("bind"))', 'let host = cfg.get("host").or_else(|| cfg.get("bind")).filter(|v| !v.is_empty())'),
        port_zero_allowed=CONFIG_SOL.replace(".filter(|&p| p != 0)", ""),
        bad_workers_become_the_default=CONFIG_SOL.replace('Some(v) => v.parse::<usize>().ok().filter(|&w| w > 0).ok_or_else(|| format!("invalid workers: {v}"))?,', "Some(v) => v.parse::<usize>().ok().filter(|&w| w > 0).unwrap_or(1),"),
    ),
    hints=[("approach", "Handle each field as its own small chain; only `port` and `workers` can fail, and `?` returns their errors in the order you write them."),
           ("rust", "`cfg.get(\"port\").ok_or_else(..).and_then(|v| v.parse::<u16>().ok().filter(..).ok_or_else(..))?`. For `host`, `a.or_else(|| b)` tries `b` only when `a` is `None`."),
           ("edge case", "`cfg.get(\"host\").or_else(..).filter(|v| !v.is_empty())` drops to the default when `host` is empty even if `bind` is set. Filter each key first.")],
    notes=("""Each field is one chain, and the chains say what absence means: required (`ok_or_else`), optional with a fallback key (`or_else`), a flag (`is_some_and`), a default (`unwrap_or_default`). The trap in `workers` is merging "absent" with "malformed": `v.parse().ok().unwrap_or(1)` would run a server with the default when someone typed `workers = many`. Syntax to remember: `opt.ok_or_else(|| e)`, `opt.or_else(|| other)`, `opt.filter(|v| pred)`, `opt.is_some_and(|v| pred)` (and `res.is_ok_and`), `map.get(k).cloned().unwrap_or_default()`, `opt.map_or(default, f)`.""", "O(total length of the values)", "O(1) beyond the output"),
    follow_up="How would you report every bad field at once instead of the first? What would a typed `ConfigError` enum buy the caller over `String`?",
    related=["L8", "S4"],
))

JSON_TYPE = """
        #[derive(Debug, Clone, PartialEq)]
        pub enum Json {
            Null,
            Bool(bool),
            Num(f64),
            Str(String),
            Arr(Vec<Json>),
            /// Fields in document order. Keys may repeat; the first one wins.
            Obj(Vec<(String, Json)>),
        }
"""

JSON_SOL = JSON_TYPE + """
        /// The value at a dotted `path` such as `"users.0.name"`, or `None`. The empty path is `root` itself.
        pub fn get<'a>(root: &'a Json, path: &str) -> Option<&'a Json> {
            if path.is_empty() {
                return Some(root);
            }
            let mut node = root;
            for seg in path.split('.') {
                node = match node {
                    Json::Obj(fields) => &fields.iter().find(|(k, _)| k == seg)?.1,
                    Json::Arr(items) => {
                        if seg.is_empty() || !seg.bytes().all(|b| b.is_ascii_digit()) {
                            return None;
                        }
                        items.get(seg.parse::<usize>().ok()?)?
                    }
                    _ => return None,
                };
            }
            Some(node)
        }

        /// The number at `path`, if there is a number there.
        pub fn num_at(root: &Json, path: &str) -> Option<f64> {
            let Json::Num(n) = get(root, path)? else {
                return None;
            };
            Some(*n)
        }

        /// True if `path` exists and holds `null`.
        pub fn is_null_at(root: &Json, path: &str) -> bool {
            matches!(get(root, path), Some(Json::Null))
        }
"""

JSON_TESTS = """
        fn s(x: &str) -> Json {
            Json::Str(x.to_string())
        }

        fn obj(fields: Vec<(&str, Json)>) -> Json {
            Json::Obj(fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
        }

        /// {"users": [{"name": "Ada", "age": 36, "admin": true, "boss": null}, {"name": "Linus", "age": 54}],
        ///  "0": "zero", "": {"x": 1}, "dup": 1, "dup": 2, "count": 2, "none": null}
        fn doc() -> Json {
            obj(vec![
                ("users", Json::Arr(vec![
                    obj(vec![("name", s("Ada")), ("age", Json::Num(36.0)), ("admin", Json::Bool(true)), ("boss", Json::Null)]),
                    obj(vec![("name", s("Linus")), ("age", Json::Num(54.0))]),
                ])),
                ("0", s("zero")),
                ("", obj(vec![("x", Json::Num(1.0))])),
                ("dup", Json::Num(1.0)),
                ("dup", Json::Num(2.0)),
                ("count", Json::Num(2.0)),
                ("none", Json::Null),
            ])
        }
"""

P.append(dict(
    slug="question-mark-on-option", title="? on Option: walk a JSON path", level="easy", stage="use-it", tags=["?", "let else", "matches!"],
    teaches=[
        "`?` on every step of a lookup chain: a missing key, a bad index and an out-of-range index all become `None`.",
        "`let ... else` pulls one variant out of an enum or bails; `matches!` answers \"is it this variant?\" without binding.",
        "`\"\".split('.')` yields one empty segment, so the empty path needs its own case.",
    ],
    statement="""
        Write three lookups into a `Json` value by a dotted path such as `"users.0.name"`:

        - `get` returns the value at the path, or `None`. On an object a segment is a key (even if it looks like a
          number; the first matching key wins). On an array it's an index made of decimal digits only. Anything else,
          or a segment that goes nowhere, is `None`. The empty path is `root` itself.
        - `num_at` returns the number at the path, if there is a number there.
        - `is_null_at` is true only if the path exists and holds `null`. A missing path is not `null`.

        None of them may panic.
    """,
    examples=[("path = \"users.1.name\"", "Some(Str(\"Linus\"))"), ("path = \"users.5\"", "None"), ("is_null_at(\"users.1.boss\")", "false: missing, not null")],
    starter=JSON_TYPE + """
        /// The value at a dotted `path` such as `"users.0.name"`, or `None`. The empty path is `root` itself.
        pub fn get<'a>(root: &'a Json, path: &str) -> Option<&'a Json> {
            todo!()
        }

        /// The number at `path`, if there is a number there.
        pub fn num_at(root: &Json, path: &str) -> Option<f64> {
            todo!()
        }

        /// True if `path` exists and holds `null`.
        pub fn is_null_at(root: &Json, path: &str) -> bool {
            todo!()
        }
    """,
    solution=JSON_SOL,
    visible=[
        JSON_TESTS,
        T("nested_lookup", "doc, path = \"users.1.name\"", 'get(&doc(), "users.1.name").cloned()', 'Some(s("Linus"))'),
        T("number_at_a_path", "doc, num_at(\"users.0.age\"), num_at(\"users.0.name\")", '(num_at(&doc(), "users.0.age"), num_at(&doc(), "users.0.name"))', "(Some(36.0), None)"),
        T("empty_path_is_the_root", "doc, path = \"\"", 'get(&d, "") == Some(&d)', "true", setup="let d = doc();"),
        T("index_out_of_range", "doc, path = \"users.5\"", 'get(&doc(), "users.5").cloned()', "None"),
        T("missing_is_not_null", "doc, is_null_at(\"users.0.boss\"), is_null_at(\"users.1.boss\")", '(is_null_at(&doc(), "users.0.boss"), is_null_at(&doc(), "users.1.boss"))', "(true, false)"),
    ],
    hidden=[
        JSON_TESTS,
        T("digit_key_on_an_object", "doc, path = \"0\"", 'get(&doc(), "0").cloned()', 'Some(s("zero"))'),
        T("empty_key_segment", "doc, path = \".x\"", 'num_at(&doc(), ".x")', "Some(1.0)"),
        T("trailing_dot_on_an_array", "doc, path = \"users.\"", 'get(&doc(), "users.").cloned()', "None"),
        T("plus_sign_is_not_an_index", "doc, path = \"users.+1\"", 'get(&doc(), "users.+1").cloned()', "None"),
        T("leading_zero_index", "doc, path = \"users.01.age\"", 'num_at(&doc(), "users.01.age")', "Some(54.0)"),
        T("huge_index_does_not_panic", "doc, path = \"users.99999999999999999999999\"", 'get(&doc(), "users.99999999999999999999999").cloned()', "None"),
        T("negative_index", "doc, path = \"users.-1\"", 'get(&doc(), "users.-1").cloned()', "None"),
        T("path_through_a_scalar", "doc, path = \"count.0\" and \"users.0.name.x\"", '(get(&doc(), "count.0").cloned(), get(&doc(), "users.0.name.x").cloned())', "(None, None)"),
        T("path_through_null", "doc, path = \"none.x\"", 'get(&doc(), "none.x").cloned()', "None"),
        T("first_duplicate_key_wins", "doc, path = \"dup\"", 'num_at(&doc(), "dup")', "Some(1.0)"),
        T("null_at_the_top", "doc, is_null_at(\"none\"), is_null_at(\"nope\"), is_null_at(\"\")", '(is_null_at(&doc(), "none"), is_null_at(&doc(), "nope"), is_null_at(&doc(), ""))', "(true, false, false)"),
        T("scalar_root", "root = 7, path = \"\" and \"a\"", '(num_at(&Json::Num(7.0), ""), get(&Json::Num(7.0), "a"))', "(Some(7.0), None)"),
        T("num_at_on_a_bool", "doc, num_at(\"users.0.admin\")", 'num_at(&doc(), "users.0.admin")', "None"),
        T("unicode_key", "{\"ключ\": [true]}, path = \"ключ.0\"", 'get(&obj(vec![("ключ", Json::Arr(vec![Json::Bool(true)]))]), "ключ.0").cloned()', "Some(Json::Bool(true))"),
        T("borrows_from_the_root", "doc, path = \"users.0\"", 'std::ptr::eq(get(&d, "users.0").unwrap(), match &d { Json::Obj(f) => match &f[0].1 { Json::Arr(a) => &a[0], _ => unreachable!() }, _ => unreachable!() })', "true", setup="let d = doc();"),
        """
        fn brute<'a>(node: &'a Json, segs: &[&str]) -> Option<&'a Json> {
            let Some((first, rest)) = segs.split_first() else {
                return Some(node);
            };
            let next = match node {
                Json::Obj(fields) => fields.iter().find(|(k, _)| k == first).map(|(_, v)| v),
                Json::Arr(items) if !first.is_empty() && first.chars().all(|c| c.is_ascii_digit()) && first.len() < 5 => items.get(first.parse::<usize>().unwrap()),
                _ => None,
            };
            brute(next?, rest)
        }

        fn random_json(rng: &mut anneal_prelude::Rng, depth: usize) -> Json {
            let kind = if depth == 0 { rng.below(3) } else { rng.below(6) };
            match kind {
                0 => Json::Null,
                1 => Json::Num(rng.int(-3, 3) as f64),
                2 => Json::Str("s".to_string()),
                3 | 4 => {
                    let n = rng.below(4);
                    let mut fields = Vec::new();
                    for _ in 0..n {
                        let k = rng.pick(&["a", "b", "0", "1", ""]).to_string();
                        fields.push((k, random_json(rng, depth - 1)));
                    }
                    Json::Obj(fields)
                }
                _ => {
                    let n = rng.below(4);
                    let mut items = Vec::new();
                    for _ in 0..n {
                        items.push(random_json(rng, depth - 1));
                    }
                    Json::Arr(items)
                }
            }
        }

        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(7102);
            for _ in 0..400 {
                let root = random_json(&mut rng, 3);
                let n = rng.below(4);
                let mut segs: Vec<&str> = Vec::new();
                for _ in 0..n {
                    segs.push(*rng.pick(&["a", "b", "0", "1", "2", "", "+1", "01"]));
                }
                let path = segs.join(".");
                let want = if path.is_empty() { Some(&root) } else { brute(&root, &segs) };
                let desc = format!("root = {root:?}, path = {path:?}");
                check!(desc.clone(), get(&root, &path), want);
                check!(format!("num_at, {desc}"), num_at(&root, &path), match want { Some(Json::Num(x)) => Some(*x), _ => None });
                check!(format!("is_null_at, {desc}"), is_null_at(&root, &path), want == Some(&Json::Null));
            }
        }

        #[test]
        fn deep_path() {
            let mut root = Json::Num(42.0);
            for i in 0..1000 {
                root = if i % 2 == 0 { Json::Arr(vec![Json::Null, root]) } else { obj(vec![("k", root)]) };
            }
            let path = vec!["k", "1"].repeat(500).join(".");
            check!("1000 levels, alternating arrays and objects", num_at(&root, &path), Some(42.0));
        }
        """,
    ],
    wrong=dict(
        empty_path_looks_up_the_empty_key=JSON_SOL.replace("            if path.is_empty() {\n                return Some(root);\n            }\n", ""),
        plus_sign_index=JSON_SOL.replace("                        if seg.is_empty() || !seg.bytes().all(|b| b.is_ascii_digit()) {\n                            return None;\n                        }\n", ""),
        missing_counts_as_null=JSON_SOL.replace("matches!(get(root, path), Some(Json::Null))", "get(root, path).is_none_or(|v| matches!(v, Json::Null))"),
        digits_always_index=JSON_SOL.replace("Json::Obj(fields) => &fields.iter().find(|(k, _)| k == seg)?.1,", "Json::Obj(fields) if seg.parse::<usize>().is_err() => &fields.iter().find(|(k, _)| k == seg)?.1,"),
    ),
    hints=[("approach", "Walk the segments, replacing `node` with its child each time. Every way a step can fail should be a `?` or a `return None`."),
           ("rust", "`fields.iter().find(|(k, _)| k == seg)?` and `items.get(i)?` never panic. `let Json::Num(n) = get(root, path)? else { return None };` and `matches!(x, Some(Json::Null))`."),
           ("edge case", "`\"\".split('.')` yields one segment, `\"\"`. And `\"+1\".parse::<usize>()` is `Ok(1)`.")],
    notes=("""`?` turns a chain of fallible steps into straight-line code: the function returns `None` at the first step that finds nothing. `let else` is the refutable-pattern version of `let`, for when only one variant is useful and everything else bails. `matches!` is the boolean form of `match`. Semantics matter as much as syntax here: missing and `null` are different answers, a digit-looking key on an object is still a key, and `usize::from_str` accepts a leading `+`. Syntax to remember: `let Json::Num(n) = expr else { return None };` (the `else` block must diverge), `matches!(v, Some(Json::Null))`, `matches!(c, 'a'..='z' | '_')`, `opt.is_none_or(f)`.""", "O(total length of the path + keys scanned)", "O(1)"),
    follow_up="How would you return an error that says which segment failed, and why? Would `get` still be written with `?`?",
    related=["S6", "L7"],
))

SENTINEL_LEGACY = """
        use std::time::Duration;

        /// Legacy: the index of `x` in `v`, or -1. Leave it as it is.
        fn legacy_find(v: &[i32], x: i32) -> i32 {
            v.iter().position(|&y| y == x).map_or(-1, |i| i as i32)
        }

        /// Legacy: the byte offset of the last `c` in `s`, or `usize::MAX` (C++'s `npos`). Leave it as it is.
        fn legacy_rfind(s: &str, c: char) -> usize {
            s.rfind(c).unwrap_or(usize::MAX)
        }
"""

SENTINEL_SOL = SENTINEL_LEGACY + """
        /// The index of `x` in `v`, or `None`.
        pub fn find(v: &[i32], x: i32) -> Option<usize> {
            usize::try_from(legacy_find(v, x)).ok()
        }

        /// The byte offset of the last `c` in `s`, or `None`.
        pub fn rfind(s: &str, c: char) -> Option<usize> {
            Some(legacy_rfind(s, c)).filter(|&i| i != usize::MAX)
        }

        /// The `ms` argument for the legacy `wait(ms)`: -1 waits forever, 0 polls, n > 0 waits up to n ms.
        pub fn timeout_ms(t: Option<Duration>) -> i64 {
            t.map_or(-1, |d| i64::try_from(d.as_nanos().div_ceil(1_000_000)).unwrap_or(i64::MAX))
        }
"""

P.append(dict(
    slug="sentinel-to-option", title="Sentinels at the API boundary", level="easy", stage="use-it", tags=["TryFrom", "Option", "sentinels"],
    teaches=[
        "Turn `-1` and `npos`-style sentinels into `Option` where the legacy API meets yours, and back again on the way out.",
        "`usize::try_from(i32)` rejects the sentinel for you; `as usize` turns -1 into 18446744073709551615.",
        "Going the other way, a real value must never collide with a sentinel: round and clamp so `Some(d)` can't become -1 or 0.",
    ],
    statement="""
        Wrap two legacy searches and one legacy argument (don't rewrite the searches):

        - `find` returns the index `legacy_find` reports, or `None` for its `-1`.
        - `rfind` returns the offset `legacy_rfind` reports, or `None` for its `usize::MAX`.
        - `timeout_ms` converts a timeout for a legacy `wait(ms: i64)`, where `-1` waits forever, `0` returns at once,
          and `n > 0` waits up to `n` ms. `None` means wait forever. A `Some` timeout must never become `-1` or `0`
          unless it is zero: round up to whole milliseconds, and clamp to `i64::MAX`.
    """,
    examples=[("find([4, 8, 15], 8)", "Some(1)"), ("rfind(\"a/b/c\", '/')", "Some(3)"), ("timeout_ms(Some(500µs))", "1: a zero would poll in a loop")],
    starter=SENTINEL_LEGACY + """
        /// The index of `x` in `v`, or `None`.
        pub fn find(v: &[i32], x: i32) -> Option<usize> {
            todo!()
        }

        /// The byte offset of the last `c` in `s`, or `None`.
        pub fn rfind(s: &str, c: char) -> Option<usize> {
            todo!()
        }

        /// The `ms` argument for the legacy `wait(ms)`: -1 waits forever, 0 polls, n > 0 waits up to n ms.
        pub fn timeout_ms(t: Option<Duration>) -> i64 {
            todo!()
        }
    """,
    solution=SENTINEL_SOL,
    visible=[
        "use std::time::Duration;",
        T("find_found_and_missing", "find([4, 8, 15], 8), find([4, 8, 15], 16)", "(find(&[4, 8, 15], 8), find(&[4, 8, 15], 16))", "(Some(1), None)"),
        T("index_zero_is_found", "find([7, 3], 7), rfind(\"/x\", '/')", "(find(&[7, 3], 7), rfind(\"/x\", '/'))", "(Some(0), Some(0))"),
        T("rfind_last_match", "rfind(\"a/b/c\", '/'), rfind(\"abc\", '/')", "(rfind(\"a/b/c\", '/'), rfind(\"abc\", '/'))", "(Some(3), None)"),
        T("timeouts", "None, Some(0), Some(250 ms)", "(timeout_ms(None), timeout_ms(Some(Duration::ZERO)), timeout_ms(Some(Duration::from_millis(250))))", "(-1, 0, 250)"),
        T("sub_millisecond_rounds_up", "Some(500 µs)", "timeout_ms(Some(Duration::from_micros(500)))", "1"),
    ],
    hidden=[
        "use std::time::Duration;",
        T("find_empty", "find([], 0)", "find(&[], 0)", "None"),
        T("searching_for_minus_one", "find([5, -1], -1), find([0, 1], -1)", "(find(&[5, -1], -1), find(&[0, 1], -1))", "(Some(1), None)"),
        T("find_duplicates_give_the_first", "find([2, 5, 5, 5], 5)", "find(&[2, 5, 5, 5], 5)", "Some(1)"),
        T("find_extremes", "find([i32::MIN, i32::MAX], i32::MAX)", "find(&[i32::MIN, i32::MAX], i32::MAX)", "Some(1)"),
        T("rfind_is_a_byte_offset", "rfind(\"héllo wörld\", 'ö')", "rfind(\"héllo wörld\", 'ö')", "Some(8)"),
        T("rfind_empty_string", "rfind(\"\", 'a')", "rfind(\"\", 'a')", "None"),
        T("rfind_multibyte_char", "rfind(\"🦀x🦀\", '🦀')", "rfind(\"🦀x🦀\", '🦀')", "Some(5)"),
        T("one_nanosecond", "Some(1 ns)", "timeout_ms(Some(Duration::from_nanos(1)))", "1"),
        T("rounds_up_not_to_nearest", "Some(1.000001 ms), Some(1.999 ms), Some(2 ms)", "(timeout_ms(Some(Duration::from_nanos(1_000_001))), timeout_ms(Some(Duration::from_micros(1999))), timeout_ms(Some(Duration::from_millis(2))))", "(2, 2, 2)"),
        T("long_timeouts_clamp", "Some(Duration::MAX), Some(u64::MAX seconds)", "(timeout_ms(Some(Duration::MAX)), timeout_ms(Some(Duration::from_secs(u64::MAX))))", "(i64::MAX, i64::MAX)"),
        T("largest_exact_timeout", "Some(i64::MAX ms)", "timeout_ms(Some(Duration::from_millis(i64::MAX as u64)))", "i64::MAX"),
        T("just_past_i64_max_ms", "Some(i64::MAX ms + 1 ns)", "timeout_ms(Some(Duration::from_millis(i64::MAX as u64) + Duration::from_nanos(1)))", "i64::MAX"),
        T("a_day", "Some(1 day)", "timeout_ms(Some(Duration::from_secs(86_400)))", "86_400_000"),
        T("large_index", "v = 0..10⁶, x = 999999", "find(&v, 999_999)", "Some(999_999)", setup="let v: Vec<i32> = (0..1_000_000).collect();"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(7103);
            for _ in 0..400 {
                let n = rng.below(8);
                let v: Vec<i32> = rng.vec(n, -3, 3);
                let x = rng.int(-4, 4) as i32;
                check!(format!("find(v = {v:?}, x = {x})"), find(&v, x), v.iter().position(|&y| y == x));
                let len = rng.below(8);
                let s = rng.string(len, "ab/é");
                let c = *rng.pick(&['/', 'é', 'z']);
                let want = s.char_indices().filter(|&(_, ch)| ch == c).map(|(i, _)| i).last();
                check!(format!("rfind(s = {s:?}, c = {c:?})"), rfind(&s, c), want);
                let t = match rng.below(4) {
                    0 => None,
                    1 => Some(Duration::from_nanos(rng.int(0, 3_000_000) as u64)),
                    2 => Some(Duration::new(rng.int(0, i64::MAX) as u64, rng.int(0, 999_999_999) as u32)),
                    _ => Some(Duration::new(u64::MAX - rng.below(3) as u64, rng.int(0, 999_999_999) as u32)),
                };
                let want = match t {
                    None => -1,
                    Some(d) => {
                        let ms = (d.as_nanos() + 999_999) / 1_000_000;
                        if ms > i64::MAX as u128 { i64::MAX } else { ms as i64 }
                    }
                };
                check!(format!("timeout_ms({t:?})"), timeout_ms(t), want);
            }
        }
        """,
    ],
    wrong=dict(
        cast_the_sentinel=SENTINEL_SOL.replace("usize::try_from(legacy_find(v, x)).ok()", "Some(legacy_find(v, x) as usize)"),
        positive_means_found=SENTINEL_SOL.replace("usize::try_from(legacy_find(v, x)).ok()", "let i = legacy_find(v, x);\n            (i > 0).then_some(i as usize)"),
        truncates_to_whole_ms=SENTINEL_SOL.replace("d.as_nanos().div_ceil(1_000_000)", "d.as_millis()"),
        casts_the_millis=SENTINEL_SOL.replace("i64::try_from(d.as_nanos().div_ceil(1_000_000)).unwrap_or(i64::MAX)", "d.as_nanos().div_ceil(1_000_000) as i64"),
    ),
    hints=[("approach", "Each wrapper is one conversion at the boundary. On the way out, ask what each legacy value means before you pick one."),
           ("rust", "`usize::try_from(i).ok()`, `Some(i).filter(|&i| i != usize::MAX)`, `opt.map_or(-1, |d| ...)`, `u128::div_ceil`, `i64::try_from(x).unwrap_or(i64::MAX)`."),
           ("edge case", "`Duration::as_millis` truncates, so 500 µs becomes 0, which means \"poll\". And `as i64` on a huge `u128` wraps, possibly to -1.")],
    notes=("""Sentinels are in-band signals: one value of the type is stolen to mean \"nothing\". The wrapper's job is to take it out at the boundary, and on the way back in to make sure no real value lands on a stolen one. `timeout_ms` has two collisions: truncating a sub-millisecond timeout gives 0 (a busy poll, a real bug mio and std guard against by rounding up), and a wrapping cast of a huge one can give -1 (wait forever) or any negative. Syntax to remember: `usize::try_from(x).ok()`, `opt.filter(|v| ..)`, `cond.then_some(v)`, `opt.map_or(default, f)`, `n.div_ceil(d)`, `Duration::from_micros` / `as_nanos() -> u128`.""", "O(n) for the searches, O(1) for the timeout", "O(1)"),
    follow_up="Why does `Option<usize>` cost 16 bytes while `Option<NonZeroUsize>` costs 8, and when would you store the sentinel form anyway?",
    related=["S8", "Y3", "S10"],
))

P.append(dict(
    slug="order-total", title="Sum into Option: an order total", level="easy", stage="use-it", tags=["try_fold", "zip", "checked_mul"],
    teaches=["`try_fold` (or `sum::<Option<_>>()`) stops at the first `None`.", "`ok()` + `zip` + `checked_*` keep every failure an `Option` instead of a panic."],
    statement="""
        Each line is `(price_cents, qty)` as text. Return the sum of `price * qty` over all lines, or `None` if
        any field isn't a `u64` or any product or the running total doesn't fit in a `u64`.
    """,
    examples=[("lines = [(\"250\", \"4\"), (\"100\", \"1\")]", "Some(1100)"), ("lines = [(\"5\", \"2\"), (\"oops\", \"1\")]", "None")],
    constraints=["Fields are parsed exactly as `str::parse::<u64>` does: no trimming."],
    starter="""
        pub fn order_total(lines: &[(&str, &str)]) -> Option<u64> {
            todo!()
        }
    """,
    solution="""
        pub fn order_total(lines: &[(&str, &str)]) -> Option<u64> {
            lines.iter().try_fold(0u64, |total, &(price, qty)| {
                let (p, q) = price.parse::<u64>().ok().zip(qty.parse::<u64>().ok())?;
                total.checked_add(p.checked_mul(q)?)
            })
        }
    """,
    visible=[
        T("two_lines", "lines = [(\"250\", \"4\"), (\"100\", \"1\")]", 'order_total(&[("250", "4"), ("100", "1")])', "Some(1100)"),
        T("empty_order", "lines = []", "order_total(&[])", "Some(0)"),
        T("bad_qty", "lines = [(\"250\", \"x\")]", 'order_total(&[("250", "x")])', "None"),
        T("one_bad_line_spoils_the_total", "lines = [(\"5\", \"2\"), (\"oops\", \"1\")]", 'order_total(&[("5", "2"), ("oops", "1")])', "None"),
        T("product_overflow_is_none", "lines = [(\"4294967296\", \"4294967296\")]", 'order_total(&[("4294967296", "4294967296")])', "None"),
    ],
    hidden=[
        T("zero_qty", "lines = [(\"999\", \"0\"), (\"1\", \"1\")]", 'order_total(&[("999", "0"), ("1", "1")])', "Some(1)"),
        T("product_exactly_max", "lines = [(\"4294967295\", \"4294967297\")]", 'order_total(&[("4294967295", "4294967297")])', "Some(u64::MAX)"),
        T("sum_exactly_max", "lines = [(\"18446744073709551614\", \"1\"), (\"1\", \"1\")]", 'order_total(&[("18446744073709551614", "1"), ("1", "1")])', "Some(u64::MAX)"),
        T("sum_overflow_is_none", "lines = [(\"18446744073709551615\", \"1\"), (\"1\", \"1\")]", 'order_total(&[("18446744073709551615", "1"), ("1", "1")])', "None"),
        T("field_too_big", "lines = [(\"18446744073709551616\", \"1\")]", 'order_total(&[("18446744073709551616", "1")])', "None"),
        T("negative_price", "lines = [(\"-1\", \"1\")]", 'order_total(&[("-1", "1")])', "None"),
        T("empty_field", "lines = [(\"\", \"1\")]", 'order_total(&[("", "1")])', "None"),
        T("plus_sign_parses", "lines = [(\"+5\", \"2\")]", 'order_total(&[("+5", "2")])', "Some(10)"),
        T("spaces_do_not_parse", "lines = [(\"5\", \" 2\")]", 'order_total(&[("5", " 2")])', "None"),
        T("full_width_digits", "lines = [(\"５\", \"2\")]", 'order_total(&[("５", "2")])', "None"),
        T("bad_line_first", "lines = [(\"x\", \"1\"), (\"5\", \"2\")]", 'order_total(&[("x", "1"), ("5", "2")])', "None"),
        """
        fn brute(s: &str) -> Option<u128> {
            let digits = s.strip_prefix('+').unwrap_or(s);
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let n = digits.bytes().fold(0u128, |n, b| (n * 10 + u128::from(b - b'0')).min(1 << 70));
            (n <= u128::from(u64::MAX)).then_some(n)
        }

        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(1311);
            let field = |rng: &mut anneal_prelude::Rng| match rng.below(6) {
                0 => {
                    let len = rng.below(4);
                    rng.string(len, "012+x")
                }
                1 => rng.int(0, i64::MAX).to_string(),
                2 => (u64::MAX - rng.below(3) as u64).to_string(),
                _ => rng.int(0, 1000).to_string(),
            };
            for _ in 0..400 {
                let n = rng.below(5);
                let mut owned: Vec<(String, String)> = Vec::new();
                for _ in 0..n {
                    let p = field(&mut rng);
                    let q = field(&mut rng);
                    owned.push((p, q));
                }
                let lines: Vec<(&str, &str)> = owned.iter().map(|(p, q)| (p.as_str(), q.as_str())).collect();
                let mut want = Some(0u128);
                for (p, q) in &lines {
                    want = match (want, brute(p), brute(q)) {
                        (Some(t), Some(p), Some(q)) if p * q <= u128::from(u64::MAX) && t + p * q <= u128::from(u64::MAX) => Some(t + p * q),
                        _ => None,
                    };
                }
                check!(format!("lines = {lines:?}"), order_total(&lines), want.map(|t| t as u64));
            }
        }

        #[test]
        fn scale_many_lines() {
            let owned: Vec<(String, String)> = (0..200_000u64).map(|i| (i.to_string(), "2".to_string())).collect();
            let lines: Vec<(&str, &str)> = owned.iter().map(|(p, q)| (p.as_str(), q.as_str())).collect();
            check!("200000 lines (i, 2) for i below 200000", order_total(&lines), Some(39_999_800_000u64));
        }
        """,
    ],
    wrong=dict(
        sum_panics_on_overflow="""
            pub fn order_total(lines: &[(&str, &str)]) -> Option<u64> {
                lines
                    .iter()
                    .map(|&(price, qty)| {
                        let (p, q) = price.parse::<u64>().ok().zip(qty.parse::<u64>().ok())?;
                        p.checked_mul(q)
                    })
                    .sum()
            }
        """,
        skips_bad_lines="""
            pub fn order_total(lines: &[(&str, &str)]) -> Option<u64> {
                lines
                    .iter()
                    .filter_map(|&(price, qty)| price.parse::<u64>().ok().zip(qty.parse::<u64>().ok()))
                    .try_fold(0u64, |total, (p, q)| total.checked_add(p.checked_mul(q)?))
            }
        """,
        saturates="""
            pub fn order_total(lines: &[(&str, &str)]) -> Option<u64> {
                lines.iter().try_fold(0u64, |total, &(price, qty)| {
                    let (p, q) = price.parse::<u64>().ok().zip(qty.parse::<u64>().ok())?;
                    Some(total.saturating_add(p.saturating_mul(q)))
                })
            }
        """,
    ),
    hints=[("approach", "One bad line makes the whole answer `None`, so the loop has to stop early with `None`."),
           ("rust", "`iter.try_fold(0u64, |acc, x| -> Option<u64> { ... })` stops at the first `None`, and `?` works inside the closure. `a.ok().zip(b.ok())?` gets both fields or bails."),
           ("edge case", "`sum::<Option<u64>>()` short-circuits too, but its `+` panics on overflow in debug builds. Use `checked_mul` and `checked_add`.")],
    notes=("`try_fold` threads the running total and stops at the first `None`, whether it came from a parse, a product or the sum. API reminder: `Option<T>` and `Result<T, E>` implement `Sum` and `Product` (`iter.sum::<Option<u64>>()`), `x.ok()` drops an error, `a.zip(b)` is `Some((x, y))` only when both are `Some`, and `checked_add`/`checked_mul` return `Option`.", "O(total length of the fields)", "O(1)"),
    follow_up="How would you report which line failed and why, instead of just `None`?",
    related=["S4", "L8"],
))

THEME_GUARD = """
        /// The chosen colour, else the palette's first colour, else "black".
        pub fn theme<'a>(chosen: Option<&'a str>, palette: &[&'a str]) -> &'a str {
            if chosen.is_none() && palette.is_empty() {
                return "black";
            }
            chosen.unwrap_or(palette[0])
        }

        /// The byte length of the colour `theme` would pick, but 0 instead of "black".
        pub fn theme_len(chosen: Option<&str>, palette: &[&str]) -> usize {
            if chosen.is_none() && palette.is_empty() {
                return 0;
            }
            chosen.map_or(palette[0].len(), str::len)
        }
"""

P.append(dict(
    slug="fix-eager-unwrap-or", title="Fix: unwrap_or and map_or panic on a Some", mode="fix", level="easy", stage="use-it", tags=["unwrap_or_else", "map_or_else", "lazy evaluation"],
    teaches=["Arguments to `unwrap_or` and `map_or` are evaluated before the call, even when the `Option` is `Some`.", "The `_else` variants take closures and run them only on `None`; `map_or_else` takes the default closure first."],
    statement="""
        Both functions panic with *index out of bounds* when a colour was chosen but the palette is empty,
        although neither needs the palette then. Fix them.
    """,
    examples=[("theme(Some(\"red\"), [])", "\"red\""), ("theme_len(Some(\"red\"), [])", "3"), ("theme_len(None, [\"blue\"])", "4")],
    starter=THEME_GUARD,
    solution=THEME_GUARD.replace("chosen.unwrap_or(palette[0])", "chosen.unwrap_or_else(|| palette[0])").replace("chosen.map_or(palette[0].len(), str::len)", "chosen.map_or_else(|| palette[0].len(), str::len)"),
    rules=dict(lines=10),
    visible=[
        T("chosen_wins", "chosen = Some(\"red\"), palette = [\"blue\", \"green\"]", '(theme(Some("red"), &["blue", "green"]), theme_len(Some("red"), &["blue", "green"]))', '("red", 3)'),
        T("first_of_palette", "chosen = None, palette = [\"blue\", \"green\"]", '(theme(None, &["blue", "green"]), theme_len(None, &["blue", "green"]))', '("blue", 4)'),
        T("nothing_at_all", "chosen = None, palette = []", "(theme(None, &[]), theme_len(None, &[]))", '("black", 0)'),
        T("theme_with_empty_palette", "chosen = Some(\"red\"), palette = []", 'theme(Some("red"), &[])', '"red"'),
        T("theme_len_with_empty_palette", "chosen = Some(\"red\"), palette = []", 'theme_len(Some("red"), &[])', "3"),
    ],
    hidden=[
        T("empty_choice_is_still_a_choice", "chosen = Some(\"\"), palette = [\"blue\"]", '(theme(Some(""), &["blue"]), theme_len(Some(""), &["blue"]))', '("", 0)'),
        T("single_colour", "chosen = None, palette = [\"teal\"]", '(theme(None, &["teal"]), theme_len(None, &["teal"]))', '("teal", 4)'),
        T("chosen_black", "chosen = Some(\"black\"), palette = []", '(theme(Some("black"), &[]), theme_len(Some("black"), &[]))', '("black", 5)'),
        T("chosen_in_palette", "chosen = Some(\"green\"), palette = [\"blue\", \"green\"]", 'theme(Some("green"), &["blue", "green"])', '"green"'),
        T("empty_first_colour", "chosen = None, palette = [\"\", \"blue\"]", '(theme(None, &["", "blue"]), theme_len(None, &["", "blue"]))', '("", 0)'),
        T("unicode_choice_empty_palette", "chosen = Some(\"青\"), palette = []", '(theme(Some("青"), &[]), theme_len(Some("青"), &[]))', '("青", 3)'),
        T("unicode_palette_length_in_bytes", "chosen = None, palette = [\"rosé\", \"青\"]", '(theme(None, &["rosé", "青"]), theme_len(None, &["rosé", "青"]))', '("rosé", 5)'),
        T("empty_choice_empty_palette", "chosen = Some(\"\"), palette = []", '(theme(Some(""), &[]), theme_len(Some(""), &[]))', '("", 0)'),
        T("returns_the_chosen_str", "chosen = Some(c), palette = []", "std::ptr::eq(theme(Some(c), &[]).as_ptr(), c.as_ptr())", "true", setup='let c = "crimson";'),
        T("long_palette", "chosen = None, palette = 100000 colours", "(theme(None, &palette), theme_len(None, &palette))", '("c0", 2)', setup='let names: Vec<String> = (0..100_000).map(|i| format!("c{i}")).collect();\nlet palette: Vec<&str> = names.iter().map(|s| s.as_str()).collect();'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(1312);
            let colours = ["red", "blue", "", "black", "青"];
            for _ in 0..300 {
                let chosen = if rng.bool() { Some(*rng.pick(&colours)) } else { None };
                let n = rng.below(4);
                let mut palette: Vec<&str> = Vec::new();
                for _ in 0..n {
                    palette.push(*rng.pick(&colours));
                }
                let want = match (chosen, palette.first()) {
                    (Some(c), _) => Some(c),
                    (None, Some(&p)) => Some(p),
                    (None, None) => None,
                };
                let desc = format!("chosen = {chosen:?}, palette = {palette:?}");
                check!(format!("theme, {desc}"), theme(chosen, &palette), want.unwrap_or("black"));
                check!(format!("theme_len, {desc}"), theme_len(chosen, &palette), want.map_or(0, str::len));
            }
        }
        """,
    ],
    wrong=dict(
        empty_palette_ignores_the_choice=THEME_GUARD.replace("if chosen.is_none() && palette.is_empty() {", "if palette.is_empty() {"),
        palette_wins=THEME_GUARD.replace("chosen.unwrap_or(palette[0])", "palette.first().copied().or(chosen).unwrap_or(\"black\")").replace("chosen.map_or(palette[0].len(), str::len)", "palette.first().or(chosen.as_ref()).map_or(0, |s| s.len())"),
        only_theme_fixed=THEME_GUARD.replace("chosen.unwrap_or(palette[0])", "chosen.unwrap_or_else(|| palette[0])"),
    ),
    hints=[("approach", "The panic comes from `palette[0]`, which runs before `unwrap_or` or `map_or` even look at `chosen`."),
           ("rust", "`unwrap_or_else(|| ...)` and `map_or_else(|| default, f)` only run the default closure on `None`. Note the order: default first, then `f`.")],
    notes=("Arguments are evaluated before a call, so `unwrap_or(palette[0])` indexes even when it isn't needed. API reminder: `unwrap_or` / `unwrap_or_else(|| ..)` / `unwrap_or_default()`, `map_or(default, f)` / `map_or_else(|| default, f)`, `or(opt)` / `or_else(|| opt)`. Clippy's `or_fun_call` lint flags the eager forms when the default is a call. The idiomatic `theme` is `chosen.or_else(|| palette.first().copied()).unwrap_or(\"black\")`.", "O(1)", "O(1)"),
    follow_up="When is the eager `unwrap_or` the better choice, and why does `map_or_else` put the default closure first?",
    related=["S4"],
))

USER_TYPE = """
        pub struct User {
            pub name: String,
            pub nickname: Option<String>,
            pub emails: Option<Vec<String>>,
        }
"""

USER_SOL = USER_TYPE + """
        /// The nickname if there is one, else the name.
        pub fn display_name(user: &User) -> &str {
            user.nickname.as_deref().unwrap_or(&user.name)
        }

        /// The first email, if the user has any.
        pub fn primary_email(user: &User) -> Option<&str> {
            user.emails.as_deref()?.first().map(String::as_str)
        }

        /// The name, then the nickname if any, then every email.
        pub fn handles(user: &User) -> Vec<&str> {
            std::iter::once(user.name.as_str())
                .chain(user.nickname.as_deref())
                .chain(user.emails.iter().flatten().map(String::as_str))
                .collect()
        }

        /// Uppercases the nickname (ASCII only) in place.
        pub fn shout_nickname(user: &mut User) {
            if let Some(n) = user.nickname.as_deref_mut() {
                n.make_ascii_uppercase();
            }
        }

        /// Appends `email`, creating the list if there is none.
        pub fn add_email(user: &mut User, email: String) {
            user.emails.get_or_insert_with(Vec::new).push(email);
        }
"""

USER_TESTS = """
        fn user(name: &str, nickname: Option<&str>, emails: Option<&[&str]>) -> User {
            User {
                name: name.to_string(),
                nickname: nickname.map(|n| n.to_string()),
                emails: emails.map(|es| es.iter().map(|e| e.to_string()).collect()),
            }
        }
"""

P.append(dict(
    slug="as-ref-as-mut-as-deref", title="as_ref, as_deref and friends", level="medium", stage="understand-it", tags=["as_deref", "as_deref_mut", "get_or_insert_with", "Option::iter"],
    teaches=[
        "`as_deref` turns `&Option<String>` into `Option<&str>` and `&Option<Vec<T>>` into `Option<&[T]>`, without moving or cloning.",
        "An `Option` is an iterator of zero or one items: `chain(opt)` and `opt.iter().flatten()` skip the `None` case.",
        "`get_or_insert_with` creates the value on first use and hands back `&mut` to it.",
    ],
    statement="""
        `User` has a name, maybe a nickname, and maybe a list of emails (`Some(vec![])` is a user with no emails). Write,
        without cloning anything:

        - `display_name`: the nickname if there is one (even an empty one), else the name.
        - `primary_email`: the first email, or `None`.
        - `handles`: the name, then the nickname if any, then every email, in that order.
        - `shout_nickname`: uppercases the nickname in place (ASCII only).
        - `add_email`: appends an email, creating the list if it's `None`.
    """,
    examples=[("name \"Ada\", nickname \"ace\", emails [\"a@x\"]", "handles = [\"Ada\", \"ace\", \"a@x\"]"), ("emails = Some([])", "primary_email = None")],
    starter=USER_TYPE + """
        /// The nickname if there is one, else the name.
        pub fn display_name(user: &User) -> &str {
            todo!()
        }

        /// The first email, if the user has any.
        pub fn primary_email(user: &User) -> Option<&str> {
            todo!()
        }

        /// The name, then the nickname if any, then every email.
        pub fn handles(user: &User) -> Vec<&str> {
            todo!()
        }

        /// Uppercases the nickname (ASCII only) in place.
        pub fn shout_nickname(user: &mut User) {
            todo!()
        }

        /// Appends `email`, creating the list if there is none.
        pub fn add_email(user: &mut User, email: String) {
            todo!()
        }
    """,
    solution=USER_SOL,
    visible=[
        USER_TESTS,
        T("nickname_wins", "name \"Ada\", nickname \"ace\"", "(display_name(&u), display_name(&v))", '("ace", "Ada")', setup='let u = user("Ada", Some("ace"), None);\nlet v = user("Ada", None, None);'),
        T("handles_in_order", "name \"Ada\", nickname \"ace\", emails [\"a@x\", \"b@y\"]", "handles(&u)", 'vec!["Ada", "ace", "a@x", "b@y"]', setup='let u = user("Ada", Some("ace"), Some(&["a@x", "b@y"]));'),
        T("empty_email_list_has_no_primary", "emails = Some([]), then None, then [\"a@x\"]", "(primary_email(&a), primary_email(&b), primary_email(&c))", '(None, None, Some("a@x"))',
          setup='let a = user("A", None, Some(&[]));\nlet b = user("B", None, None);\nlet c = user("C", None, Some(&["a@x"]));'),
        T("add_email_creates_the_list", "emails = None, add \"a@x\", add \"b@y\"", "u.emails", 'Some(vec!["a@x".to_string(), "b@y".to_string()])',
          setup='let mut u = user("Ada", None, None);\nadd_email(&mut u, "a@x".to_string());\nadd_email(&mut u, "b@y".to_string());'),
        T("shout_leaves_the_name_alone", "name \"Ada\", nickname \"ace\"", "(u.nickname, u.name)", '(Some("ACE".to_string()), "Ada".to_string())', setup='let mut u = user("Ada", Some("ace"), None);\nshout_nickname(&mut u);'),
    ],
    hidden=[
        USER_TESTS,
        T("empty_nickname_still_wins", "name \"Ada\", nickname \"\"", "display_name(&u)", '""', setup='let u = user("Ada", Some(""), None);'),
        T("handles_without_extras", "name \"Ada\" only; then emails = Some([])", "(handles(&a), handles(&b))", '(vec!["Ada"], vec!["Ada"])', setup='let a = user("Ada", None, None);\nlet b = user("Ada", None, Some(&[]));'),
        T("handles_keep_an_empty_nickname", "name \"Ada\", nickname \"\", emails [\"a@x\"]", "handles(&u)", 'vec!["Ada", "", "a@x"]', setup='let u = user("Ada", Some(""), Some(&["a@x"]));'),
        T("add_email_appends_to_an_empty_list", "emails = Some([]), add \"a@x\"", "u.emails", 'Some(vec!["a@x".to_string()])', setup='let mut u = user("Ada", None, Some(&[]));\nadd_email(&mut u, "a@x".to_string());'),
        T("add_email_keeps_existing", "emails = [\"a@x\"], add \"b@y\"", "(primary_email(&u).map(str::to_string), u.emails.as_ref().map(Vec::len))", '(Some("a@x".to_string()), Some(2))', setup='let mut u = user("Ada", None, Some(&["a@x"]));\nadd_email(&mut u, "b@y".to_string());'),
        T("add_email_moves_the_string", "add an email and keep its buffer", "u.emails.as_ref().map(|es| es[0].as_ptr())", "Some(p)", setup='let mut u = user("Ada", None, None);\nlet e = String::from("a@x");\nlet p = e.as_ptr();\nadd_email(&mut u, e);'),
        T("shout_none", "no nickname", "(u.nickname, u.name)", '(None, "Ada".to_string())', setup='let mut u = user("Ada", None, None);\nshout_nickname(&mut u);'),
        T("shout_mixed_and_unicode", "nickname \"a1-bC é\"", "u.nickname", 'Some("A1-BC é".to_string())', setup='let mut u = user("x", Some("a1-bC é"), None);\nshout_nickname(&mut u);'),
        T("shout_twice", "nickname \"ace\", shouted twice", "u.nickname", 'Some("ACE".to_string())', setup='let mut u = user("x", Some("ace"), None);\nshout_nickname(&mut u);\nshout_nickname(&mut u);'),
        T("display_name_borrows", "nickname \"ace\", and none", "(std::ptr::eq(display_name(&u).as_ptr(), u.nickname.as_ref().unwrap().as_ptr()), std::ptr::eq(display_name(&v).as_ptr(), v.name.as_ptr()))", "(true, true)",
          setup='let u = user("Ada", Some("ace"), None);\nlet v = user("Ada", None, None);'),
        T("primary_email_borrows", "emails [\"a@x\"]", "std::ptr::eq(primary_email(&u).unwrap().as_ptr(), u.emails.as_ref().unwrap()[0].as_ptr())", "true", setup='let u = user("Ada", None, Some(&["a@x"]));'),
        T("handles_borrow", "name \"Ada\", nickname \"ace\", emails [\"a@x\"]", "hs.iter().zip([u.name.as_ptr(), u.nickname.as_ref().unwrap().as_ptr(), u.emails.as_ref().unwrap()[0].as_ptr()]).all(|(h, p)| std::ptr::eq(h.as_ptr(), p))", "true",
          setup='let u = user("Ada", Some("ace"), Some(&["a@x"]));\nlet hs = handles(&u);'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(7104);
            for _ in 0..300 {
                let len = rng.below(4);
                let name = rng.string(len, "abZ ");
                let nickname = if rng.bool() {
                    let len = rng.below(4);
                    Some(rng.string(len, "xyQ!é"))
                } else {
                    None
                };
                let emails: Option<Vec<String>> = if rng.bool() {
                    let n = rng.below(3);
                    Some((0..n).map(|i| format!("e{i}@x")).collect())
                } else {
                    None
                };
                let mut u = User { name: name.clone(), nickname: nickname.clone(), emails: emails.clone() };
                let desc = format!("name = {name:?}, nickname = {nickname:?}, emails = {emails:?}");
                let mut want: Vec<String> = vec![name.clone()];
                if let Some(n) = &nickname {
                    want.push(n.clone());
                }
                if let Some(es) = &emails {
                    want.extend(es.iter().cloned());
                }
                check!(format!("display_name, {desc}"), display_name(&u).to_string(), nickname.clone().unwrap_or(name.clone()));
                check!(format!("primary_email, {desc}"), primary_email(&u).map(str::to_string), emails.as_ref().and_then(|es| es.first().cloned()));
                check!(format!("handles, {desc}"), handles(&u).iter().map(|h| h.to_string()).collect::<Vec<_>>(), want);
                shout_nickname(&mut u);
                add_email(&mut u, "new@x".to_string());
                let mut es = emails.clone().unwrap_or_default();
                es.push("new@x".to_string());
                check!(format!("shout + add_email, {desc}"), (u.nickname, u.emails, u.name), (nickname.map(|n| n.to_ascii_uppercase()), Some(es), name));
            }
        }

        #[test]
        fn scale_many_emails() {
            let mut u = user("Ada", Some("ace"), None);
            for i in 0..200_000 {
                add_email(&mut u, format!("{i}@x"));
            }
            let hs = handles(&u);
            check!("200000 add_email calls, then handles", (hs.len(), hs[2], hs[200_001], primary_email(&u)), (200_002, "0@x", "199999@x", Some("0@x")));
        }
        """,
    ],
    wrong=dict(
        empty_nickname_falls_back=USER_SOL.replace("user.nickname.as_deref().unwrap_or(&user.name)", "user.nickname.as_deref().filter(|n| !n.is_empty()).unwrap_or(&user.name)"),
        indexes_the_first_email=USER_SOL.replace("user.emails.as_deref()?.first().map(String::as_str)", "user.emails.as_ref().map(|es| es[0].as_str())"),
        add_email_replaces_the_list=USER_SOL.replace("user.emails.get_or_insert_with(Vec::new).push(email);", "user.emails = Some(vec![email]);"),
        clones_the_list=USER_SOL.replace("user.emails.get_or_insert_with(Vec::new).push(email);", "let mut es = user.emails.clone().unwrap_or_default();\n            es.push(email);\n            user.emails = Some(es);"),
        emails_before_nickname=USER_SOL.replace(".chain(user.nickname.as_deref())\n                .chain(user.emails.iter().flatten().map(String::as_str))", ".chain(user.emails.iter().flatten().map(String::as_str))\n                .chain(user.nickname.as_deref())"),
    ),
    hints=[("approach", "Every function borrows through the `Option` rather than taking the value out of it. Only `add_email` ever creates anything."),
           ("rust", "`as_deref()` on `Option<Vec<String>>` gives `Option<&[String]>`, so `?` then `.first()` works. An `Option` implements `IntoIterator`, so `iter.chain(opt)` and `opt.iter().flatten()` just work."),
           ("edge case", "`Some(vec![])` has no first email: don't index. `add_email` must keep what's already there.")],
    notes=("""`user.nickname.map(..)` would move the `String` out of a borrowed struct; the `as_*` family converts `&Option<T>` into `Option<&T>` (and `as_deref` goes one step further, through `Deref`), after which every by-value combinator is fine. Treating `Option` as a 0-or-1 item iterator removes the `if let` from `handles`. Syntax to remember: `as_ref()` → `Option<&T>`, `as_mut()` → `Option<&mut T>`, `as_deref()` → `Option<&T::Target>` (`&str`, `&[T]`), `as_deref_mut()` → `Option<&mut str>`, `opt.iter()` / `iter_mut()`, `get_or_insert_with(Vec::new)` → `&mut Vec<_>`.""", "O(1) for all but handles, which is O(number of emails)", "O(number of emails) for handles"),
    follow_up="Why does `Option<Vec<String>>` often turn into a plain `Vec<String>` in a good API, and when is the `None`/empty distinction worth keeping?",
    related=["S7", "L2", "S6"],
))

PROFILE_HEAD = """
        use std::collections::HashMap;

        pub struct Profile {
            pub name: String,
            nickname: Option<String>,
            pub labels: HashMap<u32, String>,
        }
"""

PROFILE_STARTER = PROFILE_HEAD + """
        impl Profile {
            pub fn new(name: &str, nickname: Option<&str>) -> Profile {
                Profile { name: String::from(name), nickname: nickname.map(String::from), labels: HashMap::new() }
            }

            /// The nickname, if any.
            pub fn nickname(&self) -> &Option<String> {
                &self.nickname
            }

            /// The label for `id`, if any.
            pub fn label(&self, id: u32) -> Option<&String> {
                self.labels.get(&id)
            }
        }

        /// "Hello, <nickname>!" if there is a nickname, else "Hello, <name>!".
        pub fn greeting(name: &String, nickname: &Option<String>) -> String {
            format!("Hello, {}!", nickname.as_ref().unwrap_or(name))
        }

        /// The label for `id`, or `default`.
        pub fn label_or<'a>(p: &'a Profile, id: u32, default: &'a String) -> &'a String {
            p.label(id).unwrap_or(default)
        }
"""

PROFILE_SOL = PROFILE_HEAD + """
        impl Profile {
            pub fn new(name: &str, nickname: Option<&str>) -> Profile {
                Profile { name: String::from(name), nickname: nickname.map(String::from), labels: HashMap::new() }
            }

            /// The nickname, if any.
            pub fn nickname(&self) -> Option<&str> {
                self.nickname.as_deref()
            }

            /// The label for `id`, if any.
            pub fn label(&self, id: u32) -> Option<&str> {
                self.labels.get(&id).map(String::as_str)
            }
        }

        /// "Hello, <nickname>!" if there is a nickname, else "Hello, <name>!".
        pub fn greeting(name: &str, nickname: Option<&str>) -> String {
            format!("Hello, {}!", nickname.unwrap_or(name))
        }

        /// The label for `id`, or `default`.
        pub fn label_or<'a>(p: &'a Profile, id: u32, default: &'a str) -> &'a str {
            p.label(id).unwrap_or(default)
        }
"""

PROFILE_TESTS = """
        fn profile(name: &str, nickname: Option<&str>, labels: &[(u32, &str)]) -> Profile {
            let mut p = Profile::new(name, nickname);
            for &(id, l) in labels {
                p.labels.insert(id, l.to_string());
            }
            p
        }
"""

P.append(dict(
    slug="option-ref-to-str", title="Fix: &Option<String> in an API", mode="fix", level="medium", stage="understand-it", tags=["Option<&str>", "as_deref", "API design"],
    teaches=[
        "Take and return `Option<&str>`, not `&Option<String>` or `Option<&String>`: the first accepts every caller, the others demand an owned `String` somewhere.",
        "`as_deref()` and `map(String::as_str)` convert at the one place that owns the data.",
        "A returned `&str` borrows from whichever input it came from, so both inputs share the lifetime.",
    ],
    statement="""
        The tests don't compile. They call this API the way real callers do: with string literals, with `Option<&str>`
        from other code, and comparing results against `Some("...")`. Every signature here demands an owned `String`
        (or a reference to one) that those callers don't have.

        Fix the four signatures, and their bodies, so every test compiles and passes. Don't clone or allocate
        anywhere except the `format!` that builds the greeting.
    """,
    examples=[("greeting(\"Ada\", Some(\"ace\"))", "\"Hello, ace!\""), ("label_or(&p, 7, \"?\")", "\"?\"")],
    starter=PROFILE_STARTER,
    solution=PROFILE_SOL,
    rules=dict(methods=["clone", "cloned", "to_owned", "to_string", "into"], lines=12),
    visible=[
        PROFILE_TESTS,
        T("greeting_with_literals", "greeting(\"Ada\", Some(\"ace\")), greeting(\"Ada\", None)", '(greeting("Ada", Some("ace")), greeting("Ada", None))', '("Hello, ace!".to_string(), "Hello, Ada!".to_string())'),
        T("greeting_from_a_profile", "profile Ada / ace", "greeting(&p.name, p.nickname())", '"Hello, ace!".to_string()', setup='let p = profile("Ada", Some("ace"), &[]);'),
        T("nickname_compares_to_a_literal", "profile Ada / ace, and Ada / none", '(p.nickname() == Some("ace"), q.nickname())', "(true, None)", setup='let p = profile("Ada", Some("ace"), &[]);\nlet q = profile("Ada", None, &[]);'),
        T("label_lookup", "labels {1: \"one\"}, label(1), label(2)", "(p.label(1), p.label(2))", '(Some("one"), None)', setup='let p = profile("Ada", None, &[(1, "one")]);'),
        T("label_or_with_a_literal_default", "labels {1: \"one\"}, label_or(1, \"?\"), label_or(7, \"?\")", '(label_or(&p, 1, "?"), label_or(&p, 7, "?"))', '("one", "?")', setup='let p = profile("Ada", None, &[(1, "one")]);'),
    ],
    hidden=[
        PROFILE_TESTS,
        T("empty_nickname_is_still_a_nickname", "profile Ada / \"\"", "greeting(&p.name, p.nickname())", '"Hello, !".to_string()', setup='let p = profile("Ada", Some(""), &[]);'),
        T("empty_label_is_still_a_label", "labels {3: \"\"}", '(p.label(3), label_or(&p, 3, "?"))', '(Some(""), "")', setup='let p = profile("Ada", None, &[(3, "")]);'),
        T("id_extremes", "labels {0: \"zero\", u32::MAX: \"max\"}", '(label_or(&p, 0, "?"), label_or(&p, u32::MAX, "?"), p.label(1))', '("zero", "max", None)', setup='let p = profile("Ada", None, &[(0, "zero"), (u32::MAX, "max")]);'),
        T("unicode", "profile \"Zoë\" / \"🦀\", label {5: \"café ☕\"}", '(greeting(&p.name, p.nickname()), p.label(5))', '("Hello, 🦀!".to_string(), Some("café ☕"))', setup='let p = profile("Zoë", Some("🦀"), &[(5, "café ☕")]);'),
        T("greeting_with_an_owned_option", "nickname: Option<String> held by the caller", "greeting(\"Ada\", nick.as_deref())", '"Hello, bo!".to_string()', setup='let nick: Option<String> = Some("bo".into());'),
        T("no_nickname_uses_the_name", "profile Zed / none", "greeting(&p.name, p.nickname())", '"Hello, Zed!".to_string()', setup='let p = profile("Zed", None, &[]);'),
        T("label_or_borrows_from_the_map", "labels {1: \"one\"}", 'std::ptr::eq(label_or(&p, 1, "?").as_ptr(), p.labels[&1].as_ptr())', "true", setup='let p = profile("Ada", None, &[(1, "one")]);'),
        T("label_or_returns_the_default_itself", "labels {}, default d", "std::ptr::eq(label_or(&p, 1, d).as_ptr(), d.as_ptr())", "true", setup='let p = profile("Ada", None, &[]);\nlet d = "fallback";'),
        T("default_from_a_short_lived_string", "default is a local String", "label_or(&p, 9, &local).len()", "5", setup='let p = profile("Ada", None, &[]);\nlet local = String::from("local");'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(7105);
            for _ in 0..300 {
                let n = rng.below(5);
                let mut pairs: Vec<(u32, String)> = Vec::new();
                for _ in 0..n {
                    let k = rng.below(6) as u32;
                    let len = rng.below(3);
                    let v = rng.string(len, "ab");
                    pairs.retain(|(pk, _)| *pk != k);
                    pairs.push((k, v));
                }
                let refs: Vec<(u32, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
                let nick = if rng.bool() { Some(*rng.pick(&["", "x", "ß"])) } else { None };
                let p = profile("N", nick, &refs);
                let id = rng.below(6) as u32;
                let want = refs.iter().find(|(k, _)| *k == id).map(|(_, v)| *v);
                let desc = format!("nickname = {nick:?}, labels = {refs:?}, id = {id}");
                check!(format!("label, {desc}"), p.label(id), want);
                check!(format!("label_or, {desc}"), label_or(&p, id, "-"), want.unwrap_or("-"));
                check!(format!("greeting, {desc}"), greeting(&p.name, p.nickname()), format!("Hello, {}!", nick.unwrap_or("N")));
            }
        }

        #[test]
        fn scale_many_lookups() {
            let mut p = profile("Ada", None, &[]);
            for i in 0..200_000u32 {
                p.labels.insert(i * 2, "x".into());
            }
            let found = (0..400_000u32).filter(|&id| label_or(&p, id, "").len() == 1).count();
            check!("200000 labels (even ids), look up every id below 400000", found, 200_000);
        }
        """,
    ],
    wrong=dict(
        empty_nickname_falls_back=PROFILE_SOL.replace("nickname.unwrap_or(name)", "nickname.filter(|n| !n.is_empty()).unwrap_or(name)"),
        empty_label_is_missing=PROFILE_SOL.replace("self.labels.get(&id).map(String::as_str)", "self.labels.get(&id).map(String::as_str).filter(|l| !l.is_empty())"),
    ),
    hints=[("approach", "Read the call sites in the tests: what types do they pass, and what do they compare the results with?"),
           ("rust", "Parameters: `&str` and `Option<&str>`. Returns: `Option<&str>` and `&'a str`. Inside, `self.nickname.as_deref()` and `self.labels.get(&id).map(String::as_str)`."),
           ("edge case", "`nickname.unwrap_or(name)` needs both sides to be `&str`; with `name: &str` it just works.")],
    notes=("""`&Option<String>` says \"I need a reference to an `Option` that owns a `String`\", so a caller holding a `&str` or an `Option<&str>` must allocate one just to call you. `Option<&str>` says \"maybe a string slice\", which every caller can produce for free: `Some(\"lit\")`, `opt.as_deref()`, `s.get(..)`. The same goes for returns: a getter returning `&Option<String>` leaks the field's type into every caller. It's also smaller: `Option<&str>` is two words with the null pointer as `None`. Clippy flags these as `ref_option` and `ptr_arg`. Syntax to remember: `self.field.as_deref()` (`&Option<String>` → `Option<&str>`), `map.get(k).map(String::as_str)`, `fn f<'a>(a: &'a X, d: &'a str) -> &'a str`.""", "O(1) per call", "O(1)"),
    follow_up="When is `&Option<T>` the right parameter type after all? What changes if `T` is `Copy`?",
    related=["L3", "S4", "L2"],
))

TRANSPOSE_SOL = """
        use std::num::ParseIntError;

        /// No input is `Ok(None)`; a number is `Ok(Some(n))`; anything else is an error.
        pub fn parse_optional(s: Option<&str>) -> Result<Option<i32>, ParseIntError> {
            s.map(str::parse).transpose()
        }

        /// After trimming: a blank line or a `#` comment is `Ok(None)`; anything else must be a number.
        pub fn parse_line(line: &str) -> Result<Option<i64>, ParseIntError> {
            let t = line.trim();
            (!t.is_empty() && !t.starts_with('#')).then(|| t.parse()).transpose()
        }

        /// Every number in `text`, in order, or the 1-based line number and error of the first bad line.
        pub fn parse_file(text: &str) -> Result<Vec<i64>, (usize, ParseIntError)> {
            text.lines()
                .enumerate()
                .filter_map(|(i, line)| parse_line(line).map_err(|e| (i + 1, e)).transpose())
                .collect()
        }
"""

P.append(dict(
    slug="transpose", title="transpose: skip blanks, keep errors", level="medium", stage="understand-it", tags=["transpose", "filter_map", "ParseIntError"],
    teaches=[
        "`transpose` swaps `Option<Result<T, E>>` and `Result<Option<T>, E>`, so `?` or `collect` can see the error.",
        "`filter_map(|x| f(x).transpose())` drops the `Ok(None)` items and keeps both values and errors for `collect::<Result<Vec<_>, _>>()`.",
        "`.ok().flatten()` compiles too, and silently throws the errors away.",
    ],
    statement="""
        A data file has one integer per line; blank lines and lines starting with `#` (after trimming) are ignored.
        Write:

        - `parse_optional`: `None` is `Ok(None)`, a number is `Ok(Some(n))`, anything else is the parse error.
        - `parse_line`: `Ok(None)` for a blank or comment line, `Ok(Some(n))` for a number (trimmed), else the error.
        - `parse_file`: every number in order, or `Err((line, error))` for the first bad line, numbered from 1.
    """,
    examples=[("parse_file(\"1\\n# two\\n\\n3\")", "Ok([1, 3])"), ("parse_file(\"1\\nx\\ny\")", "Err((2, <invalid digit>))")],
    starter="""
        use std::num::ParseIntError;

        /// No input is `Ok(None)`; a number is `Ok(Some(n))`; anything else is an error.
        pub fn parse_optional(s: Option<&str>) -> Result<Option<i32>, ParseIntError> {
            todo!()
        }

        /// After trimming: a blank line or a `#` comment is `Ok(None)`; anything else must be a number.
        pub fn parse_line(line: &str) -> Result<Option<i64>, ParseIntError> {
            todo!()
        }

        /// Every number in `text`, in order, or the 1-based line number and error of the first bad line.
        pub fn parse_file(text: &str) -> Result<Vec<i64>, (usize, ParseIntError)> {
            todo!()
        }
    """,
    solution=TRANSPOSE_SOL,
    visible=[
        """
        fn err<T>(s: &str) -> Result<T, std::num::ParseIntError> {
            Err(s.parse::<i64>().unwrap_err())
        }
        """,
        T("optional", "parse_optional(None), (Some(\"42\")), (Some(\"x\"))", 'parse_optional(None).unwrap() == None && parse_optional(Some("42")) == Ok(Some(42)) && parse_optional(Some("x")).is_err()', "true"),
        T("lines", "parse_line(\" 7 \"), (\"\"), (\"# note\"), (\"7x\")", '(parse_line(" 7 "), parse_line(""), parse_line("# note"), parse_line("7x"))', '(Ok(Some(7)), Ok(None), Ok(None), err("7x"))'),
        T("file_skips_blanks_and_comments", "\"1\\n# two\\n\\n3\"", 'parse_file("1\\n# two\\n\\n3")', "Ok(vec![1, 3])"),
        T("file_reports_the_first_bad_line", "\"1\\nx\\ny\"", 'parse_file("1\\nx\\ny")', 'err("x").map_err(|e| (2, e))'),
        T("empty_file", "\"\"", 'parse_file("")', "Ok(vec![])"),
    ],
    hidden=[
        """
        fn err<T>(s: &str) -> Result<T, std::num::ParseIntError> {
            Err(s.parse::<i64>().unwrap_err())
        }
        """,
        T("empty_string_is_an_error_not_none", "parse_optional(Some(\"\"))", 'parse_optional(Some("")).map_err(|e| e.kind().clone())', "Err(std::num::IntErrorKind::Empty)"),
        T("optional_bounds", "Some(\"-2147483648\"), Some(\"2147483648\")", '(parse_optional(Some("-2147483648")), parse_optional(Some("2147483648")).map_err(|e| e.kind().clone()))', "(Ok(Some(i32::MIN)), Err(std::num::IntErrorKind::PosOverflow))"),
        T("optional_is_not_trimmed", "Some(\" 5\")", 'parse_optional(Some(" 5")).is_err()', "true"),
        T("indented_comment", "\"  # x\" and \"\\t\"", '(parse_line("  # x"), parse_line("\\t"))', "(Ok(None), Ok(None))"),
        T("no_inline_comments", "\"5 # five\"", 'parse_line("5 # five")', 'err("5 # five")'),
        T("hash_inside_a_number", "\"-#1\"", 'parse_line("-#1")', 'err("-#1")'),
        T("crlf_line_endings", "\"1\\r\\n2\\r\\n\"", 'parse_file("1\\r\\n2\\r\\n")', "Ok(vec![1, 2])"),
        T("only_comments", "\"# a\\n\\n  # b\"", 'parse_file("# a\\n\\n  # b")', "Ok(vec![])"),
        T("line_numbers_count_skipped_lines", "\"# header\\n\\n5\\nfive\"", 'parse_file("# header\\n\\n5\\nfive")', 'err("five").map_err(|e| (4, e))'),
        T("first_line_bad", "\"x\\n1\"", 'parse_file("x\\n1")', 'err("x").map_err(|e| (1, e))'),
        T("i64_bounds", "\"-9223372036854775808\\n9223372036854775807\"", 'parse_file("-9223372036854775808\\n9223372036854775807")', "Ok(vec![i64::MIN, i64::MAX])"),
        T("overflow_is_an_error", "\"1\\n9223372036854775808\"", 'parse_file("1\\n9223372036854775808").map_err(|(n, e)| (n, e.kind().clone()))', "Err((2, std::num::IntErrorKind::PosOverflow))"),
        T("unicode_digits", "\"٣\"", 'parse_file("٣")', 'err("٣").map_err(|e| (1, e))'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(7106);
            let pieces = ["1", "-4", "20", "", "  ", "# c", " #", "x", "3 ", " 9", "+2", "#1", "1#"];
            for _ in 0..400 {
                let n = rng.below(6);
                let mut lines: Vec<&str> = Vec::new();
                for _ in 0..n {
                    lines.push(*rng.pick(&pieces));
                }
                let text = lines.join("\\n");
                let mut want: Result<Vec<i64>, (usize, std::num::ParseIntError)> = Ok(Vec::new());
                for (i, l) in text.lines().enumerate() {
                    let t = l.trim();
                    if t.is_empty() || t.starts_with('#') {
                        continue;
                    }
                    match t.parse::<i64>() {
                        Ok(v) => want.as_mut().unwrap().push(v),
                        Err(e) => {
                            want = Err((i + 1, e));
                            break;
                        }
                    }
                }
                check!(format!("text = {text:?}"), parse_file(&text), want);
                let s = if rng.bool() { Some(*rng.pick(&pieces)) } else { None };
                check!(format!("parse_optional({s:?})"), parse_optional(s), s.map(|t| t.parse::<i32>()).transpose());
            }
        }

        #[test]
        fn scale_bad_line_at_the_end() {
            let mut text: String = (0..200_000).map(|i| if i % 3 == 0 { "# c\\n".to_string() } else { format!("{i}\\n") }).collect();
            let want: Vec<i64> = (0..200_000).filter(|i| i % 3 != 0).collect();
            check!("200000 lines, every third a comment", parse_file(&text), Ok(want));
            text.push_str("oops\\n");
            check!("the same, then \\"oops\\" on line 200001", parse_file(&text).map_err(|(n, _)| n), Err(200_001));
        }
        """,
    ],
    wrong=dict(
        skips_bad_lines=TRANSPOSE_SOL.replace(".filter_map(|(i, line)| parse_line(line).map_err(|e| (i + 1, e)).transpose())\n                .collect()", ".filter_map(|(_, line)| parse_line(line).ok().flatten())\n                .map(Ok)\n                .collect()"),
        zero_based_line_numbers=TRANSPOSE_SOL.replace("(i + 1, e)", "(i, e)"),
        comment_check_before_trim=TRANSPOSE_SOL.replace("(!t.is_empty() && !t.starts_with('#'))", "(!t.is_empty() && !line.starts_with('#'))"),
        empty_is_none=TRANSPOSE_SOL.replace("s.map(str::parse).transpose()", "s.filter(|t| !t.is_empty()).map(str::parse).transpose()"),
    ),
    hints=[("approach", "Write `parse_line` first; `parse_file` is then one iterator chain over `lines().enumerate()`."),
           ("rust", "`cond.then(|| t.parse())` is `Option<Result<..>>`; `.transpose()` makes it `Result<Option<..>>`. In `parse_file`, `filter_map` wants an `Option`, so transpose back and collect into `Result<Vec<_>, _>`."),
           ("edge case", "Line numbers count every line, including the skipped ones. `\"  # x\"` is a comment once trimmed.")],
    notes=("""`Result<Option<T>, E>` (\"it worked, and maybe there was something\") and `Option<Result<T, E>>` (\"maybe there was something, and it may have failed\") carry the same information; `transpose` converts between them so the next step can use its natural tool: `?` wants the `Result` outside, `filter_map` wants the `Option` outside. `collect::<Result<Vec<_>, _>>()` then stops at the first `Err`. Syntax to remember: `opt.map(str::parse).transpose()`, `cond.then(|| expr)` (lazy) vs `cond.then_some(v)` (eager), `iter.filter_map(|x| f(x).transpose()).collect::<Result<Vec<_>, _>>()`, `err.kind()` → `&IntErrorKind`.""", "O(n)", "O(number of values)"),
    follow_up="How would you collect every bad line instead of stopping at the first? Where else does `Option<Result<..>>` appear in std (hint: `Iterator::next` on a fallible reader)?",
    related=["L8", "S6", "S9"],
))

STORE = """
        use std::collections::HashMap;

        #[derive(Debug, PartialEq, Eq)]
        pub enum StoreError {
            Corrupt(String),
            Missing(String),
        }

        pub struct Store {
            data: HashMap<String, String>,
        }

        impl Store {
            pub fn new(pairs: &[(&str, &str)]) -> Store {
                Store { data: pairs.iter().map(|&(k, v)| (k.to_string(), v.to_string())).collect() }
            }

            /// `Ok(None)` if `key` is absent, `Err(Corrupt(key))` if its value isn't an integer.
            pub fn get(&self, key: &str) -> Result<Option<i64>, StoreError> {
                match self.data.get(key) {
                    None => Ok(None),
                    Some(v) => v.parse().map(Some).map_err(|_| StoreError::Corrupt(key.to_string())),
                }
            }
        }
"""

P.append(dict(
    slug="result-of-option", title="Result<Option<T>> and ?", level="medium", stage="understand-it", tags=["?", "Option", "err"],
    teaches=["`?` on a `Result<Option<T>, E>` leaves the `Option` for you to handle.", "`.ok()` would silently turn an error into \"missing\"; `.err()` keeps only the error."],
    statement="""
        `Store::get` returns `Ok(None)` for a missing key and `Err(Corrupt(key))` for a value that isn't an integer.
        Write `require`, which reports a missing key as `Err(Missing(key))`, and `sum_or_zero`, which adds up
        `keys` and counts missing ones as 0. Both pass the first `Corrupt` error straight through.
        Then write `first_corrupt`, which returns the error of the first corrupt key in `keys`, or `None`.
    """,
    examples=[("store = {a: \"5\", b: \"x\"}, require(\"c\")", "Err(Missing(\"c\"))"), ("store = {a: \"5\", b: \"x\"}, sum_or_zero([\"a\", \"c\"])", "Ok(5)")],
    constraints=["Values fit in an `i64`, and so does every sum."],
    starter=STORE + """
        pub fn require(store: &Store, key: &str) -> Result<i64, StoreError> {
            todo!()
        }

        pub fn sum_or_zero(store: &Store, keys: &[&str]) -> Result<i64, StoreError> {
            todo!()
        }

        pub fn first_corrupt(store: &Store, keys: &[&str]) -> Option<StoreError> {
            todo!()
        }
    """,
    solution=STORE + """
        pub fn require(store: &Store, key: &str) -> Result<i64, StoreError> {
            store.get(key)?.ok_or_else(|| StoreError::Missing(key.to_string()))
        }

        pub fn sum_or_zero(store: &Store, keys: &[&str]) -> Result<i64, StoreError> {
            let mut total = 0;
            for key in keys {
                total += store.get(key)?.unwrap_or(0);
            }
            Ok(total)
        }

        pub fn first_corrupt(store: &Store, keys: &[&str]) -> Option<StoreError> {
            keys.iter().find_map(|k| store.get(k).err())
        }
    """,
    visible=[
        T("require_present", "store = {a: \"5\"}, require(\"a\")", 'require(&Store::new(&[("a", "5")]), "a")', "Ok(5)"),
        T("require_missing", "store = {a: \"5\"}, require(\"b\")", 'require(&Store::new(&[("a", "5")]), "b")', 'Err(StoreError::Missing("b".to_string()))'),
        T("require_corrupt", "store = {a: \"x\"}, require(\"a\")", 'require(&Store::new(&[("a", "x")]), "a")', 'Err(StoreError::Corrupt("a".to_string()))'),
        T("sum_counts_missing_as_zero", "store = {a: \"5\", b: \"-2\"}, sum_or_zero([\"a\", \"b\", \"c\"])", 'sum_or_zero(&Store::new(&[("a", "5"), ("b", "-2")]), &["a", "b", "c"])', "Ok(3)"),
        T("sum_stops_at_corrupt", "store = {a: \"1\", b: \"x\"}, sum_or_zero([\"a\", \"b\"])", 'sum_or_zero(&Store::new(&[("a", "1"), ("b", "x")]), &["a", "b"])', 'Err(StoreError::Corrupt("b".to_string()))'),
    ],
    hidden=[
        T("first_corrupt_none", "store = {a: \"1\", b: \"x\"}, first_corrupt([\"a\", \"c\"])", 'first_corrupt(&Store::new(&[("a", "1"), ("b", "x")]), &["a", "c"])', "None"),
        T("first_corrupt_first_wins", "store = {a: \"x\", b: \"y\"}, first_corrupt([\"c\", \"b\", \"a\"])", 'first_corrupt(&Store::new(&[("a", "x"), ("b", "y")]), &["c", "b", "a"])', 'Some(StoreError::Corrupt("b".to_string()))'),
        T("first_corrupt_no_keys", "store = {a: \"x\"}, first_corrupt([])", 'first_corrupt(&Store::new(&[("a", "x")]), &[])', "None"),
        T("sum_no_keys", "store = {a: \"x\"}, sum_or_zero([])", 'sum_or_zero(&Store::new(&[("a", "x")]), &[])', "Ok(0)"),
        T("sum_all_missing", "store = {}, sum_or_zero([\"a\", \"b\"])", 'sum_or_zero(&Store::new(&[]), &["a", "b"])', "Ok(0)"),
        T("sum_repeated_key", "store = {a: \"5\"}, sum_or_zero([\"a\", \"a\"])", 'sum_or_zero(&Store::new(&[("a", "5")]), &["a", "a"])', "Ok(10)"),
        T("sum_first_corrupt_wins", "store = {a: \"x\", b: \"y\"}, sum_or_zero([\"c\", \"b\", \"a\"])", 'sum_or_zero(&Store::new(&[("a", "x"), ("b", "y")]), &["c", "b", "a"])', 'Err(StoreError::Corrupt("b".to_string()))'),
        T("sum_corrupt_key_not_asked", "store = {a: \"2\", b: \"x\"}, sum_or_zero([\"a\"])", 'sum_or_zero(&Store::new(&[("a", "2"), ("b", "x")]), &["a"])', "Ok(2)"),
        T("require_empty_value", "store = {a: \"\"}, require(\"a\")", 'require(&Store::new(&[("a", "")]), "a")', 'Err(StoreError::Corrupt("a".to_string()))'),
        T("require_zero_is_present", "store = {a: \"0\"}, require(\"a\")", 'require(&Store::new(&[("a", "0")]), "a")', "Ok(0)"),
        T("require_i64_bounds", "store = {lo: \"-9223372036854775808\", hi: \"9223372036854775807\"}", '(require(&s, "lo"), require(&s, "hi"))', "(Ok(i64::MIN), Ok(i64::MAX))", setup='let s = Store::new(&[("lo", "-9223372036854775808"), ("hi", "9223372036854775807")]);'),
        T("require_empty_key", "store = {a: \"1\"}, require(\"\")", 'require(&Store::new(&[("a", "1")]), "")', 'Err(StoreError::Missing(String::new()))'),
        T("require_unicode_key", "store = {ключ: \"7\"}, require(\"ключ\"), require(\"клю\")", '(require(&s, "ключ"), require(&s, "клю"))', '(Ok(7), Err(StoreError::Missing("клю".to_string())))', setup='let s = Store::new(&[("ключ", "7")]);'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(1313);
            let names = ["a", "b", "c", "d"];
            let values = ["1", "-3", "10", "x", "", "7"];
            for _ in 0..400 {
                let mut pairs: Vec<(&str, &str)> = Vec::new();
                for &k in &names {
                    if rng.bool() {
                        pairs.push((k, *rng.pick(&values)));
                    }
                }
                let store = Store::new(&pairs);
                let n = rng.below(5);
                let mut keys: Vec<&str> = Vec::new();
                for _ in 0..n {
                    keys.push(*rng.pick(&names));
                }
                let lookup = |k: &str| pairs.iter().find(|(pk, _)| *pk == k).map(|(_, v)| *v);
                let key = *rng.pick(&names);
                let want_one = match lookup(key) {
                    None => Err(StoreError::Missing(key.to_string())),
                    Some(v) => v.parse::<i64>().map_err(|_| StoreError::Corrupt(key.to_string())),
                };
                check!(format!("store = {pairs:?}, require({key:?})"), require(&store, key), want_one);
                let mut want: Result<i64, StoreError> = Ok(0);
                for &k in &keys {
                    match lookup(k).map(|v| v.parse::<i64>()) {
                        None => {}
                        Some(Ok(v)) => {
                            if let Ok(t) = &mut want {
                                *t += v;
                            }
                        }
                        Some(Err(_)) => {
                            want = Err(StoreError::Corrupt(k.to_string()));
                            break;
                        }
                    }
                }
                check!(format!("store = {pairs:?}, sum_or_zero({keys:?})"), sum_or_zero(&store, &keys), want);
                let want_first = keys.iter().find(|&&k| lookup(k).is_some_and(|v| v.parse::<i64>().is_err())).map(|k| StoreError::Corrupt(k.to_string()));
                check!(format!("store = {pairs:?}, first_corrupt({keys:?})"), first_corrupt(&store, &keys), want_first);
            }
        }
        """,
    ],
    wrong=dict(
        corrupt_reads_as_missing=STORE + """
        pub fn require(store: &Store, key: &str) -> Result<i64, StoreError> {
            store.get(key).ok().flatten().ok_or_else(|| StoreError::Missing(key.to_string()))
        }

        pub fn sum_or_zero(store: &Store, keys: &[&str]) -> Result<i64, StoreError> {
            let mut total = 0;
            for key in keys {
                total += store.get(key)?.unwrap_or(0);
            }
            Ok(total)
        }

        pub fn first_corrupt(store: &Store, keys: &[&str]) -> Option<StoreError> {
            keys.iter().find_map(|k| store.get(k).err())
        }
        """,
        sum_skips_corrupt=STORE + """
        pub fn require(store: &Store, key: &str) -> Result<i64, StoreError> {
            store.get(key)?.ok_or_else(|| StoreError::Missing(key.to_string()))
        }

        pub fn sum_or_zero(store: &Store, keys: &[&str]) -> Result<i64, StoreError> {
            Ok(keys.iter().map(|k| store.get(k).ok().flatten().unwrap_or(0)).sum())
        }

        pub fn first_corrupt(store: &Store, keys: &[&str]) -> Option<StoreError> {
            keys.iter().find_map(|k| store.get(k).err())
        }
        """,
        require_defaults_to_zero=STORE + """
        pub fn require(store: &Store, key: &str) -> Result<i64, StoreError> {
            Ok(store.get(key)?.unwrap_or(0))
        }

        pub fn sum_or_zero(store: &Store, keys: &[&str]) -> Result<i64, StoreError> {
            let mut total = 0;
            for key in keys {
                total += store.get(key)?.unwrap_or(0);
            }
            Ok(total)
        }

        pub fn first_corrupt(store: &Store, keys: &[&str]) -> Option<StoreError> {
            keys.iter().find_map(|k| store.get(k).err())
        }
        """,
        first_corrupt_reports_the_last=STORE + """
        pub fn require(store: &Store, key: &str) -> Result<i64, StoreError> {
            store.get(key)?.ok_or_else(|| StoreError::Missing(key.to_string()))
        }

        pub fn sum_or_zero(store: &Store, keys: &[&str]) -> Result<i64, StoreError> {
            let mut total = 0;
            for key in keys {
                total += store.get(key)?.unwrap_or(0);
            }
            Ok(total)
        }

        pub fn first_corrupt(store: &Store, keys: &[&str]) -> Option<StoreError> {
            keys.iter().filter_map(|k| store.get(k).err()).last()
        }
        """,
    ),
    hints=[("rust", "`store.get(key)?` has type `Option<i64>`: the `?` deals with the error layer, then you deal with the option layer."),
           ("edge case", "`store.get(key).ok().flatten()` compiles, but it turns `Corrupt` into \"missing\". Keep the two layers apart.")],
    notes=("`Result<Option<T>, E>` means \"the lookup can fail, and a successful lookup can find nothing\". `?` peels the `Result`; `ok_or_else` or `unwrap_or` decides what absence means for each caller. API reminder: `r.ok()` is `Option<T>`, `r.err()` is `Option<E>`, `find_map` returns the first `Some`, and `transpose` flips `Result<Option<T>, E>` and `Option<Result<T, E>>`.", "O(k) for k keys", "O(1)"),
    follow_up="Why do database and cache clients return `Result<Option<T>, E>` rather than an error variant for \"not found\"?",
    related=["S6", "L8"],
))

COLLECT_SOL = """
        fn parse(s: &str) -> Result<i32, String> {
            s.parse().map_err(|_| format!("bad number: {s}"))
        }

        /// Every item as an `i32`, or the error for the first bad one.
        pub fn parse_all(items: &[&str]) -> Result<Vec<i32>, String> {
            items.iter().map(|s| parse(s)).collect()
        }

        /// The sum of every item as an `i64`, or the error for the first bad one. No intermediate `Vec`.
        pub fn sum_all(items: &[&str]) -> Result<i64, String> {
            items.iter().map(|s| parse(s).map(i64::from)).sum()
        }

        /// Every item, or every error in order.
        pub fn parse_every_error(items: &[&str]) -> Result<Vec<i32>, Vec<String>> {
            let (good, bad): (Vec<_>, Vec<_>) = items.iter().map(|s| parse(s)).partition(Result::is_ok);
            if bad.is_empty() {
                Ok(good.into_iter().flatten().collect())
            } else {
                Err(bad.into_iter().filter_map(Result::err).collect())
            }
        }

        /// Runs `check` on each item in order and stops at the first error.
        pub fn validate(items: &[&str], mut check: impl FnMut(&str) -> Result<(), String>) -> Result<(), String> {
            items.iter().map(|s| check(s)).collect()
        }
"""

P.append(dict(
    slug="collect-into-result", title="Collect into Result: first error or every error", level="medium", stage="understand-it", tags=["collect", "FromIterator", "Sum", "partition"],
    teaches=[
        "`Result<C, E>` implements `FromIterator<Result<T, E>>` for any collection `C`, including `()`; it stops at the first `Err`.",
        "`Result` also implements `Sum` and `Product`, so `sum::<Result<i64, _>>()` short-circuits without a `Vec`.",
        "Collecting every error is a different shape: `partition(Result::is_ok)` and then unwrap each side.",
    ],
    statement="""
        An item is good if it parses as an `i32`; a bad item's error is `"bad number: <item>"`. Write:

        - `parse_all`: every value, or the first error.
        - `sum_all`: the sum as an `i64`, or the first error, without building a `Vec`.
        - `parse_every_error`: every value, or **every** error, in order.
        - `validate`: calls `check` on each item in order and returns its first error. Items after that error must
          not be checked.
    """,
    examples=[("parse_all([\"1\", \"x\", \"y\"])", "Err(\"bad number: x\")"), ("parse_every_error([\"1\", \"x\", \"y\"])", "Err([\"bad number: x\", \"bad number: y\"])")],
    starter="""
        /// Every item as an `i32`, or the error for the first bad one.
        pub fn parse_all(items: &[&str]) -> Result<Vec<i32>, String> {
            todo!()
        }

        /// The sum of every item as an `i64`, or the error for the first bad one. No intermediate `Vec`.
        pub fn sum_all(items: &[&str]) -> Result<i64, String> {
            todo!()
        }

        /// Every item, or every error in order.
        pub fn parse_every_error(items: &[&str]) -> Result<Vec<i32>, Vec<String>> {
            todo!()
        }

        /// Runs `check` on each item in order and stops at the first error.
        pub fn validate(items: &[&str], mut check: impl FnMut(&str) -> Result<(), String>) -> Result<(), String> {
            todo!()
        }
    """,
    solution=COLLECT_SOL,
    visible=[
        T("parse_all_good_and_bad", "[\"1\", \"2\", \"3\"] and [\"1\", \"x\", \"y\"]", '(parse_all(&["1", "2", "3"]), parse_all(&["1", "x", "y"]))', '(Ok(vec![1, 2, 3]), Err("bad number: x".to_string()))'),
        T("sum_does_not_overflow_i32", "[\"2147483647\", \"2147483647\"]", 'sum_all(&["2147483647", "2147483647"])', "Ok(4_294_967_294)"),
        T("every_error", "[\"1\", \"x\", \"2\", \"y\"]", 'parse_every_error(&["1", "x", "2", "y"])', 'Err(vec!["bad number: x".to_string(), "bad number: y".to_string()])'),
        T("validate_stops_at_the_first_error", "items a, bad, c, bad2; check fails on \"bad…\"", "(r, seen)", '(Err("bad".to_string()), vec!["a".to_string(), "bad".to_string()])',
          setup='let mut seen = Vec::new();\nlet r = validate(&["a", "bad", "c", "bad2"], |s| {\n    seen.push(s.to_string());\n    if s.starts_with("bad") { Err(s.to_string()) } else { Ok(()) }\n});'),
        T("no_items", "[]", "(parse_all(&[]), sum_all(&[]), parse_every_error(&[]), validate(&[], |_| Err(\"never\".to_string())))", "(Ok(vec![]), Ok(0), Ok(vec![]), Ok(()))"),
    ],
    hidden=[
        T("empty_item_is_bad", "[\"1\", \"\"]", 'parse_all(&["1", ""])', 'Err("bad number: ".to_string())'),
        T("i32_bounds", "[\"-2147483648\", \"2147483647\"], and \"2147483648\"", '(parse_all(&["-2147483648", "2147483647"]), parse_all(&["2147483648"]))', '(Ok(vec![i32::MIN, i32::MAX]), Err("bad number: 2147483648".to_string()))'),
        T("sum_negative_bounds", "[\"-2147483648\", \"-2147483648\", \"5\"]", 'sum_all(&["-2147483648", "-2147483648", "5"])', "Ok(-4_294_967_291)"),
        T("sum_first_error", "[\"1\", \"x\", \"y\"]", 'sum_all(&["1", "x", "y"])', 'Err("bad number: x".to_string())'),
        T("every_error_all_good", "[\"3\", \"1\", \"3\"]", 'parse_every_error(&["3", "1", "3"])', "Ok(vec![3, 1, 3])"),
        T("every_error_keeps_duplicates", "[\"x\", \"x\", \" 1\"]", 'parse_every_error(&["x", "x", " 1"])', 'Err(vec!["bad number: x".to_string(), "bad number: x".to_string(), "bad number:  1".to_string()])'),
        T("validate_all_good_checks_everything", "[\"a\", \"b\", \"c\"]", "(r, calls)", "(Ok(()), 3)", setup='let mut calls = 0;\nlet r = validate(&["a", "b", "c"], |_| {\n    calls += 1;\n    Ok(())\n});'),
        T("validate_first_item_fails", "[\"x\", \"y\"], check always fails", "(r, calls)", '(Err("x!".to_string()), 1)', setup='let mut calls = 0;\nlet r = validate(&["x", "y"], |s| {\n    calls += 1;\n    Err(format!("{s}!"))\n});'),
        T("order_kept", "[\"3\", \"-1\", \"+2\"]", 'parse_all(&["3", "-1", "+2"])', "Ok(vec![3, -1, 2])"),
        T("unicode_item", "[\"7\", \"٣\"]", 'parse_every_error(&["7", "٣"])', 'Err(vec!["bad number: ٣".to_string()])'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(7107);
            for _ in 0..400 {
                let n = rng.below(6);
                let mut items: Vec<String> = Vec::new();
                for _ in 0..n {
                    if rng.below(4) == 0 {
                        let len = rng.below(3);
                        items.push(rng.string(len, "x1-"));
                    } else if rng.bool() {
                        items.push(rng.int(-99, 99).to_string());
                    } else {
                        items.push(rng.int(i64::from(i32::MAX) - 2, i64::from(i32::MAX) + 1).to_string());
                    }
                }
                let refs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
                let parsed: Vec<Result<i32, String>> = refs.iter().map(|s| s.parse::<i32>().map_err(|_| format!("bad number: {s}"))).collect();
                let first_err = parsed.iter().find_map(|r| r.clone().err());
                let errs: Vec<String> = parsed.iter().filter_map(|r| r.clone().err()).collect();
                let vals: Vec<i32> = parsed.iter().filter_map(|r| r.clone().ok()).collect();
                let desc = format!("items = {refs:?}");
                check!(format!("parse_all, {desc}"), parse_all(&refs), match &first_err { Some(e) => Err(e.clone()), None => Ok(vals.clone()) });
                check!(format!("sum_all, {desc}"), sum_all(&refs), match &first_err { Some(e) => Err(e.clone()), None => Ok(vals.iter().map(|&v| i64::from(v)).sum()) });
                check!(format!("parse_every_error, {desc}"), parse_every_error(&refs), if errs.is_empty() { Ok(vals.clone()) } else { Err(errs.clone()) });
                let mut calls = 0;
                let got = validate(&refs, |s| {
                    calls += 1;
                    s.parse::<i32>().map(|_| ()).map_err(|_| s.to_string())
                });
                let stop = parsed.iter().position(|r| r.is_err());
                check!(format!("validate, {desc}"), (got, calls), (stop.map_or(Ok(()), |i| Err(refs[i].to_string())), stop.map_or(refs.len(), |i| i + 1)));
            }
        }

        #[test]
        fn scale_bad_item_at_the_end() {
            let mut items: Vec<String> = (0..200_000).map(|i: i32| (i - 100_000).to_string()).collect();
            let refs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
            let want: Vec<i32> = (0..200_000).map(|i| i - 100_000).collect();
            check!("200000 numbers from -100000 up", (parse_all(&refs), sum_all(&refs), parse_every_error(&refs)), (Ok(want.clone()), Ok(-100_000), Ok(want)));
            items.push("oops".to_string());
            let refs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
            check!("the same 200000 numbers, then \\"oops\\"", (parse_all(&refs), parse_every_error(&refs)), (Err("bad number: oops".to_string()), Err(vec!["bad number: oops".to_string()])));
        }
        """,
    ],
    wrong=dict(
        sums_in_i32=COLLECT_SOL.replace("items.iter().map(|s| parse(s).map(i64::from)).sum()", "items.iter().map(|s| parse(s)).sum::<Result<i32, String>>().map(i64::from)"),
        validate_checks_everything=COLLECT_SOL.replace("items.iter().map(|s| check(s)).collect()\n", "let results: Vec<Result<(), String>> = items.iter().map(|s| check(s)).collect();\n            results.into_iter().collect()\n"),
        only_the_first_error=COLLECT_SOL.replace("Err(bad.into_iter().filter_map(Result::err).collect())", "Err(bad.into_iter().filter_map(Result::err).take(1).collect())"),
        reports_the_last_bad_item=COLLECT_SOL.replace("items.iter().map(|s| parse(s)).collect()\n", "let mut out = Vec::new();\n            let mut err = None;\n            for s in items {\n                match parse(s) {\n                    Ok(n) => out.push(n),\n                    Err(e) => err = Some(e),\n                }\n            }\n            err.map_or(Ok(out), Err)\n"),
    ),
    hints=[("approach", "Three of the four are one `collect` or `sum` with the right target type; only `parse_every_error` needs two passes."),
           ("rust", "`.collect::<Result<Vec<_>, _>>()`, `.sum::<Result<i64, _>>()`, `.collect::<Result<(), _>>()`. For every error: `let (good, bad): (Vec<_>, Vec<_>) = iter.partition(Result::is_ok);`."),
           ("edge case", "Two `i32::MAX` items overflow an `i32` sum: convert each value to `i64` before adding.")],
    notes=("""The short-circuiting impls are `impl<A, E, V: FromIterator<A>> FromIterator<Result<A, E>> for Result<V, E>` and `impl<T: Sum<U>, U, E> Sum<Result<U, E>> for Result<T, E>`: they pull items until the first `Err` and drop the rest of the iterator, so later items are never produced (which `validate`'s call count shows). `()` implements `FromIterator<()>`, which is why `collect::<Result<(), _>>()` works; `try_for_each` is the same thing spelled as a loop. Collecting all errors can't short-circuit by definition. Syntax to remember: `collect::<Result<Vec<_>, _>>()`, `sum::<Result<i64, _>>()`, `collect::<Option<Vec<_>>>()`, `partition(Result::is_ok)`, `results.into_iter().flatten()` (the `Ok` values), `filter_map(Result::err)`.""", "O(n)", "O(n) for the Vec results, O(1) for sum_all and validate"),
    follow_up="How would you return both the good values and the errors? When does collecting every error make an API worse?",
    related=["S6", "L8"],
))

WORKER_HEAD = """
        #[derive(Debug, Default)]
        pub struct Worker {
            pub current: Option<String>,
            pub done: Vec<String>,
        }
"""

P.append(dict(
    slug="fix-move-out-of-option", title="Fix: move out of an Option behind &mut", mode="fix", level="medium", stage="understand-it", tags=["take", "replace", "as_deref_mut"],
    teaches=["You can't move a value out of `&mut self`; you can swap something else in.", "`Option::take` leaves `None`; `Option::replace` leaves the new value. Both return the old one.", "`as_deref_mut` turns `&mut Option<String>` into `Option<&mut str>` without moving."],
    statement="""
        `start` begins a task and returns the one it interrupted, `finish` moves the current task to `done`, and
        `current_mut` lets the caller edit the current task in place. None of them compile:
        *cannot move out of `self.current`* (E0507). Fix them without cloning.
    """,
    examples=[("start(\"a\"), start(\"b\")", "second call returns Some(\"a\"); current = Some(\"b\")"), ("start(\"a\"), finish()", "true; current = None, done = [\"a\"]")],
    starter=WORKER_HEAD + """
        impl Worker {
            /// Starts `task` and returns the task it interrupted, if any.
            pub fn start(&mut self, task: String) -> Option<String> {
                let old = self.current;
                self.current = Some(task);
                old
            }

            /// Moves the current task to `done`. Returns false if there was none.
            pub fn finish(&mut self) -> bool {
                match self.current {
                    Some(task) => {
                        self.done.push(task);
                        true
                    }
                    None => false,
                }
            }

            /// The current task, for editing in place.
            pub fn current_mut(&mut self) -> Option<&mut str> {
                self.current.map(|mut s| s.as_mut_str())
            }
        }
    """,
    solution=WORKER_HEAD + """
        impl Worker {
            /// Starts `task` and returns the task it interrupted, if any.
            pub fn start(&mut self, task: String) -> Option<String> {
                self.current.replace(task)
            }

            /// Moves the current task to `done`. Returns false if there was none.
            pub fn finish(&mut self) -> bool {
                match self.current.take() {
                    Some(task) => {
                        self.done.push(task);
                        true
                    }
                    None => false,
                }
            }

            /// The current task, for editing in place.
            pub fn current_mut(&mut self) -> Option<&mut str> {
                self.current.as_deref_mut()
            }
        }
    """,
    rules=dict(methods=["clone", "cloned", "to_owned", "to_string"], lines=5),
    visible=[
        T("start_when_idle", "start(\"a\") on a new worker", '{ let mut w = Worker::default(); let r = w.start("a".into()); (r, w.current) }', '(None, Some("a".to_string()))'),
        T("start_interrupts", "start(\"a\"), start(\"b\")", '{ let mut w = Worker::default(); w.start("a".into()); let r = w.start("b".into()); (r, w.current) }', '(Some("a".to_string()), Some("b".to_string()))'),
        T("finish_moves_to_done", "start(\"a\"), finish()", '{ let mut w = Worker::default(); w.start("a".into()); let r = w.finish(); (r, w.current, w.done) }', '(true, None, vec!["a".to_string()])'),
        T("finish_when_idle", "finish() on a new worker", "{ let mut w = Worker::default(); let r = w.finish(); (r, w.current, w.done) }", "(false, None, Vec::<String>::new())"),
        T("finish_twice", "start(\"a\"), finish(), finish()", '{ let mut w = Worker::default(); w.start("a".into()); (w.finish(), w.finish(), w.done) }', '(true, false, vec!["a".to_string()])'),
    ],
    hidden=[
        T("current_mut_edits_in_place", "start(\"ab\"), uppercase through current_mut()", '{ let mut w = Worker::default(); w.start("ab".into()); if let Some(s) = w.current_mut() { s.make_ascii_uppercase(); } w.current }', 'Some("AB".to_string())'),
        T("current_mut_when_idle", "current_mut() on a new worker", "Worker::default().current_mut().is_none()", "true"),
        T("current_mut_then_finish", "start(\"a\"), uppercase through current_mut(), finish()", '{ let mut w = Worker::default(); w.start("a".into()); if let Some(s) = w.current_mut() { s.make_ascii_uppercase(); } w.finish(); (w.current, w.done) }', '(None, vec!["A".to_string()])'),
        T("interrupted_task_is_not_done", "start(\"a\"), start(\"b\"), finish()", '{ let mut w = Worker::default(); w.start("a".into()); w.start("b".into()); w.finish(); (w.current, w.done) }', '(None, vec!["b".to_string()])'),
        T("empty_task_name", "start(\"\"), finish(), finish()", '{ let mut w = Worker::default(); w.start(String::new()); (w.finish(), w.finish(), w.current, w.done) }', '(true, false, None, vec![String::new()])'),
        T("restart_after_finish", "start(\"a\"), finish(), start(\"b\")", '{ let mut w = Worker::default(); w.start("a".into()); w.finish(); let r = w.start("b".into()); (r, w.current, w.done) }', '(None, Some("b".to_string()), vec!["a".to_string()])'),
        T("done_keeps_order", "a, b, c each started and finished", '{ let mut w = Worker::default(); for t in ["a", "b", "c"] { w.start(t.into()); w.finish(); } w.done }', 'vec!["a".to_string(), "b".to_string(), "c".to_string()]'),
        T("same_task_twice", "start(\"a\"), start(\"a\")", '{ let mut w = Worker::default(); w.start("a".into()); let r = w.start("a".into()); (r, w.current) }', '(Some("a".to_string()), Some("a".to_string()))'),
        T("unicode_task", "start(\"修复 🦀\"), finish()", '{ let mut w = Worker::default(); w.start("修复 🦀".into()); (w.finish(), w.done) }', '(true, vec!["修复 🦀".to_string()])'),
        T("start_keeps_done", "done = [\"x\"], start(\"a\")", '{ let mut w = Worker { current: None, done: vec!["x".into()] }; w.start("a".into()); w.done }', 'vec!["x".to_string()]'),
        T("start_moves_not_copies", "start(t) returns the same buffer later", "{ let mut w = Worker::default(); w.start(t); w.start(\"b\".into()).map(|s| s.as_ptr()) }", "Some(p)", setup='let t = String::from("a");\nlet p = t.as_ptr();'),
        """
        #[test]
        fn random_vs_model() {
            let mut rng = anneal_prelude::Rng::new(1314);
            let names = ["a", "b", "", "ç"];
            for _ in 0..300 {
                let mut w = Worker::default();
                let mut cur: Option<String> = None;
                let mut done: Vec<String> = Vec::new();
                let mut log: Vec<String> = Vec::new();
                let n = rng.below(8);
                for _ in 0..n {
                    if rng.bool() {
                        let t = rng.pick(&names).to_string();
                        log.push(format!("start({t:?})"));
                        let want = std::mem::replace(&mut cur, Some(t.clone()));
                        check!(log.join(", "), w.start(t), want);
                    } else {
                        log.push("finish()".to_string());
                        let want = match std::mem::take(&mut cur) {
                            Some(t) => {
                                done.push(t);
                                true
                            }
                            None => false,
                        };
                        check!(log.join(", "), w.finish(), want);
                    }
                }
                check!(format!("state after {}", log.join(", ")), (w.current, w.done), (cur, done));
            }
        }

        #[test]
        fn many_tasks() {
            let mut w = Worker::default();
            for i in 0..100_000 {
                w.start(i.to_string());
                w.finish();
            }
            check!("100000 tasks started and finished", (w.current, w.done.len(), w.done[99_999].clone()), (None, 100_000, "99999".to_string()));
        }
        """,
    ],
    wrong=dict(
        finish_leaves_an_empty_string=WORKER_HEAD + """
        impl Worker {
            /// Starts `task` and returns the task it interrupted, if any.
            pub fn start(&mut self, task: String) -> Option<String> {
                self.current.replace(task)
            }

            /// Moves the current task to `done`. Returns false if there was none.
            pub fn finish(&mut self) -> bool {
                match self.current.as_mut() {
                    Some(task) => {
                        self.done.push(std::mem::take(task));
                        true
                    }
                    None => false,
                }
            }

            /// The current task, for editing in place.
            pub fn current_mut(&mut self) -> Option<&mut str> {
                self.current.as_deref_mut()
            }
        }
        """,
        start_ignored_when_busy=WORKER_HEAD + """
        impl Worker {
            /// Starts `task` and returns the task it interrupted, if any.
            pub fn start(&mut self, task: String) -> Option<String> {
                if self.current.is_some() {
                    return Some(task);
                }
                self.current = Some(task);
                None
            }

            /// Moves the current task to `done`. Returns false if there was none.
            pub fn finish(&mut self) -> bool {
                match self.current.take() {
                    Some(task) => {
                        self.done.push(task);
                        true
                    }
                    None => false,
                }
            }

            /// The current task, for editing in place.
            pub fn current_mut(&mut self) -> Option<&mut str> {
                self.current.as_deref_mut()
            }
        }
        """,
    ),
    hints=[("approach", "Moving out would leave `self.current` holding nothing, which Rust doesn't allow behind a reference. Put a value back in the same step."),
           ("rust", "`self.current.take()` returns the old `Option` and leaves `None`; `self.current.replace(x)` leaves `Some(x)`. To borrow instead of move, go through `as_mut()` or `as_deref_mut()`.")],
    notes=("`take` and `replace` are `mem::replace` specialised for `Option`: they move the old value out and put a valid one back, so `&mut self` is never left half-empty. `current_mut` never needed to move at all. API reminder: `take()`, `replace(v)`, `insert(v) -> &mut T`, `get_or_insert_with(f) -> &mut T`, `as_mut()`, `as_deref_mut()`.", "O(1)", "O(1)"),
    follow_up="How do `take` and `replace` relate to `std::mem::take` and `std::mem::replace`? When would you reach for `mem::swap`?",
    related=["L2", "L3"],
))

SIZE_HEAD = """
        use std::num::{ParseFloatError, ParseIntError};

        #[derive(Debug, PartialEq)]
        pub enum SizeError {
            Missing(&'static str),
            BadInt(ParseIntError),
            BadFloat(ParseFloatError),
        }
"""

SIZE_TAIL = """
        fn field<'a>(s: &'a str, name: &'static str) -> Result<&'a str, SizeError> {
            s.split(';')
                .find_map(|kv| kv.strip_prefix(name)?.strip_prefix('='))
                .ok_or(SizeError::Missing(name))
        }

        /// Parses "width=80;height=24;scale=1.5" into (80, 24, 1.5). The scale is optional and defaults to 1.0.
        pub fn size(s: &str) -> Result<(u32, u32, f64), SizeError> {
            let w = field(s, "width")?.parse::<u32>()?;
            let h = field(s, "height")?.parse::<u32>()?;
            let scale = match field(s, "scale") {
                Ok(v) => v.parse::<f64>()?,
                Err(_) => 1.0,
            };
            Ok((w, h, scale))
        }
"""

SIZE_FROM = """
        impl From<ParseIntError> for SizeError {
            fn from(e: ParseIntError) -> Self {
                SizeError::BadInt(e)
            }
        }

        impl From<ParseFloatError> for SizeError {
            fn from(e: ParseFloatError) -> Self {
                SizeError::BadFloat(e)
            }
        }
"""

P.append(dict(
    slug="fix-question-mark-from", title="Fix: ? can't convert the error", mode="fix", level="medium", stage="understand-it", tags=["?", "From", "E0277"],
    teaches=["`?` converts the error with `From::from` before returning it.", "One `impl From` per source error type makes every `?` on it work."],
    statement="""
        `size` parses `"width=80;height=24;scale=1.5"`. It doesn't compile: *`?` couldn't convert the error to `SizeError`*
        (E0277), three times. Make it compile without touching `size` or `field`, keeping each parse error in its variant.
    """,
    examples=[("s = \"width=80;height=24\"", "Ok((80, 24, 1.0))"), ("s = \"width=80;height=24;scale=big\"", "Err(BadFloat(..))")],
    starter=SIZE_HEAD + SIZE_TAIL,
    solution=SIZE_HEAD + SIZE_FROM + SIZE_TAIL,
    rules=dict(methods=["map_err", "unwrap", "expect", "ok", "unwrap_or"], lines=12),
    visible=[
        T("all_fields", "s = \"width=80;height=24;scale=1.5\"", 'size("width=80;height=24;scale=1.5")', "Ok((80, 24, 1.5))"),
        T("scale_defaults_to_one", "s = \"height=24;width=80\"", 'size("height=24;width=80")', "Ok((80, 24, 1.0))"),
        T("missing_height", "s = \"width=80\"", 'size("width=80")', 'Err(SizeError::Missing("height"))'),
        T("bad_width", "s = \"width=abc;height=1\"", 'size("width=abc;height=1")', 'Err(SizeError::BadInt("abc".parse::<u32>().unwrap_err()))'),
        T("bad_scale", "s = \"width=1;height=1;scale=big\"", 'size("width=1;height=1;scale=big")', 'Err(SizeError::BadFloat("big".parse::<f64>().unwrap_err()))'),
    ],
    hidden=[
        T("empty_input", "s = \"\"", 'size("")', 'Err(SizeError::Missing("width"))'),
        T("empty_value", "s = \"width=;height=1\"", 'size("width=;height=1")', 'Err(SizeError::BadInt("".parse::<u32>().unwrap_err()))'),
        T("negative_height", "s = \"width=80;height=-1\"", 'size("width=80;height=-1")', 'Err(SizeError::BadInt("-1".parse::<u32>().unwrap_err()))'),
        T("width_overflows", "s = \"width=4294967296;height=1\"", 'size("width=4294967296;height=1")', 'Err(SizeError::BadInt("4294967296".parse::<u32>().unwrap_err()))'),
        T("bounds", "s = \"width=4294967295;height=0;scale=-0.5\"", 'size("width=4294967295;height=0;scale=-0.5")', "Ok((u32::MAX, 0, -0.5))"),
        T("empty_scale", "s = \"width=1;height=2;scale=\"", 'size("width=1;height=2;scale=")', 'Err(SizeError::BadFloat("".parse::<f64>().unwrap_err()))'),
        T("int_scale_is_fine", "s = \"scale=3;width=1;height=2\"", 'size("scale=3;width=1;height=2")', "Ok((1, 2, 3.0))"),
        T("bad_width_before_missing_height", "s = \"width=x\"", 'size("width=x")', 'Err(SizeError::BadInt("x".parse::<u32>().unwrap_err()))'),
        T("missing_width_before_bad_height", "s = \"height=x\"", 'size("height=x")', 'Err(SizeError::Missing("width"))'),
        T("int_error_before_float_error", "s = \"scale=z;width=1;height=y\"", 'size("scale=z;width=1;height=y")', 'Err(SizeError::BadInt("y".parse::<u32>().unwrap_err()))'),
        T("first_width_wins", "s = \"width=1;width=2;height=3\"", 'size("width=1;width=2;height=3")', "Ok((1, 3, 1.0))"),
        T("full_width_digit", "s = \"width=８;height=1\"", 'size("width=８;height=1")', 'Err(SizeError::BadInt("８".parse::<u32>().unwrap_err()))'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(1315);
            let values = ["0", "7", "42", "-1", "", "x", "4294967295", "4294967296", "+3", "2.5", "1e3"];
            for _ in 0..300 {
                let w = rng.bool().then(|| *rng.pick(&values));
                let h = rng.bool().then(|| *rng.pick(&values));
                let sc = rng.bool().then(|| *rng.pick(&values));
                let mut parts: Vec<String> = Vec::new();
                for (name, v) in [("width", w), ("height", h), ("scale", sc)] {
                    if let Some(v) = v {
                        parts.push(format!("{name}={v}"));
                    }
                }
                rng.shuffle(&mut parts);
                let s = parts.join(";");
                let int = |v: Option<&str>, name: &'static str| v.ok_or(SizeError::Missing(name))?.parse::<u32>().map_err(SizeError::BadInt);
                let want = int(w, "width").and_then(|x| {
                    let y = int(h, "height")?;
                    let z = sc.map_or(Ok(1.0), |v| v.parse::<f64>().map_err(SizeError::BadFloat))?;
                    Ok((x, y, z))
                });
                check!(format!("s = {s:?}"), size(&s), want);
            }
        }
        """,
    ],
    wrong=dict(
        float_error_becomes_missing=SIZE_HEAD + SIZE_FROM.replace("SizeError::BadFloat(e)", "SizeError::Missing(\"scale\")").replace("fn from(e: ParseFloatError)", "fn from(_: ParseFloatError)") + SIZE_TAIL,
        bad_number_becomes_zero=SIZE_HEAD + SIZE_FROM + SIZE_TAIL.replace("let w = field(s, \"width\")?.parse::<u32>()?;\n            let h = field(s, \"height\")?.parse::<u32>()?;", "let w = field(s, \"width\")?.parse::<u32>().unwrap_or(0);\n            let h = field(s, \"height\")?.parse::<u32>().unwrap_or(0);"),
    ),
    hints=[("approach", "`x?` on `Err(e)` returns `Err(From::from(e))`. Which `From` impls are missing?"),
           ("rust", "`impl From<ParseIntError> for SizeError { fn from(e: ParseIntError) -> Self { SizeError::BadInt(e) } }`, and the same for `ParseFloatError`.")],
    notes=("Each `From` impl is written once, and every `?` on that error type in a function returning `SizeError` uses it. `map_err` at each call site works too but repeats itself; `thiserror`'s `#[from]` generates exactly these impls. API reminder: `?` needs `From<SourceError> for TargetError`; on `Option` it needs the function to return `Option`.", "O(n)", "O(1)"),
    follow_up="When would you prefer `map_err` at the call site over a `From` impl? What goes wrong with two variants that wrap the same source type?",
    related=["L8", "L10"],
))

P.append(dict(
    slug="fix-unwrap-on-input", title="Fix: unwrap on user input", mode="fix", level="medium", stage="understand-it", tags=["unwrap", "Result", "?"],
    teaches=["`unwrap` on input you don't control is a crash waiting for a user.", "`f64::from_str` accepts `NaN`, `inf`, `infinity` and `1e999` (which overflows to infinity): parsing isn't validating.", "Return errors that say what was wrong."],
    statement="""
        `average` takes comma-separated numbers like `"1, 2, 3"`. It panics on anything unexpected.
        Return `Err("no numbers")` for blank input and `Err("not a number: <item>")` (the trimmed item) for the first
        item that isn't a finite number.
    """,
    starter="""
        /// The average of comma-separated numbers, e.g. "1, 2, 3".
        pub fn average(input: &str) -> Result<f64, String> {
            let nums: Vec<f64> = input.split(',').map(|s| s.trim().parse().unwrap()).collect();
            Ok(nums.iter().sum::<f64>() / nums.len() as f64)
        }
    """,
    solution="""
        /// The average of comma-separated numbers, e.g. "1, 2, 3".
        pub fn average(input: &str) -> Result<f64, String> {
            if input.trim().is_empty() {
                return Err("no numbers".into());
            }
            let nums = input
                .split(',')
                .map(|s| {
                    let s = s.trim();
                    s.parse::<f64>().ok().filter(|x| x.is_finite()).ok_or_else(|| format!("not a number: {s}"))
                })
                .collect::<Result<Vec<f64>, String>>()?;
            Ok(nums.iter().sum::<f64>() / nums.len() as f64)
        }
    """,
    rules=dict(methods=["unwrap", "expect"]),
    visible=[
        T("three", "\"1, 2, 3\"", 'average("1, 2, 3")', "Ok(2.0)"),
        T("bad_item", "\"1, x\"", 'average("1, x")', 'Err("not a number: x".to_string())'),
        T("blank", "\"\"", 'average("")', 'Err("no numbers".to_string())'),
        T("single", "\"5\"", 'average("5")', "Ok(5.0)"),
        T("empty_item_is_not_a_number", "\"1,,2\"", 'average("1,,2")', 'Err("not a number: ".to_string())'),
    ],
    hidden=[
        T("whitespace_only", "\"   \"", 'average("   ")', 'Err("no numbers".to_string())'),
        T("trailing_comma", "\"4,\"", 'average("4,")', 'Err("not a number: ".to_string())'),
        T("negatives_and_decimals", "\"-1.5, 2.5\"", 'average("-1.5, 2.5")', "Ok(0.5)"),
        T("no_spaces", "\"1,2\"", 'average("1,2")', "Ok(1.5)"),
        T("spaces_around_items", "\"  1 ,\\t2  \"", 'average("  1 ,\\t2  ")', "Ok(1.5)"),
        T("message_is_trimmed", "\"1,   x  \"", 'average("1,   x  ")', 'Err("not a number: x".to_string())'),
        T("first_bad_item_wins", "\"a, b\"", 'average("a, b")', 'Err("not a number: a".to_string())'),
        T("only_a_comma", "\",\"", 'average(",")', 'Err("not a number: ".to_string())'),
        T("unicode_item", "\"1, ２\"", 'average("1, ２")', 'Err("not a number: ２".to_string())'),
        T("nan_is_not_a_number", "\"1, NaN\"", 'average("1, NaN")', 'Err("not a number: NaN".to_string())'),
        T("infinities_are_not_numbers", "\"inf\", \"-infinity\"", '(average("inf"), average("2, -infinity"))', '(Err("not a number: inf".to_string()), Err("not a number: -infinity".to_string()))'),
        T("overflowing_literal", "\"1e999\"", 'average("1e999")', 'Err("not a number: 1e999".to_string())'),
        T("large_but_finite", "\"1e300, -1e300, 3\"", 'average("1e300, -1e300, 3")', "Ok(1.0)"),
        T("exponents_and_signs", "\"+1.5e1, -5\"", 'average("+1.5e1, -5")', "Ok(5.0)"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(1308);
            for _ in 0..400 {
                let n = rng.below(6);
                let mut items: Vec<String> = Vec::new();
                for _ in 0..n {
                    let item = if rng.below(8) == 0 { (*rng.pick(&["x", "", "1x"])).to_string() } else { rng.int(-9, 9).to_string() };
                    let pad = rng.below(3);
                    items.push(format!("{}{}{}", " ".repeat(pad), item, " ".repeat(2 - pad)));
                }
                let input = items.join(",");
                let want = if input.trim().is_empty() {
                    Err("no numbers".to_string())
                } else if let Some(bad) = items.iter().map(|s| s.trim()).find(|s| s.parse::<i32>().is_err()) {
                    Err(format!("not a number: {bad}"))
                } else {
                    let sum: i32 = items.iter().map(|s| s.trim().parse::<i32>().unwrap_or(0)).sum();
                    Ok(f64::from(sum) / items.len() as f64)
                };
                check!(format!("input = {input:?}"), average(&input), want);
            }
        }
        """,
    ],
    wrong=dict(
        accepts_nan_and_infinity="""
            /// The average of comma-separated numbers, e.g. "1, 2, 3".
            pub fn average(input: &str) -> Result<f64, String> {
                if input.trim().is_empty() {
                    return Err("no numbers".into());
                }
                let nums = input
                    .split(',')
                    .map(|s| {
                        let s = s.trim();
                        s.parse::<f64>().map_err(|_| format!("not a number: {s}"))
                    })
                    .collect::<Result<Vec<f64>, String>>()?;
                Ok(nums.iter().sum::<f64>() / nums.len() as f64)
            }
        """,
        blank_check_after_parsing="""
            /// The average of comma-separated numbers, e.g. "1, 2, 3".
            pub fn average(input: &str) -> Result<f64, String> {
                let nums = input
                    .split(',')
                    .map(|s| {
                        let s = s.trim();
                        s.parse::<f64>().map_err(|_| format!("not a number: {s}"))
                    })
                    .collect::<Result<Vec<f64>, String>>()?;
                if nums.is_empty() {
                    return Err("no numbers".into());
                }
                Ok(nums.iter().sum::<f64>() / nums.len() as f64)
            }
        """,
        skips_empty_items="""
            /// The average of comma-separated numbers, e.g. "1, 2, 3".
            pub fn average(input: &str) -> Result<f64, String> {
                let nums = input
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.parse::<f64>().map_err(|_| format!("not a number: {s}")))
                    .collect::<Result<Vec<f64>, String>>()?;
                if nums.is_empty() {
                    return Err("no numbers".into());
                }
                Ok(nums.iter().sum::<f64>() / nums.len() as f64)
            }
        """,
        untrimmed_message="""
            /// The average of comma-separated numbers, e.g. "1, 2, 3".
            pub fn average(input: &str) -> Result<f64, String> {
                if input.trim().is_empty() {
                    return Err("no numbers".into());
                }
                let nums = input
                    .split(',')
                    .map(|s| s.trim().parse::<f64>().map_err(|_| format!("not a number: {s}")))
                    .collect::<Result<Vec<f64>, String>>()?;
                Ok(nums.iter().sum::<f64>() / nums.len() as f64)
            }
        """,
    ),
    hints=[("rust", "Turn each parse into a `Result` with a message, then collect into `Result<Vec<_>, _>` and use `?`."),
           ("edge case", "Blank input would otherwise divide by zero or fail on the empty item. And try `\"NaN\".parse::<f64>()`.")],
    notes=("Every failure path now returns a message instead of panicking. The blank check has to come first, or `\"\"` reports `not a number: `. `f64`'s parser follows IEEE 754 text forms, so `NaN`, `inf` and out-of-range literals parse successfully; one `NaN` then poisons the average silently, which is worse than the panic it replaced. Syntax to remember: `s.parse::<f64>().ok().filter(|x| x.is_finite()).ok_or_else(|| msg)`, `.collect::<Result<Vec<_>, _>>()?`.", "O(n)", "O(n)"),
    follow_up="Which unwraps in a codebase are fine, and how would you document them?",
    related=["L8"],
))

NICHE_STARTER = """
        /// A binary search tree in one `Vec`. Links are indices into `nodes`; the root is `nodes[0]`.
        #[derive(Debug)]
        pub struct Node {
            pub key: i32,
            pub left: Option<u32>,
            pub right: Option<u32>,
        }

        #[derive(Debug, Default)]
        pub struct Tree {
            pub nodes: Vec<Node>,
        }

        impl Tree {
            pub fn new() -> Tree {
                Tree { nodes: Vec::new() }
            }

            /// Inserts `key`. Returns false if it was already there.
            pub fn insert(&mut self, key: i32) -> bool {
                if self.nodes.is_empty() {
                    self.nodes.push(Node { key, left: None, right: None });
                    return true;
                }
                let new = u32::try_from(self.nodes.len()).expect("too many nodes");
                let mut i = 0;
                loop {
                    let node = &mut self.nodes[i];
                    let link = if key < node.key {
                        &mut node.left
                    } else if key > node.key {
                        &mut node.right
                    } else {
                        return false;
                    };
                    match *link {
                        Some(next) => i = next as usize,
                        None => {
                            *link = Some(new);
                            break;
                        }
                    }
                }
                self.nodes.push(Node { key, left: None, right: None });
                true
            }

            pub fn contains(&self, key: i32) -> bool {
                let mut link = if self.nodes.is_empty() { None } else { Some(0) };
                while let Some(i) = link {
                    let node = &self.nodes[i as usize];
                    if key == node.key {
                        return true;
                    }
                    link = if key < node.key { node.left } else { node.right };
                }
                false
            }

            /// The keys in ascending order.
            pub fn in_order(&self) -> Vec<i32> {
                let mut out = Vec::with_capacity(self.nodes.len());
                let mut stack = Vec::new();
                let mut link = if self.nodes.is_empty() { None } else { Some(0) };
                loop {
                    while let Some(i) = link {
                        stack.push(i);
                        link = self.nodes[i as usize].left;
                    }
                    let Some(i) = stack.pop() else { break };
                    out.push(self.nodes[i as usize].key);
                    link = self.nodes[i as usize].right;
                }
                out
            }
        }
"""

# Index 0 is the root, which is never anyone's child, so a child link is never 0 and fits a NonZeroU32 as is.
NICHE_SOL = (NICHE_STARTER
    .replace("/// A binary search tree in one `Vec`.", "use std::num::NonZeroU32;\n\n        /// A binary search tree in one `Vec`.")
    .replace("pub left: Option<u32>,\n            pub right: Option<u32>,", "// The root is nodes[0] and is never a child, so a child index is never 0.\n            pub left: Option<NonZeroU32>,\n            pub right: Option<NonZeroU32>,")
    .replace('let new = u32::try_from(self.nodes.len()).expect("too many nodes");', 'let new = u32::try_from(self.nodes.len()).ok().and_then(NonZeroU32::new).expect("too many nodes");')
    .replace("Some(next) => i = next as usize,", "Some(next) => i = next.get() as usize,")
    .replace("let mut link = if self.nodes.is_empty() { None } else { Some(0) };\n                while let Some(i) = link {\n                    let node = &self.nodes[i as usize];",
             "let mut link = if self.nodes.is_empty() { None } else { Some(0) };\n                while let Some(i) = link {\n                    let node = &self.nodes[i];")
    .replace("link = if key < node.key { node.left } else { node.right };", "link = (if key < node.key { node.left } else { node.right }).map(|n| n.get() as usize);")
    .replace("let mut link = if self.nodes.is_empty() { None } else { Some(0) };\n                loop {", "let mut link: Option<usize> = if self.nodes.is_empty() { None } else { Some(0) };\n                loop {")
    .replace("link = self.nodes[i as usize].left;", "link = self.nodes[i].left.map(|n| n.get() as usize);")
    .replace("out.push(self.nodes[i as usize].key);\n                    link = self.nodes[i as usize].right;", "out.push(self.nodes[i].key);\n                    link = self.nodes[i].right.map(|n| n.get() as usize);"))

NICHE_U16 = (NICHE_STARTER
    .replace("pub left: Option<u32>,\n            pub right: Option<u32>,", "pub left: Option<u16>,\n            pub right: Option<u16>,")
    .replace('let new = u32::try_from(self.nodes.len()).expect("too many nodes");', "let new = self.nodes.len() as u16;"))

NICHE_PLUS_ONE_BUG = NICHE_SOL.replace("u32::try_from(self.nodes.len()).ok().and_then(NonZeroU32::new)", "u32::try_from(self.nodes.len() + 1).ok().and_then(NonZeroU32::new)")

P.append(dict(
    slug="fix-option-niche", title="Fix: Option<u32> links bloat every node", mode="fix", level="medium", stage="understand-it", tags=["niche optimization", "NonZeroU32", "size_of"],
    teaches=[
        "`Option<T>` is free only when `T` has a niche, a bit pattern it never uses: references, `Box`, `NonNull`, `NonZero*`, `bool`, `char`, enums.",
        "`Option<u32>` has no spare value, so it needs a separate tag and doubles to 8 bytes; `Option<NonZeroU32>` stays 4.",
        "Arena and slab code keeps `Option` links compact by making index 0 impossible (or storing index + 1).",
    ],
    statement="""
        `Tree` stores a binary search tree in one `Vec`, with child links as indices. It works, but it will hold
        hundreds of millions of nodes, and each `Node` is 20 bytes: `i32` + two `Option<u32>` of 8 bytes each.

        Get `Node` down to **12 bytes** without changing what the tree does. The links must stay `Option`s (the tests
        call `.is_none()` on them), and the tree must still work past 65 536 nodes. No `unsafe`.
    """,
    examples=[("size_of::<Node>()", "12"), ("insert 5, 3, 8; in_order()", "[3, 5, 8]")],
    starter=NICHE_STARTER,
    solution=NICHE_SOL,
    rules=dict(unsafe=True, lines=16),
    visible=[
        T("node_is_twelve_bytes", "size_of::<Node>()", "std::mem::size_of::<Node>()", "12"),
        T("insert_and_in_order", "insert 5, 3, 8, 3", "(r, t.in_order())", "([true, true, true, false], vec![3, 5, 8])", setup="let mut t = Tree::new();\nlet r = [5, 3, 8, 3].map(|k| t.insert(k));"),
        T("contains", "insert 5, 3, 8; contains 3, 4, 8", "(t.contains(3), t.contains(4), t.contains(8))", "(true, false, true)", setup="let mut t = Tree::new();\nfor k in [5, 3, 8] {\n    t.insert(k);\n}"),
        T("links_are_options", "insert 5, 3: root's right and the leaf's links", "(t.nodes[0].left.is_some(), t.nodes[0].right.is_none(), t.nodes[1].left.is_none(), t.nodes[1].right.is_none())", "(true, true, true, true)", setup="let mut t = Tree::new();\nt.insert(5);\nt.insert(3);"),
        T("empty_tree", "Tree::new()", "(t.contains(0), t.in_order())", "(false, Vec::<i32>::new())", setup="let t = Tree::new();"),
    ],
    hidden=[
        T("single_node", "insert 7", "(t.contains(7), t.in_order(), t.nodes[0].left.is_none() && t.nodes[0].right.is_none())", "(true, vec![7], true)", setup="let mut t = Tree::new();\nt.insert(7);"),
        T("extreme_keys", "insert i32::MIN, i32::MAX, 0", "t.in_order()", "vec![i32::MIN, 0, i32::MAX]", setup="let mut t = Tree::new();\nfor k in [i32::MIN, i32::MAX, 0] {\n    t.insert(k);\n}"),
        T("duplicates_ignored", "insert 1, 1, 1", "(t.nodes.len(), t.in_order())", "(1, vec![1])", setup="let mut t = Tree::new();\nfor _ in 0..3 {\n    t.insert(1);\n}"),
        T("sorted_inserts_make_a_right_spine", "insert 1..=5", "(t.in_order(), t.nodes.iter().all(|n| n.left.is_none()))", "(vec![1, 2, 3, 4, 5], true)", setup="let mut t = Tree::new();\nfor k in 1..=5 {\n    t.insert(k);\n}"),
        T("reverse_inserts_make_a_left_spine", "insert 5, 4, 3, 2, 1", "(t.in_order(), t.nodes.iter().all(|n| n.right.is_none()))", "(vec![1, 2, 3, 4, 5], true)", setup="let mut t = Tree::new();\nfor k in (1..=5).rev() {\n    t.insert(k);\n}"),
        T("zero_key_is_an_ordinary_key", "insert 0, -1, 1", "(t.contains(0), t.contains(-1), t.contains(1), t.contains(2))", "(true, true, true, false)", setup="let mut t = Tree::new();\nfor k in [0, -1, 1] {\n    t.insert(k);\n}"),
        T("vec_of_nodes_is_smaller", "size_of::<Node>() * 1000", "std::mem::size_of::<Node>() * 1000", "12_000"),
        T("mixed_signs", "insert -5, 5, -3, 3, 0", "(t.in_order(), t.contains(-3), t.contains(-4))", "(vec![-5, -3, 0, 3, 5], true, false)", setup="let mut t = Tree::new();\nfor k in [-5, 5, -3, 3, 0] {\n    t.insert(k);\n}"),
        """
        #[test]
        fn random_vs_btreeset() {
            let mut rng = anneal_prelude::Rng::new(7108);
            for _ in 0..300 {
                let n = rng.below(20);
                let keys: Vec<i32> = rng.vec(n, -10, 10);
                let mut t = Tree::new();
                let mut model = std::collections::BTreeSet::new();
                for &k in &keys {
                    check!(format!("insert {k} after {model:?}"), t.insert(k), model.insert(k));
                }
                check!(format!("in_order after inserting {keys:?}"), t.in_order(), model.iter().copied().collect::<Vec<_>>());
                let q = rng.int(-11, 11) as i32;
                check!(format!("contains {q} after inserting {keys:?}"), t.contains(q), model.contains(&q));
            }
        }

        #[test]
        fn scale_past_u16() {
            let mut rng = anneal_prelude::Rng::new(7109);
            let mut keys: Vec<i32> = (0..150_000).map(|i| i * 3).collect();
            rng.shuffle(&mut keys);
            let mut t = Tree::new();
            for &k in &keys {
                t.insert(k);
            }
            let hits = (0..450_000).filter(|&k| t.contains(k)).count();
            let sorted = t.in_order();
            check!("150000 shuffled keys 0, 3, 6, …: hits among 0..450000, and in_order", (hits, sorted.len(), sorted[149_999], sorted.windows(2).all(|w| w[0] < w[1])), (150_000, 150_000, 449_997, true));
        }
        """,
    ],
    wrong=dict(
        u16_links=NICHE_U16,
        off_by_one_link=NICHE_PLUS_ONE_BUG,
    ),
    hints=[("approach", "`Option<u32>` needs somewhere to store \"is this `Some`\", because every `u32` is a valid value. Which integer type rules out one value, and which index can never be a child link?"),
           ("rust", "`std::num::NonZeroU32`: `NonZeroU32::new(x)` returns `Option<NonZeroU32>` (`None` for 0), and `n.get()` gives the `u32` back."),
           ("edge case", "The root is index 0 and is never anyone's child, so a child link is always at least 1. `u16` links would fit, but break past 65 536 nodes.")],
    notes=("""A niche is a bit pattern a type promises never to hold. The compiler uses it to encode `None`, so `Option<&T>`, `Option<Box<T>>`, `Option<NonNull<T>>`, `Option<fn()>` and `Option<NonZeroU32>` are exactly the size of the inner type; that much is guaranteed. Plain integers have no niche, so `Option<u32>` adds a tag and padding: 8 bytes. That's why linked structures in Rust use `Option<Box<Node>>` without a second thought, and why index-based arenas use `NonZeroU32` handles (index 0 reserved, or index + 1 stored). Other niches are real but unguaranteed: `Option<Vec<T>>`, `Option<String>` and `Option<Option<bool>>` also stay the same size today. Syntax to remember: `NonZeroU32::new(x) -> Option<NonZeroU32>`, `n.get()`, `std::mem::size_of::<T>()`, `u32::try_from(len).ok().and_then(NonZeroU32::new)`.""", "O(h) insert and contains, O(n) in_order", "12 bytes per node"),
    follow_up="What would you do if index 0 had to be a valid child too? How do generational arenas (slotmap) pack an index and a generation into one `Option`-friendly handle?",
    related=["S11", "Y1", "D6"],
))

MY_OPTION = """
        #[derive(Debug, PartialEq, Eq, Clone, Copy)]
        pub enum MyOption<T> {
            Some(T),
            None,
        }

        impl<T> MyOption<T> {
            pub fn is_some_and(self, f: impl FnOnce(T) -> bool) -> bool {
                match self {
                    MyOption::Some(v) => f(v),
                    MyOption::None => false,
                }
            }
            pub fn unwrap_or_else(self, f: impl FnOnce() -> T) -> T {
                match self {
                    MyOption::Some(v) => v,
                    MyOption::None => f(),
                }
            }
            pub fn unwrap_or_default(self) -> T
            where
                T: Default,
            {
                match self {
                    MyOption::Some(v) => v,
                    MyOption::None => T::default(),
                }
            }
            pub fn map<U>(self, f: impl FnOnce(T) -> U) -> MyOption<U> {
                match self {
                    MyOption::Some(v) => MyOption::Some(f(v)),
                    MyOption::None => MyOption::None,
                }
            }
            pub fn and_then<U>(self, f: impl FnOnce(T) -> MyOption<U>) -> MyOption<U> {
                match self {
                    MyOption::Some(v) => f(v),
                    MyOption::None => MyOption::None,
                }
            }
            pub fn and<U>(self, other: MyOption<U>) -> MyOption<U> {
                match self {
                    MyOption::Some(_) => other,
                    MyOption::None => MyOption::None,
                }
            }
            pub fn or_else(self, f: impl FnOnce() -> MyOption<T>) -> MyOption<T> {
                match self {
                    MyOption::Some(v) => MyOption::Some(v),
                    MyOption::None => f(),
                }
            }
            pub fn filter(self, keep: impl FnOnce(&T) -> bool) -> MyOption<T> {
                match self {
                    MyOption::Some(v) if keep(&v) => MyOption::Some(v),
                    _ => MyOption::None,
                }
            }
            pub fn take(&mut self) -> MyOption<T> {
                std::mem::replace(self, MyOption::None)
            }
            pub fn iter(&self) -> Iter<'_, T> {
                Iter {
                    inner: match self {
                        MyOption::Some(v) => MyOption::Some(v),
                        MyOption::None => MyOption::None,
                    },
                }
            }
        }

        /// Yields a reference to the value, at most once.
        pub struct Iter<'a, T> {
            inner: MyOption<&'a T>,
        }

        impl<'a, T> Iterator for Iter<'a, T> {
            type Item = &'a T;

            fn next(&mut self) -> Option<&'a T> {
                match self.inner.take() {
                    MyOption::Some(v) => Some(v),
                    MyOption::None => None,
                }
            }
        }
    """

P.append(dict(
    slug="my-option", title="Build MyOption<T>", level="hard", stage="build-it", tags=["enum", "generics", "mem::replace", "Iterator"],
    teaches=[
        "Every combinator is a `match` on two variants; the lazy ones call their closure in one arm only.",
        "`unwrap_or_default` is available only when `T: Default`: a `where` clause on a single method.",
        "`take` needs `mem::replace` to move out of `&mut self`, and an iterator over an `Option` is `take` in a loop.",
    ],
    statement="""
        Implement `MyOption<T>`, a copy of `Option<T>`, with ten methods that behave like std's:
        `is_some_and`, `unwrap_or_else`, `unwrap_or_default`, `map`, `and_then`, `and`, `or_else`, `filter`, `take`
        and `iter`. `iter` returns an `Iter` that yields `&T` at most once.

        Closures must run only when std's would, and at most once.
    """,
    starter="""
        #[derive(Debug, PartialEq, Eq, Clone, Copy)]
        pub enum MyOption<T> {
            Some(T),
            None,
        }

        impl<T> MyOption<T> {
            pub fn is_some_and(self, f: impl FnOnce(T) -> bool) -> bool { todo!() }
            pub fn unwrap_or_else(self, f: impl FnOnce() -> T) -> T { todo!() }
            pub fn unwrap_or_default(self) -> T where T: Default { todo!() }
            pub fn map<U>(self, f: impl FnOnce(T) -> U) -> MyOption<U> { todo!() }
            pub fn and_then<U>(self, f: impl FnOnce(T) -> MyOption<U>) -> MyOption<U> { todo!() }
            pub fn and<U>(self, other: MyOption<U>) -> MyOption<U> { todo!() }
            pub fn or_else(self, f: impl FnOnce() -> MyOption<T>) -> MyOption<T> { todo!() }
            pub fn filter(self, keep: impl FnOnce(&T) -> bool) -> MyOption<T> { todo!() }
            pub fn take(&mut self) -> MyOption<T> { todo!() }
            pub fn iter(&self) -> Iter<'_, T> { todo!() }
        }

        /// Yields a reference to the value, at most once.
        pub struct Iter<'a, T> {
            inner: MyOption<&'a T>,
        }

        impl<'a, T> Iterator for Iter<'a, T> {
            type Item = &'a T;

            fn next(&mut self) -> Option<&'a T> {
                todo!()
            }
        }
    """,
    solution=MY_OPTION,
    visible=[
        T("map_and_then", "Some(2)", "MyOption::Some(2).map(|x| x * 10).and_then(|x| if x > 5 { MyOption::Some(x + 1) } else { MyOption::None })", "MyOption::Some(21)"),
        T("is_some_and", "Some(4), Some(5), None", "(MyOption::Some(4).is_some_and(|x| x % 2 == 0), MyOption::Some(5).is_some_and(|x| x % 2 == 0), MyOption::<i32>::None.is_some_and(|_| true))", "(true, false, false)"),
        T("take", "Some(\"a\")", '{ let mut o = MyOption::Some("a"); let t = o.take(); (t, o) }', '(MyOption::Some("a"), MyOption::None)'),
        T("and_or_else", "Some(1).and(Some(\"x\")), None.or_else(|| Some(3))", '(MyOption::Some(1).and(MyOption::Some("x")), MyOption::None.or_else(|| MyOption::Some(3)))', '(MyOption::Some("x"), MyOption::Some(3))'),
        T("iter_yields_once", "Some(7).iter(), None.iter()", "(MyOption::Some(7).iter().copied().take(5).collect::<Vec<_>>(), MyOption::<i32>::None.iter().take(5).count())", "(vec![7], 0)"),
    ],
    hidden=[
        T("unwrap_or_default", "None::<String>, Some(\"x\"), None::<Vec<u8>>", '(MyOption::<String>::None.unwrap_or_default(), MyOption::Some("x".to_string()).unwrap_or_default(), MyOption::<Vec<u8>>::None.unwrap_or_default())', '(String::new(), "x".to_string(), Vec::new())'),
        T("unwrap_or_else_lazy", "Some(1)", "MyOption::Some(1).unwrap_or_else(|| panic!(\"should not run\"))", "1"),
        T("unwrap_or_else_none", "None", "MyOption::None.unwrap_or_else(|| 9)", "9"),
        T("or_else_keeps_some_and_skips_f", "Some(1).or_else(panics)", "MyOption::Some(1).or_else(|| panic!(\"should not run\"))", "MyOption::Some(1)"),
        T("or_else_both_none", "None.or_else(|| None)", "MyOption::<i32>::None.or_else(|| MyOption::None)", "MyOption::None"),
        T("and_with_none", "Some(1).and(None), None.and(Some(2))", "(MyOption::Some(1).and(MyOption::<u8>::None), MyOption::<i32>::None.and(MyOption::Some(2u8)))", "(MyOption::None, MyOption::None)"),
        T("and_changes_type", "Some(\"s\").and(Some(5u64))", 'MyOption::Some("s").and(MyOption::Some(5u64))', "MyOption::Some(5u64)"),
        T("is_some_and_skips_f_on_none", "None.is_some_and(panics)", "MyOption::<i32>::None.is_some_and(|x| panic!(\"should not run: {x}\"))", "false"),
        T("is_some_and_takes_ownership", "Some(String::from(\"hi\")).is_some_and(|s| s.len() == 2)", 'MyOption::Some(String::from("hi")).is_some_and(|s: String| s.len() == 2)', "true"),
        T("filter", "Some(4), Some(5), None", "(MyOption::Some(4).filter(|x| x % 2 == 0), MyOption::Some(5).filter(|x| x % 2 == 0), MyOption::<i32>::None.filter(|x| panic!(\"should not run: {x}\")))", "(MyOption::Some(4), MyOption::None, MyOption::None)"),
        T("map_changes_type_and_skips_none", "Some(\"abc\").map(len), None.map(panics)", '(MyOption::Some("abc").map(str::len), MyOption::<i32>::None.map(|x| -> i32 { panic!("should not run: {x}") }))', "(MyOption::Some(3), MyOption::None)"),
        T("and_then_none_skips_f", "None", "MyOption::<i32>::None.and_then(|x| -> MyOption<i32> { panic!(\"should not run: {x}\") })", "MyOption::None"),
        T("take_none", "None", "{ let mut o = MyOption::<i32>::None; let t = o.take(); (t, o) }", "(MyOption::None, MyOption::None)"),
        T("take_moves_a_string", "Some(String::from(\"hi\"))", '{ let mut o = MyOption::Some(String::from("hi")); let t = o.take(); (t, o) }', '(MyOption::Some("hi".to_string()), MyOption::None)'),
        T("iter_stops_after_one", "Some(1).iter(), next() three times", "{ let o = MyOption::Some(1); let mut it = o.iter(); (it.next().copied(), it.next().copied(), it.next().copied()) }", "(Some(1), None, None)"),
        T("iter_borrows_in_place", "Some(String::from(\"x\")).iter()", '{ let o = MyOption::Some(String::from("x")); let p = o.iter().next().map(|s| s.as_ptr()); let q = match &o { MyOption::Some(s) => Some(s.as_ptr()), MyOption::None => None }; p == q && p.is_some() }', "true"),
        T("iter_chains_like_std", "[Some(1), None, Some(3)] flattened through iter()", "[MyOption::Some(1), MyOption::None, MyOption::Some(3)].iter().flat_map(|o| o.iter()).copied().take(5).collect::<Vec<_>>()", "vec![1, 3]"),
        """
        fn mine<T>(o: Option<T>) -> MyOption<T> {
            match o {
                Some(v) => MyOption::Some(v),
                None => MyOption::None,
            }
        }

        #[test]
        fn random_vs_std_option() {
            let mut rng = anneal_prelude::Rng::new(7110);
            for _ in 0..300 {
                let a = if rng.bool() { Some(rng.int(-5, 5) as i32) } else { None };
                let b = if rng.bool() { Some(rng.int(-5, 5) as i32) } else { None };
                let d = rng.int(-5, 5) as i32;
                let desc = format!("a = {a:?}, b = {b:?}, d = {d}");
                let mut calls = (0, 0);
                check!(format!("is_some_and(x > d), {desc}"), mine(a).is_some_and(|x| { calls.0 += 1; x > d }), a.is_some_and(|x| { calls.1 += 1; x > d }));
                let mut calls2 = (0, 0);
                check!(format!("unwrap_or_else(|| d), {desc}"), mine(a).unwrap_or_else(|| { calls2.0 += 1; d }), a.unwrap_or_else(|| { calls2.1 += 1; d }));
                check!(format!("unwrap_or_default, {desc}"), mine(a).unwrap_or_default(), a.unwrap_or_default());
                check!(format!("map(x + d), {desc}"), mine(a).map(|x| x + d), mine(a.map(|x| x + d)));
                check!(format!("and_then(positive), {desc}"), mine(a).and_then(|x| if x > 0 { MyOption::Some(x * 3) } else { MyOption::None }), mine(a.and_then(|x| if x > 0 { Some(x * 3) } else { None })));
                check!(format!("a.and(b), {desc}"), mine(a).and(mine(b)), mine(a.and(b)));
                let mut calls3 = (0, 0);
                check!(format!("a.or_else(|| b), {desc}"), mine(a).or_else(|| { calls3.0 += 1; mine(b) }), mine(a.or_else(|| { calls3.1 += 1; b })));
                check!(format!("filter(even), {desc}"), mine(a).filter(|x| x % 2 == 0), mine(a.filter(|x| x % 2 == 0)));
                let mut m = mine(a);
                let mut s = a;
                check!(format!("take, {desc}"), (m.take(), m), (mine(s.take()), mine(s)));
                check!(format!("iter, {desc}"), mine(a).iter().copied().take(5).collect::<Vec<_>>(), a.iter().copied().collect::<Vec<_>>());
                check!(format!("closure calls, {desc}"), (calls.0, calls2.0, calls3.0), (calls.1, calls2.1, calls3.1));
            }
        }
        """,
    ],
    wrong=dict(
        eager_or_else=MY_OPTION.replace("pub fn or_else(self, f: impl FnOnce() -> MyOption<T>) -> MyOption<T> {\n                match self {\n                    MyOption::Some(v) => MyOption::Some(v),\n                    MyOption::None => f(),",
                                        "pub fn or_else(self, f: impl FnOnce() -> MyOption<T>) -> MyOption<T> {\n                let other = f();\n                match self {\n                    MyOption::Some(v) => MyOption::Some(v),\n                    MyOption::None => other,"),
        and_ignores_self=MY_OPTION.replace("MyOption::Some(_) => other,\n                    MyOption::None => MyOption::None,", "_ => other,"),
        iter_never_ends=MY_OPTION.replace("match self.inner.take() {", "match self.inner {"),
        filter_drops_matches=MY_OPTION.replace("if keep(&v)", "if !keep(&v)"),
    ),
    hints=[("approach", "Each method is one `match` on `Some(v)` / `None`. The `_else` and `_and` methods call their closure in one arm only."),
           ("rust", "A bound on one method: `pub fn unwrap_or_default(self) -> T where T: Default`. `take` is `std::mem::replace(self, MyOption::None)`."),
           ("edge case", "`Iter::next` must leave `None` behind, or the iterator never ends: take the inner reference out, don't copy it.")],
    notes=("""The lazy/eager split is the whole design: `and(other)` and `or(other)` take a value you already have, `and_then(f)` and `or_else(f)` take a closure that runs only when needed. `unwrap_or_default` shows that methods can have their own bounds, so `MyOption<Token>` still works as long as nobody calls it. `iter` borrows (`Iter<'a, T>` holds a `MyOption<&'a T>`) and is `take` in a loop; std's `Option::iter` is the same, plus `DoubleEndedIterator` and `ExactSizeIterator`. Syntax to remember: `fn m(self) -> T where T: Default`, `pub struct Iter<'a, T> { inner: MyOption<&'a T> }`, `impl<'a, T> Iterator for Iter<'a, T> { type Item = &'a T; fn next(&mut self) -> Option<&'a T> }`, `pub fn iter(&self) -> Iter<'_, T>`.""", "O(1) each", "O(1)"),
    follow_up="Why does `Option<&T>` have the same size as `&T`, and would `MyOption<&T>` too? What does `Option::iter` gain from also implementing `ExactSizeIterator`?",
    related=["L7", "L6", "Y1", "S6"],
))

MY_OPTION_BORROW = """
        #[derive(Debug, PartialEq, Eq, Clone, Copy)]
        pub enum MyOption<T> {
            Some(T),
            None,
        }

        impl<T> MyOption<T> {
            pub fn as_ref(&self) -> MyOption<&T> {
                match self {
                    MyOption::Some(v) => MyOption::Some(v),
                    MyOption::None => MyOption::None,
                }
            }
            pub fn as_mut(&mut self) -> MyOption<&mut T> {
                match self {
                    MyOption::Some(v) => MyOption::Some(v),
                    MyOption::None => MyOption::None,
                }
            }
            pub fn insert(&mut self, value: T) -> &mut T {
                *self = MyOption::Some(value);
                match self {
                    MyOption::Some(v) => v,
                    MyOption::None => unreachable!(),
                }
            }
            pub fn replace(&mut self, value: T) -> MyOption<T> {
                std::mem::replace(self, MyOption::Some(value))
            }
            pub fn get_or_insert_with(&mut self, f: impl FnOnce() -> T) -> &mut T {
                if let MyOption::None = self {
                    *self = MyOption::Some(f());
                }
                match self {
                    MyOption::Some(v) => v,
                    MyOption::None => unreachable!(),
                }
            }
            pub fn zip<U>(self, other: MyOption<U>) -> MyOption<(T, U)> {
                match (self, other) {
                    (MyOption::Some(a), MyOption::Some(b)) => MyOption::Some((a, b)),
                    _ => MyOption::None,
                }
            }
            pub fn inspect(self, f: impl FnOnce(&T)) -> MyOption<T> {
                if let MyOption::Some(v) = &self {
                    f(v);
                }
                self
            }
            pub fn map_or_else<U>(self, default: impl FnOnce() -> U, f: impl FnOnce(T) -> U) -> U {
                match self {
                    MyOption::Some(v) => f(v),
                    MyOption::None => default(),
                }
            }
        }

        impl<T> MyOption<MyOption<T>> {
            pub fn flatten(self) -> MyOption<T> {
                match self {
                    MyOption::Some(inner) => inner,
                    MyOption::None => MyOption::None,
                }
            }
        }

        impl<T: Copy> MyOption<&T> {
            pub fn copied(self) -> MyOption<T> {
                match self {
                    MyOption::Some(&v) => MyOption::Some(v),
                    MyOption::None => MyOption::None,
                }
            }
        }
    """

P.append(dict(
    slug="my-option-borrowing", title="MyOption: borrowing and in-place methods", level="hard", stage="build-it", tags=["enum", "borrowing", "impl blocks"],
    teaches=["`as_ref` / `as_mut` turn `&MyOption<T>` into `MyOption<&T>` by matching on a reference.", "`get_or_insert_with` fills the slot first and borrows second. Returning early from `if let Some(v) = self.as_mut()` and then assigning is rejected (E0506), although it is sound.", "`flatten` and `copied` exist only for some `T`, so they live in `impl` blocks for `MyOption<MyOption<T>>` and `MyOption<&T>`."],
    statement="""
        Implement ten more `Option` methods on `MyOption`, each behaving like std's: `as_ref`, `as_mut`, `insert`,
        `replace`, `get_or_insert_with`, `zip`, `inspect`, `map_or_else`, and `flatten` and `copied` in their own `impl` blocks.
        Closures must run only when std's would.
    """,
    starter="""
        #[derive(Debug, PartialEq, Eq, Clone, Copy)]
        pub enum MyOption<T> {
            Some(T),
            None,
        }

        impl<T> MyOption<T> {
            pub fn as_ref(&self) -> MyOption<&T> { todo!() }
            pub fn as_mut(&mut self) -> MyOption<&mut T> { todo!() }
            pub fn insert(&mut self, value: T) -> &mut T { todo!() }
            pub fn replace(&mut self, value: T) -> MyOption<T> { todo!() }
            pub fn get_or_insert_with(&mut self, f: impl FnOnce() -> T) -> &mut T { todo!() }
            pub fn zip<U>(self, other: MyOption<U>) -> MyOption<(T, U)> { todo!() }
            pub fn inspect(self, f: impl FnOnce(&T)) -> MyOption<T> { todo!() }
            pub fn map_or_else<U>(self, default: impl FnOnce() -> U, f: impl FnOnce(T) -> U) -> U { todo!() }
        }

        impl<T> MyOption<MyOption<T>> {
            pub fn flatten(self) -> MyOption<T> { todo!() }
        }

        impl<T: Copy> MyOption<&T> {
            pub fn copied(self) -> MyOption<T> { todo!() }
        }
    """,
    solution=MY_OPTION_BORROW,
    visible=[
        T("as_ref_borrows", "Some(String::from(\"hi\"))", '{ let o = MyOption::Some(String::from("hi")); let r = o.as_ref() == MyOption::Some(&"hi".to_string()); (r, o) }', '(true, MyOption::Some("hi".to_string()))'),
        T("as_mut_edits_in_place", "Some(String::from(\"hi\")), push '!'", "{ let mut o = MyOption::Some(String::from(\"hi\")); if let MyOption::Some(s) = o.as_mut() { s.push('!'); } o }", 'MyOption::Some("hi!".to_string())'),
        T("replace_returns_the_old_value", "Some(1), replace(2)", "{ let mut o = MyOption::Some(1); let old = o.replace(2); (old, o) }", "(MyOption::Some(1), MyOption::Some(2))"),
        T("get_or_insert_with_fills_none", "None, get_or_insert_with(|| 5), then += 1", "{ let mut o = MyOption::None; *o.get_or_insert_with(|| 5) += 1; o }", "MyOption::Some(6)"),
        T("zip_inspect_map_or_else", "Some(1).zip(Some(\"a\")), Some(5).inspect(log), None.map_or_else(|| -1, ..)", '{ let mut seen = Vec::new(); let r = (MyOption::Some(1).zip(MyOption::Some("a")), MyOption::Some(5).inspect(|x| seen.push(*x)), MyOption::<i32>::None.map_or_else(|| -1, |x| x * 2)); (r, seen) }', '((MyOption::Some((1, "a")), MyOption::Some(5), -1), vec![5])'),
    ],
    hidden=[
        T("as_ref_none", "None", "MyOption::<String>::None.as_ref()", "MyOption::None"),
        T("as_ref_points_inside", "Some(String::from(\"x\"))", "{ let o = MyOption::Some(String::from(\"x\")); let same = match (o.as_ref(), &o) { (MyOption::Some(r), MyOption::Some(s)) => std::ptr::eq(r, s), _ => false }; same }", "true"),
        T("as_mut_none", "None", "{ let mut o = MyOption::<i32>::None; let r = o.as_mut() == MyOption::None; (r, o) }", "(true, MyOption::None)"),
        T("insert_overwrites", "Some(1), insert(2), then += 10", "{ let mut o = MyOption::Some(1); *o.insert(2) += 10; o }", "MyOption::Some(12)"),
        T("insert_into_none", "None, insert(String::from(\"a\")), push 'b'", '{ let mut o = MyOption::None; o.insert(String::from("a")).push(\'b\'); o }', 'MyOption::Some("ab".to_string())'),
        T("flatten_all_shapes", "Some(Some(1)), Some(None), None", "(MyOption::Some(MyOption::Some(1)).flatten(), MyOption::Some(MyOption::<i32>::None).flatten(), MyOption::<MyOption<i32>>::None.flatten())", "(MyOption::Some(1), MyOption::None, MyOption::None)"),
        T("flatten_one_level_only", "Some(Some(Some(1)))", "MyOption::Some(MyOption::Some(MyOption::Some(1))).flatten()", "MyOption::Some(MyOption::Some(1))"),
        T("copied_from_as_ref", "Some(7).as_ref().copied(), None", "{ let o = MyOption::Some(7); (o.as_ref().copied(), MyOption::<&u8>::None.copied(), o) }", "(MyOption::Some(7), MyOption::None, MyOption::Some(7))"),
        T("replace_none", "None, replace(\"a\")", '{ let mut o = MyOption::None; let old = o.replace("a"); (old, o) }', '(MyOption::None, MyOption::Some("a"))'),
        T("get_or_insert_with_keeps_some", "Some(3), get_or_insert_with(panics)", "{ let mut o = MyOption::Some(3); let v = *o.get_or_insert_with(|| panic!(\"should not run\")); (v, o) }", "(3, MyOption::Some(3))"),
        T("get_or_insert_with_calls_once", "None, get_or_insert_with twice", "{ let mut calls = 0; let mut o = MyOption::None; o.get_or_insert_with(|| { calls += 1; 7 }); o.get_or_insert_with(|| { calls += 1; 8 }); (o, calls) }", "(MyOption::Some(7), 1)"),
        T("get_or_insert_with_returns_the_slot", "Some(vec![1]), push 2 through the reference", "{ let mut o = MyOption::Some(vec![1]); o.get_or_insert_with(Vec::new).push(2); o }", "MyOption::Some(vec![1, 2])"),
        T("zip_with_none", "Some(1) zip None, None zip Some(1)", "(MyOption::Some(1).zip(MyOption::<u8>::None), MyOption::<u8>::None.zip(MyOption::Some(1)))", "(MyOption::None, MyOption::None)"),
        T("inspect_none_skips_f", "None.inspect(panics)", "MyOption::<i32>::None.inspect(|x| panic!(\"should not run: {x}\"))", "MyOption::None"),
        T("inspect_does_not_move_the_value", "Some(String::from(\"hi\")).inspect(record the pointer)", '{ let s = String::from("hi"); let p = s.as_ptr(); let mut seen = None; let o = MyOption::Some(s).inspect(|v| seen = Some(v.as_ptr())); (seen == Some(p), o) }', '(true, MyOption::Some("hi".to_string()))'),
        T("map_or_else_some_skips_default", "Some(4)", "MyOption::Some(4).map_or_else(|| panic!(\"should not run\"), |x| x * 2)", "8"),
        T("map_or_else_none_skips_f", "None", "MyOption::<i32>::None.map_or_else(|| 0, |x| -> i32 { panic!(\"should not run: {x}\") })", "0"),
        T("map_or_else_moves_the_value", "Some(String::from(\"abc\"))", 'MyOption::Some(String::from("abc")).map_or_else(String::new, |s| s + "!")', '"abc!".to_string()'),
        """
        fn mine<T>(o: Option<T>) -> MyOption<T> {
            match o {
                Some(v) => MyOption::Some(v),
                None => MyOption::None,
            }
        }

        #[test]
        fn random_vs_std_option() {
            let mut rng = anneal_prelude::Rng::new(1316);
            for _ in 0..300 {
                let a = if rng.bool() { Some(rng.int(-5, 5) as i32) } else { None };
                let b = if rng.bool() { Some(rng.int(-5, 5) as i32) } else { None };
                let d = rng.int(-5, 5) as i32;
                let desc = format!("a = {a:?}, b = {b:?}, d = {d}");
                let m = mine(a);
                check!(format!("as_ref, {desc}"), m.as_ref(), mine(a.as_ref()));
                let (mut m, mut s) = (mine(a), a);
                if let MyOption::Some(x) = m.as_mut() {
                    *x += d;
                }
                if let Some(x) = s.as_mut() {
                    *x += d;
                }
                check!(format!("as_mut then += d, {desc}"), m, mine(s));
                let (mut m, mut s) = (mine(a), a);
                check!(format!("replace(d), {desc}"), (m.replace(d), m), (mine(s.replace(d)), mine(s)));
                let (mut m, mut s) = (mine(a), a);
                let (mut calls_m, mut calls_s) = (0, 0);
                let got = *m.get_or_insert_with(|| { calls_m += 1; d });
                let want = *s.get_or_insert_with(|| { calls_s += 1; d });
                check!(format!("get_or_insert_with(d), {desc}"), (got, m, calls_m), (want, mine(s), calls_s));
                check!(format!("a.zip(b), {desc}"), mine(a).zip(mine(b)), mine(a.zip(b)));
                let (mut seen_m, mut seen_s) = (Vec::new(), Vec::new());
                check!(format!("inspect(log), {desc}"), mine(a).inspect(|x| seen_m.push(*x)), mine(a.inspect(|x| seen_s.push(*x))));
                check!(format!("inspect calls, {desc}"), seen_m, seen_s);
                check!(format!("map_or_else(d, x * 3), {desc}"), mine(a).map_or_else(|| d, |x| x * 3), a.map_or_else(|| d, |x| x * 3));
                let (mut m, mut s) = (mine(a), a);
                check!(format!("insert(d), {desc}"), (*m.insert(d), m), (*s.insert(d), mine(s)));
                let nested = if rng.bool() { Some(b) } else { None };
                check!(format!("flatten, nested = {nested:?}"), mine(nested.map(mine)).flatten(), mine(nested.flatten()));
                check!(format!("as_ref().copied(), {desc}"), mine(a).as_ref().copied(), mine(a.as_ref().copied()));
            }
        }
        """,
    ],
    wrong=dict(
        eager_get_or_insert_with=MY_OPTION_BORROW.replace("if let MyOption::None = self {\n                    *self = MyOption::Some(f());", "let v = f();\n                if let MyOption::None = self {\n                    *self = MyOption::Some(v);"),
        inspect_drops_the_value=MY_OPTION_BORROW.replace("                    f(v);\n                }\n                self", "                    f(v);\n                }\n                MyOption::None"),
        insert_keeps_the_old_value=MY_OPTION_BORROW.replace("*self = MyOption::Some(value);\n                match self {", "if let MyOption::None = self {\n                    *self = MyOption::Some(value);\n                }\n                match self {"),
    ),
    hints=[("rust", "Matching on `&self` binds `v` as `&T` (match ergonomics), so `as_ref` is the same two-arm `match` as `map`."),
           ("rust", "In `get_or_insert_with`, first write `*self = MyOption::Some(f())` if it's `None`, then `match self` and return the `&mut` from the `Some` arm."),
           ("edge case", "`insert` always overwrites; only `get_or_insert_with` keeps an existing value. `inspect` hands the closure a reference and returns `self` unchanged.")],
    notes=("`as_ref` and `as_mut` are why most combinators can take `self` by value: borrow first, then consume the borrowed option. `get_or_insert_with` fills the slot before borrowing it. The other order, returning early from `if let MyOption::Some(v) = self.as_mut()` and assigning after it, is sound but today's borrow checker rejects it (E0506). One `match self` that assigns inside the `None` arm is accepted. API reminder: `insert(v)` overwrites, `get_or_insert(v)` / `get_or_insert_with(f)` keep what's there; `copied()` / `cloned()` turn `Option<&T>` into `Option<T>`; `flatten()` removes one level of `Option<Option<T>>`; `inspect(|v| log(v))` peeks without consuming (`Result` has `inspect` and `inspect_err`).", "O(1) each", "O(1)"),
    follow_up="Why does std's `get_or_insert_with` use `unreachable_unchecked` (or an equivalent) for the `None` arm, and is `unreachable!()` good enough here?",
    related=["L2", "L6"],
))

MY_RESULT = """
        #[derive(Debug, PartialEq, Eq)]
        pub enum MyResult<T, E> {
            Ok(T),
            Err(E),
        }

        impl<T, E> MyResult<T, E> {
            pub fn map<U>(self, f: impl FnOnce(T) -> U) -> MyResult<U, E> {
                match self {
                    MyResult::Ok(v) => MyResult::Ok(f(v)),
                    MyResult::Err(e) => MyResult::Err(e),
                }
            }
            pub fn map_err<F>(self, f: impl FnOnce(E) -> F) -> MyResult<T, F> {
                match self {
                    MyResult::Ok(v) => MyResult::Ok(v),
                    MyResult::Err(e) => MyResult::Err(f(e)),
                }
            }
            pub fn and_then<U>(self, f: impl FnOnce(T) -> MyResult<U, E>) -> MyResult<U, E> {
                match self {
                    MyResult::Ok(v) => f(v),
                    MyResult::Err(e) => MyResult::Err(e),
                }
            }
            pub fn is_ok_and(self, f: impl FnOnce(T) -> bool) -> bool {
                match self {
                    MyResult::Ok(v) => f(v),
                    MyResult::Err(_) => false,
                }
            }
            pub fn ok(self) -> Option<T> {
                match self {
                    MyResult::Ok(v) => Some(v),
                    MyResult::Err(_) => None,
                }
            }
        }

        /// Like `?` for `MyResult`: the value on `Ok`, an early return on `Err`.
        #[macro_export]
        macro_rules! try_my {
            ($e:expr) => {
                match $e {
                    $crate::MyResult::Ok(v) => v,
                    $crate::MyResult::Err(e) => return $crate::MyResult::Err(::core::convert::From::from(e)),
                }
            };
        }
    """

P.append(dict(
    slug="my-result-and-try", title="Build MyResult and a ? macro", level="hard", stage="build-it", tags=["macro_rules!", "From", "early return"],
    teaches=["`?` is a match plus an early return plus `From::from` on the error.", "`#[macro_export]` and `$crate` paths.", "`is_ok_and` and `ok` are the `Result` side of `is_some_and` and `ok_or`."],
    statement="""
        Implement `map`, `map_err`, `and_then`, `is_ok_and` and `ok` on `MyResult<T, E>`, and a macro `try_my!(expr)`
        that evaluates to the value on `Ok` and otherwise returns `MyResult::Err(From::from(e))` from
        the enclosing function, like `?`.
    """,
    starter="""
        #[derive(Debug, PartialEq, Eq)]
        pub enum MyResult<T, E> {
            Ok(T),
            Err(E),
        }

        impl<T, E> MyResult<T, E> {
            pub fn map<U>(self, f: impl FnOnce(T) -> U) -> MyResult<U, E> { todo!() }
            pub fn map_err<F>(self, f: impl FnOnce(E) -> F) -> MyResult<T, F> { todo!() }
            pub fn and_then<U>(self, f: impl FnOnce(T) -> MyResult<U, E>) -> MyResult<U, E> { todo!() }
            pub fn is_ok_and(self, f: impl FnOnce(T) -> bool) -> bool { todo!() }
            pub fn ok(self) -> Option<T> { todo!() }
        }

        /// Like `?` for `MyResult`: the value on `Ok`, an early return on `Err`.
        #[macro_export]
        macro_rules! try_my {
            ($e:expr) => {
                match $e {
                    $crate::MyResult::Ok(v) => v,
                    $crate::MyResult::Err(_) => todo!("return the error, converted with From"),
                }
            };
        }
    """,
    solution=MY_RESULT,
    visible=[
        """
        fn add(a: MyResult<i32, String>, b: MyResult<i32, String>) -> MyResult<i32, String> {
            let x = try_my!(a);
            let y = try_my!(b);
            MyResult::Ok(x + y)
        }

        #[test]
        fn try_passes_values_through() {
            check!("Ok(2) + Ok(3)", add(MyResult::Ok(2), MyResult::Ok(3)), MyResult::Ok(5));
        }

        #[test]
        fn try_returns_early() {
            check!("Ok(2) + Err(\\"no\\")", add(MyResult::Ok(2), MyResult::Err("no".into())), MyResult::Err("no".to_string()));
        }
        """,
        T("map", "Ok(2)", "MyResult::<i32, ()>::Ok(2).map(|x| x * 3)", "MyResult::Ok(6)"),
        T("map_err_leaves_ok_alone", "Ok(2)", "MyResult::<i32, i32>::Ok(2).map_err(|e| e + 1)", "MyResult::Ok(2)"),
        T("and_then_chains", "Ok(2)", "MyResult::<i32, String>::Ok(2).and_then(|v| MyResult::Ok(v * 10))", "MyResult::Ok(20)"),
    ],
    hidden=[
        """
        #[derive(Debug, PartialEq)]
        struct AppError(String);

        impl From<&'static str> for AppError {
            fn from(s: &'static str) -> Self {
                AppError(s.to_string())
            }
        }

        fn converts(r: MyResult<u8, &'static str>) -> MyResult<u8, AppError> {
            let v = try_my!(r);
            MyResult::Ok(v)
        }

        #[test]
        fn try_converts_the_error_with_from() {
            check!("Err(\\"bad\\")", converts(MyResult::Err("bad")), MyResult::Err(AppError("bad".into())));
        }
        """,
        T("map_err", "Err(4)", "MyResult::<(), i32>::Err(4).map_err(|e| e + 1)", "MyResult::Err(5)"),
        T("and_then_short_circuits", "Err(\"x\")", 'MyResult::<i32, &str>::Err("x").and_then(|v| MyResult::Ok(v + 1))', 'MyResult::Err("x")'),
        T("map_leaves_err_alone", "Err(\"x\")", 'MyResult::<i32, &str>::Err("x").map(|v| -> i32 { panic!("should not run: {v}") })', 'MyResult::Err("x")'),
        T("map_err_skips_f_on_ok", "Ok(1)", 'MyResult::<i32, i32>::Ok(1).map_err(|e| -> i32 { panic!("should not run: {e}") })', "MyResult::Ok(1)"),
        T("and_then_ok_to_err", "Ok(1)", 'MyResult::<i32, &str>::Ok(1).and_then(|_| MyResult::<i32, &str>::Err("late"))', 'MyResult::Err("late")'),
        T("map_changes_type", "Ok(\"abc\")", 'MyResult::<&str, ()>::Ok("abc").map(str::len)', "MyResult::Ok(3)"),
        T("is_ok_and", "Ok(4), Ok(5), Err(4) with is_ok_and(even)", "(MyResult::<i32, i32>::Ok(4).is_ok_and(|x| x % 2 == 0), MyResult::<i32, i32>::Ok(5).is_ok_and(|x| x % 2 == 0), MyResult::<i32, i32>::Err(4).is_ok_and(|x| panic!(\"should not run: {x}\")))", "(true, false, false)"),
        T("ok_drops_the_error", "Ok(\"a\").ok(), Err(1).ok()", '(MyResult::<&str, i32>::Ok("a").ok(), MyResult::<&str, i32>::Err(1).ok())', '(Some("a"), None)'),
        """
        fn sum3(a: MyResult<i32, String>, b: MyResult<i32, String>, c: MyResult<i32, String>, steps: &mut u32) -> MyResult<i32, String> {
            let x = try_my!(a);
            *steps += 1;
            let y = try_my!(b);
            *steps += 1;
            let z = try_my!(c);
            *steps += 1;
            MyResult::Ok(x + y + z)
        }

        fn once(r: MyResult<i32, String>, calls: &mut u32) -> MyResult<i32, String> {
            *calls += 1;
            r
        }

        fn inline(a: MyResult<i32, String>, b: MyResult<i32, String>) -> MyResult<i32, String> {
            MyResult::Ok(try_my!(a) * 10 + try_my!(b))
        }

        #[test]
        fn try_first_error_wins() {
            let mut steps = 0;
            let got = sum3(MyResult::Ok(1), MyResult::Err("b".into()), MyResult::Err("c".into()), &mut steps);
            check!("Ok(1), Err(\\"b\\"), Err(\\"c\\")", (got, steps), (MyResult::Err("b".to_string()), 1));
        }

        #[test]
        fn try_stops_at_the_first_error() {
            let mut steps = 0;
            let got = sum3(MyResult::Err("a".into()), MyResult::Ok(2), MyResult::Ok(3), &mut steps);
            check!("Err(\\"a\\"), Ok(2), Ok(3)", (got, steps), (MyResult::Err("a".to_string()), 0));
        }

        #[test]
        fn try_evaluates_its_argument_once() {
            fn run(calls: &mut u32) -> MyResult<i32, String> {
                let v = try_my!(once(MyResult::Ok(4), calls));
                MyResult::Ok(v)
            }
            let mut calls = 0;
            let got = run(&mut calls);
            check!("try_my!(once(Ok(4)))", (got, calls), (MyResult::Ok(4), 1));
        }

        #[test]
        fn try_inside_an_expression() {
            check!("Ok(Ok(4) * 10 + Ok(2))", inline(MyResult::Ok(4), MyResult::Ok(2)), MyResult::Ok(42));
            check!("Ok(Ok(4) * 10 + Err(\\"b\\"))", inline(MyResult::Ok(4), MyResult::Err("b".into())), MyResult::Err("b".to_string()));
        }

        fn add_mine(a: MyResult<i32, String>, b: MyResult<i32, String>) -> MyResult<i32, String> {
            MyResult::Ok(try_my!(a) + try_my!(b))
        }

        fn add_std(a: Result<i32, String>, b: Result<i32, String>) -> Result<i32, String> {
            Ok(a? + b?)
        }

        fn mine(r: Result<i32, String>) -> MyResult<i32, String> {
            match r {
                Ok(v) => MyResult::Ok(v),
                Err(e) => MyResult::Err(e),
            }
        }

        #[test]
        fn random_vs_std_result() {
            let mut rng = anneal_prelude::Rng::new(1310);
            for _ in 0..300 {
                let a: Result<i32, String> = if rng.below(3) > 0 { Ok(rng.int(-9, 9) as i32) } else { Err(rng.string(1, "xyz")) };
                let b: Result<i32, String> = if rng.below(3) > 0 { Ok(rng.int(-9, 9) as i32) } else { Err(rng.string(1, "xyz")) };
                let desc = format!("a = {a:?}, b = {b:?}");
                check!(format!("map(x * 2), {desc}"), mine(a.clone()).map(|x| x * 2), mine(a.clone().map(|x| x * 2)));
                check!(format!("map_err(push '!'), {desc}"), mine(a.clone()).map_err(|e| e + "!"), mine(a.clone().map_err(|e| e + "!")));
                check!(format!("and_then(positive), {desc}"), mine(a.clone()).and_then(|x| if x > 0 { MyResult::Ok(x) } else { MyResult::Err("neg".to_string()) }), mine(a.clone().and_then(|x| if x > 0 { Ok(x) } else { Err("neg".to_string()) })));
                check!(format!("is_ok_and(positive), {desc}"), mine(a.clone()).is_ok_and(|x| x > 0), a.clone().is_ok_and(|x| x > 0));
                check!(format!("ok(), {desc}"), mine(a.clone()).ok(), a.clone().ok());
                check!(format!("try_my!(a) + try_my!(b), {desc}"), add_mine(mine(a.clone()), mine(b.clone())), mine(add_std(a, b)));
            }
        }
        """,
    ],
    wrong=dict(
        is_ok_and_true_on_err=MY_RESULT.replace("MyResult::Err(_) => false,", "MyResult::Err(_) => true,"),
        try_panics_on_err=MY_RESULT.replace("$crate::MyResult::Err(e) => return $crate::MyResult::Err(::core::convert::From::from(e)),", "$crate::MyResult::Err(_) => panic!(\"try_my! on an Err\"),"),
    ),
    hints=[("approach", "`?` on `Err(e)` returns `Err(From::from(e))` from the whole function."),
           ("rust", "Inside `macro_rules!`, `return` returns from the function the macro is used in. Name the enum as `$crate::MyResult` so it resolves anywhere.")],
    notes=("The `From::from` call is what lets `?` convert a library error into your application's error type. A `macro_rules!` body is expanded in place, so `return` leaves the caller's function, and `$e` is evaluated exactly once because the macro binds it with `match`. Syntax to remember: `#[macro_export] macro_rules! try_my { ($e:expr) => { match $e { ... } }; }`, `$crate::MyResult`, `::core::convert::From::from(e)`, `res.is_ok_and(|v| ..)`, `res.ok()` / `res.err()`.", "O(1)", "O(1)"),
    follow_up="What trait does real `?` use, and why isn't it stable to implement yourself?",
    related=["L8", "L10"],
))

STAGES = [("use-it", "Use it", "easy"), ("understand-it", "Understand it", "medium"), ("build-it", "Build it", "hard")]

# Review of 2026-11-28 (group B). The folders were edited by hand; these lines bring the same edits
# back so regenerating reproduces them. FORMATTED holds the starter and solution as rustfmt left them
# (commit 37f80727) for the problems the review touched; the rest are the tests, wrong solutions and
# [perf] keys the review added.
FORMATTED = {
    'collect-into-result': (
        r'''
/// Every item as an `i32`, or the error for the first bad one.
pub fn parse_all(items: &[&str]) -> Result<Vec<i32>, String> {
    todo!()
}

/// The sum of every item as an `i64`, or the error for the first bad one. No intermediate `Vec`.
pub fn sum_all(items: &[&str]) -> Result<i64, String> {
    todo!()
}

/// Every item, or every error in order.
pub fn parse_every_error(items: &[&str]) -> Result<Vec<i32>, Vec<String>> {
    todo!()
}

/// Runs `check` on each item in order and stops at the first error.
pub fn validate(
    items: &[&str],
    mut check: impl FnMut(&str) -> Result<(), String>,
) -> Result<(), String> {
    todo!()
}
''',
        r'''
fn parse(s: &str) -> Result<i32, String> {
    s.parse().map_err(|_| format!("bad number: {s}"))
}

/// Every item as an `i32`, or the error for the first bad one.
pub fn parse_all(items: &[&str]) -> Result<Vec<i32>, String> {
    items.iter().map(|s| parse(s)).collect()
}

/// The sum of every item as an `i64`, or the error for the first bad one. No intermediate `Vec`.
pub fn sum_all(items: &[&str]) -> Result<i64, String> {
    items.iter().map(|s| parse(s).map(i64::from)).sum()
}

/// Every item, or every error in order.
pub fn parse_every_error(items: &[&str]) -> Result<Vec<i32>, Vec<String>> {
    let (good, bad): (Vec<_>, Vec<_>) = items.iter().map(|s| parse(s)).partition(Result::is_ok);
    if bad.is_empty() {
        Ok(good.into_iter().flatten().collect())
    } else {
        Err(bad.into_iter().filter_map(Result::err).collect())
    }
}

/// Runs `check` on each item in order and stops at the first error.
pub fn validate(
    items: &[&str],
    mut check: impl FnMut(&str) -> Result<(), String>,
) -> Result<(), String> {
    items.iter().map(|s| check(s)).collect()
}
''',
    ),
    'my-option-borrowing': (
        r'''
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MyOption<T> {
    Some(T),
    None,
}

impl<T> MyOption<T> {
    pub fn as_ref(&self) -> MyOption<&T> {
        todo!()
    }
    pub fn as_mut(&mut self) -> MyOption<&mut T> {
        todo!()
    }
    pub fn insert(&mut self, value: T) -> &mut T {
        todo!()
    }
    pub fn replace(&mut self, value: T) -> MyOption<T> {
        todo!()
    }
    pub fn get_or_insert_with(&mut self, f: impl FnOnce() -> T) -> &mut T {
        todo!()
    }
    pub fn zip<U>(self, other: MyOption<U>) -> MyOption<(T, U)> {
        todo!()
    }
    pub fn inspect(self, f: impl FnOnce(&T)) -> MyOption<T> {
        todo!()
    }
    pub fn map_or_else<U>(self, default: impl FnOnce() -> U, f: impl FnOnce(T) -> U) -> U {
        todo!()
    }
}

impl<T> MyOption<MyOption<T>> {
    pub fn flatten(self) -> MyOption<T> {
        todo!()
    }
}

impl<T: Copy> MyOption<&T> {
    pub fn copied(self) -> MyOption<T> {
        todo!()
    }
}
''',
        r'''
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MyOption<T> {
    Some(T),
    None,
}

impl<T> MyOption<T> {
    pub fn as_ref(&self) -> MyOption<&T> {
        match self {
            MyOption::Some(v) => MyOption::Some(v),
            MyOption::None => MyOption::None,
        }
    }
    pub fn as_mut(&mut self) -> MyOption<&mut T> {
        match self {
            MyOption::Some(v) => MyOption::Some(v),
            MyOption::None => MyOption::None,
        }
    }
    pub fn insert(&mut self, value: T) -> &mut T {
        *self = MyOption::Some(value);
        match self {
            MyOption::Some(v) => v,
            MyOption::None => unreachable!(),
        }
    }
    pub fn replace(&mut self, value: T) -> MyOption<T> {
        std::mem::replace(self, MyOption::Some(value))
    }
    pub fn get_or_insert_with(&mut self, f: impl FnOnce() -> T) -> &mut T {
        if let MyOption::None = self {
            *self = MyOption::Some(f());
        }
        match self {
            MyOption::Some(v) => v,
            MyOption::None => unreachable!(),
        }
    }
    pub fn zip<U>(self, other: MyOption<U>) -> MyOption<(T, U)> {
        match (self, other) {
            (MyOption::Some(a), MyOption::Some(b)) => MyOption::Some((a, b)),
            _ => MyOption::None,
        }
    }
    pub fn inspect(self, f: impl FnOnce(&T)) -> MyOption<T> {
        if let MyOption::Some(v) = &self {
            f(v);
        }
        self
    }
    pub fn map_or_else<U>(self, default: impl FnOnce() -> U, f: impl FnOnce(T) -> U) -> U {
        match self {
            MyOption::Some(v) => f(v),
            MyOption::None => default(),
        }
    }
}

impl<T> MyOption<MyOption<T>> {
    pub fn flatten(self) -> MyOption<T> {
        match self {
            MyOption::Some(inner) => inner,
            MyOption::None => MyOption::None,
        }
    }
}

impl<T: Copy> MyOption<&T> {
    pub fn copied(self) -> MyOption<T> {
        match self {
            MyOption::Some(&v) => MyOption::Some(v),
            MyOption::None => MyOption::None,
        }
    }
}
''',
    ),
}
ADDED_HIDDEN = {
    'collect-into-result': [r'''
#[test]
fn sum_all_builds_no_vec() {
    let items: Vec<String> = (0..10_000).map(|i: i32| i.to_string()).collect();
    let refs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
    let (got, used) = anneal_prelude::allocs(|| sum_all(&refs));
    check!("10000 good items: the sum", got, Ok(49_995_000));
    check!(format!("sum_all allocated {} bytes in {} allocations, at most 1024", used.bytes, used.count), used.bytes <= 1024, true);
}
'''],
}
ADDED_WRONG = {
    'collect-into-result': {
        'sum_builds_a_vec': r'''
fn parse(s: &str) -> Result<i32, String> {
    s.parse().map_err(|_| format!("bad number: {s}"))
}

/// Every item as an `i32`, or the error for the first bad one.
pub fn parse_all(items: &[&str]) -> Result<Vec<i32>, String> {
    items.iter().map(|s| parse(s)).collect()
}

/// The sum of every item as an `i64`, or the error for the first bad one. No intermediate `Vec`.
pub fn sum_all(items: &[&str]) -> Result<i64, String> {
    // Builds the whole Vec first, which the statement rules out.
    Ok(parse_all(items)?.into_iter().map(i64::from).sum())
}

/// Every item, or every error in order.
pub fn parse_every_error(items: &[&str]) -> Result<Vec<i32>, Vec<String>> {
    let (good, bad): (Vec<_>, Vec<_>) = items.iter().map(|s| parse(s)).partition(Result::is_ok);
    if bad.is_empty() {
        Ok(good.into_iter().flatten().collect())
    } else {
        Err(bad.into_iter().filter_map(Result::err).collect())
    }
}

/// Runs `check` on each item in order and stops at the first error.
pub fn validate(
    items: &[&str],
    mut check: impl FnMut(&str) -> Result<(), String>,
) -> Result<(), String> {
    items.iter().map(|s| check(s)).collect()
}
''',
    },
}
ADDED_PERF = {'collect-into-result': {'allocs': True}}
for p in P:
    s = p["slug"]
    if s in FORMATTED:
        p["starter"], p["solution"] = FORMATTED[s]
    if s in ADDED_HIDDEN:
        p["hidden"] = list(p["hidden"]) + ADDED_HIDDEN[s]
    if s in ADDED_WRONG:
        p["wrong"] = {**(p.get("wrong") or {}), **ADDED_WRONG[s]}
    if s in ADDED_PERF:
        p["perf"] = ADDED_PERF[s]


if __name__ == "__main__":
    n = write_track("s1-option-result", "S1", "Option & Result", "S", "core", 1,
                    "The two types every Rust API returns: combinators, `?`, conversions, and building them yourself.",
                    STAGES, P)
    print("S1", n)
