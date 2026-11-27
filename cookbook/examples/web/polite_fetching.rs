//! RECIPE 1212 (web): collect pages politely.
//! LEVEL: intermediate
//! Use in: any program that asks someone else's server for many pages.   Run: cargo run --example polite_fetching
//! COMMON USES: link checkers, feed readers, documentation indexers, importers.
//!
//! A reader for `robots.txt` that follows the main rules of RFC 9309, one request at a time to each host with a gap
//! between them, waiting that respects `Retry-After`, a user agent that says who is asking, and a stop that stays
//! stopped when a host says no. When a site refuses you, the program stops. It does not try to get round the refusal.
//!
//! **LIMITS**: following `robots.txt` is not permission. A site's terms, the law and the owner's wishes still apply.
//! This keeps its state in one process, reads one of the three HTTP date forms, and cannot recognise a refusal that
//! arrives as an ordinary page with status 200. It does not read `Crawl-delay` or `Sitemap` lines.
//! Time is passed in, so the checks never sleep. The one network check uses a local server.
//! Read: https://www.rfc-editor.org/rfc/rfc9309.html
#![allow(dead_code)]

use std::collections::HashMap;

// ---------------------------------------------------------------- robots.txt

const MAX_ROBOTS_BYTES: usize = 500 * 1024; // RFC 9309 asks for at least this much
const MAX_LINE: usize = 2048;

#[derive(Debug, Clone, PartialEq)]
struct Rule {
    allow: bool,
    pattern: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
enum Robots {
    AllowAll,
    DisallowAll,
    Rules(Vec<Rule>),
}

/// Rewrites a path or a pattern so that two spellings of the same thing compare equal. Escapes of letters, digits
/// and `-._~` are decoded. Other escapes keep their meaning and get capital hex digits. Bytes outside ASCII are escaped.
fn normalise(text: &[u8], is_pattern: bool) -> Vec<u8> {
    let hex = |b: u8| (b as char).to_digit(16);
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        let b = text[i];
        let escape = match (
            b,
            text.get(i + 1).and_then(|c| hex(*c)),
            text.get(i + 2).and_then(|c| hex(*c)),
        ) {
            (b'%', Some(high), Some(low)) => Some((high * 16 + low) as u8),
            _ => None,
        };
        match escape {
            Some(v) if v.is_ascii_alphanumeric() || b"-._~".contains(&v) => out.push(v),
            Some(v) => out.extend_from_slice(format!("%{v:02X}").as_bytes()),
            // in a path, `*` and `$` are ordinary bytes; a rule names them as %2A and %24
            None if !is_pattern && (b == b'*' || b == b'$') => {
                out.extend_from_slice(format!("%{b:02X}").as_bytes())
            }
            // in a rule, only a final `$` is the end mark; an earlier one means the byte itself
            None if is_pattern && b == b'$' && i + 1 < text.len() => out.extend_from_slice(b"%24"),
            None if !b.is_ascii_graphic() => out.extend_from_slice(format!("%{b:02X}").as_bytes()),
            None => out.push(b),
        }
        i += if escape.is_some() { 3 } else { 1 };
    }
    out
}

/// Reads the groups that apply to `token`. If no group names it, reads the `*` groups.
fn parse_robots(file: &[u8], token: &str) -> Robots {
    if file.len() > MAX_ROBOTS_BYTES {
        // rules past the cut are unknown, so a cut file is never treated as the whole one
        return Robots::DisallowAll;
    }
    let file = file.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(file);
    let (mut ours, mut anyone) = (Vec::new(), Vec::new());
    let mut named = false;
    // Disallow rules from groups whose name only might be ours
    let mut doubtful = Vec::new();
    // which lists the current group feeds
    let (mut for_us, mut for_anyone, mut for_maybe) = (false, false, false);
    let mut group_has_rules = true;
    for line in file.split(|b| *b == b'\n' || *b == b'\r') {
        let line = line.split(|b| *b == b'#').next().unwrap_or_default();
        let Some(colon) = line.iter().position(|b| *b == b':') else {
            continue;
        };
        let key = line[..colon].trim_ascii().to_ascii_lowercase();
        let value = line[colon + 1..].trim_ascii();
        match key.as_slice() {
            b"user-agent" => {
                if group_has_rules {
                    (for_us, for_anyone, for_maybe) = (false, false, false);
                    group_has_rules = false;
                }
                // `docbot/1.0` names docbot; `bot` and `docbot-images` do not
                let name = value
                    .split(|b| *b == b'/' || b.is_ascii_whitespace())
                    .next()
                    .unwrap_or_default();
                // RFC 9309 allows only letters, `_` and `-` in a name. `docbot*` and `docbot2.0` begin with ours
                // and go on with something else, so their Disallow rules are obeyed and their Allow rules are not
                let start = value
                    .split(|b| !(b.is_ascii_alphabetic() || *b == b'_' || *b == b'-'))
                    .next()
                    .unwrap_or_default();
                if name.eq_ignore_ascii_case(token.as_bytes()) {
                    for_us = true;
                    named = true;
                } else if start.eq_ignore_ascii_case(token.as_bytes()) {
                    for_maybe = true;
                }
                for_anyone |= name == b"*";
            }
            b"allow" | b"disallow" => {
                group_has_rules = true;
                if value.is_empty() || !(for_us || for_anyone || for_maybe) {
                    continue;
                }
                let allow = key == b"allow";
                if value.len() > MAX_LINE {
                    if allow {
                        continue; // dropping an Allow only makes us ask for less
                    }
                    return Robots::DisallowAll;
                }
                let rule = Rule {
                    allow,
                    pattern: normalise(value, true),
                };
                if for_us {
                    ours.push(rule.clone());
                }
                if for_maybe && !allow {
                    doubtful.push(rule.clone());
                }
                if for_anyone {
                    anyone.push(rule);
                }
            }
            _ => {} // other records, such as Sitemap, do not end a group
        }
    }
    let mut rules = if named { ours } else { anyone };
    // an extra Disallow can only block more
    rules.append(&mut doubtful);
    Robots::Rules(rules)
}

/// Whether `pattern` matches a beginning of `path`. `*` stands for any bytes. A final `$` pins the end.
fn rule_matches(pattern: &[u8], path: &[u8]) -> bool {
    let (pattern, to_end) = match pattern.strip_suffix(b"$") {
        Some(rest) => (rest, true),
        None => (pattern, false),
    };
    let mut pieces = pattern.split(|b| *b == b'*');
    let first = pieces.next().unwrap_or_default();
    let Some(mut rest) = path.strip_prefix(first) else {
        return false;
    };
    let mut pieces: Vec<&[u8]> = pieces.collect();
    if pieces.is_empty() {
        return !to_end || rest.is_empty();
    }
    let last = if to_end { pieces.pop() } else { None };
    // taking each middle piece at its first place leaves the most room for the pieces after it
    for piece in pieces.into_iter().filter(|p| !p.is_empty()) {
        let Some(at) = rest.windows(piece.len()).position(|w| w == piece) else {
            return false;
        };
        rest = &rest[at + piece.len()..];
    }
    last.is_none_or(|last| rest.ends_with(last))
}

impl Robots {
    /// `path` is the path and query of the address, such as `/search?q=1`.
    fn allows(&self, path: &str) -> bool {
        if path == "/robots.txt" {
            return true;
        }
        match self {
            Robots::AllowAll => true,
            Robots::DisallowAll => false,
            Robots::Rules(rules) => {
                if path.len() > MAX_LINE {
                    return false;
                }
                let path = normalise(path.as_bytes(), false);
                // the longest matching pattern wins; between equals, Allow wins
                rules
                    .iter()
                    .filter(|r| rule_matches(&r.pattern, &path))
                    .max_by_key(|r| (r.pattern.len(), r.allow))
                    .is_none_or(|r| r.allow)
            }
        }
    }

    /// What to assume from the answer to the `/robots.txt` request, after the fetcher followed its redirects.
    /// `None` means no answer at all.
    fn from_answer(answer: Option<(u16, &[u8])>, token: &str) -> Robots {
        match answer {
            Some((200..=299, file)) => parse_robots(file, token),
            // the RFC lets a crawler go ahead on any 4xx; 401, 403 and 429 are refusals, so this one does not
            Some((401 | 403 | 429, _)) => Robots::DisallowAll,
            Some((400..=499, _)) => Robots::AllowAll,
            _ => Robots::DisallowAll,
        }
    }
}

// ---------------------------------------------------------------- Retry-After

/// Days from 1 January 1970 to the given date.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let year_of_era = y - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Reads `Sun, 06 Nov 1994 08:49:37 GMT` as seconds since 1970. The weekday name is not checked.
fn http_date(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    let shape = b"aaa, 00 aaa 0000 00:00:00 GMT";
    let fits = b.len() == shape.len()
        && b.iter().zip(shape).all(|(c, want)| match want {
            b'0' => c.is_ascii_digit(),
            b'a' => c.is_ascii_alphabetic(),
            other => c == other,
        });
    if !fits {
        return None;
    }
    let number = |from: usize, to: usize| text[from..to].parse::<i64>().ok();
    let months = "JanFebMarAprMayJunJulAugSepOctNovDec";
    let at = (0..12).find(|m| months[m * 3..m * 3 + 3] == text[8..11])?;
    let (day, year) = (number(5, 7)?, number(12, 16)?);
    let (hour, minute, second) = (number(17, 19)?, number(20, 22)?, number(23, 25)?);
    if !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    Some(days_from_civil(year, at as i64 + 1, day) * 86_400 + hour * 3600 + minute * 60 + second)
}

/// How long the header asks us to wait, in milliseconds. `None` means it cannot be read.
/// `now` is the time in seconds since 1970, best taken from the answer's own `Date` header.
fn retry_after_ms(value: &str, now: i64) -> Option<u64> {
    let value = value.trim();
    if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
        // too many digits to hold still means "a very long time"
        return Some(
            value
                .parse::<u64>()
                .unwrap_or(u64::MAX)
                .saturating_mul(1000),
        );
    }
    let seconds = http_date(value)?.saturating_sub(now).max(0);
    Some((seconds as u64).saturating_mul(1000))
}

// ---------------------------------------------------------------- one host at a time

#[derive(Debug, Clone, Copy)]
struct Limits {
    /// the least time between the end of one request to a host and the start of the next
    gap_ms: u64,
    backoff_base_ms: u64,
    backoff_cap_ms: u64,
    /// a host that asks for a longer wait is left alone
    longest_wait_ms: u64,
    /// failures in a row before the host is left alone
    strikes: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Stop {
    Refused(u16),
    AsksTooMuch,
    UnreadableWait,
    KeptFailing,
    /// the host text could not be read, so nothing may be sent
    BadHost,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Go {
    Now,
    NotBefore(u64),
    Stopped(Stop),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Verdict {
    Done,
    /// ask for the same page again, when the host is ready
    Retry,
    /// this page will not work; others on the host may
    SkipPage,
    StopHost(Stop),
}

/// One key for every spelling of a host: lower case, no final dot and no port. Merging ports is stricter than
/// needed, never looser. `None` if the text is not a bare host with an optional port.
fn host_key(host: &str) -> Option<String> {
    if host.contains(|c: char| {
        matches!(c, '/' | '\\' | '?' | '#' | '@') || c.is_whitespace() || c.is_control()
    }) {
        return None;
    }
    let url = reqwest::Url::parse(&format!("http://{host}/")).ok()?;
    let name = url.host_str()?.trim_end_matches('.');
    (!name.is_empty()).then(|| name.to_string())
}

#[derive(Debug, Default)]
struct Host {
    ready_at: u64,
    strikes: u32,
    stopped: Option<Stop>,
}

struct Pace {
    limits: Limits,
    hosts: HashMap<String, Host>,
    random: u64,
}

impl Pace {
    fn new(limits: Limits, seed: u64) -> Pace {
        Pace {
            limits,
            hosts: HashMap::new(),
            random: seed | 1,
        }
    }

    /// A number from 0 to `most`. Good enough to spread requests out; never use it for secrets.
    fn jitter(&mut self, most: u64) -> u64 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        self.random % most.saturating_add(1)
    }

    fn before(&self, host: &str, now_ms: u64) -> Go {
        let Some(key) = host_key(host) else {
            return Go::Stopped(Stop::BadHost);
        };
        match self.hosts.get(&key) {
            Some(Host {
                stopped: Some(why), ..
            }) => Go::Stopped(*why),
            Some(h) if h.ready_at > now_ms => Go::NotBefore(h.ready_at),
            _ => Go::Now,
        }
    }

    /// Records an answer. `status` is `None` when there was no answer. Call it when the answer has arrived,
    /// and do not send another request to this host before that.
    fn after(
        &mut self,
        host: &str,
        now_ms: u64,
        status: Option<u16>,
        retry_after: Option<&str>,
        unix_now: i64,
    ) -> Verdict {
        let Some(key) = host_key(host) else {
            return Verdict::StopHost(Stop::BadHost);
        };
        let limits = self.limits;
        let before = self.hosts.get(&key).map_or(0, |h| h.strikes);
        // a Retry-After counts whatever the status is
        let asked = retry_after.map(|v| retry_after_ms(v, unix_now));
        let success = matches!(status, Some(200..=299 | 304));
        let failed =
            matches!(status, None | Some(408 | 429 | 500..=599)) || (asked.is_some() && !success);
        // only a plain success starts the count again
        let strikes = if failed {
            before.saturating_add(1)
        } else if success && asked.is_none() {
            0
        } else {
            before
        };
        let verdict = match (status, asked) {
            // the host has said no: asking again, or asking differently, is not the answer
            (Some(code @ (401 | 403 | 407 | 451)), _) => Verdict::StopHost(Stop::Refused(code)),
            (_, Some(None)) => Verdict::StopHost(Stop::UnreadableWait),
            // waiting less than asked would be ignoring the answer, so a long wait ends the visit
            (_, Some(Some(ms))) if ms > limits.longest_wait_ms => {
                Verdict::StopHost(Stop::AsksTooMuch)
            }
            _ if failed && strikes >= limits.strikes => Verdict::StopHost(Stop::KeptFailing),
            _ if failed => Verdict::Retry,
            _ if success => Verdict::Done,
            _ => Verdict::SkipPage,
        };
        let wait = match asked.flatten() {
            // never sooner than asked; the jitter comes on top, so that clients spread out
            Some(ms) => ms
                .max(limits.gap_ms)
                .saturating_add(self.jitter(limits.gap_ms)),
            None if failed => {
                let doubled = limits
                    .backoff_base_ms
                    .saturating_mul(1u64 << strikes.saturating_sub(1).min(20));
                self.jitter(doubled.min(limits.backoff_cap_ms))
                    .max(limits.gap_ms)
            }
            None => limits.gap_ms,
        };
        let entry = self.hosts.entry(key).or_default();
        entry.strikes = strikes;
        entry.ready_at = entry.ready_at.max(now_ms.saturating_add(wait));
        if let Verdict::StopHost(why) = verdict {
            entry.stopped.get_or_insert(why);
        }
        verdict
    }
}

// ---------------------------------------------------------------- who is asking

/// `docbot/1.0 (+https://example.org/bot)`: the name `robots.txt` can use, and a page that says how to reach you.
fn user_agent(token: &str, version: &str, contact: &str) -> Option<String> {
    let token_ok = !token.is_empty()
        && token
            .bytes()
            .all(|b| b.is_ascii_alphabetic() || b == b'-' || b == b'_');
    let clean = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_graphic() && b != b'(' && b != b')')
    };
    (token_ok && clean(version) && clean(contact))
        .then(|| format!("{token}/{version} (+{contact})"))
}

// ---------------------------------------------------------------- checks

fn robots_checks() {
    let file = b"\xEF\xBB\xBF# a comment\r\n\
User-agent: other\r\n\
Disallow: /\r\n\
\r\n\
User-Agent: *\r\n\
Disallow: /tmp/\r\n\
\r\n\
user-agent: DocBot/2.0   # our name, with a version\r\n\
User-agent: second\r\n\
\r\n\
Sitemap: http://good.test/map.xml\r\n\
Disallow: /shop/\r\n\
Allow: /shop/help\r\n\
Disallow: /*.pdf$\r\n\
Disallow: /same\r\n\
Allow: /same\r\n\
Disallow:\r\n\
\r\n\
User-agent: docbot\r\n\
Disallow: /later\r\n\
\r\n\
User-agent: third\r\n\
Disallow: /\r\n";
    let rules = parse_robots(file, "docbot");
    for (path, allowed) in [
        ("/", true),
        ("/tmp/x", true), // the * group does not apply to a crawler that is named
        ("/shop/", false),
        ("/shop/cart", false),
        ("/shop/help", true),
        ("/Shop/cart", true), // paths are case sensitive
        ("/a/b.pdf", false),
        ("/a/b.pdf?page=2", true), // `$` means the path and query end here
        ("/same/x", true),
        ("/later", false), // a second group for the same name is added to the first
        ("/robots.txt", true),
        ("/s%68op/cart", false), // %68 is `h`
    ] {
        assert_eq!(rules.allows(path), allowed, "{path}");
    }
    assert!(!parse_robots(file, "stranger").allows("/tmp/x"));
    assert!(parse_robots(file, "stranger").allows("/shop/cart"));
    // the longer pattern wins, whichever kind it is
    let nested = parse_robots(
        b"User-agent: *\nAllow: /docs/\nDisallow: /docs/private/\n",
        "docbot",
    );
    assert!(nested.allows("/docs/a") && !nested.allows("/docs/private/a"));
    // a byte order mark must not hide the first line
    assert!(!parse_robots(b"\xEF\xBB\xBFUser-agent: *\nDisallow: /x\n", "docbot").allows("/x"));
    assert!(parse_robots(b"User-agent: *\nDisallow: /\n", "docbot").allows("/robots.txt"));
    // a name that only might be ours: obey what it forbids, and never what it opens
    for name in ["docbot*", "docbot2.0", "DocBot.v2"] {
        let file = format!(
            "User-agent: *\nDisallow: /tmp\n\nUser-agent: {name}\nDisallow: /shop\nAllow: /tmp\n"
        );
        let robots = parse_robots(file.as_bytes(), "docbot");
        for (path, allowed) in [("/shop", false), ("/tmp", false), ("/other", true)] {
            assert_eq!(robots.allows(path), allowed, "{name} {path}");
        }
        let closed = format!("User-agent: *\nDisallow: /\n\nUser-agent: {name}\nAllow: /\n");
        assert!(
            !parse_robots(closed.as_bytes(), "docbot").allows("/x"),
            "{name}"
        );
    }
    assert!(parse_robots(b"User-agent: docbot-images\nDisallow: /\n", "docbot").allows("/x"));
    // a `$` before the end of a rule is an ordinary byte
    let dollar = parse_robots(b"User-agent: *\nDisallow: /a$b\n", "docbot");
    assert!(!dollar.allows("/a$b/c") && !dollar.allows("/a%24b") && dollar.allows("/ab"));
    // `bot` is not a group for `docbot`
    assert!(parse_robots(b"User-agent: bot\nDisallow: /\n", "docbot").allows("/x"));
    assert!(
        parse_robots(b"Disallow: /\n", "docbot").allows("/x"),
        "a rule before any group is ignored"
    );
    assert!(!parse_robots(b"User-agent: *\nDisallow: *\n", "docbot").allows("/x"));
    // spellings that must compare equal, and ones that must not
    let encoded = parse_robots(
        "User-agent: *\nDisallow: /café\nDisallow: /a%2fb\nDisallow: /star%2A\n".as_bytes(),
        "docbot",
    );
    for (path, allowed) in [
        ("/caf%c3%a9", false),
        ("/café", false),
        ("/a%2Fb", false),
        ("/a/b", true),
        ("/star*", false),
        ("/starx", true),
    ] {
        assert_eq!(encoded.allows(path), allowed, "{path}");
    }
    let mut big = b"User-agent: *\nAllow: /\n".to_vec();
    big.resize(MAX_ROBOTS_BYTES + 1, b'#');
    assert_eq!(parse_robots(&big, "docbot"), Robots::DisallowAll);
    let long = format!("User-agent: *\nDisallow: /{}\n", "a".repeat(5000));
    assert_eq!(parse_robots(long.as_bytes(), "docbot"), Robots::DisallowAll);
    assert!(!rules.allows(&format!("/{}", "a".repeat(5000))));

    let file = b"User-agent: *\nDisallow: /x\n".as_slice();
    assert!(!Robots::from_answer(Some((200, file)), "docbot").allows("/x"));
    assert_eq!(
        Robots::from_answer(Some((404, b"")), "docbot"),
        Robots::AllowAll
    );
    assert_eq!(
        Robots::from_answer(Some((410, b"")), "docbot"),
        Robots::AllowAll
    );
    for status in [301, 401, 403, 429, 500, 503] {
        assert_eq!(
            Robots::from_answer(Some((status, b"")), "docbot"),
            Robots::DisallowAll,
            "{status}"
        );
    }
    assert_eq!(Robots::from_answer(None, "docbot"), Robots::DisallowAll);
    println!("robots rules: ok");
}

/// The same matching, written the plain way: try every length for every `*`.
fn plain_match(pattern: &[u8], path: &[u8]) -> bool {
    match pattern {
        [] => true,
        [b'$'] => path.is_empty(),
        [b'*', rest @ ..] => (0..=path.len()).any(|skip| plain_match(rest, &path[skip..])),
        [c, rest @ ..] => path.first() == Some(c) && plain_match(rest, &path[1..]),
    }
}

fn random(seed: &mut u64, n: usize) -> usize {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    (*seed % n as u64) as usize
}

fn random_robots_checks() {
    let mut seed = 7u64;
    let mut matched = 0;
    for _ in 0..200_000 {
        let pattern: Vec<u8> = (0..random(&mut seed, 8))
            .map(|_| b"ab/*$"[random(&mut seed, 5)])
            .collect();
        let path: Vec<u8> = (0..random(&mut seed, 9))
            .map(|_| b"ab/$"[random(&mut seed, 4)])
            .collect();
        let expected = plain_match(&pattern, &path);
        assert_eq!(
            rule_matches(&pattern, &path),
            expected,
            "{:?} {:?}",
            String::from_utf8_lossy(&pattern),
            String::from_utf8_lossy(&path)
        );
        matched += usize::from(expected);
    }
    assert!((20_000..180_000).contains(&matched));
    // hostile files: any bytes, cut anywhere, must be read without a panic
    let pieces: [&[u8]; 14] = [
        b"User-agent",
        b"Disallow",
        b"Allow",
        b":",
        b"*",
        b"$",
        b"\n",
        b"\r",
        b"#",
        b"/",
        b"%",
        b"%f",
        b"docbot",
        &[0xff, 0xC3, 0],
    ];
    for _ in 0..20_000 {
        let mut file = Vec::new();
        for _ in 0..random(&mut seed, 60) {
            file.extend_from_slice(pieces[random(&mut seed, pieces.len())]);
        }
        file.truncate(random(&mut seed, file.len() + 1));
        let robots = parse_robots(&file, "docbot");
        let _ = (
            robots.allows("/%"),
            robots.allows("/é%2"),
            robots.allows(""),
        );
    }
    println!("robots against a plain matcher, and hostile files: ok");
}

fn retry_after_checks() {
    // the example date from RFC 9110
    assert_eq!(
        http_date("Sun, 06 Nov 1994 08:49:37 GMT"),
        Some(784_111_777)
    );
    assert_eq!(http_date("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
    assert_eq!(
        http_date("Tue, 29 Feb 2000 12:00:00 GMT"),
        Some(951_825_600)
    );
    // compare the day count with a plain walk through the calendar
    let mut expected = 0;
    for year in 1970..2200 {
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        for (month, length) in [
            31,
            if leap { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ]
        .into_iter()
        .enumerate()
        {
            for day in 1..=length {
                assert_eq!(days_from_civil(year, month as i64 + 1, day), expected);
                expected += 1;
            }
        }
    }
    let now = 784_111_700;
    assert_eq!(retry_after_ms("120", now), Some(120_000));
    assert_eq!(retry_after_ms(" 0 ", now), Some(0));
    assert_eq!(
        retry_after_ms("99999999999999999999999", now),
        Some(u64::MAX)
    );
    assert_eq!(
        retry_after_ms("Sun, 06 Nov 1994 08:49:37 GMT", now),
        Some(77_000)
    );
    assert_eq!(
        retry_after_ms("Sun, 06 Nov 1994 08:49:37 GMT", now + 1000),
        Some(0)
    );
    for unreadable in [
        "",
        "-1",
        "+5",
        "1.5",
        "soon",
        "Sunday, 06-Nov-94 08:49:37 GMT",
        "Sun Nov  6 08:49:37 1994",
        "Sun, 06 Nov 1994 08:49:37 UTC",
        "Sun, é6 Nov 1994 08:49:37 GMT",
        "Sun, 06 Xyz 1994 08:49:37 GMT",
        "Sun, 06 Nov 1994 24:00:00 GMT",
    ] {
        assert_eq!(retry_after_ms(unreadable, now), None, "{unreadable:?}");
    }
    let mut seed = 11u64;
    for _ in 0..100_000 {
        let mut bytes = b"Sun, 06 Nov 1994 08:49:37 GMT".to_vec();
        for _ in 0..=random(&mut seed, 3) {
            let at = random(&mut seed, bytes.len());
            bytes[at] = random(&mut seed, 256) as u8;
        }
        bytes.truncate(random(&mut seed, bytes.len() + 4));
        let _ = retry_after_ms(&String::from_utf8_lossy(&bytes), now);
    }
    println!("Retry-After: ok");
}

const TEST_LIMITS: Limits = Limits {
    gap_ms: 1000,
    backoff_base_ms: 2000,
    backoff_cap_ms: 60_000,
    longest_wait_ms: 600_000,
    strikes: 4,
};

fn pace_checks() {
    let mut pace = Pace::new(TEST_LIMITS, 1);
    assert_eq!(pace.before("a.test", 0), Go::Now);
    assert_eq!(pace.after("a.test", 300, Some(200), None, 0), Verdict::Done);
    assert_eq!(pace.before("a.test", 300), Go::NotBefore(1300));
    assert_eq!(pace.before("a.test", 1300), Go::Now);
    assert_eq!(
        pace.before("b.test", 300),
        Go::Now,
        "another host has its own pace"
    );

    // Retry-After holds the whole host, and is never cut short
    assert_eq!(
        pace.after("a.test", 2000, Some(429), Some("30"), 0),
        Verdict::Retry
    );
    let Go::NotBefore(ready) = pace.before("a.test", 31_999) else {
        panic!("asked too soon")
    };
    assert!((32_000..=33_000).contains(&ready), "{ready}");
    // an answer that arrives later, with a shorter wait, does not bring the time forward
    pace.after("a.test", 2000, Some(200), None, 0);
    assert_eq!(pace.before("a.test", 2000), Go::NotBefore(ready));

    for status in [401, 403, 407, 451] {
        let mut pace = Pace::new(TEST_LIMITS, 1);
        assert_eq!(
            pace.after("a.test", 0, Some(status), None, 0),
            Verdict::StopHost(Stop::Refused(status))
        );
        // later answers and a long wait change nothing
        pace.after("a.test", 1, Some(200), None, 0);
        assert_eq!(
            pace.before("a.test", u64::MAX),
            Go::Stopped(Stop::Refused(status))
        );
    }
    let mut pace = Pace::new(TEST_LIMITS, 1);
    assert_eq!(
        pace.after("a.test", 0, Some(404), None, 0),
        Verdict::SkipPage
    );
    assert_eq!(
        pace.after("a.test", 0, Some(503), Some("3600"), 0),
        Verdict::StopHost(Stop::AsksTooMuch)
    );
    let mut pace = Pace::new(TEST_LIMITS, 1);
    assert_eq!(
        pace.after("a.test", 0, Some(429), Some("a while"), 0),
        Verdict::StopHost(Stop::UnreadableWait)
    );

    // failures in a row end the visit, and a success starts the count again
    let mut pace = Pace::new(TEST_LIMITS, 1);
    for status in [
        Some(500),
        None,
        Some(502),
        Some(200),
        Some(500),
        Some(504),
        Some(503),
    ] {
        assert_ne!(
            pace.after("a.test", 0, status, None, 0),
            Verdict::StopHost(Stop::KeptFailing),
            "{status:?}"
        );
    }
    assert_eq!(
        pace.after("a.test", 0, Some(429), None, 0),
        Verdict::StopHost(Stop::KeptFailing)
    );

    // a Retry-After is honoured whatever the status, and does not start the failure count again
    for status in [200, 204, 301, 302, 404, 408, 500, 507, 509, 520, 529] {
        let mut pace = Pace::new(TEST_LIMITS, 1);
        pace.after("a.test", 0, Some(503), None, 0);
        pace.after("a.test", 0, Some(status), Some("500"), 0);
        let Go::NotBefore(ready) = pace.before("a.test", 499_999) else {
            panic!("{status}: asked too soon")
        };
        assert!(ready >= 500_000, "{status}");
        assert!(pace.hosts["a.test"].strikes >= 1, "{status}");
        let mut pace = Pace::new(TEST_LIMITS, 1);
        assert_eq!(
            pace.after("a.test", 0, Some(status), Some("3600"), 0),
            Verdict::StopHost(Stop::AsksTooMuch),
            "{status}"
        );
    }
    // statuses that mean "not now" are failures without the header too
    for status in [408, 501, 507, 509, 520, 529] {
        let mut pace = Pace::new(TEST_LIMITS, 1);
        assert_eq!(
            pace.after("a.test", 0, Some(status), None, 0),
            Verdict::Retry,
            "{status}"
        );
    }
    // neither a missing page nor a success that asks for a wait wipes the count
    let mut pace = Pace::new(TEST_LIMITS, 1);
    for (status, header) in [
        (503, None),
        (503, None),
        (404, None),
        (200, Some("1")),
        (503, None),
    ] {
        pace.after("a.test", 0, Some(status), header, 0);
    }
    assert_eq!(
        pace.after("a.test", 0, Some(503), None, 0),
        Verdict::StopHost(Stop::KeptFailing)
    );

    // the first reason to stop is the one that is kept
    let mut pace = Pace::new(TEST_LIMITS, 1);
    pace.after("a.test", 0, Some(403), None, 0);
    pace.after("a.test", 1, Some(401), None, 0);
    assert_eq!(pace.before("a.test", 2), Go::Stopped(Stop::Refused(403)));
    // and no other spelling of the host gets round it
    for spelling in [
        "A.test",
        "a.TEST.",
        "a.test..",
        "a.test:80",
        "a.test:443",
        "a.test.:8080",
        "%61.test",
    ] {
        assert_eq!(
            pace.before(spelling, 2),
            Go::Stopped(Stop::Refused(403)),
            "{spelling}"
        );
    }
    assert_eq!(pace.before("b.test", 2), Go::Now);
    for bad in [
        "",
        ".",
        "a.test/x",
        "http://a.test",
        "user@a.test",
        " a.test",
        "a.test?x",
        "a.test\\x",
        "a.test:x",
    ] {
        assert_eq!(pace.before(bad, 2), Go::Stopped(Stop::BadHost), "{bad:?}");
        assert_eq!(
            pace.after(bad, 2, Some(200), None, 0),
            Verdict::StopHost(Stop::BadHost)
        );
    }

    // an enormous wait must not wrap round to "now", even with no limit on waiting
    let endless = Limits {
        longest_wait_ms: u64::MAX,
        ..TEST_LIMITS
    };
    for seed in 0..50 {
        let mut pace = Pace::new(endless, seed);
        pace.after("a.test", 5, Some(429), Some("99999999999999999999"), 0);
        assert_eq!(
            pace.before("a.test", 6),
            Go::NotBefore(u64::MAX),
            "seed {seed}"
        );
    }

    // backoff: never under the gap, never over the ceiling for that attempt, and not always the same
    let mut seen = std::collections::HashSet::new();
    for seed in 0..500 {
        let mut pace = Pace::new(TEST_LIMITS, seed * 2654435761);
        let mut now = 0;
        for attempt in 0..3 {
            assert_eq!(
                pace.after("a.test", now, Some(503), None, 0),
                Verdict::Retry
            );
            let Go::NotBefore(ready) = pace.before("a.test", now) else {
                panic!("no wait")
            };
            let ceiling = (2000u64 << attempt).min(60_000);
            assert!(
                ready - now >= 1000 && ready - now <= ceiling,
                "attempt {attempt}: {}",
                ready - now
            );
            if attempt == 2 {
                seen.insert(ready - now);
            }
            now = ready;
        }
    }
    assert!(
        seen.len() > 100,
        "the waits must differ between clients: {}",
        seen.len()
    );
    println!("pacing and stopping: ok");
}

/// Crawls a pretend site on a pretend clock and checks what the site saw.
fn simulated_visits() {
    let robots = parse_robots(b"User-agent: *\nDisallow: /private\n", "docbot");
    let paths: Vec<String> = (0..30)
        .map(|i| {
            if i % 5 == 0 {
                format!("/private/{i}")
            } else {
                format!("/page/{i}")
            }
        })
        .collect();
    let mut endings = [0u32; 2];
    for run in 0..2000u64 {
        let mut seed = run * 31 + 1;
        let mut pace = Pace::new(TEST_LIMITS, run);
        let mut now = 0;
        // what the pretend site saw: (time of the request, status, wait it asked for, path)
        let mut seen: Vec<(u64, u16, u64, &str)> = Vec::new();
        let mut stopped = false;
        'pages: for path in paths.iter().filter(|p| robots.allows(p)) {
            loop {
                match pace.before("a.test", now) {
                    Go::NotBefore(at) => now = at,
                    Go::Stopped(_) => {
                        stopped = true;
                        break 'pages;
                    }
                    Go::Now => {
                        let status = [200, 200, 200, 200, 200, 200, 404, 429, 503, 500, 403]
                            [random(&mut seed, 11)];
                        let asked = if status == 429 {
                            random(&mut seed, 20) as u64
                        } else {
                            0
                        };
                        seen.push((now, status, asked * 1000, path));
                        let header = (status == 429).then(|| asked.to_string());
                        now += random(&mut seed, 500) as u64; // the answer takes a while
                        if pace.after("a.test", now, Some(status), header.as_deref(), 0)
                            != Verdict::Retry
                        {
                            break;
                        }
                    }
                }
            }
        }
        assert!(
            seen.iter().all(|s| !s.3.starts_with("/private")),
            "run {run}: asked for a disallowed page"
        );
        for pair in seen.windows(2) {
            assert!(
                pair[1].0 - pair[0].0 >= TEST_LIMITS.gap_ms,
                "run {run}: requests too close"
            );
            assert!(
                pair[1].0 - pair[0].0 >= pair[0].2,
                "run {run}: came back before the time asked"
            );
            assert_ne!(pair[0].1, 403, "run {run}: asked again after a refusal");
        }
        endings[usize::from(stopped)] += 1;
    }
    assert!(
        endings.iter().all(|n| *n > 50),
        "both endings must happen: {endings:?}"
    );
    println!("2000 simulated visits: ok");
}

/// What a server really receives, on a local socket.
async fn user_agent_check() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut seen = Vec::new();
        for _ in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = socket.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buffer[..n]);
            }
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
            let text = String::from_utf8(request).unwrap().to_ascii_lowercase();
            seen.push(
                text.lines()
                    .find(|l| l.starts_with("user-agent:"))
                    .map(str::to_string),
            );
        }
        seen
    });
    let name = user_agent("docbot", "1.0", "https://example.org/bot").unwrap();
    for builder in [
        reqwest::Client::builder(),
        reqwest::Client::builder().user_agent(&name),
    ] {
        let client = builder
            .no_proxy()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        client
            .get(format!("http://{address}/robots.txt"))
            .send()
            .await
            .unwrap();
    }
    let seen = tokio::time::timeout(std::time::Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        seen[0], None,
        "reqwest sends no user agent unless you set one"
    );
    assert_eq!(
        seen[1].as_deref(),
        Some("user-agent: docbot/1.0 (+https://example.org/bot)")
    );
    for (token, contact) in [
        ("doc bot", "https://example.org/bot"),
        ("docbot2", "https://example.org/bot"),
        ("", "x"),
        ("docbot", "a\r\nCookie: 1"),
    ] {
        assert_eq!(
            user_agent(token, "1.0", contact),
            None,
            "{token:?} {contact:?}"
        );
    }
    println!("user agent: ok");
}

#[tokio::main]
async fn main() {
    robots_checks();
    random_robots_checks();
    retry_after_checks();
    pace_checks();
    simulated_visits();
    user_agent_check().await;
    println!("all polite fetching checks passed");
}
