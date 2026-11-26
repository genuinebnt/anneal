//! CAPSTONE 877 (database): an embedded mini vector database: schema'd collections, WAL + memtable + immutable HNSW-lite segments, SQ8 with rescoring, filtered and hybrid search, snapshots, crash recovery, a text protocol.
//! LEVEL: capstone
//! Use in: understanding what a vector database adds on top of an ANN index; combines bytes and recovery, graph search, bitmaps, a parser, BM25, locks and model tests.   Run: cargo run --release --example mini_vector_database
#![allow(dead_code)]
//!
//! WHAT: a vector index answers "nearest ids". A vector DATABASE also has to accept writes while serving reads, delete and update, survive a
//! crash, filter on metadata, mix keyword and vector relevance, and say what it is doing. This file does all of that in one process.
//!
//! LAYERS, bottom up:
//!   1. SCHEMA: a collection has a vector dimension, a metric (l2 | cosine | dot) and typed fields (keyword, int, bool, text). Every write is
//!      validated BEFORE it is logged, so the log never contains a record that cannot be applied. Cosine vectors are normalised once, on the
//!      way in, and `get` returns the normalised form. Every metric is turned into a DISTANCE (lower is better): squared L2, 1 - cos, -dot.
//!   2. WRITE PATH: validate -> append a [len][crc32][lsn, op] frame to the WAL -> apply to the MEMTABLE (exact, brute force). Returning Ok
//!      is the acknowledgement. A full memtable is FLUSHED into an immutable SEGMENT.
//!   3. SEGMENT (immutable, shared by Arc): ids, full-precision vectors, metadata, SQ8 codes, an inverted index from (field, value) to a
//!      bitmap, BM25 postings for text fields, and an HNSW-lite graph unless it is small (then a flat exact scan is cheaper and exact).
//!      The ONLY mutable per-segment state is a tombstone bitmap kept beside the Arc. Delete = set a bit. Update = delete + insert.
//!   4. COMPACTION is split in three so the slow part runs without any lock: PLAN (read lock: pick segments, clone Arcs and tombstones),
//!      BUILD (no lock: copy live rows, build a new graph), COMMIT (write lock: swap, and tombstone every row that was deleted or updated while
//!      we were building). A job whose inputs have meanwhile disappeared is rejected, never half-applied.
//!   5. DURABILITY: `checkpoint` writes a snapshot (every segment byte for byte, graphs included, plus the memtable and the last LSN) with an
//!      atomic replace, then truncates the WAL. Recovery loads the snapshot and replays WAL records with LSN > the snapshot's. A crash
//!      between those two steps is therefore harmless: old records are skipped by LSN. A TORN TAIL (the last frame is incomplete) is cut off;
//!      a complete frame with a bad checksum and more data AFTER it is corruption of acknowledged history and is reported, not discarded.
//!      LIMIT: a damaged LENGTH field can make mid-log corruption look like a torn tail; real logs add a checksum on the header and block
//!      framing. The "disk" is a struct of byte vectors so that crashes can be injected at exact points; there is no real fsync here.
//!   6. QUERIES: `Query` builder (top_k, filter, score threshold, ef, exact, text) and a filter language
//!      `genre = "jazz" and (year in 1990..1999 or not live = true) and label in ["a", "b"]`.
//!      Predicates are two-valued: a comparison on a missing field is false, so `not x = 1` INCLUDES rows without x (SQL would not).
//!      Each segment turns the filter into a bitmap and a PLANNER picks per segment:
//!        FLAT  no graph: exact scan of the matching rows.
//!        PRE   few matches: exact scan of just those rows.
//!        POST  most rows match: ordinary graph search with a widened ef, then drop non-matches.
//!        IN    otherwise: graph search that walks through every node but keeps only matches.
//!      Graph traversal scores SQ8 codes; the surviving `oversample * k` candidates are RESCORED with full-precision vectors. If a graph
//!      plan returns fewer than min(k, matches) rows, the segment falls back to PRE, so a filtered query is never silently short.
//!   7. HYBRID: BM25 over the text fields with GLOBAL statistics of the LIVE rows (all segments + memtable), so scores are comparable across
//!      segments and deletes do not skew IDF (Lucene, by contrast, keeps counting deleted docs until segments merge; that is cheaper:
//!      here every text query walks the postings of its terms and sums document lengths). The two ranked lists are fused by RECIPROCAL RANK
//!      FUSION: score = sum 1/(60 + rank).
//!   8. CONCURRENCY: one `RwLock` around the whole database state. Any number of readers search in parallel; writes are serialised by the
//!      write lock. Searches HOLD the read lock (simple, but a long search delays the writer; a production design publishes an immutable
//!      view through an Arc swap so readers never block writes). Compaction's expensive step holds nothing.
//!   9. INTERFACE: a tiny line protocol (CREATE, INSERT, SEARCH, ...) driven in process, with `+`, `-ERR` and `*n` replies as in RESP, and
//!      atomic METRICS counters.
//!
//! VERIFIED: bitmaps against BTreeSet; the filter printer and parser round-trip on 3000 random expressions, and malformed or over-deep input
//! gives an error; WAL cut at EVERY byte offset recovers exactly the acknowledged prefix; mid-log corruption is reported; a model test of 3000
//! random operations over three metrics (insert, upsert, delete, get, count, exact and approximate filtered search, BM25, flush, compaction
//! including writes that race a compaction job, checkpoint, restart, torn append, crash between snapshot and WAL truncation) against a
//! brute-force oracle: exact queries and BM25 match exactly, approximate queries never return a dead or non-matching row, are never short of
//! min(k, matches) when no score threshold is set, and keep mean recall above 0.95; a larger collection keeps recall@10 above a threshold per filter,
//! uses all four plans, and still does so after deletes, updates and compaction; snapshots round-trip bit-exactly and sampled bit flips are
//! rejected; 4 reader threads x 5 passes against a writer that updates, deletes and compacts; 3000 mangled protocol lines never panic; a
//! scripted session is printed and its replies checked.
//! LIMITS: single process, everything in RAM, no real files, no replication, no per-tenant limits, no query cancellation; recall figures are
//! for these small synthetic data sets only, and clustered data like this is easy (the large run stays at or near 1.0).
//! THE PLANNER IS A FIXED RULE (two thresholds), NOT A COST MODEL, and it is not optimal. Read the larger-collection table: after full
//! compaction the filter matching 261 of 4600 rows runs IN-filter and spends about ten times more distance calls than an exact scan of the
//! matches would (an SQ8 distance and a full one are not equally expensive, but the gap is large). `filtered_ann_hybrid_search` shows a
//! calibrated planner.
//!
//! COMMON USES: the same shape appears in Qdrant (segments, payload index, optimizer), Milvus (growing and sealed segments, compaction),
//! Weaviate (hybrid search with rank fusion), LanceDB, Elasticsearch/Lucene (immutable segments + live docs), pgvector (inherits WAL and
//! MVCC from Postgres).
//! Read: HNSW, Malkov and Yashunin https://arxiv.org/abs/1603.09320 ; SPANN https://arxiv.org/abs/2111.08566 ; RaBitQ
//! https://arxiv.org/abs/2405.12497 ; ann-benchmarks https://ann-benchmarks.com ; DiskANN, Subramanya et al.
//! https://proceedings.neurips.cc/paper/2019/file/09853c7fb1d3f8ee67a61b6bf4a7f8e6-Paper.pdf ; cited by title: Jegou, Douze and Schmid,
//! "Product Quantization for Nearest Neighbor Search"; Gollapudi et al., "Filtered-DiskANN"; Cormack, Clarke and Buettcher, "Reciprocal
//! Rank Fusion outperforms Condorcet and individual Rank Learning Methods" (SIGIR 2009).
//!
//! TRY NEXT:
//!   - [S] Predict, then measure: set `exact_scan_limit` to 0 and to 100000 in `large_collection`. Which filters change plan, and what happens
//!     to recall and to distance computations per query?
//!   - [S] Set `oversample` to 1. Which assertion fails first, and by how much does recall drop?
//!   - [M] Publish an immutable view (`Arc<View>`) that readers clone under a very short lock, so searches stop blocking writers. What must
//!     happen to the tombstone bitmaps?
//!   - [M] Add `DELETE ... WHERE expr` to the protocol. Decide how it is logged (the predicate, or the resolved ids?) and justify it.
//!   - [A] PROJECT: replace the simulated `Disk` with real files (write-temp, fsync, rename, fsync directory), add group commit, and run the
//!     crash tests by killing a child process.
use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::ops::Bound;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, RwLock};
use thiserror::Error;

// ============================================================ errors ============================================================
#[derive(Debug, Error, PartialEq)]
enum DbError {
    #[error("unknown collection '{0}'")]
    NoCollection(String),
    #[error("collection '{0}' already exists")]
    CollectionExists(String),
    #[error("duplicate id {0}")]
    DuplicateId(u64),
    #[error("schema violation: {0}")]
    Schema(String),
    #[error("filter syntax error at byte {at}: {msg}")]
    FilterSyntax { at: usize, msg: String },
    #[error("bad query: {0}")]
    BadQuery(String),
    #[error("corrupt data: {0}")]
    Corrupt(&'static str),
    #[error("unsupported format version {0}")]
    Version(u32),
    /// The simulated process died. The handle is unusable: recover from the disk.
    #[error("crashed")]
    Crashed,
}
type Result<T> = std::result::Result<T, DbError>;
fn schema_err<T>(msg: impl Into<String>) -> Result<T> {
    Err(DbError::Schema(msg.into()))
}

// ============================================================ small utilities ============================================================
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn unit(&mut self) -> f64 {
        ((self.next_u64() >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
    fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }
    fn gauss(&mut self) -> f32 {
        let (u, v) = (self.unit(), self.unit());
        ((-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()) as f32
    }
}

/// Bitwise CRC-32 (IEEE). Slow but tiny; real systems use a table or hardware instructions.
fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = (c >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(c & 1));
        }
    }
    !c
}

/// A fixed-size set of row numbers.
#[derive(Clone, PartialEq, Debug)]
struct Bitmap {
    len: usize,
    words: Vec<u64>,
}
impl Bitmap {
    fn empty(len: usize) -> Bitmap {
        Bitmap {
            len,
            words: vec![0; len.div_ceil(64)],
        }
    }
    fn full(len: usize) -> Bitmap {
        let mut b = Bitmap::empty(len);
        b.not_assign();
        b
    }
    fn set(&mut self, i: usize) {
        assert!(i < self.len);
        self.words[i / 64] |= 1 << (i % 64);
    }
    fn get(&self, i: usize) -> bool {
        i < self.len && self.words[i / 64] >> (i % 64) & 1 == 1
    }
    fn count(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }
    fn and_assign(&mut self, o: &Bitmap) {
        self.words
            .iter_mut()
            .zip(&o.words)
            .for_each(|(a, b)| *a &= b);
    }
    fn or_assign(&mut self, o: &Bitmap) {
        self.words
            .iter_mut()
            .zip(&o.words)
            .for_each(|(a, b)| *a |= b);
    }
    fn andnot_assign(&mut self, o: &Bitmap) {
        self.words
            .iter_mut()
            .zip(&o.words)
            .for_each(|(a, b)| *a &= !b);
    }
    /// Complement within `len`: the bits past the end must stay zero or `count` would be wrong.
    fn not_assign(&mut self) {
        self.words.iter_mut().for_each(|w| *w = !*w);
        if self.len % 64 != 0 {
            *self.words.last_mut().unwrap() &= (1u64 << (self.len % 64)) - 1;
        }
    }
    fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.words.iter().enumerate().flat_map(|(wi, &w)| {
            let mut rest = w;
            std::iter::from_fn(move || {
                (rest != 0).then(|| {
                    let bit = rest.trailing_zeros() as usize;
                    rest &= rest - 1;
                    wi * 64 + bit
                })
            })
        })
    }
}

// ============================================================ schema and values ============================================================
#[derive(Clone, Copy, PartialEq, Debug)]
enum Metric {
    L2,
    Cosine,
    Dot,
}
impl Metric {
    fn name(self) -> &'static str {
        match self {
            Metric::L2 => "l2",
            Metric::Cosine => "cosine",
            Metric::Dot => "dot",
        }
    }
    /// Distance (lower is better). Cosine assumes both vectors are already unit length.
    fn distance(self, a: &[f32], b: &[f32]) -> f32 {
        match self {
            Metric::L2 => a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum(),
            Metric::Cosine => 1.0 - a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>(),
            Metric::Dot => -a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>(),
        }
    }
    /// Validate and canonicalise a vector (stored or query).
    fn prepare(self, v: &[f32], dim: usize) -> Result<Vec<f32>> {
        if v.len() != dim {
            return schema_err(format!(
                "vector has {} dimensions, collection needs {dim}",
                v.len()
            ));
        }
        // the magnitude limit keeps every squared distance finite
        if v.iter().any(|x| !x.is_finite() || x.abs() > 1e15) {
            return schema_err("vector components must be finite and at most 1e15 in magnitude");
        }
        if self != Metric::Cosine {
            return Ok(v.to_vec());
        }
        let norm = v
            .iter()
            .map(|x| (*x as f64) * (*x as f64))
            .sum::<f64>()
            .sqrt();
        if norm < 1e-12 {
            return schema_err("cosine distance is undefined for a zero vector");
        }
        Ok(v.iter().map(|x| (*x as f64 / norm) as f32).collect())
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum FieldType {
    Keyword,
    Int,
    Bool,
    /// tokenised for BM25; not usable in filters
    Text,
}
impl FieldType {
    const ALL: [FieldType; 4] = [
        FieldType::Keyword,
        FieldType::Int,
        FieldType::Bool,
        FieldType::Text,
    ];
    fn name(self) -> &'static str {
        ["keyword", "int", "bool", "text"][self as usize]
    }
}

/// The derived order (Bool < Int < Str) only matters inside one field, where all values share a variant.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Value {
    Bool(bool),
    Int(i64),
    Str(String),
}
impl Value {
    fn fits(&self, t: FieldType) -> bool {
        matches!(
            (self, t),
            (Value::Bool(_), FieldType::Bool)
                | (Value::Int(_), FieldType::Int)
                | (Value::Str(_), FieldType::Keyword | FieldType::Text)
        )
    }
}
impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Str(s) => {
                write!(f, "\"")?;
                for c in s.chars() {
                    if c == '"' || c == '\\' {
                        write!(f, "\\")?;
                    }
                    write!(f, "{c}")?;
                }
                write!(f, "\"")
            }
        }
    }
}
type Meta = BTreeMap<String, Value>;

#[derive(Clone, PartialEq, Debug)]
struct Schema {
    dim: usize,
    metric: Metric,
    fields: BTreeMap<String, FieldType>,
}
fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .enumerate()
            .all(|(i, c)| c == '_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit()))
}
const MAX_DIM: usize = 4096;
const RESERVED: [&str; 5] = ["and", "or", "not", "in", "true"];
impl Schema {
    fn new(dim: usize, metric: Metric, fields: &[(&str, FieldType)]) -> Schema {
        Schema {
            dim,
            metric,
            fields: fields.iter().map(|(n, t)| (n.to_string(), *t)).collect(),
        }
    }
    fn validate(&self) -> Result<()> {
        if self.dim == 0 || self.dim > MAX_DIM {
            return schema_err(format!("dim must be in 1..={MAX_DIM}"));
        }
        match self
            .fields
            .keys()
            .find(|n| !is_ident(n) || RESERVED.contains(&n.as_str()) || *n == "false")
        {
            Some(n) => schema_err(format!("'{n}' is not a usable field name")),
            None => Ok(()),
        }
    }
    fn check_meta(&self, meta: &Meta) -> Result<()> {
        for (name, v) in meta {
            match self.fields.get(name) {
                None => return schema_err(format!("unknown field '{name}'")),
                Some(t) if !v.fits(*t) => {
                    return schema_err(format!("field '{name}' is {}, got {v}", t.name()));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Debug)]
struct Record {
    vector: Vec<f32>,
    meta: Meta,
}

// ============================================================ filter language ============================================================
#[derive(Clone, Copy, PartialEq, Debug)]
enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}
impl CmpOp {
    const ALL: [CmpOp; 6] = [
        CmpOp::Eq,
        CmpOp::Ne,
        CmpOp::Lt,
        CmpOp::Le,
        CmpOp::Gt,
        CmpOp::Ge,
    ];
    fn symbol(self) -> &'static str {
        ["=", "!=", "<", "<=", ">", ">="][self as usize]
    }
    fn holds(self, o: Ordering) -> bool {
        match self {
            CmpOp::Eq => o == Ordering::Equal,
            CmpOp::Ne => o != Ordering::Equal,
            CmpOp::Lt => o == Ordering::Less,
            CmpOp::Le => o != Ordering::Greater,
            CmpOp::Gt => o == Ordering::Greater,
            CmpOp::Ge => o != Ordering::Less,
        }
    }
}
/// And/Or are n-ary, so a long chain `a and b and c ...` stays shallow and cannot overflow the stack when evaluated.
#[derive(Clone, PartialEq, Debug)]
enum Filter {
    Cmp(String, CmpOp, Value),
    In(String, Vec<Value>),
    /// inclusive on both ends
    Range(String, i64, i64),
    And(Vec<Filter>),
    Or(Vec<Filter>),
    Not(Box<Filter>),
}
impl Filter {
    /// Reference semantics, row by row. The bitmap evaluation must agree with this.
    fn matches(&self, meta: &Meta) -> bool {
        match self {
            // `!=` is also false on a missing field
            Filter::Cmp(f, op, v) => meta.get(f).is_some_and(|x| op.holds(x.cmp(v))),
            Filter::In(f, set) => meta.get(f).is_some_and(|x| set.contains(x)),
            Filter::Range(f, lo, hi) => {
                matches!(meta.get(f), Some(Value::Int(x)) if lo <= x && x <= hi)
            }
            Filter::And(fs) => fs.iter().all(|f| f.matches(meta)),
            Filter::Or(fs) => fs.iter().any(|f| f.matches(meta)),
            Filter::Not(f) => !f.matches(meta),
        }
    }
    fn type_check(&self, schema: &Schema) -> Result<()> {
        let field = |name: &str| match schema.fields.get(name) {
            None => schema_err(format!("unknown field '{name}' in filter")),
            Some(FieldType::Text) => schema_err(format!(
                "text field '{name}' cannot be filtered; use a text query"
            )),
            Some(t) => Ok(*t),
        };
        let value = |name: &str, t: FieldType, v: &Value| {
            if v.fits(t) {
                Ok(())
            } else {
                schema_err(format!("'{name}' is {}, compared with {v}", t.name()))
            }
        };
        match self {
            Filter::Cmp(f, op, v) => {
                let t = field(f)?;
                if t == FieldType::Bool && !matches!(op, CmpOp::Eq | CmpOp::Ne) {
                    return schema_err(format!("'{f}' is bool: only = and != apply"));
                }
                value(f, t, v)
            }
            Filter::In(f, set) => {
                let t = field(f)?;
                set.iter().try_for_each(|v| value(f, t, v))
            }
            Filter::Range(f, ..) => match field(f)? {
                FieldType::Int => Ok(()),
                t => schema_err(format!("range needs an int field, '{f}' is {}", t.name())),
            },
            Filter::And(fs) | Filter::Or(fs) => fs.iter().try_for_each(|f| f.type_check(schema)),
            Filter::Not(f) => f.type_check(schema),
        }
    }
}
impl std::fmt::Display for Filter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let join = |f: &mut std::fmt::Formatter<'_>, fs: &[Filter], word: &str| {
            write!(f, "(")?;
            for (i, x) in fs.iter().enumerate() {
                if i > 0 {
                    write!(f, " {word} ")?;
                }
                write!(f, "{x}")?;
            }
            write!(f, ")")
        };
        match self {
            Filter::Cmp(n, op, v) => write!(f, "{n} {} {v}", op.symbol()),
            Filter::In(n, set) => write!(
                f,
                "{n} in [{}]",
                set.iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Filter::Range(n, lo, hi) => write!(f, "{n} in {lo}..{hi}"),
            Filter::And(fs) => join(f, fs, "and"),
            Filter::Or(fs) => join(f, fs, "or"),
            Filter::Not(x) => write!(f, "not {x}"),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
enum Tok {
    Ident(String),
    Lit(Value),
    Sym(&'static str),
}
fn lex(s: &str) -> Result<Vec<(usize, Tok)>> {
    let b = s.as_bytes();
    let (mut i, mut out) = (0, vec![]);
    let fail = |at, msg: &str| {
        Err(DbError::FilterSyntax {
            at,
            msg: msg.into(),
        })
    };
    while i < b.len() {
        let (start, c) = (i, b[i]);
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c.is_ascii_alphabetic() || c == b'_' {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let word = &s[start..i];
            out.push((
                start,
                match word {
                    "true" => Tok::Lit(Value::Bool(true)),
                    "false" => Tok::Lit(Value::Bool(false)),
                    _ => Tok::Ident(word.into()),
                },
            ));
        } else if c.is_ascii_digit() || (c == b'-' && b.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            i += 1;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            let Ok(n) = s[start..i].parse() else {
                return fail(start, "integer out of range");
            };
            out.push((start, Tok::Lit(Value::Int(n))));
        } else if c == b'"' {
            // walk by chars so that multi-byte characters inside strings survive
            let mut text = String::new();
            let mut chars = s[i + 1..].char_indices();
            loop {
                match chars.next() {
                    None => return fail(start, "unterminated string"),
                    Some((off, '"')) => {
                        i += off + 2;
                        break;
                    }
                    Some((_, '\\')) => match chars.next() {
                        Some((_, e @ ('"' | '\\'))) => text.push(e),
                        _ => return fail(start, "bad escape"),
                    },
                    Some((_, ch)) => text.push(ch),
                }
            }
            out.push((start, Tok::Lit(Value::Str(text))));
        } else {
            // two-character symbols first, so that "<=" is not read as "<" then "="
            let Some(sym) = [
                "..", "!=", "<=", ">=", "=", "<", ">", "(", ")", "[", "]", ",",
            ]
            .into_iter()
            .find(|p| b[i..].starts_with(p.as_bytes())) else {
                return fail(start, "unexpected character");
            };
            i += sym.len();
            out.push((start, Tok::Sym(sym)));
        }
    }
    Ok(out)
}
const MAX_DEPTH: usize = 64;
/// Recursive descent, loosest binding first: or < and < not < atom.
struct Parser {
    toks: Vec<(usize, Tok)>,
    pos: usize,
    end: usize,
}
impl Parser {
    fn parse(s: &str) -> Result<Filter> {
        let mut p = Parser {
            toks: lex(s)?,
            pos: 0,
            end: s.len(),
        };
        let f = p.or(0)?;
        if p.pos != p.toks.len() {
            return p.fail("unexpected trailing input");
        }
        Ok(f)
    }
    fn fail<T>(&self, msg: &str) -> Result<T> {
        Err(DbError::FilterSyntax {
            at: self.toks.get(self.pos).map_or(self.end, |t| t.0),
            msg: msg.into(),
        })
    }
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.1)
    }
    fn eat_word(&mut self, w: &str) -> bool {
        let hit = matches!(self.peek(), Some(Tok::Ident(x)) if x == w);
        self.pos += hit as usize;
        hit
    }
    fn eat_sym(&mut self, s: &str) -> bool {
        let hit = matches!(self.peek(), Some(Tok::Sym(x)) if *x == s);
        self.pos += hit as usize;
        hit
    }
    fn literal(&mut self) -> Result<Value> {
        match self.peek().cloned() {
            Some(Tok::Lit(v)) => {
                self.pos += 1;
                Ok(v)
            }
            _ => self.fail("expected a literal"),
        }
    }
    fn or(&mut self, depth: usize) -> Result<Filter> {
        let mut parts = vec![self.and(depth)?];
        while self.eat_word("or") {
            parts.push(self.and(depth)?);
        }
        Ok(if parts.len() == 1 {
            parts.pop().unwrap()
        } else {
            Filter::Or(parts)
        })
    }
    fn and(&mut self, depth: usize) -> Result<Filter> {
        let mut parts = vec![self.unary(depth)?];
        while self.eat_word("and") {
            parts.push(self.unary(depth)?);
        }
        Ok(if parts.len() == 1 {
            parts.pop().unwrap()
        } else {
            Filter::And(parts)
        })
    }
    fn unary(&mut self, depth: usize) -> Result<Filter> {
        // every recursive path ("not not ...", "((((") passes through here, so one limit bounds the stack
        if depth >= MAX_DEPTH {
            return self.fail("expression nested too deeply");
        }
        if self.eat_word("not") {
            return Ok(Filter::Not(Box::new(self.unary(depth + 1)?)));
        }
        if self.eat_sym("(") {
            let f = self.or(depth + 1)?;
            return if self.eat_sym(")") {
                Ok(f)
            } else {
                self.fail("expected ')'")
            };
        }
        let Some(Tok::Ident(name)) = self.peek().cloned() else {
            return self.fail("expected a field name");
        };
        if RESERVED.contains(&name.as_str()) {
            return self.fail("expected a field name, found a keyword");
        }
        self.pos += 1;
        if self.eat_word("in") {
            if self.eat_sym("[") {
                let mut set = vec![];
                if !self.eat_sym("]") {
                    loop {
                        set.push(self.literal()?);
                        if self.eat_sym("]") {
                            break;
                        }
                        if !self.eat_sym(",") {
                            return self.fail("expected ',' or ']'");
                        }
                    }
                }
                return Ok(Filter::In(name, set));
            }
            let lo = self.literal()?;
            if !self.eat_sym("..") {
                return self.fail("expected '..'");
            }
            return match (lo, self.literal()?) {
                (Value::Int(a), Value::Int(b)) => Ok(Filter::Range(name, a, b)),
                _ => self.fail("range bounds must be integers"),
            };
        }
        match CmpOp::ALL.into_iter().find(|op| self.eat_sym(op.symbol())) {
            Some(op) => Ok(Filter::Cmp(name, op, self.literal()?)),
            None => self.fail("expected a comparison or 'in'"),
        }
    }
}

// ============================================================ binary codec ============================================================
#[derive(Default)]
struct Writer(Vec<u8>);
impl Writer {
    fn u8(&mut self, x: u8) {
        self.0.push(x);
    }
    fn u32(&mut self, x: u32) {
        self.0.extend_from_slice(&x.to_le_bytes());
    }
    fn u64(&mut self, x: u64) {
        self.0.extend_from_slice(&x.to_le_bytes());
    }
    fn f32s(&mut self, v: &[f32]) {
        v.iter().for_each(|x| self.u32(x.to_bits()));
    }
    fn str(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.0.extend_from_slice(s.as_bytes());
    }
    fn value(&mut self, v: &Value) {
        match v {
            Value::Bool(b) => {
                self.u8(0);
                self.u8(*b as u8);
            }
            Value::Int(i) => {
                self.u8(1);
                self.u64(*i as u64);
            }
            Value::Str(s) => {
                self.u8(2);
                self.str(s);
            }
        }
    }
    fn record(&mut self, r: &Record) {
        self.f32s(&r.vector);
        self.u32(r.meta.len() as u32);
        for (k, v) in &r.meta {
            self.str(k);
            self.value(v);
        }
    }
    fn schema(&mut self, s: &Schema) {
        self.u32(s.dim as u32);
        self.u8(s.metric as u8);
        self.u32(s.fields.len() as u32);
        for (n, t) in &s.fields {
            self.str(n);
            self.u8(*t as u8);
        }
    }
}
/// Every read is bounds checked: input bytes are never trusted, even after the checksum.
struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .p
            .checked_add(n)
            .filter(|&e| e <= self.b.len())
            .ok_or(DbError::Corrupt("truncated"))?;
        let s = &self.b[self.p..end];
        self.p = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    /// A count of items that each need at least `min_bytes`: rejects absurd counts BEFORE any allocation.
    fn count(&mut self, min_bytes: usize) -> Result<usize> {
        let n = self.u32()? as usize;
        if n.saturating_mul(min_bytes) > self.b.len() - self.p {
            return Err(DbError::Corrupt("count exceeds remaining bytes"));
        }
        Ok(n)
    }
    fn f32s(&mut self, n: usize) -> Result<Vec<f32>> {
        Ok(self
            .take(n * 4)?
            .chunks_exact(4)
            .map(|c| f32::from_bits(u32::from_le_bytes(c.try_into().unwrap())))
            .collect())
    }
    fn str(&mut self) -> Result<String> {
        let n = self.count(1)?;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| DbError::Corrupt("invalid utf-8"))
    }
    fn value(&mut self) -> Result<Value> {
        Ok(match self.u8()? {
            0 => Value::Bool(self.u8()? != 0),
            1 => Value::Int(self.u64()? as i64),
            2 => Value::Str(self.str()?),
            _ => return Err(DbError::Corrupt("unknown value tag")),
        })
    }
    fn record(&mut self, dim: usize) -> Result<Record> {
        let vector = self.f32s(dim)?;
        let mut meta = Meta::new();
        for _ in 0..self.count(6)? {
            let k = self.str()?;
            meta.insert(k, self.value()?);
        }
        Ok(Record { vector, meta })
    }
    fn schema(&mut self) -> Result<Schema> {
        let dim = self.u32()? as usize;
        let metric = *[Metric::L2, Metric::Cosine, Metric::Dot]
            .get(self.u8()? as usize)
            .ok_or(DbError::Corrupt("unknown metric"))?;
        let mut fields = BTreeMap::new();
        for _ in 0..self.count(5)? {
            let n = self.str()?;
            fields.insert(
                n,
                *FieldType::ALL
                    .get(self.u8()? as usize)
                    .ok_or(DbError::Corrupt("unknown field type"))?,
            );
        }
        let s = Schema {
            dim,
            metric,
            fields,
        };
        s.validate()
            .map_err(|_| DbError::Corrupt("invalid schema"))?;
        Ok(s)
    }
}

// ============================================================ write-ahead log ============================================================
#[derive(Clone, PartialEq, Debug)]
enum WalOp {
    Create(String, Schema),
    Put(String, u64, Record),
    Delete(String, u64),
}
fn encode_frame(lsn: u64, op: &WalOp) -> Vec<u8> {
    let mut p = Writer::default();
    p.u64(lsn);
    match op {
        WalOp::Create(name, schema) => {
            p.u8(1);
            p.str(name);
            p.schema(schema);
        }
        WalOp::Put(name, id, rec) => {
            p.u8(2);
            p.str(name);
            p.u64(*id);
            p.u32(rec.vector.len() as u32);
            p.record(rec);
        }
        WalOp::Delete(name, id) => {
            p.u8(3);
            p.str(name);
            p.u64(*id);
        }
    }
    let mut frame = Writer::default();
    frame.u32(p.0.len() as u32);
    frame.u32(crc32(&p.0));
    frame.0.extend(p.0);
    frame.0
}
fn decode_payload(payload: &[u8]) -> Result<(u64, WalOp)> {
    let mut r = Reader { b: payload, p: 0 };
    let lsn = r.u64()?;
    let op = match r.u8()? {
        1 => WalOp::Create(r.str()?, r.schema()?),
        2 => {
            let (name, id) = (r.str()?, r.u64()?);
            let dim = r.count(4)?;
            WalOp::Put(name, id, r.record(dim)?)
        }
        3 => WalOp::Delete(r.str()?, r.u64()?),
        _ => return Err(DbError::Corrupt("unknown wal op")),
    };
    if r.p != payload.len() {
        return Err(DbError::Corrupt("trailing bytes in wal frame"));
    }
    Ok((lsn, op))
}
/// Returns the decoded operations and the length of the valid prefix. An incomplete or bad FINAL frame is a torn write; a bad frame with
/// data after it is corruption.
fn read_wal(wal: &[u8]) -> Result<(Vec<(u64, WalOp)>, usize)> {
    let (mut ops, mut p) = (vec![], 0);
    while wal.len() - p >= 8 {
        let len = u32::from_le_bytes(wal[p..p + 4].try_into().unwrap()) as usize;
        let crc = u32::from_le_bytes(wal[p + 4..p + 8].try_into().unwrap());
        let Some(end) = (p + 8).checked_add(len).filter(|&e| e <= wal.len()) else {
            break;
        };
        if crc32(&wal[p + 8..end]) != crc {
            if end < wal.len() {
                return Err(DbError::Corrupt(
                    "checksum mismatch in the middle of the wal",
                ));
            }
            break;
        }
        ops.push(decode_payload(&wal[p + 8..end])?);
        p = end;
    }
    Ok((ops, p))
}

// ============================================================ SQ8 codes ============================================================
/// One byte per dimension with a per-dimension [min, min + 255*step] range fitted on the segment.
#[derive(PartialEq)]
struct Sq8 {
    mins: Vec<f32>,
    steps: Vec<f32>,
    codes: Vec<u8>,
}
impl Sq8 {
    fn build(vectors: &[f32], dim: usize) -> Sq8 {
        let (mut mins, mut maxs) = (vec![f32::INFINITY; dim], vec![f32::NEG_INFINITY; dim]);
        for row in vectors.chunks_exact(dim) {
            for d in 0..dim {
                mins[d] = mins[d].min(row[d]);
                maxs[d] = maxs[d].max(row[d]);
            }
        }
        let steps: Vec<f32> = (0..dim)
            .map(|d| ((maxs[d] - mins[d]) / 255.0).max(1e-9))
            .collect();
        let codes = vectors
            .chunks_exact(dim)
            .flat_map(|row| {
                (0..dim)
                    .map(|d| ((row[d] - mins[d]) / steps[d]).round().clamp(0.0, 255.0) as u8)
                    .collect::<Vec<_>>()
            })
            .collect();
        Sq8 { mins, steps, codes }
    }
    fn decode(&self, row: usize) -> impl Iterator<Item = f32> + '_ {
        let dim = self.mins.len();
        self.codes[row * dim..(row + 1) * dim]
            .iter()
            .enumerate()
            .map(|(d, &c)| self.mins[d] + c as f32 * self.steps[d])
    }
    /// ASYMMETRIC distance: float query against decoded codes, without materialising the decoded vector.
    fn distance(&self, metric: Metric, q: &[f32], row: usize) -> f32 {
        let pairs = q.iter().zip(self.decode(row));
        match metric {
            Metric::L2 => pairs.map(|(x, y)| (x - y) * (x - y)).sum(),
            Metric::Cosine => 1.0 - pairs.map(|(x, y)| x * y).sum::<f32>(),
            Metric::Dot => -pairs.map(|(x, y)| x * y).sum::<f32>(),
        }
    }
}

// ============================================================ HNSW-lite ============================================================
#[derive(PartialEq, Clone, Copy)]
struct Dist(f32);
impl Eq for Dist {}
impl PartialOrd for Dist {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Dist {
    fn cmp(&self, o: &Self) -> Ordering {
        self.0.total_cmp(&o.0)
    }
}
fn sort_scored(v: &mut [(f32, u32)]) {
    v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
}

#[derive(PartialEq)]
struct Graph {
    /// links[row][layer]
    links: Vec<Vec<Vec<u32>>>,
    entry: u32,
}
impl Graph {
    /// Built with full-precision distances. `seed` fixes the level assignment, so equal input gives an equal graph.
    fn build(
        vectors: &[f32],
        dim: usize,
        metric: Metric,
        m: usize,
        efc: usize,
        seed: u64,
    ) -> Graph {
        let n = vectors.len() / dim;
        let row = |i: u32| &vectors[i as usize * dim..(i as usize + 1) * dim];
        let mut g = Graph {
            links: Vec::with_capacity(n),
            entry: 0,
        };
        let mut rng = Rng::new(seed);
        for id in 0..n as u32 {
            let level = ((-rng.unit().ln() / (m as f64).ln()) as usize).min(12);
            g.links.push(vec![vec![]; level + 1]);
            if id == 0 {
                continue;
            }
            let q = row(id);
            let score = |x: u32| metric.distance(q, row(x));
            let top = g.links[g.entry as usize].len() - 1;
            let mut ep = g.entry;
            for layer in (level + 1..=top).rev() {
                ep = g.greedy(ep, layer, &score, &mut 0);
            }
            for layer in (0..=level.min(top)).rev() {
                let found = g.search_layer(ep, layer, efc, &score, &|_| true, &mut 0);
                ep = found[0].1;
                let cap = if layer == 0 { 2 * m } else { m };
                let chosen = Graph::select(&found, m, &|a, b| metric.distance(row(a), row(b)));
                g.links[id as usize][layer] = chosen.clone();
                for nb in chosen {
                    g.links[nb as usize][layer].push(id);
                    if g.links[nb as usize][layer].len() > cap {
                        let mut c: Vec<(f32, u32)> = g.links[nb as usize][layer]
                            .iter()
                            .map(|&x| (metric.distance(row(nb), row(x)), x))
                            .collect();
                        sort_scored(&mut c);
                        g.links[nb as usize][layer] =
                            Graph::select(&c, cap, &|a, b| metric.distance(row(a), row(b)));
                    }
                }
            }
            if level > top {
                g.entry = id;
            }
        }
        g
    }
    /// Diversity heuristic: skip a candidate that is closer to an already kept neighbour than to the node, then back-fill.
    fn select(cands: &[(f32, u32)], m: usize, between: &dyn Fn(u32, u32) -> f32) -> Vec<u32> {
        let (mut kept, mut dropped): (Vec<u32>, Vec<u32>) = (vec![], vec![]);
        for &(dq, e) in cands {
            if kept.len() >= m {
                break;
            }
            if kept.iter().any(|&r| between(e, r) < dq) {
                dropped.push(e)
            } else {
                kept.push(e)
            }
        }
        let room = m - kept.len();
        kept.extend(dropped.into_iter().take(room));
        kept
    }
    fn greedy(
        &self,
        mut cur: u32,
        layer: usize,
        score: &dyn Fn(u32) -> f32,
        calls: &mut u64,
    ) -> u32 {
        let mut best = score(cur);
        *calls += 1;
        loop {
            let mut moved = false;
            for &n in &self.links[cur as usize][layer] {
                let d = score(n);
                *calls += 1;
                if d < best {
                    (best, cur, moved) = (d, n, true);
                }
            }
            if !moved {
                return cur;
            }
        }
    }
    /// Best-first search. Every node can be EXPANDED; only nodes passing `allow` can be RESULTS (this is in-filter search).
    fn search_layer(
        &self,
        ep: u32,
        layer: usize,
        ef: usize,
        score: &dyn Fn(u32) -> f32,
        allow: &dyn Fn(u32) -> bool,
        calls: &mut u64,
    ) -> Vec<(f32, u32)> {
        let mut seen = Bitmap::empty(self.links.len());
        seen.set(ep as usize);
        let d0 = Dist(score(ep));
        *calls += 1;
        let mut frontier = BinaryHeap::from([Reverse((d0, ep))]);
        let mut best: BinaryHeap<(Dist, u32)> = BinaryHeap::new();
        if allow(ep) {
            best.push((d0, ep));
        }
        while let Some(Reverse((d, u))) = frontier.pop() {
            if best.len() >= ef && d > best.peek().unwrap().0 {
                break;
            }
            for &v in &self.links[u as usize][layer] {
                if seen.get(v as usize) {
                    continue;
                }
                seen.set(v as usize);
                let dv = Dist(score(v));
                *calls += 1;
                if best.len() < ef || dv < best.peek().unwrap().0 {
                    frontier.push(Reverse((dv, v)));
                    if allow(v) {
                        best.push((dv, v));
                        if best.len() > ef {
                            best.pop();
                        }
                    }
                }
            }
        }
        let mut out: Vec<(f32, u32)> = best.into_iter().map(|(d, i)| (d.0, i)).collect();
        sort_scored(&mut out);
        out
    }
    fn search(
        &self,
        ef: usize,
        score: &dyn Fn(u32) -> f32,
        allow: &dyn Fn(u32) -> bool,
        calls: &mut u64,
    ) -> Vec<(f32, u32)> {
        let mut ep = self.entry;
        for layer in (1..self.links[ep as usize].len()).rev() {
            ep = self.greedy(ep, layer, score, calls);
        }
        self.search_layer(ep, 0, ef, score, allow, calls)
    }
}

// ============================================================ text ============================================================
fn tokenize(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}
/// All tokens of the schema's text fields, in field-name order.
fn doc_tokens(schema: &Schema, meta: &Meta) -> Vec<String> {
    meta.iter()
        .filter(|(k, _)| schema.fields.get(*k) == Some(&FieldType::Text))
        .flat_map(|(_, v)| {
            if let Value::Str(s) = v {
                tokenize(s)
            } else {
                vec![]
            }
        })
        .collect()
}
const BM25_K1: f64 = 1.2;
const BM25_B: f64 = 0.75;
fn bm25_term(tf: f64, len: f64, avg_len: f64, idf: f64) -> f64 {
    idf * tf * (BM25_K1 + 1.0) / (tf + BM25_K1 * (1.0 - BM25_B + BM25_B * len / avg_len))
}
fn bm25_idf(n_docs: usize, df: usize) -> f64 {
    (1.0 + (n_docs as f64 - df as f64 + 0.5) / (df as f64 + 0.5)).ln()
}

// ============================================================ immutable segment ============================================================
#[derive(Clone, Debug)]
struct Config {
    /// memtable rows that trigger a flush
    flush_at: usize,
    /// segments with at most this many rows get no graph
    flat_threshold: usize,
    /// filters matching at most this many rows of a graph segment are scanned exactly
    exact_scan_limit: usize,
    /// at or above this selectivity use POST filtering, below it IN filtering
    post_min_selectivity: f64,
    m: usize,
    ef_construction: usize,
    ef_search: usize,
    oversample: usize,
    /// more segments than this triggers a merge of the smallest ones
    max_segments: usize,
    /// a segment with a larger dead fraction is rewritten
    max_dead_ratio: f64,
    auto_compact: bool,
}
impl Default for Config {
    fn default() -> Config {
        Config {
            flush_at: 500,
            flat_threshold: 128,
            exact_scan_limit: 100,
            post_min_selectivity: 0.6,
            m: 12,
            ef_construction: 80,
            ef_search: 64,
            oversample: 4,
            max_segments: 4,
            max_dead_ratio: 0.3,
            auto_compact: true,
        }
    }
}

struct Segment {
    sid: u64,
    ids: Vec<u64>,
    /// full precision, "cold": read only for rescoring and exact scans
    vectors: Vec<f32>,
    metas: Vec<Meta>,
    sq: Sq8,
    graph: Option<Graph>,
    /// field -> value -> rows
    index: BTreeMap<String, BTreeMap<Value, Bitmap>>,
    /// term -> (row, term frequency), sorted by row
    postings: BTreeMap<String, Vec<(u32, u32)>>,
    doc_len: Vec<u32>,
}
impl Segment {
    /// `rows` must be sorted by id. A `graph` is supplied when loading a snapshot, so the graph is restored rather than rebuilt.
    fn assemble(
        sid: u64,
        schema: &Schema,
        rows: Vec<(u64, Record)>,
        graph: Option<Graph>,
    ) -> Segment {
        let n = rows.len();
        let (mut ids, mut vectors, mut metas) = (
            Vec::with_capacity(n),
            Vec::with_capacity(n * schema.dim),
            Vec::with_capacity(n),
        );
        for (id, r) in rows {
            ids.push(id);
            vectors.extend(r.vector);
            metas.push(r.meta);
        }
        let mut index: BTreeMap<String, BTreeMap<Value, Bitmap>> = BTreeMap::new();
        let mut postings: BTreeMap<String, Vec<(u32, u32)>> = BTreeMap::new();
        let mut doc_len = vec![0u32; n];
        for (row, meta) in metas.iter().enumerate() {
            for (field, v) in meta {
                if schema.fields[field] != FieldType::Text {
                    index
                        .entry(field.clone())
                        .or_default()
                        .entry(v.clone())
                        .or_insert_with(|| Bitmap::empty(n))
                        .set(row);
                }
            }
            let tokens = doc_tokens(schema, meta);
            doc_len[row] = tokens.len() as u32;
            let mut tf: BTreeMap<String, u32> = BTreeMap::new();
            tokens
                .into_iter()
                .for_each(|t| *tf.entry(t).or_default() += 1);
            for (t, c) in tf {
                postings.entry(t).or_default().push((row as u32, c));
            }
        }
        let sq = Sq8::build(&vectors, schema.dim);
        Segment {
            sid,
            ids,
            vectors,
            metas,
            sq,
            graph,
            index,
            postings,
            doc_len,
        }
    }
    fn build(sid: u64, schema: &Schema, rows: Vec<(u64, Record)>, cfg: &Config) -> Segment {
        let mut s = Segment::assemble(sid, schema, rows, None);
        if s.len() > cfg.flat_threshold {
            s.graph = Some(Graph::build(
                &s.vectors,
                schema.dim,
                schema.metric,
                cfg.m,
                cfg.ef_construction,
                sid,
            ));
        }
        s
    }
    fn len(&self) -> usize {
        self.ids.len()
    }
    fn vector(&self, row: usize) -> &[f32] {
        let dim = self.vectors.len() / self.len();
        &self.vectors[row * dim..(row + 1) * dim]
    }
    fn record(&self, row: usize) -> Record {
        Record {
            vector: self.vector(row).to_vec(),
            meta: self.metas[row].clone(),
        }
    }
    /// Evaluate a type-checked filter with the inverted index: the result must equal `Filter::matches` row by row.
    fn eval(&self, f: &Filter) -> Bitmap {
        let n = self.len();
        let union =
            |field: &str, lo: Bound<Value>, hi: Bound<Value>, keep: &dyn Fn(&Value) -> bool| {
                let mut out = Bitmap::empty(n);
                if let Some(values) = self.index.get(field) {
                    values
                        .range((lo, hi))
                        .filter(|(v, _)| keep(v))
                        .for_each(|(_, b)| out.or_assign(b));
                }
                out
            };
        match f {
            Filter::Cmp(name, op, v) => {
                use Bound::{Excluded, Included, Unbounded};
                let (lo, hi) = match op {
                    CmpOp::Eq => (Included(v.clone()), Included(v.clone())),
                    CmpOp::Ne => (Unbounded, Unbounded),
                    CmpOp::Lt => (Unbounded, Excluded(v.clone())),
                    CmpOp::Le => (Unbounded, Included(v.clone())),
                    CmpOp::Gt => (Excluded(v.clone()), Unbounded),
                    CmpOp::Ge => (Included(v.clone()), Unbounded),
                };
                union(name, lo, hi, &|x| *op != CmpOp::Ne || x != v)
            }
            Filter::In(name, set) => {
                let mut out = Bitmap::empty(n);
                for v in set {
                    if let Some(b) = self.index.get(name).and_then(|m| m.get(v)) {
                        out.or_assign(b);
                    }
                }
                out
            }
            Filter::Range(name, lo, hi) => {
                // BTreeMap::range panics when start > end, and an inverted range is a legal (empty) query
                if lo > hi {
                    Bitmap::empty(n)
                } else {
                    union(
                        name,
                        Bound::Included(Value::Int(*lo)),
                        Bound::Included(Value::Int(*hi)),
                        &|_| true,
                    )
                }
            }
            Filter::And(fs) => fs.iter().fold(Bitmap::full(n), |mut acc, f| {
                acc.and_assign(&self.eval(f));
                acc
            }),
            Filter::Or(fs) => fs.iter().fold(Bitmap::empty(n), |mut acc, f| {
                acc.or_assign(&self.eval(f));
                acc
            }),
            Filter::Not(f) => {
                let mut b = self.eval(f);
                b.not_assign();
                b
            }
        }
    }
}

// ============================================================ metrics ============================================================
#[derive(Default)]
struct Metrics {
    writes: AtomicU64,
    deletes: AtomicU64,
    queries: AtomicU64,
    flushes: AtomicU64,
    compactions: AtomicU64,
    stale_compactions: AtomicU64,
    checkpoints: AtomicU64,
    wal_bytes: AtomicU64,
    wal_replayed: AtomicU64,
    plan_flat: AtomicU64,
    plan_pre: AtomicU64,
    plan_post: AtomicU64,
    plan_in: AtomicU64,
    plan_fallback: AtomicU64,
    distance_calls: AtomicU64,
}
impl Metrics {
    fn rows(&self) -> Vec<(&'static str, u64)> {
        macro_rules! rows {
            ($($f:ident),*) => { vec![$((stringify!($f), self.$f.load(Relaxed))),*] };
        }
        rows!(
            writes,
            deletes,
            queries,
            flushes,
            compactions,
            stale_compactions,
            checkpoints,
            wal_bytes,
            wal_replayed,
            plan_flat,
            plan_pre,
            plan_post,
            plan_in,
            plan_fallback,
            distance_calls
        )
    }
}
fn bump(c: &AtomicU64, by: u64) {
    c.fetch_add(by, Relaxed);
}

// ============================================================ collection ============================================================
#[derive(Clone, Copy, PartialEq, Debug)]
enum Loc {
    Mem,
    Seg(u64, u32),
}
#[derive(Clone)]
struct SegHandle {
    seg: Arc<Segment>,
    dead: Bitmap,
}
impl SegHandle {
    fn live(&self) -> usize {
        self.seg.len() - self.dead.count()
    }
}
struct Collection {
    schema: Schema,
    mem: BTreeMap<u64, Record>,
    segments: Vec<SegHandle>,
    /// where the one live version of every id is
    loc: BTreeMap<u64, Loc>,
    next_sid: u64,
}
/// Inputs of one compaction, captured under the read lock.
struct CompactionPlan {
    collection: String,
    schema: Schema,
    inputs: Vec<SegHandle>,
}
struct CompactionJob {
    collection: String,
    input_sids: Vec<u64>,
    /// None when every input row is dead
    output: Option<Segment>,
}
impl Collection {
    fn new(schema: Schema) -> Collection {
        Collection {
            schema,
            mem: BTreeMap::new(),
            segments: vec![],
            loc: BTreeMap::new(),
            next_sid: 1,
        }
    }
    fn handle(&self, sid: u64) -> Option<&SegHandle> {
        self.segments.iter().find(|h| h.seg.sid == sid)
    }
    fn delete(&mut self, id: u64) -> bool {
        match self.loc.remove(&id) {
            None => false,
            Some(Loc::Mem) => self.mem.remove(&id).is_some(),
            Some(Loc::Seg(sid, row)) => {
                let h = self
                    .segments
                    .iter_mut()
                    .find(|h| h.seg.sid == sid)
                    .expect("loc points at a live segment");
                h.dead.set(row as usize);
                true
            }
        }
    }
    fn put(&mut self, id: u64, rec: Record) {
        self.delete(id);
        self.mem.insert(id, rec);
        self.loc.insert(id, Loc::Mem);
    }
    fn get(&self, id: u64) -> Option<Record> {
        match *self.loc.get(&id)? {
            Loc::Mem => self.mem.get(&id).cloned(),
            Loc::Seg(sid, row) => Some(self.handle(sid)?.seg.record(row as usize)),
        }
    }
    fn flush(&mut self, cfg: &Config) -> bool {
        if self.mem.is_empty() {
            return false;
        }
        let rows: Vec<(u64, Record)> = std::mem::take(&mut self.mem).into_iter().collect(); // BTreeMap: sorted by id
        let sid = self.next_sid;
        self.next_sid += 1;
        let seg = Arc::new(Segment::build(sid, &self.schema, rows, cfg));
        for (row, &id) in seg.ids.iter().enumerate() {
            self.loc.insert(id, Loc::Seg(sid, row as u32));
        }
        let dead = Bitmap::empty(seg.len());
        self.segments.push(SegHandle { seg, dead });
        true
    }
    /// Policy: rewrite segments with too many tombstones; if there are still too many segments, also merge the smallest.
    fn pick_compaction(&self, cfg: &Config, force: bool) -> Vec<SegHandle> {
        if force {
            let worth = self.segments.len() > 1 || self.segments.iter().any(|h| h.dead.count() > 0);
            return if worth { self.segments.clone() } else { vec![] };
        }
        let mut by_size: Vec<&SegHandle> = self.segments.iter().collect();
        by_size.sort_by_key(|h| (h.live(), h.seg.sid));
        let mut picked: BTreeSet<u64> = self
            .segments
            .iter()
            .filter(|h| h.dead.count() as f64 > cfg.max_dead_ratio * h.seg.len() as f64)
            .map(|h| h.seg.sid)
            .collect();
        if self.segments.len() > cfg.max_segments {
            // merging j segments into one removes j - 1
            let need = self.segments.len() - cfg.max_segments + 1;
            picked.extend(by_size.iter().take(need.max(2)).map(|h| h.seg.sid));
        }
        self.segments
            .iter()
            .filter(|h| picked.contains(&h.seg.sid))
            .cloned()
            .collect()
    }
    /// Swap the inputs for the output. Returns false (and changes nothing) if any input is gone.
    fn commit(&mut self, job: CompactionJob) -> bool {
        if !job.input_sids.iter().all(|&s| self.handle(s).is_some()) {
            return false;
        }
        if let Some(mut seg) = job.output {
            // the id reserved here is final; the graph seed used while building does not need to match it
            let sid = self.next_sid;
            self.next_sid += 1;
            seg.sid = sid;
            let mut dead = Bitmap::empty(seg.len());
            for (row, &id) in seg.ids.iter().enumerate() {
                match self.loc.get(&id) {
                    // still living in one of the inputs: nothing changed since the plan, so repoint
                    Some(&Loc::Seg(s, _)) if job.input_sids.contains(&s) => {
                        self.loc.insert(id, Loc::Seg(sid, row as u32));
                    }
                    // deleted, or updated into the memtable or a newer segment: the copy we built is stale
                    _ => dead.set(row),
                }
            }
            self.segments.push(SegHandle {
                seg: Arc::new(seg),
                dead,
            });
        }
        self.segments
            .retain(|h| !job.input_sids.contains(&h.seg.sid));
        true
    }
}
/// The slow step. It touches only immutable data and runs with NO lock held.
fn build_compaction(plan: CompactionPlan, cfg: &Config) -> CompactionJob {
    let mut rows: Vec<(u64, Record)> = vec![];
    for h in &plan.inputs {
        rows.extend(
            (0..h.seg.len())
                .filter(|&r| !h.dead.get(r))
                .map(|r| (h.seg.ids[r], h.seg.record(r))),
        );
    }
    rows.sort_by_key(|r| r.0);
    let input_sids: Vec<u64> = plan.inputs.iter().map(|h| h.seg.sid).collect();
    let seed = input_sids.iter().max().copied().unwrap_or(0) + 1;
    let output = (!rows.is_empty()).then(|| Segment::build(seed, &plan.schema, rows, cfg));
    CompactionJob {
        collection: plan.collection,
        input_sids,
        output,
    }
}

// ============================================================ queries ============================================================
#[derive(Clone, Debug, Default)]
struct Query {
    vector: Option<Vec<f32>>,
    text: Option<String>,
    top_k: usize,
    filter: Option<Filter>,
    /// keep only hits with distance <= this (vector queries)
    max_distance: Option<f32>,
    ef: Option<usize>,
    exact: bool,
}
/// Builder: collects errors from the filter string and reports the first one at `build`.
struct QueryBuilder {
    q: Query,
    error: Option<DbError>,
}
impl Query {
    fn builder() -> QueryBuilder {
        QueryBuilder {
            q: Query {
                top_k: 10,
                ..Query::default()
            },
            error: None,
        }
    }
}
const MAX_TOP_K: usize = 10_000;
impl QueryBuilder {
    fn vector(mut self, v: &[f32]) -> Self {
        self.q.vector = Some(v.to_vec());
        self
    }
    fn text(mut self, t: &str) -> Self {
        self.q.text = Some(t.to_string());
        self
    }
    fn top_k(mut self, k: usize) -> Self {
        self.q.top_k = k;
        self
    }
    fn filter(mut self, f: Filter) -> Self {
        self.q.filter = Some(f);
        self
    }
    fn filter_str(mut self, s: &str) -> Self {
        match Parser::parse(s) {
            Ok(f) => self.q.filter = Some(f),
            Err(e) => self.error = self.error.or(Some(e)),
        }
        self
    }
    fn max_distance(mut self, d: f32) -> Self {
        self.q.max_distance = Some(d);
        self
    }
    fn ef(mut self, ef: usize) -> Self {
        self.q.ef = Some(ef);
        self
    }
    fn exact(mut self, yes: bool) -> Self {
        self.q.exact = yes;
        self
    }
    fn build(self) -> Result<Query> {
        let bad = |m: &str| Err(DbError::BadQuery(m.into()));
        if let Some(e) = self.error {
            return Err(e);
        }
        let q = self.q;
        if q.vector.is_none() && q.text.is_none() {
            return bad("need a vector, a text, or both");
        }
        if q.top_k == 0 || q.top_k > MAX_TOP_K {
            return bad("top_k must be in 1..=10000");
        }
        if q.max_distance.is_some() && (q.text.is_some() || q.vector.is_none()) {
            return bad(
                "a distance threshold applies to pure vector queries only (rank-fusion scores are not distances)",
            );
        }
        if q.max_distance.is_some_and(|d| d.is_nan()) || q.ef.is_some_and(|e| e > 1_000_000) {
            return bad("threshold or ef out of range");
        }
        Ok(q)
    }
}
#[derive(Clone, PartialEq, Debug)]
struct Hit {
    id: u64,
    /// vector: distance (lower is better); text: BM25; hybrid: fused score (both higher is better)
    score: f64,
}

impl Collection {
    /// Exact distances for the given rows of one segment.
    fn exact_rows(
        &self,
        h: &SegHandle,
        rows: impl Iterator<Item = usize>,
        q: &[f32],
        m: &Metrics,
    ) -> Vec<(f32, u64)> {
        let out: Vec<(f32, u64)> = rows
            .map(|r| {
                (
                    self.schema.metric.distance(q, h.seg.vector(r)),
                    h.seg.ids[r],
                )
            })
            .collect();
        bump(&m.distance_calls, out.len() as u64);
        out
    }
    fn search_segment(
        &self,
        h: &SegHandle,
        q: &[f32],
        query: &Query,
        k: usize,
        cfg: &Config,
        m: &Metrics,
    ) -> Vec<(f32, u64)> {
        let seg = &h.seg;
        let mut allowed = query
            .filter
            .as_ref()
            .map_or_else(|| Bitmap::full(seg.len()), |f| seg.eval(f));
        allowed.andnot_assign(&h.dead);
        let matches = allowed.count();
        if matches == 0 {
            return vec![];
        }
        let graph = match &seg.graph {
            Some(g) if !query.exact && matches > cfg.exact_scan_limit => g,
            other => {
                bump(
                    if other.is_none() {
                        &m.plan_flat
                    } else {
                        &m.plan_pre
                    },
                    1,
                );
                return self.exact_rows(h, allowed.iter(), q, m);
            }
        };
        let metric = self.schema.metric;
        let want = (k * cfg.oversample.max(1)).min(matches);
        let ef = query.ef.unwrap_or(cfg.ef_search).max(want);
        let selectivity = matches as f64 / seg.len() as f64;
        let score = |row: u32| seg.sq.distance(metric, q, row as usize);
        let mut calls = 0;
        let candidates: Vec<(f32, u32)> = if selectivity >= cfg.post_min_selectivity {
            bump(&m.plan_post, 1);
            // over-fetch by 1/selectivity so that about `ef` candidates survive the filter
            let wide = ((ef as f64 / selectivity).ceil() as usize).min(seg.len());
            graph
                .search(wide, &score, &|_| true, &mut calls)
                .into_iter()
                .filter(|c| allowed.get(c.1 as usize))
                .take(want)
                .collect()
        } else {
            bump(&m.plan_in, 1);
            graph
                .search(ef, &score, &|row| allowed.get(row as usize), &mut calls)
                .into_iter()
                .take(want)
                .collect()
        };
        bump(&m.distance_calls, calls);
        if candidates.len() < k.min(matches) {
            // never return a silently short answer
            bump(&m.plan_fallback, 1);
            return self.exact_rows(h, allowed.iter(), q, m);
        }
        // RESCORE: the SQ8 order is only a candidate generator
        self.exact_rows(h, candidates.into_iter().map(|c| c.1 as usize), q, m)
    }
    fn vector_search(
        &self,
        q: &[f32],
        query: &Query,
        k: usize,
        cfg: &Config,
        m: &Metrics,
    ) -> Vec<(f32, u64)> {
        let metric = self.schema.metric;
        let mut all: Vec<(f32, u64)> = self
            .mem
            .iter()
            .filter(|(_, r)| query.filter.as_ref().is_none_or(|f| f.matches(&r.meta)))
            .map(|(id, r)| (metric.distance(q, &r.vector), *id))
            .collect();
        bump(&m.distance_calls, all.len() as u64);
        for h in &self.segments {
            let mut part = self.search_segment(h, q, query, k, cfg, m);
            part.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            part.truncate(k);
            all.extend(part);
        }
        all.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        all.truncate(k);
        if let Some(t) = query.max_distance {
            all.retain(|h| h.0 <= t);
        }
        all
    }
    /// BM25 over live rows with global statistics. The filter limits which rows are RETURNED, not the statistics.
    fn text_search(&self, text: &str, filter: Option<&Filter>, k: usize) -> Vec<(f64, u64)> {
        let mut terms = tokenize(text);
        terms.sort();
        terms.dedup();
        let mem_docs: Vec<(u64, &Record, Vec<String>)> = self
            .mem
            .iter()
            .map(|(id, r)| (*id, r, doc_tokens(&self.schema, &r.meta)))
            .collect();
        let n_docs = self.loc.len();
        let mut total_len: u64 = mem_docs.iter().map(|d| d.2.len() as u64).sum();
        for h in &self.segments {
            total_len += (0..h.seg.len())
                .filter(|&r| !h.dead.get(r))
                .map(|r| h.seg.doc_len[r] as u64)
                .sum::<u64>();
        }
        if total_len == 0 {
            return vec![];
        }
        let avg_len = total_len as f64 / n_docs as f64;
        let allowed: Vec<Bitmap> = self
            .segments
            .iter()
            .map(|h| filter.map_or_else(|| Bitmap::full(h.seg.len()), |f| h.seg.eval(f)))
            .collect();
        let mut scores: BTreeMap<u64, f64> = BTreeMap::new();
        for term in &terms {
            // (id, tf, len, passes filter) for every LIVE doc containing the term
            let mut found: Vec<(u64, f64, f64, bool)> = vec![];
            for (id, r, tokens) in &mem_docs {
                let tf = tokens.iter().filter(|t| *t == term).count();
                if tf > 0 {
                    found.push((
                        *id,
                        tf as f64,
                        tokens.len() as f64,
                        filter.is_none_or(|f| f.matches(&r.meta)),
                    ));
                }
            }
            for (h, ok) in self.segments.iter().zip(&allowed) {
                for &(row, tf) in h.seg.postings.get(term).map_or(&[][..], |v| v) {
                    if !h.dead.get(row as usize) {
                        found.push((
                            h.seg.ids[row as usize],
                            tf as f64,
                            h.seg.doc_len[row as usize] as f64,
                            ok.get(row as usize),
                        ));
                    }
                }
            }
            let idf = bm25_idf(n_docs, found.len());
            for (id, tf, len, ok) in found {
                if ok {
                    *scores.entry(id).or_default() += bm25_term(tf, len, avg_len, idf);
                }
            }
        }
        let mut out: Vec<(f64, u64)> = scores.into_iter().map(|(id, s)| (s, id)).collect();
        out.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        out.truncate(k);
        out
    }
    fn query(&self, query: &Query, cfg: &Config, m: &Metrics) -> Result<Vec<Hit>> {
        bump(&m.queries, 1);
        if let Some(f) = &query.filter {
            f.type_check(&self.schema)?;
        }
        if query.text.is_some() && !self.schema.fields.values().any(|t| *t == FieldType::Text) {
            return Err(DbError::BadQuery(
                "this collection has no text field".into(),
            ));
        }
        let vector = query
            .vector
            .as_ref()
            .map(|v| self.schema.metric.prepare(v, self.schema.dim))
            .transpose()?;
        Ok(match (vector, &query.text) {
            (Some(v), None) => self
                .vector_search(&v, query, query.top_k, cfg, m)
                .into_iter()
                .map(|(d, id)| Hit {
                    id,
                    score: d as f64,
                })
                .collect(),
            (None, Some(t)) => self
                .text_search(t, query.filter.as_ref(), query.top_k)
                .into_iter()
                .map(|(s, id)| Hit { id, score: s })
                .collect(),
            (Some(v), Some(t)) => {
                let depth = (query.top_k * 4).max(50);
                let dense: Vec<u64> = self
                    .vector_search(&v, query, depth, cfg, m)
                    .into_iter()
                    .map(|p| p.1)
                    .collect();
                let lexical: Vec<u64> = self
                    .text_search(t, query.filter.as_ref(), depth)
                    .into_iter()
                    .map(|p| p.1)
                    .collect();
                rrf(&[dense, lexical], query.top_k)
            }
            (None, None) => return Err(DbError::BadQuery("empty query".into())),
        })
    }
    fn count(&self, filter: Option<&Filter>) -> Result<usize> {
        let Some(f) = filter else {
            return Ok(self.loc.len());
        };
        f.type_check(&self.schema)?;
        let in_segments: usize = self
            .segments
            .iter()
            .map(|h| {
                let mut b = h.seg.eval(f);
                b.andnot_assign(&h.dead);
                b.count()
            })
            .sum();
        Ok(in_segments + self.mem.values().filter(|r| f.matches(&r.meta)).count())
    }
}
const RRF_C: f64 = 60.0;
fn rrf(lists: &[Vec<u64>], k: usize) -> Vec<Hit> {
    let mut score: BTreeMap<u64, f64> = BTreeMap::new();
    for list in lists {
        for (rank, id) in list.iter().enumerate() {
            *score.entry(*id).or_default() += 1.0 / (RRF_C + rank as f64 + 1.0);
        }
    }
    let mut out: Vec<Hit> = score
        .into_iter()
        .map(|(id, score)| Hit { id, score })
        .collect();
    out.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.id.cmp(&b.id)));
    out.truncate(k);
    out
}

// ============================================================ snapshot format ============================================================
const SNAPSHOT_MAGIC: &[u8; 4] = b"MVDB";
const SNAPSHOT_VERSION: u32 = 1;
fn encode_snapshot(last_lsn: u64, collections: &BTreeMap<String, Collection>) -> Vec<u8> {
    let mut w = Writer::default();
    w.0.extend_from_slice(SNAPSHOT_MAGIC);
    w.u32(SNAPSHOT_VERSION);
    w.u64(last_lsn);
    w.u32(collections.len() as u32);
    for (name, c) in collections {
        w.str(name);
        w.schema(&c.schema);
        w.u64(c.next_sid);
        w.u32(c.mem.len() as u32);
        for (id, r) in &c.mem {
            w.u64(*id);
            w.record(r);
        }
        w.u32(c.segments.len() as u32);
        for h in &c.segments {
            w.u64(h.seg.sid);
            w.u32(h.seg.len() as u32);
            for row in 0..h.seg.len() {
                w.u64(h.seg.ids[row]);
                w.record(&h.seg.record(row));
                w.u8(h.dead.get(row) as u8);
            }
            w.u8(h.seg.graph.is_some() as u8);
            if let Some(g) = &h.seg.graph {
                w.u32(g.entry);
                for layers in &g.links {
                    w.u32(layers.len() as u32);
                    for l in layers {
                        w.u32(l.len() as u32);
                        l.iter().for_each(|&x| w.u32(x));
                    }
                }
            }
        }
    }
    let crc = crc32(&w.0);
    w.u32(crc);
    w.0
}
fn decode_graph(r: &mut Reader, n: usize) -> Result<Graph> {
    let entry = r.u32()?;
    let mut links = Vec::with_capacity(n);
    for _ in 0..n {
        let layers = r.count(4)?;
        let mut node = Vec::with_capacity(layers);
        for _ in 0..layers {
            let deg = r.count(4)?;
            node.push((0..deg).map(|_| r.u32()).collect::<Result<Vec<u32>>>()?);
        }
        links.push(node);
    }
    // structural validation: a search must never index out of bounds, whatever the bytes said
    let g = Graph { links, entry };
    let top = g
        .links
        .get(entry as usize)
        .map(Vec::len)
        .ok_or(DbError::Corrupt("graph entry out of range"))?;
    for node in &g.links {
        if node.is_empty() || node.len() > top {
            return Err(DbError::Corrupt("bad node level"));
        }
        for (layer, l) in node.iter().enumerate() {
            if l.iter()
                .any(|&x| g.links.get(x as usize).is_none_or(|t| t.len() <= layer))
            {
                return Err(DbError::Corrupt("edge to a node that is not on this layer"));
            }
        }
    }
    Ok(g)
}
fn decode_snapshot(bytes: &[u8]) -> Result<(u64, BTreeMap<String, Collection>)> {
    if bytes.len() < 12 || &bytes[..4] != SNAPSHOT_MAGIC {
        return Err(DbError::Corrupt("not a snapshot"));
    }
    let (body, tail) = bytes.split_at(bytes.len() - 4);
    if crc32(body) != u32::from_le_bytes(tail.try_into().unwrap()) {
        return Err(DbError::Corrupt("snapshot checksum mismatch"));
    }
    let mut r = Reader { b: body, p: 4 };
    let version = r.u32()?;
    if version != SNAPSHOT_VERSION {
        return Err(DbError::Version(version));
    }
    let last_lsn = r.u64()?;
    let mut collections = BTreeMap::new();
    for _ in 0..r.count(4)? {
        let name = r.str()?;
        let schema = r.schema()?;
        let mut c = Collection::new(schema.clone());
        c.next_sid = r.u64()?;
        // every row must match the schema and every id may be live only once
        let admit = |c: &mut Collection, id: u64, rec: &Record, loc: Option<Loc>| -> Result<()> {
            c.schema
                .check_meta(&rec.meta)
                .map_err(|_| DbError::Corrupt("row violates the schema"))?;
            match loc {
                Some(l) if c.loc.insert(id, l).is_some() => {
                    Err(DbError::Corrupt("id is live twice"))
                }
                _ => Ok(()),
            }
        };
        for _ in 0..r.count(8 + schema.dim * 4)? {
            let id = r.u64()?;
            let rec = r.record(schema.dim)?;
            admit(&mut c, id, &rec, Some(Loc::Mem))?;
            c.mem.insert(id, rec);
        }
        for _ in 0..r.count(13)? {
            let sid = r.u64()?;
            let n = r.count(9 + schema.dim * 4)?;
            if n == 0 || sid >= c.next_sid || c.handle(sid).is_some() {
                return Err(DbError::Corrupt("bad segment header"));
            }
            let (mut rows, mut dead) = (Vec::with_capacity(n), Bitmap::empty(n));
            for row in 0..n {
                let id = r.u64()?;
                let rec = r.record(schema.dim)?;
                let is_dead = r.u8()? != 0;
                admit(
                    &mut c,
                    id,
                    &rec,
                    (!is_dead).then_some(Loc::Seg(sid, row as u32)),
                )?;
                if is_dead {
                    dead.set(row);
                }
                rows.push((id, rec));
            }
            let graph = match r.u8()? {
                0 => None,
                _ => Some(decode_graph(&mut r, n)?),
            };
            c.segments.push(SegHandle {
                seg: Arc::new(Segment::assemble(sid, &schema, rows, graph)),
                dead,
            });
        }
        collections.insert(name, c);
    }
    if r.p != body.len() {
        return Err(DbError::Corrupt("trailing bytes in snapshot"));
    }
    Ok((last_lsn, collections))
}

// ============================================================ the database ============================================================
#[derive(Clone, Default)]
struct Disk {
    snapshot: Option<Vec<u8>>,
    wal: Vec<u8>,
}
#[derive(Clone, Copy, PartialEq, Debug)]
enum CrashPoint {
    /// the next append persists only this many bytes of its frame
    TornAppend(usize),
    /// the next checkpoint replaces the snapshot but dies before truncating the WAL
    AfterSnapshotBeforeTruncate,
}
struct Inner {
    disk: Disk,
    collections: BTreeMap<String, Collection>,
    last_lsn: u64,
    crash: Option<CrashPoint>,
    crashed: bool,
}
impl Inner {
    fn alive(&self) -> Result<()> {
        if self.crashed {
            Err(DbError::Crashed)
        } else {
            Ok(())
        }
    }
    fn coll(&self, name: &str) -> Result<&Collection> {
        self.collections
            .get(name)
            .ok_or_else(|| DbError::NoCollection(name.into()))
    }
    fn coll_mut(&mut self, name: &str) -> Result<&mut Collection> {
        self.collections
            .get_mut(name)
            .ok_or_else(|| DbError::NoCollection(name.into()))
    }
    /// Make `op` durable, then apply it. Callers have already validated it.
    fn log_and_apply(&mut self, op: WalOp, cfg: &Config, m: &Metrics) -> Result<()> {
        let frame = encode_frame(self.last_lsn + 1, &op);
        if let Some(CrashPoint::TornAppend(keep)) = self.crash {
            self.disk
                .wal
                .extend_from_slice(&frame[..keep.min(frame.len() - 1)]);
            self.crashed = true;
            return Err(DbError::Crashed);
        }
        self.disk.wal.extend_from_slice(&frame);
        bump(&m.wal_bytes, frame.len() as u64);
        self.last_lsn += 1;
        self.apply(op, cfg, m)
    }
    /// Shared by the live write path and by recovery.
    fn apply(&mut self, op: WalOp, cfg: &Config, m: &Metrics) -> Result<()> {
        match op {
            WalOp::Create(name, schema) => {
                self.collections.insert(name, Collection::new(schema));
            }
            WalOp::Put(name, id, rec) => {
                let c = self.coll_mut(&name)?;
                if rec.vector.len() != c.schema.dim || c.schema.check_meta(&rec.meta).is_err() {
                    return Err(DbError::Corrupt("logged row violates the schema"));
                }
                c.put(id, rec);
                if c.mem.len() >= cfg.flush_at && c.flush(cfg) {
                    bump(&m.flushes, 1);
                }
            }
            WalOp::Delete(name, id) => {
                self.coll_mut(&name)?.delete(id);
            }
        }
        Ok(())
    }
}

/// The handle shared between threads: every method takes `&self`.
struct Db {
    cfg: Config,
    inner: RwLock<Inner>,
    metrics: Metrics,
}
impl Db {
    /// Recovery IS the constructor: snapshot, then WAL records newer than it, then cut off a torn tail.
    fn open(mut disk: Disk, cfg: Config) -> Result<Db> {
        let metrics = Metrics::default();
        let (snap_lsn, collections) = match &disk.snapshot {
            Some(bytes) => decode_snapshot(bytes)?,
            None => (0, BTreeMap::new()),
        };
        let (ops, valid) = read_wal(&disk.wal)?;
        disk.wal.truncate(valid); // new appends must not sit behind garbage
        let mut inner = Inner {
            disk,
            collections,
            last_lsn: snap_lsn,
            crash: None,
            crashed: false,
        };
        for (lsn, op) in ops {
            if lsn <= snap_lsn {
                continue; // already inside the snapshot
            }
            if lsn != inner.last_lsn + 1 {
                return Err(DbError::Corrupt("gap in log sequence numbers"));
            }
            inner.apply(op, &cfg, &metrics)?;
            inner.last_lsn = lsn;
            bump(&metrics.wal_replayed, 1);
        }
        Ok(Db {
            cfg,
            inner: RwLock::new(inner),
            metrics,
        })
    }
    fn new(cfg: Config) -> Db {
        Db::open(Disk::default(), cfg).expect("an empty disk always opens")
    }
    fn read<T>(&self, f: impl FnOnce(&Inner) -> Result<T>) -> Result<T> {
        let g = self.inner.read().expect("lock poisoned");
        g.alive()?;
        f(&g)
    }
    fn write<T>(&self, f: impl FnOnce(&mut Inner) -> Result<T>) -> Result<T> {
        let mut g = self.inner.write().expect("lock poisoned");
        g.alive()?;
        f(&mut g)
    }
    fn create_collection(&self, name: &str, schema: Schema) -> Result<()> {
        schema.validate()?;
        if !is_ident(name) {
            return schema_err("collection names are identifiers");
        }
        self.write(|i| {
            if i.collections.contains_key(name) {
                return Err(DbError::CollectionExists(name.into()));
            }
            i.log_and_apply(WalOp::Create(name.into(), schema), &self.cfg, &self.metrics)
        })
    }
    fn store(
        &self,
        name: &str,
        id: u64,
        vector: &[f32],
        meta: Meta,
        must_be_new: bool,
    ) -> Result<()> {
        self.write(|i| {
            let c = i.coll(name)?;
            if must_be_new && c.loc.contains_key(&id) {
                return Err(DbError::DuplicateId(id));
            }
            c.schema.check_meta(&meta)?;
            let rec = Record {
                vector: c.schema.metric.prepare(vector, c.schema.dim)?,
                meta,
            };
            i.log_and_apply(WalOp::Put(name.into(), id, rec), &self.cfg, &self.metrics)?;
            bump(&self.metrics.writes, 1);
            Ok(())
        })?;
        self.maybe_compact(name)
    }
    fn insert(&self, name: &str, id: u64, vector: &[f32], meta: Meta) -> Result<()> {
        self.store(name, id, vector, meta, true)
    }
    fn upsert(&self, name: &str, id: u64, vector: &[f32], meta: Meta) -> Result<()> {
        self.store(name, id, vector, meta, false)
    }
    /// Returns whether the id existed. Deleting a missing id writes nothing.
    fn delete(&self, name: &str, id: u64) -> Result<bool> {
        let existed = self.write(|i| {
            if !i.coll(name)?.loc.contains_key(&id) {
                return Ok(false);
            }
            i.log_and_apply(WalOp::Delete(name.into(), id), &self.cfg, &self.metrics)?;
            bump(&self.metrics.deletes, 1);
            Ok(true)
        })?;
        self.maybe_compact(name)?;
        Ok(existed)
    }
    fn get(&self, name: &str, id: u64) -> Result<Option<Record>> {
        self.read(|i| Ok(i.coll(name)?.get(id)))
    }
    fn search(&self, name: &str, query: &Query) -> Result<Vec<Hit>> {
        self.read(|i| i.coll(name)?.query(query, &self.cfg, &self.metrics))
    }
    fn count(&self, name: &str, filter: Option<&Filter>) -> Result<usize> {
        self.read(|i| i.coll(name)?.count(filter))
    }
    fn flush(&self, name: &str) -> Result<bool> {
        self.write(|i| {
            let did = i.coll_mut(name)?.flush(&self.cfg);
            bump(&self.metrics.flushes, did as u64);
            Ok(did)
        })
    }
    fn plan_compaction(&self, name: &str, force: bool) -> Result<Option<CompactionPlan>> {
        self.read(|i| {
            let c = i.coll(name)?;
            let inputs = c.pick_compaction(&self.cfg, force);
            Ok((!inputs.is_empty()).then(|| CompactionPlan {
                collection: name.into(),
                schema: c.schema.clone(),
                inputs,
            }))
        })
    }
    fn commit_compaction(&self, job: CompactionJob) -> Result<bool> {
        self.write(|i| {
            let name = job.collection.clone();
            let ok = i.coll_mut(&name)?.commit(job);
            bump(
                if ok {
                    &self.metrics.compactions
                } else {
                    &self.metrics.stale_compactions
                },
                1,
            );
            Ok(ok)
        })
    }
    /// plan -> build -> commit. A real server would run this on a background thread; the steps are the same.
    fn compact(&self, name: &str, force: bool) -> Result<bool> {
        match self.plan_compaction(name, force)? {
            Some(plan) => self.commit_compaction(build_compaction(plan, &self.cfg)),
            None => Ok(false),
        }
    }
    fn maybe_compact(&self, name: &str) -> Result<()> {
        if self.cfg.auto_compact {
            self.compact(name, false)?;
        }
        Ok(())
    }
    fn snapshot_bytes(&self) -> Result<Vec<u8>> {
        self.read(|i| Ok(encode_snapshot(i.last_lsn, &i.collections)))
    }
    fn checkpoint(&self) -> Result<()> {
        self.write(|i| {
            i.disk.snapshot = Some(encode_snapshot(i.last_lsn, &i.collections)); // atomic replace = commit point
            if i.crash == Some(CrashPoint::AfterSnapshotBeforeTruncate) {
                i.crashed = true;
                return Err(DbError::Crashed);
            }
            i.disk.wal.clear();
            bump(&self.metrics.checkpoints, 1);
            Ok(())
        })
    }
    fn restore(snapshot: &[u8], cfg: Config) -> Result<Db> {
        Db::open(
            Disk {
                snapshot: Some(snapshot.to_vec()),
                wal: vec![],
            },
            cfg,
        )
    }
    fn arm_crash(&self, at: CrashPoint) {
        self.inner.write().expect("lock poisoned").crash = Some(at);
    }
    /// What survives a process death: only the disk.
    fn into_disk(self) -> Disk {
        self.inner.into_inner().expect("lock poisoned").disk
    }
}

// ============================================================ line protocol ============================================================
/// Split at whitespace that is outside "quotes" and [brackets]; returns (byte offset, word).
fn words(line: &str) -> Result<Vec<(usize, &str)>> {
    let (mut out, mut start, mut quoted, mut escaped, mut depth) =
        (vec![], None, false, false, 0i32);
    for (i, c) in line.char_indices() {
        if !quoted && depth == 0 && c.is_whitespace() {
            if let Some(s) = start.take() {
                out.push((s, &line[s..i]));
            }
            continue;
        }
        start.get_or_insert(i);
        match c {
            _ if escaped => escaped = false,
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '[' if !quoted => depth += 1,
            ']' if !quoted => depth -= 1,
            _ => {}
        }
    }
    if quoted || depth != 0 {
        return Err(DbError::BadQuery("unbalanced quotes or brackets".into()));
    }
    out.extend(start.map(|s| (s, &line[s..])));
    Ok(out)
}
fn literal(s: &str) -> Result<Value> {
    match lex(s)?.as_slice() {
        [(_, Tok::Lit(v))] => Ok(v.clone()),
        _ => Err(DbError::BadQuery(format!("not a literal: {s}"))),
    }
}
fn parse_vector(s: &str) -> Result<Vec<f32>> {
    let inner = s
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| DbError::BadQuery("expected [x,y,...]".into()))?;
    inner
        .split(',')
        .map(|x| {
            x.trim()
                .parse()
                .map_err(|_| DbError::BadQuery(format!("bad number '{}'", x.trim())))
        })
        .collect()
}
fn number<T: std::str::FromStr>(s: Option<&&str>, what: &str) -> Result<T> {
    s.and_then(|s| s.parse().ok())
        .ok_or_else(|| DbError::BadQuery(format!("expected {what}")))
}
const CLAUSES: [&str; 6] = ["TOP", "TEXT", "WHERE", "THRESHOLD", "EF", "EXACT"];

fn run_command(db: &Db, line: &str) -> Result<String> {
    let spans = words(line)?;
    let w: Vec<&str> = spans.iter().map(|s| s.1).collect();
    let bad = |m: &str| Err(DbError::BadQuery(m.into()));
    let Some(&cmd) = w.first() else {
        return bad("empty command");
    };
    let arg = |i: usize| {
        w.get(i)
            .copied()
            .ok_or_else(|| DbError::BadQuery(format!("{cmd}: missing argument {i}")))
    };
    // commands with a fixed shape reject extra words instead of ignoring them
    let exactly = |n: usize| {
        if w.len() == n {
            Ok(())
        } else {
            Err(DbError::BadQuery(format!(
                "{cmd} takes {} arguments",
                n - 1
            )))
        }
    };
    let ok = || "+OK".to_string();
    Ok(match cmd {
        "CREATE" => {
            let name = arg(1)?;
            let (mut dim, mut metric, mut fields) = (None, Metric::L2, BTreeMap::new());
            for opt in &w[2..] {
                match opt.split_once('=') {
                    Some(("dim", v)) => dim = v.parse().ok(),
                    Some(("metric", v)) => {
                        metric = [Metric::L2, Metric::Cosine, Metric::Dot]
                            .into_iter()
                            .find(|m| m.name() == v)
                            .ok_or_else(|| DbError::BadQuery("unknown metric".into()))?
                    }
                    Some(("fields", v)) => {
                        for f in v.split(',') {
                            let parsed = f.split_once(':').and_then(|(n, t)| {
                                Some((n, FieldType::ALL.into_iter().find(|x| x.name() == t)?))
                            });
                            let Some((n, t)) = parsed else {
                                return bad("fields are name:type,...");
                            };
                            if fields.insert(n.to_string(), t).is_some() {
                                return bad("duplicate field");
                            }
                        }
                    }
                    _ => return bad("unknown option"),
                }
            }
            let Some(dim) = dim else {
                return bad("dim=N is required");
            };
            db.create_collection(
                name,
                Schema {
                    dim,
                    metric,
                    fields,
                },
            )?;
            ok()
        }
        "INSERT" | "UPSERT" => {
            let (name, id, vector) = (arg(1)?, number(w.get(2), "an id")?, parse_vector(arg(3)?)?);
            let mut meta = Meta::new();
            for pair in &w[4..] {
                let Some((k, v)) = pair.split_once('=') else {
                    return bad("metadata is key=literal");
                };
                if meta.insert(k.to_string(), literal(v)?).is_some() {
                    return bad("duplicate metadata key");
                }
            }
            if cmd == "INSERT" {
                db.insert(name, id, &vector, meta)?
            } else {
                db.upsert(name, id, &vector, meta)?
            }
            ok()
        }
        "GET" => {
            exactly(3)?;
            match db.get(arg(1)?, number(w.get(2), "an id")?)? {
                None => "_".into(),
                Some(r) => {
                    let v: Vec<String> = r.vector.iter().map(|x| format!("{x}")).collect();
                    let m: Vec<String> = r.meta.iter().map(|(k, v)| format!(" {k}={v}")).collect();
                    format!("+[{}]{}", v.join(","), m.concat())
                }
            }
        }
        "DELETE" => {
            exactly(3)?;
            format!(":{}", db.delete(arg(1)?, number(w.get(2), "an id")?)? as u8)
        }
        "SEARCH" => {
            let name = arg(1)?;
            let mut b = Query::builder();
            let mut i = 2;
            if let Some(v) = w.get(i).filter(|v| v.starts_with('[')) {
                b = b.vector(&parse_vector(v)?);
                i += 1;
            }
            let mut seen = BTreeSet::new();
            while i < w.len() {
                let clause = w[i];
                if !seen.insert(clause) {
                    return bad("clause given twice");
                }
                i += 1;
                match clause {
                    "TOP" => b = b.top_k(number(w.get(i), "a count after TOP")?),
                    "TEXT" => match literal(arg(i)?)? {
                        Value::Str(s) => b = b.text(&s),
                        _ => return bad("TEXT needs a quoted string"),
                    },
                    "THRESHOLD" => {
                        b = b.max_distance(number(w.get(i), "a number after THRESHOLD")?)
                    }
                    "EF" => b = b.ef(number(w.get(i), "a count after EF")?),
                    "EXACT" => {
                        b = b.exact(true);
                        continue;
                    }
                    "WHERE" => {
                        // the expression runs to the next clause keyword (upper case) or the end of the line
                        let end = (i..w.len())
                            .find(|&j| CLAUSES.contains(&w[j]))
                            .unwrap_or(w.len());
                        if end == i {
                            return bad("empty WHERE");
                        }
                        let stop = spans.get(end).map_or(line.len(), |s| s.0);
                        b = b.filter_str(line[spans[i].0..stop].trim_end());
                        i = end;
                        continue;
                    }
                    other => return Err(DbError::BadQuery(format!("unknown clause '{other}'"))),
                }
                i += 1;
            }
            let hits = db.search(name, &b.build()?)?;
            let rows: Vec<String> = hits
                .iter()
                .map(|h| format!("\n{} {:.4}", h.id, h.score))
                .collect();
            format!("*{}{}", hits.len(), rows.concat())
        }
        "COUNT" => {
            let filter = match w.get(2) {
                None => None,
                Some(&"WHERE") if w.len() > 3 => Some(Parser::parse(&line[spans[3].0..])?),
                _ => return bad("COUNT name [WHERE expr]"),
            };
            format!(":{}", db.count(arg(1)?, filter.as_ref())?)
        }
        "FLUSH" => {
            exactly(2)?;
            format!(":{}", db.flush(arg(1)?)? as u8)
        }
        "COMPACT" => {
            exactly(2)?;
            format!(":{}", db.compact(arg(1)?, true)? as u8)
        }
        "CHECKPOINT" => {
            exactly(1)?;
            db.checkpoint()?;
            ok()
        }
        "STATS" => {
            exactly(1)?;
            let rows = db.metrics.rows();
            format!(
                "*{}{}",
                rows.len(),
                rows.iter()
                    .map(|(k, v)| format!("\n{k} {v}"))
                    .collect::<String>()
            )
        }
        _ => return bad("unknown command"),
    })
}
/// One request line in, one reply out. Errors become replies: the server never dies because of input.
fn execute(db: &Db, line: &str) -> String {
    run_command(db, line).unwrap_or_else(|e| format!("-ERR {e}"))
}

// ============================================================ verification: building blocks ============================================================
fn test_bitmap(rng: &mut Rng) {
    for _ in 0..500 {
        let n = rng.below(200);
        let mk = |rng: &mut Rng| {
            let s: BTreeSet<usize> = (0..n).filter(|_| rng.chance(0.4)).collect();
            let mut b = Bitmap::empty(n);
            s.iter().for_each(|&i| b.set(i));
            (b, s)
        };
        let ((a, sa), (b, sb)) = (mk(rng), mk(rng));
        let check = |x: &Bitmap, s: BTreeSet<usize>| {
            assert!(
                x.iter().eq(s.iter().copied())
                    && x.count() == s.len()
                    && (0..n + 3).all(|i| x.get(i) == s.contains(&i))
            )
        };
        let (mut and, mut or, mut diff, mut not) = (a.clone(), a.clone(), a.clone(), a.clone());
        and.and_assign(&b);
        or.or_assign(&b);
        diff.andnot_assign(&b);
        not.not_assign();
        check(&and, &sa & &sb);
        check(&or, &sa | &sb);
        check(&diff, &sa - &sb);
        check(&not, (0..n).filter(|i| !sa.contains(i)).collect());
        check(&Bitmap::full(n), (0..n).collect());
    }
}

const CATS: [&str; 6] = ["jazz", "rock", "folk", "né\"o", "b\\ack", ""];
const VOCAB: [&str; 14] = [
    "red", "blue", "green", "fast", "slow", "river", "stone", "cloud", "ember", "frost", "delta",
    "orbit", "cedar", "quartz",
];
fn test_schema(dim: usize, metric: Metric) -> Schema {
    Schema::new(
        dim,
        metric,
        &[
            ("cat", FieldType::Keyword),
            ("price", FieldType::Int),
            ("live", FieldType::Bool),
            ("title", FieldType::Text),
        ],
    )
}
/// Each field is sometimes missing, so predicates on absent values get exercised.
fn random_meta(rng: &mut Rng) -> Meta {
    let mut m = Meta::new();
    if rng.chance(0.9) {
        m.insert("cat".into(), Value::Str(CATS[rng.below(CATS.len())].into()));
    }
    if rng.chance(0.9) {
        m.insert("price".into(), Value::Int(rng.below(100) as i64 - 10));
    }
    if rng.chance(0.8) {
        m.insert("live".into(), Value::Bool(rng.chance(0.5)));
    }
    if rng.chance(0.85) {
        let len = 1 + rng.below(7);
        // two-level draw: early words are much more common, as in real text
        let words: Vec<&str> = (0..len)
            .map(|_| {
                let bound = 1 + rng.below(VOCAB.len());
                VOCAB[rng.below(bound)]
            })
            .collect();
        m.insert("title".into(), Value::Str(words.join(" ")));
    }
    m
}
fn random_filter(rng: &mut Rng, depth: usize) -> Filter {
    let price = |rng: &mut Rng| rng.below(110) as i64 - 15;
    let cat = |rng: &mut Rng| Value::Str(CATS[rng.below(CATS.len())].into());
    match rng.below(if depth >= 3 { 5 } else { 8 }) {
        0 => Filter::Cmp("cat".into(), CmpOp::ALL[rng.below(6)], cat(rng)),
        1 => Filter::Cmp(
            "price".into(),
            CmpOp::ALL[rng.below(6)],
            Value::Int(price(rng)),
        ),
        2 => Filter::Cmp(
            "live".into(),
            CmpOp::ALL[rng.below(2)],
            Value::Bool(rng.chance(0.5)),
        ),
        3 => Filter::In("cat".into(), (0..rng.below(4)).map(|_| cat(rng)).collect()),
        4 => Filter::Range("price".into(), price(rng), price(rng)), // may be inverted: matches nothing
        5 => Filter::And(
            (0..2 + rng.below(2))
                .map(|_| random_filter(rng, depth + 1))
                .collect(),
        ),
        6 => Filter::Or(
            (0..2 + rng.below(2))
                .map(|_| random_filter(rng, depth + 1))
                .collect(),
        ),
        _ => Filter::Not(Box::new(random_filter(rng, depth + 1))),
    }
}

fn test_filter_language(rng: &mut Rng) {
    let schema = test_schema(2, Metric::L2);
    for _ in 0..3000 {
        let f = random_filter(rng, 0);
        f.type_check(&schema).unwrap();
        let text = f.to_string();
        assert_eq!(
            Parser::parse(&text).as_ref(),
            Ok(&f),
            "round trip of {text}"
        );
    }
    // precedence: or < and < not
    let eq = |n: i64| Filter::Cmp("price".into(), CmpOp::Eq, Value::Int(n));
    assert_eq!(
        Parser::parse("price = 1 or not price = 2 and price = 3"),
        Ok(Filter::Or(vec![
            eq(1),
            Filter::And(vec![Filter::Not(Box::new(eq(2))), eq(3)])
        ]))
    );
    for text in [
        "",
        "price",
        "price =",
        "price == 1",
        "(price = 1",
        "price = 1)",
        "price in",
        "price in [1,",
        "price in 1..\"a\"",
        "and = 1",
        "price = 99999999999999999999",
        "cat = \"open",
        "price ~ 1",
        "price = 1 price = 2",
        "é = 1",
    ] {
        assert!(
            matches!(Parser::parse(text), Err(DbError::FilterSyntax { .. })),
            "should be a syntax error: {text:?}"
        );
    }
    for text in [
        "nope = 1",
        "price = \"a\"",
        "live > true",
        "title = \"x\"",
        "cat in 1..2",
        "cat in [1]",
    ] {
        assert!(
            matches!(
                Parser::parse(text).unwrap().type_check(&schema),
                Err(DbError::Schema(_))
            ),
            "should be a type error: {text}"
        );
    }
    for deep in ["(".repeat(100_000), "not ".repeat(100_000)] {
        assert!(
            Parser::parse(&deep).is_err(),
            "deep nesting must fail cleanly, not overflow the stack"
        );
    }
    let long = vec!["price = 1"; 20_000].join(" and ");
    assert!(
        Parser::parse(&long)
            .unwrap()
            .matches(&Meta::from([("price".into(), Value::Int(1))]))
    );
    // random input must never panic
    let alphabet: Vec<char> = "abcinot ()[],.=!<>\"\\-0123456789é".chars().collect();
    for _ in 0..5000 {
        let s: String = (0..rng.below(30))
            .map(|_| alphabet[rng.below(alphabet.len())])
            .collect();
        let _ = Parser::parse(&s);
    }
}

/// The logical content of a database, independent of how it is laid out in segments.
type Logical = BTreeMap<String, BTreeMap<u64, Record>>;
fn logical(db: &Db) -> Logical {
    let g = db.inner.read().unwrap();
    g.collections
        .iter()
        .map(|(n, c)| {
            (
                n.clone(),
                c.loc
                    .keys()
                    .map(|&id| (id, c.get(id).expect("loc is accurate")))
                    .collect(),
            )
        })
        .collect()
}

fn test_wal_cuts(rng: &mut Rng) {
    let cfg = Config {
        flush_at: 6,
        flat_threshold: 4,
        max_segments: 2,
        ..Config::default()
    };
    let db = Db::new(cfg.clone());
    // (wal length after the acknowledgement, state at that moment)
    let mut acked: Vec<(usize, Logical)> = vec![(0, Logical::new())];
    let mark = |db: &Db, acked: &mut Vec<(usize, Logical)>| {
        let len = db.inner.read().unwrap().disk.wal.len();
        acked.push((len, logical(db)));
    };
    db.create_collection("c", test_schema(3, Metric::L2))
        .unwrap();
    mark(&db, &mut acked);
    for _ in 0..40 {
        let id = rng.below(12) as u64;
        if rng.chance(0.25) {
            db.delete("c", id).unwrap();
        } else {
            db.upsert(
                "c",
                id,
                &[rng.gauss(), rng.gauss(), rng.gauss()],
                random_meta(rng),
            )
            .unwrap();
        }
        mark(&db, &mut acked);
    }
    let wal = db.into_disk().wal;
    for cut in 0..=wal.len() {
        let back = Db::open(
            Disk {
                snapshot: None,
                wal: wal[..cut].to_vec(),
            },
            cfg.clone(),
        )
        .unwrap_or_else(|e| panic!("cut at {cut}: {e}"));
        let want = &acked.iter().rev().find(|a| a.0 <= cut).unwrap().1;
        assert_eq!(
            &logical(&back),
            want,
            "cut at byte {cut} must give exactly the acknowledged prefix"
        );
        let kept = back.into_disk().wal.len();
        assert!(
            kept <= cut && acked.iter().any(|a| a.0 == kept),
            "the torn tail must be removed"
        );
    }
    // a damaged payload byte in the FIRST frame, with history after it, is not a torn tail
    let mut bad = wal.clone();
    bad[12] ^= 1;
    assert_eq!(
        Db::open(
            Disk {
                snapshot: None,
                wal: bad
            },
            cfg
        )
        .err(),
        Some(DbError::Corrupt(
            "checksum mismatch in the middle of the wal"
        ))
    );
    println!(
        "wal: recovery checked at all {} cut points ({} acknowledged states); mid-log damage reported",
        wal.len() + 1,
        acked.len()
    );
}

// ============================================================ verification: model test ============================================================
type Oracle = BTreeMap<u64, Record>;
fn oracle_vector(
    o: &Oracle,
    metric: Metric,
    q: &[f32],
    filter: Option<&Filter>,
    k: usize,
) -> Vec<Hit> {
    let q = metric.prepare(q, q.len()).unwrap();
    let mut all: Vec<(f32, u64)> = o
        .iter()
        .filter(|(_, r)| filter.is_none_or(|f| f.matches(&r.meta)))
        .map(|(id, r)| (metric.distance(&q, &r.vector), *id))
        .collect();
    all.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    all.into_iter()
        .take(k)
        .map(|(d, id)| Hit {
            id,
            score: d as f64,
        })
        .collect()
}
/// Naive BM25 written from the definition: every statistic recomputed from the raw documents.
fn oracle_bm25(o: &Oracle, text: &str, filter: Option<&Filter>) -> Vec<Hit> {
    let docs: BTreeMap<u64, Vec<String>> = o
        .iter()
        .map(|(id, r)| {
            (
                *id,
                match r.meta.get("title") {
                    Some(Value::Str(s)) => tokenize(s),
                    _ => vec![],
                },
            )
        })
        .collect();
    let avg = docs.values().map(|d| d.len()).sum::<usize>() as f64 / docs.len().max(1) as f64;
    let mut terms = tokenize(text);
    terms.sort();
    terms.dedup();
    let mut out = vec![];
    for (id, d) in &docs {
        let mut score = 0.0;
        for t in &terms {
            let tf = d.iter().filter(|x| *x == t).count() as f64;
            let df = docs.values().filter(|other| other.contains(t)).count() as f64;
            if tf > 0.0 {
                score += (1.0 + (docs.len() as f64 - df + 0.5) / (df + 0.5)).ln() * tf * 2.2
                    / (tf + 1.2 * (0.25 + 0.75 * d.len() as f64 / avg));
            }
        }
        if score > 0.0 && filter.is_none_or(|f| f.matches(&o[id].meta)) {
            out.push(Hit { id: *id, score });
        }
    }
    out.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.id.cmp(&b.id)));
    out
}
fn check_full_state(db: &Db, oracle: &Oracle, ctx: &str) {
    assert_eq!(
        &logical(db)["m"],
        oracle,
        "{ctx}: database and oracle differ"
    );
    assert_eq!(db.count("m", None), Ok(oracle.len()), "{ctx}");
    // structural invariant: the live rows of all segments plus the memtable are exactly the ids in `loc`
    let g = db.inner.read().unwrap();
    let c = &g.collections["m"];
    assert_eq!(
        c.segments.iter().map(SegHandle::live).sum::<usize>() + c.mem.len(),
        c.loc.len(),
        "{ctx}: a row is live twice or lost"
    );
}
#[derive(Default)]
struct ModelStats {
    ops: usize,
    exact: usize,
    approx: usize,
    approx_recall: f64,
    text: usize,
    restarts: usize,
    torn: usize,
    half_checkpoints: usize,
    raced: usize,
    stale: usize,
}
fn model_test(metric: Metric, ops: usize, seed: u64, st: &mut ModelStats) {
    // tiny thresholds so that graphs, every plan, flushes and merges all happen on a few hundred rows
    let cfg = Config {
        flush_at: 40,
        flat_threshold: 12,
        exact_scan_limit: 8,
        max_segments: 3,
        m: 8,
        ef_construction: 48,
        ef_search: 48,
        ..Config::default()
    };
    let dim = 6;
    let mut rng = Rng::new(seed);
    let mut db = Db::new(cfg.clone());
    db.create_collection("m", test_schema(dim, metric)).unwrap();
    let mut oracle = Oracle::new();
    let vector = |rng: &mut Rng| -> Vec<f32> { (0..dim).map(|_| rng.gauss()).collect() };
    for step in 0..ops {
        st.ops += 1;
        let ctx = format!("{} seed {seed} step {step}", metric.name());
        let id = rng.below(500) as u64;
        let filter = rng.chance(0.5).then(|| random_filter(&mut rng, 1));
        match rng.below(100) {
            0..=29 => {
                let (v, meta) = (vector(&mut rng), random_meta(&mut rng));
                let r = db.insert("m", id, &v, meta.clone());
                if oracle.contains_key(&id) {
                    assert_eq!(r, Err(DbError::DuplicateId(id)), "{ctx}");
                } else {
                    r.unwrap();
                    oracle.insert(
                        id,
                        Record {
                            vector: metric.prepare(&v, dim).unwrap(),
                            meta,
                        },
                    );
                }
            }
            30..=44 => {
                let (v, meta) = (vector(&mut rng), random_meta(&mut rng));
                db.upsert("m", id, &v, meta.clone()).unwrap();
                oracle.insert(
                    id,
                    Record {
                        vector: metric.prepare(&v, dim).unwrap(),
                        meta,
                    },
                );
            }
            45..=54 => assert_eq!(
                db.delete("m", id),
                Ok(oracle.remove(&id).is_some()),
                "{ctx}"
            ),
            55..=57 => assert_eq!(db.get("m", id).unwrap().as_ref(), oracle.get(&id), "{ctx}"),
            58..=60 => {
                let want = oracle
                    .values()
                    .filter(|r| filter.as_ref().is_none_or(|f| f.matches(&r.meta)))
                    .count();
                assert_eq!(
                    db.count("m", filter.as_ref()),
                    Ok(want),
                    "{ctx}: count {filter:?}"
                );
            }
            61..=84 => {
                let (q, k) = (vector(&mut rng), 1 + rng.below(8));
                let exact = rng.chance(0.4);
                let mut b = Query::builder().vector(&q).top_k(k).exact(exact);
                if let Some(f) = &filter {
                    b = b.filter(f.clone());
                }
                let want = oracle_vector(&oracle, metric, &q, filter.as_ref(), k);
                // sometimes cut at the distance of a middle result
                let threshold = (rng.chance(0.2) && !want.is_empty())
                    .then(|| want[want.len() / 2].score as f32);
                if let Some(t) = threshold {
                    b = b.max_distance(t);
                }
                let got = db.search("m", &b.build().unwrap()).unwrap();
                let want: Vec<Hit> = want
                    .into_iter()
                    .filter(|h| threshold.is_none_or(|t| h.score as f32 <= t))
                    .collect();
                if exact {
                    st.exact += 1;
                    assert_eq!(got, want, "{ctx}: exact search, filter {filter:?}");
                } else {
                    // approximate: every hit is live, matching and exactly scored; the list is sorted, unique and never short
                    let all = oracle_vector(&oracle, metric, &q, filter.as_ref(), usize::MAX);
                    let truth: BTreeMap<u64, f64> = all.iter().map(|h| (h.id, h.score)).collect();
                    assert!(
                        got.iter().all(|h| truth.get(&h.id) == Some(&h.score)),
                        "{ctx}: dead, non-matching or mis-scored hit"
                    );
                    assert!(
                        got.windows(2)
                            .all(|w| (w[0].score, w[0].id) < (w[1].score, w[1].id)),
                        "{ctx}: unsorted or duplicated"
                    );
                    assert!(
                        got.len() <= k && (threshold.is_some() || got.len() == k.min(all.len())),
                        "{ctx}: short answer"
                    );
                    if !want.is_empty() {
                        st.approx += 1;
                        st.approx_recall += want
                            .iter()
                            .filter(|w| got.iter().any(|g| g.id == w.id))
                            .count() as f64
                            / want.len() as f64;
                    }
                }
            }
            85..=89 => {
                st.text += 1;
                let text = format!(
                    "{} {}",
                    VOCAB[rng.below(VOCAB.len())],
                    VOCAB[rng.below(VOCAB.len())]
                );
                let all = oracle_bm25(&oracle, &text, filter.as_ref());
                let mut b = Query::builder().text(&text).top_k(7);
                if let Some(f) = &filter {
                    b = b.filter(f.clone());
                }
                let got = db.search("m", &b.build().unwrap()).unwrap();
                assert_eq!(got.len(), all.len().min(7), "{ctx}: bm25 length");
                for (g, w) in got.iter().zip(&all) {
                    assert!(
                        g.id == w.id && (g.score - w.score).abs() < 1e-9,
                        "{ctx}: bm25 {g:?} vs {w:?}"
                    );
                }
                // hybrid with an exact dense leg equals rank fusion of the two oracle rankings
                let q = vector(&mut rng);
                let mut b = Query::builder().text(&text).vector(&q).top_k(7).exact(true);
                if let Some(f) = &filter {
                    b = b.filter(f.clone());
                }
                let dense = oracle_vector(&oracle, metric, &q, filter.as_ref(), 50)
                    .iter()
                    .map(|h| h.id)
                    .collect();
                let lexical = all.iter().take(50).map(|h| h.id).collect();
                assert_eq!(
                    db.search("m", &b.build().unwrap()).unwrap(),
                    rrf(&[dense, lexical], 7),
                    "{ctx}: hybrid"
                );
            }
            90..=91 => {
                db.flush("m").unwrap();
            }
            92..=93 => {
                // compaction raced by writes: plan, then change rows while "the background thread builds"
                if let Some(plan) = db.plan_compaction("m", true).unwrap() {
                    let second = db.plan_compaction("m", true).unwrap().unwrap();
                    let victims: Vec<u64> = plan
                        .inputs
                        .iter()
                        .flat_map(|h| {
                            (0..h.seg.len())
                                .filter(|&r| !h.dead.get(r))
                                .map(|r| h.seg.ids[r])
                        })
                        .step_by(7)
                        .collect();
                    let job = build_compaction(plan, &cfg);
                    for (i, v) in victims.into_iter().enumerate() {
                        if i % 2 == 0 {
                            db.delete("m", v).unwrap();
                            oracle.remove(&v);
                        } else {
                            let (x, meta) = (vector(&mut rng), random_meta(&mut rng));
                            db.upsert("m", v, &x, meta.clone()).unwrap();
                            oracle.insert(
                                v,
                                Record {
                                    vector: metric.prepare(&x, dim).unwrap(),
                                    meta,
                                },
                            );
                        }
                    }
                    // the writes above can trigger automatic compaction, which makes this job stale; both outcomes must be safe
                    if db.commit_compaction(job).unwrap() {
                        st.raced += 1;
                        // the second job read the same inputs, which no longer exist: it must change nothing
                        assert!(
                            !db.commit_compaction(build_compaction(second, &cfg))
                                .unwrap(),
                            "{ctx}: stale job accepted"
                        );
                        st.stale += 1;
                    }
                    check_full_state(&db, &oracle, &ctx);
                }
            }
            94..=95 => db.checkpoint().unwrap(),
            96..=97 => {
                db = Db::open(db.into_disk(), cfg.clone()).unwrap();
                st.restarts += 1;
                check_full_state(&db, &oracle, &ctx);
            }
            98 => {
                db.arm_crash(CrashPoint::TornAppend(rng.below(60)));
                let v = vector(&mut rng);
                assert_eq!(
                    db.upsert("m", id, &v, Meta::new()),
                    Err(DbError::Crashed),
                    "{ctx}"
                );
                assert_eq!(
                    db.get("m", id),
                    Err(DbError::Crashed),
                    "a crashed handle answers nothing"
                );
                db = Db::open(db.into_disk(), cfg.clone()).unwrap();
                st.torn += 1;
                check_full_state(&db, &oracle, &ctx); // the unacknowledged write is absent
            }
            _ => {
                db.arm_crash(CrashPoint::AfterSnapshotBeforeTruncate);
                assert_eq!(db.checkpoint(), Err(DbError::Crashed), "{ctx}");
                db = Db::open(db.into_disk(), cfg.clone()).unwrap();
                st.half_checkpoints += 1;
                check_full_state(&db, &oracle, &ctx);
            }
        }
    }
    check_full_state(&db, &oracle, "end of model run");
}

// ============================================================ verification: larger collection ============================================================
const FILTERS: [&str; 5] = [
    "",
    "price < 800",
    "price in 100..399",
    "cat = \"jazz\" and live = true",
    "price < 6 or (cat = \"folk\" and price in 500..520)",
];
/// Mean recall@10 per filter against brute force; every hit is checked against the oracle.
fn measure(db: &Db, oracle: &Oracle, queries: &[Vec<f32>], label: &str) -> Vec<f64> {
    println!(
        "  {label}\n    {:<52} {:>8} {:>9} {:>13}  plans flat/pre/post/in (fallbacks)",
        "filter", "matches", "recall@10", "dist calls/q"
    );
    let mut out = vec![];
    for text in FILTERS {
        let filter = (!text.is_empty()).then(|| Parser::parse(text).unwrap());
        let before: BTreeMap<_, _> = db.metrics.rows().into_iter().collect();
        let mut recall = 0.0;
        for q in queries {
            let mut b = Query::builder().vector(q).top_k(10);
            if let Some(f) = &filter {
                b = b.filter(f.clone());
            }
            let got = db.search("big", &b.build().unwrap()).unwrap();
            let want = oracle_vector(oracle, Metric::L2, q, filter.as_ref(), 10);
            assert_eq!(got.len(), want.len(), "never short");
            assert!(
                got.iter().all(|h| oracle
                    .get(&h.id)
                    .is_some_and(|r| filter.as_ref().is_none_or(|f| f.matches(&r.meta)))),
                "dead or non-matching hit for '{text}'"
            );
            recall += want
                .iter()
                .filter(|w| got.iter().any(|g| g.id == w.id))
                .count() as f64
                / want.len().max(1) as f64;
        }
        recall /= queries.len() as f64;
        let after: BTreeMap<_, _> = db.metrics.rows().into_iter().collect();
        let d = |k: &str| after[k] - before[k];
        let matches = oracle
            .values()
            .filter(|r| filter.as_ref().is_none_or(|f| f.matches(&r.meta)))
            .count();
        println!(
            "    {:<52} {matches:>8} {recall:>9.3} {:>13}  {}/{}/{}/{} ({})",
            if text.is_empty() { "(none)" } else { text },
            d("distance_calls") / queries.len() as u64,
            d("plan_flat"),
            d("plan_pre"),
            d("plan_post"),
            d("plan_in"),
            d("plan_fallback")
        );
        out.push(recall);
    }
    out
}
fn large_collection(rng: &mut Rng) {
    let (n, dim) = (6000u64, 24usize);
    let cfg = Config {
        flush_at: 1500,
        max_segments: 6,
        ..Config::default()
    };
    let db = Db::new(cfg.clone());
    db.create_collection("big", test_schema(dim, Metric::L2))
        .unwrap();
    let centres: Vec<f32> = (0..25 * dim).map(|_| rng.gauss() * 1.5).collect();
    let point = |rng: &mut Rng| -> Vec<f32> {
        let c = rng.below(25);
        (0..dim)
            .map(|d| centres[c * dim + d] + rng.gauss())
            .collect()
    };
    let meta = |rng: &mut Rng| -> Meta {
        let mut m = random_meta(rng);
        m.insert("price".into(), Value::Int(rng.below(1000) as i64));
        m
    };
    let mut oracle = Oracle::new();
    for id in 0..n + 100 {
        let r = Record {
            vector: point(rng),
            meta: meta(rng),
        };
        db.insert("big", id, &r.vector, r.meta.clone()).unwrap();
        oracle.insert(id, r);
    }
    db.flush("big").unwrap(); // the last 100 rows become a small segment without a graph
    let queries: Vec<Vec<f32>> = (0..60).map(|_| point(rng)).collect();
    println!(
        "\n=== larger collection: {} rows, {dim} dims, l2 ===",
        oracle.len()
    );
    let first = measure(&db, &oracle, &queries, "after load");
    for name in ["plan_flat", "plan_pre", "plan_post", "plan_in"] {
        assert!(
            db.metrics.rows().iter().any(|r| r.0 == name && r.1 > 0),
            "{name} was never used"
        );
    }
    for id in 0..n {
        if id % 4 == 0 {
            db.delete("big", id).unwrap();
            oracle.remove(&id);
        } else if id % 10 == 1 {
            let r = Record {
                vector: point(rng),
                meta: meta(rng),
            };
            db.upsert("big", id, &r.vector, r.meta.clone()).unwrap();
            oracle.insert(id, r);
        }
    }
    let churned = measure(
        &db,
        &oracle,
        &queries,
        "after deleting 25% and updating about 5% (automatic compaction on)",
    );
    db.flush("big").unwrap(); // compaction merges segments only; the memtable has to become one first
    assert!(db.compact("big", true).unwrap());
    let compacted = measure(&db, &oracle, &queries, "after full compaction");
    for recalls in [&first, &churned, &compacted] {
        assert!(
            recalls.iter().all(|&r| r >= 0.98),
            "recall@10 below 0.98: {recalls:?}"
        );
    }
    {
        let g = db.inner.read().unwrap();
        let c = &g.collections["big"];
        assert_eq!(
            (c.segments.len(), c.segments[0].dead.count(), c.mem.len()),
            (1, 0, 0),
            "full compaction leaves one clean segment"
        );
    }
    // snapshot/restore: bytes and answers are identical
    let snap = db.snapshot_bytes().unwrap();
    let back = Db::restore(&snap, cfg.clone()).unwrap();
    assert!(
        back.snapshot_bytes().unwrap() == snap,
        "snapshot -> restore -> snapshot must be bit-exact"
    );
    for q in &queries {
        let query = Query::builder()
            .vector(q)
            .filter_str("price < 500")
            .build()
            .unwrap();
        assert_eq!(db.search("big", &query), back.search("big", &query));
    }
    for _ in 0..100 {
        let mut bad = snap.clone();
        let at = rng.below(bad.len());
        bad[at] ^= 1 << rng.below(8);
        assert!(
            Db::restore(&bad, cfg.clone()).is_err(),
            "bit flip at {at} accepted"
        );
    }
    assert!(
        [0, 3, 11, snap.len() / 2, snap.len() - 1]
            .iter()
            .all(|&l| Db::restore(&snap[..l], cfg.clone()).is_err())
    );
    let mut newer = snap.clone();
    newer[4] = 9;
    let body = newer.len() - 4;
    let crc = crc32(&newer[..body]);
    newer[body..].copy_from_slice(&crc.to_le_bytes());
    assert_eq!(Db::restore(&newer, cfg).err(), Some(DbError::Version(9)));
    println!(
        "  snapshot: {} KiB, bit-exact round trip, 100 bit flips and 5 truncations rejected, future version reported",
        snap.len() / 1024
    );
}

/// A checksum only catches accidents. The decoder itself must survive bytes that pass it: patch, re-checksum, load, query.
fn test_hostile_snapshots(rng: &mut Rng) {
    let cfg = Config {
        flush_at: 20,
        flat_threshold: 8,
        exact_scan_limit: 2,
        ..Config::default()
    };
    let db = Db::new(cfg.clone());
    db.create_collection("m", test_schema(3, Metric::L2))
        .unwrap();
    for id in 0..50 {
        db.insert(
            "m",
            id,
            &[rng.gauss(), rng.gauss(), rng.gauss()],
            random_meta(rng),
        )
        .unwrap();
    }
    let snap = db.snapshot_bytes().unwrap();
    let (trials, mut loaded) = (3000, 0);
    for _ in 0..trials {
        let mut bad = snap[..snap.len() - 4].to_vec();
        for _ in 0..1 + rng.below(4) {
            let at = 4 + rng.below(bad.len() - 4);
            bad[at] = if rng.chance(0.5) {
                0xff
            } else {
                rng.next_u64() as u8
            };
        }
        let crc = crc32(&bad);
        bad.extend_from_slice(&crc.to_le_bytes());
        if let Ok(h) = Db::restore(&bad, cfg.clone()) {
            loaded += 1;
            let _ = h.search(
                "m",
                &Query::builder()
                    .vector(&[0.0, 0.0, 0.0])
                    .filter_str("price >= 0")
                    .build()
                    .unwrap(),
            );
            let _ = h.search("m", &Query::builder().text("red blue").build().unwrap());
        }
    }
    println!(
        "hostile snapshots with valid checksums: {trials} loads and queries without a panic ({loaded} were accepted as well formed)"
    );
}

// ============================================================ verification: concurrency ============================================================
fn stress_pass(pass: u64) {
    let cfg = Config {
        flush_at: 60,
        flat_threshold: 16,
        exact_scan_limit: 8,
        max_segments: 3,
        ..Config::default()
    };
    let db = Db::new(cfg.clone());
    db.create_collection("s", test_schema(8, Metric::L2))
        .unwrap();
    let mut rng = Rng::new(1000 + pass);
    let stable: Vec<Vec<f32>> = (0..150)
        .map(|_| (0..8).map(|_| rng.gauss()).collect())
        .collect();
    let with_price = |p: i64| Meta::from([("price".to_string(), Value::Int(p))]);
    for (i, v) in stable.iter().enumerate() {
        db.insert("s", i as u64, v, with_price(1)).unwrap();
    }
    for id in 500..560u64 {
        db.insert("s", id, &stable[(id - 500) as usize], with_price(1))
            .unwrap();
        db.delete("s", id).unwrap(); // gone before any reader starts: must never reappear
    }
    // TOKEN stores version v as the vector [v; 8] and price = BASE + v. The offset keeps its price away from the values the
    // readers filter on: without it, version 1 legitimately matches `price = 1` for a moment.
    const TOKEN: u64 = 999;
    const BASE: i64 = 100;
    db.upsert("s", TOKEN, &[0.0; 8], with_price(BASE)).unwrap();
    std::thread::scope(|scope| {
        for t in 0..4u64 {
            let (db, stable) = (&db, &stable);
            scope.spawn(move || {
                let mut rng = Rng::new(7 + t + pass * 11);
                let mut last_version = 0;
                for _ in 0..150 {
                    // one writer replaces TOKEN: a read must see one whole version, and versions must never go backwards
                    let r = db
                        .get("s", TOKEN)
                        .unwrap()
                        .expect("an upsert never leaves a gap");
                    let Value::Int(price) = r.meta["price"] else {
                        panic!("bad meta")
                    };
                    let version = price - BASE;
                    assert!(r.vector.iter().all(|&x| x == version as f32), "torn read");
                    assert!(version >= last_version, "time went backwards");
                    last_version = version;
                    let pick = rng.below(stable.len());
                    // exact mode: acknowledged, untouched rows are always findable
                    let q = Query::builder()
                        .vector(&stable[pick])
                        .top_k(1)
                        .exact(true)
                        .filter_str("price = 1")
                        .build()
                        .unwrap();
                    let got = db.search("s", &q).unwrap();
                    assert_eq!((got[0].id, got[0].score), (pick as u64, 0.0));
                    // approximate mode: only safety properties, because which state we see depends on scheduling
                    let q = Query::builder()
                        .vector(&stable[pick])
                        .top_k(8)
                        .filter_str("price = 1")
                        .build()
                        .unwrap();
                    let got = db.search("s", &q).unwrap();
                    assert_eq!(got.len(), 8);
                    // only the stable rows have price 1: this excludes deleted rows, churn rows (price 2) and TOKEN
                    assert!(
                        got.iter().all(|h| h.id < stable.len() as u64),
                        "deleted or filtered row returned: {got:?}"
                    );
                    assert!(
                        got.windows(2)
                            .all(|w| (w[0].score, w[0].id) < (w[1].score, w[1].id))
                    );
                }
            });
        }
        // the single writer
        for round in 1..=300i64 {
            let id = 2000 + rng.below(100) as u64;
            if rng.chance(0.3) {
                db.delete("s", id).unwrap();
            } else {
                db.upsert(
                    "s",
                    id,
                    &(0..8).map(|_| rng.gauss()).collect::<Vec<_>>(),
                    with_price(2),
                )
                .unwrap();
            }
            db.upsert("s", TOKEN, &[round as f32; 8], with_price(BASE + round))
                .unwrap();
            if round % 90 == 0 {
                if let Some(plan) = db.plan_compaction("s", true).unwrap() {
                    let job = build_compaction(plan, &cfg); // no lock held: readers keep going
                    db.commit_compaction(job).unwrap();
                }
            }
        }
    });
    assert!(db.metrics.compactions.load(Relaxed) >= 1);
}

// ============================================================ verification: protocol ============================================================
fn scripted_session() {
    let db = Db::new(Config::default());
    // (request, expected reply or reply prefix)
    let script: &[(&str, &str)] = &[
        (
            "CREATE songs dim=3 metric=l2 fields=genre:keyword,year:int,live:bool,title:text",
            "+OK",
        ),
        (
            "CREATE songs dim=3",
            "-ERR collection 'songs' already exists",
        ),
        (
            "INSERT songs 1 [1,0,0] genre=\"jazz\" year=1959 title=\"blue river\"",
            "+OK",
        ),
        (
            "INSERT songs 2 [0.9,0.1,0] genre=\"jazz\" year=1965 live=true title=\"green river\"",
            "+OK",
        ),
        (
            "INSERT songs 3 [0,1,0] genre=\"rock\" year=1971 title=\"red stone\"",
            "+OK",
        ),
        (
            "INSERT songs 4 [0,0,1] genre=\"folk\" year=1999 live=false title=\"blue cloud\"",
            "+OK",
        ),
        ("INSERT songs 4 [0,0,1]", "-ERR duplicate id 4"),
        (
            "INSERT songs 5 [1,2]",
            "-ERR schema violation: vector has 2 dimensions",
        ),
        (
            "SEARCH songs [1,0,0] TOP 3",
            "*3\n1 0.0000\n2 0.0200\n3 2.0000",
        ),
        (
            "SEARCH songs [1,0,0] TOP 3 WHERE genre = \"jazz\" and year in 1960..1969",
            "*1\n2 0.0200",
        ),
        (
            "SEARCH songs [1,0,0] WHERE not live = true THRESHOLD 1.5",
            "*1\n1 0.0000",
        ),
        ("SEARCH songs TEXT \"blue river\" TOP 3", "*3\n1 "),
        (
            "SEARCH songs [0,0,1] TEXT \"river\" TOP 2 WHERE year < 1970",
            "*2\n",
        ),
        (
            "SEARCH songs [1,0,0] WHERE colour = 1",
            "-ERR schema violation: unknown field 'colour'",
        ),
        (
            "SEARCH songs [1,0,0] WHERE year >",
            "-ERR filter syntax error",
        ),
        ("UPSERT songs 3 [1,0.05,0] genre=\"jazz\" year=1961", "+OK"),
        ("COUNT songs WHERE genre in [\"jazz\", \"folk\"]", ":4"),
        ("FLUSH songs", ":1"),
        ("DELETE songs 1", ":1"),
        ("DELETE songs 1", ":0"),
        ("GET songs 1", "_"),
        ("GET songs 3", "+[1,0.05,0] genre=\"jazz\" year=1961"),
        ("COMPACT songs", ":1"),
        ("CHECKPOINT", "+OK"),
        ("SEARCH songs [1,0,0] TOP 2 EXACT", "*2\n3 0.0025\n2 0.0200"),
        ("DROP songs", "-ERR bad query: unknown command"),
    ];
    println!("\n=== scripted session ===");
    for (request, expected) in script {
        let reply = execute(&db, request);
        println!("> {request}\n{reply}");
        assert!(
            reply.starts_with(expected),
            "{request:?}: expected {expected:?}, got {reply:?}"
        );
    }
    let before = logical(&db);
    let db = Db::open(db.into_disk(), Config::default()).unwrap();
    assert_eq!(logical(&db), before);
    println!(
        "> (restart)\n> COUNT songs\n{}",
        execute(&db, "COUNT songs")
    );
    assert_eq!(execute(&db, "COUNT songs"), ":3");
    assert!(execute(&db, "STATS").contains("\nqueries 0"));

    // mangled commands: delete, duplicate or swap characters of valid requests. Nothing may panic, and every reply is well formed.
    let mut rng = Rng::new(0x877_F);
    for _ in 0..3000 {
        let mut chars: Vec<char> = script[rng.below(script.len())].0.chars().collect();
        for _ in 0..1 + rng.below(4) {
            let at = rng.below(chars.len());
            match rng.below(4) {
                0 => {
                    chars.remove(at);
                }
                1 => chars.insert(
                    at,
                    ['"', '[', ']', ' ', '=', '9', 'é', '\\', '-'][rng.below(9)],
                ),
                2 => chars.truncate(at + 1),
                _ => {
                    let other = rng.below(chars.len());
                    chars.swap(at, other);
                }
            }
            if chars.is_empty() {
                chars.push(' ');
            }
        }
        let reply = execute(&db, &chars.into_iter().collect::<String>());
        assert!(
            reply.starts_with(['+', '-', ':', '*', '_']),
            "malformed reply {reply:?}"
        );
    }
    for huge in [
        "CREATE x dim=99999999999",
        "CREATE y dim=0",
        "SEARCH songs [1,0,0] TOP 99999999999999999999",
        "SEARCH songs [NaN,0,0]",
        "INSERT songs 9 [inf,0,0]",
        "SEARCH songs [1,0,0] TOP 0",
    ] {
        assert!(execute(&db, huge).starts_with("-ERR"), "{huge}");
    }
}

fn main() {
    let mut rng = Rng::new(877);
    test_bitmap(&mut rng);
    test_filter_language(&mut rng);
    println!(
        "bitmaps match BTreeSet; 3000 filters round-trip through printer and parser; malformed and over-deep input rejected"
    );
    test_wal_cuts(&mut rng);

    let mut st = ModelStats::default();
    for (i, metric) in [Metric::L2, Metric::Cosine, Metric::Dot]
        .into_iter()
        .enumerate()
    {
        model_test(metric, 1000, 0x877 + i as u64, &mut st);
    }
    let recall = st.approx_recall / st.approx.max(1) as f64;
    println!(
        "model test: {} ops; {} exact and {} approximate vector queries (mean recall {recall:.3}), {} text + hybrid checks; {} restarts, {} torn appends, {} half checkpoints, {} raced compactions ({} stale jobs rejected)",
        st.ops,
        st.exact,
        st.approx,
        st.text,
        st.restarts,
        st.torn,
        st.half_checkpoints,
        st.raced,
        st.stale
    );
    assert_eq!(st.ops, 3000);
    assert!(recall > 0.95, "approximate recall {recall}");
    assert!(
        st.restarts > 20 && st.torn > 10 && st.half_checkpoints > 10 && st.raced > 10,
        "the random mix must reach every crash and race path"
    );

    large_collection(&mut rng);
    test_hostile_snapshots(&mut rng);
    for pass in 0..5 {
        stress_pass(pass);
    }
    println!(
        "\nconcurrency: 5 passes of 4 readers x 150 rounds against 1 writer with compaction: no torn, stale, deleted or filtered rows seen"
    );
    scripted_session();
    println!("\nall mini vector database checks passed");
}
