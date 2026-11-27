//! RECIPE 1207 (database): matching records from several sources that describe the same thing.
//! LEVEL: intermediate
//! Use in: library catalogues, product lists, importing data from partners.   Run: cargo run --release --example record_matching
//! COMMON USES: merging catalogues, cleaning imported lists, joining datasets that share no reliable key.
#![allow(dead_code)]
//!
//! Source records are never changed. The program makes canonical keys, picks candidate pairs by blocking,
//! scores each pair with named reasons, and groups the strong ones. Two records with different valid ISBNs,
//! or that a person marked as different, never share a group, however they are linked.
//! Groups are computed again from the records and a log of decisions, so a wrong merge is undone by adding to it.
//! A later entry about a pair replaces the earlier one; withdrawing a pair gives it back to the rules.
//! "Different" always holds. "Same" decisions apply newest first, and one that cannot hold is reported.
//! A valid ISBN check digit does not prove that the number is real or that it belongs to the record.
//! One thing here is a printed ISBN edition in one binding. The weights and limits are made up for the example:
//! tune real ones against pairs that people have labelled.
//! NOT COVERED: Unicode normalisation, transliteration, learned weights, updating groups one record at a time.
//! Read: https://en.wikipedia.org/wiki/ISBN

use std::collections::{BTreeMap, BTreeSet, VecDeque};

const MERGE_AT: i32 = 70;
const REVIEW_AT: i32 = 40;
const RULES: &str = "rules-v1";

struct Record {
    id: u32, // unique across all sources
    source: &'static str,
    title: String,
    author: String,
    year: Option<u16>,
    isbn: String, // as the source wrote it; may be empty or wrong
}

fn record(
    id: u32,
    source: &'static str,
    title: &str,
    author: &str,
    year: u16,
    isbn: &str,
) -> Record {
    Record {
        id,
        source,
        title: title.into(),
        author: author.into(),
        year: (year != 0).then_some(year),
        isbn: isbn.into(),
    }
}

// --- canonical keys ------------------------------------------------------------------------------

/// The 13th digit of an ISBN, from the first twelve.
fn check_digit_13(first: &[u32]) -> u32 {
    let sum: u32 = first
        .iter()
        .enumerate()
        .map(|(i, d)| d * if i % 2 == 0 { 1 } else { 3 })
        .sum();
    (10 - sum % 10) % 10
}

/// A well-formed ISBN-10 or ISBN-13 as 13 digits. Anything else gives no key at all.
/// A key can still be false: other made-up numbers pass the checksum too.
fn isbn13(raw: &str) -> Option<String> {
    if raw.len() > 32 {
        return None;
    }
    let chars: Vec<char> = raw.chars().filter(|c| !matches!(c, '-' | ' ')).collect();
    let digits: Vec<u32> = match chars.len() {
        10 => {
            let mut d: Vec<u32> = chars[..9]
                .iter()
                .map(|c| c.to_digit(10))
                .collect::<Option<_>>()?;
            // only the last place may be X, which stands for ten
            d.push(if matches!(chars[9], 'X' | 'x') {
                10
            } else {
                chars[9].to_digit(10)?
            });
            let sum: u32 = d.iter().zip((1..=10).rev()).map(|(d, w)| d * w).sum();
            if !sum.is_multiple_of(11) {
                return None;
            }
            let mut long = vec![9, 7, 8];
            long.extend(&d[..9]);
            long.push(check_digit_13(&long));
            long
        }
        13 => {
            let d: Vec<u32> = chars
                .iter()
                .map(|c| c.to_digit(10))
                .collect::<Option<_>>()?;
            if !(d.starts_with(&[9, 7, 8]) || d.starts_with(&[9, 7, 9]))
                || check_digit_13(&d[..12]) != d[12]
            {
                return None;
            }
            d
        }
        _ => return None,
    };
    // filler such as 0000000000 or 1111111111 passes the checksum but names no book
    if digits[3..12].iter().all(|&d| d == digits[3]) {
        return None;
    }
    Some(digits.iter().map(u32::to_string).collect())
}

/// Lowercase words without punctuation.
fn fold(text: &str) -> String {
    let spaced: String = text
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// "Le Guin, Ursula K." and "Ursula K. Le Guin" give the same key.
fn author_key(raw: &str) -> String {
    match raw.split_once(',') {
        Some((surname, rest)) => fold(&format!("{rest} {surname}")),
        None => fold(raw),
    }
}

struct Keys {
    id: u32,
    isbn: Option<String>,
    title: String,
    author: String,
    year: Option<u16>,
}

fn keys(r: &Record) -> Keys {
    Keys {
        id: r.id,
        isbn: isbn13(&r.isbn),
        title: fold(&r.title),
        author: author_key(&r.author),
        year: r.year,
    }
}

// --- candidate blocking --------------------------------------------------------------------------

/// Records are compared only if they share at least one of these.
fn blocking_keys(k: &Keys) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(isbn) = &k.isbn {
        out.push(format!("isbn:{isbn}"));
    }
    let word = k
        .title
        .split(' ')
        .find(|w| !matches!(*w, "the" | "a" | "an" | ""));
    // the last word of the author key is a rough surname
    let surname = k.author.rsplit(' ').next().filter(|w| !w.is_empty());
    if let (Some(word), Some(surname)) = (word, surname) {
        out.push(format!("name:{surname}|{word}"));
    }
    out
}

type Pair = (u32, u32); // smaller id first

fn pair(a: u32, b: u32) -> Pair {
    (a.min(b), a.max(b))
}

/// Candidate pairs, plus the blocks that were too big to compare. Report those: they hide matches.
fn candidates(all: &[Keys], largest_block: usize) -> (BTreeSet<Pair>, Vec<(String, usize)>) {
    let mut blocks: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    for k in all {
        for key in blocking_keys(k) {
            blocks.entry(key).or_default().push(k.id);
        }
    }
    let (mut pairs, mut skipped) = (BTreeSet::new(), Vec::new());
    for (key, ids) in blocks {
        if ids.len() > largest_block {
            skipped.push((key, ids.len()));
            continue;
        }
        for (i, &a) in ids.iter().enumerate() {
            pairs.extend(ids[i + 1..].iter().map(|&b| pair(a, b)));
        }
    }
    (pairs, skipped)
}

// --- scoring -------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct Score {
    total: i32,
    reasons: Vec<(&'static str, i32)>,
}

fn score(a: &Keys, b: &Keys) -> Score {
    let mut reasons = Vec::new();
    if a.isbn.is_some() && a.isbn == b.isbn {
        reasons.push(("same ISBN", 60));
    }
    let words = |t: &str| {
        t.split_whitespace()
            .map(str::to_string)
            .collect::<BTreeSet<_>>()
    };
    let (x, y) = (words(&a.title), words(&b.title));
    let union = x.union(&y).count();
    let shared = x.intersection(&y).count();
    // no words on either side: no points at all
    if let Some(points) = (40 * shared).checked_div(union) {
        reasons.push(("title words", points as i32));
    }
    if !a.author.is_empty() && !b.author.is_empty() {
        reasons.push(if a.author == b.author {
            ("same author", 25)
        } else {
            ("other author", -15)
        });
    }
    if let (Some(p), Some(q)) = (a.year, b.year) {
        reasons.push(if p == q {
            ("same year", 10)
        } else {
            ("other year", -10)
        });
    }
    Score {
        total: reasons.iter().map(|r| r.1).sum(),
        reasons,
    }
}

// --- decisions, grouping and provenance ----------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Verdict {
    Same,
    Different,
    /// Take back what was said about this pair and let the rules decide again.
    Withdrawn,
}

/// What a person said about two records. The log is in time order and is only ever added to.
#[derive(Debug, Clone)]
struct Decision {
    a: u32,
    b: u32,
    verdict: Verdict,
    by: String,
    why: String,
}

/// Why two records are linked: kept with the result.
#[derive(Debug, Clone, PartialEq)]
enum Basis {
    Rule { version: &'static str, score: Score },
    Person { by: String, why: String },
}

#[derive(Debug, Clone, PartialEq)]
struct Link {
    pair: Pair,
    basis: Basis,
}

#[derive(Debug, PartialEq)]
struct Resolution {
    groups: Vec<Vec<u32>>,
    links: Vec<Link>,           // used to build the groups
    refused: Vec<(Link, Pair)>, // wanted to merge, and the pair that forbids it
    review: Vec<Link>,          // too weak to merge, too strong to ignore
}

struct Facts<'a> {
    keys: BTreeMap<u32, &'a Keys>,
    // the decision in force for each pair, with its position in the log
    active: BTreeMap<Pair, (usize, &'a Decision)>,
}

impl<'a> Facts<'a> {
    fn new(all: &'a [Keys], decisions: &'a [Decision]) -> Self {
        let keys: BTreeMap<u32, &Keys> = all.iter().map(|k| (k.id, k)).collect();
        assert_eq!(keys.len(), all.len(), "record ids must be unique");
        let mut active = BTreeMap::new();
        for (position, d) in decisions.iter().enumerate() {
            assert!(
                d.a != d.b && keys.contains_key(&d.a) && keys.contains_key(&d.b),
                "a decision needs two different, known records"
            );
            if d.verdict == Verdict::Withdrawn {
                active.remove(&pair(d.a, d.b));
            } else {
                active.insert(pair(d.a, d.b), (position, d));
            }
        }
        Facts { keys, active }
    }

    /// May these two never be in one group?
    fn forbidden(&self, p: Pair) -> bool {
        match self.active.get(&p) {
            Some((_, d)) => d.verdict == Verdict::Different,
            None => {
                let (a, b) = (&self.keys[&p.0].isbn, &self.keys[&p.1].isbn);
                a.is_some() && b.is_some() && a != b
            }
        }
    }
}

/// Panics if any decision in the log, even a replaced one, is not about two different records in `all`.
fn resolve(all: &[Keys], pairs: &BTreeSet<Pair>, decisions: &[Decision]) -> Resolution {
    let facts = Facts::new(all, decisions);

    // people first, newest decision first; then the rules from the strongest score down
    let mut said_same: Vec<_> = facts
        .active
        .iter()
        .filter(|(_, (_, d))| d.verdict == Verdict::Same)
        .collect();
    said_same.sort_by_key(|(_, (position, _))| std::cmp::Reverse(*position));
    let mut wanted: Vec<Link> = said_same
        .into_iter()
        .map(|(&pair, (_, d))| Link {
            pair,
            basis: Basis::Person {
                by: d.by.clone(),
                why: d.why.clone(),
            },
        })
        .collect();
    let mut scored = Vec::new();
    let mut review = Vec::new();
    for &p in pairs {
        if facts.active.contains_key(&p) {
            continue;
        }
        let score = score(facts.keys[&p.0], facts.keys[&p.1]);
        let total = score.total;
        let link = Link {
            pair: p,
            basis: Basis::Rule {
                version: RULES,
                score,
            },
        };
        if total >= MERGE_AT {
            scored.push((total, link));
        } else if total >= REVIEW_AT && !facts.forbidden(p) {
            review.push(link);
        }
    }
    // `pairs` is an ordered set and this sort keeps equal scores in that order, so ties are
    // settled by the ids, never by the order the records arrived in
    scored.sort_by_key(|(total, _)| -total);
    wanted.extend(scored.into_iter().map(|(_, link)| link));

    let mut group_of: BTreeMap<u32, u32> = facts.keys.keys().map(|&id| (id, id)).collect();
    let mut members: BTreeMap<u32, Vec<u32>> =
        facts.keys.keys().map(|&id| (id, vec![id])).collect();
    let (mut links, mut refused) = (Vec::new(), Vec::new());
    for link in wanted {
        let (ga, gb) = (group_of[&link.pair.0], group_of[&link.pair.1]);
        if ga == gb {
            links.push(link); // more support for a group that already exists
            continue;
        }
        // check every pair the merge would bring together, not only the two linked records
        let blocker = members[&ga]
            .iter()
            .flat_map(|&x| members[&gb].iter().map(move |&y| pair(x, y)))
            .find(|&p| facts.forbidden(p));
        if let Some(blocker) = blocker {
            refused.push((link, blocker));
            continue;
        }
        let moved = members.remove(&gb).unwrap_or_default();
        for id in &moved {
            group_of.insert(*id, ga);
        }
        members.entry(ga).or_default().extend(moved);
        links.push(link);
    }
    let mut groups: Vec<Vec<u32>> = members.into_values().collect();
    for g in &mut groups {
        g.sort();
    }
    groups.sort();
    Resolution {
        groups,
        links,
        refused,
        review,
    }
}

/// A shortest chain of links from one record to another: the answer to "why are these one thing?"
fn why(links: &[Link], from: u32, to: u32) -> Option<Vec<&Link>> {
    let mut came_by: BTreeMap<u32, (u32, &Link)> = BTreeMap::new();
    let mut queue = VecDeque::from([from]);
    while let Some(here) = queue.pop_front() {
        if here == to {
            let (mut path, mut at) = (Vec::new(), to);
            while at != from {
                let (previous, link) = came_by[&at];
                path.push(link);
                at = previous;
            }
            path.reverse();
            return Some(path);
        }
        for link in links {
            let (a, b) = link.pair;
            let next = if a == here {
                b
            } else if b == here {
                a
            } else {
                continue;
            };
            if next != from && !came_by.contains_key(&next) {
                came_by.insert(next, (here, link));
                queue.push_back(next);
            }
        }
    }
    None
}

fn all_pairs(all: &[Keys]) -> BTreeSet<Pair> {
    let mut out = BTreeSet::new();
    for (i, a) in all.iter().enumerate() {
        out.extend(all[i + 1..].iter().map(|b| pair(a.id, b.id)));
    }
    out
}

/// Pairs that comparing everything would merge or send to review, but blocking never compares.
fn missed_by_blocking(all: &[Keys], pairs: &BTreeSet<Pair>) -> Vec<Pair> {
    let by_id: BTreeMap<u32, &Keys> = all.iter().map(|k| (k.id, k)).collect();
    all_pairs(all)
        .difference(pairs)
        .copied()
        .filter(|p| score(by_id[&p.0], by_id[&p.1]).total >= REVIEW_AT)
        .collect()
}

// --- checks --------------------------------------------------------------------------------------

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

fn check_isbn() {
    assert_eq!(isbn13("0-306-40615-2").as_deref(), Some("9780306406157"));
    assert_eq!(
        isbn13("978-0-306-40615-7").as_deref(),
        Some("9780306406157")
    );
    assert!(isbn13("080442957X").is_some());
    for bad in [
        "",
        "0-306-40615-3",
        "9780306406158",
        "X306406152",
        "1234567890128",
        "９７８０３０６４０６１５７",
        "978030640615é",
        "0000000000",
        "0-000-00000-0",
        "1111111111",
        "9999999999",
        "978-1-11111111-3",
        "----------",
    ] {
        assert_eq!(isbn13(bad), None, "{bad:?}");
    }
    assert_eq!(isbn13(&"-".repeat(1_000_000)), None);

    let mut rng = Rng(11);
    for _ in 0..2_000 {
        let mut d: Vec<u32> = (0..12).map(|_| rng.below(10) as u32).collect();
        d[..3].copy_from_slice(&[9, 7, 8]);
        d.push(check_digit_13(&d));
        let good: String = d.iter().map(u32::to_string).collect();
        assert_eq!(isbn13(&good).as_ref(), Some(&good));
        // changing any single digit must break it
        let at = rng.below(13);
        d[at] = (d[at] + 1 + rng.below(9) as u32) % 10;
        let bad: String = d.iter().map(u32::to_string).collect();
        assert_eq!(isbn13(&bad), None, "{good} -> {bad}");
    }
    // random text must never panic
    let alphabet = ["0", "1", "9", "7", "8", "X", "-", " ", "é", "😀", "\0"];
    for _ in 0..20_000 {
        let text: String = (0..rng.below(20)).map(|_| rng.pick(&alphabet)).collect();
        let _ = (isbn13(&text), fold(&text), author_key(&text));
    }
}

fn random_keys(rng: &mut Rng) -> Vec<Keys> {
    let isbns = ["", "", "9780306406157", "080442957X", "not an isbn"];
    let titles = [
        "Night Garden",
        "The Night Garden",
        "Night Gardens",
        "Salt Road",
        "",
    ];
    let authors = ["Ada Moss", "Moss, Ada", "Ben Reed", ""];
    (0..2 + rng.below(9) as u32)
        .map(|id| {
            let year = [0, 1990, 1991][rng.below(3)];
            keys(&record(
                id,
                "random",
                rng.pick(&titles),
                rng.pick(&authors),
                year,
                rng.pick(&isbns),
            ))
        })
        .collect()
}

fn check_random_catalogues() {
    let mut rng = Rng(0xCAB1E);
    for _ in 0..3_000 {
        let mut all = random_keys(&mut rng);
        let ids: Vec<u32> = all.iter().map(|k| k.id).collect();
        let decisions: Vec<Decision> = (0..rng.below(6))
            .map(|_| {
                let (a, b) = random_pair(&mut rng, &ids);
                let verdict = [Verdict::Same, Verdict::Different, Verdict::Withdrawn][rng.below(3)];
                decision(a, b, verdict, "test")
            })
            .collect();

        // blocking against the plain definition: a pair is a candidate if the two share a key
        let (pairs, skipped) = candidates(&all, usize::MAX);
        assert!(skipped.is_empty());
        let shares = |p: &Pair| {
            let a = blocking_keys(&all[p.0 as usize]);
            blocking_keys(&all[p.1 as usize])
                .iter()
                .any(|k| a.contains(k))
        };
        assert_eq!(pairs, all_pairs(&all).into_iter().filter(shares).collect());

        let result = resolve(&all, &pairs, &decisions);
        let facts = Facts::new(&all, &decisions);
        let together = |p: Pair| {
            result
                .groups
                .iter()
                .any(|g| g.contains(&p.0) && g.contains(&p.1))
        };
        for p in all_pairs(&all) {
            assert!(
                !(together(p) && facts.forbidden(p)),
                "{p:?} must not share a group"
            );
            // every grouped pair can be explained, and no other pair can
            assert_eq!(why(&result.links, p.0, p.1).is_some(), together(p), "{p:?}");
        }
        // nothing wanted was left apart without a reason
        for (link, blocker) in &result.refused {
            assert!(facts.forbidden(*blocker) && !together(link.pair));
        }
        for &p in &pairs {
            if !facts.active.contains_key(&p)
                && score(facts.keys[&p.0], facts.keys[&p.1]).total >= MERGE_AT
            {
                assert!(together(p) || result.refused.iter().any(|(l, _)| l.pair == p));
            }
        }
        // a decision that is then withdrawn leaves the same result as never having made it
        let (a, b) = random_pair(&mut rng, &ids);
        let mut without_pair = decisions.clone();
        without_pair.push(decision(a, b, Verdict::Withdrawn, "test"));
        let mut undone = without_pair.clone();
        let verdict = [Verdict::Same, Verdict::Different][rng.below(2)];
        undone.push(decision(a, b, verdict, "test"));
        undone.push(decision(a, b, Verdict::Withdrawn, "test"));
        let restored = resolve(&all, &pairs, &undone);
        assert_eq!(restored, resolve(&all, &pairs, &without_pair));
        if !facts.active.contains_key(&pair(a, b)) {
            assert_eq!(restored, result);
        }

        // the order in which records arrive must not matter, from blocking onwards
        for i in (1..all.len()).rev() {
            all.swap(i, rng.below(i + 1));
        }
        let (again, _) = candidates(&all, usize::MAX);
        assert_eq!(again, pairs);
        assert_eq!(resolve(&all, &again, &decisions), result);
        // the same with a block limit, where some blocks are skipped
        let limited = candidates(&all, 3);
        all.sort_by_key(|k| k.id);
        assert_eq!(candidates(&all, 3), limited);
    }
}

fn decision(a: u32, b: u32, verdict: Verdict, why: &str) -> Decision {
    Decision {
        a,
        b,
        verdict,
        by: "librarian R".into(),
        why: why.into(),
    }
}

fn random_pair(rng: &mut Rng, ids: &[u32]) -> Pair {
    let i = rng.below(ids.len());
    let j = (i + 1 + rng.below(ids.len() - 1)) % ids.len();
    (ids[i], ids[j])
}

/// One small case for each rule of the matching, so that dropping a rule fails a named check.
fn check_rules() {
    const X: &str = "978-0-306-40615-7";
    const Y: &str = "0-8044-2957-X";
    let run = |records: &[Record], decisions: &[Decision]| {
        let all: Vec<Keys> = records.iter().map(keys).collect();
        let (pairs, _) = candidates(&all, 50);
        resolve(&all, &pairs, decisions)
    };
    let book = |id, title, year, isbn| record(id, "test", title, "Ada Moss", year, isbn);
    let same = |a, b| decision(a, b, Verdict::Same, "");
    let different = |a, b| decision(a, b, Verdict::Different, "");

    // the stronger of two links that cannot both hold wins, whatever the ids are
    let long = "salt road north of the old stone mill";
    let short = "salt road north of the old stone";
    let three = [
        book(1, long, 1990, X),
        book(2, short, 1990, Y),
        book(3, short, 1990, ""),
    ];
    let r = run(&three, &[]);
    assert_eq!(r.groups, [vec![1], vec![2, 3]], "strongest score first");

    // a person's "same" is applied before any rule, even a stronger one
    let r = run(&three, &[same(1, 3)]);
    assert_eq!(r.groups, [vec![1, 3], vec![2]], "people before rules");

    // two "same" decisions that cannot both hold: the newer one wins, and the other is reported
    for (older, newer, groups) in [
        ((1, 3), (2, 3), [vec![1], vec![2, 3]]),
        ((2, 3), (1, 3), [vec![1, 3], vec![2]]),
    ] {
        let log = [same(older.0, older.1), same(newer.0, newer.1)];
        let r = run(&three, &log);
        assert_eq!(r.groups, groups, "newest decision first");
        assert!(
            r.refused
                .iter()
                .any(|(l, _)| l.pair == older && matches!(l.basis, Basis::Person { .. })),
            "the decision that cannot hold is reported"
        );
    }

    // "same" overrules the ISBN rule for its pair, and withdrawing it brings the rule back
    let two = [
        book(1, "Night Garden", 1990, X),
        book(2, "Night Garden", 1990, Y),
    ];
    let mut log = vec![same(1, 2)];
    let r = run(&two, &log);
    assert_eq!(r.groups, [vec![1, 2]], "same overrules the ISBN rule");
    assert_eq!(r.links.len(), 1, "a decided pair is not scored again");
    log.push(decision(1, 2, Verdict::Withdrawn, ""));
    assert_eq!(run(&two, &log), run(&two, &[]), "withdraw restores");

    // a pair that may never merge is not offered for review (score 65: no year)
    let r = run(
        &[book(1, "Night Garden", 0, X), book(2, "Night Garden", 0, Y)],
        &[],
    );
    assert!(r.review.is_empty(), "forbidden pairs are not reviewed");

    // a pair that a person decided is neither reviewed nor reported as a refused rule
    let plain = [
        book(1, "Night Garden", 1990, ""),
        book(2, "Night Garden", 1990, ""),
    ];
    let r = run(&plain, &[different(1, 2)]);
    assert!(
        r.links.is_empty() && r.refused.is_empty() && r.review.is_empty(),
        "decided pairs are skipped"
    );

    // a different author takes points away: 40 + 10 - 15 = 35, below review
    let other = [
        record(1, "test", "Night Garden", "Ada Moss", 1990, ""),
        record(2, "test", "Night Garden", "Ben Moss", 1990, ""),
    ];
    let r = run(&other, &[]);
    assert!(r.links.is_empty() && r.review.is_empty(), "other author");
    assert_eq!(score(&keys(&other[0]), &keys(&other[1])).total, 35);

    // a decision about one record twice, or about an unknown record, is refused loudly
    // the expected panics are not printed; a failed check below still is
    let print_panics = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut accepted = Vec::new();
    for (a, b) in [(1, 1), (1, 99), (99, 1), (98, 99)] {
        for verdict in [Verdict::Same, Verdict::Different, Verdict::Withdrawn] {
            let bad = [decision(a, b, verdict, "")];
            if std::panic::catch_unwind(|| run(&plain, &bad)).is_ok() {
                accepted.push((a, b, verdict));
            }
        }
    }
    std::panic::set_hook(print_panics);
    assert!(
        accepted.is_empty(),
        "bad decisions were accepted: {accepted:?}"
    );

    // filler numbers are no ISBN, so they cannot join two different books
    let filler = [
        book(1, "Salt Road", 1990, "0000000000"),
        book(2, "Night Garden", 1971, "0-000-00000-0"),
    ];
    let k: Vec<Keys> = filler.iter().map(keys).collect();
    assert_eq!(resolve(&k, &all_pairs(&k), &[]).groups, [vec![1], vec![2]]);
}

fn main() {
    // The ISBNs here are invented; only their check digits are right.
    let records = [
        record(
            1,
            "city",
            "A Wizard of Earthsea",
            "Le Guin, Ursula K.",
            1968,
            "978-1-4028-9462-6",
        ),
        record(
            2,
            "union",
            "A wizard of Earthsea.",
            "Ursula K. Le Guin",
            1968,
            "1-4028-9462-7",
        ),
        record(
            3,
            "donated",
            "A Wizard of Earthsea",
            "Ursula K. Le Guin",
            1968,
            "",
        ),
        record(
            4,
            "union",
            "A Wizard of Earthsea",
            "Ursula K. Le Guin",
            1968,
            "978-3-16-148410-0",
        ),
        record(
            5,
            "donated",
            "A Wizzard of Earthsea",
            "Ursula K. Le Guin",
            1968,
            "",
        ),
    ];
    let all: Vec<Keys> = records.iter().map(keys).collect();
    for k in &all {
        println!(
            "{}: isbn={:?} title={:?} author={:?}",
            k.id, k.isbn, k.title, k.author
        );
    }
    let (pairs, skipped) = candidates(&all, 50);
    println!(
        "candidates: {} of {} pairs",
        pairs.len(),
        all_pairs(&all).len()
    );
    assert!(skipped.is_empty());

    let first = resolve(&all, &pairs, &[]);
    println!("groups: {:?}", first.groups);
    for (link, blocker) in &first.refused {
        println!("refused {:?}: it would put {blocker:?} together", link.pair);
    }
    let missed = missed_by_blocking(&all, &pairs);
    println!("missed by blocking: {missed:?}");
    assert_eq!(first.groups, [vec![1, 2, 3], vec![4], vec![5]]);

    // a librarian has the donated copy in hand: it is the printing in record 4
    let mut decisions = vec![decision(
        1,
        3,
        Verdict::Different,
        "other cover and page count",
    )];
    let after_split = resolve(&all, &pairs, &decisions);
    println!("after one 'different': {:?}", after_split.groups);
    // one 'different' keeps 3 away from record 1 and from anything grouped with record 1
    assert_eq!(after_split.groups, [vec![1, 2], vec![3, 4], vec![5]]);
    for link in why(&after_split.links, 3, 4).expect("3 and 4 share a group") {
        println!("why {:?}: {:?}", link.pair, link.basis);
    }
    // that was a mistake: take the decision back, and the earlier groups return
    decisions.push(decision(
        1,
        3,
        Verdict::Withdrawn,
        "looked at the wrong copy",
    ));
    assert_eq!(resolve(&all, &pairs, &decisions), first);

    // a block over the limit is reported, not silently dropped
    let (_, skipped) = candidates(&all, 2);
    assert!(!skipped.is_empty());

    check_isbn();
    check_rules();
    check_random_catalogues();
    println!("all checks passed");
}
