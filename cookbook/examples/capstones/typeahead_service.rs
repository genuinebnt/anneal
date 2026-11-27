//! CAPSTONE 14 (search): a type-ahead service (axum): prefix trie + trigram typo index, merged ranking, LRU cache, live updates
//! LEVEL: capstone
//! Use in: search boxes, command palettes, address/product/user pickers: any "suggest as I type" backend.   Run: cargo run --release --example typeahead_service
#![allow(dead_code)]
//!
//! WHAT: an HTTP service on loopback: `GET /suggest?q=&limit=`, `POST /events` (a suggestion was chosen), `POST /index`
//! (add terms), `GET /metrics`. Behind it, one `Engine` combines two indexes and a cache:
//!
//!   GET /suggest?q=rsut --> normalize --> LRU cache (key = normalized q + limit, valid only for the current generation)
//!                                            | miss
//!                       +--------------------+---------------------+
//!                       v                                          v
//!        prefix TRIE (per-node cached top-K,            TRIGRAM inverted index (padded char trigrams)
//!        decayed popularity)  -> O(|q|+K)               infix: intersect postings, verify `contains`
//!                       |                               typo : count shared trigrams >= need, verify edit distance
//!                       +--------------> final ranker <------------+   prefix first, then (distance, popularity,
//!                                  dedupe by term id, cut to limit      recency, text, id): a TOTAL order
//!   POST /events, /index --> writer mutex --> mutate index in place (RwLock) or copy-and-swap (Arc snapshot)
//!                            --> bump generation --> invalidate only the cache entries the change can affect
//!
//! WHY each piece:
//!  * trie with cached top-K: a query for "a" must not scan every word starting with "a"; each node already holds its
//!    K best term ids, so a lookup is a walk of |q| nodes plus a clone of <= 20 ids. Writes pay: an event re-sorts one
//!    small list per node on the term's path. Scores only ever grow (popularity += 2^(t/half_life), kept as its log2), so promoting one
//!    term never invalidates the order of the others (see autocomplete_engine for the decay trick).
//!  * trigrams: a trie cannot find "script" inside "javascript" or repair "rsut". Every term is indexed by its padded
//!    character trigrams. Candidates are found by counting shared trigrams (cheap, set arithmetic on posting lists),
//!    then VERIFIED with the real predicate (substring / Damerau-Levenshtein): filter-then-verify, no false positives.
//!    Matching policy (like pg_trgm's similarity threshold): a typo match needs >= max(2, grams(q) - 4*d) shared
//!    trigrams AND distance <= d, d = 1 for 3-5 chars, 2 beyond; queries under 3 chars match by prefix only.
//!    LIMIT: the shared-trigram floor is stricter than the distance. A half-typed word loses the end padding, so a
//!    short typo in the start of a long word can be missed ("rsut" does not find "rustacean", one swap away).
//!  * one ranker, one total order, so results are deterministic and testable against a brute-force oracle.
//!  * cache invalidation: a change to term t can only alter cached queries that t matches (prefix, infix or typo), so
//!    only those entries are dropped; hot unrelated prefixes keep their hits. The cache records which index
//!    generation its entries are valid for; a reader of any other generation neither takes from it nor adds to it.
//!  * RwLock vs copy-on-write snapshots: the engine can run on either; main() MEASURES both under reader/writer load
//!    and picks the better one. In-place RwLock updates cost O(path); a snapshot swap costs a full index clone per
//!    write but readers never wait. The clone grows with index size (hundreds of writes/s on 5000 terms is fine, millions of terms is not), so big indexes
//!    use in-place updates or persistent/segmented structures; snapshots win for rare bulk rebuilds.
//!  * limits: one request may carry 128 characters of text and 1000 terms. The index as a whole is not capped, there
//!    is no authentication or rate limit, handlers run the search on the async worker threads, and the `epoch` that
//!    a write returns is read after the write, so under concurrent writers it may belong to a later one.
//!  * request coalescing / debouncing (design note, not coded): clients should debounce keystrokes (~100-150 ms) and
//!    cancel in-flight requests when a newer one is sent (the response to "ru" is useless once "rus" is typed). The
//!    server can additionally coalesce identical concurrent misses ("single flight"): the first request computes,
//!    followers wait on the same future. The LRU already gives the same effect for the second request onwards.
//!  * Unicode: queries and terms are folded (lowercase, accents stripped via a small table, combining marks dropped,
//!    whitespace collapsed), so "CAFE", "Café" and "cafe\u{301}" are one key. Display text is kept as entered.
//!
//! COMMON USES: search-as-you-type in e-commerce, IDE/command palettes, address and contact pickers, "frecency" ranking
//! in shells and browsers (decayed popularity), spell-tolerant lookup (pg_trgm, Elasticsearch completion/fuzzy suggesters
//! are the same ideas); patterns: per-node top-K caching, filter-then-verify, LRU with targeted invalidation,
//! generation stamps against races, copy-on-write snapshots vs RwLock, latency histograms with percentiles.
//! Read: https://en.wikipedia.org/wiki/Trie https://swtch.com/~rsc/regexp/regexp4.html https://en.wikipedia.org/wiki/Levenshtein_distance https://docs.rs/axum/latest/axum/ https://en.wikipedia.org/wiki/Cache_replacement_policies
use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

// ======================================================================== clock and histogram

/// Injectable time source (seconds). Tests drive a `ManualClock`; the service uses `SystemClock`.
trait Clock: Send + Sync {
    fn now(&self) -> u64;
}

struct ManualClock(AtomicU64);

impl ManualClock {
    fn new(t: u64) -> ManualClock {
        ManualClock(AtomicU64::new(t))
    }
    fn set(&self, t: u64) {
        self.0.store(t, SeqCst);
    }
    fn advance(&self, dt: u64) {
        self.0.fetch_add(dt, SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> u64 {
        self.0.load(SeqCst)
    }
}

struct SystemClock(Instant);

impl Clock for SystemClock {
    fn now(&self) -> u64 {
        self.0.elapsed().as_secs()
    }
}

/// Log-linear latency histogram in microseconds (8 sub-buckets per octave, <= 12.5% error), lock-free `record`.
struct Histogram {
    buckets: Vec<AtomicU64>,
    count: AtomicU64,
    sum: AtomicU64,
    max: AtomicU64,
}

const NB: usize = 16 + 8 * 36;

impl Histogram {
    fn new() -> Histogram {
        Histogram {
            buckets: (0..NB).map(|_| AtomicU64::new(0)).collect(),
            count: AtomicU64::new(0),
            sum: AtomicU64::new(0),
            max: AtomicU64::new(0),
        }
    }
    fn index(v: u64) -> usize {
        if v < 16 {
            return v as usize;
        }
        let e = 63 - v.leading_zeros() as usize;
        (16 + (e - 4) * 8 + ((v >> (e - 3)) & 7) as usize).min(NB - 1)
    }
    fn upper(idx: usize) -> u64 {
        if idx < 16 {
            return idx as u64;
        }
        let k = idx - 16;
        ((8 + (k % 8) as u64 + 1) << (k / 8 + 1)) - 1
    }
    fn record(&self, v: u64) {
        self.buckets[Self::index(v)].fetch_add(1, SeqCst);
        self.count.fetch_add(1, SeqCst);
        self.sum.fetch_add(v, SeqCst);
        self.max.fetch_max(v, SeqCst);
    }
    fn quantile(&self, q: f64) -> u64 {
        let n = self.count.load(SeqCst);
        let target = ((q * n as f64).ceil() as u64).max(1);
        let mut cum = 0;
        for (i, b) in self.buckets.iter().enumerate() {
            cum += b.load(SeqCst);
            if cum >= target {
                return Self::upper(i).min(self.max.load(SeqCst));
            }
        }
        0
    }
}

// ======================================================================== text normalisation and edit distance

fn fold(c: char) -> Option<&'static str> {
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

/// Lowercase, strip accents via the table, drop combining marks (decomposed accents), collapse whitespace.
fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = true;
    for c in s.chars() {
        if c.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
            continue;
        }
        if c.is_control() {
            continue; // never let input forge the '\u{1}' padding of the trigram index
        }
        // marks are dropped AFTER lowercasing: lowercasing a dotted capital I produces one
        for lc in c.to_lowercase().filter(|&l| !is_mark(l)) {
            last_space = false;
            match fold(lc) {
                Some(r) => out.push_str(r),
                None => out.push(lc),
            }
        }
    }
    while out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Optimal-string-alignment (restricted Damerau-Levenshtein): insert, delete, substitute, swap adjacent = 1 edit each.
fn osa(a: &[char], b: &[char]) -> usize {
    osa_max(a, b, usize::MAX / 2)
}

/// Same, but gives up early: any result > `max` only means "more than max". Row minima cannot shrink (a swap reaches
/// back two rows at +1), so once the last two rows are all above the budget the answer is decided.
fn osa_max(a: &[char], b: &[char], max: usize) -> usize {
    let (n, m) = (a.len(), b.len());
    let w = m + 1;
    let mut buf = vec![0usize; 3 * w]; // rows i-2, i-1, i
    for j in 0..w {
        buf[w + j] = j;
    }
    let mut prev_min = 0;
    for i in 1..=n {
        let (r0, r1, r2) = (0, w, 2 * w);
        buf[r2] = i;
        let mut row_min = i;
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut v = (buf[r1 + j] + 1)
                .min(buf[r2 + j - 1] + 1)
                .min(buf[r1 + j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                v = v.min(buf[r0 + j - 2] + 1);
            }
            buf[r2 + j] = v;
            row_min = row_min.min(v);
        }
        if row_min > max && prev_min >= max {
            return max + 1;
        }
        prev_min = row_min;
        buf.rotate_left(w); // (i-1, i, scratch) becomes (i-2, i-1, i)
    }
    buf[w + m]
}

/// Typo distance of query `q` to term `t`: the better of "whole term" and "the first |q| chars of the term" (so a typo in a
/// half-typed word can match, if enough trigrams survive: see `class_of`). Only values <= `max` are meaningful (the other variant may be skipped when it cannot fit).
fn fuzzy_within(q: &[char], t: &str, max: usize) -> Option<usize> {
    let tc: Vec<char> = t.chars().collect();
    let mut best = usize::MAX;
    if tc.len().abs_diff(q.len()) <= max {
        best = osa_max(q, &tc, max);
    }
    if tc.len() > q.len() {
        best = best.min(osa_max(q, &tc[..q.len()], max));
    }
    (best <= max).then_some(best)
}

/// Typo budget by query length. Queries shorter than 3 chars are prefix-only (no trigram to work with).
fn budget(len: usize) -> usize {
    match len {
        0..=2 => 0,
        3..=5 => 1,
        _ => 2,
    }
}

fn gram_key(a: char, b: char, c: char) -> u64 {
    ((a as u64) << 42) | ((b as u64) << 21) | c as u64
}

/// Distinct trigrams of `s`: `padded` adds two sentinel chars on each side so word starts/ends count too.
fn gram_set(s: &[char], padded: bool) -> Vec<u64> {
    let mut v: Vec<char> = Vec::with_capacity(s.len() + 4);
    if padded {
        v.extend(['\u{1}', '\u{1}']);
    }
    v.extend_from_slice(s);
    if padded {
        v.extend(['\u{1}', '\u{1}']);
    }
    let mut g: Vec<u64> = v.windows(3).map(|w| gram_key(w[0], w[1], w[2])).collect();
    g.sort_unstable();
    g.dedup();
    g
}

/// Minimum number of shared padded trigrams for a typo candidate (the q-gram lemma, floored at 2 to keep candidate sets small).
fn need_of(q: &[char]) -> usize {
    gram_set(q, true)
        .len()
        .saturating_sub(4 * budget(q.len()))
        .max(2)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Prefix,
    Infix,
    Typo,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Prefix => "prefix",
            Kind::Infix => "infix",
            Kind::Typo => "typo",
        }
    }
}

/// THE matching predicate, written naively (no index). The trigram index is an accelerator for exactly this function,
/// the brute-force oracle calls it for every term, and cache invalidation uses it to ask "can term t affect query q?".
fn class_of(qn: &str, tnorm: &str) -> Option<(Kind, u8)> {
    if tnorm.starts_with(qn) {
        return Some((Kind::Prefix, 0));
    }
    let qc: Vec<char> = qn.chars().collect();
    if qc.len() < 3 {
        return None;
    }
    if tnorm.contains(qn) {
        return Some((Kind::Infix, 0));
    }
    let d = budget(qc.len());
    let dist = fuzzy_within(&qc, tnorm, d)?;
    let tc: Vec<char> = tnorm.chars().collect();
    let (gq, gt) = (gram_set(&qc, true), gram_set(&tc, true));
    let overlap = gq.iter().filter(|g| gt.binary_search(g).is_ok()).count();
    (overlap >= need_of(&qc)).then_some((Kind::Typo, dist as u8))
}

// ======================================================================== terms, trie, trigram index

#[derive(Clone)]
struct Term {
    text: String,
    norm: String,
    /// log2 of the sum of count * 2^(t / HALF_LIFE). It only grows, and the ORDER of terms is what decay preserves.
    /// The sum itself would overflow f64 after 1024 half-lives; -inf means "never counted".
    weight: f64,
    last: u64,
    hits: u64,
}

const HALF_LIFE: f64 = 3600.0;

/// log2 of `count` selections made at time `now`.
fn boost(count: u32, now: u64) -> f64 {
    f64::from(count).log2() + now as f64 / HALF_LIFE
}

/// log2(2^a + 2^b), without ever forming 2^a or 2^b.
fn log_add(a: f64, b: f64) -> f64 {
    let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
    if lo == f64::NEG_INFINITY {
        return hi;
    }
    hi + (lo - hi).exp2().ln_1p() / std::f64::consts::LN_2
}

/// The popularity shown to clients: the sum decayed to time `now`.
fn score_at(weight: f64, now: u64) -> f64 {
    (weight - now as f64 / HALF_LIFE).exp2()
}

#[derive(Clone)]
enum Op {
    Add(Vec<(String, u32)>),
    Event(String),
}

#[derive(Clone, Default)]
struct Terms {
    list: Vec<Term>,
    by_norm: HashMap<String, u32>,
}

impl Terms {
    fn bump(&mut self, id: u32, count: u32, now: u64) {
        let t = &mut self.list[id as usize];
        t.weight = log_add(t.weight, boost(count, now));
        t.last = t.last.max(now);
        t.hits += count as u64;
    }
    /// Applies an operation to the term table; returns false when nothing changed. `on_touch(table, id, is_new)` runs
    /// after EACH change, before the next one: a sorted list can only absorb one moved element at a time.
    fn apply(&mut self, op: &Op, now: u64, mut on_touch: impl FnMut(&Terms, u32, bool)) -> bool {
        let mut touched = false;
        match op {
            Op::Add(list) => {
                for (text, count) in list {
                    let norm = normalize(text);
                    if norm.is_empty() {
                        continue;
                    }
                    if let Some(&id) = self.by_norm.get(&norm) {
                        self.bump(id, *count, now);
                        on_touch(self, id, false);
                        touched = true;
                    } else {
                        let id = self.list.len() as u32;
                        self.by_norm.insert(norm.clone(), id);
                        self.list.push(Term {
                            text: text.trim().to_string(),
                            norm,
                            weight: boost(*count, now),
                            last: now,
                            hits: *count as u64,
                        });
                        on_touch(self, id, true);
                        touched = true;
                    }
                }
            }
            Op::Event(text) => {
                let Some(&id) = self.by_norm.get(&normalize(text)) else {
                    return false;
                };
                self.bump(id, 1, now);
                on_touch(self, id, false);
                touched = true;
            }
        }
        touched
    }
    /// Total order: more popular, then more recent, then text, then id (so no two terms ever tie).
    fn rank_cmp(&self, a: u32, b: u32) -> Ordering {
        let (x, y) = (&self.list[a as usize], &self.list[b as usize]);
        y.weight
            .total_cmp(&x.weight)
            .then(y.last.cmp(&x.last))
            .then_with(|| x.norm.cmp(&y.norm))
            .then(a.cmp(&b))
    }
}

const K: usize = 20; // top-K cached per trie node; also the maximum `limit`

// What one request may carry. Without these, one long term builds one trie node per character, one long query makes
// every candidate an expensive edit-distance run, and a huge batch holds the cache lock while it is matched against
// every cached query.
const MAX_TEXT_CHARS: usize = 128;
const MAX_BATCH: usize = 1_000;

#[derive(Clone, Default)]
struct Node {
    kids: Vec<(char, u32)>,
    top: Vec<u32>, // best K term ids in this subtree, sorted by rank
}

#[derive(Clone)]
struct Trie {
    nodes: Vec<Node>,
}

impl Trie {
    fn new() -> Trie {
        Trie {
            nodes: vec![Node::default()],
        }
    }
    /// Insert (or re-rank after a score increase) `id` in the top list of every node on its path, root included.
    fn promote(&mut self, norm: &str, id: u32, terms: &Terms) {
        let mut n = 0usize;
        self.touch(n, id, terms);
        for c in norm.chars() {
            n = match self.nodes[n].kids.iter().find(|(k, _)| *k == c) {
                Some(&(_, child)) => child as usize,
                None => {
                    self.nodes.push(Node::default());
                    let child = self.nodes.len() - 1;
                    self.nodes[n].kids.push((c, child as u32));
                    child
                }
            };
            self.touch(n, id, terms);
        }
    }
    fn touch(&mut self, n: usize, id: u32, terms: &Terms) {
        let top = &mut self.nodes[n].top;
        if let Some(p) = top.iter().position(|&x| x == id) {
            top.remove(p);
        }
        let pos = top.partition_point(|&x| terms.rank_cmp(x, id) == Ordering::Less);
        top.insert(pos, id);
        top.truncate(K);
    }
    fn find(&self, q: &str) -> &[u32] {
        let mut n = 0usize;
        for c in q.chars() {
            match self.nodes[n].kids.iter().find(|(k, _)| *k == c) {
                Some(&(_, child)) => n = child as usize,
                None => return &[],
            }
        }
        &self.nodes[n].top
    }
}

#[derive(Clone, Default)]
struct Trigrams {
    post: HashMap<u64, Vec<u32>>, // trigram -> sorted term ids
}

impl Trigrams {
    fn add(&mut self, norm: &str, id: u32) {
        let c: Vec<char> = norm.chars().collect();
        for g in gram_set(&c, true) {
            self.post.entry(g).or_default().push(id); // ids arrive in increasing order
        }
    }
    /// Terms containing EVERY inner trigram of q (necessary for a substring match); verified by the caller.
    fn infix_candidates(&self, q: &[char]) -> Vec<u32> {
        let mut lists: Vec<&Vec<u32>> = Vec::new();
        for g in gram_set(q, false) {
            match self.post.get(&g) {
                Some(l) => lists.push(l),
                None => return Vec::new(),
            }
        }
        lists.sort_by_key(|l| l.len()); // rarest first
        let Some((first, rest)) = lists.split_first() else {
            return Vec::new();
        };
        let mut c: Vec<u32> = (*first).clone();
        for l in rest {
            c.retain(|id| l.binary_search(id).is_ok());
        }
        c
    }
    /// Terms sharing at least `need` of q's padded trigrams, found by counting posting-list hits.
    fn typo_candidates(&self, q: &[char], need: usize) -> Vec<u32> {
        let mut counts: HashMap<u32, usize> = HashMap::new();
        for g in gram_set(q, true) {
            for &id in self.post.get(&g).into_iter().flatten() {
                *counts.entry(id).or_insert(0) += 1;
            }
        }
        counts
            .into_iter()
            .filter(|&(_, c)| c >= need)
            .map(|(id, _)| id)
            .collect()
    }
}

#[derive(Clone)]
struct Hit {
    id: u32,
    term: String,
    weight: f64,
    kind: Kind,
    dist: u8,
}

#[derive(Clone)]
struct Index {
    terms: Terms,
    trie: Trie,
    tri: Trigrams,
    epoch: u64,
}

struct Applied {
    changed: bool,
    norms: Vec<String>,
    added: usize,
    existing: usize,
}

impl Index {
    fn new() -> Index {
        Index {
            terms: Terms::default(),
            trie: Trie::new(),
            tri: Trigrams::default(),
            epoch: 0,
        }
    }
    fn apply(&mut self, op: &Op, now: u64) -> Applied {
        let mut a = Applied {
            changed: false,
            norms: vec![],
            added: 0,
            existing: 0,
        };
        let (trie, tri) = (&mut self.trie, &mut self.tri);
        let changed = self.terms.apply(op, now, |terms, id, is_new| {
            let norm = terms.list[id as usize].norm.clone();
            trie.promote(&norm, id, terms);
            if is_new {
                tri.add(&norm, id);
                a.added += 1;
            } else {
                a.existing += 1;
            }
            a.norms.push(norm);
        });
        a.changed = changed;
        if changed {
            self.epoch += 1;
        }
        a
    }

    /// The service's answer: prefix hits from the trie, then (only if the trie could not fill `limit`) the trigram
    /// candidates verified and ordered by (distance, rank). Must equal `oracle` exactly.
    fn search(&self, qn: &str, limit: usize) -> Vec<Hit> {
        let mk = |id: u32, kind: Kind, dist: u8| {
            let t = &self.terms.list[id as usize];
            Hit {
                id,
                term: t.text.clone(),
                weight: t.weight,
                kind,
                dist,
            }
        };
        let pre = self.trie.find(qn);
        let mut out: Vec<Hit> = pre
            .iter()
            .take(limit)
            .map(|&id| mk(id, Kind::Prefix, 0))
            .collect();
        let qc: Vec<char> = qn.chars().collect();
        if pre.len() >= limit || qc.len() < 3 {
            return out;
        }
        let preset: HashSet<u32> = pre.iter().copied().collect();
        let mut fz: HashMap<u32, (Kind, u8)> = HashMap::new();
        for id in self.tri.infix_candidates(&qc) {
            if !preset.contains(&id) && self.terms.list[id as usize].norm.contains(qn) {
                fz.insert(id, (Kind::Infix, 0));
            }
        }
        let d = budget(qc.len());
        for id in self.tri.typo_candidates(&qc, need_of(&qc)) {
            if preset.contains(&id) || fz.contains_key(&id) {
                continue;
            }
            if let Some(dist) = fuzzy_within(&qc, &self.terms.list[id as usize].norm, d) {
                fz.insert(id, (Kind::Typo, dist as u8));
            }
        }
        let mut v: Vec<(u32, Kind, u8)> = fz.into_iter().map(|(id, (k, d))| (id, k, d)).collect();
        v.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| self.terms.rank_cmp(a.0, b.0)));
        out.extend(
            v.into_iter()
                .take(limit - out.len())
                .map(|(id, k, d)| mk(id, k, d)),
        );
        out
    }
}

/// Brute-force reference: classify EVERY term with `class_of`, sort by (prefix first, distance, rank), take `limit`.
fn oracle(terms: &Terms, q: &str, limit: usize) -> Vec<(u32, Kind, u8)> {
    let qn = normalize(q);
    let mut v: Vec<(u32, Kind, u8)> = (0..terms.list.len() as u32)
        .filter_map(|id| class_of(&qn, &terms.list[id as usize].norm).map(|(k, d)| (id, k, d)))
        .collect();
    v.sort_by(|a, b| {
        (a.1 != Kind::Prefix)
            .cmp(&(b.1 != Kind::Prefix))
            .then(a.2.cmp(&b.2))
            .then_with(|| terms.rank_cmp(a.0, b.0))
    });
    v.truncate(limit);
    v
}

// ======================================================================== LRU cache

const NIL: usize = usize::MAX;
type CKey = (String, usize);

struct Slot {
    key: CKey,
    val: Arc<Vec<Hit>>,
    prev: usize,
    next: usize,
}

/// LRU on an index-linked list in a Vec: O(1) get/put/evict, no unsafe, no Rc.
struct Lru {
    cap: usize,
    map: HashMap<CKey, usize>,
    slots: Vec<Slot>,
    free: Vec<usize>,
    head: usize,
    tail: usize,
}

impl Lru {
    fn new(cap: usize) -> Lru {
        Lru {
            cap,
            map: HashMap::new(),
            slots: Vec::new(),
            free: Vec::new(),
            head: NIL,
            tail: NIL,
        }
    }
    fn len(&self) -> usize {
        self.map.len()
    }
    fn unlink(&mut self, i: usize) {
        let (p, n) = (self.slots[i].prev, self.slots[i].next);
        if p != NIL {
            self.slots[p].next = n;
        } else {
            self.head = n;
        }
        if n != NIL {
            self.slots[n].prev = p;
        } else {
            self.tail = p;
        }
    }
    fn push_front(&mut self, i: usize) {
        self.slots[i].prev = NIL;
        self.slots[i].next = self.head;
        if self.head != NIL {
            let h = self.head;
            self.slots[h].prev = i;
        }
        self.head = i;
        if self.tail == NIL {
            self.tail = i;
        }
    }
    fn get(&mut self, k: &CKey) -> Option<Arc<Vec<Hit>>> {
        let &i = self.map.get(k)?;
        self.unlink(i);
        self.push_front(i);
        Some(self.slots[i].val.clone())
    }
    fn put(&mut self, k: CKey, v: Arc<Vec<Hit>>) {
        if self.cap == 0 {
            return;
        }
        if let Some(&i) = self.map.get(&k) {
            self.slots[i].val = v;
            self.unlink(i);
            self.push_front(i);
            return;
        }
        if self.map.len() >= self.cap {
            let t = self.tail;
            self.remove_slot(t);
        }
        let slot = Slot {
            key: k.clone(),
            val: v,
            prev: NIL,
            next: NIL,
        };
        let i = match self.free.pop() {
            Some(i) => {
                self.slots[i] = slot;
                i
            }
            None => {
                self.slots.push(slot);
                self.slots.len() - 1
            }
        };
        self.map.insert(k, i);
        self.push_front(i);
    }
    fn remove_slot(&mut self, i: usize) {
        self.unlink(i);
        let key = self.slots[i].key.clone();
        self.map.remove(&key);
        self.slots[i].val = Arc::new(Vec::new());
        self.free.push(i);
    }
    /// Drops every entry for which `keep` is false; returns how many were dropped.
    fn retain(&mut self, mut keep: impl FnMut(&CKey) -> bool) -> usize {
        let dead: Vec<usize> = self
            .map
            .iter()
            .filter(|(k, _)| !keep(k))
            .map(|(_, &i)| i)
            .collect();
        for &i in &dead {
            self.remove_slot(i);
        }
        dead.len()
    }
}

// ======================================================================== store (RwLock vs snapshot swap) and engine

#[derive(Clone, Copy, PartialEq, Debug)]
enum StoreKind {
    RwLock,
    Snapshot,
}

enum Store {
    Rw(RwLock<Index>),
    Cow(RwLock<Arc<Index>>),
}

impl Store {
    fn new(kind: StoreKind) -> Store {
        match kind {
            StoreKind::RwLock => Store::Rw(RwLock::new(Index::new())),
            StoreKind::Snapshot => Store::Cow(RwLock::new(Arc::new(Index::new()))),
        }
    }
    /// Run a read-only closure on a consistent index. RwLock: holds the read lock for the whole query.
    /// Snapshot: holds the lock only to clone the Arc, then queries lock-free.
    fn with<R>(&self, f: impl FnOnce(&Index) -> R) -> R {
        match self {
            Store::Rw(l) => f(&l.read().unwrap()),
            Store::Cow(l) => {
                let snap = l.read().unwrap().clone();
                f(&snap)
            }
        }
    }
    /// Callers serialise writers (Engine::writer). Snapshot: clone the whole index, mutate the copy, swap the pointer.
    fn update<R>(&self, f: impl FnOnce(&mut Index) -> R) -> R {
        match self {
            Store::Rw(l) => f(&mut l.write().unwrap()),
            Store::Cow(l) => {
                let mut next = (**l.read().unwrap()).clone();
                let r = f(&mut next);
                *l.write().unwrap() = Arc::new(next);
                r
            }
        }
    }
}

struct Cache {
    lru: Lru,
    epoch: u64, // generation all entries are valid for
}

struct Stats {
    suggests: AtomicU64,
    hits: AtomicU64,
    misses: AtomicU64,
    events: AtomicU64,
    unknown_events: AtomicU64,
    adds: AtomicU64,
    invalidated: AtomicU64,
    latency: Histogram,
}

struct Engine {
    clock: Arc<dyn Clock>,
    store: Store,
    writer: Mutex<()>,
    cache: Mutex<Cache>,
    stats: Stats,
}

struct Answer {
    epoch: u64,
    cached: bool,
    norm: String,
    hits: Arc<Vec<Hit>>,
}

impl Engine {
    fn new(
        kind: StoreKind,
        clock: Arc<dyn Clock>,
        cache_cap: usize,
        seed: Vec<(String, u32)>,
    ) -> Engine {
        let e = Engine {
            clock,
            store: Store::new(kind),
            writer: Mutex::new(()),
            cache: Mutex::new(Cache {
                lru: Lru::new(cache_cap),
                epoch: 0,
            }),
            stats: Stats {
                suggests: AtomicU64::new(0),
                hits: AtomicU64::new(0),
                misses: AtomicU64::new(0),
                events: AtomicU64::new(0),
                unknown_events: AtomicU64::new(0),
                adds: AtomicU64::new(0),
                invalidated: AtomicU64::new(0),
                latency: Histogram::new(),
            },
        };
        e.apply(Op::Add(seed));
        e
    }

    fn suggest(&self, q: &str, limit: usize) -> Answer {
        let norm = normalize(q);
        let limit = limit.clamp(1, K);
        self.stats.suggests.fetch_add(1, SeqCst);
        let key = (norm.clone(), limit);
        let (epoch, hits, cached) = self.store.with(|idx| {
            let epoch = idx.epoch;
            {
                let mut c = self.cache.lock().unwrap();
                if c.epoch == epoch {
                    if let Some(h) = c.lru.get(&key) {
                        return (epoch, h, true);
                    }
                }
            }
            let hits = Arc::new(idx.search(&norm, limit));
            self.remember(key.clone(), hits.clone(), epoch);
            (epoch, hits, false)
        });
        (if cached {
            &self.stats.hits
        } else {
            &self.stats.misses
        })
        .fetch_add(1, SeqCst);
        Answer {
            epoch,
            cached,
            norm,
            hits,
        }
    }

    /// Stores an answer computed from generation `epoch`, but never one computed from an older generation: with
    /// snapshots, a writer may have changed the index and cleaned the cache while the search was running.
    fn remember(&self, key: CKey, hits: Arc<Vec<Hit>>, epoch: u64) {
        let mut c = self.cache.lock().unwrap();
        if c.epoch == epoch {
            c.lru.put(key, hits);
        }
    }

    /// Writers are serialised. After the index changes, drop exactly the cache entries the change can affect and
    /// move the cache to the new generation in one critical section.
    fn apply(&self, op: Op) -> Applied {
        let _w = self.writer.lock().unwrap();
        let now = self.clock.now();
        let (epoch, applied) = self.store.update(|idx| {
            let a = idx.apply(&op, now);
            (idx.epoch, a)
        });
        match &op {
            Op::Event(_) if applied.changed => self.stats.events.fetch_add(1, SeqCst),
            Op::Event(_) => self.stats.unknown_events.fetch_add(1, SeqCst),
            Op::Add(_) => self.stats.adds.fetch_add(applied.added as u64, SeqCst),
        };
        if applied.changed {
            let mut c = self.cache.lock().unwrap();
            let dropped = c
                .lru
                .retain(|(q, _)| !applied.norms.iter().any(|t| class_of(q, t).is_some()));
            self.stats.invalidated.fetch_add(dropped as u64, SeqCst);
            c.epoch = epoch;
        }
        applied
    }

    fn oracle_now(&self, q: &str, limit: usize) -> Vec<(u32, Kind, u8)> {
        self.store
            .with(|idx| oracle(&idx.terms, q, limit.clamp(1, K)))
    }
    fn epoch(&self) -> u64 {
        self.store.with(|i| i.epoch)
    }
    fn term_count(&self) -> usize {
        self.store.with(|i| i.terms.list.len())
    }
    fn terms_clone(&self) -> Terms {
        self.store.with(|i| i.terms.clone())
    }

    fn metrics_text(&self) -> String {
        let s = &self.stats;
        let mut o = String::new();
        let mut line = |name: &str, kind: &str, v: String| {
            o.push_str(&format!("# TYPE {name} {kind}\n{name} {v}\n"))
        };
        line(
            "typeahead_suggest_requests_total",
            "counter",
            s.suggests.load(SeqCst).to_string(),
        );
        line(
            "typeahead_cache_hits_total",
            "counter",
            s.hits.load(SeqCst).to_string(),
        );
        line(
            "typeahead_cache_misses_total",
            "counter",
            s.misses.load(SeqCst).to_string(),
        );
        line(
            "typeahead_cache_invalidated_total",
            "counter",
            s.invalidated.load(SeqCst).to_string(),
        );
        line(
            "typeahead_events_total",
            "counter",
            s.events.load(SeqCst).to_string(),
        );
        line(
            "typeahead_terms_added_total",
            "counter",
            s.adds.load(SeqCst).to_string(),
        );
        line(
            "typeahead_indexed_terms",
            "gauge",
            self.term_count().to_string(),
        );
        line(
            "typeahead_index_generation",
            "gauge",
            self.epoch().to_string(),
        );
        o.push_str("# TYPE typeahead_suggest_latency_us summary\n");
        for q in [0.5, 0.9, 0.99] {
            o.push_str(&format!(
                "typeahead_suggest_latency_us{{quantile=\"{q}\"}} {}\n",
                s.latency.quantile(q)
            ));
        }
        o.push_str(&format!(
            "typeahead_suggest_latency_us_sum {}\ntypeahead_suggest_latency_us_count {}\n",
            s.latency.sum.load(SeqCst),
            s.latency.count.load(SeqCst)
        ));
        o
    }
}

// ======================================================================== HTTP layer (axum)

#[derive(Deserialize)]
struct SuggestParams {
    q: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct EventBody {
    term: String,
}

#[derive(Deserialize)]
struct TermIn {
    term: String,
    #[serde(default = "one")]
    count: u32,
}

fn one() -> u32 {
    1
}

#[derive(Deserialize)]
struct IndexBody {
    terms: Vec<TermIn>,
}

type Shared = Arc<Engine>;

fn too_long(s: &str) -> bool {
    s.chars().nth(MAX_TEXT_CHARS).is_some()
}

fn refuse(why: &str) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"ok": false, "error": why})),
    )
}

async fn h_suggest(
    State(e): State<Shared>,
    Query(p): Query<SuggestParams>,
) -> (StatusCode, Json<Value>) {
    if p.q.as_deref().is_some_and(too_long) {
        return refuse("q is too long");
    }
    let t = Instant::now();
    let ans = e.suggest(p.q.as_deref().unwrap_or(""), p.limit.unwrap_or(5));
    e.stats.latency.record(t.elapsed().as_micros() as u64);
    let now = e.clock.now();
    let items: Vec<Value> = ans
        .hits
        .iter()
        .map(|h| json!({"term": h.term, "score": (score_at(h.weight, now) * 1000.0).round() / 1000.0, "kind": h.kind.name(), "dist": h.dist}))
        .collect();
    (
        StatusCode::OK,
        Json(
            json!({"q": p.q.unwrap_or_default(), "normalized": ans.norm, "epoch": ans.epoch, "cached": ans.cached, "suggestions": items}),
        ),
    )
}

async fn h_events(State(e): State<Shared>, Json(b): Json<EventBody>) -> (StatusCode, Json<Value>) {
    if too_long(&b.term) {
        return refuse("term is too long");
    }
    let a = e.apply(Op::Event(b.term.clone()));
    if a.changed {
        (
            StatusCode::OK,
            Json(json!({"ok": true, "epoch": e.epoch()})),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(json!({"ok": false, "error": format!("unknown term {:?}", b.term)})),
        )
    }
}

async fn h_index(State(e): State<Shared>, Json(b): Json<IndexBody>) -> (StatusCode, Json<Value>) {
    if b.terms.len() > MAX_BATCH || b.terms.iter().any(|t| too_long(&t.term)) {
        return refuse("too many terms, or a term is too long");
    }
    let a = e.apply(Op::Add(
        b.terms.into_iter().map(|t| (t.term, t.count)).collect(),
    ));
    (
        StatusCode::OK,
        Json(json!({"added": a.added, "existing": a.existing, "epoch": e.epoch()})),
    )
}

async fn h_metrics(State(e): State<Shared>) -> String {
    e.metrics_text()
}

fn router(e: Shared) -> Router {
    Router::new()
        .route("/suggest", get(h_suggest))
        .route("/events", post(h_events))
        .route("/index", post(h_index))
        .route("/metrics", get(h_metrics))
        .with_state(e)
}

struct Http {
    addr: SocketAddr,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

async fn serve(e: Shared) -> Http {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = l.local_addr().unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let _ = axum::serve(l, router(e))
            .with_graceful_shutdown(async move {
                let _ = rx.await;
            })
            .await;
    });
    Http {
        addr,
        stop: Some(tx),
        task,
    }
}

impl Http {
    async fn stop(mut self) {
        let _ = self.stop.take().map(|t| t.send(()));
        let _ = tokio::time::timeout(Duration::from_secs(3), self.task).await; // never hangs on lingering keep-alive connections
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

// ======================================================================== data generation and test helpers

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const SYL: &[&str] = &[
    "ra", "ru", "ro", "re", "ri", "sta", "sto", "ste", "tra", "tro", "lo", "mi", "ne", "ka", "zo",
    "qua", "an", "el", "on", "ix", "fra", "gri", "bla", "cho", "dé", "mü", "ñu", "çe", "øl", "zi",
    "ba", "po", "vi", "su",
];

fn gen_terms(rng: &mut Rng, n: usize) -> Vec<(String, u32)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for t in [
        "Café",
        "Crème Brûlée",
        "naïve",
        "Zoë",
        "Straße",
        "Ångström",
        "日本語",
        "東京",
        "Łódź",
        "über",
    ] {
        seen.insert(normalize(t));
        out.push((t.to_string(), 1 + rng.below(50) as u32));
    }
    while out.len() < n {
        let mut w = String::new();
        for _ in 0..(2 + rng.below(3)) {
            w.push_str(SYL[rng.below(SYL.len())]);
        }
        if rng.below(6) == 0 {
            w.push(' ');
            w.push_str(SYL[rng.below(SYL.len())]);
            w.push_str(SYL[rng.below(SYL.len())]);
        }
        if rng.below(5) == 0 {
            w = w
                .chars()
                .next()
                .unwrap()
                .to_uppercase()
                .chain(w.chars().skip(1))
                .collect();
        }
        if seen.insert(normalize(&w)) {
            out.push((w, 1 + rng.below(50) as u32));
        }
    }
    out
}

fn mutate(rng: &mut Rng, s: &str) -> String {
    let mut c: Vec<char> = s.chars().collect();
    if c.len() < 2 {
        return s.to_string();
    }
    let i = rng.below(c.len() - 1);
    match rng.below(4) {
        0 => c.swap(i, i + 1),
        1 => {
            c.remove(i);
        }
        2 => c.insert(i, (b'a' + rng.below(26) as u8) as char),
        _ => c[i] = (b'a' + rng.below(26) as u8) as char,
    }
    c.into_iter().collect()
}

fn gen_query(rng: &mut Rng, terms: &Terms) -> String {
    let t = &terms.list[rng.below(terms.list.len())];
    let chars: Vec<char> = t.norm.chars().collect();
    let take = |rng: &mut Rng, lo: usize| -> String {
        let n = (lo + rng.below(chars.len().max(lo))).min(chars.len());
        chars[..n].iter().collect()
    };
    match rng.below(7) {
        0 | 1 => take(rng, 1), // plain prefix
        2 => {
            let w = take(rng, 3);
            mutate(rng, &w)
        } // typo in a (partial) word
        3 => {
            let a = rng.below(chars.len().saturating_sub(2).max(1));
            let b = (a + 3 + rng.below(3)).min(chars.len());
            chars[a..b].iter().collect() // infix
        }
        4 => t
            .text
            .to_uppercase()
            .chars()
            .take(1 + rng.below(4))
            .collect(), // upper-case, accents kept
        5 => (0..1 + rng.below(5))
            .map(|_| (b'a' + rng.below(26) as u8) as char)
            .collect(), // noise
        _ => [
            "日",
            "東京",
            "ł",
            "ü",
            "Ångs",
            "cafe\u{301}",
            "crème",
            "naive",
            "stras",
        ][rng.below(9)]
        .to_string(),
    }
}

fn check(name: &str, cond: bool) {
    println!("  [{}] {name}", if cond { "ok" } else { "FAIL" });
    assert!(cond, "check failed: {name}");
}

fn ids(a: &[Hit]) -> Vec<(u32, Kind, u8)> {
    a.iter().map(|h| (h.id, h.kind, h.dist)).collect()
}

fn curated() -> Vec<(String, u32)> {
    [
        ("rust", 90),
        ("rustup", 30),
        ("rustc", 25),
        ("ruby", 60),
        ("rubygems", 10),
        ("runtime", 20),
        ("rubric", 3),
        ("python", 120),
        ("pytorch", 40),
        ("javascript", 150),
        ("typescript", 100),
        ("java", 130),
        ("kotlin", 35),
        ("golang", 50),
        ("swift", 45),
        ("haskell", 15),
        ("erlang", 12),
        ("elixir", 18),
        ("react", 80),
        ("redis", 70),
        ("postgres", 75),
        ("café", 5),
        ("cafeteria", 2),
        ("Crème Brûlée", 4),
        ("naïve bayes", 6),
    ]
    .iter()
    .map(|(t, c)| (t.to_string(), *c))
    .collect()
}

async fn show(c: &reqwest::Client, base: &str, q: &str, limit: usize) -> Value {
    let resp = c
        .get(format!("{base}/suggest"))
        .query(&[("q", q), ("limit", &limit.to_string())])
        .send()
        .await
        .expect("request");
    let status = resp.status();
    let v: Value = resp.json().await.expect("json");
    println!(
        "  > GET /suggest?q={q}&limit={limit}   -> {status}  epoch={} cached={} normalized={:?}",
        v["epoch"],
        v["cached"],
        v["normalized"].as_str().unwrap_or("")
    );
    for (i, s) in v["suggestions"].as_array().unwrap().iter().enumerate() {
        println!(
            "      {}. {:<14} {:<7} dist={} score={}",
            i + 1,
            s["term"].as_str().unwrap(),
            s["kind"].as_str().unwrap(),
            s["dist"],
            s["score"]
        );
    }
    v
}

fn names(v: &Value) -> Vec<String> {
    v["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["term"].as_str().unwrap().to_string())
        .collect()
}

// ======================================================================== main

#[tokio::main]
async fn main() {
    println!(
        "=== type-ahead service: trie + trigram index + LRU + live updates (axum on loopback) ==="
    );

    // ---- 1. building blocks
    println!("\n[1] building blocks");
    check(
        "normalize folds case, accents, combining marks and spaces",
        normalize("  CAFÉ  Crème ") == "cafe creme"
            && normalize("cafe\u{301}") == "cafe"
            && normalize("Straße Ångström Łódź") == "strasse angstrom lodz",
    );
    let cs = |s: &str| s.chars().collect::<Vec<_>>();
    check(
        "osa distances (swap counts as 1)",
        osa(&cs("rsut"), &cs("rust")) == 1
            && osa(&cs("kitten"), &cs("sitting")) == 3
            && osa(&cs(""), &cs("abc")) == 3
            && osa(&cs("ab"), &cs("ba")) == 1,
    );
    let mut lru = Lru::new(2);
    let h = Arc::new(Vec::new());
    lru.put(("a".into(), 1), h.clone());
    lru.put(("b".into(), 1), h.clone());
    lru.get(&("a".into(), 1));
    lru.put(("c".into(), 1), h.clone());
    check(
        "LRU evicts the least recently used entry",
        lru.get(&("b".into(), 1)).is_none()
            && lru.get(&("a".into(), 1)).is_some()
            && lru.get(&("c".into(), 1)).is_some()
            && lru.len() == 2,
    );
    let hist = Histogram::new();
    for v in 1..=10_000u64 {
        hist.record(v);
    }
    check(
        "histogram quantiles within 12.5%",
        (4375..=5625).contains(&hist.quantile(0.5)) && hist.quantile(0.99) >= 8660,
    );

    // each of these failed before the matching fix
    check(
        "normalize is idempotent for every char, drops control chars and the dot of a capital dotted I",
        normalize("\u{130}stanbul") == "istanbul"
            && normalize("a\u{1}b") == "ab"
            && ('\0'..=char::MAX).all(|c| {
                let once = normalize(&format!("a{c}b"));
                normalize(&once) == once
            }),
    );
    // One batch that moves two terms of one list. Re-ranking after the whole batch binary-searches an unsorted list.
    let add = |l: &[(&str, u32)]| Op::Add(l.iter().map(|(t, c)| (t.to_string(), *c)).collect());
    let mut idx = Index::new();
    idx.apply(&add(&[("za", 100), ("zb", 90), ("zc", 80), ("zd", 70)]), 0);
    idx.apply(&add(&[("ze", 95), ("zc", 1000)]), 0);
    check(
        "a batch that moves several terms keeps every trie list sorted",
        ["", "z"]
            .iter()
            .all(|q| ids(&idx.search(q, 20)) == oracle(&idx.terms, q, 20)),
    );
    // 2000 half-lives (83 days of uptime): 2^2000 does not fit in an f64.
    let late = Arc::new(ManualClock::new(2_000 * HALF_LIFE as u64));
    let old = Engine::new(
        StoreKind::RwLock,
        late.clone(),
        8,
        vec![("aa".into(), 1), ("ab".into(), 5), ("ac".into(), 0)],
    );
    old.apply(Op::Event("aa".into()));
    let shown: Vec<(String, f64)> = old
        .suggest("a", 3)
        .hits
        .iter()
        .map(|h| (h.term.clone(), score_at(h.weight, late.now())))
        .collect();
    check(
        "popularity stays finite and ordered after 2000 half-lives",
        shown.iter().map(|s| s.0.as_str()).collect::<Vec<_>>() == ["ab", "aa", "ac"]
            && (shown[0].1 - 5.0).abs() < 1e-6
            && (shown[1].1 - 2.0).abs() < 1e-6
            && shown[2].1 == 0.0,
    );

    // The late reader of a snapshot store, replayed step by step: search, then a write, then the store of the answer.
    let racy = Engine::new(
        StoreKind::Snapshot,
        Arc::new(ManualClock::new(0)),
        8,
        vec![("aa".into(), 5), ("ab".into(), 1)],
    );
    let (old_epoch, old_hits) = racy.store.with(|i| (i.epoch, Arc::new(i.search("a", 5))));
    racy.apply(Op::Add(vec![("ac".into(), 9)]));
    racy.remember(("a".into(), 5), old_hits, old_epoch);
    let after = racy.suggest("a", 5);
    check(
        "an answer computed before a write is not stored after it",
        !after.cached && ids(&after.hits) == racy.oracle_now("a", 5),
    );

    // ---- 2. exact equality with a brute-force oracle
    println!(
        "\n[2] 5000 random terms, 2000 random queries: engine == brute-force oracle (cold cache, then warm cache)"
    );
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let seed = gen_terms(&mut rng, 5000);
    let clock = Arc::new(ManualClock::new(1_000));
    let engine = Engine::new(StoreKind::RwLock, clock.clone(), 512, seed.clone());
    for _ in 0..3000 {
        clock.advance(rng.below(40) as u64);
        let t = &seed[rng.below(seed.len())].0;
        engine.apply(Op::Event(t.clone()));
    }
    check(
        "5000 distinct normalized terms indexed",
        engine.term_count() == 5000,
    );
    let terms = engine.terms_clone();
    let queries: Vec<(String, usize)> = (0..2000)
        .map(|_| (gen_query(&mut rng, &terms), 1 + rng.below(20)))
        .collect();
    let (mut with_typo, mut with_infix, mut non_ascii, mut empty, mut compared) =
        (0, 0, 0, 0, 0usize);
    let t0 = Instant::now();
    for (q, limit) in &queries {
        let want = oracle(&terms, q, *limit);
        let cold = engine.suggest(q, *limit);
        assert_eq!(ids(&cold.hits), want, "query {q:?} limit {limit}");
        let warm = engine.suggest(q, *limit);
        assert_eq!(ids(&warm.hits), want, "warm query {q:?}");
        assert!(
            warm.cached,
            "the same query again must come from the cache: {q:?}"
        );
        with_typo += want.iter().any(|h| h.1 == Kind::Typo) as usize;
        with_infix += want.iter().any(|h| h.1 == Kind::Infix) as usize;
        non_ascii += (!q.is_ascii()) as usize;
        empty += want.is_empty() as usize;
        compared += want.len();
    }
    println!(
        "  2000 queries compared in {:?}: {with_typo} had typo matches, {with_infix} infix matches, {non_ascii} non-ASCII queries, {empty} empty results, {compared} hits compared",
        t0.elapsed()
    );
    check(
        "engine equals the oracle on all 2000 queries (ids, kinds, distances, order)",
        true,
    );
    check(
        "the test is not vacuous: typo, infix and non-ASCII cases all occurred",
        with_typo >= 50 && with_infix >= 50 && non_ascii >= 100 && compared > 5000,
    );
    let st = &engine.stats;
    println!(
        "  cache: hits={} misses={} (second run of every query should hit unless evicted)",
        st.hits.load(SeqCst),
        st.misses.load(SeqCst)
    );
    check("warm lookups hit the LRU", st.hits.load(SeqCst) >= 1500);

    // ---- 3. cache invalidation correctness under random updates
    println!(
        "\n[3] cache invalidation: 400 random events interleaved with queries on 40 hot (cached) prefixes"
    );
    let mut hot: Vec<String> = Vec::new();
    for i in 0..40usize {
        let stem: String = terms.list[rng.below(5000)]
            .norm
            .chars()
            .take(if i % 8 == 7 { 4 } else { 1 + i % 3 })
            .collect();
        hot.push(if i % 8 == 7 {
            mutate(&mut rng, &stem)
        } else {
            stem
        });
    }
    for q in &hot {
        engine.suggest(q, 10);
    }
    let (inv0, hits0) = (st.invalidated.load(SeqCst), st.hits.load(SeqCst));
    for _ in 0..400 {
        clock.advance(1 + rng.below(500) as u64);
        let target = if rng.below(2) == 0 {
            // pick a term the hot query actually matches, so the change MUST invalidate that entry
            let q = &hot[rng.below(hot.len())];
            engine
                .suggest(q, 10)
                .hits
                .first()
                .map(|h| h.term.clone())
                .unwrap_or_else(|| seed[0].0.clone())
        } else {
            seed[rng.below(seed.len())].0.clone()
        };
        engine.apply(Op::Event(target));
        for _ in 0..3 {
            let q = &hot[rng.below(hot.len())];
            let got = engine.suggest(q, 10);
            assert_eq!(
                ids(&got.hits),
                engine.oracle_now(q, 10),
                "stale or wrong answer for {q:?} (cached={})",
                got.cached
            );
        }
    }
    println!(
        "  entries invalidated: {}, extra cache hits during the run: {}",
        st.invalidated.load(SeqCst) - inv0,
        st.hits.load(SeqCst) - hits0
    );
    check(
        "no stale answer in 1200 post-update queries; invalidation happened and unrelated entries survived",
        st.invalidated.load(SeqCst) - inv0 > 50 && st.hits.load(SeqCst) - hits0 > 100,
    );

    // ---- 4. choose the store: RwLock vs copy-on-write snapshot
    println!(
        "\n[4] RwLock vs snapshot swap: 4 reader threads + 1 writer (1 event/ms) for 300 ms each, 5000 terms, cache off"
    );
    let probes: Vec<String> = (0..256).map(|_| gen_query(&mut rng, &terms)).collect();
    let mut results = Vec::new();
    for kind in [StoreKind::RwLock, StoreKind::Snapshot] {
        let eng = Arc::new(Engine::new(
            kind,
            Arc::new(ManualClock::new(5)),
            0,
            seed.clone(),
        ));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let readers: Vec<_> = (0..4)
            .map(|r| {
                let (eng, stop, probes) = (eng.clone(), stop.clone(), probes.clone());
                thread::spawn(move || {
                    let (mut n, mut i, lat) = (0u64, r * 17, Histogram::new());
                    while !stop.load(SeqCst) {
                        let t = Instant::now();
                        eng.suggest(&probes[i % probes.len()], 10);
                        lat.record(t.elapsed().as_micros() as u64);
                        (n, i) = (n + 1, i + 1);
                    }
                    (n, lat.quantile(0.99))
                })
            })
            .collect();
        let writer = {
            let (eng, stop, seed) = (eng.clone(), stop.clone(), seed.clone());
            thread::spawn(move || {
                let mut n = 0usize;
                while !stop.load(SeqCst) {
                    eng.apply(Op::Event(seed[n % seed.len()].0.clone()));
                    n += 1;
                    thread::sleep(Duration::from_millis(1));
                }
                n
            })
        };
        thread::sleep(Duration::from_millis(300));
        stop.store(true, SeqCst);
        let rs: Vec<(u64, u64)> = readers
            .into_iter()
            .map(|h| h.join().expect("reader"))
            .collect();
        let writes = writer.join().expect("writer");
        let (reads, p99) = (
            rs.iter().map(|r| r.0).sum::<u64>(),
            rs.iter().map(|r| r.1).max().unwrap(),
        );
        println!(
            "  {:<9} reads/s={:>8.0}  writes/s={:>6.0}  reader p99={} us",
            format!("{kind:?}"),
            reads as f64 / 0.3,
            writes as f64 / 0.3,
            p99
        );
        results.push((kind, reads as f64, writes as f64));
    }
    let (best_r, best_w) = (
        results.iter().map(|r| r.1).fold(0.0, f64::max),
        results.iter().map(|r| r.2).fold(0.0, f64::max),
    );
    let chosen = results
        .iter()
        .max_by(|a, b| {
            (a.1 / best_r + a.2 / best_w)
                .partial_cmp(&(b.1 / best_r + b.2 / best_w))
                .unwrap()
        })
        .unwrap()
        .0;
    println!(
        "  chosen for the service (best combined read+write throughput): {chosen:?}   [machine- and workload-dependent: a snapshot write clones the whole index, an RwLock write blocks readers briefly]"
    );
    let (a, b) = (
        Engine::new(
            StoreKind::RwLock,
            Arc::new(ManualClock::new(9)),
            64,
            seed[..800].to_vec(),
        ),
        Engine::new(
            StoreKind::Snapshot,
            Arc::new(ManualClock::new(9)),
            64,
            seed[..800].to_vec(),
        ),
    );
    for i in 0..150 {
        let op = if i % 5 == 0 {
            Op::Add(vec![(format!("zq{i}x"), 1 + i as u32)])
        } else {
            Op::Event(seed[(i * 7) % 800].0.clone())
        };
        a.apply(op.clone());
        b.apply(op);
    }
    check(
        "both store implementations give identical answers after the same 150 writes",
        probes
            .iter()
            .take(120)
            .all(|q| ids(&a.suggest(q, 10).hits) == ids(&b.suggest(q, 10).hits))
            && a.epoch() == b.epoch(),
    );

    // ---- 5. the HTTP service: transcript
    println!("\n[5] HTTP transcript on a curated dictionary (store = {chosen:?}, manual clock)");
    let clock = Arc::new(ManualClock::new(0));
    let svc = Arc::new(Engine::new(chosen, clock.clone(), 256, curated()));
    let http = serve(svc.clone()).await;
    let base = format!("http://{}", http.addr);
    let c = client();
    let v = show(&c, &base, "ru", 5).await;
    check(
        "q=ru -> rust, ruby, rustup, rustc, runtime (prefix, by popularity)",
        names(&v) == ["rust", "ruby", "rustup", "rustc", "runtime"]
            && v["suggestions"][0]["kind"] == "prefix",
    );
    let v = show(&c, &base, "rsut", 5).await;
    check(
        "q=rsut (transposed typo) -> rust",
        names(&v).first().map(String::as_str) == Some("rust")
            && v["suggestions"][0]["kind"] == "typo"
            && v["suggestions"][0]["dist"] == 1,
    );
    let v = show(&c, &base, "script", 5).await;
    check(
        "q=script (infix) -> javascript, typescript",
        names(&v) == ["javascript", "typescript"] && v["suggestions"][0]["kind"] == "infix",
    );
    let v = show(&c, &base, "CAFÉ", 5).await;
    check(
        "q=CAFÉ (accent + case folded) -> café, cafeteria",
        names(&v) == ["café", "cafeteria"],
    );
    let v = show(&c, &base, "pyton", 5).await;
    check(
        "q=pyton (typo) -> python before pytorch",
        names(&v)[..2] == ["python", "pytorch"],
    );

    println!(
        "  -- popularity changes: 40 selections of ruby; the cached 'ru' answer must be invalidated"
    );
    show(&c, &base, "ru", 3).await; // primes the cache entry for (ru, 3)
    let v = show(&c, &base, "ru", 3).await;
    check(
        "second identical request is served from the cache",
        v["cached"] == true,
    );
    let unrelated = show(&c, &base, "py", 3).await; // prime another entry
    for _ in 0..40 {
        let r = c
            .post(format!("{base}/events"))
            .json(&json!({"term": "Ruby"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
    }
    let v = show(&c, &base, "ru", 3).await;
    check(
        "after 40 events ruby overtakes rust and the answer is fresh",
        names(&v)[..2] == ["ruby", "rust"] && v["cached"] == false,
    );
    let v2 = show(&c, &base, "py", 3).await;
    check(
        "an unrelated cached prefix survives the invalidation",
        v2["cached"] == true && names(&v2) == names(&unrelated) && v2["epoch"] == v["epoch"],
    );
    let r = c
        .post(format!("{base}/events"))
        .json(&json!({"term": "no such term"}))
        .send()
        .await
        .unwrap();
    check("event for an unknown term is a 404", r.status() == 404);
    let r: Value = c
        .post(format!("{base}/index"))
        .json(
            &json!({"terms": [{"term": "Rustacean", "count": 200}, {"term": "rust", "count": 1}]}),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    println!("  > POST /index [Rustacean x200, rust x1] -> {r}");
    let v = show(&c, &base, "rus", 3).await;
    check(
        "a newly indexed term is visible immediately (cache invalidated by the add)",
        names(&v)[0] == "Rustacean" && r["added"] == 1 && r["existing"] == 1,
    );
    let m = c
        .get(format!("{base}/metrics"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    println!(
        "  > GET /metrics\n{}",
        m.lines()
            .filter(|l| !l.starts_with('#'))
            .map(|l| format!("      {l}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    check(
        "metrics expose counters and latency quantiles",
        m.contains("typeahead_cache_hits_total")
            && m.contains("typeahead_suggest_latency_us{quantile=\"0.99\"}"),
    );
    let long = "r".repeat(MAX_TEXT_CHARS + 1);
    let many: Vec<Value> = (0..=MAX_BATCH)
        .map(|i| json!({"term": format!("t{i}")}))
        .collect();
    let before = svc.term_count();
    let refused = [
        c.get(format!("{base}/suggest"))
            .query(&[("q", long.as_str())])
            .send()
            .await
            .unwrap()
            .status(),
        c.post(format!("{base}/events"))
            .json(&json!({"term": long}))
            .send()
            .await
            .unwrap()
            .status(),
        c.post(format!("{base}/index"))
            .json(&json!({"terms": [{"term": "fine"}, {"term": long}]}))
            .send()
            .await
            .unwrap()
            .status(),
        c.post(format!("{base}/index"))
            .json(&json!({"terms": many}))
            .send()
            .await
            .unwrap()
            .status(),
    ];
    check(
        "over-long text and over-large batches are refused with 400 and index nothing",
        refused.iter().all(|s| *s == 400) && svc.term_count() == before,
    );
    drop(c);
    http.stop().await;

    // ---- 6. concurrency: 8 clients query while a writer mutates; every answer must be a valid snapshot
    println!(
        "\n[6] concurrency: 8 HTTP clients x 250 queries while 300 writes land; each answer is replayed against the oracle at its generation"
    );
    let clock = Arc::new(ManualClock::new(10_000));
    let eng = Arc::new(Engine::new(chosen, clock.clone(), 128, seed.clone()));
    let base_gen = eng.epoch();
    let base_terms = eng.terms_clone();
    let mut ops: Vec<(Op, u64)> = Vec::new();
    let mut t = 10_000u64;
    for k in 0..300usize {
        t += 1 + rng.below(300) as u64;
        ops.push(if k % 3 == 0 {
            (
                Op::Add(vec![(
                    format!("zq{}{}", k, SYL[k % SYL.len()]),
                    1 + (k % 9) as u32,
                )]),
                t,
            )
        } else {
            (Op::Event(seed[rng.below(seed.len())].0.clone()), t)
        });
    }
    let http = serve(eng.clone()).await;
    let base = format!("http://{}", http.addr);
    let client_queries: Vec<Vec<String>> = (0..8)
        .map(|i| {
            (0..250)
                .map(|j| {
                    if (i + j) % 9 == 0 {
                        ["zq", "zq1", "zqra", "zq2s"][j % 4].to_string()
                    } else {
                        gen_query(&mut rng, &base_terms)
                    }
                })
                .collect()
        })
        .collect();
    let wc = client();
    let wbase = base.clone();
    let wclock = clock.clone();
    let wops = ops.clone();
    let writer = tokio::spawn(async move {
        for (op, when) in wops {
            wclock.set(when);
            let r = match op {
                Op::Event(t) => wc.post(format!("{wbase}/events")).json(&json!({"term": t})).send().await,
                Op::Add(l) => wc.post(format!("{wbase}/index")).json(&json!({"terms": l.iter().map(|(t, c)| json!({"term": t, "count": c})).collect::<Vec<_>>()})).send().await,
            };
            assert_eq!(r.expect("write").status(), 200);
            tokio::task::yield_now().await;
        }
    });
    let started = Instant::now();
    let readers: Vec<_> = client_queries
        .into_iter()
        .map(|qs| {
            let (c, base) = (client(), base.clone());
            tokio::spawn(async move {
                let mut rec: Vec<(String, usize, u64, Vec<String>)> = Vec::new();
                let mut lat: Vec<u64> = Vec::new();
                for (j, q) in qs.iter().enumerate() {
                    let limit = 1 + j % 10;
                    let t0 = Instant::now();
                    let r = c
                        .get(format!("{base}/suggest"))
                        .query(&[("q", q.as_str()), ("limit", &limit.to_string())])
                        .send()
                        .await
                        .expect("suggest request");
                    assert_eq!(r.status(), 200);
                    let v: Value = r.json().await.expect("json");
                    lat.push(t0.elapsed().as_micros() as u64);
                    rec.push((q.clone(), limit, v["epoch"].as_u64().unwrap(), names(&v)));
                }
                (rec, lat)
            })
        })
        .collect();
    let mut recs = Vec::new();
    let mut lats = Vec::new();
    for r in readers {
        let (a, b) = r.await.expect("reader task panicked");
        recs.extend(a);
        lats.extend(b);
    }
    writer.await.expect("writer task panicked");
    let wall = started.elapsed();
    http.stop().await;
    check(
        "all 2000 reads and 300 writes succeeded without a panic",
        recs.len() == 2000 && eng.epoch() == base_gen + 300,
    );
    lats.sort_unstable();
    let pct = |p: f64| lats[((lats.len() as f64 * p).ceil() as usize).clamp(1, lats.len()) - 1];
    println!(
        "  client-side round trip (reqwest+axum, loopback): p50={} us  p90={} us  p99={} us  max={} us; wall {:?}",
        pct(0.5),
        pct(0.9),
        pct(0.99),
        lats[lats.len() - 1],
        wall
    );
    println!(
        "  server-side handler latency (histogram at /metrics): p50={} us  p99={} us",
        eng.stats.latency.quantile(0.5),
        eng.stats.latency.quantile(0.99)
    );
    recs.sort_by_key(|r| r.2);
    let (mut terms_at, mut applied) = (base_terms, 0usize);
    let mut distinct_gens = HashSet::new();
    for (q, limit, epoch, got) in &recs {
        let k = (epoch - base_gen) as usize;
        assert!(
            k <= ops.len(),
            "response claims a generation from the future: {epoch}"
        );
        while applied < k {
            let changed = terms_at.apply(&ops[applied].0, ops[applied].1, |_, _, _| {});
            assert!(changed, "replayed op changes the index");
            applied += 1;
        }
        let want: Vec<String> = oracle(&terms_at, q, *limit)
            .iter()
            .map(|h| terms_at.list[h.0 as usize].text.clone())
            .collect();
        assert_eq!(
            got, &want,
            "answer for {q:?} limit {limit} at generation {epoch} is not the oracle's answer for that snapshot"
        );
        distinct_gens.insert(*epoch);
    }
    check(
        &format!(
            "every answer equals the oracle at the generation it reports ({} distinct snapshots observed)",
            distinct_gens.len()
        ),
        distinct_gens.len() >= 5,
    );
    let (h, mi) = (eng.stats.hits.load(SeqCst), eng.stats.misses.load(SeqCst));
    println!(
        "  cache during the run: {h} hits / {mi} misses, {} entries invalidated by the writer",
        eng.stats.invalidated.load(SeqCst)
    );

    println!("\nALL CHECKS PASSED");
}
