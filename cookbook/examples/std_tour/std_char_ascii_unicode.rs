//! RECIPE 1022 (std): `char` and `u8` ASCII/Unicode methods: classification, case, digits, conversions, UTF-16, and byte-level lexers.
//! LEVEL: simple
//! Use in: tokenizers, validators, ciphers, word counting, identifier rules, protocol parsers that work on bytes.   Run: cargo run --release --example std_char_ascii_unicode
//! COMMON USES: lexers for config/DSL/SQL, HTTP header token checks (RFC 7230 tchar), hex/base64 decoding, password-policy checks, search normalisation, ROT13/Caesar toys.
//! METHODS: char::is_alphabetic, char::is_numeric, char::is_alphanumeric, char::is_whitespace, char::is_uppercase, char::is_lowercase, char::to_digit, char::from_digit,
//! char::to_ascii_uppercase, char::to_ascii_lowercase, char::is_ascii_digit, char::is_ascii_alphabetic, char::is_ascii_alphanumeric, char::is_ascii_hexdigit,
//! char::is_ascii_punctuation, char::is_ascii_whitespace, char::is_ascii, char::from_u32, char::from, char::try_from, char::to_uppercase, char::to_lowercase, char::is_control,
//! char::len_utf8, char::len_utf16, char::encode_utf8, char::encode_utf16, char::MAX, char::REPLACEMENT_CHARACTER, char::is_digit, char::escape_default, char::escape_unicode,
//! char::eq_ignore_ascii_case, char::make_ascii_uppercase, char::make_ascii_lowercase, char::is_ascii_graphic, char::is_ascii_control, char::is_ascii_uppercase,
//! char::is_ascii_lowercase, char::from_digit, char::decode_utf16, char::from_u32_unchecked, u8::is_ascii_digit, u8::is_ascii_alphabetic, u8::is_ascii_alphanumeric,
//! u8::to_ascii_uppercase, u8::to_ascii_lowercase, u8::is_ascii_whitespace, u8::is_ascii_hexdigit, u8::eq_ignore_ascii_case, u8::is_ascii_punctuation, u8::as_ascii,
//! slice::is_ascii, slice::to_ascii_uppercase, slice::eq_ignore_ascii_case, slice::trim_ascii, slice::make_ascii_lowercase, u8::escape_ascii, slice::escape_ascii
//! DOCS: https://doc.rust-lang.org/std/primitive.char.html
//! DEFINED IN: char and u8 methods are all core (`core::char`, `core::num`); String-producing helpers in the scenarios are alloc.
//! NOT COVERED (and why): from_u32_unchecked (unsafe), as_ascii (nightly), is_ascii_octdigit (trivial), is_digit (shown through to_digit), `char::MIN`/`UNICODE_VERSION` (trivia).
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   is it a letter (any language)       -> `c.is_alphabetic()`; ASCII letter only -> `c.is_ascii_alphabetic()`
//!   digit value of '7'                  -> `c.to_digit(10)` -> Some(7); hex -> `to_digit(16)`
//!   digit char from a number            -> `char::from_digit(7, 10)`
//!   upper/lower case a char             -> `c.to_uppercase()` (ITERATOR: ß -> SS) / `to_ascii_uppercase()` (char)
//!   a code point number                 -> `c as u32`; back -> `char::from_u32(n)` (Option) / `char::from(u8)` (infallible)
//!   byte as char                        -> `b as char` (Latin-1 semantic) / `char::from(b)`
//!   bytes in UTF-8 of this char         -> `c.len_utf8()`
//!   lex a byte buffer                   -> `b.is_ascii_digit()`, `b.is_ascii_alphabetic()` on `u8`
//!   compare ASCII case-insensitively    -> `eq_ignore_ascii_case` (u8, char, str, [u8])
//!   skip whitespace                     -> `c.is_whitespace()` (Unicode) / `b.is_ascii_whitespace()` (protocols)
//!   hex pair to byte                    -> `(hi.to_digit(16)? << 4 | lo.to_digit(16)?) as u8`
//!   is the whole buffer ASCII           -> `bytes.is_ascii()` (vectorised)
//!
//! Read: https://doc.rust-lang.org/std/primitive.char.html, https://www.unicode.org/reports/tr15/ (normalisation), https://www.unicode.org/faq/casemap_charprop.html,
//! https://doc.rust-lang.org/std/primitive.u8.html
//!
//! TRY NEXT:
//!   - [S] Make `caesar` handle negative shifts via `rem_euclid`; assert `caesar(caesar(s, k), -k) == s` on 1000 random strings.
//!   - [S] Predict: what does `'ß'.to_uppercase().count()` give, and `'İ'.to_lowercase().count()`? Assert both, then explain in a comment.
//!   - [M] Extend `lex` with string literals and `//` comments and report byte offsets for errors; keep it on `&[u8]`.
//!   - [M] Write `fn is_valid_header_name(&[u8]) -> bool` using a 256-entry `const` table built with a `const fn`; benchmark vs `matches!`.
//!   - [A] PROJECT: combine with `std_str_search_split_parse` and `std_collections_choice_guide`: a streaming word-frequency top-k over a large file with a HashMap vs BTreeMap comparison.
#![allow(dead_code)]

use std::collections::BTreeMap;

const COVERED: &[&str] = &[
    "is_alphabetic",
    "is_numeric",
    "is_alphanumeric",
    "is_whitespace",
    "is_uppercase",
    "is_lowercase",
    "to_digit",
    "from_digit",
    "to_ascii_uppercase",
    "to_ascii_lowercase",
    "is_ascii_digit",
    "is_ascii_alphabetic",
    "is_ascii_alphanumeric",
    "is_ascii_hexdigit",
    "is_ascii_punctuation",
    "is_ascii_whitespace",
    "is_ascii",
    "from_u32",
    "from",
    "try_from",
    "to_uppercase",
    "to_lowercase",
    "is_control",
    "len_utf8",
    "len_utf16",
    "encode_utf8",
    "encode_utf16",
    "MAX",
    "REPLACEMENT_CHARACTER",
    "is_digit",
    "escape_default",
    "escape_unicode",
    "eq_ignore_ascii_case",
    "make_ascii_uppercase",
    "make_ascii_lowercase",
    "is_ascii_graphic",
    "is_ascii_control",
    "is_ascii_uppercase",
    "is_ascii_lowercase",
    "decode_utf16",
    "as",
    "escape_ascii",
    "trim_ascii",
];

struct XorShift(u64);
impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

// ---------------------------------------------------------------- 0. Everyday core

fn section_everyday_core() {
    assert!(
        'a'.is_alphabetic() && 'é'.is_alphabetic() && '世'.is_alphabetic() && !'1'.is_alphabetic()
    );
    assert!('a'.is_alphanumeric() && '7'.is_alphanumeric() && !'-'.is_alphanumeric());
    assert!(' '.is_whitespace() && '\t'.is_whitespace() && '\u{a0}'.is_whitespace());
    assert!('A'.is_uppercase() && 'a'.is_lowercase());
    assert_eq!('7'.to_digit(10), Some(7));
    assert_eq!('f'.to_digit(16), Some(15));
    assert_eq!('f'.to_digit(10), None);
    assert_eq!(char::from_digit(7, 10), Some('7'));
    assert_eq!(char::from_digit(11, 16), Some('b')); // lowercase hex
    assert_eq!('a'.to_ascii_uppercase(), 'A');
    assert!('x'.is_ascii_alphabetic() && '5'.is_ascii_digit() && 'F'.is_ascii_hexdigit());
    assert_eq!('a' as u32, 97);
    assert_eq!(char::from_u32(0x41), Some('A'));
    assert_eq!(char::from(65u8), 'A');
    assert_eq!('é'.len_utf8(), 2);
    assert!(b'7'.is_ascii_digit() && !b'x'.is_ascii_digit());
}

// ---------------------------------------------------------------- 1. classification: ASCII vs Unicode

fn section_classification() {
    // is_numeric is broader than is_ascii_digit: it includes digits of other scripts and things like '½' and '²'.
    assert!('٣'.is_numeric() && '½'.is_numeric() && '²'.is_numeric());
    assert!(!'٣'.is_ascii_digit());
    // is_digit(radix) and to_digit only accept ASCII digits/letters: Arabic-Indic digits do NOT parse as numbers.
    assert_eq!('٣'.to_digit(10), None);
    assert!('7'.is_digit(8) == true && '8'.is_digit(8) == false);
    // PITFALL: validating "is a number" with is_numeric then parse::<u32>() would reject '٣': use is_ascii_digit for parsing prechecks.
    assert!("٣".parse::<u32>().is_err());
    // Punctuation / graphic / control
    assert!(
        '!'.is_ascii_punctuation() && ','.is_ascii_punctuation() && !'a'.is_ascii_punctuation()
    );
    assert!('~'.is_ascii_graphic() && !' '.is_ascii_graphic());
    assert!('\n'.is_control() && '\u{7f}'.is_control() && !'a'.is_control());
    assert!('\n'.is_ascii_control());
    assert!(
        '\n'.is_ascii_whitespace() && !'\u{b}'.is_ascii_whitespace(),
        "ASCII whitespace excludes vertical tab; is_whitespace includes it"
    );
    assert!('\u{b}'.is_whitespace());
    assert!(
        'A'.is_ascii_uppercase()
            && 'a'.is_ascii_lowercase()
            && 'é'.is_lowercase()
            && !'é'.is_ascii_lowercase()
    );
    assert!('a'.is_ascii() && !'é'.is_ascii());
    // Titlecase digraphs: 'ǆ' is lowercase; its title form 'ǅ' is neither upper nor lower.
    assert!('ǆ'.is_lowercase() && !'ǅ'.is_lowercase() && !'ǅ'.is_uppercase());
}

// ---------------------------------------------------------------- 2. case mapping pitfalls

fn section_case() {
    // to_uppercase/to_lowercase return an ITERATOR (ToUppercase) because mapping can expand to several chars.
    assert_eq!('ß'.to_uppercase().collect::<String>(), "SS");
    assert_eq!('ß'.to_uppercase().count(), 2);
    assert_eq!('a'.to_uppercase().to_string(), "A"); // ToUppercase implements Display
    assert_eq!('İ'.to_lowercase().count(), 2); // U+0130 -> 'i' + U+0307 combining dot
    // The ASCII variants return a char and never expand.
    assert_eq!('ß'.to_ascii_uppercase(), 'ß');
    let mut c = 'q';
    c.make_ascii_uppercase();
    assert_eq!(c, 'Q');
    c.make_ascii_lowercase();
    assert_eq!(c, 'q');
    assert!('a'.eq_ignore_ascii_case(&'A'));
    // Case-insensitive compare pitfalls:
    // 1. Lowercasing both sides is not enough in general ("STRASSE" vs "straße" need case FOLDING, not lowercasing).
    assert_ne!("STRASSE".to_lowercase(), "straße".to_lowercase());
    // 2. Turkish: in tr/az locales 'I' lowercases to dotless 'ı' and 'i' uppercases to 'İ'. Rust's mapping is locale-independent (always 'I'<->'i'),
    //    so `"TITLE".to_lowercase() == "title"` is wrong for Turkish text but right for protocol tokens. Use ASCII compare for tokens.
    assert_eq!('I'.to_lowercase().next(), Some('i'));
    assert_eq!('ı'.to_uppercase().next(), Some('I'));
    // 3. Composed vs decomposed forms compare unequal even when case-equal: need normalisation (NFC) before comparing (not in std).
    assert_ne!("\u{e9}".to_lowercase(), "e\u{301}".to_lowercase());
    // Length can change: never preallocate to_uppercase output as input.len().
    assert!("ß".to_uppercase().len() > "ß".len() - 1);
}

// ---------------------------------------------------------------- 3. conversions

fn section_conversions() {
    // u8 -> char is infallible (Latin-1 mapping); u32 -> char is fallible; char -> u32 is infallible.
    assert_eq!(char::from(0xe9u8), 'é');
    assert_eq!(char::try_from(0xe9u32), Ok('é'));
    assert!(char::try_from(0xd800u32).is_err());
    assert_eq!(char::from_u32(char::MAX as u32), Some(char::MAX));
    assert_eq!(char::MAX as u32, 0x10ffff);
    assert_eq!(char::REPLACEMENT_CHARACTER, '\u{fffd}');
    assert_eq!(u8::try_from('é'), Ok(0xe9)); // char -> u8 only for U+0000..=U+00FF
    assert!(u8::try_from('世').is_err());
    // PITFALL: `b as char` treats the byte as Latin-1, NOT as UTF-8. Decoding a UTF-8 buffer byte by byte this way yields mojibake.
    let utf8 = "é".as_bytes();
    let mojibake: String = utf8.iter().map(|&b| b as char).collect();
    assert_eq!(mojibake, "Ã©");
    // UTF-16
    assert_eq!('😀'.len_utf16(), 2);
    let units: Vec<u16> = "a😀".encode_utf16().collect();
    let back: String = char::decode_utf16(units.iter().copied())
        .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect();
    assert_eq!(back, "a😀");
    let lone = [0xd800u16, 0x61];
    let lossy: String = char::decode_utf16(lone)
        .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect();
    assert_eq!(lossy, "\u{fffd}a");
    // encode_utf8 into a stack buffer: no heap
    let mut buf = [0u8; 4];
    assert_eq!('€'.encode_utf8(&mut buf).len(), 3);
    assert_eq!('€'.escape_unicode().to_string(), "\\u{20ac}");
    assert_eq!('\n'.escape_default().to_string(), "\\n");
    // Digit arithmetic: the classic `c as u8 - b'0'` is safe only after is_ascii_digit; to_digit does the check for you.
    let n = "9071"
        .chars()
        .try_fold(0u32, |acc, c| c.to_digit(10).map(|d| acc * 10 + d));
    assert_eq!(n, Some(9071));
    assert_eq!(
        "90x1"
            .chars()
            .try_fold(0u32, |acc, c| c.to_digit(10).map(|d| acc * 10 + d)),
        None
    );
}

// ---------------------------------------------------------------- 4. byte-level lexer (u8 ASCII methods)

#[derive(Debug, PartialEq)]
enum Tok<'a> {
    Ident(&'a str),
    Int(u64),
    Punct(u8),
}

/// A lexer over `&[u8]` for an ASCII language: no UTF-8 decoding, byte indices = positions. Non-ASCII bytes are an error.
fn lex(src: &[u8]) -> Result<Vec<Tok<'_>>, (usize, u8)> {
    let mut toks = Vec::new();
    let mut i = 0;
    while i < src.len() {
        let b = src[i];
        if b.is_ascii_whitespace() {
            i += 1;
        } else if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            while i < src.len() && (src[i].is_ascii_alphanumeric() || src[i] == b'_') {
                i += 1;
            }
            // SAFETY-free: every byte in the slice is ASCII, so from_utf8 cannot fail; we still use the checked version.
            toks.push(Tok::Ident(std::str::from_utf8(&src[start..i]).unwrap()));
        } else if b.is_ascii_digit() {
            let mut v = 0u64;
            while i < src.len() && src[i].is_ascii_digit() {
                v = v.saturating_mul(10).saturating_add((src[i] - b'0') as u64);
                i += 1;
            }
            toks.push(Tok::Int(v));
        } else if b.is_ascii_punctuation() {
            toks.push(Tok::Punct(b));
            i += 1;
        } else {
            return Err((i, b));
        }
    }
    Ok(toks)
}

fn section_lexer() {
    let toks = lex(b"let x_1 = 42 + y;").unwrap();
    assert_eq!(
        toks,
        vec![
            Tok::Ident("let"),
            Tok::Ident("x_1"),
            Tok::Punct(b'='),
            Tok::Int(42),
            Tok::Punct(b'+'),
            Tok::Ident("y"),
            Tok::Punct(b';')
        ]
    );
    assert_eq!(lex("é".as_bytes()), Err((0, 0xc3)));
    assert_eq!(
        lex(b"99999999999999999999999"),
        Ok(vec![Tok::Int(u64::MAX)]),
        "saturating, not wrapping"
    );
    // slice-level ASCII helpers
    assert!(b"plain".is_ascii() && !"é".as_bytes().is_ascii());
    assert_eq!(b"MiXed".to_ascii_lowercase(), b"mixed");
    assert!(b"Host".eq_ignore_ascii_case(b"hOST"));
    assert_eq!(b"  x \n".trim_ascii(), b"x");
    assert_eq!(b"a\n\xff".escape_ascii().to_string(), "a\\n\\xff");
    assert_eq!(b'\n'.escape_ascii().to_string(), "\\n");
    let mut v = b"abC".to_vec();
    v.make_ascii_uppercase();
    assert_eq!(v, b"ABC");
    // HTTP token chars (RFC 7230 tchar) as a lexer-style predicate
    fn is_tchar(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
    }
    assert!(
        b"Content-Type".iter().all(|&b| is_tchar(b)) && !b"bad header".iter().all(|&b| is_tchar(b))
    );
}

// ---------------------------------------------------------------- 5. Caesar / ROT13

fn caesar(s: &str, shift: i32) -> String {
    let k = shift.rem_euclid(26) as u8;
    s.chars()
        .map(|c| match c {
            'a'..='z' => ((c as u8 - b'a' + k) % 26 + b'a') as char,
            'A'..='Z' => ((c as u8 - b'A' + k) % 26 + b'A') as char,
            _ => c, // non-letters (and all non-ASCII) pass through
        })
        .collect()
}

fn section_caesar() {
    assert_eq!(caesar("Hello, World!", 3), "Khoor, Zruog!");
    assert_eq!(caesar("Khoor, Zruog!", -3), "Hello, World!");
    assert_eq!(caesar("abc", 26), "abc");
    assert_eq!(caesar("café", 13), "pnsé"); // é untouched: ciphers on ASCII only
    // ROT13 is an involution: applying twice is the identity. Property over random ASCII strings.
    let mut rng = XorShift(99);
    for _ in 0..1000 {
        let s: String = (0..rng.next() % 30)
            .map(|_| (32 + rng.next() % 95) as u8 as char)
            .collect();
        assert_eq!(caesar(&caesar(&s, 13), 13), s);
        assert_eq!(caesar(&caesar(&s, 5), -5), s);
    }
}

// ---------------------------------------------------------------- 6. word frequency

fn word_freq(text: &str) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    for w in text
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
    {
        // to_lowercase allocates once per word; entry avoids a second lookup. For ASCII-only input use eq_ignore_ascii_case keys.
        *m.entry(w.to_lowercase()).or_insert(0) += 1;
    }
    m
}

fn section_word_freq() {
    let f = word_freq("The cat; the CAT! Don't panic: the cafe, the Café.");
    assert_eq!(f["the"], 4);
    assert_eq!(f["cat"], 2);
    assert_eq!(f["don't"], 1);
    assert_eq!(f["café"], 1);
    assert_eq!(f["cafe"], 1, "normalisation would merge these");
    let top = f
        .iter()
        .max_by_key(|(w, c)| (**c, std::cmp::Reverse((*w).clone())))
        .unwrap();
    assert_eq!(top, (&"the".to_string(), &4));
}

// ---------------------------------------------------------------- 7. hex decoding with to_digit

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    s.as_bytes()
        .chunks_exact(2)
        .map(|p| Some(((p[0] as char).to_digit(16)? << 4 | (p[1] as char).to_digit(16)?) as u8))
        .collect()
}

fn section_hex() {
    assert_eq!(decode_hex("00ff7A"), Some(vec![0, 255, 0x7a]));
    assert_eq!(decode_hex("0g"), None);
    assert_eq!(decode_hex("abc"), None);
    // PITFALL: chunking `as_bytes()` of a non-ASCII string is fine here because we then reject non-hex digits; slicing the &str by 2s would panic.
    assert_eq!(decode_hex("é0"), None);
    let enc: String = [0u8, 255, 16]
        .iter()
        .flat_map(|b| {
            [
                char::from_digit((b >> 4) as u32, 16),
                char::from_digit((b & 15) as u32, 16),
            ]
        })
        .flatten()
        .collect();
    assert_eq!(enc, "00ff10");
}

// ---------------------------------------------------------------- Production tips and tricks

fn section_production_tips() {
    // 1. Protocol text (HTTP, JSON keys, config tokens) -> ASCII methods; human text -> Unicode methods. Mixing causes locale bugs.
    assert!(b'\xa0'.is_ascii_whitespace() == false && '\u{a0}'.is_whitespace());
    // 2. `c.to_ascii_lowercase()` in a map/filter: use `eq_ignore_ascii_case` instead of allocating lowercase copies (clippy::manual_ignore_case_cmp).
    assert!("Accept".eq_ignore_ascii_case("ACCEPT"));
    // 3. `c as u8 - b'0'` without a check underflows/garbage; use `to_digit` (returns Option). Anti-pattern value:
    assert_eq!(('x' as u8).wrapping_sub(b'0'), 72);
    assert_eq!('x'.to_digit(10), None);
    // 4. Use char ranges / `matches!` in hot lexers, not `contains` on a string of valid chars (clippy::manual_range_contains for numeric ranges).
    assert!(matches!('m', 'a'..='z'));
    // 5. Don't `to_uppercase().next().unwrap()`: it silently drops expansions (ß -> S). Use the full iterator or to_ascii_uppercase.
    assert_eq!('ß'.to_uppercase().next(), Some('S'));
    // 6. is_alphanumeric on user IDs accepts any script: for ID/slug validation say what you mean.
    assert!('é'.is_alphanumeric() && !'é'.is_ascii_alphanumeric());
    // 7. For hot loops over ASCII data iterate bytes (no UTF-8 decode): `s.bytes().filter(u8::is_ascii_digit)`.
    assert_eq!("a1b22".bytes().filter(u8::is_ascii_digit).count(), 3);
    // 8. Check a whole buffer with `is_ascii()` (SIMD-friendly) then take the byte fast path.
    assert!("hello".is_ascii());
    // 9. Control characters in logs enable log injection: escape with escape_debug/escape_default before writing user text to a log line.
    assert_eq!("ok\nFAKE LINE".escape_debug().to_string(), "ok\\nFAKE LINE");
    // 10. Use `char::from(u8)` / `u32::from(c)` (lossless From) over `as` so a later type change fails to compile (clippy::cast_lossless).
    let c = char::from(b'a');
    assert_eq!(u32::from(c), 97);
    // 11. A replacement char in output means decoding failed upstream: count U+FFFD to alert on corrupt input.
    assert_eq!(
        String::from_utf8_lossy(&[0xff, b'a'])
            .chars()
            .filter(|&c| c == char::REPLACEMENT_CHARACTER)
            .count(),
        1
    );
    // 12. Identifier rules: XID_Start/XID_Continue are NOT in std; `is_alphabetic() || '_'` over-accepts (use the `unicode-ident` crate for languages).
    assert!('٣'.is_alphanumeric());
}

fn main() {
    let mut seen = std::collections::HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    section_everyday_core();
    section_classification();
    section_case();
    section_conversions();
    section_lexer();
    section_caesar();
    section_word_freq();
    section_hex();
    section_production_tips();
    println!("{}", caesar("Rust std tour", 13));
    println!("{:?}", word_freq("a b a").iter().collect::<Vec<_>>());
    println!(
        "covered {} char/u8 entries; all asserts passed",
        COVERED.len()
    );
}
