//! RECIPE 700 (database): slotted pages and a heap file: variable-length tuples, stable record ids, compaction, forwarding, overflow pages.
//! LEVEL: advanced
//! Use in: row-store storage engines, anything that keeps variable-length records in fixed-size disk blocks.   Run: cargo run --release --example slotted_pages_heap_file
//! COMMON USES: the heap files of PostgreSQL (line pointers + tuples in 8 KiB pages, TOAST for big values), the page format of
//! SQLite and InnoDB (a cell/record pointer array in a page), SQL Server (forwarded records), and the "record id = (page, slot)"
//! convention every secondary index in a row store stores as its payload.
//! Course: CMU 15-445 Intro to Database Systems, Lecture 03 (Database Storage I: pages, slotted pages, heap files, tuple layout).
#![allow(dead_code)]
//!
//! WHAT: a DBMS stores tuples in fixed-size PAGES (here 512 bytes, so tests hit every corner; real engines use 4-16 KiB).
//! A SLOTTED PAGE has a small header, a SLOT ARRAY growing from the front and tuple bytes growing from the back; the free
//! space is the gap in the middle. A slot is `(offset, len)`; the record id `Rid = (page_id, slot_id)` names a tuple forever,
//! so an index may keep it. The page may move tuple bytes around (COMPACTION) as long as the slot keeps pointing at them.
//!
//! HOW (page): header `[checksum u32 | page_id u32 | kind u8 | pad | nslots u16 | free_end u16 | live_bytes u16]`.
//! `live_bytes` makes FREE-SPACE ACCOUNTING exact and O(1): `free = PAGE - HEADER - 4*nslots - live_bytes`. Deleting a tuple
//! zeroes its slot (a TOMBSTONE that insert reuses, so rids of other tuples never shift) and leaves a hole; when an insert
//! fits in total free space but not in the contiguous gap, the page compacts itself (slides tuples to the back, slot ids and
//! therefore rids unchanged). A checksum over the page catches torn or rotted pages on read.
//! HOW (updates): a record is `[tag | ...]` padded to 9 bytes minimum so it can always be overwritten by a pointer.
//!   Inline(0): the data. Forward(1): `(page, slot)` of the real data, a Moved(3) record in some other page; Overflow(2): the
//!   length and first page of a chain of OVERFLOW pages for values over 120 bytes (PostgreSQL's TOAST idea). An update first
//!   tries in place (shrink, or grow after compaction); failing that, it relocates the data and leaves a FORWARD pointer in
//!   the original slot, so the rid never changes; the next update collapses or re-points the pointer (never a chain of two).
//! HOW (heap file): the pages of a table plus a PAGE DIRECTORY with the free bytes of each page (a free-space map) so insert
//! finds a page with room without any IO. The alternatives are measured too: a LINKED LIST of pages (walk and read pages
//! until one fits: O(pages) IO per insert) and APPEND-ONLY (only ever look at the last page: never reuses freed space).
//! Storage is a `PageStore` trait with a counted in-memory disk and a real temp file using positional IO (`read_at`).
//!
//! VERIFIED: 30k seeded random insert/delete/update/read operations against a `HashMap<Rid, Vec<u8>>` model; after every
//! op the touched page is checked (tuples never overlap and stay inside the data area, `free` and `live_bytes` equal a
//! recomputation, the directory entry equals the page) and periodically the whole file (every forward pointer targets a
//! Moved record, every overflow page is reachable exactly once, a full scan equals the model). The same workload on the
//! file-backed store must give byte-identical results to the in-memory store, and a flipped byte must fail the checksum.
//!
//! Read: https://15445.courses.cs.cmu.edu/fall2024/slides/03-storage1.pdf (CMU 15-445 storage lecture),
//! https://www.postgresql.org/docs/current/storage-page-layout.html (PostgreSQL page layout) and
//! https://www.postgresql.org/docs/current/storage-toast.html (TOAST: oversized values out of line)
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::os::unix::fs::FileExt;
use std::sync::atomic::{AtomicU64, Ordering};

const PAGE_SIZE: usize = 512;
const HEADER: usize = 16;
const SLOT: usize = 4;
const MIN_REC: usize = 9; // big enough for the largest pointer record (Overflow)
const INLINE_MAX: usize = 120; // payloads above this go to overflow pages
const OV_CHUNK: usize = PAGE_SIZE - HEADER;
const NONE: u32 = u32::MAX;

const KIND_SLOTTED: u8 = 0;
const KIND_OVERFLOW: u8 = 1;
const KIND_FREE: u8 = 2;

static COMPACTIONS: AtomicU64 = AtomicU64::new(0);

// ---------- deterministic randomness ----------
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
    fn var_bytes(&mut self, lo: usize, span: usize) -> Vec<u8> {
        let n = lo + self.below(span);
        self.bytes(n)
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| (self.next() >> 24) as u8).collect()
    }
}

// ---------- the slotted page ----------
#[derive(Clone)]
struct Page {
    b: [u8; PAGE_SIZE],
}

fn fnv32(data: &[u8]) -> u32 {
    data.iter().fold(0x811c_9dc5u32, |h, &x| {
        (h ^ x as u32).wrapping_mul(0x0100_0193)
    })
}

impl Page {
    fn zeroed() -> Page {
        Page { b: [0; PAGE_SIZE] }
    }
    fn new_slotted(id: u32) -> Page {
        let mut p = Page::zeroed();
        p.b[4..8].copy_from_slice(&id.to_le_bytes());
        p.b[8] = KIND_SLOTTED;
        p.set_u16(12, PAGE_SIZE);
        p
    }
    fn new_overflow(id: u32, next: u32, chunk: &[u8]) -> Page {
        let mut p = Page::zeroed();
        p.b[4..8].copy_from_slice(&id.to_le_bytes());
        p.b[8] = KIND_OVERFLOW;
        p.b[10..14].copy_from_slice(&next.to_le_bytes());
        p.set_u16(14, chunk.len());
        p.b[HEADER..HEADER + chunk.len()].copy_from_slice(chunk);
        p
    }
    fn u16_at(&self, o: usize) -> usize {
        u16::from_le_bytes([self.b[o], self.b[o + 1]]) as usize
    }
    fn set_u16(&mut self, o: usize, v: usize) {
        self.b[o..o + 2].copy_from_slice(&(v as u16).to_le_bytes());
    }
    fn kind(&self) -> u8 {
        self.b[8]
    }
    fn nslots(&self) -> usize {
        self.u16_at(10)
    }
    fn free_end(&self) -> usize {
        self.u16_at(12)
    }
    fn live(&self) -> usize {
        self.u16_at(14)
    }
    fn slot(&self, i: usize) -> (usize, usize) {
        let o = HEADER + i * SLOT;
        (self.u16_at(o), self.u16_at(o + 2))
    }
    fn set_slot(&mut self, i: usize, off: usize, len: usize) {
        let o = HEADER + i * SLOT;
        self.set_u16(o, off);
        self.set_u16(o + 2, len);
    }
    /// Gap between the end of the slot array and the start of tuple data.
    fn contiguous_free(&self) -> usize {
        self.free_end() - (HEADER + self.nslots() * SLOT)
    }
    /// All reclaimable bytes, gap plus holes left by deletes and shrinking updates.
    fn total_free(&self) -> usize {
        PAGE_SIZE - HEADER - self.nslots() * SLOT - self.live()
    }
    fn get(&self, slot: usize) -> Option<&[u8]> {
        if slot >= self.nslots() {
            return None;
        }
        let (off, len) = self.slot(slot);
        (off != 0).then(|| &self.b[off..off + len])
    }
    fn place(&mut self, slot: usize, rec: &[u8]) {
        let off = self.free_end() - rec.len();
        self.b[off..off + rec.len()].copy_from_slice(rec);
        self.set_u16(12, off);
        self.set_slot(slot, off, rec.len());
        self.set_u16(14, self.live() + rec.len());
    }
    /// Insert, reusing a tombstone slot if there is one. Compacts when only the holes make it fit.
    fn insert(&mut self, rec: &[u8]) -> Option<usize> {
        let n = self.nslots();
        let reuse = (0..n).find(|&i| self.slot(i).0 == 0);
        let need = rec.len() + if reuse.is_some() { 0 } else { SLOT };
        if need > self.total_free() {
            return None;
        }
        if need > self.contiguous_free() {
            self.compact();
        }
        let slot = reuse.unwrap_or_else(|| {
            self.set_u16(10, n + 1);
            n
        });
        self.place(slot, rec);
        Some(slot)
    }
    fn delete(&mut self, slot: usize) {
        let (off, len) = self.slot(slot);
        assert!(off != 0, "double delete");
        self.set_slot(slot, 0, 0);
        self.set_u16(14, self.live() - len);
        // Trailing tombstones can be dropped: no rid beyond them exists.
        while self.nslots() > 0 && self.slot(self.nslots() - 1).0 == 0 {
            self.set_u16(10, self.nslots() - 1);
        }
    }
    /// Overwrite a tuple. Shrinking happens in place; growing needs enough total free space (after compaction).
    fn update(&mut self, slot: usize, rec: &[u8]) -> bool {
        let (off, len) = self.slot(slot);
        assert!(off != 0);
        if rec.len() <= len {
            self.b[off..off + rec.len()].copy_from_slice(rec);
            self.set_slot(slot, off, rec.len());
            self.set_u16(14, self.live() - (len - rec.len()));
            return true;
        }
        if rec.len() > self.total_free() + len {
            return false;
        }
        self.set_slot(slot, 0, 0); // detach so compaction skips it; the slot id is kept
        self.set_u16(14, self.live() - len);
        if rec.len() > self.contiguous_free() {
            self.compact();
        }
        self.place(slot, rec);
        true
    }
    /// Slide every live tuple to the back of the page. Slot ids (hence rids) are untouched.
    fn compact(&mut self) {
        COMPACTIONS.fetch_add(1, Ordering::Relaxed);
        let tuples: Vec<(usize, Vec<u8>)> = (0..self.nslots())
            .filter_map(|i| self.get(i).map(|t| (i, t.to_vec())))
            .collect();
        self.set_u16(12, PAGE_SIZE);
        self.set_u16(14, 0);
        for (i, t) in tuples {
            self.place(i, &t);
        }
    }
    fn seal(&mut self) {
        let c = fnv32(&self.b[4..]);
        self.b[0..4].copy_from_slice(&c.to_le_bytes());
    }
    fn verify(&self) -> bool {
        u32::from_le_bytes([self.b[0], self.b[1], self.b[2], self.b[3]]) == fnv32(&self.b[4..])
    }
    /// Structural invariants of a slotted page.
    fn check(&self) -> Result<(), String> {
        let slots_end = HEADER + self.nslots() * SLOT;
        if slots_end > self.free_end() || self.free_end() > PAGE_SIZE {
            return Err("slot array overlaps data area".into());
        }
        let mut spans: Vec<(usize, usize)> = Vec::new();
        let mut live = 0;
        for i in 0..self.nslots() {
            let (off, len) = self.slot(i);
            if off == 0 {
                continue;
            }
            if off < self.free_end() || off + len > PAGE_SIZE || len < MIN_REC {
                return Err(format!("slot {i} out of bounds ({off},{len})"));
            }
            spans.push((off, len));
            live += len;
        }
        spans.sort();
        if spans.windows(2).any(|w| w[0].0 + w[0].1 > w[1].0) {
            return Err("tuples overlap".into());
        }
        if live != self.live() {
            return Err(format!("live_bytes {} != recomputed {live}", self.live()));
        }
        if self.total_free() != PAGE_SIZE - slots_end - live {
            return Err("free-space accounting off".into());
        }
        Ok(())
    }
}

// ---------- page stores: counted memory disk and a real file ----------
trait PageStore {
    fn read(&mut self, id: u32) -> Page;
    fn write(&mut self, id: u32, p: &Page);
    fn allocate(&mut self) -> u32;
    fn pages(&self) -> u32;
    fn io(&self) -> (u64, u64);
    fn flip_bit(&mut self, id: u32, byte: usize);
}

#[derive(Default)]
struct SimDisk {
    pages: Vec<Page>,
    reads: u64,
    writes: u64,
}
impl PageStore for SimDisk {
    fn read(&mut self, id: u32) -> Page {
        self.reads += 1;
        self.pages[id as usize].clone()
    }
    fn write(&mut self, id: u32, p: &Page) {
        self.writes += 1;
        self.pages[id as usize] = p.clone();
    }
    fn allocate(&mut self) -> u32 {
        self.pages.push(Page::zeroed());
        self.pages.len() as u32 - 1
    }
    fn pages(&self) -> u32 {
        self.pages.len() as u32
    }
    fn io(&self) -> (u64, u64) {
        (self.reads, self.writes)
    }
    fn flip_bit(&mut self, id: u32, byte: usize) {
        self.pages[id as usize].b[byte] ^= 1;
    }
}

/// A file of back-to-back pages addressed with positional IO (pread/pwrite): no seek state to race on.
struct FileDisk {
    f: File,
    path: std::path::PathBuf,
    n: u32,
    reads: u64,
    writes: u64,
}
impl FileDisk {
    fn create(tag: &str) -> FileDisk {
        let path =
            std::env::temp_dir().join(format!("cookbook_heap_{}_{tag}.db", std::process::id()));
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .expect("create temp file");
        FileDisk {
            f,
            path,
            n: 0,
            reads: 0,
            writes: 0,
        }
    }
}
impl Drop for FileDisk {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
impl PageStore for FileDisk {
    fn read(&mut self, id: u32) -> Page {
        self.reads += 1;
        let mut p = Page::zeroed();
        self.f
            .read_exact_at(&mut p.b, id as u64 * PAGE_SIZE as u64)
            .expect("pread");
        p
    }
    fn write(&mut self, id: u32, p: &Page) {
        self.writes += 1;
        self.f
            .write_all_at(&p.b, id as u64 * PAGE_SIZE as u64)
            .expect("pwrite");
    }
    fn allocate(&mut self) -> u32 {
        self.n += 1;
        self.f
            .write_all_at(&[0; PAGE_SIZE], (self.n as u64 - 1) * PAGE_SIZE as u64)
            .expect("extend");
        self.n - 1
    }
    fn pages(&self) -> u32 {
        self.n
    }
    fn io(&self) -> (u64, u64) {
        (self.reads, self.writes)
    }
    fn flip_bit(&mut self, id: u32, byte: usize) {
        let mut p = self.read(id);
        p.b[byte] ^= 1;
        self.write(id, &p);
    }
}

// ---------- records ----------
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct Rid {
    page: u32,
    slot: u16,
}

enum Rec {
    Inline(Vec<u8>),
    Forward(Rid),
    Overflow { len: usize, first: u32 },
    Moved(Vec<u8>),
}

fn pad(mut v: Vec<u8>) -> Vec<u8> {
    if v.len() < MIN_REC {
        v.resize(MIN_REC, 0);
    }
    v
}
fn enc_data(tag: u8, data: &[u8]) -> Vec<u8> {
    let mut v = vec![tag];
    v.extend_from_slice(&(data.len() as u16).to_le_bytes());
    v.extend_from_slice(data);
    pad(v)
}
fn enc_forward(r: Rid) -> Vec<u8> {
    let mut v = vec![1];
    v.extend_from_slice(&r.page.to_le_bytes());
    v.extend_from_slice(&r.slot.to_le_bytes());
    pad(v)
}
fn enc_overflow(len: usize, first: u32) -> Vec<u8> {
    let mut v = vec![2];
    v.extend_from_slice(&(len as u32).to_le_bytes());
    v.extend_from_slice(&first.to_le_bytes());
    v
}
fn decode(b: &[u8]) -> Rec {
    let u32_at = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    let data = || {
        let n = u16::from_le_bytes([b[1], b[2]]) as usize;
        b[3..3 + n].to_vec()
    };
    match b[0] {
        0 => Rec::Inline(data()),
        1 => Rec::Forward(Rid {
            page: u32_at(1),
            slot: u16::from_le_bytes([b[5], b[6]]),
        }),
        2 => Rec::Overflow {
            len: u32_at(1) as usize,
            first: u32_at(5),
        },
        3 => Rec::Moved(data()),
        t => panic!("bad record tag {t}"),
    }
}

// ---------- heap file ----------
#[derive(Debug)]
enum HeapError {
    Corrupt(u32),
}

#[derive(Clone, Copy, PartialEq)]
enum Placement {
    /// Free-space map in memory: pick the first page with room, no IO.
    Directory,
    /// Walk the page chain reading each page until one has room.
    LinkedList,
    /// Only the last page is considered: never reuses space freed earlier.
    AppendOnly,
}

#[derive(Clone, Copy)]
struct Entry {
    kind: u8,
    free: usize,
}

#[derive(Default, Debug, Clone, Copy)]
struct Stats {
    in_place: u64,
    forwarded: u64,
    overflow_values: u64,
    collapsed: u64,
}

struct HeapFile<D: PageStore> {
    disk: D,
    dir: Vec<Entry>,
    free_pages: Vec<u32>,
    placement: Placement,
    stats: Stats,
}

type R<T> = Result<T, HeapError>;

impl<D: PageStore> HeapFile<D> {
    fn new(disk: D, placement: Placement) -> Self {
        HeapFile {
            disk,
            dir: Vec::new(),
            free_pages: Vec::new(),
            placement,
            stats: Stats::default(),
        }
    }
    fn load(&mut self, id: u32) -> R<Page> {
        let p = self.disk.read(id);
        if p.verify() {
            Ok(p)
        } else {
            Err(HeapError::Corrupt(id))
        }
    }
    fn store(&mut self, id: u32, p: &mut Page) {
        p.seal();
        self.disk.write(id, p);
        self.dir[id as usize] = Entry {
            kind: p.kind(),
            free: if p.kind() == KIND_SLOTTED {
                p.total_free()
            } else {
                0
            },
        };
    }
    /// A raw page for the slotted or overflow role: reuse a freed overflow page before growing the file.
    fn alloc_page(&mut self) -> u32 {
        if let Some(id) = self.free_pages.pop() {
            return id;
        }
        let id = self.disk.allocate();
        self.dir.push(Entry {
            kind: KIND_FREE,
            free: 0,
        });
        id
    }
    fn new_slotted_page(&mut self) -> u32 {
        let id = self.alloc_page();
        let mut p = Page::new_slotted(id);
        self.store(id, &mut p);
        id
    }
    fn find_page(&mut self, need: usize) -> R<Option<u32>> {
        match self.placement {
            Placement::Directory => Ok(self
                .dir
                .iter()
                .position(|e| e.kind == KIND_SLOTTED && e.free >= need)
                .map(|i| i as u32)),
            Placement::LinkedList => {
                for id in 0..self.dir.len() as u32 {
                    if self.dir[id as usize].kind == KIND_SLOTTED
                        && self.load(id)?.total_free() >= need
                    {
                        return Ok(Some(id));
                    }
                }
                Ok(None)
            }
            Placement::AppendOnly => {
                let last = (0..self.dir.len())
                    .rev()
                    .find(|&i| self.dir[i].kind == KIND_SLOTTED);
                Ok(last.filter(|&i| self.dir[i].free >= need).map(|i| i as u32))
            }
        }
    }
    fn place(&mut self, rec: &[u8]) -> R<Rid> {
        let need = rec.len() + SLOT;
        let id = match self.find_page(need)? {
            Some(id) => id,
            None => self.new_slotted_page(),
        };
        let mut p = self.load(id)?;
        let slot = p.insert(rec).expect("directory promised room");
        self.store(id, &mut p);
        Ok(Rid {
            page: id,
            slot: slot as u16,
        })
    }
    /// Large values live in a chain of overflow pages; the home record keeps just (len, first page).
    fn write_chain(&mut self, data: &[u8]) -> u32 {
        let mut next = NONE;
        for chunk in data.chunks(OV_CHUNK).rev() {
            let id = self.alloc_page();
            let mut p = Page::new_overflow(id, next, chunk);
            self.store(id, &mut p);
            next = id;
        }
        next
    }
    fn read_chain(&mut self, first: u32, len: usize) -> R<Vec<u8>> {
        let (mut out, mut id) = (Vec::with_capacity(len), first);
        while id != NONE {
            let p = self.load(id)?;
            let n = p.u16_at(14);
            out.extend_from_slice(&p.b[HEADER..HEADER + n]);
            id = u32::from_le_bytes([p.b[10], p.b[11], p.b[12], p.b[13]]);
        }
        assert_eq!(out.len(), len);
        Ok(out)
    }
    fn free_chain(&mut self, first: u32) -> R<()> {
        let mut id = first;
        while id != NONE {
            let p = self.load(id)?;
            let next = u32::from_le_bytes([p.b[10], p.b[11], p.b[12], p.b[13]]);
            self.dir[id as usize] = Entry {
                kind: KIND_FREE,
                free: 0,
            };
            self.free_pages.push(id);
            id = next;
        }
        Ok(())
    }
    fn encode_home(&mut self, data: &[u8]) -> Vec<u8> {
        if data.len() <= INLINE_MAX {
            enc_data(0, data)
        } else {
            self.stats.overflow_values += 1;
            let first = self.write_chain(data);
            enc_overflow(data.len(), first)
        }
    }
    fn record_at(&mut self, rid: Rid) -> R<Option<Rec>> {
        let p = self.load(rid.page)?;
        Ok(p.get(rid.slot as usize).map(decode))
    }

    fn insert(&mut self, data: &[u8]) -> R<Rid> {
        let rec = self.encode_home(data);
        self.place(&rec)
    }
    fn read(&mut self, rid: Rid) -> R<Option<Vec<u8>>> {
        match self.record_at(rid)? {
            Some(Rec::Inline(d)) => Ok(Some(d)),
            Some(Rec::Overflow { len, first }) => Ok(Some(self.read_chain(first, len)?)),
            Some(Rec::Forward(t)) => match self.record_at(t)? {
                Some(Rec::Moved(d)) => Ok(Some(d)),
                _ => panic!("forward pointer to a non-moved record"),
            },
            Some(Rec::Moved(_)) | None => Ok(None), // a Moved record is not addressable by users
        }
    }
    fn release_dependents(&mut self, old: Rec) -> R<()> {
        match old {
            Rec::Forward(t) => {
                let mut p = self.load(t.page)?;
                p.delete(t.slot as usize);
                self.store(t.page, &mut p);
            }
            Rec::Overflow { first, .. } => self.free_chain(first)?,
            _ => {}
        }
        Ok(())
    }
    fn delete(&mut self, rid: Rid) -> R<bool> {
        let old = match self.record_at(rid)? {
            Some(Rec::Moved(_)) | None => return Ok(false),
            Some(r) => r,
        };
        let mut p = self.load(rid.page)?;
        p.delete(rid.slot as usize);
        self.store(rid.page, &mut p);
        self.release_dependents(old)?;
        Ok(true)
    }
    /// Replace the value; the rid is stable. In place when it fits (growing may compact), otherwise relocate
    /// the data and leave a forward pointer in the original slot.
    fn update(&mut self, rid: Rid, data: &[u8]) -> R<bool> {
        let old = match self.record_at(rid)? {
            Some(Rec::Moved(_)) | None => return Ok(false),
            Some(r) => r,
        };
        let was_forward = matches!(old, Rec::Forward(_));
        let new_rec = self.encode_home(data);
        let mut p = self.load(rid.page)?;
        if p.update(rid.slot as usize, &new_rec) {
            self.store(rid.page, &mut p);
            self.stats.in_place += 1;
            self.stats.collapsed += was_forward as u64;
        } else {
            let target = self.place(&enc_data(3, data))?;
            let mut p = self.load(rid.page)?; // placement may have rewritten pages
            assert!(
                p.update(rid.slot as usize, &enc_forward(target)),
                "a pointer always fits over a record"
            );
            self.store(rid.page, &mut p);
            self.stats.forwarded += 1;
        }
        self.release_dependents(old)?;
        Ok(true)
    }
    /// Sequential scan through the page directory in page order.
    fn scan(&mut self) -> R<Vec<(Rid, Vec<u8>)>> {
        let mut out = Vec::new();
        for id in 0..self.dir.len() as u32 {
            if self.dir[id as usize].kind != KIND_SLOTTED {
                continue;
            }
            let p = self.load(id)?;
            for s in 0..p.nslots() {
                let rid = Rid {
                    page: id,
                    slot: s as u16,
                };
                if let Some(Rec::Moved(_)) = p.get(s).map(decode) {
                    continue; // reached through its forward pointer instead
                }
                if let Some(d) = self.read(rid)? {
                    out.push((rid, d));
                }
            }
        }
        Ok(out)
    }

    // ----- invariants -----
    fn check_page(&mut self, id: u32) {
        let e = self.dir[id as usize];
        if e.kind != KIND_SLOTTED {
            return;
        }
        let p = self.load(id).expect("checksum");
        p.check().unwrap_or_else(|m| panic!("page {id}: {m}"));
        assert_eq!(
            e.free,
            p.total_free(),
            "directory out of sync with page {id}"
        );
    }
    fn check_all(&mut self) {
        assert_eq!(self.dir.len() as u32, self.disk.pages());
        let (mut forwards, mut moved) = (HashSet::new(), 0);
        let mut chain_pages = HashSet::new();
        for id in 0..self.dir.len() as u32 {
            self.check_page(id);
            if self.dir[id as usize].kind != KIND_SLOTTED {
                continue;
            }
            let p = self.load(id).unwrap();
            for s in 0..p.nslots() {
                match p.get(s).map(decode) {
                    Some(Rec::Forward(t)) => {
                        assert!(forwards.insert(t), "two forwards to one target")
                    }
                    Some(Rec::Moved(_)) => moved += 1,
                    Some(Rec::Overflow { len, first }) => {
                        let (mut n, mut pid) = (0, first);
                        while pid != NONE {
                            assert!(chain_pages.insert(pid), "overflow page shared");
                            let q = self.load(pid).unwrap();
                            assert_eq!(q.kind(), KIND_OVERFLOW);
                            n += q.u16_at(14);
                            pid = u32::from_le_bytes([q.b[10], q.b[11], q.b[12], q.b[13]]);
                        }
                        assert_eq!(n, len);
                    }
                    _ => {}
                }
            }
        }
        assert_eq!(
            forwards.len(),
            moved,
            "forward pointers and moved records must pair up"
        );
        for t in &forwards {
            assert!(matches!(self.record_at(*t).unwrap(), Some(Rec::Moved(_))));
        }
        let overflow_pages: HashSet<u32> = (0..self.dir.len() as u32)
            .filter(|&i| self.dir[i as usize].kind == KIND_OVERFLOW)
            .collect();
        assert_eq!(
            overflow_pages, chain_pages,
            "leaked or dangling overflow pages"
        );
        let free: HashSet<u32> = self.free_pages.iter().copied().collect();
        assert!(free.iter().all(|&i| self.dir[i as usize].kind == KIND_FREE));
    }
    /// (slotted pages, overflow pages, payload bytes, hole bytes, total file bytes)
    fn space(&mut self) -> (usize, usize, usize, usize, usize) {
        let (mut slotted, mut ov, mut payload, mut holes) = (0, 0, 0, 0);
        for id in 0..self.dir.len() as u32 {
            match self.dir[id as usize].kind {
                KIND_SLOTTED => {
                    let p = self.load(id).unwrap();
                    slotted += 1;
                    payload += p.live();
                    holes += p.total_free() - p.contiguous_free();
                }
                KIND_OVERFLOW => {
                    ov += 1;
                    payload += self.load(id).unwrap().u16_at(14);
                }
                _ => {}
            }
        }
        (slotted, ov, payload, holes, self.dir.len() * PAGE_SIZE)
    }
}

// ---------- the model test ----------
fn gen_data(rng: &mut Rng) -> Vec<u8> {
    let r = rng.below(100);
    let len = match r {
        0..=69 => rng.below(60),
        70..=92 => 60 + rng.below(100),
        93..=98 => rng.below(400),
        _ => 400 + rng.below(1500),
    };
    rng.bytes(len)
}

fn digest(h: u64, x: &[u8]) -> u64 {
    x.iter()
        .fold(h, |h, &b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}

/// Random operations against a HashMap oracle; returns a digest of every observable result (for cross-backend equality).
fn model_test<D: PageStore>(heap: &mut HeapFile<D>, ops: usize, seed: u64) -> u64 {
    let mut rng = Rng(seed);
    let mut model: HashMap<Rid, Vec<u8>> = HashMap::new();
    let mut keys: Vec<Rid> = Vec::new();
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for step in 0..ops {
        let r = rng.below(100);
        let touched: u32;
        if r < 35 || keys.is_empty() {
            let data = gen_data(&mut rng);
            let rid = heap.insert(&data).unwrap();
            assert!(
                model.insert(rid, data).is_none(),
                "rid {rid:?} handed out twice while live"
            );
            keys.push(rid);
            h = digest(h, &[rid.page as u8, rid.slot as u8]);
            touched = rid.page;
        } else if r < 55 {
            let rid = keys.swap_remove(rng.below(keys.len()));
            assert!(heap.delete(rid).unwrap());
            model.remove(&rid);
            assert_eq!(heap.read(rid).unwrap(), None, "deleted rid still readable");
            touched = rid.page;
        } else if r < 80 {
            let rid = keys[rng.below(keys.len())];
            let data = gen_data(&mut rng);
            assert!(heap.update(rid, &data).unwrap());
            model.insert(rid, data); // same key: the rid never changes on update
            touched = rid.page;
        } else {
            let rid = keys[rng.below(keys.len())];
            let got = heap.read(rid).unwrap();
            assert_eq!(got.as_ref(), model.get(&rid));
            h = digest(h, got.as_deref().unwrap());
            touched = rid.page;
        }
        heap.check_page(touched);
        if step % 1000 == 999 {
            heap.check_all();
            let scanned: HashMap<Rid, Vec<u8>> = heap.scan().unwrap().into_iter().collect();
            assert_eq!(scanned, model, "scan differs from the model at step {step}");
        }
    }
    for (rid, v) in &model {
        assert_eq!(heap.read(*rid).unwrap().as_ref(), Some(v));
    }
    heap.check_all();
    h
}

// ---------- targeted demos ----------
fn page_demo() {
    let mut p = Page::new_slotted(7);
    let ids: Vec<usize> = (0..6)
        .map(|i| p.insert(&pad(vec![i as u8; 40 + i * 5])).unwrap())
        .collect();
    println!(
        "page after 6 inserts: slots={} contiguous_free={} total_free={}",
        p.nslots(),
        p.contiguous_free(),
        p.total_free()
    );
    p.delete(ids[1]);
    p.delete(ids[3]);
    let (cf, tf) = (p.contiguous_free(), p.total_free());
    println!(
        "after deleting slots 1 and 3: contiguous_free={cf} total_free={tf} (holes={})",
        tf - cf
    );
    assert!(tf - cf > 0);
    // Insert something bigger than the gap but smaller than gap+holes: forces in-page compaction.
    let before = COMPACTIONS.load(Ordering::Relaxed);
    let big = pad(vec![9; cf + 20]);
    assert!(big.len() <= tf);
    let s = p.insert(&big).unwrap();
    assert_eq!(s, ids[1], "tombstone slot is reused");
    assert_eq!(COMPACTIONS.load(Ordering::Relaxed), before + 1);
    for &i in &[0usize, 2, 4, 5] {
        assert_eq!(
            p.get(i).unwrap(),
            &pad(vec![i as u8; 40 + i * 5])[..],
            "slot {i} moved bytes but kept its id"
        );
    }
    p.check().unwrap();
    println!(
        "insert of {} bytes compacted the page; slot ids 0,2,4,5 still resolve to their data",
        big.len()
    );
    // Oversize insert must fail cleanly.
    assert!(p.insert(&vec![1; PAGE_SIZE]).is_none());
    // Checksum detects a flipped bit.
    p.seal();
    assert!(p.verify());
    p.b[300] ^= 0x10;
    assert!(!p.verify());
}

fn update_demo() {
    let mut heap = HeapFile::new(SimDisk::default(), Placement::Directory);
    // Fill one page tightly so that growing a tuple must relocate.
    let rids: Vec<Rid> = (0..8)
        .map(|i| heap.insert(&vec![i as u8; 50]).unwrap())
        .collect();
    assert!(rids.iter().all(|r| r.page == 0));
    let victim = rids[2];
    let st = heap.stats;
    heap.update(victim, &vec![0xAB; 110]).unwrap(); // does not fit in page 0 any more
    assert_eq!(heap.stats.forwarded, st.forwarded + 1);
    assert_eq!(heap.read(victim).unwrap().unwrap(), vec![0xAB; 110]);
    println!(
        "grown tuple relocated behind a forward pointer; rid {victim:?} unchanged, data intact"
    );
    let p0 = heap.load(0).unwrap();
    assert!(matches!(
        decode(p0.get(victim.slot as usize).unwrap()),
        Rec::Forward(_)
    ));
    // Shrink it: the data comes home, the Moved record is freed (collapse).
    heap.update(victim, &[1, 2, 3]).unwrap();
    assert!(matches!(
        decode(heap.load(0).unwrap().get(victim.slot as usize).unwrap()),
        Rec::Inline(_)
    ));
    heap.check_all();
    // A big value goes to overflow pages and back.
    let big: Vec<u8> = (0..1300u32).map(|i| (i % 251) as u8).collect();
    heap.update(rids[5], &big).unwrap();
    let (_, ov, ..) = heap.space();
    assert_eq!(ov, big.len().div_ceil(OV_CHUNK));
    assert_eq!(heap.read(rids[5]).unwrap().unwrap(), big);
    heap.update(rids[5], b"tiny").unwrap();
    assert_eq!(heap.space().1, 0, "chain freed on shrink");
    assert_eq!(heap.free_pages.len(), ov);
    // Freed overflow pages are recycled by the next big value instead of growing the file.
    let pages_before = heap.disk.pages();
    heap.insert(&big).unwrap();
    assert_eq!(heap.disk.pages(), pages_before, "overflow pages reused");
    heap.check_all();
    println!(
        "overflow chain of {ov} pages allocated, freed on shrink and recycled by the next big insert"
    );
}

fn corruption_demo<D: PageStore>(name: &str, disk: D) {
    let mut heap = HeapFile::new(disk, Placement::Directory);
    let rid = heap.insert(b"precious bytes").unwrap();
    assert!(heap.read(rid).unwrap().is_some());
    heap.disk.flip_bit(rid.page, 400);
    match heap.read(rid) {
        Err(HeapError::Corrupt(p)) => {
            println!("{name}: flipped bit on page {p} detected by the page checksum")
        }
        other => panic!("corruption not detected: {other:?}"),
    }
}

fn placement_demo() {
    println!(
        "\n--- placement strategies: 4000 variable inserts, delete 60%, then 2000 more inserts ---"
    );
    println!(
        "{:<12} {:>7} {:>9} {:>10} {:>10} {:>10}",
        "placement", "pages", "util %", "page reads", "page writes", "holes B"
    );
    let mut results = Vec::new();
    for pl in [
        Placement::Directory,
        Placement::LinkedList,
        Placement::AppendOnly,
    ] {
        let mut heap = HeapFile::new(SimDisk::default(), pl);
        let mut rng = Rng(99);
        let mut rids: Vec<Rid> = (0..4000)
            .map(|_| heap.insert(&rng.var_bytes(10, 100)).unwrap())
            .collect();
        for _ in 0..2400 {
            let i = rng.below(rids.len());
            heap.delete(rids.swap_remove(i)).unwrap();
        }
        let mut again: Vec<Rid> = (0..2000)
            .map(|_| heap.insert(&rng.var_bytes(10, 100)).unwrap())
            .collect();
        rids.append(&mut again);
        heap.check_all();
        let (sl, ov, payload, holes, total) = heap.space();
        let (r, w) = heap.disk.io();
        let name = match pl {
            Placement::Directory => "directory",
            Placement::LinkedList => "linked-list",
            Placement::AppendOnly => "append-only",
        };
        println!(
            "{name:<12} {:>7} {:>8.1}% {r:>10} {w:>10} {holes:>10}",
            sl + ov,
            100.0 * payload as f64 / total as f64
        );
        results.push((pl, sl, r, rids));
    }
    let (dir, ll, ao) = (&results[0], &results[1], &results[2]);
    assert_eq!(
        dir.3, ll.3,
        "linked list and directory both do first-fit: identical rids"
    );
    assert!(
        ll.2 > 5 * dir.2,
        "walking the chain costs far more reads than the directory ({} vs {})",
        ll.2,
        dir.2
    );
    assert!(
        ao.1 > dir.1,
        "append-only wastes the space freed by deletes"
    );
}

fn workload_table() {
    println!("\n--- space utilisation and fragmentation by workload (512-byte pages) ---");
    println!(
        "{:<34} {:>6} {:>8} {:>9} {:>9}",
        "workload", "pages", "util %", "holes B", "compacts"
    );
    let report = |name: &str, heap: &mut HeapFile<SimDisk>, c0: u64| {
        let (sl, ov, payload, holes, total) = heap.space();
        let c = COMPACTIONS.load(Ordering::Relaxed) - c0;
        println!(
            "{name:<34} {:>6} {:>7.1}% {holes:>9} {c:>9}",
            sl + ov,
            100.0 * payload as f64 / total as f64
        );
        100.0 * payload as f64 / total as f64
    };
    let new = || HeapFile::new(SimDisk::default(), Placement::Directory);
    let mut rng = Rng(5);

    let c0 = COMPACTIONS.load(Ordering::Relaxed);
    let mut h = new();
    for _ in 0..3000 {
        h.insert(&[7; 40]).unwrap();
    }
    let fixed = report("insert 3000 x 40B", &mut h, c0);

    let c0 = COMPACTIONS.load(Ordering::Relaxed);
    let mut h = new();
    let mut rids: Vec<Rid> = (0..3000)
        .map(|_| h.insert(&rng.var_bytes(5, 110)).unwrap())
        .collect();
    let var = report("insert 3000 x 5..115B", &mut h, c0);

    let c0 = COMPACTIONS.load(Ordering::Relaxed);
    for _ in 0..1800 {
        let i = rng.below(rids.len());
        h.delete(rids.swap_remove(i)).unwrap();
    }
    let churn = report("... then delete 60% (no compaction)", &mut h, c0);

    let c0 = COMPACTIONS.load(Ordering::Relaxed);
    for _ in 0..1800 {
        rids.push(h.insert(&rng.var_bytes(5, 110)).unwrap());
    }
    report("... then insert 1800 more", &mut h, c0);

    let c0 = COMPACTIONS.load(Ordering::Relaxed);
    let mut h2 = new();
    let rids2: Vec<Rid> = (0..3000)
        .map(|_| h2.insert(&rng.bytes(20)).unwrap())
        .collect();
    for r in &rids2 {
        h2.update(*r, &rng.bytes(60)).unwrap();
    }
    report("grow every tuple 20B -> 60B", &mut h2, c0);
    println!(
        "    (forwarded {} of {} grown tuples, {} grew in place)",
        h2.stats.forwarded,
        rids2.len(),
        h2.stats.in_place
    );
    assert!(h2.stats.forwarded > 0);
    assert!(fixed > 80.0, "fixed-size tuples pack tightly: {fixed:.1}%");
    assert!(
        var > 70.0 && churn < var,
        "deletes leave the file emptier than it was: {churn:.1} vs {var:.1}"
    );
    let _ = rids;
}

fn main() {
    println!("--- one slotted page ---");
    page_demo();
    println!("\n--- updates: in place, forward pointer, overflow pages ---");
    update_demo();
    println!("\n--- checksums ---");
    corruption_demo("sim disk ", SimDisk::default());
    corruption_demo("temp file", FileDisk::create("corrupt"));

    println!("\n--- model test: 30000 random ops vs HashMap<Rid, Vec<u8>> ---");
    let c0 = COMPACTIONS.load(Ordering::Relaxed);
    let mut heap = HeapFile::new(SimDisk::default(), Placement::Directory);
    let sim_digest = model_test(&mut heap, 30_000, 0x5eed_0001);
    let (sl, ov, payload, holes, total) = heap.space();
    let (r, w) = heap.disk.io();
    let compactions = COMPACTIONS.load(Ordering::Relaxed) - c0;
    println!(
        "slotted pages={sl} overflow pages={ov} payload={payload}B of {total}B ({:.1}% utilised, {holes}B holes) io: {r} reads {w} writes",
        100.0 * payload as f64 / total as f64
    );
    println!(
        "stats: {:?}, in-page compactions: {compactions}",
        heap.stats
    );
    assert!(
        heap.stats.forwarded > 100
            && heap.stats.in_place > 1000
            && heap.stats.overflow_values > 50
            && heap.stats.collapsed > 0
    );
    assert!(compactions > 100);

    println!("\n--- same workload on a real file (positional IO) vs the simulated disk ---");
    let mut a = HeapFile::new(SimDisk::default(), Placement::Directory);
    let mut b = HeapFile::new(FileDisk::create("model"), Placement::Directory);
    let da = model_test(&mut a, 4000, 0x5eed_0002);
    let db = model_test(&mut b, 4000, 0x5eed_0002);
    assert_eq!(da, db, "both backends must behave identically");
    assert_eq!(a.disk.io(), b.disk.io());
    assert_eq!(a.scan().unwrap(), b.scan().unwrap());
    println!(
        "4000 ops: identical rids, data and IO counts ({:?}) on both backends; digest {da:016x}",
        a.disk.io()
    );
    let _ = sim_digest;

    placement_demo();
    workload_table();
    println!("\nall slotted-page / heap-file checks passed");
}
