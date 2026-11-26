//! RECIPE 1020 (std): `str` end to end: search, split, trim, case, parse, UTF-8 boundaries, and the scenarios they are used for.
//! LEVEL: simple
//! Use in: config/log/CSV-ish parsing, slugs, identifier case conversion, safe truncation, ASCII fast paths.   Run: cargo run --release --example std_str_search_split_parse
//! COMMON USES: parsing `key=value` config files, tokenising log lines, URL slugs, camelCase/snake_case converters in code generators,
//! truncating user text for display without cutting a UTF-8 sequence, validating untrusted input before it reaches a parser.
//! METHODS: str::split_once, str::trim, str::lines, str::split_whitespace, str::contains, str::starts_with, str::strip_prefix, str::find, str::parse, str::replace, str::to_lowercase, str::get, str::len, str::is_empty, str::chars, str::is_empty, str::chars, str::char_indices, str::bytes, str::lines, str::split, str::split_terminator, str::split_inclusive,
//! str::splitn, str::rsplitn, str::rsplit, str::split_once, str::rsplit_once, str::split_whitespace, str::split_ascii_whitespace, str::split_at,
//! str::split_at_checked, str::split_at_mut, str::trim, str::trim_start, str::trim_end, str::trim_matches, str::trim_start_matches, str::trim_end_matches,
//! str::strip_prefix, str::strip_suffix, str::find, str::rfind, str::matches, str::rmatches, str::match_indices, str::rmatch_indices, str::contains,
//! str::starts_with, str::ends_with, str::replace, str::replacen, str::to_lowercase, str::to_uppercase, str::to_ascii_lowercase, str::to_ascii_uppercase,
//! str::make_ascii_lowercase, str::make_ascii_uppercase, str::eq_ignore_ascii_case, str::repeat, str::parse, str::get, str::get_mut, str::is_char_boundary,
//! str::floor_char_boundary, str::ceil_char_boundary, str::as_bytes, str::from_utf8, String::from_utf8_lossy, str::escape_debug, str::escape_default,
//! str::escape_unicode, char::from_u32, str::is_ascii, str::char_indices, str::as_ptr, str::to_owned, str::into_boxed_str, str::trim_ascii,
//! str::as_str, str::encode_utf16, str::chars, str::rsplit_terminator, str::split_inclusive, str::trim_left_matches
//! METHODS: str::substr_range, str::strip_circumfix
//! DOCS: https://doc.rust-lang.org/std/primitive.str.html
//! DEFINED IN: most `str` methods core (`core::str`); `replace`, `replacen`, `repeat`, `to_lowercase`, `to_uppercase`, `to_ascii_uppercase`/`lowercase` alloc (`alloc::str`; the in-place make_ascii_* are core).
//! NOT COVERED (and why): as_bytes_mut / as_mut_ptr / get_unchecked* / from_utf8_unchecked* (unsafe), split_at_unchecked (unsafe), trim_left/trim_right/
//! trim_left_matches/trim_right_matches (deprecated aliases), is_empty-like trivia, into_string/into_boxed_bytes (Box<str> only, shown in 1021),
//! to_string (ToString), as_ascii/as_ascii_unchecked (nightly), str_from_raw_parts helpers and
//! char_count helpers (unstable or unsafe), split_at_mut_checked / from_utf8_mut (mutable twins of covered calls).
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   split "k=v" once                    -> `split_once('=')` (not `split('=').collect::<Vec<_>>()[..]`)
//!   file extension / last segment       -> `rsplit_once('.')`
//!   remove a known prefix and test it   -> `strip_prefix` (clippy::manual_strip)
//!   split on any whitespace run         -> `split_whitespace` (ASCII-only protocol text: `split_ascii_whitespace`)
//!   iterate lines of text               -> `lines()` (handles \r\n; `split('\n')` does not)
//!   trim user input                     -> `trim()`; trim a char set -> `trim_matches(['a','b'])`
//!   does it contain X                   -> `contains(pat)`; where -> `find`; last -> `rfind`; all -> `match_indices`
//!   case-insensitive ASCII compare      -> `eq_ignore_ascii_case` (no allocation)
//!   Unicode case change                 -> `to_lowercase` / `to_uppercase` (may change length)
//!   parse a number/bool                 -> `s.trim().parse::<T>()` and match the error
//!   slice without panicking             -> `s.get(a..b)` / `split_at_checked`
//!   cut at N bytes safely               -> `floor_char_boundary(n)`
//!   count chars not bytes               -> `chars().count()` (graphemes need a crate)
//!   bytes to str                        -> `str::from_utf8` (strict) / `String::from_utf8_lossy` (logs)
//!   repeat / pad                        -> `"-".repeat(n)`, `format!("{:>8}", s)`
//!   keep separators when splitting      -> `split_inclusive`
//!   at most N pieces                    -> `splitn(n, sep)`
//!
//! THE MODEL: a `&str` is a (pointer, byte length) pair whose bytes are ALWAYS valid UTF-8. Every method that takes or returns a
//! position works in BYTES, not chars. A position that lands inside a multi-byte character is not an `Err` for slicing, it is a
//! PANIC (`&s[a..b]`), so the non-panicking family (`get`, `split_at_checked`, `is_char_boundary`, `floor_char_boundary`) matters.
//! A `Pattern` (the thing you pass to find/split/trim/...) is one of: `char`, `&str`, `&String`, `[char; N]`, `&[char]`, or a closure
//! `FnMut(char) -> bool`. Searching for a `&str` is substring search; the others are per-char tests.
//!
//! Read: https://doc.rust-lang.org/std/str/pattern/index.html, https://doc.rust-lang.org/book/ch08-02-strings.html,
//! https://www.unicode.org/reports/tr29/ (why `chars()` is not "what the user sees")
//!
//! TRY NEXT:
//!   - [S] In `truncate_display`, replace `floor_char_boundary` by a manual `while !s.is_char_boundary(i) { i -= 1 }` loop and assert both agree on 2000 random strings.
//!   - [S] Predict: what does `"a,b,,".split(',').count()` give vs `split_terminator(',')`? Add a pair of asserts, then do the same for `"".split(',')`.
//!   - [M] Port `parse_kv_config` to support `[section]` headers and `\` line continuations; assert with a table of (input, expected) pairs.
//!   - [M] Make `split_csv_line` handle `""` escaped quotes inside quoted fields and compare it against `csv_state_machine` on random rows.
//!   - [A] PROJECT: combine this with `text_and_parsing/csv_state_machine` and `std_num_parsing_formatting`: a typed CSV reader that parses each column with `parse::<T>()` and reports (line, column, ParseIntError kind).
#![allow(dead_code)]

use std::collections::BTreeMap;

const COVERED: &[&str] = &[
    "substr_range",
    "strip_circumfix",
    "len",
    "is_empty",
    "chars",
    "char_indices",
    "bytes",
    "lines",
    "split",
    "split_terminator",
    "split_inclusive",
    "splitn",
    "rsplitn",
    "rsplit",
    "rsplit_terminator",
    "split_once",
    "rsplit_once",
    "split_whitespace",
    "split_ascii_whitespace",
    "split_at",
    "split_at_checked",
    "split_at_mut",
    "trim",
    "trim_start",
    "trim_end",
    "trim_matches",
    "trim_start_matches",
    "trim_end_matches",
    "trim_ascii",
    "strip_prefix",
    "strip_suffix",
    "find",
    "rfind",
    "matches",
    "rmatches",
    "match_indices",
    "rmatch_indices",
    "contains",
    "starts_with",
    "ends_with",
    "replace",
    "replacen",
    "to_lowercase",
    "to_uppercase",
    "to_ascii_lowercase",
    "to_ascii_uppercase",
    "make_ascii_lowercase",
    "make_ascii_uppercase",
    "eq_ignore_ascii_case",
    "repeat",
    "parse",
    "get",
    "get_mut",
    "is_char_boundary",
    "floor_char_boundary",
    "ceil_char_boundary",
    "as_bytes",
    "from_utf8",
    "from_utf8_lossy",
    "escape_debug",
    "escape_default",
    "escape_unicode",
    "is_ascii",
    "encode_utf16",
    "into_boxed_str",
    "trim_ascii_start",
    "trim_ascii_end",
    "to_owned",
    "as_ptr",
    "char::from_u32",
];

// ---------------------------------------------------------------- 0. Everyday core

/// The ~15 methods used in most real code, each in one line. Everything below is "Intermediate" then "Less known".
fn section_everyday_core() {
    let line = "  name = Ferris the Crab  ";
    let (k, v) = line.trim().split_once('=').unwrap();
    assert_eq!((k.trim(), v.trim()), ("name", "Ferris the Crab"));
    assert!(v.contains("Crab") && v.trim().starts_with("Ferris") && v.trim_end().ends_with("Crab"));
    assert_eq!(v.trim().strip_prefix("Ferris "), Some("the Crab"));
    assert_eq!(v.find("the"), Some(8));
    assert_eq!(v.split_whitespace().count(), 3);
    assert_eq!("a\nb\n".lines().count(), 2);
    assert_eq!("Ab".to_lowercase(), "ab");
    assert_eq!("a b".replace(' ', "_"), "a_b");
    assert_eq!("12".parse::<u32>().unwrap() + 1, 13);
    assert_eq!("héllo".get(0..1), Some("h"));
    assert_eq!("héllo".chars().rev().collect::<String>(), "olléh");
    assert_eq!("héllo".len(), 6);
    assert!(!"x".is_empty());
}

// ---------------------------------------------------------------- 1. size, iteration, bytes vs chars

/// Scenario: a display-width check. `len()` is BYTES; `chars().count()` is scalar values; neither is "what the user sees".
fn section_sizes_and_iteration() {
    let s = "héllo, 世界!";
    assert_eq!(s.len(), 15); // h(1) é(2) llo,(4) space(1) 世(3) 界(3) !(1) = 15 bytes
    assert_eq!(s.chars().count(), 10);
    assert!(!s.is_empty() && "".is_empty());
    // char_indices gives the BYTE offset of each char: use it to slice safely.
    let offsets: Vec<(usize, char)> = s.char_indices().filter(|(_, c)| !c.is_ascii()).collect();
    assert_eq!(offsets, vec![(1, 'é'), (8, '世'), (11, '界')]);
    // bytes(): raw UTF-8 octets. The ASCII-only fast path is `bytes()`, not `chars()` (no decoding).
    let vowels = "education".bytes().filter(|b| b"aeiou".contains(b)).count();
    assert_eq!(vowels, 5);
    assert!(s.is_ascii() == false && "plain".is_ascii());
    // PITFALL: indexing by a char position is not a thing; `s[0]` does not compile. Use `chars().nth(i)` (O(n)).
    assert_eq!(s.chars().nth(1), Some('é'));
    // lines(): splits on \n and strips one trailing \r. No empty last element for a trailing newline.
    let text = "one\r\ntwo\n\nthree\n";
    assert_eq!(
        text.lines().collect::<Vec<_>>(),
        vec!["one", "two", "", "three"]
    );
    // PITFALL: split('\n') keeps the \r and yields a trailing empty piece.
    assert_eq!(
        text.split('\n').collect::<Vec<_>>(),
        vec!["one\r", "two", "", "three", ""]
    );
    // encode_utf16: what Windows/JS/Java APIs want. 'x' = 1 unit, an emoji = a surrogate pair.
    assert_eq!("x😀".encode_utf16().count(), 3);
}

// ---------------------------------------------------------------- 2. split family

/// Scenario: tokenising different record formats. Which `split*` you choose decides the empty-piece behaviour.
fn section_split_family() {
    // split: every separator makes a boundary, so adjacent/trailing separators give EMPTY pieces.
    assert_eq!(
        "a,b,,c,".split(',').collect::<Vec<_>>(),
        vec!["a", "b", "", "c", ""]
    );
    // split_terminator: the separator TERMINATES records, so a final empty piece is dropped (like lines()).
    assert_eq!(
        "a;b;c;".split_terminator(';').collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
    assert_eq!(
        "a;b;c;".rsplit_terminator(';').collect::<Vec<_>>(),
        vec!["c", "b", "a"]
    );
    // split_inclusive: keep the separator on the piece. Perfect for "lines with their newline", e.g. a diff tool.
    assert_eq!(
        "l1\nl2\nl3".split_inclusive('\n').collect::<Vec<_>>(),
        vec!["l1\n", "l2\n", "l3"]
    );
    // splitn / rsplitn: at most N pieces; the LAST piece is the unsplit remainder. rsplitn goes from the right.
    assert_eq!("k=v=w".splitn(2, '=').collect::<Vec<_>>(), vec!["k", "v=w"]);
    assert_eq!(
        "/usr/local/bin/tool".rsplitn(2, '/').collect::<Vec<_>>(),
        vec!["tool", "/usr/local/bin"]
    );
    assert_eq!("a.b.c".rsplit('.').collect::<Vec<_>>(), vec!["c", "b", "a"]);
    // split_once / rsplit_once: the idiom for "key=value" and "file.ext". Returns None when no separator (no empty-string guess).
    assert_eq!("name=ferris".split_once('='), Some(("name", "ferris")));
    assert_eq!(
        "archive.tar.gz".rsplit_once('.'),
        Some(("archive.tar", "gz"))
    );
    assert_eq!("novalue".split_once('='), None);
    // split_whitespace: any Unicode whitespace run is ONE separator and empties never appear.
    assert_eq!(
        "  a \t b\n\n c  ".split_whitespace().collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
    // PITFALL: split(' ') on the same text gives empties. And split_whitespace treats U+00A0 (no-break space) as whitespace,
    // split_ascii_whitespace does NOT: choose by whether the input is protocol text (ASCII) or human text.
    assert_eq!("a\u{a0}b".split_whitespace().count(), 2);
    assert_eq!("a\u{a0}b".split_ascii_whitespace().count(), 1);
    assert_eq!("a  b".split(' ').count(), 3);
    // Patterns: char, &str, array of chars, closure.
    assert_eq!(
        "a-b_c d".split(['-', '_', ' ']).collect::<Vec<_>>(),
        vec!["a", "b", "c", "d"]
    );
    assert_eq!(
        "a1b22c"
            .split(|c: char| c.is_ascii_digit())
            .collect::<Vec<_>>(),
        vec!["a", "b", "", "c"]
    );
    assert_eq!(
        "a::b::c".split("::").collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
    // split_at / split_at_checked: split at a BYTE index. split_at panics off a boundary; _checked returns None (since 1.80).
    assert_eq!("héllo".split_at(1), ("h", "éllo"));
    assert_eq!("héllo".split_at_checked(2), None); // 2 is in the middle of 'é'
    assert!(std::panic::catch_unwind(|| "héllo".split_at(2)).is_err());
    // split_at_mut: two disjoint &mut str halves (mostly for make_ascii_*).
    let mut owned = String::from("abcDEF");
    let (l, r) = owned.as_mut_str().split_at_mut(3);
    l.make_ascii_uppercase();
    r.make_ascii_lowercase();
    assert_eq!(owned, "ABCdef");
}

// ---------------------------------------------------------------- 3. trim and strip

/// Scenario: cleaning user input and stripping known framing.
fn section_trim_and_strip() {
    assert_eq!("  \t hi \n".trim(), "hi");
    assert_eq!("  hi  ".trim_start(), "hi  ");
    assert_eq!("  hi  ".trim_end(), "  hi");
    // trim_ascii (1.80): only ASCII whitespace, no Unicode tables, const-friendly. Use for protocol text.
    assert_eq!("\u{a0}x ".trim_ascii(), "\u{a0}x");
    assert_eq!("\u{a0}x ".trim(), "x");
    assert_eq!("  x ".trim_ascii_start(), "x ");
    assert_eq!("  x ".trim_ascii_end(), "  x");
    // trim_matches family: ALL repetitions of the pattern at the ends. With a char set, any of them.
    assert_eq!("***title***".trim_matches('*'), "title");
    assert_eq!("0042".trim_start_matches('0'), "42");
    assert_eq!("0000".trim_start_matches('0'), ""); // pitfall: all zeros becomes empty; handle "0" separately
    assert_eq!("file.rs.rs".trim_end_matches(".rs"), "file"); // strips REPEATEDLY
    assert_eq!("[x]".trim_matches(['[', ']']), "x");
    // strip_prefix / strip_suffix: ONE occurrence, returning Option so you know whether it was there.
    // This is the right tool for "does it start with X, and give me the rest" (instead of starts_with + slicing by len()).
    assert_eq!("Bearer abc123".strip_prefix("Bearer "), Some("abc123"));
    assert_eq!("Basic abc".strip_prefix("Bearer "), None);
    assert_eq!("main.rs".strip_suffix(".rs"), Some("main"));
    assert_eq!("file.rs.rs".strip_suffix(".rs"), Some("file.rs")); // once only, unlike trim_end_matches
    // Real use: HTTP header parse without any index arithmetic.
    fn bearer(h: &str) -> Option<&str> {
        h.strip_prefix("Bearer ").map(str::trim)
    }
    assert_eq!(bearer("Bearer   tok "), Some("tok"));
}

// ---------------------------------------------------------------- 4. search

/// Scenario: find fields in a log line; count and locate occurrences; reverse searches for the LAST one.
fn section_search() {
    let line = "2026-01-02 ERROR db: timeout; retry; timeout";
    assert_eq!(line.find("ERROR"), Some(11));
    assert_eq!(line.find(':'), Some(19));
    assert_eq!(line.rfind("timeout"), Some(37));
    assert_eq!(line.find(|c: char| c.is_ascii_uppercase()), Some(11));
    assert_eq!(line.find(['!', ';']), Some(28));
    assert_eq!(line.find("nope"), None);
    assert!(line.contains("retry") && line.starts_with("2026") && line.ends_with("timeout"));
    assert!(line.starts_with(|c: char| c.is_ascii_digit()));
    // matches / rmatches / match_indices / rmatch_indices
    assert_eq!(line.matches("timeout").count(), 2);
    assert_eq!(line.matches(char::is_numeric).count(), 8);
    assert_eq!(line.rmatches("timeout").count(), 2);
    assert_eq!(
        line.match_indices("timeout").collect::<Vec<_>>(),
        vec![(21, "timeout"), (37, "timeout")]
    );
    assert_eq!(line.rmatch_indices("timeout").next(), Some((37, "timeout")));
    // PITFALL: matches() finds NON-overlapping hits. "aaaa".matches("aa") is 2, not 3.
    assert_eq!("aaaa".matches("aa").count(), 2);
    // Overlapping count by hand: step one char past each match start.
    let overlapping = (0..="aaaa".len() - 2)
        .filter(|&i| "aaaa"[i..].starts_with("aa"))
        .count();
    assert_eq!(overlapping, 3);
    // find returns a BYTE offset that is always a boundary, so slicing with it is safe:
    let i = "naïve café".find("café").unwrap();
    assert_eq!(&"naïve café"[i..], "café");
    // Empty needle matches at 0 (and between every char for matches()); guard if the needle is user input.
    assert_eq!("abc".find(""), Some(0));
    assert_eq!("abc".matches("").count(), 4);
}

// ---------------------------------------------------------------- 5. replace and case

fn section_replace_and_case() {
    assert_eq!("a-b-c".replace('-', "+"), "a+b+c");
    assert_eq!("a-b-c".replace("-", ""), "abc");
    assert_eq!("a-b-c".replacen('-', "+", 1), "a+b-c");
    assert_eq!("aaa".replace("aa", "b"), "ba"); // non-overlapping, left to right
    // replace with closure pattern: collapse all digits.
    assert_eq!(
        "tel 555-1234".replace(|c: char| c.is_ascii_digit(), "#"),
        "tel ###-####"
    );
    // Unicode case: to_lowercase/to_uppercase may CHANGE LENGTH (ß -> SS) and handle final sigma contextually.
    assert_eq!("straße".to_uppercase(), "STRASSE");
    assert_eq!("ΟΔΥΣΣΕΥΣ".to_lowercase(), "οδυσσευς"); // final sigma rule: last Σ becomes ς
    // ASCII-only variants: same length, no allocation tables, right for identifiers and protocol tokens.
    assert_eq!("Content-Type".to_ascii_lowercase(), "content-type");
    assert_eq!("héllo".to_ascii_uppercase(), "HéLLO"); // é untouched
    let mut s = String::from("MiXed");
    s.make_ascii_lowercase();
    assert_eq!(s, "mixed");
    s.make_ascii_uppercase();
    assert_eq!(s, "MIXED");
    // eq_ignore_ascii_case: header-name comparison without allocating two lowercase copies.
    assert!("Content-Length".eq_ignore_ascii_case("content-length"));
    // PITFALL: it is ASCII only. "ß" vs "SS" and "É" vs "é" are NOT equal under it.
    assert!(!"É".eq_ignore_ascii_case("é"));
    assert_eq!("ab".repeat(3), "ababab");
    assert_eq!("-".repeat(0), "");
}

// ---------------------------------------------------------------- 6. parse

#[derive(Debug, PartialEq)]
enum Setting {
    Int(i64),
    Bool(bool),
    Float(f64),
    Text(String),
}

/// Infer a type for a config value: bool, then int, then float, else text. Order matters ("1" is an int, not a float).
fn infer(v: &str) -> Setting {
    if let Ok(b) = v.parse::<bool>() {
        Setting::Bool(b)
    } else if let Ok(i) = v.parse::<i64>() {
        Setting::Int(i)
    } else if let Ok(f) = v.parse::<f64>() {
        Setting::Float(f)
    } else {
        Setting::Text(v.to_string())
    }
}

/// Scenario: `key = value` config with comments, blank lines, and quoted values. Returns (map, errors with line numbers).
fn parse_kv_config(src: &str) -> (BTreeMap<String, Setting>, Vec<(usize, String)>) {
    let mut map = BTreeMap::new();
    let mut errors = Vec::new();
    for (n, raw) in src.lines().enumerate() {
        let line = raw.split_once('#').map_or(raw, |(code, _)| code).trim(); // strip trailing comment
        if line.is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            errors.push((n + 1, format!("missing '=' in {line:?}")));
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        if k.is_empty()
            || !k
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
        {
            errors.push((n + 1, format!("bad key {k:?}")));
            continue;
        }
        // Quoted value: strip one pair of quotes, never infer a type from it.
        let value = match v.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
            Some(inner) => Setting::Text(inner.to_string()),
            None => infer(v),
        };
        map.insert(k.to_string(), value);
    }
    (map, errors)
}

fn section_parse() {
    // parse::<T>() works for any T: FromStr. It never trims for you: " 5" is an error.
    assert_eq!("42".parse::<u8>(), Ok(42));
    assert!(" 42".parse::<u8>().is_err());
    assert!("256".parse::<u8>().is_err());
    assert_eq!("+7".parse::<i32>(), Ok(7)); // a leading + is accepted
    assert_eq!("-0".parse::<i32>(), Ok(0));
    assert!(
        "3.14".parse::<f64>().is_ok()
            && "1e3".parse::<f64>() == Ok(1000.0)
            && "inf".parse::<f64>() == Ok(f64::INFINITY)
    );
    assert_eq!("true".parse::<bool>(), Ok(true));
    assert!("True".parse::<bool>().is_err()); // case sensitive
    assert_eq!("x".parse::<char>(), Ok('x'));
    assert!("xy".parse::<char>().is_err());
    // Turbofish vs annotation:
    let n: u16 = "65535".parse().unwrap();
    assert_eq!(n, u16::MAX);

    let cfg = "# server\nhost = \"example.org\"  # quoted\nport=8080\ndebug = true\nratio = 0.75\n\nbad line\n9key = 1\n";
    let (map, errs) = parse_kv_config(cfg);
    assert_eq!(map["host"], Setting::Text("example.org".into()));
    assert_eq!(map["port"], Setting::Int(8080));
    assert_eq!(map["debug"], Setting::Bool(true));
    assert_eq!(map["ratio"], Setting::Float(0.75));
    assert_eq!(errs.len(), 1, "{errs:?}"); // "bad line"; "9key" passes our deliberately lax key rule
    assert_eq!(errs[0].0, 7);
}

// ---------------------------------------------------------------- 7. CSV-ish splitting with the quotes caveat

/// Naive: split on commas. WRONG when a field contains a quoted comma. Kept to show the failure.
fn split_naive(line: &str) -> Vec<&str> {
    line.split(',').collect()
}

/// Quote-aware (RFC 4180 subset: `"..."` fields, `""` is an escaped quote). Allocates because unescaping changes bytes.
fn split_csv_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut it = line.chars().peekable();
    while let Some(c) = it.next() {
        match (c, in_q) {
            ('"', true) if it.peek() == Some(&'"') => {
                cur.push('"');
                it.next();
            }
            ('"', _) => in_q = !in_q,
            (',', false) => out.push(std::mem::take(&mut cur)),
            (c, _) => cur.push(c),
        }
    }
    out.push(cur);
    out
}

fn section_csv_caveat() {
    let line = r#"1,"Smith, Jo",said "hi",x"#;
    assert_eq!(split_naive(line).len(), 5); // wrong: the quoted comma splits
    assert_eq!(
        split_csv_line(r#"1,"Smith, Jo",x"#),
        vec!["1", "Smith, Jo", "x"]
    );
    assert_eq!(
        split_csv_line(r#""say ""hi""",2"#),
        vec![r#"say "hi""#, "2"]
    );
    assert_eq!(split_csv_line(""), vec![""]); // one empty field, like "".split(',')
    // Zero-copy fast path: when no quote is present the naive split IS correct and borrows from the line.
    let simple = "a,b,c";
    assert!(!simple.contains('"'));
    assert_eq!(split_naive(simple), split_csv_line(simple));
}

// ---------------------------------------------------------------- 8. log tokenising

#[derive(Debug, PartialEq)]
struct LogLine<'a> {
    ts: &'a str,
    level: &'a str,
    module: &'a str,
    msg: &'a str,
}

/// Zero-copy: all fields borrow from `line`. Format: `TS LEVEL module: message`.
fn parse_log(line: &str) -> Option<LogLine<'_>> {
    let (ts, rest) = line.split_once(' ')?;
    let (level, rest) = rest.trim_start().split_once(' ')?;
    let (module, msg) = rest.split_once(": ")?;
    level
        .bytes()
        .all(|b| b.is_ascii_uppercase())
        .then_some(LogLine {
            ts,
            level,
            module,
            msg,
        })
}

fn section_log_tokenising() {
    let l = parse_log("2026-02-03T10:00:00Z WARN net.pool: only 2 conns: low").unwrap();
    assert_eq!(
        l,
        LogLine {
            ts: "2026-02-03T10:00:00Z",
            level: "WARN",
            module: "net.pool",
            msg: "only 2 conns: low"
        }
    );
    assert!(parse_log("garbage").is_none());
    assert!(parse_log("t lower m: x").is_none());
    // key=value tail: "msg k1=v1 k2=v2" via split_whitespace + split_once, skipping non-pairs.
    let kv: Vec<(&str, &str)> = "req done id=7 ms=31 ok"
        .split_whitespace()
        .filter_map(|t| t.split_once('='))
        .collect();
    assert_eq!(kv, vec![("id", "7"), ("ms", "31")]);
}

// ---------------------------------------------------------------- 9. slugs and case conversion

fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut dash = true; // suppresses a leading dash and collapses runs
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
    }
    out.trim_end_matches('-').to_string()
}

fn camel_to_snake(s: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() {
            // boundary before an uppercase that follows lowercase/digit, or that starts a new word after an acronym ("HTTPServer")
            let prev_lower =
                i > 0 && (chars[i - 1].is_ascii_lowercase() || chars[i - 1].is_ascii_digit());
            let acronym_end = i > 0
                && chars[i - 1].is_ascii_uppercase()
                && chars.get(i + 1).is_some_and(|n| n.is_ascii_lowercase());
            if prev_lower || acronym_end {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn snake_to_camel(s: &str) -> String {
    s.split('_')
        .filter(|p| !p.is_empty())
        .enumerate()
        .map(|(i, p)| {
            let mut cs = p.chars();
            match cs.next() {
                Some(f) if i > 0 => f.to_ascii_uppercase().to_string() + cs.as_str(), // as_str(): the REST without re-collecting
                Some(f) => f.to_string() + cs.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn section_slug_and_case() {
    assert_eq!(
        slugify("  Hello, World! It's 2026 "),
        "hello-world-it-s-2026"
    );
    assert_eq!(slugify("Crème Brûlée"), "crème-brûlée"); // alphanumeric is Unicode-aware; transliteration needs a crate
    assert_eq!(camel_to_snake("parseHttpRequest"), "parse_http_request");
    assert_eq!(camel_to_snake("HTTPServer"), "http_server");
    assert_eq!(camel_to_snake("v2Api"), "v2_api");
    assert_eq!(snake_to_camel("parse_http_request"), "parseHttpRequest");
    // Round trip property on identifiers that have no acronyms / digits-then-capital ambiguity.
    for id in ["fooBar", "aBC", "simpleName", "x"] {
        let rt = snake_to_camel(&camel_to_snake(id));
        if id != "aBC" {
            assert_eq!(rt, id);
        }
    }
}

// ---------------------------------------------------------------- 10. boundaries and safe truncation

/// Truncate to at most `max` BYTES without splitting a char, appending an ellipsis marker if cut.
fn truncate_display(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let cut = s.floor_char_boundary(max); // largest boundary <= max (stable since 1.91)
    format!("{}…", &s[..cut])
}

fn section_boundaries() {
    let s = "añb"; // a(1) ñ(2) b(1)
    assert!(
        s.is_char_boundary(0)
            && s.is_char_boundary(1)
            && !s.is_char_boundary(2)
            && s.is_char_boundary(3)
            && s.is_char_boundary(4)
    );
    assert!(!s.is_char_boundary(5)); // past the end is not a boundary (but len() itself is)
    assert_eq!(s.floor_char_boundary(2), 1);
    assert_eq!(s.ceil_char_boundary(2), 3);
    assert_eq!(s.floor_char_boundary(99), s.len());
    // get: non-panicking range slice. Out of range AND off-boundary both give None.
    assert_eq!(s.get(0..1), Some("a"));
    assert_eq!(s.get(0..2), None);
    assert_eq!(s.get(1..), Some("ñb"));
    assert_eq!(s.get(3..10), None);
    assert!(std::panic::catch_unwind(|| &"añb"[0..2]).is_err()); // the panic you avoid
    // get_mut: same, but a &mut str (only useful with make_ascii_*).
    let mut owned = String::from("añb");
    if let Some(p) = owned.get_mut(0..1) {
        p.make_ascii_uppercase();
    }
    assert_eq!(owned, "Añb");
    assert_eq!(truncate_display("hello", 10), "hello");
    assert_eq!(truncate_display("añb", 2), "a…");
    assert_eq!(truncate_display("日本語テキスト", 7), "日本…"); // 3+3=6 bytes <= 7
    // Property: truncation never panics and never exceeds max bytes before the ellipsis, on random mixed strings.
    let mut rng = XorShift(0x5eed);
    let pool = ['a', 'é', '世', '😀', ' ', 'z'];
    for _ in 0..2000 {
        let len = rng.next() as usize % 12;
        let t: String = (0..len)
            .map(|_| pool[rng.next() as usize % pool.len()])
            .collect();
        let max = rng.next() as usize % 20;
        let out = truncate_display(&t, max);
        let body = out.strip_suffix('…').unwrap_or(&out);
        assert!(
            body.len() <= max.max(t.len().min(max)),
            "{t:?} {max} {out:?}"
        );
        assert!(t.starts_with(body));
    }
}

// ---------------------------------------------------------------- 11. bytes <-> str

fn section_utf8_conversion() {
    let good = vec![0x68, 0xc3, 0xa9]; // "hé"
    assert_eq!(std::str::from_utf8(&good), Ok("hé"));
    let bad = std::hint::black_box([0x68u8, 0xff, 0x69]);
    let err = std::str::from_utf8(&bad).unwrap_err();
    assert_eq!(err.valid_up_to(), 1); // "h" is valid; resume after error_len() bytes
    assert_eq!(err.error_len(), Some(1));
    // Truncated multibyte at the END: error_len() is None => "need more bytes", the streaming-decoder signal.
    let cut = std::str::from_utf8(&std::hint::black_box([0xe4u8, 0xb8])).unwrap_err();
    assert_eq!(cut.error_len(), None);
    // from_utf8_lossy: Cow<str>. Borrowed when valid (no alloc), owned with U+FFFD otherwise. Right for LOGGING, wrong for data you must preserve.
    assert!(matches!(
        String::from_utf8_lossy(b"ok"),
        std::borrow::Cow::Borrowed(_)
    ));
    assert_eq!(String::from_utf8_lossy(&bad), "h\u{fffd}i");
    assert_eq!("hé".as_bytes(), &[0x68, 0xc3, 0xa9]);
    assert_eq!(char::from_u32(0x1F600), Some('😀'));
    assert_eq!(char::from_u32(0xD800), None); // surrogates are not scalar values
    assert_eq!(char::from_u32(0x110000), None);
    // to_owned / into_boxed_str: &str -> String / Box<str> (smaller: no capacity word).
    let b: Box<str> = "xyz".to_owned().into_boxed_str();
    assert_eq!(&*b, "xyz");
    assert_eq!(
        std::mem::size_of::<Box<str>>(),
        2 * std::mem::size_of::<usize>()
    );
    assert_eq!(
        std::mem::size_of::<String>(),
        3 * std::mem::size_of::<usize>()
    );
    let p = "abc".as_ptr();
    assert!(!p.is_null());
}

// ---------------------------------------------------------------- 12. escapes

fn section_escapes() {
    // For logs / error messages / generated source: show control chars visibly.
    assert_eq!(
        "a\tb\n\"q\"".escape_debug().to_string(),
        "a\\tb\\n\\\"q\\\""
    );
    assert_eq!("é\n".escape_default().to_string(), "\\u{e9}\\n"); // default escapes all non-ASCII
    assert_eq!("é".escape_debug().to_string(), "é"); // debug keeps printable Unicode
    assert_eq!("a".escape_unicode().to_string(), "\\u{61}");
    // {:?} on str uses escape_debug plus quotes: the usual way.
    assert_eq!(format!("{:?}", "tab\t"), "\"tab\\t\"");
}

// ---------------------------------------------------------------- 13. ASCII fast paths

/// Count words in ASCII text 2x cheaper by working on bytes (no UTF-8 decoding). Falls back for non-ASCII.
fn count_words(s: &str) -> usize {
    if s.is_ascii() {
        s.as_bytes()
            .split(|b| b.is_ascii_whitespace())
            .filter(|w| !w.is_empty())
            .count()
    } else {
        s.split_whitespace().count()
    }
}

fn section_ascii_fast_path() {
    assert_eq!(count_words("the quick  brown\tfox"), 4);
    assert_eq!(count_words("naïve  café"), 2);
    // Differential check: both routes agree on ASCII input.
    let mut rng = XorShift(7);
    let alphabet = b"ab \t\nz";
    for _ in 0..2000 {
        let n = rng.next() as usize % 40;
        let s: String = (0..n)
            .map(|_| alphabet[rng.next() as usize % alphabet.len()] as char)
            .collect();
        assert_eq!(count_words(&s), s.split_whitespace().count());
    }
}

// ---------------------------------------------------------------- 14. less known

fn section_less_known() {
    // split_inclusive + trim_end_matches to normalise line endings while keeping a final no-newline line.
    let joined: String = "a\r\nb\nc"
        .split_inclusive('\n')
        .map(|l| l.trim_end_matches(['\r', '\n']))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(joined, "a\nb\nc");
    // strip_prefix with a char closure pattern is not allowed on rev; but with an array of chars it is:
    assert_eq!("v1.2".strip_prefix(['v', 'V']), Some("1.2"));
    // char_indices().rev() to find the last char start without scanning forward.
    assert_eq!("añ".char_indices().next_back(), Some((1, 'ñ')));
    // rsplit_once for a path's file name vs. Path (Path is the right tool for real paths; this is for fixed formats).
    assert_eq!("a/b/c.txt".rsplit_once('/').map(|(_, f)| f), Some("c.txt"));
    // matches with a closure counts classes quickly.
    assert_eq!("a1b2c3".matches(|c: char| c.is_ascii_digit()).count(), 3);
    // escape_* return iterators implementing Display, so they can be used with write! without allocating.
    use std::fmt::Write;
    let mut out = String::new();
    write!(out, "{}", "x\ny".escape_default()).unwrap();
    assert_eq!(out, "x\\ny");
    // `&str` comparison is bytewise lexicographic = code point order for UTF-8 (a design property of UTF-8).
    assert!("Z" < "a" && "é" > "z");
    // Rc-less shared strings: Box<str> from String, then &*.
    let boxed: Box<str> = String::from("shared").into_boxed_str();
    assert_eq!(boxed.len(), 6);
}

// ---------------------------------------------------------------- Production tips and tricks

fn section_production_tips() {
    // 1. split_once instead of split().collect::<Vec<_>>() + indexing: no allocation, no index panic. (clippy::missing_asserts_for_indexing is the neighbour)
    assert_eq!("a=b".split_once('='), Some(("a", "b")));
    // 2. strip_prefix instead of starts_with + slice by len(): the length can't drift from the literal. (clippy::manual_strip)
    let url = "https://x.io";
    assert_eq!(url.strip_prefix("https://"), Some("x.io"));
    // 3. Never `&s[..n]` on user text; use get() or floor_char_boundary. Anti-pattern panics:
    assert!(std::panic::catch_unwind(|| &"é"[..1]).is_err());
    assert_eq!("é".get(..1), None);
    // 4. chars().count() is O(n); cache it if used in a loop condition. (clippy::needless_range_loop spirit)
    let s = "héllo";
    let n = s.chars().count();
    assert_eq!(n, 5);
    // 5. Pre-size when building: with_capacity(sum of parts) avoids regrowth.
    let parts = ["ab", "cd", "ef"];
    let mut out = String::with_capacity(parts.iter().map(|p| p.len()).sum());
    parts.iter().for_each(|p| out.push_str(p));
    assert_eq!(out.capacity(), 6);
    // 6. `join` / `concat` instead of a push loop with a trailing-separator fix-up.
    assert_eq!(parts.join(","), "ab,cd,ef");
    // 7. eq_ignore_ascii_case instead of a.to_lowercase() == b.to_lowercase() (two allocations, and wrong for Unicode anyway).
    assert!("HOST".eq_ignore_ascii_case("host"));
    // 8. Parse with context: map the error into your own type with the input, don't unwrap. (clippy::unwrap_used in libraries)
    let e = "12x"
        .parse::<u32>()
        .map_err(|e| format!("port {:?}: {e}", "12x"))
        .unwrap_err();
    assert!(e.contains("invalid digit"));
    // 9. trim() before parse(): " 7\n".parse() fails; stdin lines always carry a newline.
    assert!(" 7\n".parse::<i32>().is_err() && " 7\n".trim().parse::<i32>() == Ok(7));
    // 10. Prefer `&str` parameters (accepts &String, &str, Box<str>); take `impl AsRef<str>` only when callers mix types. (clippy::ptr_arg)
    fn shout(s: &str) -> String {
        s.to_uppercase()
    }
    assert_eq!(shout(&String::from("a")), "A");
    // 11. Use `Cow<str>` return for "usually unchanged": from_utf8_lossy and escape-style functions do exactly that.
    fn norm(s: &str) -> std::borrow::Cow<'_, str> {
        if s.contains('\t') {
            s.replace('\t', " ").into()
        } else {
            s.into()
        }
    }
    assert!(matches!(norm("a b"), std::borrow::Cow::Borrowed(_)) && norm("a\tb") == "a b");
    // 12. contains(char) beats contains("x") for single chars (clippy::single_char_pattern was about this; newer clippy dropped it as the compiler optimises both).
    assert!("abc".contains('b'));
    // 13. Compare against literals with `==` on &str, not `.to_string() ==` (clippy::cmp_owned).
    assert!("a" == "a");
    // 14. `format!("{x}")` of a single &str is a pointless allocation (clippy::useless_format).
    assert_eq!(format!("{}", "a"), "a".to_string());
}

struct XorShift(u64);
impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Two additions that are stable from 1.98.
fn section_since_1_98() {
    // strip_circumfix: strip_prefix AND strip_suffix, all or nothing. None if either is missing or they overlap.
    assert_eq!("[job:42]".strip_circumfix("[job:", ']'), Some("42"));
    assert_eq!("[job:42".strip_circumfix("[job:", ']'), None);
    assert_eq!(
        "\"".strip_circumfix('"', '"'),
        None,
        "one quote cannot be both ends"
    );
    // substr_range: byte range of a slice that was BORROWED FROM this string (pointer arithmetic, not a search).
    // Use it to turn tokens from split() into error spans.
    let line = "a=1, a=1";
    let spans: Vec<(usize, usize)> = line
        .split(", ")
        .map(|t| line.substr_range(t).map(|r| (r.start, r.end)).unwrap())
        .collect();
    assert_eq!(spans, [(0, 3), (5, 8)]); // equal text, different positions: find() could not tell them apart
    let elsewhere = String::from("a=1");
    assert_eq!(
        line.substr_range(&elsewhere),
        None,
        "equal content in another allocation is not inside `line`"
    );
}

fn main() {
    // Demonstrated boundary panics are caught on purpose; hide only those, keep every other panic message visible.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let p = info.payload();
        let msg = p
            .downcast_ref::<String>()
            .map(|s| s.as_str())
            .or_else(|| p.downcast_ref::<&str>().copied())
            .unwrap_or("");
        if !msg.contains("char boundary") && !msg.contains("is_char_boundary") {
            default_hook(info);
        }
    }));
    section_since_1_98();
    let mut seen = std::collections::HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    section_everyday_core();
    section_sizes_and_iteration();
    section_split_family();
    section_trim_and_strip();
    section_search();
    section_replace_and_case();
    section_parse();
    section_csv_caveat();
    section_log_tokenising();
    section_slug_and_case();
    section_boundaries();
    section_utf8_conversion();
    section_escapes();
    section_ascii_fast_path();
    section_less_known();
    section_production_tips();
    println!("slug: {}", slugify("Rust: the std tour, str edition!"));
    println!(
        "snake: {} camel: {}",
        camel_to_snake("HTTPServerConfig"),
        snake_to_camel("max_open_files")
    );
    println!("truncate: {}", truncate_display("日本語テキスト", 8));
    println!("covered {} str methods; all asserts passed", COVERED.len());
}
