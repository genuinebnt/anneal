//! RECIPE 702 (database): storage models: row store (NSM), column store (DSM) and PAX, with an IO and cache-line cost meter.
//! LEVEL: advanced
//! Use in: choosing a physical layout for OLTP vs OLAP tables; understanding why column stores scan fast.   Run: cargo run --release --example storage_models_nsm_dsm_pax
//! COMMON USES: NSM is the layout of PostgreSQL, MySQL/InnoDB and SQLite tables (OLTP); DSM is the layout of column stores
//! such as ClickHouse, DuckDB's storage, Vertica and Parquet/ORC files; PAX (minipages per column inside each page) is what
//! Parquet row groups and several hybrid engines (and the original PAX paper's research systems) do to keep one page per
//! row group while getting columnar cache behaviour.
//! Course: CMU 15-445 Intro to Database Systems, Lecture 06 (Storage Models and Compression: NSM, DSM, PAX).
#![allow(dead_code)]
//!
//! WHAT: one table (`id u64, qty u32, price i64, cat` short low-cardinality string, `name` variable string), three physical
//! layouts, all built from real 4 KiB byte pages and accessed only through a `Meter` that records every byte range read:
//! distinct PAGES touched (the IO cost), distinct 64-byte CACHE LINES touched (the memory-bandwidth/CPU cost) and bytes read.
//! * NSM (row store): a slotted page of whole rows. Reading one full tuple touches one page and about one cache line; scanning
//!   one column still drags every row's other columns through the cache.
//! * DSM (column store): one file of pages per column, fixed-width values packed densely, strings in slotted column pages.
//!   Row i is value i of every column (RECORD ID ALIGNMENT), so rebuilding a tuple touches one page per column (5 here) and
//!   a column aggregate touches only that column's pages.
//! * PAX: pages hold the same rows as NSM would, but inside the page each column has its own MINIPAGE. IO is as cheap as NSM
//!   (one page per tuple), the cache behaviour of a column scan is as good as DSM, but a scan still reads every page of the
//!   table. Optionally the low-cardinality `cat` minipage is DICTIONARY ENCODED (per-page dictionary + 1-byte codes): more
//!   rows per page and an equality filter can first look the constant up in the dictionary (absent: skip the page's codes) and
//!   then compare one-byte codes.
//! Late vs early MATERIALISATION: filter on one column, collect matching row ids, and only then fetch the other columns for
//! the survivors (late) versus rebuilding every tuple first and filtering afterwards (early).
//!
//! VERIFIED: all four layouts (NSM, DSM, PAX, PAX+dictionary) answer 500 random queries (point reads, column projections of
//! random rows, column sums, filter-then-aggregate, wide scans, string equality) identically to a plain `Vec<Row>` oracle.
//! The meter then asserts the textbook winners: NSM wins full-tuple point reads (1 page vs 5 for DSM), DSM and PAX win
//! single-column scans in cache lines (and DSM wins in pages), PAX does not win in pages, dictionary PAX shrinks the file,
//! and late materialisation touches less than early.
//!
//! Read: https://15445.courses.cs.cmu.edu/fall2024/slides/05-storage3.pdf (CMU 15-445 storage models lecture),
//! https://www.pdl.cmu.edu/PDL-FTP/Database/pax.pdf (Ailamaki et al., "Weaving Relations for Cache Performance", VLDB 2001) and
//! https://db.cs.cmu.edu/papers/2022/cidr2022-p13-crotty.pdf (Crotty et al., "Are You Sure You Want to Use MMAP in Your DBMS", background on page IO)
use std::collections::HashSet;

const PAGE: usize = 4096;
const LINE: usize = 64;
const NCOLS: usize = 5;
const WIDTH: [usize; 3] = [8, 4, 8];
const CATS: [&str; 8] = [
    "books",
    "games",
    "toys",
    "garden",
    "electronics",
    "grocery",
    "pets",
    "office",
];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() >> 11) as usize % n
    }
}

#[derive(Clone, PartialEq, Debug)]
enum Val {
    U(u64),
    I(i64),
    S(String),
}
impl Val {
    fn num(&self) -> i64 {
        match self {
            Val::U(x) => *x as i64,
            Val::I(x) => *x,
            Val::S(_) => panic!("not numeric"),
        }
    }
}

#[derive(Clone)]
struct Row {
    id: u64,
    qty: u32,
    price: i64,
    cat: String,
    name: String,
}
impl Row {
    fn val(&self, c: usize) -> Val {
        match c {
            0 => Val::U(self.id),
            1 => Val::U(self.qty as u64),
            2 => Val::I(self.price),
            3 => Val::S(self.cat.clone()),
            _ => Val::S(self.name.clone()),
        }
    }
    fn fixed(&self, c: usize) -> Vec<u8> {
        match c {
            0 => self.id.to_le_bytes().to_vec(),
            1 => self.qty.to_le_bytes().to_vec(),
            _ => self.price.to_le_bytes().to_vec(),
        }
    }
    /// NSM row image: id, qty, price, cat (u8 len + bytes), name (u16 len + bytes).
    fn encode(&self) -> Vec<u8> {
        let mut v = Vec::new();
        (0..3).for_each(|c| v.extend(self.fixed(c)));
        v.push(self.cat.len() as u8);
        v.extend(self.cat.as_bytes());
        v.extend((self.name.len() as u16).to_le_bytes());
        v.extend(self.name.as_bytes());
        v
    }
}
fn decode_fixed(c: usize, b: &[u8]) -> Val {
    match c {
        0 => Val::U(u64::from_le_bytes(b.try_into().unwrap())),
        1 => Val::U(u32::from_le_bytes(b.try_into().unwrap()) as u64),
        _ => Val::I(i64::from_le_bytes(b.try_into().unwrap())),
    }
}
fn text(b: &[u8]) -> Val {
    Val::S(String::from_utf8(b.to_vec()).unwrap())
}

// ---------- the meter: every read of table bytes goes through here ----------
#[derive(Default, Clone)]
struct Meter {
    pages: HashSet<(usize, usize)>,
    lines: HashSet<(usize, usize, usize)>,
    bytes: usize,
}
impl Meter {
    fn rd<'a>(
        &mut self,
        file: usize,
        page: usize,
        buf: &'a [u8],
        off: usize,
        len: usize,
    ) -> &'a [u8] {
        if len > 0 {
            self.pages.insert((file, page));
            for l in off / LINE..=(off + len - 1) / LINE {
                self.lines.insert((file, page, l));
            }
        }
        self.bytes += len;
        &buf[off..off + len]
    }
    fn u16(&mut self, file: usize, page: usize, buf: &[u8], off: usize) -> usize {
        let b = self.rd(file, page, buf, off, 2);
        u16::from_le_bytes([b[0], b[1]]) as usize
    }
}
#[derive(Default, Clone, Copy, Debug)]
struct Cost {
    pages: usize,
    lines: usize,
    bytes: usize,
}
impl Cost {
    fn add(&mut self, m: &Meter) {
        self.pages += m.pages.len();
        self.lines += m.lines.len();
        self.bytes += m.bytes;
    }
}

enum Pred {
    GtNum(i64),
    EqStr(String),
}
impl Pred {
    fn test(&self, v: &Val) -> bool {
        match (self, v) {
            (Pred::GtNum(t), v) => v.num() > *t,
            (Pred::EqStr(s), Val::S(x)) => s == x,
            _ => false,
        }
    }
}

trait Layout {
    fn name(&self) -> String;
    /// Append a row; returns how many distinct pages were written.
    fn insert(&mut self, r: &Row) -> usize;
    fn finish(&mut self) {}
    fn num_pages(&self) -> usize;
    fn project(&self, rows: &[usize], cols: &[usize], m: &mut Meter) -> Vec<Vec<Val>>;
    fn select(&self, col: usize, p: &Pred, m: &mut Meter) -> Vec<usize>;
    fn sum_col(&self, col: usize, m: &mut Meter) -> i64;
}

/// A slotted page: [n u16][offset u16 * n] ... free ... values packed from the back.
struct NPage {
    buf: Vec<u8>,
    n: usize,
    free_end: usize,
}
impl NPage {
    fn new() -> NPage {
        NPage {
            buf: vec![0; PAGE],
            n: 0,
            free_end: PAGE,
        }
    }
    fn append(&mut self, v: &[u8]) -> bool {
        if 2 + 2 * (self.n + 1) + v.len() > self.free_end {
            return false;
        }
        self.free_end -= v.len();
        self.buf[self.free_end..self.free_end + v.len()].copy_from_slice(v);
        let o = 2 + 2 * self.n;
        self.buf[o..o + 2].copy_from_slice(&(self.free_end as u16).to_le_bytes());
        self.n += 1;
        self.buf[0..2].copy_from_slice(&(self.n as u16).to_le_bytes());
        true
    }
}
fn locate(starts: &[usize], r: usize) -> (usize, usize) {
    let p = starts.partition_point(|&s| s <= r) - 1;
    (p, r - starts[p])
}

// ======================= NSM =======================
struct Nsm {
    pages: Vec<NPage>,
    starts: Vec<usize>,
    total: usize,
}
impl Nsm {
    fn new() -> Nsm {
        Nsm {
            pages: Vec::new(),
            starts: Vec::new(),
            total: 0,
        }
    }
    fn field(m: &mut Meter, p: usize, buf: &[u8], off: usize, c: usize) -> Val {
        match c {
            0..=2 => {
                let o = off + [0, 8, 12][c];
                decode_fixed(c, m.rd(0, p, buf, o, WIDTH[c]))
            }
            3 => {
                let n = m.rd(0, p, buf, off + 20, 1)[0] as usize;
                text(m.rd(0, p, buf, off + 21, n))
            }
            _ => {
                let cl = m.rd(0, p, buf, off + 20, 1)[0] as usize;
                let no = off + 21 + cl;
                let n = m.u16(0, p, buf, no);
                text(m.rd(0, p, buf, no + 2, n))
            }
        }
    }
    fn row_off(m: &mut Meter, p: usize, page: &NPage, s: usize) -> usize {
        m.u16(0, p, &page.buf, 2 + 2 * s)
    }
}
impl Layout for Nsm {
    fn name(&self) -> String {
        "NSM".into()
    }
    fn insert(&mut self, r: &Row) -> usize {
        let bytes = r.encode();
        if self.pages.last_mut().is_none_or(|p| !p.append(&bytes)) {
            self.starts.push(self.total);
            let mut p = NPage::new();
            assert!(p.append(&bytes));
            self.pages.push(p);
        }
        self.total += 1;
        1
    }
    fn num_pages(&self) -> usize {
        self.pages.len()
    }
    fn project(&self, rows: &[usize], cols: &[usize], m: &mut Meter) -> Vec<Vec<Val>> {
        rows.iter()
            .map(|&r| {
                let (p, s) = locate(&self.starts, r);
                let off = Nsm::row_off(m, p, &self.pages[p], s);
                cols.iter()
                    .map(|&c| Nsm::field(m, p, &self.pages[p].buf, off, c))
                    .collect()
            })
            .collect()
    }
    fn select(&self, col: usize, pred: &Pred, m: &mut Meter) -> Vec<usize> {
        let mut out = Vec::new();
        for (p, page) in self.pages.iter().enumerate() {
            for s in 0..page.n {
                let off = Nsm::row_off(m, p, page, s);
                if pred.test(&Nsm::field(m, p, &page.buf, off, col)) {
                    out.push(self.starts[p] + s);
                }
            }
        }
        out
    }
    fn sum_col(&self, col: usize, m: &mut Meter) -> i64 {
        let mut sum = 0i64;
        for (p, page) in self.pages.iter().enumerate() {
            for s in 0..page.n {
                let off = Nsm::row_off(m, p, page, s);
                sum = sum.wrapping_add(Nsm::field(m, p, &page.buf, off, col).num());
            }
        }
        sum
    }
}

// ======================= DSM =======================
enum Col {
    Fixed {
        w: usize,
        pages: Vec<Vec<u8>>,
        n: usize,
    },
    Var {
        pages: Vec<NPage>,
        starts: Vec<usize>,
        total: usize,
    },
}
struct Dsm {
    cols: Vec<Col>,
}
impl Dsm {
    fn new() -> Dsm {
        let mut cols: Vec<Col> = WIDTH
            .iter()
            .map(|&w| Col::Fixed {
                w,
                pages: Vec::new(),
                n: 0,
            })
            .collect();
        cols.push(Col::Var {
            pages: Vec::new(),
            starts: Vec::new(),
            total: 0,
        });
        cols.push(Col::Var {
            pages: Vec::new(),
            starts: Vec::new(),
            total: 0,
        });
        Dsm { cols }
    }
    fn value(&self, c: usize, r: usize, m: &mut Meter) -> Val {
        match &self.cols[c] {
            Col::Fixed { w, pages, .. } => {
                let cap = PAGE / w;
                let (p, i) = (r / cap, r % cap);
                decode_fixed(c, m.rd(c, p, &pages[p], i * w, *w))
            }
            Col::Var { pages, starts, .. } => {
                let (p, s) = locate(starts, r);
                let pg = &pages[p];
                let off = m.u16(c, p, &pg.buf, 2 + 2 * s);
                let end = if s == 0 {
                    PAGE
                } else {
                    m.u16(c, p, &pg.buf, 2 + 2 * (s - 1))
                };
                text(m.rd(c, p, &pg.buf, off, end - off))
            }
        }
    }
    /// Read a whole column page-by-page, calling `f(row, value)`. Reads exactly the bytes a scan needs.
    fn scan(&self, c: usize, m: &mut Meter, f: &mut dyn FnMut(usize, Val)) {
        match &self.cols[c] {
            Col::Fixed { w, pages, n } => {
                let cap = PAGE / w;
                for (p, page) in pages.iter().enumerate() {
                    let cnt = cap.min(n - p * cap);
                    let bytes = m.rd(c, p, page, 0, cnt * w);
                    for (i, ch) in bytes.chunks(*w).enumerate() {
                        f(p * cap + i, decode_fixed(c, ch));
                    }
                }
            }
            Col::Var { pages, starts, .. } => {
                for (p, pg) in pages.iter().enumerate() {
                    let offs = m.rd(c, p, &pg.buf, 0, 2 + 2 * pg.n).to_vec();
                    let data = m.rd(c, p, &pg.buf, pg.free_end, PAGE - pg.free_end);
                    for s in 0..pg.n {
                        let off = u16::from_le_bytes([offs[2 + 2 * s], offs[3 + 2 * s]]) as usize;
                        let end = if s == 0 {
                            PAGE
                        } else {
                            u16::from_le_bytes([offs[2 * s], offs[2 * s + 1]]) as usize
                        };
                        f(
                            starts[p] + s,
                            text(&data[off - pg.free_end..end - pg.free_end]),
                        );
                    }
                }
            }
        }
    }
}
impl Layout for Dsm {
    fn name(&self) -> String {
        "DSM".into()
    }
    fn insert(&mut self, r: &Row) -> usize {
        let mut touched = HashSet::new();
        for c in 0..NCOLS {
            match &mut self.cols[c] {
                Col::Fixed { w, pages, n } => {
                    let cap = PAGE / *w;
                    if *n % cap == 0 {
                        pages.push(vec![0; PAGE]);
                    }
                    let (p, i) = (*n / cap, *n % cap);
                    pages[p][i * *w..(i + 1) * *w].copy_from_slice(&r.fixed(c));
                    *n += 1;
                    touched.insert((c, p));
                }
                Col::Var {
                    pages,
                    starts,
                    total,
                } => {
                    let v = if c == 3 {
                        r.cat.as_bytes()
                    } else {
                        r.name.as_bytes()
                    };
                    if pages.last_mut().is_none_or(|p| !p.append(v)) {
                        starts.push(*total);
                        let mut p = NPage::new();
                        assert!(p.append(v));
                        pages.push(p);
                    }
                    *total += 1;
                    touched.insert((c, pages.len() - 1));
                }
            }
        }
        touched.len()
    }
    fn num_pages(&self) -> usize {
        self.cols
            .iter()
            .map(|c| match c {
                Col::Fixed { pages, .. } => pages.len(),
                Col::Var { pages, .. } => pages.len(),
            })
            .sum()
    }
    fn project(&self, rows: &[usize], cols: &[usize], m: &mut Meter) -> Vec<Vec<Val>> {
        rows.iter()
            .map(|&r| cols.iter().map(|&c| self.value(c, r, m)).collect())
            .collect()
    }
    fn select(&self, col: usize, pred: &Pred, m: &mut Meter) -> Vec<usize> {
        let mut out = Vec::new();
        self.scan(col, m, &mut |r, v| {
            if pred.test(&v) {
                out.push(r);
            }
        });
        out
    }
    fn sum_col(&self, col: usize, m: &mut Meter) -> i64 {
        let mut sum = 0i64;
        self.scan(col, m, &mut |_, v| sum = sum.wrapping_add(v.num()));
        sum
    }
}

// ======================= PAX =======================
/// Header: [n u16][minipage start u16 * 5][dict_len u16] = 14 bytes.
const PAX_HDR: usize = 14;
struct PaxPage {
    buf: Vec<u8>,
}
struct Pax {
    dict: bool,
    pages: Vec<PaxPage>,
    starts: Vec<usize>,
    open: Vec<Row>,
    sealed_rows: usize,
}
fn dict_of(rows: &[Row]) -> Vec<String> {
    let mut d: Vec<String> = Vec::new();
    for r in rows {
        if !d.contains(&r.cat) {
            d.push(r.cat.clone());
        }
    }
    d
}
fn dict_section(d: &[String]) -> Vec<u8> {
    let mut v = vec![d.len() as u8];
    for s in d {
        v.push(s.len() as u8);
        v.extend(s.as_bytes());
    }
    v
}
fn var_minipage(vals: &[&[u8]]) -> Vec<u8> {
    let mut v = Vec::new();
    let mut end = 0u16;
    v.extend(end.to_le_bytes());
    for s in vals {
        end += s.len() as u16;
        v.extend(end.to_le_bytes());
    }
    vals.iter().for_each(|s| v.extend(*s));
    v
}
impl Pax {
    fn new(dict: bool) -> Pax {
        Pax {
            dict,
            pages: Vec::new(),
            starts: Vec::new(),
            open: Vec::new(),
            sealed_rows: 0,
        }
    }
    fn minipages(&self, rows: &[Row]) -> (Vec<Vec<u8>>, usize) {
        let mut mp: Vec<Vec<u8>> = (0..3)
            .map(|c| rows.iter().flat_map(|r| r.fixed(c)).collect())
            .collect();
        let mut dict_len = 0;
        if self.dict {
            let d = dict_of(rows);
            let mut sec = dict_section(&d);
            dict_len = sec.len();
            sec.extend(
                rows.iter()
                    .map(|r| d.iter().position(|x| *x == r.cat).unwrap() as u8),
            );
            mp.push(sec);
        } else {
            mp.push(var_minipage(
                &rows.iter().map(|r| r.cat.as_bytes()).collect::<Vec<_>>(),
            ));
        }
        mp.push(var_minipage(
            &rows.iter().map(|r| r.name.as_bytes()).collect::<Vec<_>>(),
        ));
        (mp, dict_len)
    }
    fn size(&self, rows: &[Row]) -> usize {
        PAX_HDR + self.minipages(rows).0.iter().map(Vec::len).sum::<usize>()
    }
    fn seal(&mut self) {
        if self.open.is_empty() {
            return;
        }
        let (mp, dict_len) = self.minipages(&self.open);
        let mut buf = vec![0u8; PAGE];
        buf[0..2].copy_from_slice(&(self.open.len() as u16).to_le_bytes());
        let mut off = PAX_HDR;
        for (c, m) in mp.iter().enumerate() {
            buf[2 + 2 * c..4 + 2 * c].copy_from_slice(&(off as u16).to_le_bytes());
            buf[off..off + m.len()].copy_from_slice(m);
            off += m.len();
        }
        buf[12..14].copy_from_slice(&(dict_len as u16).to_le_bytes());
        self.starts.push(self.sealed_rows);
        self.sealed_rows += self.open.len();
        self.pages.push(PaxPage { buf });
        self.open.clear();
    }
    fn header(&self, m: &mut Meter, p: usize) -> (usize, [usize; NCOLS], usize) {
        let b = &self.pages[p].buf;
        let n = m.u16(0, p, b, 0);
        let mut st = [0; NCOLS];
        for (c, s) in st.iter_mut().enumerate() {
            *s = m.u16(0, p, b, 2 + 2 * c);
        }
        (n, st, m.u16(0, p, b, 12))
    }
    fn cell(
        &self,
        m: &mut Meter,
        p: usize,
        j: usize,
        c: usize,
        h: &(usize, [usize; NCOLS], usize),
    ) -> Val {
        let (n, st, dl) = h;
        let b = &self.pages[p].buf;
        match c {
            0..=2 => decode_fixed(c, m.rd(0, p, b, st[c] + j * WIDTH[c], WIDTH[c])),
            3 if self.dict => {
                let d = self.read_dict(m, p, st[3], *dl);
                let code = m.rd(0, p, b, st[3] + dl + j, 1)[0] as usize;
                Val::S(d[code].clone())
            }
            _ => {
                let o0 = m.u16(0, p, b, st[c] + 2 * j);
                let o1 = m.u16(0, p, b, st[c] + 2 * j + 2);
                text(m.rd(0, p, b, st[c] + 2 * (n + 1) + o0, o1 - o0))
            }
        }
    }
    fn read_dict(&self, m: &mut Meter, p: usize, start: usize, dl: usize) -> Vec<String> {
        let b = m.rd(0, p, &self.pages[p].buf, start, dl);
        let mut out = Vec::new();
        let mut o = 1;
        for _ in 0..b[0] {
            let l = b[o] as usize;
            out.push(String::from_utf8(b[o + 1..o + 1 + l].to_vec()).unwrap());
            o += 1 + l;
        }
        out
    }
}
impl Layout for Pax {
    fn name(&self) -> String {
        if self.dict {
            "PAX+dict".into()
        } else {
            "PAX".into()
        }
    }
    fn insert(&mut self, r: &Row) -> usize {
        self.open.push(r.clone());
        if self.size(&self.open) > PAGE {
            let last = self.open.pop().unwrap();
            self.seal();
            self.open.push(last);
        }
        1
    }
    fn finish(&mut self) {
        self.seal();
    }
    fn num_pages(&self) -> usize {
        self.pages.len()
    }
    fn project(&self, rows: &[usize], cols: &[usize], m: &mut Meter) -> Vec<Vec<Val>> {
        rows.iter()
            .map(|&r| {
                let (p, j) = locate(&self.starts, r);
                let h = self.header(m, p);
                cols.iter().map(|&c| self.cell(m, p, j, c, &h)).collect()
            })
            .collect()
    }
    fn select(&self, col: usize, pred: &Pred, m: &mut Meter) -> Vec<usize> {
        let mut out = Vec::new();
        for p in 0..self.pages.len() {
            let h = self.header(m, p);
            let (n, st, dl) = h;
            let b = &self.pages[p].buf;
            match (col, pred) {
                (0..=2, _) => {
                    let bytes = m.rd(0, p, b, st[col], n * WIDTH[col]);
                    for (j, ch) in bytes.chunks(WIDTH[col]).enumerate() {
                        if pred.test(&decode_fixed(col, ch)) {
                            out.push(self.starts[p] + j);
                        }
                    }
                }
                (3, Pred::EqStr(s)) if self.dict => {
                    let d = self.read_dict(m, p, st[3], dl);
                    let Some(code) = d.iter().position(|x| x == s) else {
                        continue;
                    }; // constant absent: skip the codes
                    let codes = m.rd(0, p, b, st[3] + dl, n);
                    out.extend(
                        codes
                            .iter()
                            .enumerate()
                            .filter(|&(_, &c)| c as usize == code)
                            .map(|(j, _)| self.starts[p] + j),
                    );
                }
                (_, Pred::EqStr(s)) => {
                    let last = m.u16(0, p, b, st[col] + 2 * n);
                    let mp = m.rd(0, p, b, st[col], 2 * (n + 1) + last);
                    let base = 2 * (n + 1);
                    for j in 0..n {
                        let o0 = u16::from_le_bytes([mp[2 * j], mp[2 * j + 1]]) as usize;
                        let o1 = u16::from_le_bytes([mp[2 * j + 2], mp[2 * j + 3]]) as usize;
                        if &mp[base + o0..base + o1] == s.as_bytes() {
                            out.push(self.starts[p] + j);
                        }
                    }
                }
                _ => panic!("unsupported predicate"),
            }
        }
        out
    }
    fn sum_col(&self, col: usize, m: &mut Meter) -> i64 {
        let mut sum = 0i64;
        for p in 0..self.pages.len() {
            let (n, st, _) = self.header(m, p);
            let bytes = m.rd(0, p, &self.pages[p].buf, st[col], n * WIDTH[col]);
            bytes
                .chunks(WIDTH[col])
                .for_each(|ch| sum = sum.wrapping_add(decode_fixed(col, ch).num()));
        }
        sum
    }
}

// ======================= data, oracle, queries =======================
fn gen_rows(n: usize) -> Vec<Row> {
    let mut rng = Rng(0x5107_0001);
    (0..n)
        .map(|_| Row {
            id: rng.next() >> 16,
            qty: rng.below(1000) as u32,
            price: rng.below(100_000) as i64 - 20_000,
            cat: CATS[rng.below(CATS.len())].to_string(),
            name: (0..6 + rng.below(20))
                .map(|_| (b'a' + rng.below(26) as u8) as char)
                .collect(),
        })
        .collect()
}

#[derive(Clone, Debug)]
enum Query {
    Point(usize),
    Project(Vec<usize>, Vec<usize>),
    Sum(usize),
    FilterSum(String),
    Wide(i64),
    NameEq(String),
}
const KINDS: [&str; 6] = [
    "point read (full tuple)",
    "project 10 rows x 2 cols",
    "sum(col) over table",
    "sum(price) where cat=X",
    "wide: all cols where qty>900",
    "name = X (string eq)",
];
impl Query {
    fn kind(&self) -> usize {
        match self {
            Query::Point(_) => 0,
            Query::Project(..) => 1,
            Query::Sum(_) => 2,
            Query::FilterSum(_) => 3,
            Query::Wide(_) => 4,
            Query::NameEq(_) => 5,
        }
    }
}

fn run(q: &Query, l: &dyn Layout, m: &mut Meter) -> Vec<Vec<Val>> {
    let all: Vec<usize> = (0..NCOLS).collect();
    match q {
        Query::Point(r) => l.project(&[*r], &all, m),
        Query::Project(rows, cols) => l.project(rows, cols, m),
        Query::Sum(c) => vec![vec![Val::I(l.sum_col(*c, m))]],
        Query::FilterSum(cat) => {
            // Late materialisation: filter on one column, fetch the other only for survivors.
            let ids = l.select(3, &Pred::EqStr(cat.clone()), m);
            let s: i64 = l.project(&ids, &[2], m).iter().map(|r| r[0].num()).sum();
            vec![vec![Val::I(s)]]
        }
        Query::Wide(t) => {
            let ids = l.select(1, &Pred::GtNum(*t), m);
            l.project(&ids, &all, m)
        }
        Query::NameEq(s) => l
            .select(4, &Pred::EqStr(s.clone()), m)
            .into_iter()
            .map(|r| vec![Val::U(r as u64)])
            .collect(),
    }
}

fn oracle(q: &Query, rows: &[Row]) -> Vec<Vec<Val>> {
    let all: Vec<Val> = Vec::new();
    let _ = all;
    let full = |r: &Row| (0..NCOLS).map(|c| r.val(c)).collect::<Vec<_>>();
    match q {
        Query::Point(r) => vec![full(&rows[*r])],
        Query::Project(rs, cs) => rs
            .iter()
            .map(|&r| cs.iter().map(|&c| rows[r].val(c)).collect())
            .collect(),
        Query::Sum(c) => vec![vec![Val::I(
            rows.iter()
                .fold(0i64, |a, r| a.wrapping_add(r.val(*c).num())),
        )]],
        Query::FilterSum(cat) => vec![vec![Val::I(
            rows.iter().filter(|r| r.cat == *cat).map(|r| r.price).sum(),
        )]],
        Query::Wide(t) => rows
            .iter()
            .filter(|r| r.qty as i64 > *t)
            .map(full)
            .collect(),
        Query::NameEq(s) => rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.name == *s)
            .map(|(i, _)| vec![Val::U(i as u64)])
            .collect(),
    }
}

fn gen_queries(rows: &[Row], n: usize) -> Vec<Query> {
    let mut rng = Rng(0xabc);
    (0..n)
        .map(|i| match i % 6 {
            0 => Query::Point(rng.below(rows.len())),
            1 => {
                let mut rs: Vec<usize> = (0..10).map(|_| rng.below(rows.len())).collect();
                rs.sort();
                Query::Project(rs, vec![rng.below(NCOLS), rng.below(NCOLS)])
            }
            2 => Query::Sum(rng.below(3)),
            3 => Query::FilterSum(CATS[rng.below(CATS.len())].to_string()),
            4 => Query::Wide(900),
            _ => Query::NameEq(rows[rng.below(rows.len())].name.clone()),
        })
        .collect()
}

fn build(layouts: &mut [Box<dyn Layout>], rows: &[Row]) -> Vec<f64> {
    layouts
        .iter_mut()
        .map(|l| {
            let touched: usize = rows.iter().map(|r| l.insert(r)).sum();
            l.finish();
            touched as f64 / rows.len() as f64
        })
        .collect()
}

fn main() {
    let rows = gen_rows(20_000);
    let mut layouts: Vec<Box<dyn Layout>> = vec![
        Box::new(Nsm::new()),
        Box::new(Dsm::new()),
        Box::new(Pax::new(false)),
        Box::new(Pax::new(true)),
    ];
    let ins = build(&mut layouts, &rows);
    println!("{} rows, {} byte pages", rows.len(), PAGE);
    println!("{:<10} {:>10} {:>14}", "layout", "pages", "pages/insert");
    for (l, i) in layouts.iter().zip(&ins) {
        println!("{:<10} {:>10} {:>14.2}", l.name(), l.num_pages(), i);
    }
    // OLTP insert cost: row-wise layouts write one page, DSM one page per column.
    assert_eq!(ins[0], 1.0);
    assert_eq!(ins[1], NCOLS as f64);
    assert_eq!(ins[2], 1.0);
    assert!(
        layouts[3].num_pages() < layouts[2].num_pages(),
        "dictionary encoding packs more rows per page"
    );
    assert!(
        layouts[2].num_pages() <= layouts[0].num_pages() * 11 / 10 + 1,
        "PAX pays little space overhead vs NSM"
    );

    // ----- 500 random queries, every layout against the oracle -----
    let queries = gen_queries(&rows, 500);
    let mut cost = vec![vec![Cost::default(); layouts.len()]; KINDS.len()];
    for q in &queries {
        let expect = oracle(q, &rows);
        for (li, l) in layouts.iter().enumerate() {
            let mut m = Meter::default();
            let got = run(q, l.as_ref(), &mut m);
            assert_eq!(
                got,
                expect,
                "{} disagrees with the oracle on {:?}",
                l.name(),
                q
            );
            cost[q.kind()][li].add(&m);
        }
    }
    println!("\n500 random queries: all four layouts agree with the Vec<Row> oracle");
    println!(
        "\nTotal cost per query kind (pages touched / cache lines / bytes read), 83-84 queries each:"
    );
    println!(
        "{:<30} {:>22} {:>22} {:>22} {:>22}",
        "query", "NSM", "DSM", "PAX", "PAX+dict"
    );
    for (k, name) in KINDS.iter().enumerate() {
        let f = |c: &Cost| format!("{}/{}/{}", c.pages, c.lines, c.bytes);
        println!(
            "{:<30} {:>22} {:>22} {:>22} {:>22}",
            name,
            f(&cost[k][0]),
            f(&cost[k][1]),
            f(&cost[k][2]),
            f(&cost[k][3])
        );
    }

    // ----- the expected winners -----
    let (nsm, dsm, pax, pxd) = (0, 1, 2, 3);
    let point = &cost[0];
    assert!(
        point[nsm].pages * NCOLS == point[dsm].pages,
        "DSM point read touches one page per column"
    );
    assert!(
        point[pax].pages == point[nsm].pages,
        "PAX keeps a tuple inside one page"
    );
    assert!(
        point[dsm].lines > 2 * point[nsm].lines,
        "tuple reconstruction costs cache lines in DSM"
    );
    let sum = &cost[2];
    assert!(
        sum[dsm].pages * 3 < sum[nsm].pages,
        "column scan reads far fewer pages in DSM"
    );
    assert!(
        sum[dsm].lines * 3 < sum[nsm].lines,
        "and far fewer cache lines"
    );
    assert!(
        sum[pax].lines * 3 < sum[nsm].lines,
        "PAX scans a column cache-friendly too"
    );
    assert!(
        sum[pax].pages > sum[dsm].pages * 2,
        "but PAX still reads every page of the table (IO)"
    );
    let fs = &cost[3];
    assert!(fs[dsm].lines < fs[nsm].lines && fs[pax].lines < fs[nsm].lines);
    assert!(
        fs[pxd].lines < fs[pax].lines,
        "equality on dictionary codes touches fewer lines than on strings"
    );
    assert!(
        fs[pxd].pages < fs[pax].pages,
        "and the compressed file has fewer pages to read"
    );
    let wide = &cost[4];
    println!(
        "\nwide scans (10% survive, all 5 columns rebuilt): DSM/NSM cache-line ratio {:.2} (NSM {} lines, DSM {} lines); the gap shrinks as selectivity and width grow",
        wide[dsm].lines as f64 / wide[nsm].lines as f64,
        wide[nsm].lines,
        wide[dsm].lines
    );
    assert!(
        wide[dsm].pages >= wide[nsm].pages / 2,
        "no free lunch: wide results make DSM read about as much as NSM"
    );

    // ----- late vs early materialisation -----
    println!("\nlate vs early materialisation: sum(price) where cat = 'books'");
    let all: Vec<usize> = (0..rows.len()).collect();
    let cols: Vec<usize> = (0..NCOLS).collect();
    let truth: i64 = rows
        .iter()
        .filter(|r| r.cat == "books")
        .map(|r| r.price)
        .sum();
    for l in layouts.iter() {
        let mut late = Meter::default();
        let ids = l.select(3, &Pred::EqStr("books".into()), &mut late);
        let s_late: i64 = l
            .project(&ids, &[2], &mut late)
            .iter()
            .map(|r| r[0].num())
            .sum();
        let mut early = Meter::default();
        let s_early: i64 = l
            .project(&all, &cols, &mut early)
            .iter()
            .filter(|r| r[3] == Val::S("books".into()))
            .map(|r| r[2].num())
            .sum();
        assert_eq!((s_late, s_early), (truth, truth));
        println!(
            "  {:<9} late: {:>5} pages {:>7} lines | early: {:>5} pages {:>7} lines",
            l.name(),
            late.pages.len(),
            late.lines.len(),
            early.pages.len(),
            early.lines.len()
        );
        // A row store reads whole rows either way, so late == early there; the layouts that can skip columns must gain.
        assert!(
            late.lines.len() <= early.lines.len(),
            "{}: late materialisation must not touch more",
            l.name()
        );
        assert!(
            l.name() == "NSM" || late.lines.len() * 2 < early.lines.len(),
            "{}: late materialisation should save a lot",
            l.name()
        );
    }

    // ----- dictionary interplay -----
    let mut m_plain = Meter::default();
    let mut m_dict = Meter::default();
    let a = layouts[pax].select(3, &Pred::EqStr("pets".into()), &mut m_plain);
    let b = layouts[pxd].select(3, &Pred::EqStr("pets".into()), &mut m_dict);
    assert_eq!(a, b);
    let none = layouts[pxd].select(3, &Pred::EqStr("absent".into()), &mut Meter::default());
    assert!(none.is_empty());
    println!(
        "\nequality scan on cat: PAX reads {} bytes, PAX+dict {} bytes ({:.1}x less)",
        m_plain.bytes,
        m_dict.bytes,
        m_plain.bytes as f64 / m_dict.bytes as f64
    );
    assert!(m_dict.bytes * 3 < m_plain.bytes);
    println!("\nall storage-model checks passed");
}
