//! CAPSTONE 07 (search): a mini full-text search engine: positional postings, BM25, boolean/phrase/wildcard queries, segments
//! LEVEL: capstone
//! Use in: site/log/code search, embedded search in CLIs and services, learning how Lucene/tantivy work.   Run: cargo run --release --example search_engine
#![allow(dead_code)]
//!
//! WHAT: documents are tokenised and written into immutable SEGMENTS. Each segment holds (1) an inverted index whose
//! positional postings are delta + varint compressed byte strings, (2) a trigram index over the raw lowercase text for
//! substring and `*wildcard*` queries, and (3) per-document lengths. An `Index` is a list of segments plus global
//! statistics, so BM25 gives the same answer however documents are split across segments.
//! LIMITS: everything is in memory; nothing is deleted or updated; `merge_all` is called by hand and folds every
//! segment into one, re-encoding shared terms once per segment (there is no background merge or merge policy). Every
//! query decodes its posting lists and builds whole doc sets. Doc ids and positions are `u32`. Text is not Unicode
//! normalised, so a decomposed "é" is not the same as a composed one. `AND`, `OR` and `NOT` are keywords only in capitals.
//! A wildcard word matches anywhere in the raw text (across word boundaries), so `cast*` also finds "broadcast"; it
//! adds nothing to a score. A query may hold at most 256 tokens, which bounds the recursion of parse, eval and drop.
//! WHY: delta coding turns large, increasing ids into small numbers, and varints store small numbers in one byte, so
//! postings shrink several-fold versus `Vec<u32>`. Immutable segments make incremental indexing cheap (build a new
//! small segment, never rewrite old ones); a MERGE folds segments together so a query visits fewer of them.
//! HOW: a boolean query string is parsed (`NOT` > `AND` > `OR`, parentheses, "quoted phrases", `*wildcards*`) into an
//! `Expr`; evaluation yields a sorted doc set (phrases check consecutive positions, wildcards use trigram
//! filter-then-verify); matches are scored with BM25 over the terms under an even number of NOTs, and the best k are
//! kept in a min-heap.
//! COMMON USES: product/site search (Lucene, Elasticsearch, tantivy), log search with substring filters (trigram
//! indexes as in Google Code Search), IDE "find in files", email/notes apps, autocomplete backends; patterns:
//! inverted index, posting compression, LSM-style immutable segments + merge, filter-then-verify, top-k heap.
//! Read: https://github.com/quickwit-oss/tantivy, https://swtch.com/~rsc/regexp/regexp4.html, https://en.wikipedia.org/wiki/Okapi_BM25
use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap};

// ---------- tokenizer ----------

/// Lowercase one char at a time, for substring search. `str::to_lowercase` looks at neighbours (a Greek capital sigma
/// becomes a different letter at the end of a word), so a lowercased needle could stop being part of the lowercased text.
fn fold(text: &str) -> String {
    text.chars().flat_map(char::to_lowercase).collect()
}

/// Lowercase and split on every non-alphanumeric char. Working on `char`s (not bytes) keeps "café" one token.
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

// ---------- varint + delta posting codec ----------

/// LEB128 varint (see bytes): 7 bits per byte, high bit = continuation.
fn put_varint(mut n: u32, out: &mut Vec<u8>) {
    while n >= 0x80 {
        out.push((n & 0x7f) as u8 | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
}

fn get_varint(buf: &[u8], pos: &mut usize) -> Result<u32, &'static str> {
    let mut value = 0u32;
    for shift in (0..35).step_by(7) {
        let b = *buf.get(*pos).ok_or("truncated varint")?;
        *pos += 1;
        if shift == 28 && b > 0x0f {
            return Err("varint overflows u32"); // the fifth byte has room for 4 more bits only
        }
        value |= u32::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err("varint too long")
}

/// One document's occurrences of a term. `tf` is just `positions.len()`.
#[derive(Debug, Clone, PartialEq)]
struct Posting {
    doc: u32,
    positions: Vec<u32>,
}

/// Layout: `n_docs, then per doc: doc_delta, tf, pos_delta * tf`. Both docs and positions are ascending, so
/// every stored number is a small gap rather than a large absolute value.
fn encode_postings(list: &[Posting]) -> Vec<u8> {
    let mut out = Vec::new();
    put_varint(list.len() as u32, &mut out);
    let mut prev_doc = 0;
    for p in list {
        put_varint(p.doc - prev_doc, &mut out);
        prev_doc = p.doc;
        put_varint(p.positions.len() as u32, &mut out);
        let mut prev_pos = 0;
        for &pos in &p.positions {
            put_varint(pos - prev_pos, &mut out);
            prev_pos = pos;
        }
    }
    out
}

/// Adds a decoded gap. Only the first gap of a list may be 0: later ones must move strictly forward.
fn add_gap(prev: u32, gap: u32, first: bool) -> Result<u32, &'static str> {
    if gap == 0 && !first {
        return Err("values are not ascending");
    }
    prev.checked_add(gap).ok_or("value overflows u32")
}

fn decode_postings(buf: &[u8]) -> Result<Vec<Posting>, &'static str> {
    let mut pos = 0;
    let n = get_varint(buf, &mut pos)?;
    let (mut doc, mut list) = (0u32, Vec::new());
    for i in 0..n {
        doc = add_gap(doc, get_varint(buf, &mut pos)?, i == 0)?;
        let tf = get_varint(buf, &mut pos)?;
        if tf == 0 {
            return Err("posting without positions");
        }
        let (mut p, mut positions) = (0u32, Vec::new());
        for j in 0..tf {
            p = add_gap(p, get_varint(buf, &mut pos)?, j == 0)?;
            positions.push(p);
        }
        list.push(Posting { doc, positions });
    }
    if pos == buf.len() {
        Ok(list)
    } else {
        Err("trailing bytes")
    }
}

/// What the same postings would cost as a flat `Vec<u32>` of `[doc, tf, positions...]`.
fn plain_size(list: &[Posting]) -> usize {
    list.iter().map(|p| 4 * (2 + p.positions.len())).sum()
}

// ---------- wildcard / substring helpers ----------

/// `*` splits a pattern into literal pieces that must occur in order ("contains" semantics at both ends).
fn pieces(pattern: &str) -> Vec<&str> {
    pattern.split('*').filter(|p| !p.is_empty()).collect()
}

/// Verification step: the ground truth the trigram filter must never contradict.
fn wildcard_match(text: &str, pattern: &str) -> bool {
    let mut rest = text;
    for piece in pieces(pattern) {
        match rest.find(piece) {
            Some(i) => rest = &rest[i + piece.len()..],
            None => return false,
        }
    }
    true
}

fn intersect_sorted(a: &[u32], b: &[u32]) -> Vec<u32> {
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            Ordering::Less => i += 1,
            Ordering::Greater => j += 1,
            Ordering::Equal => {
                out.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out
}

// ---------- segment: immutable index over a contiguous range of doc ids ----------

#[derive(Debug, Clone, PartialEq)]
struct Segment {
    base: u32,                            // global id of the first doc
    doc_len: Vec<u32>,                    // tokens per doc (for BM25 length normalisation)
    texts: Vec<String>,                   // lowercase text, needed to verify trigram candidates
    terms: HashMap<String, Vec<u8>>,      // term -> compressed positional postings
    trigrams: HashMap<[u8; 3], Vec<u32>>, // byte trigram -> sorted global doc ids
}

impl Segment {
    fn build(base: u32, docs: &[String]) -> Segment {
        let mut seg = Segment {
            base,
            doc_len: Vec::new(),
            texts: Vec::new(),
            terms: HashMap::new(),
            trigrams: HashMap::new(),
        };
        let mut acc: HashMap<String, Vec<Posting>> = HashMap::new();
        for (i, text) in docs.iter().enumerate() {
            let id = base + i as u32;
            let tokens = tokenize(text);
            let mut by_term: HashMap<&str, Vec<u32>> = HashMap::new();
            for (pos, tok) in tokens.iter().enumerate() {
                by_term.entry(tok).or_default().push(pos as u32);
            }
            for (term, positions) in by_term {
                acc.entry(term.to_string())
                    .or_default()
                    .push(Posting { doc: id, positions });
            }
            let lower = fold(text);
            let tris: BTreeSet<[u8; 3]> = lower
                .as_bytes()
                .windows(3)
                .map(|w| [w[0], w[1], w[2]])
                .collect();
            for t in tris {
                seg.trigrams.entry(t).or_default().push(id); // ids arrive ascending
            }
            seg.doc_len.push(tokens.len() as u32);
            seg.texts.push(lower);
        }
        seg.terms = acc
            .into_iter()
            .map(|(t, l)| (t, encode_postings(&l)))
            .collect();
        seg
    }

    fn end(&self) -> u32 {
        self.base + self.doc_len.len() as u32
    }

    fn postings(&self, term: &str) -> Vec<Posting> {
        self.terms
            .get(term)
            .map(|b| decode_postings(b).expect("segment postings are self-produced"))
            .unwrap_or_default()
    }

    /// Merge two ADJACENT segments. Doc ranges are disjoint and ordered, so postings simply concatenate;
    /// we re-encode because the first delta of the second list must now be relative to the previous doc.
    fn merge(a: &Segment, b: &Segment) -> Segment {
        assert_eq!(a.end(), b.base, "only adjacent segments can merge");
        let mut m = a.clone();
        m.doc_len.extend(&b.doc_len);
        m.texts.extend(b.texts.iter().cloned());
        for (term, bytes) in &b.terms {
            let mut list = m.postings(term);
            list.extend(decode_postings(bytes).expect("valid postings"));
            m.terms.insert(term.clone(), encode_postings(&list));
        }
        for (tri, ids) in &b.trigrams {
            m.trigrams.entry(*tri).or_default().extend(ids);
        }
        m
    }

    /// Trigram filter then verify. Returns (#candidates after filtering, verified matches).
    fn wildcard(&self, pattern: &str) -> (usize, Vec<u32>) {
        let mut lists: Vec<Option<&Vec<u32>>> = Vec::new();
        for piece in pieces(pattern) {
            for w in piece.as_bytes().windows(3) {
                lists.push(self.trigrams.get(&[w[0], w[1], w[2]]));
            }
        }
        let mut cands: Vec<u32> = if lists.is_empty() {
            (self.base..self.end()).collect() // pieces shorter than 3 bytes: the index cannot help
        } else if lists.iter().any(Option::is_none) {
            Vec::new() // a required trigram occurs nowhere
        } else {
            let mut ls: Vec<&Vec<u32>> = lists.into_iter().flatten().collect();
            ls.sort_by_key(|l| l.len()); // start from the rarest trigram: smallest intermediate sets
            let mut acc = ls[0].clone();
            for l in &ls[1..] {
                acc = intersect_sorted(&acc, l);
            }
            acc
        };
        let n = cands.len();
        cands.retain(|&id| wildcard_match(&self.texts[(id - self.base) as usize], pattern));
        (n, cands)
    }
}

// ---------- query language ----------

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Empty, // a word that tokenises to nothing: matches no document
    Term(String),
    Phrase(Vec<String>),
    Wild(String),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
}

/// Parsing, evaluating and dropping an `Expr` all recurse, and each level of the tree uses up at least one token.
const MAX_TOKENS: usize = 256;

#[derive(Debug, thiserror::Error, PartialEq)]
enum QueryError {
    #[error("query has more than {MAX_TOKENS} tokens")]
    TooLong,
    #[error("unterminated quote")]
    UnterminatedQuote,
    #[error("unexpected end of query")]
    UnexpectedEnd,
    #[error("unexpected token {0:?}")]
    Unexpected(String),
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    LParen,
    RParen,
    Quoted(String),
    Word(String),
}

fn lex(q: &str) -> Result<Vec<Tok>, QueryError> {
    let mut chars = q.chars().peekable();
    let mut toks = Vec::new();
    while let Some(&c) = chars.peek() {
        if toks.len() > MAX_TOKENS {
            break;
        }
        match c {
            '(' | ')' => {
                chars.next();
                toks.push(if c == '(' { Tok::LParen } else { Tok::RParen });
            }
            '"' => {
                chars.next();
                let mut s = String::new();
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some(c) => s.push(c),
                        None => return Err(QueryError::UnterminatedQuote),
                    }
                }
                toks.push(Tok::Quoted(s));
            }
            c if c.is_whitespace() => {
                chars.next();
            }
            _ => {
                let mut w = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || c == '(' || c == ')' || c == '"' {
                        break;
                    }
                    w.push(c);
                    chars.next();
                }
                toks.push(Tok::Word(w));
            }
        }
    }
    if toks.len() > MAX_TOKENS {
        return Err(QueryError::TooLong);
    }
    Ok(toks)
}

/// Recursive descent, one function per precedence level: or < and < not < atom.
struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek_word(&self, kw: &str) -> bool {
        matches!(self.toks.get(self.pos), Some(Tok::Word(w)) if w == kw)
    }

    fn or(&mut self) -> Result<Expr, QueryError> {
        let mut left = self.and()?;
        while self.peek_word("OR") {
            self.pos += 1;
            left = Expr::Or(Box::new(left), Box::new(self.and()?));
        }
        Ok(left)
    }

    fn and(&mut self) -> Result<Expr, QueryError> {
        let mut left = self.not()?;
        loop {
            if self.peek_word("AND") {
                self.pos += 1; // explicit AND
            } else if matches!(self.toks.get(self.pos), None | Some(Tok::RParen))
                || self.peek_word("OR")
            {
                return Ok(left);
            } // otherwise: adjacent operands mean implicit AND
            left = Expr::And(Box::new(left), Box::new(self.not()?));
        }
    }

    fn not(&mut self) -> Result<Expr, QueryError> {
        if self.peek_word("NOT") {
            self.pos += 1;
            return Ok(Expr::Not(Box::new(self.not()?)));
        }
        self.atom()
    }

    fn atom(&mut self) -> Result<Expr, QueryError> {
        let tok = self
            .toks
            .get(self.pos)
            .cloned()
            .ok_or(QueryError::UnexpectedEnd)?;
        self.pos += 1;
        match tok {
            Tok::LParen => {
                let e = self.or()?;
                match self.toks.get(self.pos) {
                    Some(Tok::RParen) => self.pos += 1,
                    Some(t) => return Err(QueryError::Unexpected(format!("{t:?}"))),
                    None => return Err(QueryError::UnexpectedEnd),
                }
                Ok(e)
            }
            Tok::RParen => Err(QueryError::Unexpected(")".into())),
            Tok::Word(w) if w == "AND" || w == "OR" => Err(QueryError::Unexpected(w)),
            Tok::Word(w) if w.contains('*') => Ok(Expr::Wild(fold(&w))),
            Tok::Word(w) | Tok::Quoted(w) => Ok(text_to_expr(&w)),
        }
    }
}

fn text_to_expr(text: &str) -> Expr {
    let mut toks = tokenize(text);
    match toks.len() {
        0 => Expr::Empty,
        1 => Expr::Term(toks.remove(0)),
        _ => Expr::Phrase(toks),
    }
}

fn parse(q: &str) -> Result<Expr, QueryError> {
    let mut p = Parser {
        toks: lex(q)?,
        pos: 0,
    };
    let e = p.or()?;
    match p.toks.get(p.pos) {
        None => Ok(e),
        Some(t) => Err(QueryError::Unexpected(format!("{t:?}"))),
    }
}

/// Terms that should contribute to ranking: those under an even number of NOTs (so `NOT NOT rust` ranks like `rust`).
fn positive_terms(e: &Expr, negated: bool, out: &mut BTreeSet<String>) {
    match e {
        Expr::Term(t) if !negated => {
            out.insert(t.clone());
        }
        Expr::Phrase(ts) if !negated => out.extend(ts.iter().cloned()),
        Expr::And(a, b) | Expr::Or(a, b) => {
            positive_terms(a, negated, out);
            positive_terms(b, negated, out);
        }
        Expr::Not(a) => positive_terms(a, !negated, out),
        Expr::Empty | Expr::Wild(_) | Expr::Term(_) | Expr::Phrase(_) => {}
    }
}

// ---------- index: segments + global statistics + search ----------

#[derive(Debug, Clone, Copy)]
struct Bm25 {
    k1: f64,
    b: f64,
}

/// Heap entry ordered by score, ties broken so the LOWER doc id is "greater" (deterministic output).
#[derive(Debug)]
struct Ranked {
    score: f64,
    doc: u32,
}
/// Equality must agree with `Ord`; a derived one would compare the floats with `==`, which disagrees on NaN and -0.0.
impl PartialEq for Ranked {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}
impl Eq for Ranked {}
impl PartialOrd for Ranked {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Ranked {
    fn cmp(&self, o: &Self) -> Ordering {
        self.score.total_cmp(&o.score).then(o.doc.cmp(&self.doc))
    }
}

/// Keep the k best in a MIN-heap (via `Reverse`): O(n log k), popping the current worst when it overflows.
fn top_k(scores: impl Iterator<Item = (u32, f64)>, k: usize) -> Vec<Ranked> {
    let mut heap = BinaryHeap::new();
    for (doc, score) in scores {
        heap.push(Reverse(Ranked { score, doc }));
        if heap.len() > k {
            heap.pop();
        }
    }
    heap.into_sorted_vec()
        .into_iter()
        .map(|Reverse(r)| r)
        .collect() // ascending Reverse = best first
}

struct Index {
    segments: Vec<Segment>,
    bm25: Bm25,
}

impl Index {
    fn new(bm25: Bm25) -> Index {
        Index {
            segments: Vec::new(),
            bm25,
        }
    }

    fn num_docs(&self) -> u32 {
        self.segments.last().map_or(0, Segment::end)
    }

    /// Incremental indexing: new documents become a new immutable segment.
    fn add_segment(&mut self, docs: &[String]) {
        let seg = Segment::build(self.num_docs(), docs);
        self.segments.push(seg);
    }

    /// Fold all segments into one (a real engine merges a few similar-sized ones at a time).
    fn merge_all(&mut self) {
        let mut it = std::mem::take(&mut self.segments).into_iter();
        if let Some(first) = it.next() {
            self.segments
                .push(it.fold(first, |acc, s| Segment::merge(&acc, &s)));
        }
    }

    fn doc_len(&self, id: u32) -> u32 {
        let seg = self
            .segments
            .iter()
            .find(|s| id >= s.base && id < s.end())
            .expect("doc id in range");
        seg.doc_len[(id - seg.base) as usize]
    }

    fn postings(&self, term: &str) -> Vec<Posting> {
        self.segments
            .iter()
            .flat_map(|s| s.postings(term))
            .collect()
    }

    fn all_docs(&self) -> BTreeSet<u32> {
        (0..self.num_docs()).collect()
    }

    fn phrase_docs(&self, terms: &[String]) -> BTreeSet<u32> {
        let lists: Vec<HashMap<u32, Vec<u32>>> = terms
            .iter()
            .map(|t| {
                self.postings(t)
                    .into_iter()
                    .map(|p| (p.doc, p.positions))
                    .collect()
            })
            .collect();
        let Some(first) = lists.first() else {
            return BTreeSet::new();
        };
        // A phrase matches where term i sits at position p + i; binary_search works because positions are sorted.
        first
            .iter()
            .filter(|(doc, starts)| {
                starts.iter().any(|&p| {
                    lists[1..].iter().zip(1u32..).all(|(l, i)| {
                        l.get(doc)
                            .is_some_and(|ps| ps.binary_search(&(p + i)).is_ok())
                    })
                })
            })
            .map(|(&doc, _)| doc)
            .collect()
    }

    /// Wildcard over all segments; also reports how many candidates the trigram filter let through.
    fn wildcard(&self, pattern: &str) -> (usize, BTreeSet<u32>) {
        let (mut cands, mut hits) = (0, BTreeSet::new());
        for s in &self.segments {
            let (c, h) = s.wildcard(pattern);
            cands += c;
            hits.extend(h);
        }
        (cands, hits)
    }

    fn eval(&self, e: &Expr) -> BTreeSet<u32> {
        match e {
            Expr::Empty => BTreeSet::new(),
            Expr::Term(t) => self.postings(t).iter().map(|p| p.doc).collect(),
            Expr::Phrase(ts) => self.phrase_docs(ts),
            Expr::Wild(p) => self.wildcard(p).1,
            Expr::And(a, b) => self.eval(a).intersection(&self.eval(b)).copied().collect(),
            Expr::Or(a, b) => self.eval(a).union(&self.eval(b)).copied().collect(),
            Expr::Not(a) => self.all_docs().difference(&self.eval(a)).copied().collect(),
        }
    }

    /// BM25 per term: idf * tf*(k1+1) / (tf + k1*(1 - b + b*len/avglen)). Statistics are GLOBAL (all segments),
    /// otherwise the same document would score differently depending on how the corpus was segmented.
    fn bm25_scores(&self, terms: &BTreeSet<String>, matched: &BTreeSet<u32>) -> HashMap<u32, f64> {
        let n = f64::from(self.num_docs());
        let total_len: u32 = self.segments.iter().flat_map(|s| s.doc_len.iter()).sum();
        let avg_len = f64::from(total_len) / n.max(1.0);
        let Bm25 { k1, b } = self.bm25;
        let mut scores: HashMap<u32, f64> = matched.iter().map(|&d| (d, 0.0)).collect();
        for term in terms {
            let list = self.postings(term);
            let idf = (1.0 + (n - list.len() as f64 + 0.5) / (list.len() as f64 + 0.5)).ln();
            for p in list {
                if let Some(s) = scores.get_mut(&p.doc) {
                    let tf = p.positions.len() as f64;
                    let norm = 1.0 - b + b * f64::from(self.doc_len(p.doc)) / avg_len;
                    *s += idf * tf * (k1 + 1.0) / (tf + k1 * norm);
                }
            }
        }
        scores
    }

    fn search(&self, query: &str, k: usize) -> Result<Vec<Ranked>, QueryError> {
        let expr = parse(query)?;
        let matched = self.eval(&expr);
        let mut terms = BTreeSet::new();
        positive_terms(&expr, false, &mut terms);
        let scores = self.bm25_scores(&terms, &matched);
        Ok(top_k(scores.into_iter(), k))
    }

    /// (compressed bytes, plain Vec<u32> bytes) over every posting list.
    fn posting_sizes(&self) -> (usize, usize) {
        let mut sizes = (0, 0);
        for s in &self.segments {
            for (term, bytes) in &s.terms {
                sizes.0 += bytes.len();
                sizes.1 += plain_size(&s.postings(term));
            }
        }
        sizes
    }
}

// ---------- oracle (brute force, shares no index code) ----------

fn oracle_match(e: &Expr, tokens: &[String], text: &str) -> bool {
    match e {
        Expr::Empty => false,
        Expr::Term(t) => tokens.contains(t),
        Expr::Phrase(ts) => tokens.windows(ts.len()).any(|w| w == ts.as_slice()),
        Expr::Wild(p) => oracle_wildcard(&fold(text), p),
        Expr::And(a, b) => oracle_match(a, tokens, text) && oracle_match(b, tokens, text),
        Expr::Or(a, b) => oracle_match(a, tokens, text) || oracle_match(b, tokens, text),
        Expr::Not(a) => !oracle_match(a, tokens, text),
    }
}

/// A second wildcard matcher (a table over chars, with a `*` added at both ends), so the oracle does not call the code
/// that the index uses to verify candidates.
fn oracle_wildcard(text: &str, pattern: &str) -> bool {
    let pattern: Vec<char> = format!("*{pattern}*").chars().collect();
    // reach[i]: the first i pattern chars can match the text read so far
    let mut reach = vec![false; pattern.len() + 1];
    reach[0] = true;
    for i in 0..pattern.len() {
        reach[i + 1] = reach[i] && pattern[i] == '*';
    }
    for c in text.chars() {
        let mut next = vec![false; pattern.len() + 1];
        for (i, &p) in pattern.iter().enumerate() {
            next[i + 1] = if p == '*' {
                next[i] || reach[i + 1]
            } else {
                p == c && reach[i]
            };
        }
        reach = next;
    }
    reach[pattern.len()]
}

fn oracle_scan(docs: &[String], e: &Expr) -> BTreeSet<u32> {
    (0..docs.len() as u32)
        .filter(|&i| oracle_match(e, &tokenize(&docs[i as usize]), &docs[i as usize]))
        .collect()
}

// ---------- deterministic corpus and query generation ----------

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const VOCAB: [&str; 24] = [
    "rust",
    "async",
    "tokio",
    "memory",
    "ordering",
    "atomic",
    "cast",
    "broadcast",
    "forecast",
    "typecast",
    "index",
    "search",
    "trigram",
    "segment",
    "merge",
    "query",
    "score",
    "token",
    "stream",
    "lock",
    "thread",
    "future",
    "café",
    "pin",
];

/// Random prose with capitals and punctuation so the tokenizer has real work to do.
fn gen_docs(rng: &mut Rng, n: usize) -> Vec<String> {
    (0..n)
        .map(|_| {
            let words: Vec<String> = (0..5 + rng.below(20))
                .map(|_| {
                    let w = VOCAB[rng.below(VOCAB.len())];
                    match rng.below(6) {
                        0 => w.to_uppercase(),
                        1 => format!("{w},"),
                        _ => w.to_string(),
                    }
                })
                .collect();
            words.join(" ")
        })
        .collect()
}

fn gen_query(rng: &mut Rng, depth: u32) -> String {
    let word = |rng: &mut Rng| VOCAB[rng.below(VOCAB.len())];
    match if depth == 0 {
        rng.below(3)
    } else {
        rng.below(7)
    } {
        0 => word(rng).to_string(),
        1 => format!("\"{} {}\"", word(rng), word(rng)),
        2 => format!("*{}*", &word(rng)[..3]),
        3 | 4 => {
            let op = ["AND", "OR", ""][rng.below(3)];
            format!(
                "({} {op} {})",
                gen_query(rng, depth - 1),
                gen_query(rng, depth - 1)
            )
        }
        _ => format!("NOT {}", gen_query(rng, depth - 1)),
    }
}

// ---------- verification ----------

fn hand_built_bm25() {
    let docs: Vec<String> = [
        "rust rust cat",
        "rust dog cat",
        "rust dog cat bird fish",
        "dog cat",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let mut idx = Index::new(Bm25 { k1: 1.2, b: 0.75 });
    idx.add_segment(&docs);
    let hits = idx.search("rust", 10).unwrap();
    let order: Vec<u32> = hits.iter().map(|h| h.doc).collect();
    // doc0: tf=2, short; doc1: tf=1, short; doc2: tf=1 but longer => strictly decreasing scores.
    assert_eq!(
        order,
        vec![0, 1, 2],
        "more occurrences and shorter length must rank higher"
    );
    assert!(hits[0].score > hits[1].score && hits[1].score > hits[2].score);
    // b = 0 disables length normalisation: equal tf then ties, proving b controls the length effect.
    let mut flat = Index::new(Bm25 { k1: 1.2, b: 0.0 });
    flat.add_segment(&docs);
    let h = flat.search("rust", 10).unwrap();
    assert!((h[1].score - h[2].score).abs() < 1e-12);
}

fn check_codec(rng: &mut Rng) {
    for _ in 0..200 {
        let (mut doc, mut list) = (rng.below(3) as u32, Vec::new());
        for _ in 0..rng.below(40) {
            let (mut p, mut positions) = (rng.below(5) as u32, Vec::new());
            for _ in 0..1 + rng.below(8) {
                positions.push(p);
                let span = if rng.below(10) == 0 { 100_000 } else { 30 };
                p += 1 + rng.below(span) as u32;
            }
            list.push(Posting { doc, positions });
            doc += 1 + rng.below(1000) as u32;
        }
        let bytes = encode_postings(&list);
        assert_eq!(
            decode_postings(&bytes).unwrap(),
            list,
            "postings must round-trip exactly"
        );
        if !bytes.is_empty() {
            assert!(decode_postings(&bytes[..bytes.len() - 1]).is_err() || list.is_empty());
        }
    }
}

fn check_queries(idx: &Index, docs: &[String], rng: &mut Rng) -> usize {
    let mut non_empty = 0;
    let fixed = [
        "rust AND async",
        "rust OR tokio",
        "rust NOT async",
        "(rust OR tokio) AND NOT (lock OR pin)",
        "NOT NOT rust",
    ];
    let random: Vec<String> = (0..400).map(|_| gen_query(rng, 3)).collect();
    for q in fixed.iter().map(|s| s.to_string()).chain(random) {
        let expr = parse(&q).unwrap_or_else(|e| panic!("query {q:?} failed to parse: {e}"));
        let got = idx.eval(&expr);
        assert_eq!(got, oracle_scan(docs, &expr), "boolean mismatch for {q}");
        non_empty += usize::from(!got.is_empty());
    }
    non_empty
}

fn check_phrases(idx: &Index, docs: &[String], rng: &mut Rng) -> usize {
    let mut hits = 0;
    for i in 0..300 {
        // Half the phrases are lifted from a real document (guaranteed hits), half are random word pairs.
        let words: Vec<String> = if i % 2 == 0 {
            let toks = tokenize(&docs[rng.below(docs.len())]);
            let start = rng.below(toks.len().saturating_sub(2).max(1));
            toks[start..(start + 2 + rng.below(2)).min(toks.len())].to_vec()
        } else {
            (0..2)
                .map(|_| VOCAB[rng.below(VOCAB.len())].to_string())
                .collect()
        };
        let got = idx.phrase_docs(&words);
        let naive: BTreeSet<u32> = (0..docs.len() as u32)
            .filter(|&d| {
                tokenize(&docs[d as usize])
                    .windows(words.len())
                    .any(|w| w == words.as_slice())
            })
            .collect();
        assert_eq!(got, naive, "phrase mismatch for {words:?}");
        hits += usize::from(!got.is_empty());
    }
    hits
}

/// Input that an earlier version accepted wrongly, panicked on or crashed on.
fn check_hostile_input() {
    for q in ["rust \"", "\"a\" \"", "\"", "\"a"] {
        assert_eq!(parse(q), Err(QueryError::UnterminatedQuote), "{q:?}");
    }
    let mut at = 0;
    assert!(
        get_varint(&[0xff, 0xff, 0xff, 0xff, 0x7f], &mut at).is_err(),
        "bits above 31 must not vanish"
    );
    let mut at = 0;
    assert_eq!(
        get_varint(&[0xff, 0xff, 0xff, 0xff, 0x0f], &mut at),
        Ok(u32::MAX)
    );
    assert!(
        decode_postings(&[2, 0xff, 0xff, 0xff, 0xff, 0x0f, 1, 0, 5, 1, 0]).is_err(),
        "doc overflow"
    );
    assert!(
        decode_postings(&[1, 0, 2, 0xff, 0xff, 0xff, 0xff, 0x0f, 5]).is_err(),
        "position overflow"
    );
    assert!(
        decode_postings(&[2, 3, 1, 0, 0, 1, 0]).is_err(),
        "the same doc twice"
    );
    assert!(
        decode_postings(&[1, 3, 2, 4, 0]).is_err(),
        "the same position twice"
    );
    assert!(
        decode_postings(&[1, 3, 0]).is_err(),
        "a posting without positions"
    );
    // each of these once overflowed the stack (which aborts the process)
    for q in [
        "(".repeat(200_000),
        "NOT ".repeat(200_000) + "a",
        "a ".repeat(200_000),
    ] {
        assert_eq!(parse(&q), Err(QueryError::TooLong));
    }
    let widest = "a ".repeat(MAX_TOKENS);
    let deepest = "NOT ".repeat(MAX_TOKENS - 1) + "a";
    let mut idx = Index::new(Bm25 { k1: 1.2, b: 0.75 });
    idx.add_segment(&[
        "rust a b c".to_string(),
        "rust rust".to_string(),
        "ΟΔΟΣΤΡΩΜΑ".to_string(),
    ]);
    assert!(idx.search(&widest, 3).is_ok() && idx.search(&deepest, 3).is_ok());
    assert_eq!(
        parse(&"a ".repeat(MAX_TOKENS + 1)),
        Err(QueryError::TooLong)
    );
    let twice = idx.search("NOT NOT rust", 3).unwrap();
    assert_eq!(
        twice,
        idx.search("rust", 3).unwrap(),
        "two NOTs cancel, in the scores too"
    );
    assert!(twice[0].score > 0.0);
    let found: Vec<u32> = idx
        .search("*ΟΔΟΣ*", 3)
        .unwrap()
        .iter()
        .map(|h| h.doc)
        .collect();
    assert_eq!(found, vec![2], "a capital sigma at the end of a needle");
    // a segment folds its pages one char at a time too: lowercased as a string, this page would end in the other sigma
    idx.add_segment(&["ΟΔΟΣ".to_string()]);
    assert_eq!(
        idx.wildcard("οδοσ").1,
        BTreeSet::from([2, 3]),
        "a page that ends in a capital sigma"
    );
    let nan = Ranked {
        score: f64::NAN,
        doc: 1,
    };
    assert!(
        nan == Ranked {
            score: f64::NAN,
            doc: 1
        }
    );
    assert!(
        Ranked { score: 0.0, doc: 1 }
            != Ranked {
                score: -0.0,
                doc: 1
            }
    );
}

fn check_substrings(idx: &Index, docs: &[String], rng: &mut Rng) -> f64 {
    let lower: Vec<String> = docs.iter().map(|d| fold(d)).collect();
    let mut needles: Vec<String> = [
        "cast", "ast", "ord rin", "ing", "rust a", "xy", "é", "zzz", ", ",
    ]
    .map(String::from)
    .to_vec();
    for _ in 0..300 {
        let d = &lower[rng.below(lower.len())];
        let bounds: Vec<usize> = d.char_indices().map(|(i, _)| i).chain([d.len()]).collect();
        let (a, b) = (rng.below(bounds.len()), rng.below(bounds.len()));
        needles.push(d[bounds[a.min(b)]..bounds[a.max(b)]].to_string());
    }
    // two or three pieces cut from one text, in either order, against the independent matcher
    for _ in 0..300 {
        let d: Vec<char> = lower[rng.below(lower.len())].chars().collect();
        let pieces: Vec<String> = (0..2 + rng.below(2))
            .map(|_| {
                let a = rng.below(d.len());
                d[a..(a + 1 + rng.below(6)).min(d.len())].iter().collect()
            })
            .collect();
        let pattern = pieces.join("*");
        let want: BTreeSet<u32> = (0..lower.len() as u32)
            .filter(|&i| oracle_wildcard(&lower[i as usize], &pattern))
            .collect();
        assert_eq!(
            idx.wildcard(&pattern).1,
            want,
            "wildcard mismatch for {pattern:?}"
        );
    }
    let (mut cands, mut hits) = (0, 0);
    for n in &needles {
        let (c, got) = idx.wildcard(n);
        let want: BTreeSet<u32> = (0..lower.len() as u32)
            .filter(|&i| lower[i as usize].contains(n.as_str()))
            .collect();
        assert_eq!(got, want, "substring mismatch for {n:?}");
        assert!(c >= got.len(), "filter must never drop true matches");
        (cands, hits) = (cands + c, hits + got.len());
    }
    hits as f64 / cands.max(1) as f64 // precision of the trigram filter before verification
}

fn check_segments(docs: &[String], single: &Index, rng: &mut Rng) {
    let mut multi = Index::new(single.bm25);
    for chunk in docs.chunks(37) {
        multi.add_segment(chunk); // incremental: one small segment per batch
    }
    assert!(multi.segments.len() > 3);
    let queries = [
        "rust AND async",
        "\"memory ordering\"",
        "*cast* OR token",
        "NOT (rust OR tokio) AND lock",
        "future",
    ];
    let random: Vec<String> = (0..100).map(|_| gen_query(rng, 2)).collect();
    let all = queries
        .iter()
        .map(|s| s.to_string())
        .chain(random)
        .collect::<Vec<_>>();
    let compare = |a: &Index, b: &Index| {
        for q in &all {
            let (ra, rb) = (a.search(q, 10).unwrap(), b.search(q, 10).unwrap());
            assert_eq!(ra.len(), rb.len(), "{q}");
            for (x, y) in ra.iter().zip(&rb) {
                assert!(
                    x.doc == y.doc && (x.score - y.score).abs() < 1e-9,
                    "ranking differs for {q}"
                );
            }
        }
    };
    compare(single, &multi); // global statistics make segmentation invisible to ranking
    multi.merge_all();
    assert_eq!(multi.segments.len(), 1);
    assert_eq!(
        multi.segments[0], single.segments[0],
        "merged segment must equal a freshly built one"
    );
    compare(single, &multi);
}

fn demo(idx: &Index, docs: &[String]) {
    println!("--- demo (BM25 k1={}, b={}) ---", idx.bm25.k1, idx.bm25.b);
    for q in [
        "rust AND async",
        "\"memory ordering\" OR atomic",
        "*cast* AND NOT broadcast",
        "(tokio OR thread) NOT lock",
    ] {
        let total = idx.eval(&parse(q).unwrap()).len();
        println!("query {q:?}: {total} matches, top 3:");
        for h in idx.search(q, 3).unwrap() {
            let text: String = docs[h.doc as usize].chars().take(48).collect();
            println!("  doc {:>3}  score {:.3}  {text}...", h.doc, h.score);
        }
    }
    println!("parse error example: {}", parse("(rust AND").unwrap_err());
}

fn main() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let docs = gen_docs(&mut rng, 300);
    let mut single = Index::new(Bm25 { k1: 1.2, b: 0.75 });
    single.add_segment(&docs);

    check_codec(&mut rng);
    check_hostile_input();
    hand_built_bm25();
    let q_hits = check_queries(&single, &docs, &mut rng);
    let p_hits = check_phrases(&single, &docs, &mut rng);
    let precision = check_substrings(&single, &docs, &mut rng);
    check_segments(&docs, &single, &mut rng);
    assert!(
        q_hits > 100 && p_hits > 100,
        "test corpus must exercise real matches"
    );

    let (packed, plain) = single.posting_sizes();
    assert!(
        packed * 2 < plain,
        "delta+varint should at least halve the size"
    );
    println!(
        "postings: {packed} bytes compressed vs {plain} bytes as Vec<u32> ({:.2}x smaller)",
        plain as f64 / packed as f64
    );
    println!(
        "trigram filter precision before verify: {:.1}%",
        precision * 100.0
    );
    demo(&single, &docs);
    let counts: BTreeMap<bool, usize> = [("rust", true), ("zzz", false)]
        .iter()
        .map(|(q, want)| (*want, single.eval(&parse(q).unwrap()).len()))
        .collect();
    assert!(counts[&true] > 0 && counts[&false] == 0);
    assert_eq!(
        parse("rust AND (async").unwrap_err(),
        QueryError::UnexpectedEnd
    );
    println!("all search-engine checks passed");
}
