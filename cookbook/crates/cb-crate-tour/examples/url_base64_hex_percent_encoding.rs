//! RECIPE 1110 (crates): url, percent-encoding, base64, hex and data-encoding
//! LEVEL: intermediate
//! Use in: building and validating URLs, signing requests, tokens in headers and links, binary data in text formats.   Run: cargo run --release -p cb-crate-tour --example url_base64_hex_percent_encoding
//! COMMON USES: HTTP clients (reqwest uses the url crate), OAuth redirect handling, JWT segments (base64url without padding),
//! Basic auth, content digests (hex), TOTP secrets (base32).
//! METHODS: Url::parse, Url::parse_with_params, Url::join, Url::scheme, Url::host, Url::host_str, Url::port, Url::port_or_known_default, Url::path, Url::path_segments, Url::path_segments_mut, Url::query, Url::query_pairs, Url::query_pairs_mut, Url::set_query, Url::set_fragment, Url::set_path, Url::username, Url::password, Url::origin, Url::make_relative, url::Host, url::form_urlencoded::Serializer, percent_encoding::utf8_percent_encode, percent_encoding::percent_decode_str, percent_encoding::AsciiSet, percent_encoding::NON_ALPHANUMERIC, percent_encoding::CONTROLS, base64::Engine::encode, base64::Engine::decode, base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD}, base64::DecodeError, hex::encode, hex::encode_upper, hex::decode, hex::decode_to_slice, hex::FromHexError, data_encoding::BASE32, data_encoding::BASE32_NOPAD, data_encoding::HEXLOWER_PERMISSIVE, Url::{set_scheme, set_host, set_port, set_username, set_password, domain, from_file_path, to_file_path, from_directory_path, options}, url::Position, url::Host::parse, ParseOptions::syntax_violation_callback, form_urlencoded::parse, base64::engine::{GeneralPurpose, GeneralPurposeConfig, DecodePaddingMode}, base64::alphabet, Engine::{encode_slice, decode_slice, decode_vec}, base64::encoded_len, base64::write::EncoderStringWriter, base64::read::DecoderReader, data_encoding::{BASE64_MIME, Specification, Encoding::{encode_len, encode_display}}
//! DOCS: https://docs.rs/url/2/url/ https://docs.rs/percent-encoding/2/ https://docs.rs/base64/0.22/ https://docs.rs/hex/0.4/ https://docs.rs/data-encoding/2/
//! CRATES: url = "2", percent-encoding = "2", base64 = "0.22", hex = "0.4", data-encoding = "2"
//!
//! CHEAT SHEET
//!   resolve a link found on a page        -> base.join(href)   (mind the trailing slash!)
//!   add query parameters                  -> query_pairs_mut().append_pair      (never format!)
//!   add a user value as ONE path segment  -> path_segments_mut().push           (never join(format!(..)))
//!   check where a URL really points       -> url.host(), compare exactly         (never starts_with/contains on the string)
//!   bytes in a URL or file name           -> URL_SAFE_NO_PAD
//!   bytes in MIME/JSON/Basic auth         -> STANDARD
//!   log a URL                             -> strip userinfo (set_password(None)) and often the query
//!   cache key without query/fragment      -> &url[..Position::AfterPath]
//!   local path <-> file URL               -> from_file_path / to_file_path   (never format!("file://{path}"))
//!   accept padded AND unpadded base64     -> a GeneralPurpose engine with DecodePaddingMode::Indifferent
//!   large or incremental base64           -> base64::write / base64::read adapters
//!   line-wrapped MIME, custom alphabets   -> data_encoding (BASE64_MIME, Specification)
//!   digests for humans                    -> hex
//!
//! PITFALLS: base64 and hex are ENCODINGS, not encryption. Query strings use form encoding (space <-> '+'), paths do not.
//! INTERNATIONAL DOMAINS: the parser converts them to punycode ("xn--..."); look-alike characters then stand out in the
//! ASCII form, but IDNA is not a defence against homograph phishing by itself.
//!
//! Read: https://url.spec.whatwg.org/ ; https://www.rfc-editor.org/rfc/rfc3986.html ; https://www.rfc-editor.org/rfc/rfc4648.html
//!
//! TRY NEXT:
//!   - [S] Predict base.join("b") for ".../a" and ".../a/" before reading the table.
//!   - [M] Write an open-redirect check `is_safe_redirect(base, target)` and attack it with //evil.example and \\evil.example.
//!   - [A] PROJECT: implement base64 yourself and test it against the crate with the random inputs below.
#![allow(dead_code)]
use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use data_encoding::{BASE32, BASE32_NOPAD, HEXLOWER_PERMISSIVE};
use percent_encoding::{
    AsciiSet, CONTROLS, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode,
};
use url::{Host, Url, form_urlencoded};

fn url_parts_and_normalisation() {
    let u = Url::parse("HTTPS://user:p%40ss@Example.COM:443/a/./b/../c%20d?q=1&q=2#frag").unwrap();
    assert_eq!(u.scheme(), "https");
    assert_eq!(
        u.host_str(),
        Some("example.com"),
        "scheme and host are lower-cased"
    );
    assert_eq!(
        (u.port(), u.port_or_known_default()),
        (None, Some(443)),
        "default port is dropped"
    );
    assert_eq!(
        u.path(),
        "/a/c%20d",
        "dot segments are resolved, escapes stay escaped"
    );
    assert_eq!((u.username(), u.password()), ("user", Some("p%40ss")));
    assert_eq!((u.query(), u.fragment()), (Some("q=1&q=2"), Some("frag")));
    assert_eq!(
        u.path_segments().unwrap().collect::<Vec<_>>(),
        ["a", "c%20d"]
    );
    assert_eq!(
        u.as_str(),
        "https://user:p%40ss@example.com/a/c%20d?q=1&q=2#frag"
    );
    // Parsing the normalised form is a fixed point.
    assert_eq!(Url::parse(u.as_str()).unwrap(), u);
    for bad in [
        "example.com/x",
        "/relative",
        "http://",
        "http://[::1",
        "http://exa mple.com",
        "http://a:99999/",
    ] {
        assert!(Url::parse(bad).is_err(), "{bad:?}");
    }
    assert_eq!(
        Url::parse("/x"),
        Err(url::ParseError::RelativeUrlWithoutBase)
    );
    // "localhost:80" is a valid URL whose SCHEME is localhost.
    let odd = Url::parse("localhost:80").unwrap();
    assert_eq!(
        (odd.scheme(), odd.host(), odd.cannot_be_a_base()),
        ("localhost", None, true)
    );
}

fn join_rules() {
    let table = [
        ("http://h/a/b", "c", "http://h/a/c"), // last segment REPLACED
        ("http://h/a/b/", "c", "http://h/a/b/c"),
        ("http://h/a/b", "/c", "http://h/c"),
        ("http://h/a/b/", "../c", "http://h/a/c"),
        ("http://h/a/b", "../../../c", "http://h/c"), // cannot climb above the root
        ("http://h/a?x=1", "?y=2", "http://h/a?y=2"),
        ("http://h/a?x=1", "#f", "http://h/a?x=1#f"),
        (
            "https://h/a",
            "//other.example/p",
            "https://other.example/p",
        ), // protocol-relative: NEW HOST
        (
            "http://h/a",
            "https://other.example/",
            "https://other.example/",
        ),
    ];
    for (base, href, want) in table {
        assert_eq!(
            Url::parse(base).unwrap().join(href).unwrap().as_str(),
            want,
            "{base} + {href}"
        );
    }
    let base = Url::parse("http://h/a/b/").unwrap();
    let target = Url::parse("http://h/a/x/y").unwrap();
    let rel = base.make_relative(&target).unwrap();
    assert_eq!(base.join(&rel).unwrap(), target, "{rel}");
}

fn building_urls_safely() {
    let hostile = "../admin?x=1&y=#z";
    // WRONG: string formatting lets the value change the URL structure.
    let naive = Url::parse(&format!("https://api.example/users/{hostile}")).unwrap();
    assert_eq!(
        (naive.path(), naive.query()),
        ("/admin", Some("x=1&y=")),
        "left /users, and grew a query and a fragment"
    );
    // RIGHT: one segment stays one segment.
    let mut u = Url::parse("https://api.example/users").unwrap();
    u.path_segments_mut().unwrap().push(hostile);
    assert_eq!(u.path(), "/users/..%2Fadmin%3Fx=1&y=%23z");
    assert_eq!(u.query(), None);
    assert_eq!(
        percent_decode_str(u.path_segments().unwrap().next_back().unwrap())
            .decode_utf8()
            .unwrap(),
        hostile
    );
    u.query_pairs_mut()
        .append_pair("q", "a b&c=d")
        .append_pair("empty", "");
    assert_eq!(u.query(), Some("q=a+b%26c%3Dd&empty="));
    let pairs: Vec<(String, String)> = u.query_pairs().into_owned().collect();
    assert_eq!(
        pairs,
        [("q".into(), "a b&c=d".into()), ("empty".into(), "".into())]
    );
    // set_query takes an ALREADY structured string: '&' and '=' keep their meaning.
    u.set_query(Some("a=1&b=x y"));
    assert_eq!(u.query(), Some("a=1&b=x%20y"));
    u.set_query(None);
    u.set_path("p q");
    u.set_fragment(Some("s"));
    assert_eq!(u.as_str(), "https://api.example/p%20q#s");
    let w = Url::parse_with_params("https://x.example/s", [("k", "v 1")]).unwrap();
    assert_eq!(w.as_str(), "https://x.example/s?k=v+1");
    let body = form_urlencoded::Serializer::new(String::new())
        .append_pair("name", "a+b c")
        .finish();
    assert_eq!(body, "name=a%2Bb+c");
}

fn hosts_origins_and_ssrf() {
    // String checks on URLs are wrong. Compare the parsed host.
    let trick = Url::parse("https://bank.example@evil.example/").unwrap();
    assert!(trick.as_str().starts_with("https://bank.example"));
    assert_eq!(trick.host_str(), Some("evil.example"));
    let sub = Url::parse("https://bank.example.evil.example/").unwrap();
    assert!(sub.host_str().unwrap().starts_with("bank.example"));
    // The WHATWG parser canonicalises unusual IPv4 spellings, so check the PARSED address.
    for spelled in [
        "http://127.1/",
        "http://0x7f.0.0.1/",
        "http://2130706433/",
        "http://0177.0.0.1/",
    ] {
        match Url::parse(spelled).unwrap().host() {
            Some(Host::Ipv4(ip)) => assert!(ip.is_loopback(), "{spelled} -> {ip}"),
            other => panic!("{spelled}: {other:?}"),
        }
    }
    assert!(
        matches!(Url::parse("http://[::1]:8080/").unwrap().host(), Some(Host::Ipv6(ip)) if ip.is_loopback())
    );
    // (A domain name can still RESOLVE to a private address: SSRF defence must also check after DNS resolution.)
    let origin = |s| Url::parse(s).unwrap().origin();
    assert_eq!(
        origin("https://a.example/x"),
        origin("https://a.example:443/y")
    );
    assert_ne!(origin("https://a.example"), origin("http://a.example"));
    assert_ne!(
        origin("data:text/plain,x"),
        origin("data:text/plain,x"),
        "opaque origins never compare equal"
    );
    assert_eq!(
        Url::parse("https://bücher.example/").unwrap().host_str(),
        Some("xn--bcher-kva.example")
    );
}

/// RFC 3986 "unreserved" characters stay; everything else is escaped. Good for one component of anything.
const COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');
const SPACES_ONLY: &AsciiSet = &CONTROLS.add(b' ');

struct Rng(u64);
impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
    fn bytes(&mut self, max: u64) -> Vec<u8> {
        (0..self.below(max + 1))
            .map(|_| self.below(256) as u8)
            .collect()
    }
    fn string(&mut self) -> String {
        const P: [&str; 16] = [
            "a", "Z", "9", " ", "+", "%", "%2", "/", "?", "#", "&", "=", "é", "🦀", "\0", "~",
        ];
        (0..self.below(12))
            .map(|_| P[self.below(P.len() as u64) as usize])
            .collect()
    }
}

fn percent_encoding_sets() {
    let s = "a b/c?d=é~";
    assert_eq!(
        utf8_percent_encode(s, COMPONENT).to_string(),
        "a%20b%2Fc%3Fd%3D%C3%A9~"
    );
    assert_eq!(
        utf8_percent_encode(s, SPACES_ONLY).to_string(),
        "a%20b/c?d=%C3%A9~",
        "non-ASCII is ALWAYS escaped"
    );
    assert_eq!(
        utf8_percent_encode("-~", NON_ALPHANUMERIC).to_string(),
        "%2D%7E"
    );
    // PITFALL: an escape set without '%' is not reversible.
    assert_eq!(
        utf8_percent_encode("100%41", SPACES_ONLY).to_string(),
        "100%41"
    );
    assert_eq!(percent_decode_str("100%41").decode_utf8().unwrap(), "100A");
    // Decoding is lenient: a malformed escape passes through unchanged. '+' is NOT a space here.
    assert_eq!(
        percent_decode_str("%zz%4+").decode_utf8().unwrap(),
        "%zz%4+"
    );
    // Decoded bytes need not be UTF-8.
    assert!(percent_decode_str("%FF").decode_utf8().is_err());
    assert_eq!(percent_decode_str("%FF").decode_utf8_lossy(), "\u{FFFD}");
}

fn base64_engines_and_errors() {
    let data = [0xFBu8, 0xEF, 0xFF, 0x01];
    assert_eq!(STANDARD.encode(data), "++//AQ==");
    assert_eq!(STANDARD_NO_PAD.encode(data), "++//AQ");
    assert_eq!(URL_SAFE.encode(data), "--__AQ==");
    assert_eq!(URL_SAFE_NO_PAD.encode(data), "--__AQ");
    // RFC 4648 section 10 test vectors.
    for (plain, enc) in [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ] {
        assert_eq!(STANDARD.encode(plain), enc);
        assert_eq!(STANDARD.decode(enc).unwrap(), plain.as_bytes());
    }
    // Engines are STRICT: alphabet and padding must match, and non-canonical trailing bits are rejected.
    assert!(matches!(
        STANDARD.decode("--__AQ=="),
        Err(base64::DecodeError::InvalidByte(0, b'-'))
    ));
    assert!(STANDARD.decode("Zg").is_err(), "missing padding");
    assert!(
        STANDARD_NO_PAD.decode("Zg==").is_err(),
        "unexpected padding"
    );
    assert!(
        STANDARD.decode("Zh==").is_err(),
        "two spellings of one value are not both accepted"
    );
    assert!(
        STANDARD.decode("Zm9v\n").is_err(),
        "whitespace: strip it yourself for MIME input"
    );
    // A JWT-style segment: base64url, no padding. (Decoding is NOT verification: check the signature.)
    let claims = URL_SAFE_NO_PAD.encode(br#"{"sub":"1"}"#);
    let v: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(&claims).unwrap()).unwrap();
    assert_eq!(v["sub"], "1");
    let mut out = String::from("Basic ");
    STANDARD.encode_string("user:pass", &mut out);
    assert_eq!(out, "Basic dXNlcjpwYXNz");
}

fn hex_and_data_encoding() {
    assert_eq!(hex::encode([0x00, 0xAB, 0xff]), "00abff");
    assert_eq!(hex::encode_upper([0xab]), "AB");
    assert_eq!(hex::decode("AbCd").unwrap(), [0xab, 0xcd]);
    assert_eq!(hex::decode("abc"), Err(hex::FromHexError::OddLength));
    assert_eq!(
        hex::decode("zz"),
        Err(hex::FromHexError::InvalidHexCharacter { c: 'z', index: 0 })
    );
    let mut key = [0u8; 4];
    hex::decode_to_slice("01020304", &mut key).unwrap(); // fixed size, no allocation
    assert!(
        hex::decode_to_slice("0102", &mut key).is_err(),
        "wrong length is an error"
    );
    assert_eq!(BASE32.encode(b"foobar"), "MZXW6YTBOI======");
    assert_eq!(BASE32_NOPAD.decode(b"MZXW6YTBOI").unwrap(), b"foobar");
    assert!(
        BASE32.decode(b"mzxw6ytboi======").is_err(),
        "this base32 is case-sensitive"
    );
    assert_eq!(HEXLOWER_PERMISSIVE.decode(b"aBcD").unwrap(), [0xab, 0xcd]);
}

fn setters_slicing_and_redaction() {
    use url::Position;
    let mut u = Url::parse("https://ann:s3cret@api.example:8443/v1/items?token=abc#top").unwrap();
    // Position indexes the serialised URL: substrings without re-formatting.
    assert_eq!(
        &u[..Position::AfterPath],
        "https://ann:s3cret@api.example:8443/v1/items"
    );
    assert_eq!(
        &u[Position::BeforeHost..Position::AfterPort],
        "api.example:8443"
    );
    assert_eq!(&u[Position::BeforePath..], "/v1/items?token=abc#top");
    // PITFALL: Display includes the password. Redact before logging.
    assert!(u.to_string().contains("s3cret"));
    u.set_password(None).unwrap();
    u.set_username("").unwrap();
    u.set_query(None);
    u.set_fragment(None);
    assert_eq!(u.as_str(), "https://api.example:8443/v1/items");

    // Setters enforce the URL standard and report refusal; the value is unchanged on error. Do not ignore the Result.
    assert_eq!(
        u.set_scheme("foo"),
        Err(()),
        "cannot turn a special scheme into a non-special one"
    );
    assert_eq!(u.set_scheme("http"), Ok(()));
    assert!(u.set_host(None).is_err(), "http requires a host");
    assert!(u.set_host(Some("bad host")).is_err());
    assert_eq!(u.as_str(), "http://api.example:8443/v1/items");
    u.set_port(Some(80)).unwrap();
    assert_eq!(
        (u.port(), u.as_str()),
        (None, "http://api.example/v1/items"),
        "the default port disappears"
    );
    let mut mail = Url::parse("mailto:a@b.example").unwrap();
    assert!(
        mail.set_port(Some(25)).is_err() && mail.path_segments_mut().is_err(),
        "no authority, not hierarchical"
    );
    // domain() is None for IP addresses, which makes it a poor "give me the host" accessor.
    assert_eq!(
        (
            u.domain(),
            Url::parse("http://127.0.0.1/").unwrap().domain()
        ),
        (Some("api.example"), None)
    );

    // The parser silently repairs some input. Strict validators can observe the repairs.
    let seen = std::cell::RefCell::new(Vec::new());
    let callback = |v| seen.borrow_mut().push(v);
    let repaired = Url::options()
        .syntax_violation_callback(Some(&callback))
        .parse("  https:\\\\h.example\\a\tb ")
        .unwrap();
    assert_eq!(
        repaired.as_str(),
        "https://h.example/ab",
        "backslashes became slashes; the tab vanished"
    );
    let seen = seen.into_inner();
    assert!(
        seen.contains(&url::SyntaxViolation::Backslash)
            && seen.contains(&url::SyntaxViolation::TabOrNewlineIgnored),
        "{seen:?}"
    );

    // Parsing a form body or a raw query without a URL.
    let pairs: Vec<(String, String)> = form_urlencoded::parse(b"a=1+2&b=%26&b=&c")
        .into_owned()
        .collect();
    assert_eq!(
        pairs,
        [
            ("a".into(), "1 2".into()),
            ("b".into(), "&".into()),
            ("b".into(), "".into()),
            ("c".into(), "".into())
        ]
    );
}

#[cfg(unix)]
fn file_urls() {
    let path = std::path::Path::new("/tmp/report #1?.txt");
    let u = Url::from_file_path(path).unwrap();
    assert_eq!(u.as_str(), "file:///tmp/report%20%231%3F.txt");
    assert_eq!(u.to_file_path().unwrap(), path);
    // WRONG: formatting a path into a URL string cuts it at '#'.
    let naive = Url::parse(&format!("file://{}", path.display())).unwrap();
    assert_eq!(
        (naive.path(), naive.fragment()),
        ("/tmp/report%20", Some("1?.txt"))
    );
    assert!(
        Url::from_file_path("relative/path").is_err(),
        "needs an absolute path"
    );
    // A directory URL ends in '/', so that join() keeps the directory.
    let dir = Url::from_directory_path("/srv/data").unwrap();
    assert_eq!(
        dir.join("a.csv").unwrap().as_str(),
        "file:///srv/data/a.csv"
    );
    assert_eq!(
        Url::from_file_path("/srv/data")
            .unwrap()
            .join("a.csv")
            .unwrap()
            .as_str(),
        "file:///srv/a.csv"
    );
    assert!(
        Url::parse("https://h.example/x")
            .unwrap()
            .to_file_path()
            .is_err()
    );
}
#[cfg(not(unix))]
fn file_urls() {}

fn base64_custom_buffers_and_streams() {
    use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
    use std::io::{Read, Write};
    // A lenient reader for tokens from many producers: URL-safe alphabet, padding optional on input, none on output.
    const LENIENT: GeneralPurpose = GeneralPurpose::new(
        &base64::alphabet::URL_SAFE,
        GeneralPurposeConfig::new()
            .with_encode_padding(false)
            .with_decode_padding_mode(DecodePaddingMode::Indifferent),
    );
    assert_eq!(LENIENT.encode(b"f"), "Zg");
    for input in ["Zg", "Zg=="] {
        assert_eq!(LENIENT.decode(input).unwrap(), b"f");
    }
    assert!(
        LENIENT.decode("+/8").is_err(),
        "the alphabet is still strict"
    );

    // No allocation: size the buffer first.
    let secret = [7u8; 32];
    let mut text = [0u8; 64];
    let n = STANDARD.encode_slice(secret, &mut text).unwrap();
    assert_eq!(Some(n), base64::encoded_len(32, true));
    assert_eq!(n, 44);
    assert!(STANDARD.encode_slice(secret, &mut [0u8; 43]).is_err());
    let mut key = [0u8; 32];
    assert_eq!(STANDARD.decode_slice(&text[..n], &mut key).unwrap(), 32);
    assert_eq!(key, secret);
    assert!(
        STANDARD.decode_slice(&text[..n], &mut [0u8; 31]).is_err(),
        "too small is an error, not a truncation"
    );
    // decode_vec and encode_string APPEND to what is already there.
    let mut out = vec![b'>'];
    STANDARD.decode_vec("aGk=", &mut out).unwrap();
    assert_eq!(out, b">hi");

    // Streaming: feed arbitrary pieces. finish/into_inner writes the final partial group; without it data is missing.
    let data: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
    let mut enc = base64::write::EncoderStringWriter::new(&STANDARD);
    for chunk in data.chunks(7) {
        enc.write_all(chunk).unwrap(); // 7 is not a multiple of 3: groups straddle writes
    }
    let encoded = enc.into_inner();
    assert_eq!(encoded, STANDARD.encode(&data));
    let mut back = Vec::new();
    base64::read::DecoderReader::new(encoded.as_bytes(), &STANDARD)
        .read_to_end(&mut back)
        .unwrap();
    assert_eq!(back, data);
    // Concatenating separately encoded chunks is NOT the encoding of the concatenation.
    let glued = STANDARD.encode(b"ab") + &STANDARD.encode(b"cd");
    assert_ne!(glued, STANDARD.encode(b"abcd"));
    assert!(STANDARD.decode(glued).is_err());
}

fn data_encoding_specifications() {
    use data_encoding::{BASE64, BASE64_MIME, Specification};
    // MIME: 76-column lines with CRLF, and line breaks ignored when decoding.
    let data = [0xA5u8; 100];
    let wrapped = BASE64_MIME.encode(&data);
    assert!(
        wrapped.ends_with("\r\n")
            && wrapped.lines().all(|l| l.len() <= 76)
            && wrapped.lines().count() == 2
    );
    assert_eq!(BASE64_MIME.decode(wrapped.as_bytes()).unwrap(), data);
    assert!(BASE64.decode(wrapped.as_bytes()).is_err());

    // Crockford-style base32 for ids humans retype: no I, L, O, U; accept lower case and the look-alikes.
    let mut spec = Specification::new();
    spec.symbols.push_str("0123456789ABCDEFGHJKMNPQRSTVWXYZ");
    spec.translate
        .from
        .push_str("abcdefghjkmnpqrstvwxyzOoIiLl-");
    spec.translate.to.push_str("ABCDEFGHJKMNPQRSTVWXYZ001111-");
    spec.ignore.push('-');
    let crockford = spec.encoding().unwrap();
    let mut r = Rng(0xC20C);
    for _ in 0..1000 {
        let b = r.bytes(40);
        let text = crockford.encode(&b);
        assert_eq!(text.len(), crockford.encode_len(b.len()));
        assert!(text.bytes().all(|c| !b"ILOU".contains(&c)));
        // What a human might type back: lower case, grouped with dashes, O for 0 and l for 1.
        let retyped: String = text
            .to_lowercase()
            .replace('0', "o")
            .replace('1', "l")
            .chars()
            .enumerate()
            .flat_map(|(i, c)| (i % 4 == 3).then_some('-').into_iter().chain([c]))
            .collect();
        assert_eq!(
            crockford.decode(retyped.as_bytes()).unwrap(),
            b,
            "{retyped}"
        );
    }
    // An inconsistent specification is rejected when built, not at use.
    let mut bad = Specification::new();
    bad.symbols.push_str("ABC"); // not a power of two
    assert!(bad.encoding().is_err());
    // Errors carry the input position.
    assert_eq!(crockford.decode(b"00U0").unwrap_err().position, 2);
    // encode_display formats without an intermediate String.
    assert_eq!(
        format!(
            "id={}",
            data_encoding::HEXLOWER.encode_display(&[0xde, 0xad])
        ),
        "id=dead"
    );
}

/// An allow-list for outgoing requests (webhooks, link previews). Everything is checked on the PARSED value.
fn is_allowed_target(raw: &str, allowed_hosts: &[&str]) -> bool {
    let Ok(u) = Url::parse(raw) else { return false };
    u.scheme() == "https"
        && u.username().is_empty()
        && u.password().is_none()
        && u.port().is_none() // None also covers an explicit :443
        && matches!(u.host(), Some(Host::Domain(d)) if allowed_hosts.contains(&d))
}
fn host_allow_list() {
    let allowed = ["api.example.com"];
    for ok in [
        "https://api.example.com/x",
        "HTTPS://API.EXAMPLE.COM:443/x?y=https://evil.example",
    ] {
        assert!(is_allowed_target(ok, &allowed), "{ok}");
    }
    for bad in [
        "http://api.example.com/",
        "https://api.example.com:8443/",
        "https://u@api.example.com/",
        "https://api.example.com.evil.example/",
        "https://evil.example/api.example.com",
        "https://127.0.0.1/",
        "not a url",
    ] {
        assert!(!is_allowed_target(bad, &allowed), "{bad}");
    }
    // PITFALL for DENY lists: a trailing dot names the same DNS host but is a different string to the url crate, so
    // `host != "internal.example"` lets "internal.example." through. An allow-list fails closed.
    let dotted = Url::parse("https://internal.example./").unwrap();
    assert_eq!(dotted.host_str(), Some("internal.example."));
    assert!(dotted.host_str() != Some("internal.example"));
    assert!(!is_allowed_target("https://api.example.com./", &allowed));
    // Host::parse normalises a bare host name the same way, so build allow-lists with it, not with raw config strings.
    assert_eq!(
        Host::parse("API.Example.COM").unwrap(),
        Host::Domain("api.example.com".to_string())
    );
    assert!(matches!(Host::parse("0x7f.1").unwrap(), Host::Ipv4(ip) if ip.is_loopback()));
}

fn round_trip_properties() {
    let mut r = Rng(0x1110);
    for i in 0..3000 {
        let b = r.bytes(100);
        let n = b.len();
        for (engine_len, text, back) in [
            (
                4 * n.div_ceil(3),
                STANDARD.encode(&b),
                STANDARD.decode(STANDARD.encode(&b)),
            ),
            (
                (4 * n).div_ceil(3),
                URL_SAFE_NO_PAD.encode(&b),
                URL_SAFE_NO_PAD.decode(URL_SAFE_NO_PAD.encode(&b)),
            ),
        ] {
            assert_eq!(
                (text.len(), back.unwrap()),
                (engine_len, b.clone()),
                "case {i}"
            );
        }
        assert_eq!(hex::decode(hex::encode(&b)).unwrap(), b);
        assert_eq!(BASE32.decode(BASE32.encode(&b).as_bytes()).unwrap(), b);

        let s = r.string();
        let enc = utf8_percent_encode(&s, COMPONENT).to_string();
        assert!(
            enc.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-._~%".contains(&c)),
            "{enc}"
        );
        assert_eq!(percent_decode_str(&enc).decode_utf8().unwrap(), s);

        // Arbitrary keys and values survive a trip through a URL string.
        let (k, v) = (r.string(), r.string());
        let mut u = Url::parse("https://h.example/").unwrap();
        u.query_pairs_mut().append_pair(&k, &v);
        let reparsed = Url::parse(u.as_str()).unwrap();
        assert_eq!(
            reparsed
                .query_pairs()
                .next()
                .map(|(a, b)| (a.into_owned(), b.into_owned())),
            Some((k, v))
        );
        assert_eq!(reparsed, u);
    }
    println!(
        "round trips: 3000 random inputs through base64, hex, base32, percent-encoding and URL query strings"
    );
}

pub fn main() {
    url_parts_and_normalisation();
    join_rules();
    building_urls_safely();
    hosts_origins_and_ssrf();
    percent_encoding_sets();
    base64_engines_and_errors();
    hex_and_data_encoding();
    round_trip_properties();
    setters_slicing_and_redaction();
    file_urls();
    base64_custom_buffers_and_streams();
    data_encoding_specifications();
    host_allow_list();
    println!("url + encodings: all sections passed");
}
