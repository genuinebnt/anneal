//! RECIPE 1203 (parsing): Reading HTML safely: pull links and text out of a page you don't trust.
//! LEVEL: intermediate
//! Use in: link collectors, link checkers, previews.   Run: cargo run --release --example reading_html_safely
//! COMMON USES: crawlers, link previews in chat and mail, feed readers, site audits.
#![allow(dead_code)]
//!
//! This is a teaching subset of the tokenizer in the HTML standard. It READS pages. It is not a sanitizer: never use it
//! to decide what a browser may show. It builds no tree, knows five named character references, refuses `<svg>`,
//! `<math>` and `<!--` inside a script, and treats `<noscript>` as ordinary markup (a browser with scripts on does not).
//! In a browser, tree construction decides whether `<title>`, `<textarea>`, `<iframe>`, `<plaintext>`, `<style>`,
//! `<script>`, `<xmp>`, `<noembed>` or `<noframes>` turns what follows into text. This reader always does, so a page can
//! be written to HIDE REAL LINKS from it: "no link found" proves nothing. For real pages use a full parser (html5ever).
//! Read: https://html.spec.whatwg.org/multipage/parsing.html#tokenization

#[derive(Debug, Clone, Copy)]
struct Limits {
    bytes: usize,
    tokens: usize,
    /// Attributes in one tag, repeats included.
    attributes: usize,
}

const LIMITS: Limits = Limits { bytes: 2 << 20, tokens: 200_000, attributes: 100 };

#[derive(Debug, Clone, PartialEq)]
struct Attr {
    name: String,
    /// `None`: the value holds a reference this reader can't decode, so it doesn't know what a browser would see.
    value: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    /// Text between tags (or the title, or the text in a text box). `sure` is false when a reference was left as
    /// written, such as `&nbsp;`: a browser may show something else.
    Text { text: String, sure: bool },
    /// What stands inside `<script>`, `<style>` and the like: not markup, not visible text.
    Hidden(String),
    Start { name: String, attrs: Vec<Attr> },
    End(String),
    Comment,
}

#[derive(Debug, Clone, PartialEq)]
enum Refused {
    TooLarge,
    TooManyTokens,
    TooManyAttributes,
    Unsupported(&'static str),
}

/// HTML's own white space. `char::is_whitespace` accepts far more.
fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0C)
}

/// The first position from `from` where `stop` is true, or the end.
fn scan(b: &[u8], from: usize, stop: impl Fn(u8) -> bool) -> usize {
    (from..b.len()).find(|&i| stop(b[i])).unwrap_or(b.len())
}

/// Decodes `&…;` references. The second value is false when a reference was left as written
/// although a browser might decode it.
fn decode(raw: &str, in_attribute: bool) -> (String, bool) {
    let mut out = String::new();
    let mut sure = true;
    let mut rest = raw;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at + 1..];
        let b = rest.as_bytes();
        if b.first() == Some(&b'#') {
            let hex = matches!(b.get(1), Some(b'x' | b'X'));
            let from = if hex { 2 } else { 1 };
            let radix = if hex { 16 } else { 10 };
            let end = scan(b, from, |c| !(c as char).is_digit(radix));
            if end == from {
                out.push('&');
                continue;
            }
            // saturate: `&#99999999999999999999;` must not wrap round to a small number
            let n = rest[from..end]
                .chars()
                .filter_map(|c| c.to_digit(radix))
                .fold(0u32, |n, d| n.saturating_mul(radix).saturating_add(d));
            let semicolon = usize::from(b.get(end) == Some(&b';'));
            if (0x80..=0x9F).contains(&n) {
                // a browser maps these through an old Windows table that is not here
                sure = false;
                out.push('&');
                continue;
            }
            out.push(char::from_u32(n).filter(|&c| c != '\0').unwrap_or('\u{FFFD}'));
            rest = &rest[end + semicolon..];
            continue;
        }
        let end = scan(b, 0, |c| !c.is_ascii_alphanumeric());
        let name = &rest[..end];
        let semicolon = b.get(end) == Some(&b';');
        // in an attribute, `&page=2` is never a reference: this keeps ordinary query strings intact
        if name.is_empty() || (in_attribute && b.get(end) == Some(&b'=')) {
            out.push('&');
            continue;
        }
        let known = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" if semicolon => Some('\''),
            _ => None,
        };
        match known {
            Some(c) => {
                out.push(c);
                rest = &rest[end + usize::from(semicolon)..];
            }
            None => {
                sure = false;
                out.push('&');
            }
        }
    }
    out.push_str(rest);
    (out, sure)
}

struct Tag {
    name: String,
    attrs: Vec<Attr>,
    /// Just past the `>`.
    end: usize,
}

/// Reads a tag whose name starts at `at`. `Ok(None)` means the input ended inside it: a browser drops such a tag.
fn tag(s: &str, at: usize, limits: Limits) -> Result<Option<Tag>, Refused> {
    let b = s.as_bytes();
    let mut i = scan(b, at, |c| is_space(c) || c == b'/' || c == b'>');
    let name = s[at..i].to_ascii_lowercase();
    let mut attrs: Vec<Attr> = Vec::new();
    let mut seen = 0;
    loop {
        i = scan(b, i, |c| !(is_space(c) || c == b'/'));
        match b.get(i) {
            None => return Ok(None),
            Some(b'>') => return Ok(Some(Tag { name, attrs, end: i + 1 })),
            Some(_) => {}
        }
        // checked before the work: looking for repeats below is quadratic without a limit
        seen += 1;
        if seen > limits.attributes {
            return Err(Refused::TooManyAttributes);
        }
        // the first character belongs to the name even when it is `=`
        let name_end = scan(b, i + 1, |c| is_space(c) || matches!(c, b'/' | b'>' | b'='));
        let key = s[i..name_end].to_ascii_lowercase();
        i = scan(b, name_end, |c| !is_space(c));
        let mut raw = "";
        if b.get(i) == Some(&b'=') {
            i = scan(b, i + 1, |c| !is_space(c));
            match b.get(i) {
                Some(&quote @ (b'"' | b'\'')) => {
                    let close = scan(b, i + 1, |c| c == quote);
                    if close == b.len() {
                        return Ok(None);
                    }
                    raw = &s[i + 1..close];
                    i = close + 1;
                }
                _ => {
                    let value_end = scan(b, i, |c| is_space(c) || c == b'>');
                    raw = &s[i..value_end];
                    i = value_end;
                }
            }
        }
        // when a name is repeated, the first one counts
        if !attrs.iter().any(|a| a.name == key) {
            let (value, sure) = decode(raw, true);
            attrs.push(Attr { name: key, value: sure.then_some(value) });
        }
    }
}

/// Where the content of a raw text element ends: at `</name` followed by a space, `/` or `>`.
fn closing(b: &[u8], from: usize, name: &str) -> usize {
    let n = name.len();
    (from..b.len())
        .find(|&i| {
            b[i..].starts_with(b"</")
                && b.get(i + 2..i + 2 + n).is_some_and(|w| w.eq_ignore_ascii_case(name.as_bytes()))
                && b.get(i + 2 + n).is_some_and(|&c| is_space(c) || c == b'/' || c == b'>')
        })
        .unwrap_or(b.len())
}

/// Where the comment that starts with `<!--` at `at` ends.
fn comment_end(s: &str, at: usize) -> usize {
    let body = &s[at + 4..];
    // `<!-->` and `<!--->` are whole comments
    for short in [">", "->"] {
        if body.starts_with(short) {
            return at + 4 + short.len();
        }
    }
    let b = body.as_bytes();
    (0..b.len())
        .find_map(|i| {
            [&b"-->"[..], b"--!>"].iter().find(|close| b[i..].starts_with(close)).map(|close| at + 4 + i + close.len())
        })
        .unwrap_or(s.len())
}

fn push(out: &mut Vec<Token>, token: Token, limits: Limits) -> Result<(), Refused> {
    if out.len() >= limits.tokens {
        return Err(Refused::TooManyTokens);
    }
    out.push(token);
    Ok(())
}

fn text(raw: &str) -> Token {
    let (text, sure) = decode(raw, false);
    Token::Text { text, sure }
}

fn tokenize(html: &str, limits: Limits) -> Result<Vec<Token>, Refused> {
    if html.len() > limits.bytes {
        return Err(Refused::TooLarge);
    }
    // one kind of line break, as the standard does first; zero bytes are replaced here too
    let s = html.replace("\r\n", "\n").replace('\r', "\n").replace('\0', "\u{FFFD}");
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut text_from = 0;
    let mut i = 0;
    // every delimiter is ASCII, so every slice below starts and ends between characters
    while i < b.len() {
        let lt = scan(b, i, |c| c == b'<');
        if lt == b.len() {
            break;
        }
        let first = b.get(lt + 1).copied().unwrap_or(0);
        let third = b.get(lt + 2).copied();
        // what this `<` opens: (the token, where it ends)
        let (token, end) = if s[lt..].starts_with("<!--") {
            (Some(Token::Comment), comment_end(&s, lt))
        } else if first == b'!' || first == b'?' {
            (Some(Token::Comment), (scan(b, lt + 2, |c| c == b'>') + 1).min(b.len()))
        } else if first == b'/' && third.is_some_and(|c| c.is_ascii_alphabetic()) {
            match tag(&s, lt + 2, limits)? {
                Some(t) => (Some(Token::End(t.name)), t.end),
                None => (None, b.len()),
            }
        } else if first == b'/' && third == Some(b'>') {
            (None, lt + 3)
        } else if first == b'/' && third.is_some() {
            (Some(Token::Comment), (scan(b, lt + 2, |c| c == b'>') + 1).min(b.len()))
        } else if first.is_ascii_alphabetic() {
            match tag(&s, lt + 1, limits)? {
                Some(t) => (Some(Token::Start { name: t.name, attrs: t.attrs }), t.end),
                None => (None, b.len()),
            }
        } else {
            // a `<` that opens nothing is text
            i = lt + 1;
            continue;
        };
        if text_from < lt {
            push(&mut out, text(&s[text_from..lt]), limits)?;
        }
        i = end;
        text_from = end;
        let Some(token) = token else { continue };
        let element = match &token {
            Token::Start { name, .. } => name.clone(),
            _ => String::new(),
        };
        push(&mut out, token, limits)?;
        // inside these elements `<` is not markup
        let (stop, decoded) = match element.as_str() {
            "svg" | "math" => return Err(Refused::Unsupported("svg and math follow other rules")),
            "script" | "style" | "xmp" | "iframe" | "noembed" | "noframes" => (closing(b, end, &element), false),
            "textarea" | "title" => (closing(b, end, &element), true),
            "plaintext" => (b.len(), false),
            _ => continue,
        };
        let content = &s[end..stop];
        if element == "script" && content.contains("<!--") {
            return Err(Refused::Unsupported("<!-- inside a script changes where it ends"));
        }
        if !content.is_empty() {
            let token = if decoded { text(content) } else { Token::Hidden(content.to_owned()) };
            push(&mut out, token, limits)?;
        }
        i = stop;
        text_from = stop;
    }
    if text_from < b.len() {
        push(&mut out, text(&s[text_from..]), limits)?;
    }
    Ok(out)
}

fn attribute<'a>(attrs: &'a [Attr], name: &str) -> Option<&'a Attr> {
    attrs.iter().find(|a| a.name == name)
}

/// The `href` of every `<a>` and `<area>`, as the page wrote it, decoded.
fn links(tokens: &[Token]) -> Vec<Option<String>> {
    tokens
        .iter()
        .filter_map(|t| match t {
            Token::Start { name, attrs } if name == "a" || name == "area" => {
                attribute(attrs, "href").map(|a| a.value.clone())
            }
            _ => None,
        })
        .collect()
}

#[derive(Debug, PartialEq)]
enum Target {
    /// Starts with `http:` or `https:`.
    Web(String),
    /// No scheme: resolve it against the page's address with a URL library, then check the result.
    Relative(String),
    /// Another scheme, or a value that could not be decoded.
    Rejected,
}

/// An allow-list, applied after cleaning the text the way a browser's URL parser does.
fn target(value: Option<&str>) -> Target {
    let Some(value) = value else { return Target::Rejected };
    let url: String = value
        .trim_matches(|c: char| c <= ' ')
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    let scheme = url.split_once(':').map(|(s, _)| s).filter(|s| {
        s.starts_with(|c: char| c.is_ascii_alphabetic())
            && s.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    });
    match scheme.map(str::to_ascii_lowercase).as_deref() {
        None => Target::Relative(url),
        Some("http" | "https") => Target::Web(url),
        Some(_) => Target::Rejected,
    }
}

/// Escapes text for exactly TWO places in HTML you write: the text between the tags of an ordinary element (`p`, `li`,
/// `td`), and a QUOTED value of `title`, `alt` or `data-*` that no script later runs, reads as HTML or uses as a
/// template. It is NOT safe anywhere else. That includes `on…` attributes and `srcdoc` (the browser undoes the
/// escaping, then runs the result), `style`, attributes that hold an address (`href`, `src`, `action`), the inside of
/// `<script>` and `<style>`, unquoted values, tag and attribute names, and comments.
fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

struct Rng(u64);
impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
    fn pick<'a>(&mut self, from: &[&'a str]) -> &'a str {
        from[self.below(from.len())]
    }
}

fn values(html: &str) -> Vec<String> {
    links(&tokenize(html, LIMITS).unwrap()).into_iter().flatten().collect()
}

fn fixed_cases() {
    // each line breaks a shortcut: [page, the links a browser finds]
    let cases: [(&str, &[&str]); 16] = [
        (r#"<A HREF = '/one'>x</a>"#, &["/one"]),
        (r#"<a title="a > b" href=/two>x</a>"#, &["/two"]),
        (r#"<!-- <a href="/no"> --><a href="/yes">"#, &["/yes"]),
        (r#"<!--> <a href="/after-empty-comment"> -->"#, &["/after-empty-comment"]),
        (r#"<!---><a href="/also-after"> -->"#, &["/also-after"]),
        (r#"<!-- x --!><a href="/bang">"#, &["/bang"]),
        (r#"<script>t = '</scripture> <a href="/no">';</script><a href="/yes">"#, &["/yes"]),
        (r#"<SCRIPT/><a href="/no"></ScRiPt ><a href="/yes">"#, &["/yes"]),
        (r#"<textarea><a href="/no"></textarea><title></a href=x><a href=/no></title>"#, &[]),
        (r#"<a href="/first" href="/second">"#, &["/first"]),
        (r#"<a href="/q?a=1&b=2&amp;c=3">"#, &["/q?a=1&b=2&c=3"]),
        (r#"<a href="/cut-off"#, &[]),
        (r#"<a href=/cut-off "#, &[]),
        ("<a\nhref\n=\n\"/lines\">", &["/lines"]),
        (r#"</a href="/no"><a data-href="/no" href=/yes/>"#, &["/yes/"]),
        (r#"1 < 2 <3 </ > <? <a href="/no"> ?> </><a href="/yes">"#, &["/yes"]),
    ];
    for (html, expected) in cases {
        assert_eq!(values(html), expected, "{html}");
    }
    // `&colon;` is a real reference this reader doesn't know: it says so instead of guessing
    assert_eq!(links(&tokenize(r#"<a href="javascript&colon;go()">"#, LIMITS).unwrap()), vec![None]);
    assert_eq!(decode("&#x6A;&#97;va&#99999999999999999999;&#0;", true), ("java\u{FFFD}\u{FFFD}".into(), true));
    // 2^32 + 106: wrapping arithmetic would turn this into `j`
    assert_eq!(decode("&#4294967402;", true), ("\u{FFFD}".into(), true));
    assert!(!decode("&#128;", false).1);
    assert!(!decode("a&copy b", true).1);
    assert!(decode("a & b &; &#; &#x;", false) == ("a & b &; &#; &#x;".into(), true));
    assert_eq!(texts("<p>2 &lt; 3<style>p{}</style><!--no--> ok</p>"), [("2 < 3".into(), true), (" ok".into(), true)]);
    // a reference this reader can't decode stays as written, and the token says so
    for page in ["<p>a&nbsp;b</p>", "<title>a&nbsp;b</title>", "<textarea>a&nbsp;b</textarea>"] {
        assert_eq!(texts(page), [("a&nbsp;b".into(), false)], "{page}");
    }
    // `&apos;` needs its semicolon: without one it is not a reference this reader will decode
    assert_eq!(decode("&apos;", false), ("'".into(), true));
    assert_eq!(decode("&apos", false), ("&apos".into(), false));
    // CRLF and a lone CR both become one line feed, in values and between the parts of a tag
    assert_eq!(values("<a href=\"/a\r\nb\">"), ["/a\nb"]);
    assert_eq!(values("<a\rhref=\"/a\rb\">"), ["/a\nb"]);
}

fn texts(html: &str) -> Vec<(String, bool)> {
    tokenize(html, LIMITS)
        .unwrap()
        .into_iter()
        .filter_map(|t| if let Token::Text { text, sure } = t { Some((text, sure)) } else { None })
        .collect()
}

/// A KNOWN FAILURE, kept in view. A browser finds `/seen` on each of these pages: its tree construction ignores the
/// tag that would turn the rest into text. This tokenizer has no tree, so it misses the link.
fn hidden_links() {
    for tag in ["title", "textarea", "iframe", "plaintext", "style", "script", "xmp", "noembed", "noframes"] {
        let page = format!("<template><col><{tag}></template><a href=/seen>");
        assert_eq!(values(&page), Vec::<String>::new(), "{page}");
    }
}

fn url_cases() {
    for bad in ["javascript:go()", " JaVaScRiPt:go()", "java\tscri\npt:go()", "\u{1}javascript:go()", "data:text/html,x", "file:///etc/hosts"] {
        assert_eq!(target(Some(bad)), Target::Rejected, "{bad:?}");
    }
    let from_page = values(r#"<a href="&#106;ava&#x09;script:go()">"#);
    assert_eq!(target(Some(&from_page[0])), Target::Rejected);
    assert!(matches!(target(Some("HTTPS://example.com/")), Target::Web(_)));
    // no scheme, yet it leaves this site: this is why the resolved address needs its own check
    assert!(matches!(target(Some("//other.example/x")), Target::Relative(_)));
    assert!(matches!(target(Some("a/b:c")), Target::Relative(_)));
    assert_eq!(target(None), Target::Rejected);
    assert_eq!(escape_text(r#"<b a='1' c="2">&"#), "&lt;b a=&#39;1&#39; c=&quot;2&quot;&gt;&amp;");
}

/// What escaping does, and what it can't do.
fn escape_cases() {
    // it changes nothing in a script address, so it can't protect `href`
    assert_eq!(escape_text("javascript:go()"), "javascript:go()");
    // an `onclick` or `srcdoc` value is decoded first: the script or the inner page gets the original characters back
    for payload in ["');go('", "<img src=x onerror=go()>"] {
        assert_eq!(decode(&escape_text(payload), true), (payload.into(), true));
    }
    let mut rng = Rng(0x00e5_ca9e_0000_0003);
    for _ in 0..5_000 {
        let s: String = (0..rng.below(20)).map(|_| rng.pick(&["<", ">", "\"", "'", "&", "amp;", "#", ";", "=", "a", " ", "é"])).collect();
        let safe = escape_text(&s);
        assert!(!safe.contains(['<', '>', '"', '\'']), "{s:?}");
        assert_eq!(decode(&safe, false), (s.clone(), true));
        assert_eq!(decode(&safe, true), (s.clone(), true));
    }
}

fn limit_cases() {
    let small = Limits { bytes: 100, tokens: 5, attributes: 3 };
    assert_eq!(tokenize(&"x".repeat(101), small), Err(Refused::TooLarge));
    assert!(tokenize(&"x".repeat(100), small).is_ok());
    assert_eq!(tokenize("<i><i><i><i><i><i>", small), Err(Refused::TooManyTokens));
    assert!(tokenize("<i><i><i><i><i>", small).is_ok());
    assert_eq!(tokenize("<i a a a a>", small), Err(Refused::TooManyAttributes));
    assert!(tokenize("<i a b c>", small).is_ok());
    for unsupported in ["<svg>", "<MATH>", "<script><!--</script>"] {
        assert!(matches!(tokenize(unsupported, LIMITS), Err(Refused::Unsupported(_))), "{unsupported}");
    }
}

/// Builds pages from parts whose links are known, so the generator is the second opinion.
fn generated_pages() {
    let mut rng = Rng(0x5eed_1234_9876_0001);
    for round in 0..4_000 {
        let mut html = String::new();
        let mut expected = Vec::new();
        for part in 0..rng.below(12) {
            let url = format!("/p{round}-{part}");
            let fake = format!("<a href=\"/fake{part}\">");
            let gap = rng.pick(&[" ", "\n", "\t", "  ", " / "]);
            let name = rng.pick(&["a", "A"]);
            let key = rng.pick(&["href", "HREF", "Href"]);
            let quote = rng.pick(&["\"", "'", ""]);
            let extra = rng.pick(&["", " x", " title='>'", " t=\"<a href=/fake>\""]);
            match rng.below(6) {
                0 => {
                    html += &format!("<{name}{extra}{gap}{key}={quote}{url}{quote}{gap}>t</a>");
                    expected.push(url);
                }
                1 => html += &format!("<!-- {fake} -->"),
                2 => {
                    let raw = rng.pick(&["script", "style", "textarea", "title", "iframe", "xmp"]);
                    html += &format!("<{raw}></{raw}s>{fake}</{raw}{gap}>");
                }
                3 => html += &format!("<p data-x={quote}y{quote}>"),
                4 => html += rng.pick(&["a < b", "x > y", "& co", "&lt;a href=/fake&gt;", "é🔥", "\r\n"]),
                _ => html += &format!("<!x {url}><?y?></ z>"),
            }
        }
        assert_eq!(values(&html), expected, "{html:?}");
    }
}

/// Nothing here may panic, whatever the bytes are.
fn hostile_pages() {
    let parts = [
        "<", ">", "/", "!", "-", "=", "\"", "'", "&", "#", ";", "x", "a", " ", "\n", "\r", "\0", "é", "🔥", "<!--",
        "-->", "--!>", "<script", "</script", "<title", "</title>", "<plaintext>", "&#x", "99999999999", "<a href=",
    ];
    let mut rng = Rng(0x0bad_cafe_0000_0007);
    for _ in 0..20_000 {
        let s: String = (0..rng.below(30)).map(|_| rng.pick(&parts)).collect();
        if let Ok(tokens) = tokenize(&s, LIMITS) {
            assert!(tokens.len() <= s.len() + 1, "{s:?}");
        }
    }
    for _ in 0..20_000 {
        // bytes that need not be UTF-8: decide how to decode them before the HTML reader sees them
        let alphabet = b"<>=/!-&\"' a\xff\xc3\xa9\0;#";
        let bytes: Vec<u8> = (0..rng.below(40)).map(|_| alphabet[rng.below(alphabet.len())]).collect();
        let _ = tokenize(&String::from_utf8_lossy(&bytes), LIMITS);
    }
    let page = "<!DOCTYPE html><a title='é>🔥' href=\"/x&amp;\">&#x1F525;</a><script>1</script><!-- c -->";
    for (cut, _) in page.char_indices() {
        let _ = tokenize(&page[..cut], LIMITS);
    }
    // large unfinished constructs: each must be read once, not once per `<`
    for piece in ["<!--", "<a ", "<a x='", "<script>", "</", "&", "<!", "<"] {
        let s = piece.repeat(LIMITS.bytes / piece.len());
        let _ = tokenize(&s, LIMITS);
    }
}

fn main() {
    fixed_cases();
    url_cases();
    escape_cases();
    hidden_links();
    limit_cases();
    generated_pages();
    hostile_pages();

    let page = r#"<!-- <a href="/old"> -->
<script>var s = '<a href="/in-a-string">';</script>
<a href="/docs">Docs</a>
<A HREF='&#106;avascript:go()'>Click</A>
<a title="1 > 0" href=https://example.com/?a=1&b=2>Out</a>"#;
    for link in links(&tokenize(page, LIMITS).expect("a small page")) {
        println!("{:?}", target(link.as_deref()));
    }
    println!("all checks passed");
}
