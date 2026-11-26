//! RECIPE 1106 (crates): regex: captures, replacement, sets, builders, bytes, and the linear-time guarantee
//! LEVEL: intermediate
//! Use in: log parsing, templating, tokenisers, input filters over untrusted text.   Run: cargo run --release -p cb-crate-tour --example regex_text_processing
//! COMMON USES: ripgrep's engine is built on this crate's internals; log pipelines, routing rules, secret scanners (RegexSet).
//! METHODS: Regex::new, Regex::is_match, Regex::find, Regex::find_iter, Regex::find_at, Regex::captures, Regex::captures_iter, Regex::capture_names, Regex::captures_len, Captures::get, Captures::name, Captures::extract, Captures::expand, Match::start, Match::end, Match::range, Match::as_str, Regex::replace, Regex::replace_all, Regex::replacen, regex::NoExpand, Regex::split, Regex::splitn, regex::escape, regex::RegexBuilder, RegexBuilder::case_insensitive, RegexBuilder::multi_line, RegexBuilder::size_limit, regex::RegexSet::new, RegexSet::matches, SetMatches::matched, regex::bytes::Regex, regex::Error, Regex::{is_match_at, shortest_match, capture_locations, captures_read, static_captures_len}, CaptureLocations::get, regex::Replacer, Replacer::{replace_append, no_expansion}, RegexBuilder::{crlf, swap_greed, ignore_whitespace, unicode, line_terminator, nest_limit, dot_matches_new_line}, regex::RegexSetBuilder, RegexSet::{patterns, len}
//! DOCS: https://docs.rs/regex/1/regex/
//! CRATES: regex = "1"
//!
//! CHEAT SHEET
//!   yes/no                              -> is_match (cheapest)
//!   where                               -> find / find_iter ; pieces -> captures / captures_iter
//!   fixed number of groups              -> captures_iter(..).map(|c| c.extract())
//!   rewrite                             -> replace_all with "${name}" or a closure ; literal replacement -> NoExpand
//!   user-supplied literal inside regex  -> regex::escape
//!   which of N patterns match           -> RegexSet (one pass; no positions)
//!   non-UTF-8 input                     -> regex::bytes::Regex
//!   untrusted PATTERN                   -> RegexBuilder::size_limit
//!   compile once                        -> static RE: LazyLock<Regex>
//!   resume a search at an offset        -> find_at / is_match_at (keeps context) ; &text[i..] changes what ^ and \b see
//!   captures in a hot loop              -> capture_locations + captures_read (reuses the slots)
//!   many fixed words                    -> one alternation of escaped words, LONGEST FIRST
//!
//! GUARANTEE AND ITS PRICE. Search time is O(pattern size * text length) for any single search: no catastrophic
//! backtracking. The price: no look-around and no backreferences. (Iterating over ALL matches is a repeated search and
//! has a weaker bound; the docs spell this out under "Untrusted input".)
//!
//! Read: https://docs.rs/regex/latest/regex/#syntax ; https://docs.rs/regex/latest/regex/#untrusted-input ; https://swtch.com/~rsc/regexp/regexp1.html
//!
//! TRY NEXT:
//!   - [S] Predict what `"$1x"` expands to before reading `replacement`; then fix a template that has this bug.
//!   - [S] Replace `static LOG` with `Regex::new` inside the loop and time both on 100k lines.
//!   - [M] Extend the tokenizer with string literals including escapes; fuzz it by re-joining tokens.
//!   - [A] PROJECT: run your own backtracking matcher (below) and `Regex` over growing inputs and plot both on a log scale.
#![allow(dead_code)]
use regex::{Captures, NoExpand, Regex, RegexBuilder, RegexSet};
use std::collections::HashMap;
use std::sync::LazyLock;

/// Compiling is far more expensive than matching: do it once. `unwrap` is fine for a literal pattern covered by a test.
static LOG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)                              # verbose mode: whitespace and comments are ignored
        ^(?P<ip>\d{1,3}(?:\.\d{1,3}){3})\s  # client
        \[(?P<time>[^\]]+)\]\s
        \x22(?P<method>[A-Z]+)\x20(?P<path>\S+)\x22\s   # \x20: a literal space must be escaped under (?x)
        (?P<status>\d{3})
        (?:\s(?P<ms>\d+)ms)?$               # optional duration
        ",
    )
    .unwrap()
});

#[derive(Debug, PartialEq)]
struct Hit<'a> {
    ip: &'a str,
    method: &'a str,
    path: &'a str,
    status: u16,
    ms: Option<u32>,
}
fn parse_line(line: &str) -> Option<Hit<'_>> {
    let c = LOG.captures(line)?;
    Some(Hit {
        ip: c.name("ip")?.as_str(), // borrows from `line`: no allocation
        method: c.name("method")?.as_str(),
        path: c.name("path")?.as_str(),
        status: c["status"].parse().ok()?, // c["name"] PANICS if the group did not participate
        ms: c.name("ms").and_then(|m| m.as_str().parse().ok()),
    })
}

fn log_parsing() {
    let h = parse_line(r#"10.0.0.7 [05/Mar/2024:10:00:01] "GET /a?x=1" 200 12ms"#).unwrap();
    assert_eq!(
        h,
        Hit {
            ip: "10.0.0.7",
            method: "GET",
            path: "/a?x=1",
            status: 200,
            ms: Some(12)
        }
    );
    assert_eq!(
        parse_line(r#"10.0.0.7 [t] "POST /b" 500"#).unwrap().ms,
        None
    );
    assert!(parse_line("garbage").is_none());
    // The ip group checks SHAPE only. Validate meaning with a real parser after matching.
    let weird = parse_line(r#"999.1.1.1 [t] "GET /" 200"#).unwrap();
    assert!(weird.ip.parse::<std::net::Ipv4Addr>().is_err());
    let names: Vec<_> = LOG.capture_names().flatten().collect();
    assert_eq!(names, ["ip", "time", "method", "path", "status", "ms"]);
    assert_eq!(
        LOG.captures_len(),
        7,
        "6 groups plus group 0 (the whole match)"
    );
}

fn matching_basics() {
    let re = Regex::new(r"\d+").unwrap();
    // Unanchored by default: is_match means "contains a match".
    assert!(re.is_match("ab12cd"));
    assert!(!Regex::new(r"^(?:\d+)$").unwrap().is_match("ab12cd"));
    let m = re.find("ab12cd345").unwrap();
    assert_eq!(
        (m.start(), m.end(), m.range(), m.as_str()),
        (2, 4, 2..4, "12")
    );
    assert_eq!(re.find_at("ab12cd345", 4).unwrap().as_str(), "345");
    assert_eq!(
        re.find_iter("1 22 333")
            .map(|m| m.as_str())
            .collect::<Vec<_>>(),
        ["1", "22", "333"]
    );
    // Offsets are BYTE offsets on char boundaries, so slicing is safe.
    let m = Regex::new("b+").unwrap().find("éébb").unwrap();
    assert_eq!((m.start(), &"éébb"[m.range()]), (4, "bb"));
    // Matches never overlap.
    assert_eq!(Regex::new("aa").unwrap().find_iter("aaaa").count(), 2);
    // Alternation is leftmost-FIRST (preference order), not longest.
    assert_eq!(
        Regex::new("sam|samwise")
            .unwrap()
            .find("samwise")
            .unwrap()
            .as_str(),
        "sam"
    );
    // Greedy vs lazy.
    assert_eq!(
        Regex::new("<.+>").unwrap().find("<a><b>").unwrap().as_str(),
        "<a><b>"
    );
    assert_eq!(
        Regex::new("<.+?>")
            .unwrap()
            .find("<a><b>")
            .unwrap()
            .as_str(),
        "<a>"
    );
    // extract(): fixed-size array of groups. It PANICS if the pattern's group count differs from the array size.
    let kv = Regex::new(r"(\w+)=(\d+)").unwrap();
    let pairs: HashMap<&str, u32> = kv
        .captures_iter("a=1; b=22")
        .map(|c| c.extract())
        .map(|(_, [k, v])| (k, v.parse().unwrap()))
        .collect();
    assert_eq!(pairs, HashMap::from([("a", 1), ("b", 22)]));
    // Optional group that did not take part.
    let c = Regex::new(r"a(b)?c").unwrap().captures("ac").unwrap();
    assert!(c.get(1).is_none());
    assert_eq!(&c[0], "ac");
}

fn replacement() {
    let date = Regex::new(r"(?P<y>\d{4})-(?P<m>\d{2})-(?P<d>\d{2})").unwrap();
    let text = "from 2024-03-05 to 2024-04-01";
    assert_eq!(
        date.replace_all(text, "$d/$m/$y"),
        "from 05/03/2024 to 01/04/2024"
    );
    assert_eq!(date.replace(text, "X"), "from X to 2024-04-01");
    assert_eq!(date.replacen(text, 1, "X"), "from X to 2024-04-01");
    // PITFALL: `$yth` means the group NAMED "yth", which does not exist, so it expands to nothing. Use braces.
    assert_eq!(date.replace("2024-03-05", "$yth"), "");
    assert_eq!(date.replace("2024-03-05", "${y}th"), "2024th");
    assert_eq!(date.replace("2024-03-05", "$$y"), "$y");
    assert_eq!(
        date.replace("2024-03-05", NoExpand("$y")),
        "$y",
        "NoExpand: use for user-supplied replacement text"
    );
    // Cow: no allocation when nothing matched.
    assert!(matches!(
        date.replace_all("no dates", "X"),
        std::borrow::Cow::Borrowed(_)
    ));

    // Template substitution with a closure; unknown names are left visible.
    let var = Regex::new(r"\{\{\s*(\w+)\s*\}\}").unwrap();
    let vars = HashMap::from([("name", "Ann"), ("n", "3"), ("evil", "{{name}}")]);
    let render = |t: &str| {
        var.replace_all(t, |c: &Captures| {
            vars.get(&c[1])
                .map_or_else(|| c[0].to_string(), |v| v.to_string())
        })
        .into_owned()
    };
    assert_eq!(
        render("hi {{ name }}, {{n}} new {{missing}}"),
        "hi Ann, 3 new {{missing}}"
    );
    assert_eq!(
        render("{{evil}}"),
        "{{name}}",
        "one pass: substituted values are not expanded again"
    );
    // expand(): format from captures without replacing.
    let mut out = String::new();
    date.captures(text).unwrap().expand("${m}.${y}", &mut out);
    assert_eq!(out, "03.2024");
}

fn split_escape_flags() {
    let sep = Regex::new(r"\s*[,;]\s*").unwrap();
    assert_eq!(
        sep.split("a, b ;c,,d").collect::<Vec<_>>(),
        ["a", "b", "c", "", "d"]
    );
    assert_eq!(sep.splitn("a,b,c", 2).collect::<Vec<_>>(), ["a", "b,c"]);

    let user = "1+1=2 (really?)";
    assert_eq!(regex::escape(user), r"1\+1=2 \(really\?\)");
    assert!(
        Regex::new(&format!("^{}$", regex::escape(user)))
            .unwrap()
            .is_match(user)
    );
    assert!(
        !Regex::new(&format!("^{user}$")).unwrap().is_match(user),
        "unescaped input changes the meaning"
    );

    // Flags: inline or via the builder.
    assert!(Regex::new("(?i)straße").unwrap().is_match("STRAßE"));
    assert!(
        Regex::new("(?i)δ").unwrap().is_match("Δ"),
        "case folding is Unicode aware"
    );
    let re = RegexBuilder::new("^error")
        .case_insensitive(true)
        .multi_line(true)
        .build()
        .unwrap();
    assert_eq!(re.find_iter("ok\nERROR 1\n  error 2\nError 3").count(), 2);
    assert!(!Regex::new("a.b").unwrap().is_match("a\nb"));
    assert!(
        Regex::new("(?s)a.b").unwrap().is_match("a\nb"),
        "(?s): dot also matches newline"
    );
    // \d, \w, \b are Unicode by default: this matters for validators.
    assert!(
        Regex::new(r"^\d+$").unwrap().is_match("١٢٣"),
        "Arabic-Indic digits are \\d"
    );
    assert!(!Regex::new(r"^[0-9]+$").unwrap().is_match("١٢٣"));
    assert!(Regex::new(r"^\p{Greek}+$").unwrap().is_match("αβγ"));
    // PITFALL: `$` is end of TEXT without (?m), so a trailing newline is not silently accepted.
    assert!(!Regex::new(r"^\d+$").unwrap().is_match("12\n"));
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Kind {
    Number,
    Ident,
    Symbol,
}
/// A tokenizer: one alternation, anchored at the current position, one named group per token kind.
fn tokenize(src: &str) -> Result<Vec<(Kind, &str)>, usize> {
    static TOKEN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"^(?:(?P<ws>\s+)|(?P<num>[0-9]+)|(?P<ident>[A-Za-z_]\w*)|(?P<sym>==|[-+*/=()]))",
        )
        .unwrap()
    });
    let (mut at, mut out) = (0, vec![]);
    while at < src.len() {
        let c = TOKEN.captures(&src[at..]).ok_or(at)?;
        for (group, kind) in [
            ("num", Kind::Number),
            ("ident", Kind::Ident),
            ("sym", Kind::Symbol),
        ] {
            if let Some(m) = c.name(group) {
                out.push((kind, m.as_str()));
            }
        }
        at += c.get(0).unwrap().end();
    }
    Ok(out)
}
fn tokenizing() {
    use Kind::*;
    assert_eq!(
        tokenize("x1 == 42+(y)").unwrap(),
        [
            (Ident, "x1"),
            (Symbol, "=="),
            (Number, "42"),
            (Symbol, "+"),
            (Symbol, "("),
            (Ident, "y"),
            (Symbol, ")")
        ]
    );
    assert_eq!(
        tokenize("a = $"),
        Err(4),
        "the error carries the byte offset"
    );
}

fn sets_and_bytes() {
    // A secret scanner: one pass tells WHICH rules fire. For positions, re-run the individual regex.
    let rules = [
        r"AKIA[0-9A-Z]{16}",
        r"-----BEGIN [A-Z ]*PRIVATE KEY-----",
        r"(?i)password\s*=\s*\S+",
    ];
    let set = RegexSet::new(rules).unwrap();
    let hits: Vec<usize> = set
        .matches("PASSWORD = hunter2 AKIAABCDEFGHIJKLMNOP")
        .into_iter()
        .collect();
    assert_eq!(hits, [0, 2]);
    assert!(!set.matches("clean").matched_any());
    assert!(set.matches("password=x").matched(2));
    let singles: Vec<Regex> = rules.iter().map(|r| Regex::new(r).unwrap()).collect();
    for text in ["", "password=1", "AKIA", "-----BEGIN RSA PRIVATE KEY-----"] {
        let by_loop: Vec<usize> = (0..rules.len())
            .filter(|&i| singles[i].is_match(text))
            .collect();
        assert_eq!(
            set.matches(text).into_iter().collect::<Vec<_>>(),
            by_loop,
            "{text:?}"
        );
    }
    // bytes::Regex searches arbitrary bytes. (?-u) lets `.` and \xNN mean raw bytes instead of code points.
    let re = regex::bytes::Regex::new(r"(?-u)\x00(.{2})\xFF").unwrap();
    let data = [9u8, 0x00, 0xC3, 0x28, 0xFF, 9]; // C3 28 is not valid UTF-8
    assert_eq!(&re.captures(&data).unwrap()[1], &[0xC3, 0x28]);
    assert!(std::str::from_utf8(std::hint::black_box(&data)).is_err()); // black_box: rustc would otherwise lint the literal
}

fn limits_and_errors() {
    // Unsupported by design.
    for bad in [r"(?=x)", r"(?<!x)y", r"(a)\1", "(unclosed", "a{2,1}"] {
        assert!(Regex::new(bad).is_err(), "{bad}");
    }
    // The error message points at the problem.
    assert!(
        Regex::new(r"(a)\1")
            .unwrap_err()
            .to_string()
            .contains("backreferences are not supported")
    );
    // Without look-ahead, express "contains a digit AND a capital" as two checks.
    let (digit, upper) = (Regex::new(r"\d").unwrap(), Regex::new(r"[A-Z]").unwrap());
    let strong = |p: &str| p.chars().count() >= 8 && digit.is_match(p) && upper.is_match(p);
    assert!(strong("Secr3tpass") && !strong("secretpass1"));
    // A small pattern can compile to a huge program: cap it for untrusted patterns.
    let e = RegexBuilder::new(r"\w{100}{100}")
        .size_limit(1 << 16)
        .build()
        .unwrap_err();
    assert!(matches!(e, regex::Error::CompiledTooBig(_)), "{e}");
    // Do not "validate email" with a monster regex. Check the minimum, then send a confirmation message.
    let looks_like_email = Regex::new(r"^[^@\s]+@[^@\s]+$").unwrap();
    assert!(looks_like_email.is_match("a+tag@例え.jp") && !looks_like_email.is_match("a b@c"));
}

/// A textbook backtracking matcher for the pattern (a|a)*b, counting steps. Both branches match 'a', so on a string of
/// n a's with no b it explores every one of the 2^n ways to split them.
fn backtrack(text: &[u8], at: usize, steps: &mut u64) -> bool {
    *steps += 1;
    for _branch in 0..2 {
        if text.get(at) == Some(&b'a') && backtrack(text, at + 1, steps) {
            return true;
        }
    }
    text.get(at) == Some(&b'b') && at + 1 == text.len()
}
fn linear_time_guarantee() {
    let mut counts = vec![];
    for n in [10usize, 14, 18] {
        let text = vec![b'a'; n];
        let mut steps = 0;
        assert!(!backtrack(&text, 0, &mut steps));
        assert_eq!(steps, (1u64 << (n + 1)) - 1, "exactly exponential");
        counts.push(steps);
    }
    println!("backtracking (a|a)*b on 10/14/18 a's: {counts:?} calls");
    // The same pattern on 100_000 a's. A backtracker would need about 2^100000 steps; finishing at all is the proof.
    let big = "a".repeat(100_000);
    for pat in [r"^(a|a)*b$", r"^(a+)+b$", r"^(\w+\s?)*!$"] {
        assert!(!Regex::new(pat).unwrap().is_match(&big), "{pat}");
    }
    println!("regex crate: three catastrophic patterns rejected 100000 chars");
}

fn offsets_and_context() {
    // PITFALL: slicing the haystack is not the same as searching from an offset. The slice has a new "start of text" and
    // no character before it, so ^ and \b give different answers.
    let anchored = Regex::new(r"^bar").unwrap();
    let word = Regex::new(r"\bbar\b").unwrap();
    let text = "foobar bar";
    assert!(anchored.is_match(&text[3..]) && !anchored.is_match_at(text, 3));
    assert!(word.is_match(&text[3..6]) && word.find_at(text, 3).unwrap().start() == 7);
    // So a tokenizer like the one above must not use ^ plus slicing if its patterns use look-around style assertions
    // (\b, (?m)^) that need to see the previous character.

    // shortest_match answers "where does SOME match first end", which can be before the leftmost-first match ends.
    let digits = Regex::new(r"\d+").unwrap();
    assert_eq!(digits.find("a1234").unwrap().end(), 5);
    assert!(
        digits
            .shortest_match("a1234")
            .is_some_and(|end| (2..=5).contains(&end))
    );

    // Empty matches: iteration always advances, in whole characters.
    let empty_ok = Regex::new("x*").unwrap();
    assert_eq!(
        empty_ok
            .find_iter("éxxé")
            .map(|m| m.range())
            .collect::<Vec<_>>(),
        [0..0, 2..4, 6..6]
    );
    assert!(
        empty_ok.is_match(""),
        "a pattern that can match nothing matches EVERY string: anchor your validators"
    );

    // split drops the separators. To keep them, walk the matches and take the gaps yourself.
    let op = Regex::new(r"[-+*/]").unwrap();
    let src = "12+3*45-6";
    let mut parts = vec![];
    let mut last = 0;
    for m in op.find_iter(src) {
        parts.extend([&src[last..m.start()], m.as_str()]);
        last = m.end();
    }
    parts.push(&src[last..]);
    assert_eq!(parts, ["12", "+", "3", "*", "45", "-", "6"]);
    assert_eq!(parts.concat(), src, "nothing lost");
}

/// A Replacer with state: number each placeholder in order of appearance and remember the names (SQL parameter binding).
struct Numberer {
    names: Vec<String>,
}
impl regex::Replacer for Numberer {
    fn replace_append(&mut self, caps: &Captures<'_>, dst: &mut String) {
        let name = &caps["name"];
        let i = self
            .names
            .iter()
            .position(|n| n == name)
            .unwrap_or_else(|| {
                self.names.push(name.to_owned());
                self.names.len() - 1
            });
        dst.push('$'); // written straight into the output: no expansion of '$' happens here
        dst.push_str(&(i + 1).to_string());
    }
}

fn reusable_replacers_and_captures() {
    let param = Regex::new(r":(?P<name>[a-z_]+)").unwrap();
    let mut numberer = Numberer { names: vec![] };
    // by_ref() lends the replacer, so its state can be read afterwards.
    let sql = param.replace_all(
        "WHERE a = :id AND b = :kind OR c = :id",
        regex::Replacer::by_ref(&mut numberer),
    );
    assert_eq!(sql, "WHERE a = $1 AND b = $2 OR c = $1");
    assert_eq!(numberer.names, ["id", "kind"]);
    // A &str replacement without '$' is detected as literal (no_expansion), which skips capture work entirely.
    assert!(regex::Replacer::no_expansion(&mut "plain").is_some());
    assert!(regex::Replacer::no_expansion(&mut "has $1").is_none());

    // captures() allocates slot storage per call. In a hot loop allocate the slots once.
    let kv = Regex::new(r"(\w+)=(\d+)").unwrap();
    let mut slots = kv.capture_locations();
    let line = "a=1 bb=22 ccc=333";
    let (mut at, mut got) = (0, vec![]);
    while let Some(m) = kv.captures_read_at(&mut slots, line, at) {
        let (ks, ke) = slots.get(1).unwrap();
        let (vs, ve) = slots.get(2).unwrap();
        got.push((&line[ks..ke], &line[vs..ve]));
        at = m.end(); // fine here: this pattern cannot match the empty string
    }
    let simple: Vec<(&str, &str)> = kv
        .captures_iter(line)
        .map(|c| c.extract())
        .map(|(_, [k, v])| (k, v))
        .collect();
    assert_eq!(got, simple);
    assert!(kv.captures_read(&mut slots, "nothing").is_none());
    // static_captures_len is Some only when every match fills the same groups: the precondition of extract().
    assert_eq!(kv.static_captures_len(), Some(3));
    assert_eq!(Regex::new(r"a(b)?c").unwrap().static_captures_len(), None);
}

/// Many literals as one regex. Order matters because alternation is leftmost-first.
fn keyword_regex(words: &[&str]) -> Regex {
    let mut sorted = words.to_vec();
    sorted.sort_by_key(|w| std::cmp::Reverse(w.len()));
    let alternation = sorted
        .iter()
        .map(|w| regex::escape(w))
        .collect::<Vec<_>>()
        .join("|");
    Regex::new(&alternation).unwrap()
}

fn builder_options_and_classes() {
    use regex::RegexSetBuilder;
    let ops = ["=", "==", "=>", "+", "++"];
    let naive = Regex::new(
        &ops.iter()
            .map(|w| regex::escape(w))
            .collect::<Vec<_>>()
            .join("|"),
    )
    .unwrap();
    let found = |re: &Regex| {
        re.find_iter("a==b=>c++")
            .map(|m| m.as_str().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        found(&naive),
        ["=", "=", "=", "+", "+"],
        "a short word listed first hides the longer ones"
    );
    assert_eq!(found(&keyword_regex(&ops)), ["==", "=>", "++"]);

    // CRLF input: with (?m) alone, '$' sits before '\n' only, so '\r' stays at the end of each line.
    let windows = "key=1\r\nkey=2\r\n";
    let values = |crlf| {
        let re = RegexBuilder::new(r"=(.+)$")
            .multi_line(true)
            .crlf(crlf)
            .build()
            .unwrap();
        re.captures_iter(windows)
            .map(|c| c[1].to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(values(false), ["1\r", "2\r"]);
    assert_eq!(values(true), ["1", "2"]);
    // NUL-separated records (find -print0): change what ^, $ and . treat as the line terminator.
    let nul = RegexBuilder::new("^b.*$")
        .multi_line(true)
        .line_terminator(0)
        .build()
        .unwrap();
    assert_eq!(nul.find("a\0b\nc\0d").unwrap().as_str(), "b\nc");
    assert!(
        RegexBuilder::new("a.b")
            .dot_matches_new_line(true)
            .build()
            .unwrap()
            .is_match("a\nb")
    );
    // swap_greed flips greedy and lazy; ignore_whitespace is (?x).
    assert_eq!(
        RegexBuilder::new("<.+>")
            .swap_greed(true)
            .build()
            .unwrap()
            .find("<a><b>")
            .unwrap()
            .as_str(),
        "<a>"
    );
    assert!(
        RegexBuilder::new(r"\d{3} - \d{4} # phone")
            .ignore_whitespace(true)
            .build()
            .unwrap()
            .is_match("555-1234")
    );
    // unicode(false): \w, \d and (?i) become ASCII only (smaller and sometimes faster).
    let ascii = RegexBuilder::new(r"^\w+$").unicode(false).build().unwrap();
    assert!(ascii.is_match("abc_1") && !ascii.is_match("é"));
    // nest_limit bounds recursion for hostile PATTERNS; size_limit (below) bounds compiled size. Set both.
    let nested = format!("{}a{}", "(".repeat(50), ")".repeat(50));
    assert!(RegexBuilder::new(&nested).nest_limit(10).build().is_err());
    assert!(Regex::new(&nested).is_ok());

    // Character class algebra: intersection, subtraction, ASCII classes.
    let consonants = Regex::new("^[a-z&&[^aeiou]]+$").unwrap();
    assert!(consonants.is_match("rhythm") && !consonants.is_match("rust"));
    assert!(
        Regex::new(r"^[\p{Greek}--π]+$").unwrap().is_match("αβ")
            && !Regex::new(r"^[\p{Greek}--π]+$").unwrap().is_match("απ")
    );
    assert!(
        Regex::new("^[[:alpha:]]+$").unwrap().is_match("abc")
            && !Regex::new("^[[:alpha:]]+$").unwrap().is_match("é"),
        "[[:alpha:]] is ASCII"
    );
    // \b{start-half}: "no word character before here", usable next to punctuation where \b is not.
    let flag = Regex::new(r"\b{start-half}--force\b{end-half}").unwrap();
    assert!(
        flag.is_match("rm --force x") && !flag.is_match("x--force") && !flag.is_match("--forced")
    );
    assert!(
        !Regex::new(r"\b--force\b").unwrap().is_match("rm --force x"),
        "\\b needs a word character on exactly one side"
    );

    // Options apply to every pattern of a set.
    let set = RegexSetBuilder::new(["^error", "timeout$"])
        .case_insensitive(true)
        .build()
        .unwrap();
    assert_eq!((set.len(), set.patterns().len()), (2, 2));
    assert_eq!(
        set.matches("ERROR: Timeout")
            .into_iter()
            .collect::<Vec<_>>(),
        [0, 1]
    );

    // A Regex is Send + Sync: share one. (Cloning is cheap too.)
    let shared = keyword_regex(&["warn", "error"]);
    let total: usize = std::thread::scope(|s| {
        let handles: Vec<_> = (0..4)
            .map(|_| s.spawn(|| shared.find_iter("warn error warn").count()))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).sum()
    });
    assert_eq!(total, 12);
}

pub fn main() {
    log_parsing();
    matching_basics();
    replacement();
    split_escape_flags();
    tokenizing();
    sets_and_bytes();
    limits_and_errors();
    linear_time_guarantee();
    offsets_and_context();
    reusable_replacers_and_captures();
    builder_options_and_classes();
    println!("regex: all sections passed");
}
