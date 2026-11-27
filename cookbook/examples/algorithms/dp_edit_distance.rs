//! RECIPE 72 (dp): edit distance (Levenshtein) and "did you mean ...?" suggestions.
//! LEVEL: intermediate
//! Use in: friendly error messages for an unknown command, option or key.   Run: cargo run --release --example dp_edit_distance
//! COMMON USES: "did you mean ...?" in command line tools and compilers, spell checking, record matching.
//!
//! The distance is the smallest number of single-character inserts, deletes and replacements that turn one text into the
//! other. Two rows of the table are enough, so memory grows with the length of `b` only. Time grows with the product of
//! the two lengths, so `suggest` refuses a very long word before it compares anything.
//! A character here is a Rust `char` (a Unicode scalar value). The code does not normalize: the same accented letter
//! written two ways counts as different. A swap of two neighbours counts as 2.
//! Read: https://en.wikipedia.org/wiki/Levenshtein_distance and https://github.com/rapidfuzz/strsim-rs

/// The longest word, in characters, that `suggest` will look at.
const MAX_WORD: usize = 64;

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    // previous[j] = distance from the part of `a` read so far to the first j characters of `b`
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0; b.len() + 1];
    for (i, from_a) in a.chars().enumerate() {
        current[0] = i + 1; // delete everything read so far
        for (j, &from_b) in b.iter().enumerate() {
            let replace = previous[j] + usize::from(from_a != from_b);
            let delete = previous[j + 1] + 1;
            let insert = current[j] + 1;
            current[j + 1] = replace.min(delete).min(insert);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

/// The closest known name, if one is close enough: at most a third of the typed length, and at least 1.
fn suggest<'a>(typed: &str, known: &[&'a str]) -> Option<&'a str> {
    // stop counting just past the limit, so a huge word costs almost nothing
    let length = typed.chars().take(MAX_WORD + 1).count();
    if length == 0 || length > MAX_WORD {
        return None;
    }
    let limit = (length / 3).max(1);
    known
        .iter()
        // the difference in length is a lower bound, so these need no table
        .filter(|name| length.abs_diff(name.chars().take(MAX_WORD + limit + 1).count()) <= limit)
        .map(|&name| (edit_distance(typed, name), name))
        .filter(|&(distance, name)| distance <= limit && distance < name.chars().count())
        // on equal distance the name that sorts first wins, whatever the order of the list
        .min()
        .map(|(_, name)| name)
}

/// An oracle with no table: breadth-first search over single edits from `a` until `b` turns up.
fn distance_by_search(a: &str, b: &str, alphabet: &[char]) -> usize {
    use std::collections::HashSet;
    let target: Vec<char> = b.chars().collect();
    let start: Vec<char> = a.chars().collect();
    let longest = start.len().max(target.len()); // a shortest route never needs to be longer
    let mut seen = HashSet::from([start.clone()]);
    let mut frontier = vec![start];
    for steps in 0.. {
        if frontier.contains(&target) {
            return steps;
        }
        let mut next = Vec::new();
        for word in &frontier {
            let mut add = |candidate: Vec<char>| {
                if candidate.len() <= longest && seen.insert(candidate.clone()) {
                    next.push(candidate);
                }
            };
            for at in 0..=word.len() {
                for &c in alphabet {
                    let mut inserted = word.clone();
                    inserted.insert(at, c);
                    add(inserted);
                    if at < word.len() {
                        let mut replaced = word.clone();
                        replaced[at] = c;
                        add(replaced);
                    }
                }
                if at < word.len() {
                    let mut deleted = word.clone();
                    deleted.remove(at);
                    add(deleted);
                }
            }
        }
        frontier = next;
    }
    unreachable!()
}

/// A small seeded random generator (xorshift), so every run tests the same inputs.
struct Rng(u64);
impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
    fn word(&mut self, alphabet: &[char], max_len: usize) -> String {
        (0..self.below(max_len + 1))
            .map(|_| alphabet[self.below(alphabet.len())])
            .collect()
    }
}

fn main() {
    for (a, b, expected) in [
        ("kitten", "sitting", 3),
        ("flaw", "lawn", 2),
        ("", "abc", 3),
        ("same", "same", 0),
        ("héllo", "hello", 1),
        ("form", "from", 2),             // a swap of neighbours is two edits here
        ("caf\u{e9}", "cafe\u{301}", 2), // the same word on screen, written two ways
    ] {
        println!("{a:?} -> {b:?}: {}", edit_distance(a, b));
        assert_eq!(edit_distance(a, b), expected);
        assert_eq!(edit_distance(b, a), expected);
    }

    let commands = ["build", "check", "clean", "publish", "update"];
    for typed in ["biuld", "chek", "pubilsh", "xyz", "Build"] {
        match suggest(typed, &commands) {
            Some(name) => println!("unknown command `{typed}`: did you mean `{name}`?"),
            None => println!("unknown command `{typed}`"),
        }
    }
    assert_eq!(suggest("chek", &commands), Some("check"));
    // five letters allow 1 edit, and a swap costs 2
    assert_eq!(suggest("biuld", &commands), None);
    assert_eq!(suggest("xyz", &commands), None);
    assert_eq!(suggest("", &commands), None);
    assert_eq!(
        suggest("x", &["a"]),
        None,
        "nothing in common is not a typo"
    );
    assert_eq!(suggest("buil", &["built", "build"]), Some("build"));
    assert_eq!(suggest("buil", &["build", "built"]), Some("build"));
    let huge = "b".repeat(10_000_000);
    assert_eq!(suggest(&huge, &commands), None);
    assert_eq!(suggest("build", &[huge.as_str()]), None);
    let long_name = "n".repeat(MAX_WORD + 1);
    assert_eq!(
        suggest(&long_name, &[long_name.as_str()]),
        None,
        "the cap on the typed word"
    );

    // every pair of short words against the search oracle
    let alphabet = ['a', 'b', 'é'];
    let mut words = vec![String::new()];
    let mut from = 0;
    for _ in 0..4 {
        let to = words.len();
        for i in from..to {
            for c in alphabet {
                let longer = format!("{}{c}", words[i]);
                words.push(longer);
            }
        }
        from = to;
    }
    for a in &words {
        for b in &words {
            assert_eq!(
                edit_distance(a, b),
                distance_by_search(a, b, &alphabet),
                "{a:?} {b:?}"
            );
        }
    }
    println!(
        "checked {} small pairs against a search",
        words.len() * words.len()
    );

    // longer random words: rules that every distance must follow
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let alphabet = ['a', 'b', 'c', 'é', '🦀'];
    for _ in 0..20_000 {
        let (a, b, c) = (
            rng.word(&alphabet, 20),
            rng.word(&alphabet, 20),
            rng.word(&alphabet, 20),
        );
        let (la, lb) = (a.chars().count(), b.chars().count());
        let ab = edit_distance(&a, &b);
        assert!(la.abs_diff(lb) <= ab && ab <= la.max(lb));
        assert_eq!(ab == 0, a == b);
        assert_eq!(ab, edit_distance(&b, &a));
        assert!(edit_distance(&a, &c) <= ab + edit_distance(&b, &c));

        // the shortcuts in `suggest` must not change its answer
        let known = [b.as_str(), c.as_str()];
        let limit = (la / 3).max(1);
        let plain = known
            .iter()
            .map(|&name| (edit_distance(&a, name), name))
            .filter(|&(d, name)| la > 0 && d <= limit && d < name.chars().count())
            .min()
            .map(|(_, name)| name);
        assert_eq!(suggest(&a, &known), plain, "{a:?} {known:?}");
    }
    println!("all checks passed");
}
