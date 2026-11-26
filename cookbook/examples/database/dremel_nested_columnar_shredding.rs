//! RECIPE 740 (database): Dremel/Parquet nested record shredding: repetition and definition levels, the assembly FSM, level encoding.
//! LEVEL: advanced
//! Use in: columnar file formats (Parquet, ORC-like layouts), nested/JSON analytics, BigQuery-style engines.   Run: cargo run --release --example dremel_nested_columnar_shredding
//! COMMON USES: Apache Parquet (every nested column is stored exactly this way), Google Dremel/BigQuery (Capacitor descends from it),
//! Apache Arrow's list/struct to Parquet conversion, Spark/Trino/DuckDB Parquet readers, Impala, Drill.
//! Course: CMU 15-721 Advanced Database Systems (Spring), lecture on storage models and columnar/nested data formats (Dremel, Parquet).
#![allow(dead_code)]
//!
//! WHAT: a record with nested, optional and repeated fields is flattened into ONE COLUMN PER LEAF PATH without losing structure.
//! Each stored value carries two small integers:
//! * REPETITION level r: at which REPEATED ancestor of the leaf this value repeats. r = 0 means "first value of a new record";
//!   r = k means "a new element of the k-th repeated field on the path (counting from the root)".
//! * DEFINITION level d: how many of the optional/repeated fields on the path are actually present. d = max means the value
//!   itself exists; d < max means a NULL, and d tells exactly WHICH ancestor was missing (so `Links{}` and absent `Links` differ).
//! Required fields add nothing to either level, which is why a flat required column has no levels at all.
//!
//! WHY: a scan of `Name.Url` must not touch `Name.Language.Code`. Row-wise nested storage (JSON, protobuf) reads everything;
//! shredded storage reads one narrow column and yet can still rebuild whole records or answer per-record questions (a COUNT of
//! a repeated field is just "count the entries, split at r = 0") from the levels alone.
//!
//! HOW (shredding): walk the record against the schema recursively. For a child field with no values, emit one NULL entry
//! (r of the parent walk, d of the parent) into every leaf below it. For the j-th value of a child, r is the incoming r if j = 0
//! and otherwise the child's own repetition level; d is the child's definition level. Both maxima come from the schema alone.
//!
//! HOW (assembly): a finite-state machine of leaf readers. After reading a value from leaf g, peek the NEXT repetition level r'
//! of g's column: r' = 0 continues with the next leaf in schema order (or ends the record); r' > 0 means the repeated ancestor A
//! of level r' gets another element, so if the next leaf still lies inside A we finish the current element first, otherwise we
//! jump back to A's first leaf. A forward move reuses the ancestors shared with the previous leaf; a backward move appends a new
//! element of A at level r and reuses everything above it. The d level truncates descent (NULLs create no nodes beyond d).
//!
//! PARQUET LEVEL ENCODING: levels are small ints (bit width = ceil(log2(max+1))), mostly runs, so Parquet uses the RLE /
//! BIT-PACKED HYBRID: a varint header whose low bit selects the run kind. Even header = RLE run (count << 1) followed by the
//! value in ceil(width/8) bytes; odd header = bit-packed run ((groups of 8) << 1 | 1) followed by groups*width bytes, LSB-first.
//! A max level of 0 needs no bytes at all. Implemented below with a round-trip test on random level sequences.
//!
//! CHECKS: the paper's two-record Document example must give the published columns exactly (values, r, d); 3000 random records
//! over the paper schema plus 1000 over a deeper schema round-trip shred -> assemble exactly; levels survive the hybrid encoding;
//! projection, COUNT and a per-record predicate computed from columns alone match an oracle over the records.
//!
//! Read: Dremel: Interactive Analysis of Web-Scale Datasets https://research.google/pubs/dremel-interactive-analysis-of-web-scale-datasets/
//!       Parquet file format (nested encoding, RLE/bit-packing) https://parquet.apache.org/docs/file-format/
//!       parquet-format README https://github.com/apache/parquet-format/blob/master/README.md

// ---------------------------------------------------------------- schema

#[derive(Clone, Copy, PartialEq, Debug)]
enum Rep {
    Required,
    Optional,
    Repeated,
}

/// Declarative schema; a node without children is a leaf (string valued, to keep the paper's examples readable).
struct Spec {
    name: &'static str,
    rep: Rep,
    kids: Vec<Spec>,
}

fn leaf(name: &'static str, rep: Rep) -> Spec {
    Spec {
        name,
        rep,
        kids: vec![],
    }
}
fn group(name: &'static str, rep: Rep, kids: Vec<Spec>) -> Spec {
    Spec { name, rep, kids }
}

/// A resolved schema node with precomputed levels. Nodes live in a flat vec (index 0 is the root).
struct Info {
    name: &'static str,
    path: String,
    parent: Option<usize>,
    rep: Rep,
    rlevel: u8, // max repetition level at this node = repeated nodes on the path
    dlevel: u8, // max definition level at this node = optional/repeated nodes on the path
    kids: Vec<usize>,
    leaf: Option<usize>,
    lo: usize, // leaf-index range [lo, hi) of the subtree
    hi: usize,
}

struct Schema {
    nodes: Vec<Info>,
    leaves: Vec<usize>, // node index of each leaf, in depth-first (= schema) order
}

impl Schema {
    fn build(spec: &Spec) -> Schema {
        let mut s = Schema {
            nodes: vec![],
            leaves: vec![],
        };
        s.add(spec, None, 0, 0, "");
        s
    }
    fn add(&mut self, sp: &Spec, parent: Option<usize>, rl: u8, dl: u8, prefix: &str) -> usize {
        let rlevel = rl + (sp.rep == Rep::Repeated) as u8;
        let dlevel = dl + (sp.rep != Rep::Required) as u8;
        let idx = self.nodes.len();
        let path = if prefix.is_empty() {
            sp.name.to_string()
        } else {
            format!("{prefix}.{}", sp.name)
        };
        let lo = self.leaves.len();
        self.nodes.push(Info {
            name: sp.name,
            path: path.clone(),
            parent,
            rep: sp.rep,
            rlevel,
            dlevel,
            kids: vec![],
            leaf: None,
            lo,
            hi: lo,
        });
        if sp.kids.is_empty() {
            self.nodes[idx].leaf = Some(lo);
            self.leaves.push(idx);
        }
        let child_prefix = if parent.is_none() {
            String::new()
        } else {
            path
        };
        for k in &sp.kids {
            let c = self.add(k, Some(idx), rlevel, dlevel, &child_prefix);
            self.nodes[idx].kids.push(c);
        }
        self.nodes[idx].hi = self.leaves.len();
        idx
    }
    /// Schema nodes from the root's child down to the leaf (the root is excluded).
    fn ancestors(&self, leaf: usize) -> Vec<usize> {
        let mut v = vec![];
        let mut n = Some(self.leaves[leaf]);
        while let Some(i) = n {
            if self.nodes[i].parent.is_some() {
                v.push(i);
            }
            n = self.nodes[i].parent;
        }
        v.reverse();
        v
    }
    fn child_index(&self, node: usize) -> usize {
        let p = self.nodes[node].parent.unwrap();
        self.nodes[p].kids.iter().position(|&k| k == node).unwrap()
    }
    fn max_r(&self, leaf: usize) -> u8 {
        self.nodes[self.leaves[leaf]].rlevel
    }
    fn max_d(&self, leaf: usize) -> u8 {
        self.nodes[self.leaves[leaf]].dlevel
    }
}

fn paper_schema() -> Spec {
    use Rep::*;
    group(
        "Document",
        Required,
        vec![
            leaf("DocId", Required),
            group(
                "Links",
                Optional,
                vec![leaf("Backward", Repeated), leaf("Forward", Repeated)],
            ),
            group(
                "Name",
                Repeated,
                vec![
                    group(
                        "Language",
                        Repeated,
                        vec![leaf("Code", Required), leaf("Country", Optional)],
                    ),
                    leaf("Url", Optional),
                ],
            ),
        ],
    )
}

/// A deeper, differently shaped schema to show the algorithm does not depend on the paper's example.
fn deep_schema() -> Spec {
    use Rep::*;
    group(
        "R",
        Required,
        vec![
            group(
                "a",
                Repeated,
                vec![
                    leaf("b", Repeated),
                    group(
                        "c",
                        Optional,
                        vec![group(
                            "d",
                            Repeated,
                            vec![leaf("e", Optional), leaf("f", Required)],
                        )],
                    ),
                ],
            ),
            leaf("g", Optional),
        ],
    )
}

// ---------------------------------------------------------------- records

/// Per schema child: the list of values (0..1 for optional, exactly 1 for required, any number for repeated).
type Fields = Vec<Vec<Val>>;

#[derive(Clone, PartialEq, Debug)]
enum Val {
    Leaf(String),
    Group(Fields),
}

fn l(s: &str) -> Val {
    Val::Leaf(s.to_string())
}

// ---------------------------------------------------------------- columns and shredding

#[derive(Default, Clone, PartialEq, Debug)]
struct Column {
    r: Vec<u8>,
    d: Vec<u8>,
    v: Vec<String>, // only entries with d == max definition level carry a value
}

impl Column {
    fn len(&self) -> usize {
        self.r.len()
    }
}

fn push_null_leaves(s: &Schema, node: usize, r: u8, d: u8, cols: &mut [Column]) {
    for g in s.nodes[node].lo..s.nodes[node].hi {
        cols[g].r.push(r);
        cols[g].d.push(d);
    }
}

fn shred_walk(s: &Schema, node: usize, f: &Fields, r: u8, d: u8, cols: &mut [Column]) {
    for (ci, &child) in s.nodes[node].kids.iter().enumerate() {
        let info = &s.nodes[child];
        if f[ci].is_empty() {
            // Missing optional/repeated parent: ONE null per leaf below, carrying the depth reached so far (d of the parent).
            push_null_leaves(s, child, r, d, cols);
            continue;
        }
        for (j, v) in f[ci].iter().enumerate() {
            // Only the first value inherits the caller's r; later siblings repeat at this field's own level.
            let rj = if j == 0 { r } else { info.rlevel };
            match v {
                Val::Leaf(x) => {
                    let c = &mut cols[info.leaf.unwrap()];
                    c.r.push(rj);
                    c.d.push(info.dlevel);
                    c.v.push(x.clone());
                }
                Val::Group(g) => shred_walk(s, child, g, rj, info.dlevel, cols),
            }
        }
    }
}

fn shred(s: &Schema, records: &[Fields]) -> Vec<Column> {
    let mut cols = vec![Column::default(); s.leaves.len()];
    for rec in records {
        shred_walk(s, 0, rec, 0, 0, &mut cols);
    }
    cols
}

// ---------------------------------------------------------------- assembly FSM

/// next[g][r'] = leaf to read after leaf g when g's next repetition level is r'; None = record complete.
fn build_fsm(s: &Schema) -> Vec<Vec<Option<usize>>> {
    let n = s.leaves.len();
    (0..n)
        .map(|g| {
            (0..=s.max_r(g))
                .map(|rp| {
                    let after = if g + 1 < n { Some(g + 1) } else { None };
                    if rp == 0 {
                        return after;
                    }
                    // The repeated ancestor whose level is r' gets a new element once its current one is complete.
                    let a = s
                        .ancestors(g)
                        .into_iter()
                        .find(|&a| s.nodes[a].rep == Rep::Repeated && s.nodes[a].rlevel == rp)
                        .unwrap();
                    match after {
                        Some(nx) if nx >= s.nodes[a].lo && nx < s.nodes[a].hi => Some(nx),
                        _ => Some(s.nodes[a].lo),
                    }
                })
                .collect()
        })
        .collect()
}

/// Insert one column entry into the record under construction.
/// `new_from` = index in the ancestor path from which nodes are freshly created; earlier ones reuse their last element.
fn insert(s: &Schema, root: &mut Fields, g: usize, d: u8, value: Option<&str>, new_from: usize) {
    let anc = s.ancestors(g);
    let mut cur = root;
    for (i, &a) in anc.iter().enumerate() {
        let info = &s.nodes[a];
        if d < info.dlevel {
            return; // this node is absent: the NULL stops the descent here
        }
        let ci = s.child_index(a);
        if i == anc.len() - 1 {
            cur[ci].push(Val::Leaf(
                value.expect("value present when d is max").to_string(),
            ));
            return;
        }
        let list = &mut cur[ci];
        if i >= new_from || list.is_empty() {
            list.push(Val::Group(vec![vec![]; info.kids.len()]));
        }
        match list.last_mut().unwrap() {
            Val::Group(f) => cur = f,
            Val::Leaf(_) => unreachable!(),
        }
    }
}

fn assemble(s: &Schema, cols: &[Column]) -> Vec<Fields> {
    let fsm = build_fsm(s);
    let mut pos = vec![0usize; cols.len()];
    let mut vpos = vec![0usize; cols.len()];
    let mut out = vec![];
    while pos[0] < cols[0].len() {
        let mut rec: Fields = vec![vec![]; s.nodes[0].kids.len()];
        let mut prev: Option<usize> = None;
        let mut g = 0usize;
        loop {
            let c = &cols[g];
            let (r, d) = (c.r[pos[g]], c.d[pos[g]]);
            let value = if d == s.max_d(g) {
                vpos[g] += 1;
                Some(c.v[vpos[g] - 1].as_str())
            } else {
                None
            };
            let anc = s.ancestors(g);
            let new_from = match prev {
                Some(p) if g <= p => anc
                    .iter()
                    .position(|&a| s.nodes[a].rep == Rep::Repeated && s.nodes[a].rlevel == r)
                    .expect("r > 0 on a backward move"),
                Some(p) => {
                    let pa = s.ancestors(p);
                    anc.iter()
                        .zip(pa.iter())
                        .take_while(|(x, y)| x == y)
                        .count()
                }
                None => 0,
            };
            insert(s, &mut rec, g, d, value, new_from);
            pos[g] += 1;
            let r_next = cols[g].r.get(pos[g]).copied().unwrap_or(0);
            prev = Some(g);
            match fsm[g][r_next as usize] {
                Some(nx) => g = nx,
                None => break,
            }
        }
        out.push(rec);
    }
    out
}

// ---------------------------------------------------------------- Parquet RLE / bit-packed hybrid

fn bit_width(max: u8) -> usize {
    (8 - max.leading_zeros()) as usize
}

fn put_varint(out: &mut Vec<u8>, mut x: usize) {
    while x >= 0x80 {
        out.push((x & 0x7f) as u8 | 0x80);
        x >>= 7;
    }
    out.push(x as u8);
}

fn get_varint(b: &[u8], p: &mut usize) -> usize {
    let (mut x, mut shift) = (0usize, 0);
    loop {
        let byte = b[*p];
        *p += 1;
        x |= ((byte & 0x7f) as usize) << shift;
        if byte & 0x80 == 0 {
            return x;
        }
        shift += 7;
    }
}

fn flush_packed(out: &mut Vec<u8>, pending: &mut Vec<u8>, bw: usize) {
    if pending.is_empty() {
        return;
    }
    while pending.len() % 8 != 0 {
        pending.push(0); // only the final group is ever padded, the decoder truncates to the known count
    }
    put_varint(out, ((pending.len() / 8) << 1) | 1);
    let mut bits = vec![0u8; pending.len() * bw / 8];
    for (i, &v) in pending.iter().enumerate() {
        for b in 0..bw {
            if v >> b & 1 == 1 {
                let bit = i * bw + b;
                bits[bit / 8] |= 1 << (bit % 8);
            }
        }
    }
    out.extend(bits);
    pending.clear();
}

/// Runs of >= 8 equal values become RLE runs; everything else is bit-packed in groups of 8.
fn encode_levels(levels: &[u8], max: u8) -> Vec<u8> {
    let bw = bit_width(max);
    if bw == 0 {
        return vec![]; // max level 0: nothing to store
    }
    let (mut out, mut pending) = (vec![], vec![]);
    let mut i = 0;
    while i < levels.len() {
        let run = levels[i..].iter().take_while(|&&x| x == levels[i]).count();
        if run >= 8 && pending.len() % 8 == 0 {
            flush_packed(&mut out, &mut pending, bw);
            put_varint(&mut out, run << 1);
            out.extend(&(levels[i] as u32).to_le_bytes()[..bw.div_ceil(8)]);
            i += run;
        } else {
            pending.push(levels[i]); // keep groups aligned: absorb values one at a time until a run can start cleanly
            i += 1;
        }
    }
    flush_packed(&mut out, &mut pending, bw);
    out
}

fn decode_levels(b: &[u8], max: u8, n: usize) -> Vec<u8> {
    let bw = bit_width(max);
    if bw == 0 {
        return vec![0; n];
    }
    let (mut out, mut p) = (Vec::with_capacity(n), 0);
    while out.len() < n {
        let h = get_varint(b, &mut p);
        if h & 1 == 0 {
            let mut v = 0u32;
            for k in 0..bw.div_ceil(8) {
                v |= (b[p + k] as u32) << (8 * k);
            }
            p += bw.div_ceil(8);
            out.extend(std::iter::repeat_n(v as u8, h >> 1));
        } else {
            let groups = h >> 1;
            for i in 0..groups * 8 {
                let mut v = 0u8;
                for k in 0..bw {
                    let bit = i * bw + k;
                    v |= (b[p + bit / 8] >> (bit % 8) & 1) << k;
                }
                out.push(v);
            }
            p += groups * bw;
        }
    }
    out.truncate(n);
    out
}

/// Stored size of a column: encoded levels plus PLAIN byte-array values (4-byte length prefix each).
fn column_bytes(s: &Schema, g: usize, c: &Column) -> (usize, usize) {
    let lv = encode_levels(&c.r, s.max_r(g)).len() + encode_levels(&c.d, s.max_d(g)).len();
    let vals: usize = c.v.iter().map(|x| 4 + x.len()).sum();
    (lv, vals)
}

// ---------------------------------------------------------------- random data

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn random_fields(s: &Schema, node: usize, rng: &mut Rng) -> Fields {
    s.nodes[node]
        .kids
        .iter()
        .map(|&k| {
            let info = &s.nodes[k];
            let n = match info.rep {
                Rep::Required => 1,
                Rep::Optional => rng.below(2) as usize,
                Rep::Repeated => [0, 0, 1, 1, 2, 3][rng.below(6) as usize],
            };
            (0..n)
                .map(|_| {
                    if info.kids.is_empty() {
                        Val::Leaf(format!("v{}", rng.below(40)))
                    } else {
                        Val::Group(random_fields(s, k, rng))
                    }
                })
                .collect()
        })
        .collect()
}

// ---------------------------------------------------------------- the paper's example

fn paper_records() -> Vec<Fields> {
    // Document layout: [DocId, Links, Name]
    let lang = |code: &str, country: Option<&str>| {
        Val::Group(vec![vec![l(code)], country.map(l).into_iter().collect()])
    };
    let name = |langs: Vec<Val>, url: Option<&str>| {
        Val::Group(vec![langs, url.map(l).into_iter().collect()])
    };
    let r1 = vec![
        vec![l("10")],
        vec![Val::Group(vec![vec![], vec![l("20"), l("40"), l("60")]])],
        vec![
            name(
                vec![lang("en-us", Some("us")), lang("en", None)],
                Some("http://A"),
            ),
            name(vec![], Some("http://B")),
            name(vec![lang("en-gb", Some("gb"))], None),
        ],
    ];
    let r2 = vec![
        vec![l("20")],
        vec![Val::Group(vec![vec![l("10"), l("30")], vec![l("80")]])],
        vec![name(vec![], Some("http://C"))],
    ];
    vec![r1, r2]
}

fn check_paper(s: &Schema, cols: &[Column]) {
    let expected: Vec<(&str, Vec<(Option<&str>, u8, u8)>)> = vec![
        ("DocId", vec![(Some("10"), 0, 0), (Some("20"), 0, 0)]),
        (
            "Links.Backward",
            vec![(None, 0, 1), (Some("10"), 0, 2), (Some("30"), 1, 2)],
        ),
        (
            "Links.Forward",
            vec![
                (Some("20"), 0, 2),
                (Some("40"), 1, 2),
                (Some("60"), 1, 2),
                (Some("80"), 0, 2),
            ],
        ),
        (
            "Name.Language.Code",
            vec![
                (Some("en-us"), 0, 2),
                (Some("en"), 2, 2),
                (None, 1, 1),
                (Some("en-gb"), 1, 2),
                (None, 0, 1),
            ],
        ),
        (
            "Name.Language.Country",
            vec![
                (Some("us"), 0, 3),
                (None, 2, 2),
                (None, 1, 1),
                (Some("gb"), 1, 3),
                (None, 0, 1),
            ],
        ),
        (
            "Name.Url",
            vec![
                (Some("http://A"), 0, 2),
                (Some("http://B"), 1, 2),
                (None, 1, 1),
                (Some("http://C"), 0, 2),
            ],
        ),
    ];
    assert_eq!(expected.len(), cols.len());
    for (g, (path, exp)) in expected.iter().enumerate() {
        assert_eq!(&s.nodes[s.leaves[g]].path, path);
        let c = &cols[g];
        let mut vi = 0;
        let got: Vec<(Option<String>, u8, u8)> = (0..c.len())
            .map(|i| {
                let v = if c.d[i] == s.max_d(g) {
                    vi += 1;
                    Some(c.v[vi - 1].clone())
                } else {
                    None
                };
                (v, c.r[i], c.d[i])
            })
            .collect();
        let want: Vec<(Option<String>, u8, u8)> = exp
            .iter()
            .map(|(v, r, d)| (v.map(String::from), *r, *d))
            .collect();
        assert_eq!(got, want, "column {path} differs from the paper");
    }
}

fn show_column(s: &Schema, g: usize, c: &Column) {
    let mut vi = 0;
    let cells: Vec<String> = (0..c.len())
        .map(|i| {
            let v = if c.d[i] == s.max_d(g) {
                vi += 1;
                c.v[vi - 1].as_str()
            } else {
                "NULL"
            };
            format!("{v}({},{})", c.r[i], c.d[i])
        })
        .collect();
    println!(
        "  {:<22} maxR={} maxD={}  {}",
        s.nodes[s.leaves[g]].path,
        s.max_r(g),
        s.max_d(g),
        cells.join(" ")
    );
}

// ---------------------------------------------------------------- queries on columns

fn walk_names(rec: &Fields, url_ci: usize, name_ci: usize) -> (usize, Vec<String>) {
    let mut n = 0;
    let mut urls = vec![];
    for nm in &rec[name_ci] {
        n += 1;
        if let Val::Group(f) = nm {
            for u in &f[url_ci] {
                if let Val::Leaf(x) = u {
                    urls.push(x.clone());
                }
            }
        }
    }
    (n, urls)
}

fn main() {
    let s = Schema::build(&paper_schema());
    println!("== schema levels (max repetition / definition per leaf)");
    for g in 0..s.leaves.len() {
        println!(
            "  {:<22} maxR={} maxD={}",
            s.nodes[s.leaves[g]].path,
            s.max_r(g),
            s.max_d(g)
        );
    }

    println!("== paper example: shredded columns, value(r,d)");
    let recs = paper_records();
    let cols = shred(&s, &recs);
    for (g, c) in cols.iter().enumerate() {
        show_column(&s, g, c);
    }
    check_paper(&s, &cols);
    assert_eq!(assemble(&s, &cols), recs, "paper records must reassemble");
    println!(
        "  columns equal the published Figure 3 contents; assembly returns the original two records"
    );

    println!("== assembly FSM (leaf -> next leaf by next repetition level)");
    let fsm = build_fsm(&s);
    for (g, row) in fsm.iter().enumerate() {
        let t: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(r, n)| {
                format!(
                    "r'={r}->{}",
                    n.map_or("END".into(), |x| s.nodes[s.leaves[x]].path.clone())
                )
            })
            .collect();
        println!("  {:<22} {}", s.nodes[s.leaves[g]].path, t.join("  "));
    }

    println!("== round trip on random nested records");
    let mut rng = Rng(0x9E3779B97F4A7C15);
    let big: Vec<Fields> = (0..3000).map(|_| random_fields(&s, 0, &mut rng)).collect();
    let big_cols = shred(&s, &big);
    assert_eq!(assemble(&s, &big_cols), big, "3000 paper-schema records");
    let ds = Schema::build(&deep_schema());
    let deep: Vec<Fields> = (0..1000).map(|_| random_fields(&ds, 0, &mut rng)).collect();
    let deep_cols = shred(&ds, &deep);
    assert_eq!(assemble(&ds, &deep_cols), deep, "1000 deep-schema records");
    println!("  3000 Document records and 1000 deep-schema records: shred -> assemble is exact");

    println!("== Parquet RLE/bit-packed hybrid level encoding");
    let (mut raw_total, mut enc_total) = (0usize, 0usize);
    for (g, c) in big_cols.iter().enumerate() {
        for (levels, max) in [(&c.r, s.max_r(g)), (&c.d, s.max_d(g))] {
            let enc = encode_levels(levels, max);
            assert_eq!(
                decode_levels(&enc, max, levels.len()),
                *levels,
                "level round trip"
            );
            raw_total += levels.len();
            enc_total += enc.len();
        }
    }
    for _ in 0..500 {
        let max = 1 + rng.below(7) as u8;
        let n = rng.below(200) as usize;
        let mut v = vec![];
        while v.len() < n {
            let (x, run) = (rng.below(max as u64 + 1) as u8, 1 + rng.below(20) as usize);
            v.extend(std::iter::repeat_n(x, run));
        }
        v.truncate(n);
        assert_eq!(
            decode_levels(&encode_levels(&v, max), max, n),
            v,
            "random run-structured levels, max {max}"
        );
    }
    println!(
        "  levels: {raw_total} one-byte entries -> {enc_total} bytes ({:.1}x smaller); 500 random sequences round-trip",
        raw_total as f64 / enc_total as f64
    );

    println!("== column projection and COUNT without assembling records");
    let url_g = s
        .leaves
        .iter()
        .position(|&n| s.nodes[n].path == "Name.Url")
        .unwrap();
    let fwd_g = s
        .leaves
        .iter()
        .position(|&n| s.nodes[n].path == "Links.Forward")
        .unwrap();
    let name_node = s.nodes[0].kids[2];
    let name_d = s.nodes[name_node].dlevel;
    // SELECT Name.Url: non-null values of one column.
    let urls: Vec<String> = big_cols[url_g].v.clone();
    let oracle_urls: Vec<String> = big.iter().flat_map(|r| walk_names(r, 1, 2).1).collect();
    assert_eq!(urls, oracle_urls);
    // SELECT COUNT(Name): a Name exists wherever the Url column has d >= dlevel(Name); no other column is read.
    let count = big_cols[url_g].d.iter().filter(|&&d| d >= name_d).count();
    let oracle_count: usize = big.iter().map(|r| walk_names(r, 1, 2).0).sum();
    assert_eq!(count, oracle_count);
    // Per-record predicate from levels alone: records with more than 2 Links.Forward values (split the column at r = 0).
    let fc = &big_cols[fwd_g];
    let mut per_rec = vec![];
    for i in 0..fc.len() {
        if fc.r[i] == 0 {
            per_rec.push(0usize);
        }
        if fc.d[i] == s.max_d(fwd_g) {
            *per_rec.last_mut().unwrap() += 1;
        }
    }
    let heavy = per_rec.iter().filter(|&&n| n > 2).count();
    let oracle_heavy = big
        .iter()
        .filter(|r| matches!(&r[1].first(), Some(Val::Group(f)) if f[1].len() > 2))
        .count();
    assert_eq!(per_rec.len(), big.len());
    assert_eq!(heavy, oracle_heavy);
    println!(
        "  SELECT Name.Url -> {} values; COUNT(Name) = {count}; records with >2 Forward links = {heavy}",
        urls.len()
    );

    let sizes: Vec<(usize, usize)> = big_cols
        .iter()
        .enumerate()
        .map(|(g, c)| column_bytes(&s, g, c))
        .collect();
    let total: usize = sizes.iter().map(|(a, b)| a + b).sum();
    println!("  {:<22} {:>8} {:>8}", "column", "levels B", "values B");
    for (g, (a, b)) in sizes.iter().enumerate() {
        println!("  {:<22} {:>8} {:>8}", s.nodes[s.leaves[g]].path, a, b);
    }
    let q1 = sizes[url_g].0 + sizes[url_g].1;
    let q2 = sizes[fwd_g].0 + sizes[fwd_g].1;
    println!(
        "  bytes read: Name.Url only {q1} ({:.1}% of {total}), Links.Forward only {q2} ({:.1}%), whole records {total}",
        100.0 * q1 as f64 / total as f64,
        100.0 * q2 as f64 / total as f64
    );
    assert!(q1 < total && q2 < total);

    println!("dremel_nested_columnar_shredding: all checks passed");
}
