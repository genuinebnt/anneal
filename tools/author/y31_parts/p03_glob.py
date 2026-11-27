from y17_parts.common import broken

SOLUTION = r'''/// One token of the pattern at `p[i..]`: whether it accepts `c`, and where the next token starts.
fn token(p: &[u8], mut i: usize, c: u8) -> (bool, usize) {
    match p[i] {
        b'?' => (true, i + 1),
        b'\\' if i + 1 < p.len() => (p[i + 1] == c, i + 2),
        b'[' => {
            i += 1;
            let negated = p.get(i) == Some(&b'^');
            i += usize::from(negated);
            let mut found = false;
            while i < p.len() && p[i] != b']' {
                let escaped = p[i] == b'\\' && i + 1 < p.len();
                i += usize::from(escaped);
                let low = p[i];
                if p.get(i + 1) == Some(&b'-') && i + 2 < p.len() && p[i + 2] != b']' {
                    let high = p[i + 2];
                    found |= low.min(high) <= c && c <= low.max(high);
                    i += 3;
                } else {
                    found |= low == c;
                    i += 1;
                }
            }
            (found != negated, (i + 1).min(p.len()))
        }
        literal => (literal == c, i + 1),
    }
}

/// Whether the whole of `name` fits `pattern`.
pub fn matches(pattern: &[u8], name: &[u8]) -> bool {
    let (mut p, mut t) = (0, 0);
    // where to resume if the latest `*` has to take one byte more
    let mut star: Option<(usize, usize)> = None;
    while t < name.len() {
        if pattern.get(p) == Some(&b'*') {
            p += 1;
            star = Some((p, t));
            continue;
        }
        if p < pattern.len() {
            let (accepted, next) = token(pattern, p, name[t]);
            if accepted {
                p = next;
                t += 1;
                continue;
            }
        }
        let Some((after, taken)) = star else {
            return false;
        };
        p = after;
        t = taken + 1;
        star = Some((after, taken + 1));
    }
    pattern[p..].iter().all(|&b| b == b'*')
}
'''

STARTER = r'''/// Whether the whole of `name` fits `pattern`.
pub fn matches(pattern: &[u8], name: &[u8]) -> bool {
    todo!()
}
'''

HELPERS = r'''
#[allow(dead_code)]
fn table(cases: &[(&str, &str, bool)]) {
    for &(pattern, name, want) in cases {
        check!(format!("pattern {pattern:?}, name {name:?}"), matches(pattern.as_bytes(), name.as_bytes()), want);
    }
}
'''

VISIBLE = [
    HELPERS,
    r'''
#[test]
fn plain_names_and_question_marks() {
    table(&[("cat", "cat", true), ("cat", "cats", false), ("cat", "ca", false), ("c?t", "cut", true), ("c?t", "ct", false), ("", "", true), ("", "a", false)]);
}

#[test]
fn stars() {
    table(&[
        ("*", "", true), ("*", "anything", true), ("user:*", "user:42", true), ("user:*", "users", false),
        ("*.log", "a.b.log", true), ("a*b*c", "a-b-b-c", true), ("a*b*c", "acb", false),
    ]);
}

#[test]
fn sets_of_characters() {
    table(&[
        ("[abc]1", "b1", true), ("[abc]1", "d1", false), ("v[0-9]", "v7", true), ("v[0-9]", "vx", false),
        ("[^0-9]", "x", true), ("[^0-9]", "5", false), ("[z-a]", "m", true),
    ]);
}

#[test]
fn escapes() {
    table(&[(r"a\*b", "a*b", true), (r"a\*b", "axxb", false), (r"\?", "?", true), (r"\?", "x", false), (r"\\", r"\", true)]);
}

#[test]
fn a_hard_pattern_answers_quickly() {
    let pattern = "a*".repeat(40) + "b";
    let name = "a".repeat(5000);
    check!("40 stars against 5000 letters with no b", matches(pattern.as_bytes(), name.as_bytes()), false);
}
''',
]

HIDDEN = [
    HELPERS,
    r'''
#[test]
fn stars_at_every_place() {
    table(&[
        ("**", "", true), ("a**b", "ab", true), ("*a", "a", true), ("a*", "a", true), ("*a*", "bab", true), ("*ab", "aab", true),
        ("a*ab", "aab", true), ("a*ab", "ab", false), ("*aab", "aaab", true), ("a?*", "a", false), ("*?", "", false), ("?*?", "ab", true),
        ("*x*y*", "xy", true), ("*x*y*", "yx", false), ("ab*", "a", false),
    ]);
}

#[test]
fn the_inside_of_a_set() {
    table(&[
        ("[]", "a", false), ("[]", "", false), ("[^]", "a", true), ("[^]", "", false), ("x[]y", "xy", false),
        ("[a]", "a", true), ("[a-a]", "a", true), ("[a-c]", "d", false), ("[a-ce]", "e", true), ("[-a]", "-", true),
        ("[a-]", "-", true), ("[a-]", "a", true), ("[a-]", "b", false), ("[*]", "*", true), ("[*]", "x", false), ("[?]", "x", false),
        (r"[\]]", "]", true), (r"[\]]", "\\", false), (r"[\^a]", "^", true), (r"[\-]", "-", true), (r"[a\-c]", "b", false),
        ("[a^]", "^", true), ("[^^]", "^", false), ("[[]", "[", true), ("a]", "a]", true), ("[ab][cd]", "ad", true),
    ]);
}

#[test]
fn what_is_left_unfinished() {
    table(&[
        // a set with no `]` runs to the end of the pattern
        ("[abc", "b", true), ("[abc", "d", false), ("x[ab", "xa", true), ("[^ab", "c", true), ("[a-", "-", true), ("[", "a", false),
        ("[^", "a", true),
        // a lone backslash at the end is itself
        ("a\\", "a\\", true), ("a\\", "a", false),
        ("[ab\\", "\\", true),
    ]);
}

#[test]
fn the_whole_name_must_fit() {
    table(&[("a", "ab", false), ("a", "ba", false), ("?", "", false), ("[a]", "aa", false), ("", "x", false), ("abc", "abc", true), ("a?c", "abbc", false)]);
}

#[test]
fn bytes_that_are_not_text() {
    check!("any byte", matches(b"?", &[0xff]), true);
    check!("a range of bytes", matches(&[b'[', 0x80, b'-', 0xff, b']'], &[0xc0]), true);
    check!("a zero byte", matches(b"a*", &[b'a', 0, 0]), true);
    check!("one character of two bytes is two bytes", matches(b"?", "é".as_bytes()), false);
}

/// The slow and obvious matcher over parsed tokens.
#[allow(dead_code)]
#[derive(Debug, Clone)]
enum Piece {
    Star,
    Any,
    Literal(u8),
    Set(bool, Vec<(u8, u8)>),
}

#[allow(dead_code)]
fn slow(pieces: &[Piece], name: &[u8]) -> bool {
    match pieces.first() {
        None => name.is_empty(),
        Some(Piece::Star) => (0..=name.len()).any(|i| slow(&pieces[1..], &name[i..])),
        Some(piece) => {
            let Some(&c) = name.first() else { return false };
            let fits = match piece {
                Piece::Literal(l) => *l == c,
                Piece::Set(negated, ranges) => ranges.iter().any(|&(a, b)| a <= c && c <= b) != *negated,
                _ => true,
            };
            fits && slow(&pieces[1..], &name[1..])
        }
    }
}

#[test]
fn random_patterns_match_a_slow_matcher() {
    let mut rng = anneal_prelude::Rng::new(0x3104);
    for _ in 0..20_000 {
        let (mut text, mut pieces) = (String::new(), Vec::new());
        for _ in 0..rng.below(6) {
            match rng.below(6) {
                0 => {
                    text.push('*');
                    pieces.push(Piece::Star);
                }
                1 => {
                    text.push('?');
                    pieces.push(Piece::Any);
                }
                2 => {
                    let negated = rng.bool();
                    text.push('[');
                    if negated {
                        text.push('^');
                    }
                    let mut ranges = Vec::new();
                    for _ in 0..rng.below(3) {
                        let a = *rng.pick(b"abc");
                        if rng.bool() {
                            let b = *rng.pick(b"abc");
                            text.push(a as char);
                            text.push('-');
                            text.push(b as char);
                            ranges.push((a.min(b), a.max(b)));
                        } else {
                            text.push(a as char);
                            ranges.push((a, a));
                        }
                    }
                    text.push(']');
                    pieces.push(Piece::Set(negated, ranges));
                }
                3 => {
                    let c = *rng.pick(b"*?[a\\");
                    text.push('\\');
                    text.push(c as char);
                    pieces.push(Piece::Literal(c));
                }
                _ => {
                    let c = *rng.pick(b"abc");
                    text.push(c as char);
                    pieces.push(Piece::Literal(c));
                }
            }
        }
        let len = rng.below(7);
        let name = rng.string(len, "abc*?[\\");
        check!(format!("pattern {text:?}, name {name:?}"), matches(text.as_bytes(), name.as_bytes()), slow(&pieces, name.as_bytes()));
    }
}

#[test]
fn many_stars_and_a_long_name() {
    // trying every way to split the name among the stars cannot finish
    let name = vec![b'a'; 100_000];
    for pattern in ["a*".repeat(30) + "b", "*a".repeat(30) + "b", "*?*?*?*?*?*?*?*?*b".to_string(), "*[ab]".repeat(20) + "c"] {
        check!(format!("{pattern:?} against 100000 letters"), matches(pattern.as_bytes(), &name), false);
    }
    let pattern = "*a".repeat(1000);
    check!("a thousand stars that do match", matches(pattern.as_bytes(), &name), true);
}

#[test]
fn a_long_literal_tail_after_a_star() {
    // retrying from each position is quadratic in the worst case, which is still fine at these sizes
    let pattern = "*".to_string() + &"a".repeat(100) + "b";
    let name = "a".repeat(50_000);
    check!("no b anywhere", matches(pattern.as_bytes(), name.as_bytes()), false);
}
''',
]

STATEMENT = r'''
An operator searches a store with millions of key names: `session:*`, `user:?:cart`, `log-2026-1[0-2]`. Write the test that decides whether one name fits such a pattern.

**`matches(pattern, name)`** is true when the **whole** name fits the whole pattern. Both are bytes.

| In the pattern | Matches |
|---|---|
| `*` | any number of bytes, none included |
| `?` | exactly one byte |
| `[abc]` | one of the listed bytes |
| `[a-z]` | one byte in the range; `[z-a]` means the same |
| `[^...]` | one byte **not** listed |
| `\x` | the byte `x` itself, also inside `[...]` |
| anything else | itself |

Inside brackets:

- `*`, `?` and `[` are ordinary bytes. `^` is special only straight after `[`.
- A `-` that comes first or last is itself. So is an escaped one.
- The first `]` that is not escaped closes the set, even straight after `[`. So `[]` matches nothing, and `[^]` any one byte.
- A range is `x-y`, where the `y` is the byte straight after the `-`. (The tests do not escape the end of a range.)

Nothing is ever an error. A `[` with no closing `]` lists everything up to the end of the pattern. A single `\` at the very end is an ordinary backslash.

The name may be 100000 bytes long and the pattern may hold a thousand `*`. Every answer must come quickly, whatever the pattern.
'''

PROBLEM = dict(
    slug="which-names-fit", title="Which names fit the pattern?", level="medium", stage="data",
    tags=["wildcard matching", "two pointers", "backtracking", "ReDoS"],
    statement=STATEMENT, starter=STARTER, solution=SOLUTION, visible=VISIBLE, hidden=HIDDEN,
    teaches=[
        "For wildcards, only the latest `*` ever needs to be tried again.",
        "A matcher that tries every split takes exponential time, and a pattern from a user can ask for it.",
        "Precise rules for malformed input beat silent surprises.",
    ],
    follow_up="1) Why is it enough to go back to the latest `*`, and never to an earlier one? 2) Compile the pattern once and match many names. What would the compiled form look like? 3) Compare with a regex engine built as an automaton: what extra power does it have, and what does that cost here?",
    hints=[
        ("approach", "Walk the pattern and the name together. When you meet a mismatch, you may only need to go back to one earlier choice. Which one, and why can no other help?"),
        ("rust", "A helper that reads one pattern token and says whether it accepts a byte, and where the next token starts, keeps the main loop short. Remember the last star as `Option<(pattern_pos, name_pos)>`."),
        ("edge", "After the name ends, only stars may remain in the pattern. Check every bound before you read the byte after a `\\` or a `-`."),
    ],
    notes=("The loop remembers where the latest star was and how much it has taken. On a mismatch it lets that star take one byte more. Earlier stars never need to change, because the latest one can absorb anything they could have.",
           "O(pattern × name) in the worst case", "O(1)"),
    wrong={
        "backtracking_over_every_split": broken(SOLUTION, (
            "/// Whether the whole of `name` fits `pattern`.\npub fn matches(pattern: &[u8], name: &[u8]) -> bool {\n    let (mut p, mut t) = (0, 0);",
            "fn all(pattern: &[u8], name: &[u8]) -> bool {\n    if pattern.is_empty() {\n        return name.is_empty();\n    }\n    if pattern[0] == b'*' {\n        return (0..=name.len()).any(|i| all(&pattern[1..], &name[i..]));\n    }\n    if name.is_empty() {\n        return false;\n    }\n    let (ok, next) = token(pattern, 0, name[0]);\n    ok && all(&pattern[next..], &name[1..])\n}\n\npub fn matches(pattern: &[u8], name: &[u8]) -> bool {\n    if true {\n        return all(pattern, name);\n    }\n    let (mut p, mut t) = (0, 0);")),
        "ranges_only_forwards": broken(SOLUTION, ("                    found |= low.min(high) <= c && c <= low.max(high);", "                    found |= low <= c && c <= high;")),
        "trailing_dash_starts_a_range": broken(SOLUTION, (
            "                if p.get(i + 1) == Some(&b'-') && i + 2 < p.len() && p[i + 2] != b']' {", "                if p.get(i + 1) == Some(&b'-') && i + 2 < p.len() {")),
        "no_escapes_inside_sets": broken(SOLUTION, (
            "                let escaped = p[i] == b'\\\\' && i + 1 < p.len();", "                let escaped = false;")),
        "pattern_left_over_is_ignored": broken(SOLUTION, ("    pattern[p..].iter().all(|&b| b == b'*')", "    p >= pattern.len() || pattern[p] == b'*'")),
    },
)
