//! CAPSTONE 01 (database): an LSM-tree-lite key-value store: WAL + memtable + SSTables with bloom filters + k-way merge compaction.
//! LEVEL: capstone
//! Use in: embedded storage engines, durable job/state stores, the DBMS roadmap; combines bytes, IO, errors, heaps, model tests.   Run: cargo run --release --example mini_kv_store
#![allow(dead_code)]
//!
//! WHAT: writes go to a write-ahead log (WAL, [len][crc32][payload] frames) and then to an in-memory sorted memtable (BTreeMap; a delete
//! is a TOMBSTONE, i.e. `None`, because older files may still hold the key). When the memtable grows past a threshold it is FLUSHED into an
//! immutable SSTable file: sorted records, a SPARSE index (one key per 16 records, so memory stays small) and a BLOOM filter.
//! WHY: appends and sequential writes are the fastest thing disks do; sorting once per flush gives binary-searchable files without ever
//! updating in place. Reads pay for that: they check the memtable, then SSTables newest to oldest, and the bloom filter lets a read skip
//! a file that definitely lacks the key. COMPACTION merges all tables with a BinaryHeap (k-way merge), keeping only the newest version of
//! each key and dropping tombstones (safe only because nothing older remains). RECOVERY = reopen the directory: load tables, replay the
//! WAL, stop at the first torn/corrupt frame and truncate it away. A WAL reset after a flush is safe to crash through: replay is idempotent.
//! A frame header with length 0 also ends the log: a tail of zeros would otherwise pass the checksum, because the CRC32 of nothing is 0.
//! Compaction deletes the old tables OLDEST FIRST and forgets each one only after its file is gone, so a delete that fails leaves the open
//! store, like the directory, with every table that still matters.
//! A log append that fails is cut back off the log, so later writes never sit behind half a frame; if even that fails, the log refuses
//! more writes. The directory is synced after a table is renamed into place (before the log is emptied) and after each delete. Once a
//! write is logged and applied it has happened: a failed automatic flush is counted and retried by the next write, not returned.
//! Opening a table also checks that its index offsets rise and stay inside the data. NOT promised: the log itself is never synced, so
//! an acknowledged write can still be lost when the power fails.
//! VERIFIED by a model test: thousands of seeded random operations (put/delete/get/scan/flush/compact/crash/torn-tail) against a BTreeMap.
//! COMMON USES: RocksDB/LevelDB/Cassandra/ScyllaDB storage engines, Kafka-style commit logs, embedded state in stream processors, sled-like
//! stores, search-index segments (Lucene merges), time-series ingest (write-optimised), any "append fast, compact later" system.
//! Read: https://github.com/skyzh/mini-lsm , https://github.com/google/leveldb/blob/main/doc/impl.md , https://docs.rs/crc32fast
use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, BinaryHeap};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::ops::Bound;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
enum KvError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("corrupt data: {0}")]
    Corrupt(&'static str),
    #[error("the log is unusable after an earlier failure")]
    LogUnusable,
}
type Result<T> = std::result::Result<T, KvError>;

type Key = Vec<u8>;
type Val = Option<Vec<u8>>; // None = tombstone
type Entry = (Key, Val);

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

// ---------- bounds-checked decoding: every read of untrusted bytes goes through `take` ----------
fn take<'a>(buf: &'a [u8], pos: &mut usize, n: usize) -> Result<&'a [u8]> {
    let end = pos
        .checked_add(n)
        .ok_or(KvError::Corrupt("length overflow"))?;
    let s = buf.get(*pos..end).ok_or(KvError::Corrupt("short buffer"))?;
    *pos = end;
    Ok(s)
}
fn take_u32(buf: &[u8], pos: &mut usize) -> Result<u32> {
    let b = take(buf, pos, 4)?
        .try_into()
        .map_err(|_| KvError::Corrupt("u32"))?;
    Ok(u32::from_le_bytes(b))
}
fn take_u64(buf: &[u8], pos: &mut usize) -> Result<u64> {
    let b = take(buf, pos, 8)?
        .try_into()
        .map_err(|_| KvError::Corrupt("u64"))?;
    Ok(u64::from_le_bytes(b))
}

/// One record layout shared by the WAL payload and the SSTable data block: [tag][klen][key][vlen][value]
fn encode_entry(key: &[u8], val: Option<&[u8]>, out: &mut Vec<u8>) {
    out.push(val.is_some() as u8);
    out.extend((key.len() as u32).to_le_bytes());
    out.extend_from_slice(key);
    let v = val.unwrap_or(&[]);
    out.extend((v.len() as u32).to_le_bytes());
    out.extend_from_slice(v);
}
fn decode_entry(buf: &[u8], pos: &mut usize) -> Result<Entry> {
    let tag = take(buf, pos, 1)?[0];
    let klen = take_u32(buf, pos)? as usize;
    let key = take(buf, pos, klen)?.to_vec();
    let vlen = take_u32(buf, pos)? as usize;
    let val = take(buf, pos, vlen)?.to_vec();
    match tag {
        1 => Ok((key, Some(val))),
        0 => Ok((key, None)),
        _ => Err(KvError::Corrupt("bad record tag")),
    }
}

// ---------- write-ahead log ----------
struct Wal {
    file: File,
    len: u64,     // the bytes of whole frames
    broken: bool, // the file may hold something after `len`: nothing more may be appended
}

/// parse frames from the start; the first short or checksum-failing frame is the torn tail. Returns (entries, valid_len).
fn replay(bytes: &[u8]) -> Result<(Vec<Entry>, u64)> {
    let (mut out, mut pos) = (vec![], 0usize);
    loop {
        let Some(h) = bytes.get(pos..pos + 8) else {
            break;
        };
        let len = u32::from_le_bytes([h[0], h[1], h[2], h[3]]) as usize;
        let crc = u32::from_le_bytes([h[4], h[5], h[6], h[7]]);
        // No real record is empty. A zero-filled tail (left by some crashes) reads as len 0 and crc 0, and crc32(b"") IS 0,
        // so without this line it would pass the checksum, fail to decode, and the store could never be opened again.
        if len == 0 {
            break;
        }
        let Some(payload) = bytes.get(pos + 8..pos + 8 + len) else {
            break;
        };
        if crc32(payload) != crc {
            break;
        }
        // a frame with a VALID checksum that does not decode is a bug or real corruption, not a torn write: surface it
        out.push(decode_entry(payload, &mut 0)?);
        pos += 8 + len;
    }
    Ok((out, pos as u64))
}

impl Wal {
    fn open(path: &Path) -> Result<(Wal, Vec<Entry>)> {
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![],
            Err(e) => return Err(e.into()),
        };
        let (entries, valid) = replay(&bytes)?;
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        file.set_len(valid)?; // chop the torn tail so new appends never follow garbage
        let wal = Wal {
            file,
            len: valid,
            broken: false,
        };
        Ok((wal, entries))
    }

    fn append(&mut self, key: &[u8], val: Option<&[u8]>) -> Result<()> {
        // one write keeps the frame contiguous; a real store would also sync_data() (group commit)
        self.append_with(key, val, |file, frame| file.write_all(frame))
    }

    /// `append` with the write passed in, so that a check can make it fail part-way
    fn append_with(
        &mut self,
        key: &[u8],
        val: Option<&[u8]>,
        write: impl FnOnce(&mut File, &[u8]) -> std::io::Result<()>,
    ) -> Result<()> {
        if self.broken {
            return Err(KvError::LogUnusable);
        }
        let mut payload = Vec::new();
        encode_entry(key, val, &mut payload);
        let mut frame = Vec::with_capacity(8 + payload.len());
        frame.extend((payload.len() as u32).to_le_bytes());
        frame.extend(crc32(&payload).to_le_bytes());
        frame.extend(payload);
        if let Err(e) = write(&mut self.file, &frame) {
            // part of the frame may be in the file. Replay would stop there and silently drop every LATER write, so cut it off now
            self.broken = self.file.set_len(self.len).is_err();
            return Err(e.into());
        }
        self.len += frame.len() as u64;
        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
        if self.broken {
            return Err(KvError::LogUnusable);
        }
        if let Err(e) = self.file.set_len(0) {
            self.broken = true; // the real length is unknown now
            return Err(e.into());
        }
        self.len = 0;
        Ok(())
    }
}

// ---------- bloom filter ----------
struct Bloom {
    bits: Vec<u8>,
    k: u8,
}

fn fnv1a(key: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &b in key {
        h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
    }
    h ^ (h >> 32) // extra mix: FNV's low bits are weak and we take a modulus
}

impl Bloom {
    /// ~10 bits per key with 4 probes gives about a 1-2% false-positive rate; false NEGATIVES are impossible by construction.
    fn build(entries: &[Entry]) -> Bloom {
        let nbytes = (entries.len() * 10).max(64).div_ceil(8);
        let mut bloom = Bloom {
            bits: vec![0; nbytes],
            k: 4,
        };
        for (key, _) in entries {
            for p in bloom.probes(key).collect::<Vec<_>>() {
                bloom.bits[p / 8] |= 1 << (p % 8);
            }
        }
        bloom
    }
    /// double hashing: probe_i = h1 + i*h2 simulates k independent hash functions from one 64-bit hash
    fn probes(&self, key: &[u8]) -> impl Iterator<Item = usize> + use<> {
        let nbits = (self.bits.len() * 8) as u64;
        let (h1, h2) = (fnv1a(key), fnv1a(key).rotate_left(31) | 1);
        (0..self.k as u64).map(move |i| (h1.wrapping_add(i.wrapping_mul(h2)) % nbits) as usize)
    }
    fn may_contain(&self, key: &[u8]) -> bool {
        self.probes(key)
            .all(|p| self.bits[p / 8] & (1 << (p % 8)) != 0)
    }
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(self.k);
        out.extend((self.bits.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.bits);
    }
    fn decode(buf: &[u8]) -> Result<Bloom> {
        let mut pos = 0;
        let k = take(buf, &mut pos, 1)?[0];
        let n = take_u32(buf, &mut pos)? as usize;
        let bits = take(buf, &mut pos, n)?.to_vec();
        if bits.is_empty() || k == 0 {
            return Err(KvError::Corrupt("empty bloom"));
        }
        Ok(Bloom { bits, k })
    }
}

// ---------- directory sync ----------
/// how the store makes a change of names (rename, remove) durable. A function pointer, so that a check can watch or fail it.
type DirSync = fn(&Path) -> std::io::Result<()>;

/// syncing a file does not make its NAME durable: that needs a sync of the directory (opening one read-only works on Unix)
fn sync_dir(dir: &Path) -> std::io::Result<()> {
    File::open(dir)?.sync_all()
}

// ---------- SSTable: [data][index][bloom][footer: data_end, bloom_off, count, crc32, magic] ----------
const INDEX_EVERY: usize = 16;
const MAGIC: u32 = 0x5353_5431;
const FOOTER: usize = 32;

struct SsTable {
    id: u64,
    path: PathBuf,
    index: Vec<(Key, u64)>, // sparse: first key of every 16th record -> file offset
    data_end: u64,
    bloom: Bloom,
}

impl SsTable {
    fn path_for(dir: &Path, id: u64) -> PathBuf {
        dir.join(format!("sst_{id:06}.sst"))
    }

    /// write to a temp file, fsync, rename, fsync the directory: readers/recovery never see a half-written table under its final
    /// name, and when this returns the name is durable too, so the caller may discard what the table replaces
    fn write(dir: &Path, id: u64, entries: &[Entry], sync: DirSync) -> Result<SsTable> {
        let (mut buf, mut index) = (Vec::new(), Vec::new());
        for (i, (k, v)) in entries.iter().enumerate() {
            if i % INDEX_EVERY == 0 {
                index.push((k.clone(), buf.len() as u64));
            }
            encode_entry(k, v.as_deref(), &mut buf);
        }
        let data_end = buf.len() as u64;
        buf.extend((index.len() as u32).to_le_bytes());
        for (k, off) in &index {
            buf.extend((k.len() as u32).to_le_bytes());
            buf.extend_from_slice(k);
            buf.extend(off.to_le_bytes());
        }
        let bloom_off = buf.len() as u64;
        let bloom = Bloom::build(entries);
        bloom.encode(&mut buf);
        for n in [data_end, bloom_off, entries.len() as u64] {
            buf.extend(n.to_le_bytes());
        }
        let crc = crc32(&buf);
        buf.extend(crc.to_le_bytes());
        buf.extend(MAGIC.to_le_bytes());

        let path = Self::path_for(dir, id);
        let tmp = path.with_extension("tmp");
        let mut f = File::create(&tmp)?;
        f.write_all(&buf)?;
        f.sync_all()?;
        fs::rename(&tmp, &path)?;
        sync(dir)?;
        Ok(SsTable {
            id,
            path,
            index,
            data_end,
            bloom,
        })
    }

    /// load index + bloom into memory; data stays on disk. The whole-file checksum is verified once, here.
    fn open(path: &Path, id: u64) -> Result<SsTable> {
        let bytes = fs::read(path)?;
        let body_len = bytes
            .len()
            .checked_sub(FOOTER)
            .ok_or(KvError::Corrupt("file too short"))?;
        let mut pos = body_len;
        let (data_end, bloom_off) = (take_u64(&bytes, &mut pos)?, take_u64(&bytes, &mut pos)?);
        let _count = take_u64(&bytes, &mut pos)?;
        let (crc, magic) = (take_u32(&bytes, &mut pos)?, take_u32(&bytes, &mut pos)?);
        if magic != MAGIC || crc != crc32(&bytes[..body_len + 24]) {
            return Err(KvError::Corrupt("bad sstable footer or checksum"));
        }
        let (data_end_u, bloom_off_u) = (data_end as usize, bloom_off as usize);
        if data_end_u > bloom_off_u || bloom_off_u > body_len {
            return Err(KvError::Corrupt("bad sstable offsets"));
        }
        let section = &bytes[data_end_u..bloom_off_u];
        let mut pos = 0;
        let n = take_u32(section, &mut pos)?;
        let mut index: Vec<(Key, u64)> = Vec::new();
        for _ in 0..n {
            let klen = take_u32(section, &mut pos)? as usize;
            let key = take(section, &mut pos, klen)?.to_vec();
            let off = take_u64(section, &mut pos)?;
            // a valid checksum does not make offsets sensible: reads compute `end - start` and allocate that much
            let in_order = match index.last() {
                None => off == 0,
                Some((last_key, last)) => *last_key < key && *last < off,
            };
            if !in_order || off >= data_end {
                return Err(KvError::Corrupt("bad sstable index"));
            }
            index.push((key, off));
        }
        if pos != section.len() || index.is_empty() != (data_end == 0) {
            return Err(KvError::Corrupt("bad sstable index"));
        }
        let bloom = Bloom::decode(&bytes[bloom_off_u..body_len])?;
        Ok(SsTable {
            id,
            path: path.to_path_buf(),
            index,
            data_end,
            bloom,
        })
    }

    fn read_range(&self, start: u64, end: u64) -> Result<Vec<u8>> {
        let mut f = File::open(&self.path)?;
        f.seek(SeekFrom::Start(start))?;
        let mut buf = vec![0u8; (end - start) as usize];
        f.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Some(None) = a tombstone lives here (stop searching older tables); None = key not in this table.
    fn get(&self, key: &[u8]) -> Result<Option<Val>> {
        let slot = self.index.partition_point(|(k, _)| k.as_slice() <= key);
        if slot == 0 {
            return Ok(None); // smaller than the table's first key
        }
        let start = self.index[slot - 1].1;
        let end = self.index.get(slot).map_or(self.data_end, |e| e.1);
        let buf = self.read_range(start, end)?; // at most INDEX_EVERY records
        let mut pos = 0;
        while pos < buf.len() {
            let (k, v) = decode_entry(&buf, &mut pos)?;
            match k.as_slice().cmp(key) {
                Ordering::Equal => return Ok(Some(v)),
                Ordering::Greater => break, // sorted: we passed where it would be
                Ordering::Less => {}
            }
        }
        Ok(None)
    }

    fn read_all(&self) -> Result<Vec<Entry>> {
        let buf = self.read_range(0, self.data_end)?;
        let (mut out, mut pos) = (Vec::new(), 0);
        while pos < buf.len() {
            out.push(decode_entry(&buf, &mut pos)?);
        }
        Ok(out)
    }
}

/// k-way merge. `sources` are sorted runs ordered NEWEST FIRST. The heap is a min-heap on (key, source index), so for equal keys the
/// newest source pops first and every later copy is a shadowed older version to discard. Tombstones may be dropped only when the
/// merge covers every older table (otherwise the delete would "resurrect" an old value).
fn merge_sources(sources: Vec<Vec<Entry>>, drop_tombstones: bool) -> Vec<Entry> {
    let mut iters: Vec<_> = sources.into_iter().map(|s| s.into_iter()).collect();
    let mut heap = BinaryHeap::new();
    for (i, it) in iters.iter_mut().enumerate() {
        if let Some((k, v)) = it.next() {
            heap.push(Reverse((k, i, v)));
        }
    }
    let mut out = Vec::new();
    while let Some(Reverse((key, src, val))) = heap.pop() {
        if let Some((k, v)) = iters[src].next() {
            heap.push(Reverse((k, src, v)));
        }
        while heap.peek().is_some_and(|Reverse((k, _, _))| *k == key) {
            if let Some(Reverse((_, s, _))) = heap.pop() {
                if let Some((k, v)) = iters[s].next() {
                    heap.push(Reverse((k, s, v)));
                }
            }
        }
        if !(drop_tombstones && val.is_none()) {
            out.push((key, val));
        }
    }
    out
}

// ---------- the store ----------
#[derive(Default, Debug)]
struct Stats {
    bloom_skips: u64,
    table_reads: u64,
    failed_auto_flushes: u64,
}

struct Store {
    dir: PathBuf,
    memtable: BTreeMap<Key, Val>,
    mem_bytes: usize,
    wal: Wal,
    tables: Vec<SsTable>, // oldest first
    next_id: u64,
    flush_threshold: usize,
    stats: Stats,
    dir_sync: DirSync,
}

impl Store {
    /// Open == recover: delete leftover temp files, load tables, replay the WAL into a fresh memtable.
    fn open(dir: &Path, flush_threshold: usize) -> Result<Store> {
        fs::create_dir_all(dir)?;
        let mut found = Vec::new();
        for ent in fs::read_dir(dir)? {
            let path = ent?.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if name.ends_with(".tmp") {
                fs::remove_file(&path)?; // crash during a flush or compaction: the table never became visible
            } else if let Some(id) = name
                .strip_prefix("sst_")
                .and_then(|n| n.strip_suffix(".sst"))
            {
                found.push((
                    id.parse::<u64>()
                        .map_err(|_| KvError::Corrupt("bad table name"))?,
                    path,
                ));
            }
        }
        found.sort();
        let tables = found
            .iter()
            .map(|(id, p)| SsTable::open(p, *id))
            .collect::<Result<Vec<_>>>()?;
        let next_id = tables.last().map_or(1, |t| t.id + 1);
        let (wal, entries) = Wal::open(&dir.join("wal.log"))?;
        let mut store = Store {
            dir: dir.to_path_buf(),
            memtable: BTreeMap::new(),
            mem_bytes: 0,
            wal,
            tables,
            next_id,
            flush_threshold,
            stats: Stats::default(),
            dir_sync: sync_dir,
        };
        for (k, v) in entries {
            store.mem_insert(k, v);
        }
        Ok(store)
    }

    fn mem_insert(&mut self, key: Key, val: Val) {
        self.mem_bytes += key.len() + val.as_ref().map_or(0, Vec::len) + 16;
        self.memtable.insert(key, val);
    }

    /// log first, memtable second: if we crash in between, replay redoes the write; the reverse order could lose acknowledged data
    fn apply(&mut self, key: &[u8], val: Option<&[u8]>) -> Result<()> {
        self.wal.append(key, val)?;
        self.mem_insert(key.to_vec(), val.map(<[u8]>::to_vec));
        // the write has happened whatever becomes of the flush: an Err here would tell the caller it had not.
        // The memtable and log keep everything, and the next write tries again.
        if self.mem_bytes >= self.flush_threshold && self.flush().is_err() {
            self.stats.failed_auto_flushes += 1;
        }
        Ok(())
    }
    fn put(&mut self, key: &[u8], val: &[u8]) -> Result<()> {
        self.apply(key, Some(val))
    }
    fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.apply(key, None)
    }
    fn wal_len(&self) -> u64 {
        self.wal.len
    }

    fn get(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        if let Some(v) = self.memtable.get(key) {
            return Ok(v.clone());
        }
        for t in self.tables.iter().rev() {
            if !t.bloom.may_contain(key) {
                self.stats.bloom_skips += 1;
                continue;
            }
            self.stats.table_reads += 1;
            if let Some(v) = t.get(key)? {
                return Ok(v); // first hit wins, even a tombstone
            }
        }
        Ok(None)
    }

    /// keys in [from, to): overlay oldest tables first, memtable last, so newer versions overwrite older ones
    fn scan(&self, from: &[u8], to: &[u8]) -> Result<Vec<(Key, Vec<u8>)>> {
        if from >= to {
            return Ok(Vec::new()); // BTreeMap::range panics when start > end
        }
        let mut merged: BTreeMap<Key, Val> = BTreeMap::new();
        for t in &self.tables {
            for (k, v) in t.read_all()? {
                if k.as_slice() >= from && k.as_slice() < to {
                    merged.insert(k, v);
                }
            }
        }
        for (k, v) in self
            .memtable
            .range::<[u8], _>((Bound::Included(from), Bound::Excluded(to)))
        {
            merged.insert(k.clone(), v.clone());
        }
        Ok(merged
            .into_iter()
            .filter_map(|(k, v)| v.map(|v| (k, v)))
            .collect())
    }

    /// table first, WAL reset second: a crash in between replays records that are already in the table, which is harmless
    fn flush(&mut self) -> Result<()> {
        if self.memtable.is_empty() {
            return Ok(());
        }
        let entries: Vec<Entry> = self
            .memtable
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let table = SsTable::write(&self.dir, self.next_id, &entries, self.dir_sync)?;
        self.tables.push(table);
        self.next_id += 1;
        self.memtable.clear();
        self.mem_bytes = 0;
        self.wal.reset()
    }

    /// full compaction: merge every table into one, then delete the old files OLDEST FIRST. The order matters: the merged table holds
    /// no tombstones, so the table with a delete marker must outlive every older table that still holds the deleted value. Then any
    /// state a crash or a failed delete leaves behind reads the same as before.
    fn compact(&mut self) -> Result<()> {
        self.compact_with(|path| fs::remove_file(path))
    }

    /// `compact` with the delete passed in, so that a check can make it fail
    fn compact_with(&mut self, mut remove: impl FnMut(&Path) -> std::io::Result<()>) -> Result<()> {
        if self.tables.len() < 2 {
            return Ok(());
        }
        let sources = self
            .tables
            .iter()
            .rev()
            .map(SsTable::read_all)
            .collect::<Result<Vec<_>>>()?;
        let merged = merge_sources(sources, true);
        let new_table = if merged.is_empty() {
            None
        } else {
            Some(SsTable::write(
                &self.dir,
                self.next_id,
                &merged,
                self.dir_sync,
            )?)
        };
        self.next_id += 1;
        let old = self.tables.len();
        self.tables.extend(new_table); // newest: it is visible on disk already
        for _ in 0..old {
            // forget a table only after its file is gone; on an error the list still matches the directory.
            // (Draining the list here lost EVERY table when one delete failed: dropping a Drain removes the rest.)
            remove(&self.tables[0].path)?;
            self.tables.remove(0);
            // make this removal durable before the next one: after a power cut, "oldest first" must still hold
            (self.dir_sync)(&self.dir)?;
        }
        Ok(())
    }

    fn sst_files_on_disk(&self) -> Result<usize> {
        let names = fs::read_dir(&self.dir)?
            .map(|e| e.map(|e| e.file_name()))
            .collect::<std::io::Result<Vec<_>>>()?;
        Ok(names
            .iter()
            .filter(|n| n.to_string_lossy().ends_with(".sst"))
            .count())
    }
}

// ---------- test harness ----------
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

struct TempDir(PathBuf);
impl TempDir {
    fn new(tag: &str) -> std::io::Result<TempDir> {
        let p = std::env::temp_dir().join(format!("{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p)?;
        Ok(TempDir(p))
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn rand_key(rng: &mut Rng) -> Key {
    format!("k{:03}", rng.below(200)).into_bytes()
}

/// every key stored in a table must pass its bloom filter; absent keys should mostly be rejected. Returns (false positives, probes).
fn check_blooms(store: &Store) -> Result<(usize, usize)> {
    let (mut fp, mut probes) = (0, 0);
    for t in &store.tables {
        for (k, _) in t.read_all()? {
            assert!(t.bloom.may_contain(&k), "bloom false negative for {k:?}");
        }
        for i in 0..200 {
            probes += 1;
            fp += t.bloom.may_contain(format!("absent-{i}").as_bytes()) as usize;
        }
    }
    Ok((fp, probes))
}

struct LastMutation {
    key: Key,
    prev: Option<Vec<u8>>,
    before: u64,
    after: u64,
}

#[derive(Default, Debug)]
struct Tally {
    crashes: usize,
    torn: usize,
    flushes: usize,
    compactions: usize,
    max_tables: usize,
    fp: usize,
    probes: usize,
}

fn model_test(dir: &Path, seed: u64, ops: usize) -> Result<(Store, Tally)> {
    const THRESHOLD: usize = 1500; // tiny on purpose: forces many flushes and tables
    let (mut rng, mut tally) = (Rng(seed), Tally::default());
    let mut store = Store::open(dir, THRESHOLD)?;
    let mut model: BTreeMap<Key, Vec<u8>> = BTreeMap::new();
    let mut last: Option<LastMutation> = None;
    for _ in 0..ops {
        let key = rand_key(&mut rng);
        let (prev, before) = (model.get(&key).cloned(), store.wal_len());
        match rng.below(100) {
            0..45 | 45..65 => {
                if rng.below(45 + 20) < 45 {
                    let val: Vec<u8> = (0..rng.below(60)).map(|_| rng.next() as u8).collect();
                    store.put(&key, &val)?;
                    model.insert(key.clone(), val);
                } else {
                    store.delete(&key)?;
                    model.remove(&key);
                }
                // only a record still sitting in the WAL (no flush happened) can be torn by a crash
                let after = store.wal_len();
                last = (after > before).then(|| LastMutation {
                    key,
                    prev,
                    before,
                    after,
                });
            }
            65..85 => assert_eq!(store.get(&key)?, model.get(&key).cloned(), "get {key:?}"),
            85..90 => {
                let (mut a, mut b) = (rand_key(&mut rng), rand_key(&mut rng));
                if a > b {
                    std::mem::swap(&mut a, &mut b);
                }
                let want: Vec<_> = model
                    .range(a.clone()..b.clone())
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                assert_eq!(store.scan(&a, &b)?, want, "scan {a:?}..{b:?}");
            }
            90..93 => {
                store.flush()?;
                (tally.flushes, last) = (tally.flushes + 1, None);
            }
            93..95 => {
                let n = store.tables.len();
                store.compact()?;
                if n >= 2 {
                    assert!(
                        store.tables.len() < n,
                        "compaction must reduce the file count"
                    );
                    assert_eq!(
                        store.sst_files_on_disk()?,
                        store.tables.len(),
                        "old files deleted"
                    );
                }
                (tally.compactions, last) = (tally.compactions + 1, None);
            }
            95..99 => {
                drop(store); // crash: no flush, no goodbye
                let mut expect_wal = None;
                if let Some(l) = last.take().filter(|_| rng.below(2) == 0) {
                    // tear the last record only: cut 1..=4 bytes off its end (never into earlier records)
                    let cut = 1 + rng.below(4.min((l.after - l.before - 1) as usize)) as u64;
                    OpenOptions::new()
                        .write(true)
                        .open(dir.join("wal.log"))?
                        .set_len(l.after - cut)?;
                    match l.prev {
                        Some(v) => model.insert(l.key, v),
                        None => model.remove(&l.key),
                    };
                    (expect_wal, tally.torn) = (Some(l.before), tally.torn + 1);
                }
                store = Store::open(dir, THRESHOLD)?;
                if let Some(len) = expect_wal {
                    assert_eq!(store.wal_len(), len, "recovery truncates the torn record");
                }
                (tally.crashes, last) = (tally.crashes + 1, None);
            }
            _ => {
                let (fp, probes) = check_blooms(&store)?;
                (tally.fp, tally.probes) = (tally.fp + fp, tally.probes + probes);
            }
        }
        tally.max_tables = tally.max_tables.max(store.tables.len());
    }
    // final audit: reopen cleanly and compare everything
    drop(store);
    let mut store = Store::open(dir, THRESHOLD)?;
    let all = store.scan(b"", b"\xff")?;
    assert_eq!(
        all,
        model
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Vec<_>>(),
        "final scan"
    );
    for i in 0..200 {
        let k = format!("k{i:03}").into_bytes();
        assert_eq!(store.get(&k)?, model.get(&k).cloned(), "final get {k:?}");
    }
    let (fp, probes) = check_blooms(&store)?;
    (tally.fp, tally.probes) = (tally.fp + fp, tally.probes + probes);
    Ok((store, tally))
}

fn demo(dir: &Path) -> Result<()> {
    let mut s = Store::open(dir, 1 << 20)?;
    s.put(b"apple", b"red")?;
    s.put(b"banana", b"yellow")?;
    s.flush()?; // table 1
    s.put(b"apple", b"green")?; // shadows the table-1 value
    s.delete(b"banana")?; // tombstone shadows table 1
    s.flush()?; // table 2
    assert_eq!(s.get(b"apple")?, Some(b"green".to_vec()));
    assert_eq!(s.get(b"banana")?, None);
    s.put(b"cherry", b"dark")?; // memtable + WAL only
    drop(s); // simulated crash
    let mut s = Store::open(dir, 1 << 20)?;
    assert_eq!(
        s.get(b"cherry")?,
        Some(b"dark".to_vec()),
        "WAL replay restored the unflushed write"
    );
    let before = s.tables.len();
    s.compact()?;
    assert_eq!(s.tables.len(), 1);
    let kept = s.tables[0].read_all()?;
    assert_eq!(
        kept,
        vec![(b"apple".to_vec(), Some(b"green".to_vec()))],
        "old versions and the tombstone are gone"
    );
    println!(
        "demo: {before} tables -> {} after compaction; recovered unflushed write from the WAL",
        s.tables.len()
    );
    Ok(())
}

/// a log followed by zeros (any number of them) must open, keep every record and accept new writes after them
fn check_zero_filled_tail(dir: &Path) -> Result<()> {
    for zeros in [1usize, 7, 8, 9, 64, 4096] {
        let dir = dir.join(format!("zeros{zeros}"));
        let mut s = Store::open(&dir, 1 << 20)?;
        s.put(b"a", b"1")?;
        let good = s.wal_len();
        drop(s);
        OpenOptions::new()
            .append(true)
            .open(dir.join("wal.log"))?
            .write_all(&vec![0; zeros])?;
        let mut s = Store::open(&dir, 1 << 20)?;
        assert_eq!(s.wal_len(), good, "the {zeros} zeros are cut off");
        s.put(b"b", b"2")?;
        drop(s);
        let mut s = Store::open(&dir, 1 << 20)?;
        assert_eq!(s.get(b"a")?, Some(b"1".to_vec()));
        assert_eq!(s.get(b"b")?, Some(b"2".to_vec()));
    }
    Ok(())
}

/// let `ok` deletes succeed and fail the next one, for every `ok`: the open store and a reopened one must both still read the same.
/// `live` decides whether the compaction writes a new table or has nothing left to write.
fn check_failed_delete(dir: &Path) -> Result<()> {
    for live in [false, true] {
        for ok in 0..3 {
            let dir = dir.join(format!("delete{live}{ok}"));
            let mut s = Store::open(&dir, 1 << 20)?;
            s.put(b"gone", b"old")?;
            s.flush()?;
            if live {
                s.put(b"kept", b"yes")?;
            } else {
                s.put(b"gone", b"newer")?;
            }
            s.flush()?;
            s.delete(b"gone")?;
            s.flush()?;
            let want: Vec<(Key, Vec<u8>)> = live
                .then(|| (b"kept".to_vec(), b"yes".to_vec()))
                .into_iter()
                .collect();
            let mut done = 0;
            let failed = s.compact_with(|path| {
                if done == ok {
                    return Err(std::io::Error::other("injected delete failure"));
                }
                done += 1;
                fs::remove_file(path)
            });
            assert!(matches!(failed, Err(KvError::Io(_))));
            assert_eq!(s.scan(b"", b"\xff")?, want, "open store, {ok} deleted");
            assert_eq!(s.tables.len(), s.sst_files_on_disk()?);
            assert_eq!(s.get(b"gone")?, None);
            drop(s);
            let mut s = Store::open(&dir, 1 << 20)?;
            assert_eq!(s.scan(b"", b"\xff")?, want, "reopened, {ok} deleted");
            assert_eq!(s.get(b"gone")?, None);
        }
    }
    Ok(())
}

/// a write that fails after part of the frame reached the file must not hide the writes that come after it
fn check_partial_append(dir: &Path) -> Result<()> {
    for keep in [0, 1, 7, 8, 12] {
        let dir = dir.join(format!("keep{keep}"));
        let mut s = Store::open(&dir, 1 << 20)?;
        s.put(b"before", b"1")?;
        let failed = s
            .wal
            .append_with(b"lost", Some(b"x".as_slice()), |file, frame| {
                file.write_all(&frame[..keep])?;
                Err(std::io::Error::other("injected: disk full"))
            });
        assert!(matches!(failed, Err(KvError::Io(_))));
        s.put(b"after", b"2")?;
        drop(s);
        let mut s = Store::open(&dir, 1 << 20)?;
        assert_eq!(s.get(b"before")?, Some(b"1".to_vec()));
        assert_eq!(s.get(b"after")?, Some(b"2".to_vec()), "{keep} bytes kept");
        assert_eq!(s.get(b"lost")?, None);
    }
    // if the half frame cannot be removed either, no later write may be accepted. A read-only handle makes set_len fail.
    let dir = dir.join("stuck");
    let mut s = Store::open(&dir, 1 << 20)?;
    let log = dir.join("wal.log");
    let failed = s.wal.append_with(b"k", None, |file, _| {
        *file = File::open(&log)?;
        Err(std::io::Error::other("injected"))
    });
    assert!(matches!(failed, Err(KvError::Io(_))));
    assert!(matches!(s.put(b"later", b"v"), Err(KvError::LogUnusable)));
    assert_eq!(s.get(b"later")?, None);
    Ok(())
}

fn check_reversed_scan(dir: &Path) -> Result<()> {
    let mut s = Store::open(dir, 1 << 20)?;
    s.put(b"m", b"1")?; // the memtable must not be empty: an empty BTreeMap does not even look at the bounds
    assert!(s.scan(b"z", b"a")?.is_empty());
    assert!(s.scan(b"m", b"m")?.is_empty());
    Ok(())
}

/// what each directory sync saw: (number of tables, bytes in the log)
fn observed_sync(dir: &Path) -> std::io::Result<()> {
    let mut tables = 0;
    for entry in fs::read_dir(dir)? {
        tables += entry?.file_name().to_string_lossy().ends_with(".sst") as usize;
    }
    let log = fs::metadata(dir.join("wal.log"))?.len();
    SYNCS.with(|seen| seen.borrow_mut().push((tables, log)));
    sync_dir(dir)
}
thread_local!(static SYNCS: std::cell::RefCell<Vec<(usize, u64)>> = Default::default());

/// No test in a running system can see whether a sync was durable. These check what CAN be checked: the directory is synced
/// after the rename and before the log is emptied, after each delete, and a failed sync stops the step that depends on it.
fn check_directory_syncs(dir: &Path) -> Result<()> {
    let mut s = Store::open(&dir.join("order"), 1 << 20)?;
    s.dir_sync = observed_sync;
    let mut logs = Vec::new();
    for key in [b"a", b"b", b"c"] {
        s.put(key, b"v")?;
        logs.push(s.wal_len());
        s.flush()?;
    }
    let seen = SYNCS.with(|seen| std::mem::take(&mut *seen.borrow_mut()));
    assert_eq!(
        seen,
        vec![(1, logs[0]), (2, logs[1]), (3, logs[2])],
        "each table is in place and the log still full"
    );
    s.compact()?;
    let seen = SYNCS.with(|seen| std::mem::take(&mut *seen.borrow_mut()));
    let counts: Vec<usize> = seen.iter().map(|s| s.0).collect();
    assert_eq!(counts, [4, 3, 2, 1], "the new table, then every delete");

    let mut s = Store::open(&dir.join("failing"), 1 << 20)?;
    s.dir_sync = |_| Err(std::io::Error::other("injected: sync failed"));
    s.put(b"k", b"v")?;
    let log = s.wal_len();
    assert!(matches!(s.flush(), Err(KvError::Io(_))));
    assert_eq!(
        s.wal_len(),
        log,
        "the table's name may not be durable: keep the log"
    );
    assert_eq!(s.get(b"k")?, Some(b"v".to_vec()));
    s.dir_sync = sync_dir;
    s.flush()?;
    s.put(b"j", b"w")?;
    s.flush()?;
    // the first delete works and its sync fails: no further delete may follow
    let mut deletes = 0;
    s.dir_sync = |dir| {
        let old_gone = !SsTable::path_for(dir, 1).exists();
        if old_gone {
            Err(std::io::Error::other("injected"))
        } else {
            sync_dir(dir)
        }
    };
    let failed = s.compact_with(|path| {
        deletes += 1;
        fs::remove_file(path)
    });
    assert!(matches!(failed, Err(KvError::Io(_))));
    assert_eq!(deletes, 1);
    assert_eq!(s.tables.len(), s.sst_files_on_disk()?);
    assert_eq!(s.get(b"k")?, Some(b"v".to_vec()));
    assert_eq!(s.get(b"j")?, Some(b"w".to_vec()));
    Ok(())
}

/// a table whose checksum is right but whose index offsets are wrong must be refused when it is opened
fn check_bad_index_offsets(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    let entries: Vec<Entry> = (0..40)
        .map(|i| (format!("k{i:03}").into_bytes(), Some(vec![1])))
        .collect();
    let table = SsTable::write(dir, 1, &entries, sync_dir)?;
    let good = fs::read(&table.path)?;
    // the index: count, then three times [klen 4][key 4][offset 8]
    let offset_at = |i: usize| table.data_end as usize + 4 + i * 16 + 8;
    for (entry, offset) in [
        (0, 5u64),
        (2, 1),
        (1, 0),
        (2, table.data_end),
        (2, u64::MAX),
    ] {
        let mut bad = good.clone();
        bad[offset_at(entry)..offset_at(entry) + 8].copy_from_slice(&offset.to_le_bytes());
        let crc_at = bad.len() - 8;
        let crc = crc32(&bad[..crc_at]);
        bad[crc_at..crc_at + 4].copy_from_slice(&crc.to_le_bytes());
        fs::write(&table.path, bad)?;
        assert!(
            matches!(SsTable::open(&table.path, 1), Err(KvError::Corrupt(_))),
            "index entry {entry} with offset {offset}"
        );
    }
    Ok(())
}

/// when only the automatic flush fails, the write has happened and must be reported as done
fn check_failed_auto_flush(dir: &Path) -> Result<()> {
    let mut s = Store::open(dir, 1)?; // every write triggers a flush
    fs::create_dir(dir.join("sst_000001.tmp"))?; // and the flush will fail
    s.put(b"k", b"v")?;
    assert_eq!(s.stats.failed_auto_flushes, 1);
    assert_eq!(s.get(b"k")?, Some(b"v".to_vec()));
    assert!(s.wal_len() > 0);
    assert!(s.flush().is_err(), "an explicit flush still reports it");
    fs::remove_dir(dir.join("sst_000001.tmp"))?;
    s.put(b"j", b"w")?;
    assert_eq!((s.wal_len(), s.tables.len()), (0, 1));
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let tmp = TempDir::new("c01_mini_kv")?;
    demo(&tmp.0.join("demo"))?;
    check_zero_filled_tail(&tmp.0.join("zero_tail"))?;
    check_failed_delete(&tmp.0.join("failed_delete"))?;
    check_partial_append(&tmp.0.join("partial_append"))?;
    check_reversed_scan(&tmp.0.join("reversed_scan"))?;
    check_directory_syncs(&tmp.0.join("dir_sync"))?;
    check_bad_index_offsets(&tmp.0.join("bad_index"))?;
    check_failed_auto_flush(&tmp.0.join("auto_flush"))?;

    // merge unit check: newest version wins, tombstone dropped only when asked
    let newer = vec![(b"a".to_vec(), Some(b"2".to_vec())), (b"b".to_vec(), None)];
    let older = vec![
        (b"a".to_vec(), Some(b"1".to_vec())),
        (b"b".to_vec(), Some(b"x".to_vec())),
        (b"c".to_vec(), Some(b"3".to_vec())),
    ];
    assert_eq!(
        merge_sources(vec![newer.clone(), older.clone()], true),
        vec![
            (b"a".to_vec(), Some(b"2".to_vec())),
            (b"c".to_vec(), Some(b"3".to_vec()))
        ]
    );
    assert_eq!(merge_sources(vec![newer, older], false).len(), 3);

    // a corrupted table must be detected, not silently served
    let cdir = tmp.0.join("corrupt");
    let mut s = Store::open(&cdir, 1 << 20)?;
    s.put(b"k", b"v")?;
    s.flush()?;
    let sst = s.tables[0].path.clone();
    drop(s);
    let mut bytes = fs::read(&sst)?;
    bytes[2] ^= 0xFF;
    fs::write(&sst, bytes)?;
    assert!(matches!(
        Store::open(&cdir, 1 << 20),
        Err(KvError::Corrupt(_))
    ));

    let (mut total, mut final_store) = (Tally::default(), None);
    for seed in [0xC0FFEE_u64, 0xBADC0DE, 0x5EED_1234] {
        let (store, t) = model_test(&tmp.0.join(format!("model{seed}")), seed, 7000)?;
        total.crashes += t.crashes;
        total.torn += t.torn;
        total.flushes += t.flushes;
        total.compactions += t.compactions;
        total.max_tables = total.max_tables.max(t.max_tables);
        (total.fp, total.probes) = (total.fp + t.fp, total.probes + t.probes);
        final_store = Some(store);
    }
    let mut store = final_store.expect("ran at least one seed");
    let before = store.tables.len();
    store.compact()?;
    assert!(store.tables.len() <= 1 && (before < 2 || store.tables.len() < before));
    assert!(
        total.crashes > 50 && total.torn > 10,
        "crash paths exercised: {total:?}"
    );
    assert!(
        total.flushes > 50 && total.compactions > 50 && total.max_tables >= 3,
        "{total:?}"
    );
    assert!(store.stats.bloom_skips + store.stats.table_reads > 0);
    let fp_rate = total.fp as f64 / total.probes as f64;
    assert!(
        fp_rate < 0.05,
        "bloom false-positive rate too high: {fp_rate}"
    );
    println!("model test: 3 seeds x 7000 ops OK; {total:?}");
    println!(
        "bloom false-positive rate {:.2}% (no false negatives); final compaction {before} -> {} table(s)",
        fp_rate * 100.0,
        store.tables.len()
    );
    println!("ALL OK");
    Ok(())
}
