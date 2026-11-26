//! Behaviour claims made on the page.

use cow_borrow_until_you_must_own::*;
use std::borrow::Cow;
use std::collections::HashMap;

#[allow(clippy::ptr_arg)] // the variant is the point, so we need the `Cow` itself
fn is_borrowed<T: ToOwned + ?Sized>(c: &Cow<'_, T>) -> bool {
    matches!(c, Cow::Borrowed(_))
}

/// The obvious always-allocating version, used as the oracle.
fn escape_oracle(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[test]
fn escape_matches_the_oracle_and_borrows_exactly_when_clean() {
    let alphabet = ['a', 'b', ' ', 'é', '漢', '<', '>', '&', '"', '\''];
    let mut state = 0x9E37_79B9_7F4A_7C15_u64; // seeded, so a failure repeats
    let (mut borrowed, mut owned) = (0, 0);
    for _ in 0..2000 {
        let mut input = String::new();
        let len = state % 7;
        for _ in 0..len {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            input.push(alphabet[(state % alphabet.len() as u64) as usize]);
        }
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);

        let out = escape_html(&input);
        assert_eq!(out, escape_oracle(&input), "input {input:?}");
        let clean = !input.contains(['<', '>', '&', '"', '\'']);
        assert_eq!(is_borrowed(&out), clean, "input {input:?}");
        if clean {
            assert!(std::ptr::eq(out.as_ptr(), input.as_ptr()), "same bytes");
            borrowed += 1;
        } else {
            owned += 1;
        }
    }
    assert!(
        borrowed > 100 && owned > 100,
        "{borrowed} / {owned}: both paths ran"
    );
}

#[test]
fn to_mut_leaves_the_source_alone_and_is_only_as_lazy_as_you_are() {
    let source = String::from("dir");
    let mut path = Cow::Borrowed(source.as_str());
    ensure_trailing_slash(&mut path);
    assert_eq!(path, "dir/");
    assert_eq!(source, "dir");

    let mut done = Cow::Borrowed("dir/");
    ensure_trailing_slash(&mut done);
    assert!(is_borrowed(&done));

    let mut eager = Cow::Borrowed("already lower");
    lowercase_eager(&mut eager);
    assert!(!is_borrowed(&eager), "cloned although nothing changed");

    let mut lazy = Cow::Borrowed("already lower");
    lowercase_lazy(&mut lazy);
    assert!(is_borrowed(&lazy));
    let mut upper = Cow::Borrowed("MiXed");
    lowercase_lazy(&mut upper);
    assert_eq!(upper, "mixed");
}

#[test]
fn owned_does_not_mean_changed() {
    let eq: Cow<'_, str> = Cow::Owned("same".to_string());
    assert_eq!(eq, Cow::Borrowed("same"), "equality looks at the text only");
}

#[test]
#[allow(clippy::suspicious_to_owned)] // the mistake being demonstrated
fn to_owned_on_a_cow_gives_a_cow() {
    let original: Cow<'_, str> = Cow::Borrowed("x");
    let copy: Cow<'_, str> = original.to_owned(); // not a String
    assert!(is_borrowed(&copy));
    let string: String = original.into_owned();
    assert_eq!(string, "x");
}

#[test]
fn into_owned_moves_but_to_string_copies() {
    let owned: Cow<'_, str> = Cow::Owned("hello".repeat(10));
    let buffer = owned.as_ptr();
    let copy = owned.to_string();
    assert!(!std::ptr::eq(copy.as_ptr(), buffer));
    let moved = owned.into_owned();
    assert!(std::ptr::eq(moved.as_ptr(), buffer));
}

#[test]
fn constructors_and_static_values() {
    let text = String::from("t");
    assert!(is_borrowed(&Label::new("literal").text));
    assert!(is_borrowed(&Label::new(&text).text));
    assert!(!is_borrowed(&Label::new(text.clone()).text));

    let detached = {
        let buffer = String::from("temporary");
        Label::new(buffer.as_str()).into_static()
    }; // `buffer` is gone, `detached` is still usable
    assert_eq!(detached.text, "temporary");

    assert!(is_borrowed(&status_text(404)));
    assert_eq!(status_text(418), "status 418");
    assert!(!is_borrowed(&status_text(418)));
}

#[test]
fn a_map_with_cow_keys_is_searched_with_str() {
    let mut m: HashMap<Cow<'static, str>, u32> = HashMap::new();
    m.insert(Cow::Borrowed("literal"), 1);
    m.insert(Cow::Owned("built".to_string()), 2);
    assert_eq!(m.get("literal"), Some(&1));
    assert_eq!(m.get("built"), Some(&2));
    // Borrowed and Owned with the same text are the same key.
    m.insert(Cow::Owned("literal".to_string()), 3);
    assert_eq!((m.len(), m["literal"]), (2, 3));
}

#[test]
fn slices() {
    assert!(is_borrowed(&sorted(&[1, 2, 2, 3])));
    assert!(is_borrowed(&sorted(&[])));
    let s = sorted(&[3, 1, 2]);
    assert!(!is_borrowed(&s));
    assert_eq!(*s, [1, 2, 3]);
}

#[test]
fn std_lossy_conversions_borrow_valid_input() {
    use std::ffi::{CStr, OsStr};
    use std::path::Path;

    assert!(is_borrowed(&String::from_utf8_lossy(b"ok")));
    assert!(is_borrowed(&String::from_utf8_lossy(b"")));
    let bad = String::from_utf8_lossy(b"a\xFFb");
    assert!(!is_borrowed(&bad));
    assert_eq!(bad, "a\u{FFFD}b");

    assert!(is_borrowed(&OsStr::new("name").to_string_lossy()));
    assert!(is_borrowed(&Path::new("a/b").to_string_lossy()));
    let c: &CStr = c"hi";
    assert!(is_borrowed(&c.to_string_lossy()));
    assert!(!is_borrowed(&c"\xFF".to_string_lossy()));
}

#[cfg(unix)]
#[test]
fn invalid_os_strings_are_copied() {
    use std::os::unix::ffi::OsStrExt;
    let name = std::ffi::OsStr::from_bytes(b"f\x80o");
    let text = name.to_string_lossy();
    assert!(!is_borrowed(&text));
    assert_eq!(text, "f\u{FFFD}o");
}

#[test]
fn lossy_from_vec_keeps_the_buffer_when_valid() {
    let bytes = b"valid".to_vec();
    let buffer = bytes.as_ptr();
    assert!(std::ptr::eq(lossy_from_vec(bytes).as_ptr(), buffer));
    assert_eq!(lossy_from_vec(vec![b'a', 0xFF]), "a\u{FFFD}");
}
