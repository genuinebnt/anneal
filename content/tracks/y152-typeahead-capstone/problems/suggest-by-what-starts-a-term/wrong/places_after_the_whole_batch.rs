use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

/// A play counts half after this many seconds.
pub const HALF_LIFE: u64 = 3600;
/// No question is answered with more suggestions than this.
pub const MOST: usize = 20;

#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    /// The term as it was first spelled, with the whitespace around it removed.
    pub term: String,
    /// Its decayed popularity at the time of the question.
    pub score: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Added {
    /// Pairs that created a term.
    pub added: usize,
    /// Pairs for a term that was already there.
    pub existing: usize,
}

struct Term {
    shown: String,
    key: String,
    /// log2 of the sum of `count * 2^(at / HALF_LIFE)`; minus infinity while nothing was counted.
    weight: f64,
}

#[derive(Default)]
struct Node {
    next: BTreeMap<char, usize>,
    /// The `MOST` best terms under this beginning, best first.
    top: Vec<usize>,
}

/// log2(2^a + 2^b), without forming either power.
fn log_add(a: f64, b: f64) -> f64 {
    let (high, low) = if a >= b { (a, b) } else { (b, a) };
    if low == f64::NEG_INFINITY {
        return high;
    }
    high + (low - high).exp2().ln_1p() / std::f64::consts::LN_2
}

fn rank(terms: &[Term], a: usize, b: usize) -> Ordering {
    terms[b]
        .weight
        .total_cmp(&terms[a].weight)
        .then_with(|| terms[a].key.cmp(&terms[b].key))
}

fn score(weight: f64, now: u64) -> f64 {
    (weight - now as f64 / HALF_LIFE as f64).exp2()
}

pub struct Terms {
    terms: Vec<Term>,
    numbers: HashMap<String, usize>,
    nodes: Vec<Node>,
}

impl Terms {
    pub fn new() -> Terms {
        Terms {
            terms: Vec::new(),
            numbers: HashMap::new(),
            nodes: vec![Node::default()],
        }
    }

    /// The number of terms held.
    pub fn len(&self) -> usize {
        self.terms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn count(&mut self, number: usize, count: u32, now: u64) {
        if count > 0 {
            let new = f64::from(count).log2() + now as f64 / HALF_LIFE as f64;
            let term = &mut self.terms[number];
            term.weight = log_add(term.weight, new);
        }
    }

    /// Puts one term, and only that one, back in its place in every list on its path. It must run before another
    /// term changes: binary search needs the rest of each list to be in order.
    fn place(&mut self, number: usize) {
        let mut at = 0;
        let mut letters = self.terms[number].key.chars();
        loop {
            let top = &mut self.nodes[at].top;
            if let Some(old) = top.iter().position(|&n| n == number) {
                top.remove(old);
            }
            let terms = &self.terms;
            let to = top.partition_point(|&other| rank(terms, other, number) == Ordering::Less);
            top.insert(to, number);
            top.truncate(MOST);
            let Some(c) = letters.next() else { return };
            at = match self.nodes[at].next.get(&c) {
                Some(&child) => child,
                None => {
                    let child = self.nodes.len();
                    self.nodes.push(Node::default());
                    self.nodes[at].next.insert(c, child);
                    child
                }
            };
        }
    }

    /// Adds `count` choices of each term, in order, all at time `now`.
    pub fn add(&mut self, batch: &[(&str, u32)], now: u64) -> Added {
        let mut report = Added {
            added: 0,
            existing: 0,
        };
        let mut moved = Vec::new();
        for &(text, count) in batch {
            let key = search_key(text);
            if key.is_empty() {
                continue;
            }
            let number = match self.numbers.get(&key) {
                Some(&number) => {
                    report.existing += 1;
                    number
                }
                None => {
                    report.added += 1;
                    let number = self.terms.len();
                    self.numbers.insert(key.clone(), number);
                    self.terms.push(Term {
                        shown: text.trim().to_string(),
                        key,
                        weight: f64::NEG_INFINITY,
                    });
                    number
                }
            };
            self.count(number, count, now);
            moved.push(number);
        }
        for number in moved {
            self.place(number);
        }
        report
    }

    /// One user chose this term at time `now`. False when there is no such term.
    pub fn choose(&mut self, term: &str, now: u64) -> bool {
        let Some(&number) = self.numbers.get(&search_key(term)) else {
            return false;
        };
        self.count(number, 1, now);
        self.place(number);
        true
    }

    fn beginning(&self, key: &str) -> &[usize] {
        let mut at = 0;
        for c in key.chars() {
            match self.nodes[at].next.get(&c) {
                Some(&child) => at = child,
                None => return &[],
            }
        }
        &self.nodes[at].top
    }

    /// The best terms whose key starts with the key of `typed`.
    pub fn suggest(&self, typed: &str, limit: usize, now: u64) -> Vec<Suggestion> {
        self.beginning(&search_key(typed))
            .iter()
            .take(limit)
            .map(|&n| Suggestion {
                term: self.terms[n].shown.clone(),
                score: score(self.terms[n].weight, now),
            })
            .collect()
    }
}

// ---- given: the search key (from "Light up what the user typed") ----

/// What a lowercase letter is replaced with in a key. `None` means: keep it. This table is deliberately small.
fn plain(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => "a",
        'ç' | 'ć' | 'č' => "c",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ě' => "e",
        'ì' | 'í' | 'î' | 'ï' => "i",
        'ñ' | 'ń' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => "o",
        'ù' | 'ú' | 'û' | 'ü' | 'ū' => "u",
        'ý' | 'ÿ' => "y",
        'ß' => "ss",
        'æ' => "ae",
        'œ' => "oe",
        'ł' => "l",
        'š' | 'ś' => "s",
        'ž' | 'ź' | 'ż' => "z",
        _ => return None,
    })
}

fn is_mark(c: char) -> bool {
    ('\u{300}'..='\u{36f}').contains(&c)
}

/// Adds one character of the text to a key that may still end in a space.
fn push(key: &mut String, c: char) {
    if c.is_whitespace() {
        if !key.is_empty() && !key.ends_with(' ') {
            key.push(' ');
        }
    } else if !c.is_control() {
        for low in c.to_lowercase().filter(|&l| !is_mark(l)) {
            match plain(low) {
                Some(letters) => key.push_str(letters),
                None => key.push(low),
            }
        }
    }
}

/// The form of `text` that is used for matching.
pub fn search_key(text: &str) -> String {
    let mut key = String::new();
    for c in text.chars() {
        push(&mut key, c);
    }
    if key.ends_with(' ') {
        key.pop();
    }
    key
}
