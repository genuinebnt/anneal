from author import T, write_track


def sub(s, old, new):
    """str.replace that fails loudly when `old` isn't there (a wrong solution that silently equals the reference)."""
    assert old in s, f"not found: {old[:60]!r}"
    return s.replace(old, new)


P = []

# ---------------------------------------------------------------------------------------------------------------
# 1. Trigram index
TG_STARTER = """
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// A substring-search index over documents, built on trigrams (every window of 3 consecutive `char`s), as in Google Code Search.
pub struct Index {
    docs: BTreeMap<u32, String>,
    grams: HashMap<[char; 3], BTreeSet<u32>>,
}

impl Index {
    pub fn new() -> Index {
        todo!()
    }

    /// Adds a document. Adding an id that already exists **replaces** its text (its old trigrams must stop matching).
    pub fn add(&mut self, id: u32, text: &str) {
        todo!()
    }

    /// Removes a document; true if it existed.
    pub fn remove(&mut self, id: u32) -> bool {
        todo!()
    }

    /// The ids of documents that contain **every trigram** of `query`, sorted: a superset of the real matches, found without
    /// reading any document. `None` if the query has fewer than 3 chars (the index cannot help).
    pub fn candidates(&self, query: &str) -> Option<Vec<u32>> {
        todo!()
    }

    /// The ids, sorted, of documents whose text contains `query` as a substring (case-sensitive). The empty query matches every
    /// document. Use `candidates` to avoid scanning, and then confirm each candidate against the text.
    pub fn search(&self, query: &str) -> Vec<u32> {
        todo!()
    }
}
"""

TG = """
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub struct Index {
    docs: BTreeMap<u32, String>,
    grams: HashMap<[char; 3], BTreeSet<u32>>,
}

fn grams_of(text: &str) -> BTreeSet<[char; 3]> {
    let chars: Vec<char> = text.chars().collect();
    chars.windows(3).map(|w| [w[0], w[1], w[2]]).collect()
}

impl Index {
    pub fn new() -> Index {
        Index { docs: BTreeMap::new(), grams: HashMap::new() }
    }

    pub fn add(&mut self, id: u32, text: &str) {
        self.remove(id); // a replaced document must not keep matching its old trigrams
        for g in grams_of(text) {
            self.grams.entry(g).or_default().insert(id);
        }
        self.docs.insert(id, text.to_string());
    }

    pub fn remove(&mut self, id: u32) -> bool {
        let Some(old) = self.docs.remove(&id) else {
            return false;
        };
        for g in grams_of(&old) {
            if let Some(set) = self.grams.get_mut(&g) {
                set.remove(&id);
                if set.is_empty() {
                    self.grams.remove(&g);
                }
            }
        }
        true
    }

    pub fn candidates(&self, query: &str) -> Option<Vec<u32>> {
        if query.chars().count() < 3 {
            return None;
        }
        let mut lists: Vec<&BTreeSet<u32>> = Vec::new();
        for g in grams_of(query) {
            match self.grams.get(&g) {
                Some(set) => lists.push(set),
                None => return Some(Vec::new()), // a trigram nobody has: no document can match
            }
        }
        lists.sort_by_key(|s| s.len()); // start from the rarest trigram
        let mut result: Vec<u32> = lists[0].iter().copied().collect();
        for other in &lists[1..] {
            result.retain(|id| other.contains(id));
        }
        Some(result)
    }

    pub fn search(&self, query: &str) -> Vec<u32> {
        match self.candidates(query) {
            Some(c) => c.into_iter().filter(|id| self.docs[id].contains(query)).collect(), // trigrams present is not the same as contiguous
            None => self.docs.iter().filter(|(_, t)| t.contains(query)).map(|(&id, _)| id).collect(),
        }
    }
}
"""

TG_HELPERS = """
        fn index(docs: &[(u32, &str)]) -> Index {
            let mut i = Index::new();
            for (id, t) in docs {
                i.add(*id, t);
            }
            i
        }
"""

P.append(dict(
    slug="trigram-index", title="Code search with a trigram index", level="medium", stage="search",
    tags=["code search", "trigram index", "inverted index", "posting lists", "intersection", "candidate verification", "regex search"],
    teaches=["To find a substring in a million files without reading them, index every 3-character window: a query's trigrams must **all** appear in a document that contains it, so intersecting their posting lists leaves a few candidates.",
             "The index only *rules documents out*: 'has all the trigrams' is necessary, not sufficient (`abc xbcd` has `abc` and `bcd` but not `abcd`), so every candidate is verified against the text.",
             "Intersect starting from the **rarest** trigram: the smallest list bounds the work, and one trigram that nobody has settles the query at once."],
    statement="""
        Build the index behind Google Code Search, GitHub's code search and `ripgrep`-style tools: a **trigram index**. A trigram is
        any 3 consecutive `char`s of a document. `grams` maps each trigram to the sorted set of document ids that contain it.

        - `add(id, text)` indexes a document. Adding an existing id **replaces** it: the old text's trigrams must no longer
          match. `remove(id)` deletes a document (and its trigrams) and says whether it existed.
        - `candidates(query)` returns the sorted ids of documents that contain **all** trigrams of `query`, found from the index
          alone; `Some(vec![])` if some trigram is in no document; `None` if the query has fewer than 3 `char`s.
        - `search(query)` returns the sorted ids of documents that really contain `query` as a substring (case-sensitive): the
          candidates, each confirmed against its text. Queries shorter than 3 `char`s fall back to scanning every document, and the
          empty query matches every document.

        Work in `char`s, not bytes, so non-ASCII text works.
    """,
    starter=TG_STARTER,
    solution=TG,
    visible=[
        TG_HELPERS + """
        #[test]
        fn an_index_built_from_a_list() {
            let i = index(&[(4, "needle in a haystack")]);
            check!("found", i.search("needle"), vec![4]);
        }
        """,
        T("finds_substrings", "not just words", "{ let i = index(&[(1, \"hello world\"), (2, \"yellow\"), (3, \"nothing\")]); (i.search(\"llo\"), i.search(\"low\"), i.search(\"world\")) }", "(vec![1, 2], vec![2], vec![1])"),
        T("results_are_sorted", "ids ascending", "{ let i = index(&[(9, \"foo bar\"), (2, \"bar foo\"), (5, \"foobar\")]); i.search(\"foo\") }", "vec![2, 5, 9]"),
        T("trigrams_present_is_not_enough", "abc xbcd contains abc and bcd, not abcd", "{ let i = index(&[(1, \"abc xbcd\"), (2, \"abcd\")]); (i.candidates(\"abcd\"), i.search(\"abcd\")) }", "(Some(vec![1, 2]), vec![2])"),
        T("short_queries_scan", "1 and 2 chars", "{ let i = index(&[(1, \"ab\"), (2, \"xyz\"), (3, \"a\")]); (i.candidates(\"ab\"), i.search(\"ab\"), i.search(\"a\"), i.search(\"\")) }", "(None, vec![1], vec![1, 3], vec![1, 2, 3])"),
        T("case_sensitive", "A is not a", "{ let i = index(&[(1, \"Hello\")]); (i.search(\"Hello\"), i.search(\"hello\")) }", "(vec![1], vec![])"),
        T("replacing_a_document", "old trigrams stop matching", "{ let mut i = index(&[(1, \"alpha beta\")]); i.add(1, \"gamma delta\"); (i.search(\"alpha\"), i.candidates(\"alpha\"), i.search(\"delta\")) }", "(vec![], Some(vec![]), vec![1])"),
    ],
    hidden=[
        TG_HELPERS + """
        #[test]
        fn candidates_are_narrow_because_the_rarest_trigram_decides() {
            let mut docs: Vec<(u32, String)> = (0..500u32).map(|i| (i, format!("common text number {i} with some filler"))).collect();
            docs.push((900, "xx needle yy".to_string()));
            let mut i = Index::new();
            for (id, t) in &docs {
                i.add(*id, t);
            }
            check!("a rare word has one candidate", i.candidates("needle"), Some(vec![900]));
            check!("an unseen trigram settles it", i.candidates("zzz"), Some(vec![]));
            check!("a common word has them all", i.candidates("common").map(|c| c.len()), Some(500));
            check!("search agrees", i.search("needle"), vec![900]);
        }

        #[test]
        fn every_trigram_must_be_present() {
            let i = index(&[(1, "need food"), (2, "feed"), (3, "needle")]);
            check!("only the third has all of nee, eed, edl, dle", i.candidates("needle"), Some(vec![3]));
            check!("two documents have nee and eed", i.candidates("need"), Some(vec![1, 3]));
        }

        #[test]
        fn non_ascii_text_is_indexed_by_char() {
            let i = index(&[(1, "h\\u{e9}llo w\\u{f6}rld"), (2, "hello world")]);
            check!("two-byte chars", i.search("w\\u{f6}rl"), vec![1]);
            check!("a trigram made of multibyte chars", i.candidates("\\u{e9}ll"), Some(vec![1]));
            check!("a 2-char query with a multibyte char scans", i.search("\\u{e9}l"), vec![1]);
            let j = index(&[(1, "\\u{e9}a \\u{a9}ab"), (2, "\\u{e9}ab")]);
            check!("chars that share bytes are different trigrams", j.candidates("\\u{e9}ab"), Some(vec![2]));
            check!("an emoji is one char", index(&[(1, "a\\u{1F980}b")]).search("a\\u{1F980}b"), vec![1]);
        }

        #[test]
        fn removing_documents() {
            let mut i = index(&[(1, "shared text"), (2, "shared other")]);
            check!("existing", i.remove(1), true);
            check!("again", i.remove(1), false);
            check!("gone from search", i.search("shared"), vec![2]);
            check!("and from the candidates", i.candidates("text"), Some(vec![]));
            check!("never existed", i.remove(77), false);
        }

        #[test]
        fn repeated_trigrams_in_the_query_and_the_document() {
            let i = index(&[(1, "aaaaaa"), (2, "aaa"), (3, "aabaa")]);
            check!("aaaa", i.search("aaaa"), vec![1]);
            check!("aaa", i.search("aaa"), vec![1, 2]);
            check!("a query longer than the document", i.search("aaaaaaa"), Vec::<u32>::new());
        }

        #[test]
        fn matches_a_brute_force_scan_on_random_data() {
            let mut rng = anneal_prelude::Rng::new(321);
            for round in 0..60 {
                let mut i = Index::new();
                let mut docs: Vec<(u32, String)> = Vec::new();
                for id in 0..30u32 {
                    let len = rng.below(25);
                    let text: String = (0..len).map(|_| ['a', 'b', 'c'][rng.below(3)]).collect();
                    i.add(id, &text);
                    docs.retain(|(d, _)| *d != id);
                    docs.push((id, text));
                }
                if rng.below(3) == 0 {
                    let gone = rng.below(30) as u32;
                    i.remove(gone);
                    docs.retain(|(d, _)| *d != gone);
                }
                docs.sort();
                for _ in 0..40 {
                    let qlen = rng.below(6);
                    let q: String = (0..qlen).map(|_| ['a', 'b', 'c'][rng.below(3)]).collect();
                    let want: Vec<u32> = docs.iter().filter(|(_, t)| t.contains(&q)).map(|(d, _)| *d).collect();
                    check!(format!("round {round}, query {q:?}"), i.search(&q), want.clone());
                    if let Some(c) = i.candidates(&q) {
                        check!(format!("round {round}, query {q:?}: candidates cover every real match"), want.iter().all(|d| c.contains(d)), true);
                        check!(format!("candidates are sorted"), c.windows(2).all(|w| w[0] < w[1]), true);
                    }
                }
            }
        }
        """,
        T("replacing_keeps_the_new_text_searchable", "and nothing of the old", "{ let mut i = index(&[(1, \"one two three\")]); i.add(1, \"four five six\"); (i.search(\"two\"), i.search(\"five\"), i.candidates(\"one\")) }", "(vec![], vec![1], Some(vec![]))"),
        T("adding_the_same_text_twice_is_idempotent", "same id, same text", "{ let mut i = index(&[(1, \"repeat me\")]); i.add(1, \"repeat me\"); (i.search(\"repeat\"), i.candidates(\"me\"), i.search(\"\")) }", "(vec![1], None, vec![1])"),
        T("an_empty_index", "no documents", "{ let i = Index::new(); (i.search(\"abc\"), i.search(\"\"), i.candidates(\"abc\")) }", "(vec![], vec![], Some(vec![]))"),
        T("documents_shorter_than_three_chars_have_no_trigrams", "found only by scanning", "{ let i = index(&[(1, \"ab\"), (2, \"abc\")]); (i.search(\"ab\"), i.search(\"abc\"), i.candidates(\"abc\")) }", "(vec![1, 2], vec![2], Some(vec![2]))"),
    ],
    wrong=dict(
        no_verification=sub(TG, "        match self.candidates(query) {\n            Some(c) => c.into_iter().filter(|id| self.docs[id].contains(query)).collect(), // trigrams present is not the same as contiguous", "        match self.candidates(query) {\n            Some(c) => c,"),
        union_instead_of_intersection=sub(TG, "        lists.sort_by_key(|s| s.len()); // start from the rarest trigram\n        let mut result: Vec<u32> = lists[0].iter().copied().collect();\n        for other in &lists[1..] {\n            result.retain(|id| other.contains(id));\n        }\n        Some(result)", "        let mut all: BTreeSet<u32> = BTreeSet::new();\n        for l in &lists {\n            all.extend(l.iter().copied());\n        }\n        Some(all.into_iter().collect())"),
        first_trigram_only=sub(TG, "        lists.sort_by_key(|s| s.len()); // start from the rarest trigram\n        let mut result: Vec<u32> = lists[0].iter().copied().collect();\n        for other in &lists[1..] {\n            result.retain(|id| other.contains(id));\n        }\n        Some(result)", "        Some(lists[0].iter().copied().collect())"),
        replaced_documents_keep_old_trigrams=sub(TG, "        self.remove(id); // a replaced document must not keep matching its old trigrams\n", ""),
        remove_leaves_trigrams=sub(TG, "        for g in grams_of(&old) {\n            if let Some(set) = self.grams.get_mut(&g) {\n                set.remove(&id);\n                if set.is_empty() {\n                    self.grams.remove(&g);\n                }\n            }\n        }\n        true", "        let _ = old;\n        true"),
        short_queries_match_nothing=sub(TG, "            None => self.docs.iter().filter(|(_, t)| t.contains(query)).map(|(&id, _)| id).collect(),", "            None => Vec::new(),"),
        trigrams_of_bytes=sub(TG, "    let chars: Vec<char> = text.chars().collect();\n    chars.windows(3).map(|w| [w[0], w[1], w[2]]).collect()", "    let bytes: Vec<char> = text.bytes().map(|b| b as char).collect();\n    bytes.windows(3).map(|w| [w[0], w[1], w[2]]).collect()"),
        unseen_trigram_is_ignored=sub(TG, "                None => return Some(Vec::new()), // a trigram nobody has: no document can match", "                None => {}"),
        short_query_threshold_off_by_one=sub(TG, "        if query.chars().count() < 3 {\n            return None;\n        }", "        if query.chars().count() < 4 {\n            return None;\n        }"),
    ),
    hints=[("approach", "Index time: for each document, add its id to the set of every distinct trigram. Query time: take the query's distinct trigrams, fetch their id sets, intersect them (smallest first), then check each surviving document with `text.contains(query)`. Replacing a document means first removing the old text's trigrams."),
           ("rust", "`let chars: Vec<char> = text.chars().collect(); chars.windows(3)` gives the trigram windows; collect them into a `BTreeSet<[char; 3]>` to deduplicate. Keep `BTreeSet<u32>` posting sets so results come out sorted. `Vec::retain` with `other.contains(id)` is a simple intersection; on sorted vectors you would merge instead."),
           ("edge", "A query shorter than three chars has no trigrams: scan. A query with one trigram missing from the whole index has no matches: return at once. When `remove`-ing, drop trigram entries whose set becomes empty, or the map grows forever. Windows are over `char`s: a byte-based window would split multibyte characters.")],
    notes=("This is Russ Cox's 'Regular Expression Matching with a Trigram Index' (the design of Google Code Search, and of Zoekt and GitHub's code search): real systems extend it to regular expressions by extracting a boolean combination of trigrams from the regex, and store the posting lists compressed (see the F7 posting-list codec) and memory-mapped. The recall/precision split is the whole idea: the index guarantees *no false negatives*, the verification removes the false positives.", "O(trigrams x rarest list) per query plus verification", "O(total text) for the index"),
    follow_up="1) Make the posting lists sorted `Vec<u32>`s and intersect by merging (and by galloping when one is much shorter); measure it. 2) Support case-insensitive search by indexing lowercased trigrams and verifying on the original text. 3) Extend to simple regexes: for `foo.*bar` index `foo` and `bar`, for `a|b` take a union; write `candidates_for_regex` using the Y8 regex engine to verify. 4) Store postings with the F7 codec and decode lazily. 5) Handle updates cheaply: keep a small mutable index in memory plus immutable segments, and search both.",
    related=["F7", "Y8", "F4"],
))

# ---------------------------------------------------------------------------------------------------------------
# 2. BM25
BM_STARTER = """
use std::collections::{BTreeMap, HashMap};

/// A ranked full-text index using Okapi BM25 (`k1 = 1.2`, `b = 0.75`).
pub struct Index {
    // your fields
}

/// Splits text into lowercase terms: maximal runs of alphanumeric chars (`char::is_alphanumeric`), lowercased with `to_lowercase`.
pub fn tokenize(text: &str) -> Vec<String> {
    todo!()
}

impl Index {
    pub fn new() -> Index {
        todo!()
    }

    /// Adds a document; an existing id is **replaced**.
    pub fn add(&mut self, id: u32, text: &str) {
        todo!()
    }

    /// The best `k` documents for `query` as `(id, score)`, highest score first, ties by smaller id first. Only documents that
    /// contain at least one query term are returned. Each *distinct* query term counts once.
    ///
    /// score(d) = sum over distinct query terms t of  idf(t) * tf * (k1 + 1) / (tf + k1 * (1 - b + b * len(d) / avglen))
    /// idf(t)   = ln(1 + (N - n_t + 0.5) / (n_t + 0.5))
    /// where N is the number of documents, n_t the number containing t, tf the count of t in d, len(d) the number of terms in d,
    /// and avglen the mean length over all documents.
    pub fn search(&self, query: &str, k: usize) -> Vec<(u32, f64)> {
        todo!()
    }
}
"""

BM = """
use std::collections::{BTreeMap, HashMap};

const K1: f64 = 1.2;
const B: f64 = 0.75;

pub struct Index {
    /// term -> (doc -> term frequency)
    postings: HashMap<String, BTreeMap<u32, u32>>,
    /// doc -> its terms (kept so a replaced document can be removed cleanly)
    docs: BTreeMap<u32, Vec<String>>,
    total_len: usize,
}

pub fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(|w| w.to_lowercase()).collect()
}

impl Index {
    pub fn new() -> Index {
        Index { postings: HashMap::new(), docs: BTreeMap::new(), total_len: 0 }
    }

    pub fn add(&mut self, id: u32, text: &str) {
        if let Some(old) = self.docs.remove(&id) {
            self.total_len -= old.len();
            for t in &old {
                if let Some(p) = self.postings.get_mut(t) {
                    p.remove(&id);
                    if p.is_empty() {
                        self.postings.remove(t);
                    }
                }
            }
        }
        let terms = tokenize(text);
        self.total_len += terms.len();
        for t in &terms {
            *self.postings.entry(t.clone()).or_default().entry(id).or_insert(0) += 1;
        }
        self.docs.insert(id, terms);
    }

    pub fn search(&self, query: &str, k: usize) -> Vec<(u32, f64)> {
        let n = self.docs.len() as f64;
        if self.docs.is_empty() {
            return Vec::new();
        }
        let avglen = self.total_len as f64 / n;
        let mut seen = std::collections::HashSet::new();
        let mut scores: HashMap<u32, f64> = HashMap::new();
        for term in tokenize(query) {
            if !seen.insert(term.clone()) {
                continue; // each distinct term counts once
            }
            let Some(posting) = self.postings.get(&term) else {
                continue;
            };
            let nt = posting.len() as f64;
            let idf = (1.0 + (n - nt + 0.5) / (nt + 0.5)).ln();
            for (&doc, &tf) in posting {
                let len = self.docs[&doc].len() as f64;
                let tf = tf as f64;
                *scores.entry(doc).or_insert(0.0) += idf * tf * (K1 + 1.0) / (tf + K1 * (1.0 - B + B * len / avglen));
            }
        }
        let mut out: Vec<(u32, f64)> = scores.into_iter().collect();
        out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
        out.truncate(k);
        out
    }
}
"""

BM_HELPERS = """
        fn index(docs: &[(u32, &str)]) -> Index {
            let mut i = Index::new();
            for (id, t) in docs {
                i.add(*id, t);
            }
            i
        }

        fn ids(r: &[(u32, f64)]) -> Vec<u32> {
            r.iter().map(|x| x.0).collect()
        }

        /// A from-scratch scorer: the oracle.
        fn reference(docs: &[(u32, String)], query: &str, k: usize) -> Vec<(u32, f64)> {
            let toks: Vec<(u32, Vec<String>)> = docs.iter().map(|(id, t)| (*id, tokenize(t))).collect();
            let n = toks.len() as f64;
            if toks.is_empty() {
                return vec![];
            }
            let avg = toks.iter().map(|(_, t)| t.len()).sum::<usize>() as f64 / n;
            let mut qt: Vec<String> = Vec::new();
            for t in tokenize(query) {
                if !qt.contains(&t) {
                    qt.push(t);
                }
            }
            let mut out = Vec::new();
            for (id, t) in &toks {
                let mut score = 0.0;
                let mut hit = false;
                for q in &qt {
                    let nt = toks.iter().filter(|(_, d)| d.contains(q)).count() as f64;
                    let tf = t.iter().filter(|w| *w == q).count() as f64;
                    if tf > 0.0 {
                        hit = true;
                        let idf = (1.0 + (n - nt + 0.5) / (nt + 0.5)).ln();
                        score += idf * tf * 2.2 / (tf + 1.2 * (0.25 + 0.75 * t.len() as f64 / avg));
                    }
                }
                if hit {
                    out.push((*id, score));
                }
            }
            out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
            out.truncate(k);
            out
        }
"""

P.append(dict(
    slug="bm25-ranking", title="Rank search results with BM25", level="hard", stage="search",
    tags=["BM25", "ranking", "tf-idf", "inverted index", "relevance", "top-k", "Lucene", "tokenization"],
    teaches=["Ranking is arithmetic on three counts: how often a term is in this document (tf), how rare it is across all documents (idf), and how long this document is compared to the average (length normalisation).",
             "BM25 saturates term frequency (the 10th occurrence adds much less than the 2nd) and discounts long documents: the two fixes that made it beat plain tf-idf, and the default in Lucene, Elasticsearch and tantivy.",
             "An index that keeps per-term posting lists and per-document lengths can score a query by touching only the documents that contain a query term."],
    statement="""
        Add ranking to a search index with **Okapi BM25** (`k1 = 1.2`, `b = 0.75`). `tokenize` splits text into lowercase terms (maximal
        runs of `char::is_alphanumeric`, lowercased with `to_lowercase`). `Index::add(id, text)` indexes a document; an existing id is
        **replaced**.

        `search(query, k)` returns up to `k` pairs `(id, score)`, best first, ties broken by the smaller id, and only documents that
        contain at least one query term. Each *distinct* query term counts once (`rust rust` scores like `rust`). With `N` documents, `n_t`
        of them containing `t`, `tf` the count of `t` in the document, `len(d)` its number of terms and `avglen` the mean length of all
        documents:

        `score(d) = sum over query terms t of idf(t) * tf * (k1 + 1) / (tf + k1 * (1 - b + b * len(d) / avglen))`
        `idf(t) = ln(1 + (N - n_t + 0.5) / (n_t + 0.5))`

        An empty index, an empty query, or `k == 0` gives an empty result.
    """,
    starter=BM_STARTER,
    solution=BM,
    visible=[
        BM_HELPERS.split("/// A from-scratch scorer")[0] + """
        #[test]
        fn an_index_built_from_a_list() {
            let i = index(&[(4, "needle in a haystack")]);
            check!("found", ids(&i.search("needle", 3)), vec![4]);
        }
        """,
        T("tokenizing", "lowercase alphanumeric runs", "tokenize(\"Hello, World! It's 3D-printing\")", "vec![\"hello\", \"world\", \"it\", \"s\", \"3d\", \"printing\"].into_iter().map(String::from).collect::<Vec<_>>()"),
        T("the_score_of_a_single_document", "N = 1, n = 1, len = avglen", "{ let i = index(&[(1, \"rust\")]); let r = i.search(\"rust\", 5); (r.len(), r[0].0, (r[0].1 - 0.28768207245178096).abs() < 1e-9) }", "(1, 1, true)"),
        T("more_occurrences_rank_higher", "tf = 3 beats tf = 1", "{ let i = index(&[(1, \"rust is fast\"), (2, \"rust rust rust everywhere\"), (3, \"python is slow\")]); ids(&i.search(\"rust\", 10)) }", "vec![2, 1]"),
        T("only_matching_documents", "no zero scores", "{ let i = index(&[(1, \"alpha\"), (2, \"beta\"), (3, \"alpha beta\")]); ids(&i.search(\"alpha\", 10)) }", "vec![1, 3]"),
        T("k_limits_the_results", "top 1", "{ let i = index(&[(1, \"a b\"), (2, \"a\"), (3, \"a c d\")]); ids(&i.search(\"a\", 1)) }", "vec![2]"),
        T("rare_terms_matter_more", "idf: the rare word wins", "{ let i = index(&[(1, \"the common word\"), (2, \"the rare zebra\"), (3, \"the common thing\"), (4, \"the common stuff\")]); ids(&i.search(\"common zebra\", 4))[0] }", "2"),
    ],
    hidden=[
        BM_HELPERS + """
        #[test]
        fn matches_a_from_scratch_scorer_on_random_corpora() {
            let mut rng = anneal_prelude::Rng::new(2718);
            let vocab = ["rust", "go", "zig", "c", "java", "swift", "kotlin", "ruby", "lua", "perl"];
            for round in 0..80 {
                let ndocs = rng.below(25) + 1;
                let mut docs: Vec<(u32, String)> = Vec::new();
                let mut idx = Index::new();
                for id in 0..ndocs as u32 {
                    let len = rng.below(12);
                    let text = (0..len).map(|_| vocab[rng.below(vocab.len())]).collect::<Vec<_>>().join(" ");
                    idx.add(id, &text);
                    docs.push((id, text));
                }
                for _ in 0..10 {
                    let qn = rng.below(4) + 1;
                    let query = (0..qn).map(|_| vocab[rng.below(vocab.len())]).collect::<Vec<_>>().join(" ");
                    let k = rng.below(8) + 1;
                    let got = idx.search(&query, k);
                    let want = reference(&docs, &query, k);
                    check!(format!("round {round} {query:?}: ids"), ids(&got), ids(&want));
                    for (g, w) in got.iter().zip(&want) {
                        check!(format!("round {round} {query:?}: score of {}", g.0), (g.1 - w.1).abs() < 1e-9, true);
                    }
                }
            }
        }

        #[test]
        fn replacing_a_document_updates_every_statistic() {
            let mut i = index(&[(1, "apple banana"), (2, "banana cherry")]);
            i.add(1, "durian");
            check!("old terms no longer match", ids(&i.search("apple", 5)), Vec::<u32>::new());
            let docs = vec![(1u32, "durian".to_string()), (2u32, "banana cherry".to_string())];
            let want = reference(&docs, "banana durian", 5);
            let got = i.search("banana durian", 5);
            check!("ids", ids(&got), ids(&want));
            check!("scores use the new lengths and counts", got.iter().zip(&want).all(|(g, w)| (g.1 - w.1).abs() < 1e-9), true);
        }
        """,
        T("repeated_query_terms_count_once", "rust rust = rust", "{ let i = index(&[(1, \"rust code\"), (2, \"code only\")]); let a = i.search(\"rust\", 5); let b = i.search(\"rust rust RUST\", 5); a.len() == b.len() && (a[0].1 - b[0].1).abs() < 1e-12 }", "true"),
        T("ties_are_broken_by_id", "identical documents", "{ let i = index(&[(7, \"same text\"), (3, \"same text\"), (5, \"same text\")]); ids(&i.search(\"same\", 10)) }", "vec![3, 5, 7]"),
        T("longer_documents_score_lower", "same tf, more words", "{ let i = index(&[(1, \"rust a b c d e f g h i j\"), (2, \"rust\")]); ids(&i.search(\"rust\", 5)) }", "vec![2, 1]"),
        T("term_frequency_saturates", "20 occurrences are not 10x one", "{ let many = \"rust \".repeat(20); let i = index(&[(1, many.trim()), (2, \"rust x x x x x x x x x x x x x x x x x x x\")]); let r = i.search(\"rust\", 5); (r[0].0, r[0].1 / r[1].1 < 3.0) }", "(1, true)"),
        T("scores_are_positive_even_for_common_terms", "idf uses ln(1 + ...)", "{ let i = index(&[(1, \"the cat\"), (2, \"the dog\"), (3, \"the bird\")]); i.search(\"the\", 5).iter().all(|r| r.1 > 0.0) }", "true"),
        T("empty_things", "no index, no query, k = 0", "{ let e = Index::new(); let i = index(&[(1, \"a b c\")]); (e.search(\"a\", 5).len(), i.search(\"\", 5).len(), i.search(\"!!!\", 5).len(), i.search(\"a\", 0).len(), i.search(\"zzz\", 5).len()) }", "(0, 0, 0, 0, 0)"),
        T("unicode_terms", "case folding and non-ASCII letters", "{ let i = index(&[(1, \"\\u{c9}COLE \\u{e9}cole\"), (2, \"college\")]); ids(&i.search(\"\\u{e9}cole\", 5)) }", "vec![1]"),
        T("numbers_are_terms", "alphanumeric", "{ let i = index(&[(1, \"error 404 not found\"), (2, \"error 500\")]); ids(&i.search(\"404\", 5)) }", "vec![1]"),
    ],
    wrong=dict(
        idf_without_the_one=sub(BM, "let idf = (1.0 + (n - nt + 0.5) / (nt + 0.5)).ln();", "let idf = ((n - nt + 0.5) / (nt + 0.5)).ln();"),
        no_length_normalisation=sub(BM, "(tf + K1 * (1.0 - B + B * len / avglen))", "(tf + K1)"),
        duplicate_query_terms_count_twice=sub(BM, "            if !seen.insert(term.clone()) {\n                continue; // each distinct term counts once\n            }\n", "            seen.insert(term.clone());\n"),
        ties_by_larger_id=sub(BM, ".then(a.0.cmp(&b.0))", ".then(b.0.cmp(&a.0))"),
        raw_term_frequency=sub(BM, "idf * tf * (K1 + 1.0) / (tf + K1 * (1.0 - B + B * len / avglen))", "idf * tf"),
        k_ignored=sub(BM, "        out.truncate(k);\n        out", "        out"),
        replaced_document_keeps_its_length=sub(BM, "            self.total_len -= old.len();\n", ""),
        replaced_document_keeps_its_postings=sub(BM, "            for t in &old {\n                if let Some(p) = self.postings.get_mut(t) {\n                    p.remove(&id);\n                    if p.is_empty() {\n                        self.postings.remove(t);\n                    }\n                }\n            }\n", "            let _ = &old;\n"),
        tokenizer_keeps_punctuation=sub(BM, "text.split(|c: char| !c.is_alphanumeric())", "text.split(|c: char| c.is_whitespace())"),
        tokenizer_is_case_sensitive=sub(BM, ".map(|w| w.to_lowercase()).collect()", ".map(|w| w.to_string()).collect()"),
        zero_score_documents_are_returned=sub(BM, "        let mut out: Vec<(u32, f64)> = scores.into_iter().collect();", "        for &id in self.docs.keys() {\n            scores.entry(id).or_insert(0.0);\n        }\n        let mut out: Vec<(u32, f64)> = scores.into_iter().collect();"),
    ),
    hints=[("approach", "Keep, per term, a map from document to term frequency (the posting list), and per document its length; also the total length so `avglen` is cheap. To search: for each distinct query term, compute `idf` once and add its contribution to every document in its posting list; then sort the accumulated scores and truncate."),
           ("rust", "`HashMap<String, BTreeMap<u32, u32>>` for postings and `BTreeMap<u32, Vec<String>>` for the documents' terms (needed to remove a replaced document). Accumulate scores in a `HashMap<u32, f64>`; sort with `b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0))`. `ln` is `f64::ln`; `(1.0 + x).ln()` is what makes `idf` positive even for a term in every document."),
           ("edge", "`avglen` changes whenever any document is added or replaced: compute it at query time from `total_len / N`, and keep `total_len` exact when replacing. A term repeated in the query counts once. Documents with no query term never appear: scores are only created from posting lists.")],
    notes=("BM25 (Robertson and Walker, 1994, from the Okapi system) is Lucene's default similarity since 6.0, so it is what Elasticsearch, OpenSearch, Solr and tantivy rank with. Real engines avoid scoring every posting with block-max indexes (WAND / BlockMax-WAND skip documents that cannot reach the current top-k) and precompute each document's length-normalisation factor into a one-byte 'norm'.", "O(sum of the query terms' posting lengths + matches log matches)", "O(total terms)"),
    follow_up="1) Replace the full sort with a bounded min-heap of size `k` and compare for large corpora. 2) Add field boosts (title counts double) by indexing fields separately and summing weighted scores (BM25F). 3) Add phrase search using positional postings. 4) Implement block-max WAND: store the maximum possible contribution per block and skip documents that cannot beat the current k-th best. 5) Evaluate: build a few queries with known relevant documents and compute precision@k and MRR for BM25 vs raw tf-idf vs your `idf`-less variant.",
    related=["Y10", "F7", "F4"],
))

STAGES = [("search", "Search", "medium")]

if __name__ == "__main__":
    P.sort(key=lambda p: {"medium": 0, "hard": 1}[p["level"]])
    n = write_track("y10-search-engine-core", "Y10", "Search engine core", "Y", "sde3", 10,
                    "The two halves of code and text search: a trigram index that narrows substring queries without reading documents, and BM25 ranking over an inverted index.",
                    STAGES, P)
    print("Y10", n)
