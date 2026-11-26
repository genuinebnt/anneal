//! RECIPE 915 (web): security pitfalls as testable vulnerable/fixed pairs: injection, traversal, SSRF, XSS, ReDoS, timing, randomness, limits, CSRF
//! LEVEL: intermediate
//! Use in: reviewing request handlers, writing security regression tests, learning why the obvious fix is not enough.   Run: cargo run --release --example security_pitfalls_secure_coding
//! COMMON USES: any service that takes input from outside: web handlers, file servers, webhooks that fetch URLs, protocol parsers, admin tools.
#![allow(dead_code)]
//!
//! DEFENSIVE TEST FIXTURES. Every attack string here exists so a test can show that the FIXED function resists it. Everything runs
//! in-process or in a private temporary directory: no network, and the only "payload" any command runs is `echo`. The `vuln_*`
//! functions are deliberately broken: never copy them.
//! THE SHAPE OF EACH PAIR: (1) the attack SUCCEEDS against `vuln_*` (so the test really exercises the bug), (2) the same attack
//! FAILS against the fixed version, (3) ordinary input still works (a fix that rejects everything proves nothing).
//! THE COMMON ROOT: data crossing into a place where it is interpreted as CODE or STRUCTURE (SQL, shell, path, HTML, URL, log
//! line, object fields). The fix is to keep data and structure separate (parameters, argument vectors, typed DTOs), or where that
//! is impossible, to PARSE strictly, then check the parsed value, then use THAT value (not the original string).
//! WHAT RUST DOES AND DOES NOT DO: safe Rust prevents out-of-bounds reads, use-after-free and data races. It does NOT prevent
//! injection, traversal, logic errors, secrets left in a reused buffer, arithmetic wrapping in release builds (unless
//! `overflow-checks` is on), unbounded recursion (a stack overflow aborts the process) or unbounded allocation.
//! LIMITS: these are focused demonstrations, not a security library. In production use maintained crates (`url`, `ammonia` or a
//! templating engine with auto-escaping, `regex`, `subtle`, `getrandom`) and get a review. The timing demo counts comparisons
//! instead of measuring time; constant-time guarantees against an optimising compiler need `subtle` or assembly-level checks.
//! Unix only for the symlink, TOCTOU, shell and /dev/urandom parts.
//! Read: https://cheatsheetseries.owasp.org/ and https://owasp.org/www-community/attacks/Path_Traversal
//! Read: https://doc.rust-lang.org/std/process/struct.Command.html and https://swtch.com/~rsc/regexp/regexp1.html
//!
//! TRY NEXT:
//!   - [S] In `fixed_resolve` remove the second (canonicalised) check and predict which attack gets through.
//!   - [S] Add `100.64.0.0/10` (carrier-grade NAT) to an attack list and check whether `is_public` blocks it; read the code to see why.
//!   - [M] Add a CSS-context encoder and say why "escape it" is not an acceptable answer for a CSS `url()`.
//!   - [M] Make `fixed_open_no_follow` safe against a symlinked PARENT directory (hint: openat one component at a time).
//!   - [A] PROJECT: wrap three of these as `axum` extractors (`SafePath`, `SafeRedirect`, `CsrfGuarded`) so a handler cannot receive an unchecked value.

use rusqlite::{Connection, params};
use serde::Deserialize;
use std::cell::Cell;
use std::collections::{BTreeSet, HashMap};
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt, symlink};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

// ===========================================================================
// 1. SQL injection
// ===========================================================================
fn db() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch(
        "CREATE TABLE users(name TEXT, pw_hash TEXT, secret TEXT);
         INSERT INTO users VALUES ('alice','h1','alice-secret'), ('bob','h2','bob-secret'), ('o''brien','h3','irish');",
    )
    .unwrap();
    c
}
fn vuln_login(c: &Connection, name: &str, hash: &str) -> rusqlite::Result<Vec<String>> {
    let sql = format!("SELECT name FROM users WHERE name = '{name}' AND pw_hash = '{hash}'");
    c.prepare(&sql)?.query_map([], |r| r.get(0))?.collect()
}
fn fixed_login(c: &Connection, name: &str, hash: &str) -> rusqlite::Result<Vec<String>> {
    c.prepare("SELECT name FROM users WHERE name = ?1 AND pw_hash = ?2")?
        .query_map(params![name, hash], |r| r.get(0))?
        .collect()
}
/// Identifiers and keywords CANNOT be bound as parameters: map user input to a fixed set of query fragments.
fn fixed_list(c: &Connection, sort: &str) -> Option<Vec<String>> {
    let column = match sort {
        "name" => "name",
        "newest" => "rowid DESC",
        _ => return None,
    };
    c.prepare(&format!("SELECT name FROM users ORDER BY {column}"))
        .ok()?
        .query_map([], |r| r.get(0))
        .ok()?
        .collect::<rusqlite::Result<_>>()
        .ok()
}
fn sql_injection() {
    let c = db();
    let bypass = "' OR '1'='1";
    assert_eq!(
        vuln_login(&c, "x", bypass).unwrap().len(),
        3,
        "attack: log in as everybody without a password"
    );
    let union = "x' UNION SELECT secret FROM users --";
    assert!(
        vuln_login(&c, union, "")
            .unwrap()
            .contains(&"bob-secret".to_string()),
        "attack: read another column"
    );
    assert!(
        fixed_login(&c, "x", bypass).unwrap().is_empty()
            && fixed_login(&c, union, "").unwrap().is_empty()
    );
    assert!(
        vuln_login(&c, "o'brien", "h3").is_err(),
        "the same bug also breaks honest users"
    );
    assert_eq!(fixed_login(&c, "o'brien", "h3").unwrap(), ["o'brien"]);
    assert_eq!(fixed_list(&c, "newest").unwrap()[0], "o'brien");
    assert!(fixed_list(&c, "name; DROP TABLE users").is_none());
}

// ===========================================================================
// 2. path traversal, 3. TOCTOU
// ===========================================================================
struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("cb-security-{}-{nanos}", std::process::id()));
        // fails if the name already exists; 0o700 keeps other users out
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&p)
            .expect("fresh directory");
        TempDir(p.canonicalize().unwrap())
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn vuln_join(base: &Path, user: &str) -> Option<String> {
    std::fs::read_to_string(base.join(user)).ok()
}
/// The popular "fix" number one: delete the bad substring once.
fn vuln_strip(base: &Path, user: &str) -> Option<String> {
    std::fs::read_to_string(base.join(user.replace("../", ""))).ok()
}
/// "Fix" number two: canonicalise, then compare STRINGS.
fn vuln_string_prefix(base: &Path, user: &str) -> Option<String> {
    let full = base.join(user).canonicalize().ok()?;
    full.to_str()?
        .starts_with(base.to_str()?)
        .then(|| std::fs::read_to_string(full).ok())
        .flatten()
}
/// "Fix" number three: purely lexical checks, blind to symlinks.
fn vuln_lexical_only(base: &Path, user: &str) -> Option<String> {
    let ok = Path::new(user)
        .components()
        .all(|c| matches!(c, Component::Normal(_)));
    ok.then(|| std::fs::read_to_string(base.join(user)).ok())
        .flatten()
}
fn fixed_resolve(base: &Path, user: &str) -> Option<PathBuf> {
    // reject backslashes and NUL outright: on Windows `\` is a separator, and another layer may treat it as one
    if user.is_empty() || user.contains(['\\', '\0']) {
        return None;
    }
    // only ordinary names: no root, no drive prefix, no `..`
    if !Path::new(user)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
    {
        return None;
    }
    // then follow symlinks and check where we REALLY end up, comparing whole components (`Path::starts_with`)
    let real = base.join(user).canonicalize().ok()?;
    real.starts_with(base.canonicalize().ok()?).then_some(real)
}
fn path_traversal() {
    let tmp = TempDir::new();
    let mode = std::fs::metadata(&tmp.0).unwrap().permissions().mode();
    assert_eq!(mode & 0o077, 0, "the scratch directory is private");
    let (base, sibling) = (tmp.0.join("public"), tmp.0.join("public-private"));
    for d in [&base, &sibling, &base.join("docs")] {
        std::fs::create_dir(d).unwrap();
    }
    std::fs::write(base.join("docs/a.txt"), "public text").unwrap();
    std::fs::write(tmp.0.join("secret.txt"), "TOP SECRET").unwrap();
    std::fs::write(sibling.join("key"), "SIBLING SECRET").unwrap();
    symlink(tmp.0.join("secret.txt"), base.join("shortcut")).unwrap();
    let secret = Some("TOP SECRET".to_string());
    let absolute = tmp.0.join("secret.txt").to_str().unwrap().to_string();

    assert_eq!(vuln_join(&base, "../secret.txt"), secret, "attack: dot-dot");
    assert_eq!(
        vuln_join(&base, &absolute),
        secret,
        "attack: an absolute argument REPLACES the base in Path::join"
    );
    assert_eq!(
        vuln_strip(&base, "....//secret.txt"),
        secret,
        "attack: stripping once creates a new ../"
    );
    assert_eq!(
        vuln_string_prefix(&base, "../public-private/key"),
        Some("SIBLING SECRET".into()),
        "attack: sibling shares the string prefix"
    );
    assert_eq!(
        vuln_lexical_only(&base, "shortcut"),
        secret,
        "attack: a symlink inside the base points outside"
    );

    for attack in [
        "../secret.txt",
        &absolute,
        "....//secret.txt",
        "../public-private/key",
        "shortcut",
        "docs/../../secret.txt",
        "..\\secret.txt",
        "docs\\..\\..\\x",
        "a\0b",
        "",
        "..",
    ] {
        assert!(
            fixed_resolve(&base, attack).is_none(),
            "{attack:?} must be refused"
        );
    }
    let ok = fixed_resolve(&base, "docs/a.txt").unwrap();
    assert_eq!(std::fs::read_to_string(ok).unwrap(), "public text");
}

/// Check then use, with `between` standing for an attacker who runs at the worst moment (deterministic, no race needed).
fn vuln_checked_write(path: &Path, data: &str, between: impl FnOnce()) -> std::io::Result<()> {
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(std::io::Error::other("refusing symlink"));
    }
    between();
    std::fs::write(path, data)
}
/// One system call both checks and opens, so there is no gap. O_NOFOLLOW covers the LAST component only, and
/// it does not stop a HARD link: that is the same file under a second name.
fn fixed_open_no_follow(path: &Path, data: &str, between: impl FnOnce()) -> std::io::Result<()> {
    between();
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .custom_flags(libc::O_NOFOLLOW)
        .mode(0o600)
        .open(path)?;
    f.write_all(data.as_bytes())
}
fn toctou() {
    let tmp = TempDir::new();
    let (victim, spool) = (tmp.0.join("victim.conf"), tmp.0.join("spool"));
    std::fs::write(&victim, "original").unwrap();
    let swap = || {
        let _ = std::fs::remove_file(&spool);
        symlink(&victim, &spool).unwrap();
    };
    std::fs::write(&spool, "").unwrap();
    vuln_checked_write(&spool, "attacker data", swap).unwrap();
    assert_eq!(
        std::fs::read_to_string(&victim).unwrap(),
        "attacker data",
        "attack: swapped after the check"
    );
    std::fs::write(&victim, "original").unwrap();
    assert!(fixed_open_no_follow(&spool, "attacker data", swap).is_err());
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "original");
    // creating a NEW file: create_new (O_CREAT|O_EXCL) fails on anything already there, symlinks included
    let create = |p: &Path| {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(p)
    };
    assert!(create(&spool).is_err() && create(&tmp.0.join("fresh")).is_ok());
}

// ===========================================================================
// 4. command and argument injection
// ===========================================================================
fn run(cmd: &mut Command) -> String {
    let out = cmd.stdin(Stdio::null()).output().expect("spawn");
    String::from_utf8_lossy(&out.stdout).into_owned()
}
fn vuln_greet(name: &str) -> String {
    run(Command::new("/bin/sh")
        .arg("-c")
        .arg(format!("echo hello {name}")))
}
/// No shell: the string arrives in the child as exactly one argument, whatever it contains.
fn fixed_greet(name: &str) -> String {
    run(Command::new("/bin/echo").arg("hello").arg(name))
}
fn vuln_search(pattern: &str, file: &Path) -> String {
    run(Command::new("grep").arg(pattern).arg(file))
}
fn fixed_search(pattern: &str, file: &Path) -> String {
    // grep reads a newline in a pattern as "another pattern", and an empty pattern matches every line
    if pattern.is_empty() || pattern.contains('\n') {
        return String::new();
    }
    // -F: literal text; -e: the next argument is the pattern even if it starts with '-'; --: no options after this
    run(Command::new("grep")
        .arg("-F")
        .arg("-e")
        .arg(pattern)
        .arg("--")
        .arg(file))
}
fn command_injection() {
    for attack in [
        "x; echo INJECTED",
        "$(echo INJECTED)",
        "`echo INJECTED`",
        "x\necho INJECTED",
    ] {
        let out = vuln_greet(attack);
        assert!(
            out.contains("INJECTED") && !out.contains("echo INJECTED"),
            "attack ran: {attack:?} -> {out:?}"
        );
        assert_eq!(
            fixed_greet(attack),
            format!("hello {attack}\n"),
            "inert: printed back verbatim"
        );
    }
    assert_eq!(fixed_greet("world"), "hello world\n");
    let tmp = TempDir::new();
    let file = tmp.0.join("notes");
    std::fs::write(&file, "public line\ninternal: do not show\n").unwrap();
    // ARGUMENT injection needs no shell: "-e." is read by grep as the OPTION "pattern is ." which matches every line
    assert!(vuln_search("-e.", &file).contains("do not show"));
    assert_eq!(fixed_search("-e.", &file), "");
    assert_eq!(fixed_search("public", &file), "public line\n");
    // one "literal" argument can still hold several patterns: "zzz" or the empty one, which matches everything
    let two_patterns = "zzz\n";
    let raw = run(Command::new("grep")
        .args(["-F", "-e", two_patterns, "--"])
        .arg(&file));
    assert!(
        raw.contains("do not show"),
        "attack: -F -e -- is not enough"
    );
    assert_eq!(fixed_search(two_patterns, &file), "");
    assert_eq!(fixed_search("", &file), "");
}

// ===========================================================================
// 5. cross-site scripting: escaping depends on the CONTEXT
// ===========================================================================
/// For HTML text and for QUOTED attribute values.
fn html_escape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            '\'' => "&#39;".into(),
            c => c.to_string(),
        })
        .collect()
}
/// One query-string component: everything outside the unreserved set is percent encoded.
fn url_component(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
/// For the inside of a quoted JavaScript string, also safe inside <script>: only [A-Za-z0-9 ] pass through.
fn js_string(s: &str) -> String {
    s.encode_utf16()
        .map(|u| match u8::try_from(u) {
            Ok(b) if b.is_ascii_alphanumeric() || b == b' ' => char::from(b).to_string(),
            _ => format!("\\u{u:04x}"),
        })
        .collect()
}
/// A link target chosen by a user: allow schemes, do not block them ("javascript:" has many spellings).
fn safe_href(url: &str) -> Option<String> {
    let lower = url.to_ascii_lowercase();
    let allowed = lower.starts_with("https://")
        || lower.starts_with("http://")
        || (url.starts_with('/') && !url.starts_with("//") && !url.contains('\\'));
    (allowed && !url.chars().any(char::is_control)).then(|| html_escape(url))
}
fn xss() {
    let tag = "<img src=x onerror=alert(1)>";
    assert!(
        format!("<p>{tag}</p>").contains("<img"),
        "attack: unescaped text becomes markup"
    );
    assert!(!html_escape(tag).contains(['<', '>']));
    let breakout = "\" onmouseover=\"alert(1)";
    assert!(
        format!("<a title=\"{breakout}\">").contains("onmouseover=\"alert"),
        "attack: close the attribute"
    );
    assert!(!html_escape(breakout).contains('"'));
    // WRONG CONTEXT 1: correct HTML escaping in an UNQUOTED attribute does nothing against spaces
    assert!(format!("<a title={}>", html_escape("x onclick=alert(1)")).contains(" onclick="));
    // WRONG CONTEXT 2: HTML escaping does not neutralise a dangerous URL scheme
    assert_eq!(
        html_escape("javascript:alert(1)"),
        "javascript:alert(1)",
        "attack survives HTML escaping"
    );
    for bad in [
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        " javascript:alert(1)",
        "java\tscript:alert(1)",
        "data:text/html,x",
        "//evil.example",
        "/\\evil.example",
    ] {
        assert_eq!(safe_href(bad), None, "{bad:?}");
    }
    assert_eq!(
        safe_href("https://example.com/?a=1&b=2").unwrap(),
        "https://example.com/?a=1&amp;b=2"
    );
    // WRONG CONTEXT 3: a script block ends at "</script>" even inside a JS string
    let end_script = "</script><script>alert(1)//";
    assert!(
        format!("var s = '{}';", end_script.replace('\'', "\\'")).contains("</script>"),
        "attack: quote escaping is not enough"
    );
    assert!(!js_string(end_script).contains(['<', '>', '/', '\'', '"']));
    assert_eq!(js_string("a'😀"), "a\\u0027\\ud83d\\ude00");
    assert_eq!(
        url_component("a&admin=true#x"),
        "a%26admin%3Dtrue%23x",
        "a value cannot add parameters"
    );
    // property test: no escaper output ever contains a character that is special in its context
    let alphabet = [
        '<', '>', '"', '\'', '&', '\\', '\n', '/', 'a', ' ', 'é', '=', '#', '%', '\u{2028}',
    ];
    let mut r = Rng(5);
    for _ in 0..5000 {
        let s: String = (0..r.below(12))
            .map(|_| alphabet[r.below(alphabet.len() as u64) as usize])
            .collect();
        assert!(!html_escape(&s).contains(['<', '>', '"', '\'']));
        assert!(!js_string(&s).contains(['<', '>', '"', '\'', '/', '\n', '\u{2028}']));
        assert!(
            url_component(&s)
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-._~%".contains(c))
        );
    }
}

// ===========================================================================
// 6. SSRF, 7. open redirect
// ===========================================================================
/// What the classic C `inet_aton` accepts (many HTTP stacks end up here): 1 to 4 parts, each decimal, octal or hex.
fn inet_aton(s: &str) -> Option<Ipv4Addr> {
    let parts: Vec<u64> = s
        .split('.')
        .map(|p| {
            let (digits, radix) = match p.strip_prefix("0x").or_else(|| p.strip_prefix("0X")) {
                Some(hex) => (hex, 16),
                None if p.len() > 1 && p.starts_with('0') => (&p[1..], 8),
                None => (p, 10),
            };
            digits
                .chars()
                .all(|c| c.is_digit(radix))
                .then(|| u64::from_str_radix(digits, radix).ok())
                .flatten()
        })
        .collect::<Option<_>>()?;
    let (last, head) = parts.split_last()?;
    if head.len() > 3 || head.iter().any(|&p| p > 255) || *last >= 1u64 << (8 * (4 - head.len())) {
        return None;
    }
    let high = head
        .iter()
        .enumerate()
        .fold(0u64, |acc, (i, &p)| acc | p << (24 - 8 * i));
    Some(Ipv4Addr::from(u32::try_from(high | last).ok()?))
}
/// An ALLOW rule for ordinary global unicast; everything special is refused.
fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let [a, b, c, _] = v.octets();
            !(v.is_private()
                || v.is_loopback()
                || v.is_link_local()
                || v.is_broadcast()
                || v.is_documentation()
                || a == 0
                || a >= 224
                || (a == 100 && (64..128).contains(&b))
                || (a == 192 && b == 0 && c == 0)
                || (a == 198 && (b == 18 || b == 19)))
        }
        // only 2000::/3 is global unicast; mapped, loopback, ULA and link-local all fall outside it. 2002::/16 (6to4) embeds IPv4.
        IpAddr::V6(v) => {
            (v.segments()[0] & 0xe000) == 0x2000
                && v.segments()[0] != 0x2002
                && !(v.segments()[0] == 0x2001 && v.segments()[1] <= 0x01ff)
                && !(v.segments()[0] == 0x2001 && v.segments()[1] == 0x0db8)
        }
    }
}
type Resolver<'a> = &'a dyn Fn(&str) -> Vec<IpAddr>;
/// The usual blocklist on the URL STRING; returns the address the request would really reach.
fn vuln_fetch_target(url: &str, resolve: Resolver) -> Option<IpAddr> {
    if url.contains("127.0.0.1") || url.contains("localhost") || url.contains("169.254.169.254") {
        return None;
    }
    connect_target(url, resolve)
}
/// What a lenient HTTP client does with the same string.
fn connect_target(url: &str, resolve: Resolver) -> Option<IpAddr> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit_once('@').map_or(authority, |x| x.1);
    if let Some(v6) = host_port.strip_prefix('[') {
        return v6
            .split(']')
            .next()?
            .parse::<Ipv6Addr>()
            .ok()
            .map(IpAddr::V6);
    }
    let host = host_port.split(':').next()?;
    inet_aton(host)
        .map(IpAddr::V4)
        .or_else(|| resolve(host).first().copied())
}
/// Strict parse, exact host allow-list, resolve ONCE, check every address, return the address to connect to (pinning).
fn fixed_fetch_target(url: &str, allowed_hosts: &[&str], resolve: Resolver) -> Option<IpAddr> {
    let rest = url.strip_prefix("https://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    // no userinfo, port, brackets, escapes or backslashes: the authority must BE one of the allowed names
    if !allowed_hosts
        .iter()
        .any(|h| authority.eq_ignore_ascii_case(h))
    {
        return None;
    }
    let addresses = resolve(&authority.to_ascii_lowercase());
    (!addresses.is_empty() && addresses.iter().all(|&a| is_public(a))).then(|| addresses[0])
}
fn ssrf() {
    let dns = |host: &str| -> Vec<IpAddr> {
        match host {
            "api.example.com" => vec!["93.184.215.14".parse().unwrap()],
            "api.example.com.evil.test" | "rebind.test" | "partner.example.com" => {
                vec!["10.0.0.7".parse().unwrap()]
            }
            "mixed.example.com" => vec![
                "93.184.215.14".parse().unwrap(),
                "127.0.0.1".parse().unwrap(),
            ],
            _ => vec![],
        }
    };
    let attacks = [
        "http://2130706433/",      // decimal 127.0.0.1
        "http://017700000001/",    // octal
        "http://0x7f.1/",          // hex, two parts
        "http://127.1/",           // short form
        "http://[::ffff:7f00:1]/", // IPv4-mapped IPv6 spelled in hex
        "http://[::1]/",
        "http://2852039166/latest/meta-data/", // 169.254.169.254, the cloud metadata address
        "http://api.example.com@10.0.0.7/",    // userinfo that LOOKS like the host
        "http://api.example.com.evil.test/",
    ];
    for a in &attacks {
        let reached = vuln_fetch_target(a, &dns).unwrap_or_else(|| panic!("blocklist stopped {a}"));
        assert!(
            !is_public(reached),
            "attack reaches an internal address: {a} -> {reached}"
        );
        assert_eq!(
            fixed_fetch_target(a, &["api.example.com"], &dns),
            None,
            "{a}"
        );
        let https = a.replacen("http://", "https://", 1);
        assert_eq!(
            fixed_fetch_target(&https, &["api.example.com"], &dns),
            None,
            "{https}"
        );
    }
    assert!(
        "2130706433".parse::<Ipv4Addr>().is_err() && "127.1".parse::<Ipv4Addr>().is_err(),
        "std is strict; other parsers are not: never validate with one parser and connect with another"
    );
    // an ALLOWED name that resolves inside (or partly inside) is still refused
    assert_eq!(
        fixed_fetch_target(
            "https://partner.example.com/",
            &["partner.example.com"],
            &dns
        ),
        None
    );
    assert_eq!(
        fixed_fetch_target("https://mixed.example.com/", &["mixed.example.com"], &dns),
        None
    );
    assert_eq!(
        fixed_fetch_target("https://api.example.com/v1?u=1", &["api.example.com"], &dns),
        Some("93.184.215.14".parse().unwrap())
    );
    // DNS REBINDING: the name answers "public" to the check and "internal" to the connect
    let answers = Cell::new(0);
    let rebinding = |_: &str| -> Vec<IpAddr> {
        answers.set(answers.get() + 1);
        vec![
            if answers.get() == 1 {
                "93.184.215.14"
            } else {
                "127.0.0.1"
            }
            .parse()
            .unwrap(),
        ]
    };
    let check_then_connect = |url: &str| {
        connect_target(url, &rebinding).filter(|&a| is_public(a))?; // the check
        connect_target(url, &rebinding) // resolves AGAIN
    };
    assert_eq!(
        check_then_connect("https://rebind.test/"),
        Some("127.0.0.1".parse().unwrap()),
        "attack: rebinding"
    );
    answers.set(0);
    let pinned = fixed_fetch_target("https://rebind.test/", &["rebind.test"], &rebinding).unwrap();
    assert!(
        is_public(pinned) && answers.get() == 1,
        "the caller must connect to THIS address (and re-check every redirect)"
    );
    for ip in ["8.8.8.8", "2606:4700:4700::1111"] {
        assert!(is_public(ip.parse().unwrap()));
    }
    for ip in [
        "0.0.0.0",
        "100.100.0.1",
        "192.168.1.1",
        "224.0.0.1",
        "fd00::1",
        "fe80::1",
        "::",
        "::ffff:8.8.8.8",
        "2002:7f00:1::",
    ] {
        assert!(!is_public(ip.parse().unwrap()), "{ip}");
    }
}
fn vuln_redirect(next: &str) -> Option<&str> {
    next.starts_with('/').then_some(next)
}
fn fixed_redirect(next: &str) -> Option<&str> {
    let local = next.starts_with('/')
        && !next.starts_with("//")
        && !next.contains('\\')
        && !next.chars().any(char::is_control);
    local.then_some(next)
}
/// A simplified model of how browsers read a Location value: tabs/newlines are removed and `\` acts like `/`.
fn browser_leaves_site(location: &str) -> bool {
    let s: String = location
        .chars()
        .filter(|c| !matches!(c, '\t' | '\r' | '\n'))
        .map(|c| if c == '\\' { '/' } else { c })
        .collect();
    s.starts_with("//") || s.contains("://") && !s.starts_with('/')
}
fn open_redirect() {
    for attack in [
        "//evil.example/x",
        "/\\evil.example",
        "/\t/evil.example",
        "/\n/evil.example",
    ] {
        assert!(
            vuln_redirect(attack).is_some_and(browser_leaves_site),
            "attack leaves the site: {attack:?}"
        );
        assert_eq!(fixed_redirect(attack), None);
    }
    for attack in ["https://evil.example", "javascript:alert(1)", ""] {
        assert_eq!(fixed_redirect(attack), None);
    }
    assert_eq!(fixed_redirect("/account?tab=2"), Some("/account?tab=2"));
    assert!(!browser_leaves_site("/account"));
}

// ===========================================================================
// 8. ReDoS
// ===========================================================================
#[derive(Debug, Clone)]
enum Re {
    Char(u8),
    Seq(Box<Re>, Box<Re>),
    Alt(Box<Re>, Box<Re>),
    Star(Box<Re>),
}
fn seq(a: Re, b: Re) -> Re {
    Re::Seq(Box::new(a), Box::new(b))
}
fn star(a: Re) -> Re {
    Re::Star(Box::new(a))
}
fn alt(a: Re, b: Re) -> Re {
    Re::Alt(Box::new(a), Box::new(b))
}
/// Backtracking full match with a step budget; `None` = gave up.
fn backtrack(
    re: &Re,
    s: &[u8],
    at: usize,
    steps: &mut u64,
    rest: &dyn Fn(usize, &mut u64) -> Option<bool>,
) -> Option<bool> {
    *steps += 1;
    if *steps > 3_000_000 {
        return None;
    }
    match re {
        Re::Char(c) => {
            if s.get(at) == Some(c) {
                rest(at + 1, steps)
            } else {
                Some(false)
            }
        }
        Re::Seq(a, b) => backtrack(a, s, at, steps, &|next, st| backtrack(b, s, next, st, rest)),
        Re::Alt(a, b) => {
            if backtrack(a, s, at, steps, rest)? {
                Some(true)
            } else {
                backtrack(b, s, at, steps, rest)
            }
        }
        Re::Star(a) => {
            let greedy = backtrack(a, s, at, steps, &|next, st| {
                if next > at {
                    backtrack(re, s, next, st, rest)
                } else {
                    Some(false)
                }
            })?;
            if greedy { Some(true) } else { rest(at, steps) }
        }
    }
}
fn backtrack_match(re: &Re, s: &[u8]) -> (Option<bool>, u64) {
    let mut steps = 0;
    let r = backtrack(re, s, 0, &mut steps, &|end, _| Some(end == s.len()));
    (r, steps)
}
/// Thompson construction: states are `(Some(byte), next)` or epsilon splits.
enum Op {
    Byte(u8, usize),
    Split(usize, usize),
    Accept,
}
fn compile(re: &Re, next: usize, ops: &mut Vec<Op>) -> usize {
    match re {
        Re::Char(c) => ops.push(Op::Byte(*c, next)),
        Re::Seq(a, b) => {
            let second = compile(b, next, ops);
            return compile(a, second, ops);
        }
        Re::Alt(a, b) => {
            let (x, y) = (compile(a, next, ops), compile(b, next, ops));
            ops.push(Op::Split(x, y));
        }
        Re::Star(a) => {
            let split = ops.len();
            ops.push(Op::Split(0, next)); // patched below
            let body = compile(a, split, ops);
            ops[split] = Op::Split(body, next);
            return split;
        }
    }
    ops.len() - 1
}
fn nfa_match(re: &Re, s: &[u8]) -> (bool, u64) {
    let mut ops = vec![Op::Accept];
    let start = compile(re, 0, &mut ops);
    let mut steps = 0;
    let closure = |from: Vec<usize>, steps: &mut u64| {
        let (mut stack, mut seen) = (from, BTreeSet::new());
        while let Some(i) = stack.pop() {
            if seen.insert(i) {
                *steps += 1;
                if let Op::Split(a, b) = ops[i] {
                    stack.extend([a, b]);
                }
            }
        }
        seen
    };
    let mut current = closure(vec![start], &mut steps);
    for &c in s {
        let moved = current
            .iter()
            .filter_map(|&i| match ops[i] {
                Op::Byte(b, n) if b == c => Some(n),
                _ => None,
            })
            .collect();
        current = closure(moved, &mut steps);
    }
    (current.contains(&0), steps)
}
fn random_re(r: &mut Rng, depth: u32) -> Re {
    match r.below(if depth >= 3 { 1 } else { 4 }) {
        0 => Re::Char(b'a' + r.below(2) as u8),
        1 => seq(random_re(r, depth + 1), random_re(r, depth + 1)),
        2 => alt(random_re(r, depth + 1), random_re(r, depth + 1)),
        _ => star(random_re(r, depth + 1)),
    }
}
fn redos() {
    let mut r = Rng(8);
    let mut compared = 0;
    for _ in 0..4000 {
        let re = random_re(&mut r, 0);
        let s: Vec<u8> = (0..r.below(8)).map(|_| b'a' + r.below(2) as u8).collect();
        if let (Some(b), _) = backtrack_match(&re, &s) {
            assert_eq!(b, nfa_match(&re, &s).0, "{re:?} on {s:?}");
            compared += 1;
        }
    }
    assert!(compared > 3500, "{compared}");
    let evil = seq(star(star(Re::Char(b'a'))), Re::Char(b'!')); // (a*)*!
    let attack = |n: usize| vec![b'a'; n];
    let (b10, b14, b30) = (
        backtrack_match(&evil, &attack(10)).1,
        backtrack_match(&evil, &attack(14)).1,
        backtrack_match(&evil, &attack(30)).0,
    );
    assert!(
        b14 > 12 * b10,
        "attack: four more bytes multiply the work: {b10} -> {b14}"
    );
    assert_eq!(
        b30, None,
        "attack: 30 bytes exhaust a 3 million step budget"
    );
    let (n1, n2) = (
        nfa_match(&evil, &attack(1000)).1,
        nfa_match(&evil, &attack(2000)).1,
    );
    assert!(n2 <= n1 * 2 + 10, "the automaton is linear: {n1} -> {n2}");
    assert!(!nfa_match(&evil, &attack(2000)).0);
    println!(
        "      backtracking steps: n=10 {b10}, n=14 {b14}, n=30 gave up at 3000000; NFA n=1000 {n1}, n=2000 {n2}"
    );
}

// ===========================================================================
// 9. sizes, 10. untrusted-input limits, 11. stale buffers
// ===========================================================================
const MAX_BYTES: u32 = 4096;
/// `wrapping_mul` is what `*` does in a release build without overflow checks.
fn vuln_table_bytes(count: u32, item: u32) -> Option<u32> {
    let total = count.wrapping_mul(item);
    (total <= MAX_BYTES).then_some(total)
}
fn fixed_table_bytes(count: u32, item: u32) -> Option<u32> {
    count.checked_mul(item).filter(|&t| t <= MAX_BYTES)
}
fn vuln_accept_length(len: usize) -> bool {
    (len as u16) <= 1024 // truncates BEFORE comparing
}
fn fixed_accept_length(len: usize) -> bool {
    u16::try_from(len).is_ok_and(|l| l <= 1024)
}
/// Returns the bytes it WOULD reserve (we do not really allocate gigabytes in a test).
fn vuln_reservation(input: &[u8]) -> Option<u64> {
    Some(u64::from(u32::from_be_bytes(
        input.get(..4)?.try_into().ok()?,
    )))
}
fn fixed_reservation(input: &[u8]) -> Option<u64> {
    let claimed = vuln_reservation(input)?;
    (claimed <= (input.len() - 4) as u64).then_some(claimed) // cannot claim more than actually arrived
}
/// Run-length decoding of (count, byte) pairs, with an output cap enforced BEFORE growing.
fn rle_decode(input: &[u8], cap: usize) -> Option<Vec<u8>> {
    if !input.len().is_multiple_of(2) {
        return None; // a count without its byte: `chunks_exact` would drop it silently
    }
    let mut out = vec![];
    for pair in input.chunks_exact(2) {
        if out.len() + usize::from(pair[0]) > cap {
            return None;
        }
        out.resize(out.len() + usize::from(pair[0]), pair[1]);
    }
    Some(out)
}
/// Recursive-descent depth of "[[[...]]]". `limit = None` recurses as deep as the INPUT says.
fn nesting(s: &[u8], at: &mut usize, depth: u32, limit: Option<u32>) -> Option<u32> {
    if limit.is_some_and(|l| depth > l) {
        return None;
    }
    let mut deepest = depth;
    while s.get(*at) == Some(&b'[') {
        *at += 1;
        deepest = deepest.max(nesting(s, at, depth + 1, limit)?);
        if s.get(*at) != Some(&b']') {
            return None;
        }
        *at += 1;
    }
    Some(deepest)
}
/// A server that reuses one buffer. The reply length comes from the CLIENT (the Heartbleed shape).
struct EchoServer {
    buffer: [u8; 128],
}
impl EchoServer {
    fn receive(&mut self, payload: &[u8]) -> usize {
        self.buffer[..payload.len()].copy_from_slice(payload);
        payload.len()
    }
    /// Entirely safe Rust, in bounds, and it still leaks the previous request.
    fn vuln_echo(&mut self, payload: &[u8], claimed: usize) -> Option<&[u8]> {
        self.receive(payload);
        self.buffer.get(..claimed)
    }
    fn fixed_echo(&mut self, payload: &[u8], claimed: usize) -> Option<&[u8]> {
        if payload.len() > self.buffer.len() {
            return None; // `receive` would panic: refuse before copying
        }
        let received = self.receive(payload);
        self.buffer[..received].get(..claimed)
    }
}
fn sizes_and_limits() {
    assert_eq!(
        vuln_table_bytes(0x4000_0001, 4),
        Some(4),
        "attack: 2^30+1 items 'need' 4 bytes"
    );
    assert_eq!(fixed_table_bytes(0x4000_0001, 4), None);
    assert_eq!(fixed_table_bytes(100, 4), Some(400));
    assert!(
        vuln_accept_length(65_536 + 10),
        "attack: 65546 passes a limit of 1024"
    );
    assert!(!fixed_accept_length(65_546) && fixed_accept_length(10));
    let mut r = Rng(9);
    for _ in 0..20_000 {
        let (a, b) = (
            r.next() as u32 >> r.below(32),
            r.next() as u32 >> r.below(32),
        );
        let truth = u64::from(a) * u64::from(b);
        assert_eq!(
            fixed_table_bytes(a, b),
            (truth <= 4096).then_some(truth as u32)
        );
    }
}
fn input_limits() {
    let lie = [0xff, 0xff, 0xff, 0xff, 1];
    assert_eq!(
        vuln_reservation(&lie),
        Some(u64::from(u32::MAX)),
        "attack: 5 bytes demand 4 GiB"
    );
    assert_eq!(fixed_reservation(&lie), None);
    assert_eq!(fixed_reservation(&[0, 0, 0, 2, 7, 7]), Some(2));
    let bomb: Vec<u8> = [255, 0].repeat(10_000);
    assert_eq!(
        rle_decode(&bomb, usize::MAX).unwrap().len(),
        2_550_000,
        "attack: 127x expansion with no limit"
    );
    assert_eq!(rle_decode(&bomb, 64 * 1024), None);
    assert_eq!(rle_decode(&[3, b'a', 2, b'b'], 64).unwrap(), b"aaabb");
    assert_eq!(
        rle_decode(&[3, b'a', 2], 64),
        None,
        "truncated input is refused"
    );
    // 5000 levels is safe to run here; a few hundred thousand would overflow the stack and ABORT (not catchable)
    let deep: Vec<u8> = [vec![b'['; 5000], vec![b']'; 5000]].concat();
    assert_eq!(
        nesting(&deep, &mut 0, 0, None),
        Some(5000),
        "attack: recursion depth chosen by the sender"
    );
    assert_eq!(nesting(&deep, &mut 0, 0, Some(64)), None);
    assert_eq!(nesting(b"[[][]]", &mut 0, 0, Some(64)), Some(2));
}
fn stale_buffer() {
    let mut s = EchoServer { buffer: [0; 128] };
    s.fixed_echo(b"Authorization: Bearer s3cr3t-token", 4)
        .unwrap();
    let leaked = s.vuln_echo(b"hi", 60).unwrap().to_vec();
    assert!(
        leaked.windows(12).any(|w| w == b"s3cr3t-token"),
        "attack: over-read of a reused buffer, no unsafe needed"
    );
    s.fixed_echo(b"Authorization: Bearer s3cr3t-token", 4)
        .unwrap();
    assert_eq!(s.fixed_echo(b"hi", 60), None);
    assert_eq!(s.fixed_echo(b"hi", 2), Some(&b"hi"[..]));
    assert_eq!(
        s.fixed_echo(&[0; 129], 1),
        None,
        "too long: refused, no panic"
    );
    // what bounds checking DOES stop: reading past the allocation is None (or a panic), never neighbouring memory
    assert_eq!(s.vuln_echo(b"hi", 129), None);
}

// ===========================================================================
// 12. timing side channel, 13. predictable tokens
// ===========================================================================
/// Returns (equal, bytes compared): the second value stands for elapsed time.
fn vuln_eq(a: &[u8], b: &[u8]) -> (bool, usize) {
    if a.len() != b.len() {
        return (false, 0);
    }
    match a.iter().zip(b).position(|(x, y)| x != y) {
        Some(i) => (false, i + 1),
        None => (true, a.len()),
    }
}
/// Same work for every input of a given length. The length itself is public.
fn fixed_eq(a: &[u8], b: &[u8]) -> (bool, usize) {
    if a.len() != b.len() {
        return (false, 0);
    }
    let difference = a
        .iter()
        .zip(b)
        .fold(0u8, |acc, (x, y)| std::hint::black_box(acc | (x ^ y)));
    (difference == 0, a.len())
}
/// Recover a secret one byte at a time using only the "time" leaked by the comparison.
fn timing_attack(secret: &[u8], compare: fn(&[u8], &[u8]) -> (bool, usize)) -> (Vec<u8>, u64) {
    let mut guess = vec![0u8; secret.len()];
    let mut queries = 0;
    for i in 0..secret.len() {
        let mut best = (0usize, 0u8);
        for candidate in 0..=255u8 {
            guess[i] = candidate;
            queries += 1;
            let (equal, time) = compare(secret, &guess);
            let score = if equal { usize::MAX } else { time };
            if score > best.0 {
                best = (score, candidate);
            }
        }
        guess[i] = best.1;
    }
    (guess, queries)
}
fn timing() {
    let secret = b"k9-Qz!7mVx2#pL0s";
    let (recovered, queries) = timing_attack(secret, vuln_eq);
    assert_eq!(
        recovered, secret,
        "attack: at most 256 x length guesses instead of 256^length"
    );
    assert!(queries <= 256 * 16);
    assert_ne!(timing_attack(secret, fixed_eq).0, secret);
    let mut r = Rng(12);
    for _ in 0..5000 {
        let n = r.below(6) as usize;
        let (a, b): (Vec<u8>, Vec<u8>) = (
            (0..n).map(|_| r.below(2) as u8).collect(),
            (0..n).map(|_| r.below(2) as u8).collect(),
        );
        assert_eq!(fixed_eq(&a, &b).0, a == b);
        assert_eq!(fixed_eq(&a, &b).1, n, "work does not depend on content");
    }
}
/// A fine simulation RNG and a terrible token generator: seeded with the time, and invertible.
fn vuln_tokens(unix_millis: u64) -> impl FnMut() -> u64 {
    let mut r = Rng(unix_millis);
    move || r.next()
}
fn os_token() -> std::io::Result<[u8; 32]> {
    let mut t = [0u8; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut t)?;
    Ok(t)
}
fn predictable_tokens() {
    let server_started = 1_790_000_123_456u64;
    let mut server = vuln_tokens(server_started);
    let (mine, victims) = (server(), server());
    // the attacker got one token for their own account and knows the start time to within a minute
    let seed = (server_started - 30_000..server_started + 30_000)
        .find(|&s| vuln_tokens(s)() == mine)
        .expect("attack: brute force 60000 seeds");
    let mut clone = vuln_tokens(seed);
    clone();
    assert_eq!(clone(), victims, "attack: the NEXT user's token, predicted");
    let tokens: Vec<[u8; 32]> = (0..1000).map(|_| os_token().unwrap()).collect();
    assert_eq!(tokens.iter().collect::<BTreeSet<_>>().len(), 1000);
    assert!(
        !(server_started - 30_000..server_started + 30_000)
            .any(|s| tokens[0][..8] == vuln_tokens(s)().to_le_bytes())
    );
}

// ===========================================================================
// 14. log injection, 15. mass assignment, 16. CSRF
// ===========================================================================
fn vuln_log(user: &str) -> String {
    format!("INFO login failed user={user}\n")
}
/// Escapes newlines, other control characters and quotes; the value stays on one line inside quotes.
fn fixed_log(user: &str) -> String {
    format!("INFO login failed user=\"{}\"\n", user.escape_default())
}
fn log_injection() {
    let forge = "bob\nINFO login succeeded user=admin";
    assert!(
        vuln_log(forge)
            .lines()
            .any(|l| l == "INFO login succeeded user=admin"),
        "attack: a forged line"
    );
    assert!(
        vuln_log("\x1b[2J").contains('\x1b'),
        "attack: terminal escape sequences"
    );
    let mut r = Rng(14);
    let alphabet = [
        '\n', '\r', '\x1b', '\0', '"', '\\', 'a', ' ', '\u{2028}', '\u{85}',
    ];
    for _ in 0..3000 {
        let s: String = (0..r.below(10))
            .map(|_| alphabet[r.below(alphabet.len() as u64) as usize])
            .collect();
        let line = fixed_log(&s);
        assert!(
            line.ends_with('\n')
                && line[..line.len() - 1]
                    .chars()
                    .all(|c| c.is_ascii() && !c.is_ascii_control()),
            "{line:?}"
        );
    }
}
#[derive(Deserialize, Debug, Clone, PartialEq)]
struct User {
    display_name: String,
    #[serde(default)]
    is_admin: bool,
}
/// The request type lists only what a client may change.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileUpdate {
    display_name: String,
}
fn vuln_update(_current: &User, body: &str) -> Option<User> {
    serde_json::from_str(body).ok()
}
fn fixed_update(current: &User, body: &str) -> Option<User> {
    let u: ProfileUpdate = serde_json::from_str(body).ok()?;
    Some(User {
        display_name: u.display_name,
        ..current.clone()
    })
}
fn mass_assignment() {
    let current = User {
        display_name: "ann".into(),
        is_admin: false,
    };
    let attack = r#"{"display_name":"ann","is_admin":true}"#;
    assert!(
        vuln_update(&current, attack).unwrap().is_admin,
        "attack: set a field the form never offered"
    );
    assert!(fixed_update(&current, attack).is_none());
    assert_eq!(
        fixed_update(&current, r#"{"display_name":"Ann B"}"#).unwrap(),
        User {
            display_name: "Ann B".into(),
            is_admin: false
        }
    );
}
struct Request<'a> {
    cookie_session: Option<&'a str>, // the browser attaches this even on cross-site requests (without SameSite)
    origin: Option<&'a str>,
    csrf_field: Option<&'a str>,
}
struct Bank {
    sessions: HashMap<String, (String, String)>, // session id -> (user, csrf token)
    transfers: Vec<String>,
}
impl Bank {
    const ORIGIN: &'static str = "https://bank.example";
    fn vuln_transfer(&mut self, r: &Request) -> bool {
        let Some((user, _)) = r.cookie_session.and_then(|s| self.sessions.get(s)) else {
            return false;
        };
        self.transfers.push(user.clone());
        true
    }
    /// Two independent checks: the Origin header (browsers set it, pages cannot forge it) and a per-session token that
    /// another site cannot read. SameSite cookies are a third layer.
    fn fixed_transfer(&mut self, r: &Request) -> bool {
        let Some((_, token)) = r.cookie_session.and_then(|s| self.sessions.get(s)) else {
            return false;
        };
        let token_ok = r
            .csrf_field
            .is_some_and(|f| fixed_eq(f.as_bytes(), token.as_bytes()).0);
        r.origin == Some(Self::ORIGIN) && token_ok && self.vuln_transfer(r)
    }
}
fn csrf() {
    let hex = |b: [u8; 32]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    let token = hex(os_token().unwrap());
    let mut bank = Bank {
        sessions: HashMap::from([("sess-1".into(), ("alice".into(), token.clone()))]),
        transfers: vec![],
    };
    let forged = Request {
        cookie_session: Some("sess-1"),
        origin: Some("https://evil.example"),
        csrf_field: None,
    };
    assert!(
        bank.vuln_transfer(&forged),
        "attack: the victim's browser supplies the cookie"
    );
    bank.transfers.clear();
    assert!(!bank.fixed_transfer(&forged));
    let stolen_origin_only = Request {
        origin: Some(Bank::ORIGIN),
        csrf_field: Some("guess"),
        ..forged
    };
    assert!(!bank.fixed_transfer(&stolen_origin_only));
    let right_token_wrong_origin = Request {
        cookie_session: Some("sess-1"),
        origin: Some("https://bank.example.evil.test"),
        csrf_field: Some(&token),
    };
    assert!(!bank.fixed_transfer(&right_token_wrong_origin) && bank.transfers.is_empty());
    let honest = Request {
        cookie_session: Some("sess-1"),
        origin: Some(Bank::ORIGIN),
        csrf_field: Some(&token),
    };
    assert!(bank.fixed_transfer(&honest));
    assert_eq!(bank.transfers, ["alice"]);
}

fn main() {
    let pairs: [(&str, fn()); 16] = [
        ("SQL injection", sql_injection),
        ("path traversal (5 broken variants)", path_traversal),
        ("TOCTOU on files", toctou),
        ("command and argument injection", command_injection),
        ("XSS output contexts", xss),
        ("SSRF and DNS rebinding", ssrf),
        ("open redirect", open_redirect),
        ("ReDoS", redos),
        ("integer overflow and truncation", sizes_and_limits),
        ("allocation, expansion and depth limits", input_limits),
        ("stale data in a reused buffer", stale_buffer),
        ("timing side channel", timing),
        ("predictable tokens", predictable_tokens),
        ("log injection", log_injection),
        ("mass assignment", mass_assignment),
        ("CSRF", csrf),
    ];
    for (name, check) in pairs {
        check();
        println!("  [attack works on vulnerable, inert on fixed] {name}");
    }
    println!("all 16 vulnerable/fixed pairs verified");
}
