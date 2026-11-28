use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, RwLock};

/// A play counts half after this many seconds.
pub const HALF_LIFE: u64 = 3600;
/// No question is answered with more suggestions than this.
pub const MOST: usize = 20;

/// How a term fits what was typed. The order of the variants is not the order of the answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Prefix,
    Infix,
    Typo,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    /// The term as it was first spelled, with the whitespace around it removed.
    pub term: String,
    /// Its decayed popularity at the time of the question.
    pub score: f64,
    pub kind: Kind,
    /// The number of slips; 0 unless `kind` is `Typo`.
    pub dist: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Added {
    pub added: usize,
    pub existing: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    /// True when the answer came from the cache.
    pub cached: bool,
    pub items: Vec<Suggestion>,
}

/// Decides how a term fits what was typed: `judge(typed_key, term_key)`.
pub type Judge = Box<dyn Fn(&str, &str) -> Option<(Kind, u8)> + Send + Sync>;

struct Stored {
    items: Vec<(usize, Kind, u8)>,
}

pub struct Service {
    judge: Judge,
    /// Readers hold this for reading from the cache lookup until their answer is stored. A writer holds it for
    /// writing from the first change until the cache is clean. So no answer can be stored late.
    index: RwLock<Index>,
    /// Locked only for short steps, and only while `index` is already held.
    cache: Mutex<Recent<(String, usize), std::sync::Arc<Stored>>>,
    /// The hook of "For the tests" in the statement.
    searched: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>>,
}

impl Service {
    /// `capacity` is the number of answers the cache may hold.
    pub fn new(capacity: usize, judge: Judge) -> Service {
        Service {
            judge,
            index: RwLock::new(Index::new()),
            cache: Mutex::new(Recent::new(capacity)),
            searched: std::sync::OnceLock::new(),
        }
    }

    /// The number of terms held.
    pub fn len(&self) -> usize {
        self.index.read().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn suggest(&self, typed: &str, limit: usize, now: u64) -> Answer {
        let key = (search_key(typed), limit.min(MOST));
        let index = self.index.read().unwrap();
        let hit = self.cache.lock().unwrap().get(&key);
        let cached = hit.is_some();
        let stored = hit.unwrap_or_else(|| {
            let items = index.find(&key.0, key.1, &mut |term| (self.judge)(&key.0, term));
            // the search has ended and nothing is stored yet
            if let Some(hook) = self.searched.get() {
                hook();
            }
            let stored = std::sync::Arc::new(Stored { items });
            self.cache.lock().unwrap().put(key.clone(), stored.clone());
            stored
        });
        let items = stored
            .items
            .iter()
            .map(|&(n, kind, dist)| Suggestion {
                term: index.terms[n].shown.clone(),
                score: score(index.terms[n].weight, now),
                kind,
                dist,
            })
            .collect();
        Answer { cached, items }
    }

    /// Drops the stored answers that one of the changed terms can alter: those whose typed key it matches. That
    /// includes answers which do not list it yet.
    fn forget(&self, changed: &[String], names: &[String]) {
        if changed.is_empty() {
            return;
        }
        self.cache.lock().unwrap().retain_values(|stored| {
            !stored
                .items
                .iter()
                .any(|&(n, _, _)| changed.iter().any(|t| names[n] == *t))
        });
    }

    /// For the tests: see "For the tests" in the statement. Only the first hook is kept.
    pub fn on_searched(&self, hook: impl Fn() + Send + Sync + 'static) {
        let _ = self.searched.set(Box::new(hook));
    }

    pub fn add(&self, batch: &[(&str, u32)], now: u64) -> Added {
        let mut index = self.index.write().unwrap();
        let report = index.add(batch, now);
        let changed: Vec<String> = batch
            .iter()
            .map(|(text, _)| search_key(text))
            .filter(|key| !key.is_empty())
            .collect();
        let names: Vec<String> = index.terms.iter().map(|t| t.key.clone()).collect();
        self.forget(&changed, &names);
        report
    }

    pub fn choose(&self, term: &str, now: u64) -> bool {
        let mut index = self.index.write().unwrap();
        let known = index.choose(term, now);
        if known {
            let names: Vec<String> = index.terms.iter().map(|t| t.key.clone()).collect();
            self.forget(&[search_key(term)], &names);
        }
        known
    }
}

// ---- given: a small cache that forgets what was used longest ago ----

/// Not the fastest possible (see "An O(1) LRU cache without Rc or unsafe"): every use gets a rising number, and a
/// second map finds the lowest one.
struct Recent<K, V> {
    capacity: usize,
    clock: u64,
    values: HashMap<K, (u64, V)>,
    ages: BTreeMap<u64, K>,
}

impl<K: Clone + Eq + std::hash::Hash, V: Clone> Recent<K, V> {
    fn new(capacity: usize) -> Self {
        Recent {
            capacity,
            clock: 0,
            values: HashMap::new(),
            ages: BTreeMap::new(),
        }
    }

    /// The value, which now counts as just used.
    fn get(&mut self, key: &K) -> Option<V> {
        let (age, value) = self.values.get_mut(key)?;
        self.ages.remove(age);
        self.clock += 1;
        *age = self.clock;
        self.ages.insert(self.clock, key.clone());
        Some(value.clone())
    }

    fn put(&mut self, key: K, value: V) {
        if self.capacity == 0 {
            return;
        }
        if let Some((age, _)) = self.values.remove(&key) {
            self.ages.remove(&age);
        } else if self.values.len() == self.capacity {
            let oldest = self.ages.pop_first().map(|(_, key)| key);
            if let Some(oldest) = oldest {
                self.values.remove(&oldest);
            }
        }
        self.clock += 1;
        self.ages.insert(self.clock, key.clone());
        self.values.insert(key, (self.clock, value));
    }

    fn retain_values(&mut self, mut keep: impl FnMut(&V) -> bool) {
        self.values.retain(|_, (age, value)| {
            let stays = keep(value);
            if !stays {
                self.ages.remove(age);
            }
            stays
        });
    }

    /// Keeps only the entries whose key passes.
    fn retain(&mut self, mut keep: impl FnMut(&K) -> bool) {
        self.values.retain(|key, (age, _)| {
            let stays = keep(key);
            if !stays {
                self.ages.remove(age);
            }
            stays
        });
    }
}

// ---- given: the reference index of steps 1 and 2. Change it as you need. ----

/// Three neighbouring places of a key.
type Window = [Option<char>; 3];

fn distinct(mut windows: Vec<Window>) -> Vec<Window> {
    windows.sort_unstable();
    windows.dedup();
    windows
}

fn inner_windows(key: &[char]) -> Vec<Window> {
    distinct(
        key.windows(3)
            .map(|w| [Some(w[0]), Some(w[1]), Some(w[2])])
            .collect(),
    )
}

/// The windows of the key with two empty places added at each end.
fn padded_windows(key: &[char]) -> Vec<Window> {
    let mut places = vec![None, None];
    places.extend(key.iter().map(|&c| Some(c)));
    places.extend([None, None]);
    distinct(places.windows(3).map(|w| [w[0], w[1], w[2]]).collect())
}

fn allowed_slips(length: usize) -> usize {
    if length <= 5 {
        1
    } else {
        2
    }
}

/// How many padded windows a typo match must share.
fn needed(typed: &[char]) -> usize {
    padded_windows(typed)
        .len()
        .saturating_sub(4 * allowed_slips(typed.len()))
        .max(2)
}

/// The matching rule. Both arguments are search keys.
pub fn classify(typed_key: &str, term_key: &str) -> Option<(Kind, u8)> {
    if term_key.starts_with(typed_key) {
        return Some((Kind::Prefix, 0));
    }
    let typed: Vec<char> = typed_key.chars().collect();
    if typed.len() < 3 {
        return None;
    }
    if term_key.contains(typed_key) {
        return Some((Kind::Infix, 0));
    }
    let term: Vec<char> = term_key.chars().collect();
    let most = allowed_slips(typed.len());
    let whole = slips(&typed, &term, most);
    let start = (term.len() > typed.len())
        .then(|| slips(&typed, &term[..typed.len()], most))
        .flatten();
    let dist = whole.into_iter().chain(start).min()?;
    let theirs = padded_windows(&term);
    let shared = padded_windows(&typed)
        .iter()
        .filter(|w| theirs.binary_search(w).is_ok())
        .count();
    (shared >= needed(&typed)).then_some((Kind::Typo, dist as u8))
}

struct Term {
    shown: String,
    key: String,
    /// log2 of the sum of `count * 2^(at / HALF_LIFE)`; minus infinity while nothing was counted.
    weight: f64,
}

#[derive(Default, Clone)]
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

struct Index {
    /// For each padded window, the numbers of the terms that have it, rising.
    holders: HashMap<Window, Vec<usize>>,
    terms: Vec<Term>,
    numbers: HashMap<String, usize>,
    nodes: Vec<Node>,
}

impl Index {
    fn new() -> Index {
        Index {
            holders: HashMap::new(),
            terms: Vec::new(),
            numbers: HashMap::new(),
            nodes: vec![Node::default()],
        }
    }

    /// The number of terms held.
    fn len(&self) -> usize {
        self.terms.len()
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
    fn add(&mut self, batch: &[(&str, u32)], now: u64) -> Added {
        let mut report = Added {
            added: 0,
            existing: 0,
        };
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
                    let letters: Vec<char> = key.chars().collect();
                    for window in padded_windows(&letters) {
                        self.holders.entry(window).or_default().push(number);
                    }
                    self.terms.push(Term {
                        shown: text.trim().to_string(),
                        key,
                        weight: f64::NEG_INFINITY,
                    });
                    number
                }
            };
            self.count(number, count, now);
            self.place(number);
        }
        report
    }

    /// One user chose this term at time `now`. False when there is no such term.
    fn choose(&mut self, term: &str, now: u64) -> bool {
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

    /// The terms worth judging: those with every inner window of the typed key, and those that share enough
    /// padded windows. The lists are in rising term numbers, so a binary search looks one up.
    fn candidates(&self, typed: &[char]) -> Vec<usize> {
        let empty = Vec::new();
        let list = |w: &Window| self.holders.get(w).unwrap_or(&empty);
        let mut shared: HashMap<usize, usize> = HashMap::new();
        for window in padded_windows(typed) {
            for &number in list(&window) {
                *shared.entry(number).or_insert(0) += 1;
            }
        }
        let need = needed(typed);
        let mut found: Vec<usize> = shared
            .into_iter()
            .filter(|&(_, count)| count >= need)
            .map(|(number, _)| number)
            .collect();
        let mut inner: Vec<&Vec<usize>> = inner_windows(typed).iter().map(list).collect();
        inner.sort_by_key(|l| l.len());
        if let Some((shortest, others)) = inner.split_first() {
            found.extend(
                shortest
                    .iter()
                    .filter(|n| others.iter().all(|l| l.binary_search(n).is_ok())),
            );
        }
        found.sort_unstable();
        found.dedup();
        found
    }

    /// The numbers of the matching terms with how they match, in answer order.
    fn find(
        &self,
        key: &str,
        limit: usize,
        judge: &mut dyn FnMut(&str) -> Option<(Kind, u8)>,
    ) -> Vec<(usize, Kind, u8)> {
        let limit = limit.min(MOST);
        let beginning = self.beginning(key);
        let mut found: Vec<(usize, Kind, u8)> = beginning
            .iter()
            .take(limit)
            .map(|&n| (n, Kind::Prefix, 0))
            .collect();
        let typed: Vec<char> = key.chars().collect();
        if found.len() >= limit || typed.len() < 3 {
            return found;
        }
        // the beginning's list was not full, so it holds every term that starts with the key
        let mut others: Vec<(usize, Kind, u8)> = self
            .candidates(&typed)
            .into_iter()
            .filter(|&n| !self.terms[n].key.starts_with(key))
            .filter_map(|n| judge(&self.terms[n].key).map(|(kind, dist)| (n, kind, dist)))
            .collect();
        others.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| rank(&self.terms, a.0, b.0)));
        others.truncate(limit - found.len());
        found.extend(others);
        found
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

// ---- given: slips between two texts (from "How many slips of the finger?") ----

/// The number of slips from `a` to `b`, when it is at most `max`.
fn slips<T: PartialEq>(a: &[T], b: &[T], max: usize) -> Option<usize> {
    let (n, m) = (a.len(), b.len());
    if n.abs_diff(m) > max {
        return None;
    }
    let max = max.min(n.max(m));
    const FAR: usize = usize::MAX / 2;
    // Row i holds the distance from the first i items of `a` to the first j items of `b`. Only cells with
    // |i - j| <= max can hold a value of at most `max`, so only those are worked out.
    let mut two_back = vec![FAR; m + 1];
    let mut last = vec![FAR; m + 1];
    let mut row = vec![FAR; m + 1];
    for (j, cell) in last.iter_mut().enumerate().take(max.min(m) + 1) {
        *cell = j;
    }
    for i in 1..=n {
        let low = i.saturating_sub(max).max(1);
        let high = (i + max).min(m);
        row[low - 1] = if low == 1 && i <= max { i } else { FAR };
        for j in low..=high {
            let changed = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (last[j - 1] + changed).min(last[j] + 1).min(row[j - 1] + 1);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(two_back[j - 2] + 1);
            }
            row[j] = best;
        }
        if high < m {
            row[high + 1] = FAR;
        }
        std::mem::swap(&mut two_back, &mut last);
        std::mem::swap(&mut last, &mut row);
    }
    (last[m] <= max).then_some(last[m])
}
