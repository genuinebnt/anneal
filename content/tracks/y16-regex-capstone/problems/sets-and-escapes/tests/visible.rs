use solution::*;

/// The set of the single item the pattern parses to, asked about each character of `probe`.
#[allow(dead_code)]
fn members(pattern: &str, probe: &str) -> Result<String, ParseError> {
    match parse(pattern)? {
        Ast::Class(class) => Ok(probe.chars().filter(|&c| class.matches(c)).collect()),
        other => panic!("{pattern:?} should be one set of characters, not {other:?}"),
    }
}

#[allow(dead_code)]
const PROBE: &str = "abcxyzAZ059_ \t\n\r\u{b}\u{c}-^]\\[.*dswé";

#[allow(dead_code)]
fn of(chars: &str) -> Result<String, ParseError> {
    Ok(PROBE.chars().filter(|c| chars.contains(*c)).collect())
}

#[allow(dead_code)]
fn all_but(chars: &str) -> Result<String, ParseError> {
    Ok(PROBE.chars().filter(|c| !chars.contains(*c)).collect())
}

#[test]
fn shorthands() {
    check!(r"\d", members(r"\d", PROBE), of("059"));
    check!(r"\w", members(r"\w", PROBE), of("abcxyzAZ059_dsw"));
    check!(r"\s", members(r"\s", PROBE), of(" \t\n\r\u{b}\u{c}"));
}

#[test]
fn escaped_symbols_are_ordinary() {
    check!(r"\.", members(r"\.", PROBE), of("."));
    check!(r"\*", members(r"\*", PROBE), of("*"));
    check!(r"\\", members(r"\\", PROBE), of("\\"));
    check!(r"\n", members(r"\n", PROBE), of("\n"));
}

#[test]
fn sets_and_their_opposites() {
    check!("[a-c9]", members("[a-c9]", PROBE), of("abc9"));
    check!("[^a-c9]", members("[^a-c9]", PROBE), all_but("abc9"));
    check!("[]", members("[]", PROBE), of(""));
    check!("[^]", members("[^]", PROBE), all_but(""));
}

#[test]
fn dashes_and_carets() {
    check!("[-a]", members("[-a]", PROBE), of("-a"));
    check!("[a-]", members("[a-]", PROBE), of("-a"));
    check!("[a^]", members("[a^]", PROBE), of("a^"));
    check!("[a-c-z]", members("[a-c-z]", PROBE), of("abc-z"));
}

#[test]
fn escapes_inside_sets() {
    check!(r"[\d_]", members(r"[\d_]", PROBE), of("059_"));
    check!(r"[\]\-]", members(r"[\]\-]", PROBE), of("]-"));
    check!(r"[^\s]", members(r"[^\s]", PROBE), all_but(" \t\n\r\u{b}\u{c}"));
}

#[test]
fn new_errors() {
    check!("ab[cd", parse("ab[cd"), Err(ParseError::UnclosedClass(2)));
    check!("[z-a]", parse("[z-a]"), Err(ParseError::BadRange(1)));
    check!(r"a\q", parse(r"a\q"), Err(ParseError::BadEscape(1)));
    check!("ab\\", parse("ab\\"), Err(ParseError::TrailingEscape(2)));
}
