//! CAPSTONE 11 (storage): a toy UNIX filesystem on a block device: inodes, indirect blocks, directories, a write-ahead journal, fsck, and a buffer cache.
//! LEVEL: capstone
//! Use in: understanding ext2/ext3/ext4 and xv6-style filesystems, embedded storage engines, crash-consistency testing of any on-disk format.   Run: cargo run --release --example toy_filesystem
#![allow(dead_code)]
//!
//! WHAT: a filesystem stored in ONE file that is treated as an array of 4096-byte blocks (a BLOCK DEVICE). Everything above the device
//! (cache, journal, allocators, inodes, directories, path resolution) is ordinary Rust over `read_block / write_block / sync`.
//!
//! ON-DISK LAYOUT (block numbers; the example geometry is 256 blocks, 64 inodes, 64 journal blocks):
//!
//! ```text
//!   0        1 .. 1+J          ibm        dbm .. dbm+D        itable ..            data_start .. total
//!  +--------+-----------------+----------+-------------------+--------------------+--------------------+
//!  | super  | journal         | inode    | data-block bitmap | inode table        | data blocks        |
//!  | block  | hdr|images..|cmt| bitmap   | (1 bit per block)  | (32 x 128 B/block) | files, dirs, ind.  |
//!  +--------+-----------------+----------+-------------------+--------------------+--------------------+
//!   magic, version, geometry, crc32        bit i = block i (metadata bits pre-set; next-fit allocator)
//!
//!   inode (128 B): kind, nlink, mode, size, mtime, ctime, direct[12], single-indirect, double-indirect
//!   file blocks: 12 direct + 1024 (via indirect) + 1024*1024 (via double indirect) = 1,049,612 blocks  (about 4.0 GiB)
//!   directory  : a regular file whose content is 64-byte entries [inode u32][name_len u8][name <= 59 B]; "." and ".." are real entries
//! ```
//!
//! JOURNAL (ext3 `data=journal` style, one transaction at a time): every public operation is a TRANSACTION, except that a long write is
//! cut into several (each is atomic, the whole write is not: after a crash or NoSpace a prefix of it may be there). Block writes only go to the
//! buffer cache (dirty, never evicted). COMMIT = write [header: seq, target block numbers] + [block images], sync, write [commit record:
//! seq + crc32 over header and images], sync, then CHECKPOINT the images to their home locations, sync, and finally mark the journal clean.
//! Recovery on mount: a header with a matching, checksummed commit record is replayed (idempotent: the same images are written again);
//! anything else is discarded. So a crash at any write boundary leaves the operation either entirely applied or not applied at all.
//! The checksum is what makes torn and REORDERED writes between two syncs harmless (the fault injector below simulates both).
//!
//! FSCK walks the tree from the root and checks: every entry points at an allocated inode, link counts equal the number of directory
//! entries, no block is claimed twice (cross-link), bitmaps equal the set of reachable blocks, no orphan inodes, "." and ".." are right,
//! every directory is reachable exactly once, no name is there twice. Simple problems (link counts, leaked blocks, orphans, missing bits)
//! are repaired. If some part of the tree cannot be read, repair only marks things used: it frees nothing it could not account for.
//!
//! REPAIR CAN FREE LIVE DATA: a directory whose size was shortened, or an entry that was zeroed, looks like fewer names. The walk seems
//! complete, so the files named there are taken for orphans and freed.
//!
//! WHAT THE CRASH TESTS CANNOT PROVE: with a checksummed commit record, no cut shows a need for the sync between the journal copies and
//! that record; nor for the last sync of a commit, of recovery or of format. Removing any other sync of these three makes a check fail.
//!
//! LIMITS: only the superblock and journal records are checksummed, so some damage elsewhere cannot be told from good data (a directory
//! whose kind byte says "file" hides its children, and repair would free them). Timestamps come from a counter that restarts at each
//! mount. A very large device with a small journal can make one unlink too big for a transaction.
//!
//! COMMON USES: ext2 taught the inode/indirect-block/bitmap design (that is this file's layout); ext3/ext4 added the journal (crash recovery
//! in time proportional to the log, not the disk, and the checksummed commit block of ext4's journal_checksum); FFS introduced cylinder
//! groups and locality-aware allocation (here: next-fit); xv6's file system (a few hundred lines in C) is the smallest teaching
//! implementation of inodes + log + directories; ZFS-style designs put checksums on everything and detect silent corruption (here: the
//! superblock and journal records are checksummed). The same crash-injection method tests databases and key-value stores.
//! Read: https://pages.cs.wisc.edu/~remzi/OSTEP/file-implementation.pdf , https://pdos.csail.mit.edu/6.828/2023/xv6/book-riscv-rev3.pdf
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::FileExt;
use std::path::PathBuf;

// ============================================================================================================================
// Errors, small helpers
// ============================================================================================================================

#[derive(Debug, Clone, PartialEq)]
enum FsError {
    NotFound,
    Exists,
    NotDir,
    IsDir,
    NotEmpty,
    Invalid(&'static str),
    NameTooLong,
    NoSpace,
    NoInodes,
    FileTooBig,
    TxTooBig,
    BadMagic,
    BadChecksum,
    BadGeometry(String),
    Corrupt(String),
    Io(String),
    Crashed,
    /// A commit failed on the device. What reached the disk is unknown, so only a fresh mount may go on.
    Poisoned,
}
impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for FsError {}
type R<T> = Result<T, FsError>;

const BS: usize = 4096;
type Block = [u8; BS];
fn zero_block() -> Box<Block> {
    Box::new([0u8; BS])
}
fn g16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn g32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn g64(b: &[u8], o: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[o..o + 8]);
    u64::from_le_bytes(a)
}
fn p16(b: &mut [u8], o: usize, v: u16) {
    b[o..o + 2].copy_from_slice(&v.to_le_bytes());
}
fn p32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
fn p64(b: &mut [u8], o: usize, v: u64) {
    b[o..o + 8].copy_from_slice(&v.to_le_bytes());
}

const CRC_TABLE: [u32; 256] = {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 == 1 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
};
/// CRC-32 that can be chained: `crc32_extend(crc32(a), b) == crc32(a ++ b)`.
fn crc32_extend(prev: u32, data: &[u8]) -> u32 {
    let mut c = !prev;
    for &b in data {
        c = CRC_TABLE[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8);
    }
    !c
}
fn crc32(data: &[u8]) -> u32 {
    crc32_extend(0, data)
}

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { (self.next() >> 11) % n }
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

// ============================================================================================================================
// 1. BLOCK DEVICES: file-backed (pread/pwrite), in-memory, and a fault-injecting wrapper
// ============================================================================================================================
// A block device only knows "read block n", "write block n" and "sync" (a barrier: everything written before it is durable before
// anything written after it). Writes between two syncs may be lost, torn or reordered by a real disk; FaultDevice models exactly that.

trait BlockDevice {
    fn num_blocks(&self) -> u64;
    fn read_block(&mut self, n: u64, buf: &mut Block) -> R<()>;
    fn write_block(&mut self, n: u64, buf: &Block) -> R<()>;
    fn sync(&mut self) -> R<()>;
}
/// A borrowed device, for a caller that must still have it after a call fails.
impl<D: BlockDevice> BlockDevice for &mut D {
    fn num_blocks(&self) -> u64 {
        (**self).num_blocks()
    }
    fn read_block(&mut self, n: u64, buf: &mut Block) -> R<()> {
        (**self).read_block(n, buf)
    }
    fn write_block(&mut self, n: u64, buf: &Block) -> R<()> {
        (**self).write_block(n, buf)
    }
    fn sync(&mut self) -> R<()> {
        (**self).sync()
    }
}

struct FileDevice {
    file: File,
    blocks: u64,
}
impl FileDevice {
    fn create(path: &PathBuf, blocks: u64) -> R<FileDevice> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(io_err)?;
        file.set_len(blocks * BS as u64).map_err(io_err)?;
        Ok(FileDevice { file, blocks })
    }
    fn open(path: &PathBuf) -> R<FileDevice> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(io_err)?;
        let blocks = file.metadata().map_err(io_err)?.len() / BS as u64;
        Ok(FileDevice { file, blocks })
    }
}
fn io_err(e: std::io::Error) -> FsError {
    FsError::Io(e.to_string())
}
impl BlockDevice for FileDevice {
    fn num_blocks(&self) -> u64 {
        self.blocks
    }
    fn read_block(&mut self, n: u64, buf: &mut Block) -> R<()> {
        if n >= self.blocks {
            return Err(FsError::Io(format!("read past end: block {n}")));
        }
        self.file.read_exact_at(buf, n * BS as u64).map_err(io_err)
    }
    fn write_block(&mut self, n: u64, buf: &Block) -> R<()> {
        if n >= self.blocks {
            return Err(FsError::Io(format!("write past end: block {n}")));
        }
        self.file.write_all_at(buf, n * BS as u64).map_err(io_err)
    }
    fn sync(&mut self) -> R<()> {
        self.file.sync_data().map_err(io_err)
    }
}

#[derive(Clone)]
struct MemDisk {
    data: Vec<u8>,
}
impl MemDisk {
    fn new(blocks: u64) -> MemDisk {
        MemDisk {
            data: vec![0; blocks as usize * BS],
        }
    }
    fn from_image(data: Vec<u8>) -> MemDisk {
        MemDisk { data }
    }
}
impl BlockDevice for MemDisk {
    fn num_blocks(&self) -> u64 {
        (self.data.len() / BS) as u64
    }
    fn read_block(&mut self, n: u64, buf: &mut Block) -> R<()> {
        let o = (n as usize)
            .checked_mul(BS)
            .filter(|o| o.checked_add(BS).is_some_and(|end| end <= self.data.len()))
            .ok_or_else(|| FsError::Io(format!("read past end: block {n}")))?;
        buf.copy_from_slice(&self.data[o..o + BS]);
        Ok(())
    }
    fn write_block(&mut self, n: u64, buf: &Block) -> R<()> {
        let o = (n as usize)
            .checked_mul(BS)
            .filter(|o| o.checked_add(BS).is_some_and(|end| end <= self.data.len()))
            .ok_or_else(|| FsError::Io(format!("write past end: block {n}")))?;
        self.data[o..o + BS].copy_from_slice(buf);
        Ok(())
    }
    fn sync(&mut self) -> R<()> {
        Ok(())
    }
}

/// What a crash does to writes that were issued but not yet made durable by a `sync`.
#[derive(Clone, Copy, Debug)]
enum CrashMode {
    /// every issued write reached the platter, in order (a pure "power cut between two write calls")
    Prefix,
    /// unsynced writes are shuffled, each is dropped, applied, or TORN (random 512-byte sectors only); seeded
    Chaos(u64),
}

/// Memory disk + volatile write cache. With `arm(Some(k))`, after `k` more writes the next write or sync fails with `Crashed`
/// (and every later call too).
struct FaultDevice {
    durable: MemDisk,
    pending: Vec<(u64, Box<Block>)>,
    writes: u64,
    crash_after: Option<u64>,
    crashed: bool,
}
impl FaultDevice {
    fn new(blocks: u64) -> FaultDevice {
        FaultDevice::from_image(vec![0; blocks as usize * BS])
    }
    fn from_image(img: Vec<u8>) -> FaultDevice {
        FaultDevice {
            durable: MemDisk::from_image(img),
            pending: Vec::new(),
            writes: 0,
            crash_after: None,
            crashed: false,
        }
    }
    fn arm(&mut self, k: Option<u64>) {
        self.writes = 0;
        self.crash_after = k;
    }
    /// The disk contents after a power cut, according to `mode`.
    fn post_crash_image(&self, mode: CrashMode) -> Vec<u8> {
        let mut img = self.durable.data.clone();
        match mode {
            CrashMode::Prefix => {
                for (n, b) in &self.pending {
                    img[*n as usize * BS..(*n as usize + 1) * BS].copy_from_slice(&b[..]);
                }
            }
            CrashMode::Chaos(seed) => {
                let mut rng = Rng::new(seed ^ (self.writes << 20) ^ self.pending.len() as u64);
                let mut order: Vec<usize> = (0..self.pending.len()).collect();
                for i in (1..order.len()).rev() {
                    order.swap(i, rng.below(i as u64 + 1) as usize);
                }
                for i in order {
                    let (n, b) = &self.pending[i];
                    let base = *n as usize * BS;
                    match rng.below(10) {
                        0..=3 => {}
                        4..=7 => img[base..base + BS].copy_from_slice(&b[..]),
                        _ => {
                            for s in 0..BS / 512 {
                                if rng.chance(50) {
                                    img[base + s * 512..base + (s + 1) * 512]
                                        .copy_from_slice(&b[s * 512..(s + 1) * 512]);
                                }
                            }
                        }
                    }
                }
            }
        }
        img
    }
    /// everything issued so far, as if no crash happened
    fn snapshot(&self) -> Vec<u8> {
        self.post_crash_image(CrashMode::Prefix)
    }
}
impl BlockDevice for FaultDevice {
    fn num_blocks(&self) -> u64 {
        self.durable.num_blocks()
    }
    fn read_block(&mut self, n: u64, buf: &mut Block) -> R<()> {
        if self.crashed {
            return Err(FsError::Crashed);
        }
        if let Some((_, b)) = self.pending.iter().rev().find(|(m, _)| *m == n) {
            buf.copy_from_slice(&b[..]);
            return Ok(());
        }
        self.durable.read_block(n, buf)
    }
    fn write_block(&mut self, n: u64, buf: &Block) -> R<()> {
        if self.crashed || self.crash_after.is_some_and(|k| self.writes >= k) {
            self.crashed = true;
            return Err(FsError::Crashed);
        }
        if n >= self.num_blocks() {
            return Err(FsError::Io(format!("write past end: block {n}")));
        }
        self.writes += 1;
        self.pending.push((n, Box::new(*buf)));
        Ok(())
    }
    fn sync(&mut self) -> R<()> {
        // the cut can also come just before a sync: only then is a whole group of writes still in doubt
        if self.crashed || self.crash_after.is_some_and(|k| self.writes >= k) {
            self.crashed = true;
            return Err(FsError::Crashed);
        }
        for (n, b) in self.pending.drain(..) {
            self.durable.write_block(n, &b)?;
        }
        Ok(())
    }
}

// ============================================================================================================================
// 5. BUFFER CACHE: LRU, write-back with dirty tracking. Dirty blocks belong to the open transaction and are never evicted (no-steal).
// ============================================================================================================================

struct CEntry {
    data: Box<Block>,
    dirty: bool,
    stamp: u64,
}
struct Cache<D> {
    dev: D,
    cap: usize,
    map: HashMap<u64, CEntry>,
    tick: u64,
    hits: u64,
    misses: u64,
    writebacks: u64,
}
impl<D: BlockDevice> Cache<D> {
    fn new(dev: D, cap: usize) -> Self {
        Cache {
            dev,
            cap,
            map: HashMap::new(),
            tick: 0,
            hits: 0,
            misses: 0,
            writebacks: 0,
        }
    }
    fn read(&mut self, n: u64) -> R<Box<Block>> {
        self.tick += 1;
        if let Some(e) = self.map.get_mut(&n) {
            e.stamp = self.tick;
            self.hits += 1;
            return Ok(e.data.clone());
        }
        self.misses += 1;
        let mut b = zero_block();
        self.dev.read_block(n, &mut b)?;
        self.map.insert(
            n,
            CEntry {
                data: b.clone(),
                dirty: false,
                stamp: self.tick,
            },
        );
        self.evict();
        Ok(b)
    }
    fn write(&mut self, n: u64, b: &Block) {
        self.tick += 1;
        self.map.insert(
            n,
            CEntry {
                data: Box::new(*b),
                dirty: true,
                stamp: self.tick,
            },
        );
        self.evict();
    }
    fn evict(&mut self) {
        while self.map.len() > self.cap {
            let victim = self
                .map
                .iter()
                .filter(|(_, e)| !e.dirty)
                .min_by_key(|(_, e)| e.stamp)
                .map(|(k, _)| *k);
            match victim {
                Some(k) => {
                    self.map.remove(&k);
                }
                None => break, // everything is dirty: the cache temporarily exceeds its capacity
            }
        }
    }
    fn dirty_list(&self) -> Vec<(u64, Box<Block>)> {
        let mut v: Vec<_> = self
            .map
            .iter()
            .filter(|(_, e)| e.dirty)
            .map(|(k, e)| (*k, e.data.clone()))
            .collect();
        v.sort_by_key(|(k, _)| *k);
        v
    }
    /// write-back: push every dirty block to the device (home location) and mark it clean
    fn flush_dirty(&mut self) -> R<()> {
        let mut keys: Vec<u64> = self
            .map
            .iter()
            .filter(|(_, e)| e.dirty)
            .map(|(k, _)| *k)
            .collect();
        keys.sort();
        for k in keys {
            let e = self.map.get_mut(&k).unwrap();
            self.dev.write_block(k, &e.data)?;
            e.dirty = false;
            self.writebacks += 1;
        }
        self.evict();
        Ok(())
    }
    fn discard_dirty(&mut self) {
        self.map.retain(|_, e| !e.dirty);
    }
}

// ============================================================================================================================
// 2. ON-DISK LAYOUT: superblock, inodes, geometry
// ============================================================================================================================

const MAGIC: u64 = 0x5459_4F46_5330_3031; // "TYOFS001"
const VERSION: u32 = 1;
const ND: usize = 12; // direct pointers
const PPB: u64 = (BS / 4) as u64; // pointers per indirect block = 1024
const MAX_BLOCKS: u64 = ND as u64 + PPB + PPB * PPB;
const MAX_FILE_SIZE: u64 = MAX_BLOCKS * BS as u64;
const INODE_SIZE: usize = 128;
const INODES_PER_BLOCK: u64 = (BS / INODE_SIZE) as u64;
const BITS: u64 = (BS * 8) as u64;
const DIRENT: usize = 64;
const MAX_NAME: usize = DIRENT - 5;
const ROOT: u32 = 1;
const K_FREE: u8 = 0;
const K_FILE: u8 = 1;
const K_DIR: u8 = 2;
const CHUNK_BLOCKS: u64 = 16; // one transaction writes into at most this many data blocks of a file
/// A transaction may also hold pointer blocks, bitmap blocks and an inode block: this many beside the data.
const TX_OVERHEAD: u64 = 8;
const MIN_JOURNAL: u32 = TX_OVERHEAD as u32 + 4; // header + commit record + overhead + two data blocks

#[derive(Clone, Copy, Debug)]
struct Geometry {
    total_blocks: u64,
    inode_count: u32,
    journal_blocks: u32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Super {
    total_blocks: u64,
    inode_count: u32,
    journal_blocks: u32,
    journal_start: u64,
    ibm_start: u64,
    dbm_start: u64,
    itable_start: u64,
    data_start: u64,
}
impl Super {
    fn layout(g: &Geometry) -> R<Super> {
        if g.inode_count == 0 || g.inode_count as u64 + 1 > BITS {
            return Err(FsError::BadGeometry("inode count must be 1..32767".into()));
        }
        if g.journal_blocks < MIN_JOURNAL || g.journal_blocks > 1000 {
            return Err(FsError::BadGeometry(
                "journal must be 12..1000 blocks".into(),
            ));
        }
        let journal_start = 1;
        let ibm_start = journal_start + g.journal_blocks as u64;
        let dbm_start = ibm_start + 1;
        let dbm_blocks = g.total_blocks.div_ceil(BITS).max(1);
        let itable_start = dbm_start + dbm_blocks;
        let itable_blocks = (g.inode_count as u64 + 1).div_ceil(INODES_PER_BLOCK);
        let data_start = itable_start + itable_blocks;
        if g.total_blocks < data_start + 8 || g.total_blocks > u32::MAX as u64 {
            return Err(FsError::BadGeometry(format!(
                "device too small: {} blocks, metadata needs {}",
                g.total_blocks,
                data_start + 8
            )));
        }
        Ok(Super {
            total_blocks: g.total_blocks,
            inode_count: g.inode_count,
            journal_blocks: g.journal_blocks,
            journal_start,
            ibm_start,
            dbm_start,
            itable_start,
            data_start,
        })
    }
    fn encode(&self) -> Box<Block> {
        let mut b = zero_block();
        p64(&mut b[..], 0, MAGIC);
        p32(&mut b[..], 8, VERSION);
        p32(&mut b[..], 12, BS as u32);
        p64(&mut b[..], 16, self.total_blocks);
        p32(&mut b[..], 24, self.inode_count);
        p32(&mut b[..], 28, self.journal_blocks);
        p64(&mut b[..], 32, self.journal_start);
        p64(&mut b[..], 40, self.ibm_start);
        p64(&mut b[..], 48, self.dbm_start);
        p64(&mut b[..], 56, self.itable_start);
        p64(&mut b[..], 64, self.data_start);
        let c = crc32(&b[..72]);
        p32(&mut b[..], 72, c);
        b
    }
    fn decode(b: &Block) -> R<Super> {
        if g64(b, 0) != MAGIC {
            return Err(FsError::BadMagic);
        }
        if g32(b, 72) != crc32(&b[..72]) {
            return Err(FsError::BadChecksum);
        }
        if g32(b, 8) != VERSION || g32(b, 12) != BS as u32 {
            return Err(FsError::BadGeometry(
                "unsupported version or block size".into(),
            ));
        }
        let g = Geometry {
            total_blocks: g64(b, 16),
            inode_count: g32(b, 24),
            journal_blocks: g32(b, 28),
        };
        let sb = Super::layout(&g)?;
        if sb.encode()[..72] != b[..72] {
            return Err(FsError::BadGeometry(
                "stored layout disagrees with geometry".into(),
            ));
        }
        Ok(sb)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Inode {
    kind: u8,
    nlink: u16,
    mode: u16,
    size: u64,
    mtime: u64,
    ctime: u64,
    direct: [u32; ND],
    ind: u32,
    dind: u32,
}
impl Inode {
    fn encode(&self, out: &mut [u8]) {
        out[..INODE_SIZE].fill(0);
        out[0] = self.kind;
        p16(out, 2, self.nlink);
        p16(out, 4, self.mode);
        p64(out, 8, self.size);
        p64(out, 16, self.mtime);
        p64(out, 24, self.ctime);
        for (i, d) in self.direct.iter().enumerate() {
            p32(out, 32 + 4 * i, *d);
        }
        p32(out, 80, self.ind);
        p32(out, 84, self.dind);
    }
    fn decode(b: &[u8]) -> Inode {
        let mut direct = [0u32; ND];
        for (i, d) in direct.iter_mut().enumerate() {
            *d = g32(b, 32 + 4 * i);
        }
        Inode {
            kind: b[0],
            nlink: g16(b, 2),
            mode: g16(b, 4),
            size: g64(b, 8),
            mtime: g64(b, 16),
            ctime: g64(b, 24),
            direct,
            ind: g32(b, 80),
            dind: g32(b, 84),
        }
    }
}

// ----- journal record layout (inside the journal region) -----
const JMAGIC: u32 = 0x4A52_4E4C;
const CMAGIC: u32 = 0x434D_4954;

trait Clock {
    fn now(&mut self) -> u64;
}
/// deterministic injected clock: every call is one tick
struct TickClock(u64);
impl Clock for TickClock {
    fn now(&mut self) -> u64 {
        self.0 += 1;
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    File,
    Dir,
}
#[derive(Clone, Debug)]
struct Stat {
    ino: u32,
    kind: Kind,
    size: u64,
    nlink: u16,
    blocks: usize,
    mtime: u64,
    ctime: u64,
}
#[derive(Clone, Debug, PartialEq)]
struct Node {
    kind: Kind,
    data: Vec<u8>,
    nlink: u16,
}

#[derive(Default, Clone, Debug)]
struct FsStats {
    commits: u64,
    journal_images: u64,
    replayed: u64,
}

struct Fs<D: BlockDevice> {
    cache: Cache<D>,
    sb: Super,
    clock: Box<dyn Clock>,
    jseq: u64,
    now: u64,
    in_tx: bool,
    poisoned: bool,
    ino_cursor: u64,
    blk_cursor: u64,
    stats: FsStats,
}

fn parse_dir(raw: &[u8]) -> R<Vec<(usize, u32, String)>> {
    if raw.len() % DIRENT != 0 {
        return Err(FsError::Corrupt(
            "directory size is not a multiple of the entry size".into(),
        ));
    }
    let mut v = Vec::new();
    for (i, e) in raw.chunks(DIRENT).enumerate() {
        let ino = g32(e, 0);
        if ino == 0 {
            continue;
        }
        let n = e[4] as usize;
        if n == 0 || n > MAX_NAME {
            return Err(FsError::Corrupt("bad directory entry name length".into()));
        }
        let name = std::str::from_utf8(&e[5..5 + n])
            .map_err(|_| FsError::Corrupt("directory name is not UTF-8".into()))?;
        if name.contains(['/', '\0']) {
            return Err(FsError::Corrupt(
                "directory name holds a slash or a NUL".into(),
            ));
        }
        v.push((i * DIRENT, ino, name.to_string()));
    }
    Ok(v)
}
fn encode_entry(ino: u32, name: &str) -> [u8; DIRENT] {
    let mut e = [0u8; DIRENT];
    p32(&mut e, 0, ino);
    e[4] = name.len() as u8;
    e[5..5 + name.len()].copy_from_slice(name.as_bytes());
    e
}
fn check_name(name: &str) -> R<()> {
    if name.is_empty() || name == "." || name == ".." || name.contains('\0') {
        return Err(FsError::Invalid("bad file name"));
    }
    if name.len() > MAX_NAME {
        return Err(FsError::NameTooLong);
    }
    Ok(())
}

impl<D: BlockDevice> Fs<D> {
    // ---------------------------------------------------------------------------------------------------------------------
    // format / mount / journal recovery
    // ---------------------------------------------------------------------------------------------------------------------
    fn format(mut dev: D, g: &Geometry) -> R<Fs<D>> {
        let sb = Super::layout(g)?;
        if dev.num_blocks() < sb.total_blocks {
            return Err(FsError::BadGeometry(
                "device smaller than the requested filesystem".into(),
            ));
        }
        let z = zero_block();
        // Take the old superblock away first, and make that durable. From here until the new one is written, nothing mounts:
        // a half-made filesystem can never be read with the old one's journal or bitmaps.
        dev.write_block(0, &z)?;
        dev.sync()?;
        for n in 1..sb.data_start {
            dev.write_block(n, &z)?;
        }
        let mut jh = zero_block();
        p32(&mut jh[..], 0, JMAGIC);
        dev.write_block(sb.journal_start, &jh)?;
        let mut ibm = zero_block();
        ibm[0] |= 1; // inode 0 is never valid
        dev.write_block(sb.ibm_start, &ibm)?;
        let dbm_blocks = sb.itable_start - sb.dbm_start;
        for bi in 0..dbm_blocks {
            let mut b = zero_block();
            for bit in (bi * BITS)..((bi + 1) * BITS).min(sb.data_start) {
                b[((bit % BITS) / 8) as usize] |= 1 << (bit % 8);
            }
            dev.write_block(sb.dbm_start + bi, &b)?;
        }
        dev.sync()?;
        dev.write_block(0, &sb.encode())?;
        dev.sync()?;
        let mut fs = Fs::mount_inner(dev, Box::new(TickClock(0)), false)?;
        fs.tx(|fs| {
            let ino = fs.alloc_inode(K_DIR, 2)?;
            assert_eq!(ino, ROOT);
            fs.init_dir(ino, ino)
        })?;
        Ok(fs)
    }

    fn mount(dev: D) -> R<Fs<D>> {
        Fs::mount_with_clock(dev, Box::new(TickClock(0)))
    }

    /// Reads the superblock and replays/discards the journal (writes to the device only if a valid transaction is found).
    fn replay_journal(dev: &mut D) -> R<(Super, u64, u64)> {
        if dev.num_blocks() == 0 {
            return Err(FsError::BadMagic);
        }
        let mut b = zero_block();
        dev.read_block(0, &mut b)?;
        let sb = Super::decode(&b)?;
        if dev.num_blocks() < sb.total_blocks {
            return Err(FsError::BadGeometry(
                "device is smaller than the superblock claims".into(),
            ));
        }
        let j = sb.journal_start;
        let cap = sb.journal_blocks as u64 - 2;
        let mut h = zero_block();
        dev.read_block(j, &mut h)?;
        if g32(&h[..], 0) != JMAGIC {
            return Ok((sb, 0, 0)); // never written (or garbage): treat as an empty journal
        }
        let seq = g64(&h[..], 8);
        let count = g32(&h[..], 16) as u64;
        if count == 0 {
            return Ok((sb, seq, 0));
        }
        let mut replayed = 0;
        if count <= cap {
            let mut c = zero_block();
            dev.read_block(j + 1 + count, &mut c)?;
            let mut crc = crc32(&h[..]);
            let mut images = Vec::new();
            for i in 0..count {
                let mut img = zero_block();
                dev.read_block(j + 1 + i, &mut img)?;
                crc = crc32_extend(crc, &img[..]);
                images.push(img);
            }
            let valid = g32(&c[..], 0) == CMAGIC && g64(&c[..], 8) == seq && g32(&c[..], 16) == crc;
            let targets: Vec<u64> = (0..count as usize)
                .map(|i| g32(&h[..], 32 + 4 * i) as u64)
                .collect();
            if valid
                && targets
                    .iter()
                    .all(|t| *t >= j + sb.journal_blocks as u64 && *t < sb.total_blocks)
            {
                for (t, img) in targets.iter().zip(images.iter()) {
                    dev.write_block(*t, img)?;
                }
                dev.sync()?;
                replayed = 1;
            }
        }
        // mark the journal clean but remember the sequence number (a stale commit record can never match a later seq)
        let mut clean = zero_block();
        p32(&mut clean[..], 0, JMAGIC);
        p64(&mut clean[..], 8, seq);
        dev.write_block(j, &clean)?;
        dev.sync()?;
        Ok((sb, seq, replayed))
    }

    fn mount_with_clock(dev: D, clock: Box<dyn Clock>) -> R<Fs<D>> {
        Fs::mount_inner(dev, clock, true)
    }

    fn mount_inner(mut dev: D, clock: Box<dyn Clock>, check_root: bool) -> R<Fs<D>> {
        let (sb, seq, replayed) = Fs::replay_journal(&mut dev)?;
        let mut fs = Fs {
            cache: Cache::new(dev, 128),
            sb,
            clock,
            jseq: seq,
            now: 0,
            in_tx: false,
            poisoned: false,
            ino_cursor: 1,
            blk_cursor: sb.data_start,
            stats: FsStats {
                replayed,
                ..Default::default()
            },
        };
        if !check_root {
            return Ok(fs);
        }
        let root = fs.load_inode(ROOT)?;
        if root.kind != K_DIR || !fs.bit_get(sb.ibm_start, ROOT as u64)? {
            return Err(FsError::Corrupt("root inode is not a directory".into()));
        }
        Ok(fs)
    }

    fn into_device(self) -> D {
        self.cache.dev
    }

    // ---------------------------------------------------------------------------------------------------------------------
    // 4. transactions + journal commit
    // ---------------------------------------------------------------------------------------------------------------------
    fn tx<T>(&mut self, f: impl FnOnce(&mut Self) -> R<T>) -> R<T> {
        if self.poisoned {
            return Err(FsError::Poisoned);
        }
        if self.in_tx {
            return Err(FsError::Invalid("nested transaction"));
        }
        self.in_tx = true;
        self.now = self.clock.now();
        let r = f(self);
        self.in_tx = false;
        match r {
            Ok(v) => {
                self.commit()?;
                Ok(v)
            }
            Err(e) => {
                self.cache.discard_dirty(); // abort: nothing reached the device
                Err(e)
            }
        }
    }

    fn commit(&mut self) -> R<()> {
        let dirty = self.cache.dirty_list();
        if dirty.is_empty() {
            return Ok(());
        }
        let cap = self.sb.journal_blocks as usize - 2;
        if dirty.len() > cap {
            self.cache.discard_dirty();
            return Err(FsError::TxTooBig);
        }
        let n = dirty.len() as u64;
        // a header may hold any number, even u64::MAX: only equality with the commit record matters
        let seq = self.jseq.wrapping_add(1);
        if let Err(e) = self.write_through_journal(seq, &dirty) {
            // Some of the writes may be on the disk and some not, and a failed sync may have lost writes that the cache
            // already counts as clean. Going on could overwrite a committed transaction in the journal before it is home.
            self.poisoned = true;
            self.cache.discard_dirty();
            return Err(e);
        }
        self.jseq = seq;
        self.stats.commits += 1;
        self.stats.journal_images += n;
        Ok(())
    }

    fn write_through_journal(&mut self, seq: u64, dirty: &[(u64, Box<Block>)]) -> R<()> {
        let j = self.sb.journal_start;
        let n = dirty.len() as u64;
        let mut h = zero_block();
        p32(&mut h[..], 0, JMAGIC);
        p64(&mut h[..], 8, seq);
        p32(&mut h[..], 16, n as u32);
        for (i, (t, _)) in dirty.iter().enumerate() {
            p32(&mut h[..], 32 + 4 * i, *t as u32);
        }
        let mut crc = crc32(&h[..]);
        for (_, img) in dirty {
            crc = crc32_extend(crc, &img[..]);
        }
        let dev = &mut self.cache.dev;
        dev.write_block(j, &h)?;
        for (i, (_, img)) in dirty.iter().enumerate() {
            dev.write_block(j + 1 + i as u64, img)?;
        }
        dev.sync()?; // barrier 1: header + images are durable before the commit record exists
        let mut c = zero_block();
        p32(&mut c[..], 0, CMAGIC);
        p64(&mut c[..], 8, seq);
        p32(&mut c[..], 16, crc);
        dev.write_block(j + 1 + n, &c)?;
        dev.sync()?; // barrier 2: the transaction is now COMMITTED (it will be replayed after a crash)
        self.cache.flush_dirty()?; // checkpoint: write-back to the home locations
        self.cache.dev.sync()?;
        let mut clean = zero_block();
        p32(&mut clean[..], 0, JMAGIC);
        p64(&mut clean[..], 8, seq);
        self.cache.dev.write_block(j, &clean)?;
        self.cache.dev.sync()?;
        Ok(())
    }

    // ---------------------------------------------------------------------------------------------------------------------
    // block access, bitmaps (next-fit allocator), inodes
    // ---------------------------------------------------------------------------------------------------------------------
    fn rd(&mut self, n: u64) -> R<Box<Block>> {
        if self.poisoned {
            return Err(FsError::Poisoned);
        }
        if n >= self.sb.total_blocks {
            return Err(FsError::Corrupt(format!("block {n} out of range")));
        }
        self.cache.read(n)
    }
    fn wr(&mut self, n: u64, b: &Block) {
        self.cache.write(n, b);
    }
    fn bit_get(&mut self, start: u64, i: u64) -> R<bool> {
        let b = self.rd(start + i / BITS)?;
        let o = (i % BITS) as usize;
        Ok(b[o / 8] >> (o % 8) & 1 == 1)
    }
    fn bit_put(&mut self, start: u64, i: u64, v: bool) -> R<()> {
        let mut b = self.rd(start + i / BITS)?;
        let o = (i % BITS) as usize;
        if v {
            b[o / 8] |= 1 << (o % 8);
        } else {
            b[o / 8] &= !(1 << (o % 8));
        }
        self.wr(start + i / BITS, &b);
        Ok(())
    }
    /// NEXT-FIT: start scanning at `from` (where the last allocation ended) and wrap once; skips full bytes quickly.
    fn alloc_bit(&mut self, start: u64, nbits: u64, from: u64) -> R<Option<u64>> {
        let mut i = from % nbits;
        let mut remaining = nbits;
        while remaining > 0 {
            let bi = i / BITS;
            let end = ((bi + 1) * BITS).min(nbits);
            let mut blk = self.rd(start + bi)?;
            let mut j = i;
            while j < end {
                let o = (j % BITS) as usize;
                if o % 8 == 0 && blk[o / 8] == 0xFF && j + 8 <= end {
                    j += 8;
                    continue;
                }
                if blk[o / 8] >> (o % 8) & 1 == 0 {
                    blk[o / 8] |= 1 << (o % 8);
                    self.wr(start + bi, &blk);
                    return Ok(Some(j));
                }
                j += 1;
            }
            remaining = remaining.saturating_sub(end - i);
            i = if end >= nbits { 0 } else { end };
        }
        Ok(None)
    }
    fn check_data_blk(&self, b: u32) -> R<u32> {
        if (b as u64) < self.sb.data_start || b as u64 >= self.sb.total_blocks {
            return Err(FsError::Corrupt(format!(
                "pointer to block {b} outside the data area"
            )));
        }
        Ok(b)
    }
    fn alloc_block(&mut self) -> R<u32> {
        let b = self
            .alloc_bit(self.sb.dbm_start, self.sb.total_blocks, self.blk_cursor)?
            .ok_or(FsError::NoSpace)?;
        self.blk_cursor = b + 1;
        self.wr(b, &zero_block()); // never expose stale bytes; costs nothing extra in the journal (one image per block)
        Ok(self.check_data_blk(b as u32)?)
    }
    fn free_block(&mut self, b: u32) -> R<()> {
        self.check_data_blk(b)?;
        if !self.bit_get(self.sb.dbm_start, b as u64)? {
            return Err(FsError::Corrupt(format!("double free of block {b}")));
        }
        self.bit_put(self.sb.dbm_start, b as u64, false)
    }
    fn free_blocks(&mut self) -> R<u64> {
        let mut used = 0;
        for bi in 0..(self.sb.itable_start - self.sb.dbm_start) {
            let b = self.rd(self.sb.dbm_start + bi)?;
            let hi = ((bi + 1) * BITS).min(self.sb.total_blocks) - bi * BITS;
            used += (0..hi as usize)
                .filter(|o| b[o / 8] >> (o % 8) & 1 == 1)
                .count() as u64;
        }
        Ok(self.sb.total_blocks - used)
    }

    fn inode_pos(&self, ino: u32) -> R<(u64, usize)> {
        if ino == 0 || ino > self.sb.inode_count {
            return Err(FsError::Corrupt(format!("inode number {ino} out of range")));
        }
        Ok((
            self.sb.itable_start + ino as u64 / INODES_PER_BLOCK,
            (ino as u64 % INODES_PER_BLOCK) as usize * INODE_SIZE,
        ))
    }
    fn load_inode(&mut self, ino: u32) -> R<Inode> {
        let (blk, off) = self.inode_pos(ino)?;
        let b = self.rd(blk)?;
        let i = Inode::decode(&b[off..off + INODE_SIZE]);
        if i.kind > K_DIR || i.size > MAX_FILE_SIZE {
            return Err(FsError::Corrupt(format!("inode {ino} is damaged")));
        }
        Ok(i)
    }
    fn store_inode(&mut self, ino: u32, i: &Inode) -> R<()> {
        let (blk, off) = self.inode_pos(ino)?;
        let mut b = self.rd(blk)?;
        i.encode(&mut b[off..off + INODE_SIZE]);
        self.wr(blk, &b);
        Ok(())
    }
    fn alloc_inode(&mut self, kind: u8, nlink: u16) -> R<u32> {
        let n = self.sb.inode_count as u64 + 1;
        let ino = self
            .alloc_bit(self.sb.ibm_start, n, self.ino_cursor)?
            .ok_or(FsError::NoInodes)?;
        self.ino_cursor = ino + 1;
        let i = Inode {
            kind,
            nlink,
            mode: if kind == K_DIR { 0o755 } else { 0o644 },
            mtime: self.now,
            ctime: self.now,
            ..Default::default()
        };
        self.store_inode(ino as u32, &i)?;
        Ok(ino as u32)
    }
    fn free_inode(&mut self, ino: u32) -> R<()> {
        self.store_inode(ino, &Inode::default())?;
        self.bit_put(self.sb.ibm_start, ino as u64, false)
    }
    fn adjust_nlink(&mut self, ino: u32, delta: i32) -> R<()> {
        let mut i = self.load_inode(ino)?;
        let n = i.nlink as i32 + delta;
        if !(0..=u16::MAX as i32).contains(&n) {
            return Err(FsError::Invalid("link count out of range"));
        }
        i.nlink = n as u16;
        i.ctime = self.now;
        self.store_inode(ino, &i)
    }

    // ---------------------------------------------------------------------------------------------------------------------
    // file block mapping: 12 direct, 1 single-indirect, 1 double-indirect
    // ---------------------------------------------------------------------------------------------------------------------
    fn ptr_slot(&mut self, blk: u32, idx: u64, alloc: bool) -> R<u32> {
        let mut b = self.rd(self.check_data_blk(blk)? as u64)?;
        let p = g32(&b[..], idx as usize * 4);
        if p != 0 {
            return self.check_data_blk(p);
        }
        if !alloc {
            return Ok(0);
        }
        let n = self.alloc_block()?;
        p32(&mut b[..], idx as usize * 4, n);
        self.wr(blk as u64, &b);
        Ok(n)
    }
    /// logical block -> disk block (0 = hole). With `alloc`, missing blocks (and pointer blocks) are allocated and zeroed.
    fn bmap(&mut self, ino: &mut Inode, lbn: u64, alloc: bool) -> R<u32> {
        if lbn < ND as u64 {
            let p = ino.direct[lbn as usize];
            if p == 0 && alloc {
                ino.direct[lbn as usize] = self.alloc_block()?;
            }
            return if ino.direct[lbn as usize] == 0 {
                Ok(0)
            } else {
                self.check_data_blk(ino.direct[lbn as usize])
            };
        }
        let l = lbn - ND as u64;
        if l < PPB {
            if ino.ind == 0 {
                if !alloc {
                    return Ok(0);
                }
                ino.ind = self.alloc_block()?;
            }
            return self.ptr_slot(ino.ind, l, alloc);
        }
        let l = l - PPB;
        if l < PPB * PPB {
            if ino.dind == 0 {
                if !alloc {
                    return Ok(0);
                }
                ino.dind = self.alloc_block()?;
            }
            let mid = self.ptr_slot(ino.dind, l / PPB, alloc)?;
            if mid == 0 {
                return Ok(0);
            }
            return self.ptr_slot(mid, l % PPB, alloc);
        }
        Err(FsError::FileTooBig)
    }
    fn collect(&mut self, blk: u32, depth: u32, out: &mut Vec<u32>) -> R<()> {
        self.check_data_blk(blk)?;
        // a sound tree cannot own more blocks than the disk has; pointer blocks that repeat each other could name millions
        if out.len() as u64 >= self.sb.total_blocks {
            return Err(FsError::Corrupt(
                "a file owns more blocks than the disk has".into(),
            ));
        }
        out.push(blk);
        if depth == 0 {
            return Ok(());
        }
        let b = self.rd(blk as u64)?;
        for i in 0..PPB as usize {
            let p = g32(&b[..], i * 4);
            if p != 0 {
                self.collect(p, depth - 1, out)?;
            }
        }
        Ok(())
    }
    /// every block owned by the inode: data blocks plus pointer blocks
    fn file_blocks(&mut self, ino: &Inode) -> R<Vec<u32>> {
        let mut out = Vec::new();
        for d in ino.direct.iter().filter(|d| **d != 0) {
            self.collect(*d, 0, &mut out)?;
        }
        if ino.ind != 0 {
            self.collect(ino.ind, 1, &mut out)?;
        }
        if ino.dind != 0 {
            self.collect(ino.dind, 2, &mut out)?;
        }
        Ok(out)
    }
    /// free everything at logical block >= keep inside a subtree; returns the (possibly cleared) pointer
    fn free_range(&mut self, blk: u32, depth: u32, keep: u64) -> R<u32> {
        if depth == 0 {
            if keep == 0 {
                self.free_block(blk)?;
                return Ok(0);
            }
            return Ok(blk);
        }
        let mut b = self.rd(self.check_data_blk(blk)? as u64)?;
        let span = PPB.pow(depth - 1);
        let mut changed = false;
        for i in 0..PPB {
            let p = g32(&b[..], i as usize * 4);
            if p == 0 {
                continue;
            }
            let start = i * span;
            if start >= keep {
                self.free_range(p, depth - 1, 0)?;
                p32(&mut b[..], i as usize * 4, 0);
                changed = true;
            } else if start + span > keep {
                let np = self.free_range(p, depth - 1, keep - start)?;
                if np != p {
                    p32(&mut b[..], i as usize * 4, np);
                    changed = true;
                }
            }
        }
        if keep == 0 {
            self.free_block(blk)?;
            return Ok(0);
        }
        if changed {
            self.wr(blk as u64, &b);
        }
        Ok(blk)
    }
    /// release all blocks at logical index >= keep (truncate / unlink)
    fn free_from(&mut self, ino: &mut Inode, keep: u64) -> R<()> {
        for i in 0..ND {
            if i as u64 >= keep && ino.direct[i] != 0 {
                self.free_block(ino.direct[i])?;
                ino.direct[i] = 0;
            }
        }
        if ino.ind != 0 {
            ino.ind = self.free_range(ino.ind, 1, keep.saturating_sub(ND as u64))?;
        }
        if ino.dind != 0 {
            ino.dind = self.free_range(ino.dind, 2, keep.saturating_sub(ND as u64 + PPB))?;
        }
        Ok(())
    }

    // ---------------------------------------------------------------------------------------------------------------------
    // file data: block-granular read-modify-write; holes read as zeros and are never allocated by reads
    // ---------------------------------------------------------------------------------------------------------------------
    fn read_data(&mut self, ino: &Inode, off: u64, len: usize) -> R<Vec<u8>> {
        if off >= ino.size {
            return Ok(Vec::new());
        }
        let n = (len as u64).min(ino.size - off) as usize;
        let mut out = vec![0u8; n];
        let mut tmp = ino.clone();
        let mut pos = 0;
        while pos < n {
            let abs = off + pos as u64;
            let (lbn, bo) = (abs / BS as u64, (abs % BS as u64) as usize);
            let take = (BS - bo).min(n - pos);
            let blk = self.bmap(&mut tmp, lbn, false)?;
            if blk != 0 {
                let b = self.rd(blk as u64)?;
                out[pos..pos + take].copy_from_slice(&b[bo..bo + take]);
            }
            pos += take;
        }
        Ok(out)
    }
    fn write_data(&mut self, num: u32, ino: &mut Inode, off: u64, data: &[u8]) -> R<()> {
        let end = off
            .checked_add(data.len() as u64)
            .ok_or(FsError::FileTooBig)?;
        if end > MAX_FILE_SIZE {
            return Err(FsError::FileTooBig);
        }
        let mut pos = 0;
        while pos < data.len() {
            let abs = off + pos as u64;
            let (lbn, bo) = (abs / BS as u64, (abs % BS as u64) as usize);
            let take = (BS - bo).min(data.len() - pos);
            let blk = self.bmap(ino, lbn, true)? as u64;
            if take == BS {
                self.wr(blk, <&Block>::try_from(&data[pos..pos + BS]).unwrap());
            } else {
                let mut b = self.rd(blk)?; // read-modify-write of a partial block
                b[bo..bo + take].copy_from_slice(&data[pos..pos + take]);
                self.wr(blk, &b);
            }
            pos += take;
        }
        ino.size = ino.size.max(end);
        ino.mtime = self.now;
        ino.ctime = self.now;
        self.store_inode(num, ino)
    }

    // ---------------------------------------------------------------------------------------------------------------------
    // directories: files of 64-byte entries
    // ---------------------------------------------------------------------------------------------------------------------
    fn dir_raw(&mut self, dir: &Inode) -> R<Vec<u8>> {
        if dir.kind != K_DIR {
            return Err(FsError::NotDir);
        }
        // a directory has no holes, so it cannot be larger than the data area: refuse before allocating its size
        if dir.size > (self.sb.total_blocks - self.sb.data_start) * BS as u64 {
            return Err(FsError::Corrupt("directory larger than the disk".into()));
        }
        self.read_data(dir, 0, dir.size as usize)
    }
    fn dir_entries(&mut self, dir: u32) -> R<Vec<(usize, u32, String)>> {
        let i = self.load_inode(dir)?;
        let raw = self.dir_raw(&i)?;
        parse_dir(&raw)
    }
    fn lookup(&mut self, dir: u32, name: &str) -> R<Option<u32>> {
        Ok(self
            .dir_entries(dir)?
            .into_iter()
            .find(|(_, _, n)| n == name)
            .map(|(_, i, _)| i))
    }
    fn add_entry(&mut self, dir: u32, name: &str, ino: u32) -> R<()> {
        check_name(name)?;
        let mut d = self.load_inode(dir)?;
        let raw = self.dir_raw(&d)?;
        let slot = raw
            .chunks(DIRENT)
            .position(|e| g32(e, 0) == 0)
            .map(|s| s * DIRENT)
            .unwrap_or(raw.len());
        self.write_data(dir, &mut d, slot as u64, &encode_entry(ino, name))
    }
    fn remove_entry(&mut self, dir: u32, name: &str) -> R<()> {
        let off = self
            .dir_entries(dir)?
            .into_iter()
            .find(|(_, _, n)| n == name)
            .map(|(o, _, _)| o)
            .ok_or(FsError::NotFound)?;
        let mut d = self.load_inode(dir)?;
        self.write_data(dir, &mut d, off as u64, &[0u8; DIRENT])
    }
    fn set_entry(&mut self, dir: u32, name: &str, ino: u32) -> R<()> {
        let off = self
            .dir_entries(dir)?
            .into_iter()
            .find(|(_, _, n)| n == name)
            .map(|(o, _, _)| o)
            .ok_or(FsError::NotFound)?;
        let mut d = self.load_inode(dir)?;
        self.write_data(dir, &mut d, off as u64, &encode_entry(ino, name))
    }
    fn init_dir(&mut self, ino: u32, parent: u32) -> R<()> {
        let mut i = self.load_inode(ino)?;
        let mut raw = encode_entry(ino, ".").to_vec();
        raw.extend_from_slice(&encode_entry(parent, ".."));
        self.write_data(ino, &mut i, 0, &raw)
    }
    fn dir_is_empty(&mut self, dir: u32) -> R<bool> {
        Ok(self
            .dir_entries(dir)?
            .iter()
            .all(|(_, _, n)| n == "." || n == ".."))
    }

    // ---------------------------------------------------------------------------------------------------------------------
    // path resolution ("." and ".." are ordinary entries, so resolution is uniform)
    // ---------------------------------------------------------------------------------------------------------------------
    fn components(path: &str) -> R<Vec<&str>> {
        if !path.starts_with('/') {
            return Err(FsError::Invalid("path must be absolute"));
        }
        Ok(path.split('/').filter(|c| !c.is_empty()).collect())
    }
    fn walk(&mut self, comps: &[&str]) -> R<u32> {
        let mut cur = ROOT;
        for c in comps {
            cur = self.lookup(cur, c)?.ok_or(FsError::NotFound)?;
        }
        Ok(cur)
    }
    fn resolve(&mut self, path: &str) -> R<u32> {
        let c = Fs::<D>::components(path)?;
        self.walk(&c)
    }
    fn resolve_parent(&mut self, path: &str) -> R<(u32, String)> {
        let mut c = Fs::<D>::components(path)?;
        let name = c.pop().ok_or(FsError::Invalid("the root has no parent"))?;
        check_name(name)?;
        let p = self.walk(&c)?;
        if self.load_inode(p)?.kind != K_DIR {
            return Err(FsError::NotDir);
        }
        Ok((p, name.to_string()))
    }
    fn drop_inode(&mut self, ino: u32) -> R<()> {
        let mut i = self.load_inode(ino)?;
        self.free_from(&mut i, 0)?;
        self.free_inode(ino)
    }

    // ---------------------------------------------------------------------------------------------------------------------
    // 3. public operations (each is one transaction; writes are chunked into several)
    // ---------------------------------------------------------------------------------------------------------------------
    fn create(&mut self, path: &str) -> R<u32> {
        self.tx(|fs| {
            let (p, name) = fs.resolve_parent(path)?;
            if fs.lookup(p, &name)?.is_some() {
                return Err(FsError::Exists);
            }
            let ino = fs.alloc_inode(K_FILE, 1)?;
            fs.add_entry(p, &name, ino)?;
            Ok(ino)
        })
    }
    fn mkdir(&mut self, path: &str) -> R<u32> {
        self.tx(|fs| {
            let (p, name) = fs.resolve_parent(path)?;
            if fs.lookup(p, &name)?.is_some() {
                return Err(FsError::Exists);
            }
            let ino = fs.alloc_inode(K_DIR, 2)?; // the parent's entry + its own "."
            fs.init_dir(ino, p)?;
            fs.add_entry(p, &name, ino)?;
            fs.adjust_nlink(p, 1)?; // the child's ".."
            Ok(ino)
        })
    }
    fn write(&mut self, path: &str, off: u64, data: &[u8]) -> R<usize> {
        let st = self.stat(path)?;
        if st.kind == Kind::Dir {
            return Err(FsError::IsDir);
        }
        if off
            .checked_add(data.len() as u64)
            .is_none_or(|e| e > MAX_FILE_SIZE)
        {
            return Err(FsError::FileTooBig); // checked up front: no partial write
        }
        let chunk = CHUNK_BLOCKS.min(self.sb.journal_blocks as u64 - 2 - TX_OVERHEAD) as usize;
        let mut pos = 0;
        while pos < data.len() {
            // end each piece on a block edge, so it never touches more than `chunk` data blocks
            let into = ((off + pos as u64) % BS as u64) as usize;
            let n = (chunk * BS - into).min(data.len() - pos);
            let (abs, piece) = (off + pos as u64, &data[pos..pos + n]);
            self.tx(|fs| {
                let ino = fs.resolve(path)?;
                let mut i = fs.load_inode(ino)?;
                fs.write_data(ino, &mut i, abs, piece)
            })?;
            pos += n;
        }
        Ok(data.len())
    }
    fn read(&mut self, path: &str, off: u64, len: usize) -> R<Vec<u8>> {
        let ino = self.resolve(path)?;
        let i = self.load_inode(ino)?;
        if i.kind == K_DIR {
            return Err(FsError::IsDir);
        }
        self.read_data(&i, off, len)
    }
    fn truncate(&mut self, path: &str, size: u64) -> R<()> {
        if size > MAX_FILE_SIZE {
            return Err(FsError::FileTooBig);
        }
        self.tx(|fs| {
            let ino = fs.resolve(path)?;
            let mut i = fs.load_inode(ino)?;
            if i.kind == K_DIR {
                return Err(FsError::IsDir);
            }
            if size < i.size {
                if size % BS as u64 != 0 {
                    // zero the tail of the last kept block so a later extension reads zeros
                    let blk = fs.bmap(&mut i, size / BS as u64, false)?;
                    if blk != 0 {
                        let mut b = fs.rd(blk as u64)?;
                        b[(size % BS as u64) as usize..].fill(0);
                        fs.wr(blk as u64, &b);
                    }
                }
                fs.free_from(&mut i, size.div_ceil(BS as u64))?;
            }
            i.size = size;
            i.mtime = fs.now;
            i.ctime = fs.now;
            fs.store_inode(ino, &i)
        })
    }
    fn unlink(&mut self, path: &str) -> R<()> {
        self.tx(|fs| {
            let (p, name) = fs.resolve_parent(path)?;
            let ino = fs.lookup(p, &name)?.ok_or(FsError::NotFound)?;
            let i = fs.load_inode(ino)?;
            if i.kind == K_DIR {
                return Err(FsError::IsDir);
            }
            fs.remove_entry(p, &name)?;
            if i.nlink <= 1 {
                fs.drop_inode(ino)
            } else {
                fs.adjust_nlink(ino, -1)
            }
        })
    }
    fn rmdir(&mut self, path: &str) -> R<()> {
        self.tx(|fs| {
            let (p, name) = fs.resolve_parent(path)?;
            let ino = fs.lookup(p, &name)?.ok_or(FsError::NotFound)?;
            if fs.load_inode(ino)?.kind != K_DIR {
                return Err(FsError::NotDir);
            }
            if !fs.dir_is_empty(ino)? {
                return Err(FsError::NotEmpty);
            }
            fs.remove_entry(p, &name)?;
            fs.adjust_nlink(p, -1)?; // its ".." is gone
            fs.drop_inode(ino)
        })
    }
    fn link(&mut self, existing: &str, new: &str) -> R<()> {
        self.tx(|fs| {
            let ino = fs.resolve(existing)?;
            if fs.load_inode(ino)?.kind == K_DIR {
                return Err(FsError::IsDir); // directory hard links would create cycles
            }
            let (p, name) = fs.resolve_parent(new)?;
            if fs.lookup(p, &name)?.is_some() {
                return Err(FsError::Exists);
            }
            fs.adjust_nlink(ino, 1)?;
            fs.add_entry(p, &name, ino)
        })
    }
    fn rename(&mut self, from: &str, to: &str) -> R<()> {
        self.tx(|fs| {
            let (sp, sn) = fs.resolve_parent(from)?;
            let sino = fs.lookup(sp, &sn)?.ok_or(FsError::NotFound)?;
            let (dp, dn) = fs.resolve_parent(to)?;
            let is_dir = fs.load_inode(sino)?.kind == K_DIR;
            if is_dir {
                let mut cur = dp; // refuse to move a directory into its own subtree
                let mut steps = 0;
                loop {
                    steps += 1;
                    if steps > fs.sb.inode_count {
                        return Err(FsError::Corrupt("'..' entries form a cycle".into()));
                    }
                    if cur == sino {
                        return Err(FsError::Invalid("cannot move a directory into itself"));
                    }
                    if cur == ROOT {
                        break;
                    }
                    cur = fs
                        .lookup(cur, "..")?
                        .ok_or_else(|| FsError::Corrupt("missing ..".into()))?;
                }
            }
            if let Some(dino) = fs.lookup(dp, &dn)? {
                if dino == sino {
                    return Ok(()); // same file (same path or two hard links): POSIX says do nothing
                }
                let din = fs.load_inode(dino)?;
                match (is_dir, din.kind == K_DIR) {
                    (false, true) => return Err(FsError::IsDir),
                    (true, false) => return Err(FsError::NotDir),
                    _ => {}
                }
                if is_dir {
                    if !fs.dir_is_empty(dino)? {
                        return Err(FsError::NotEmpty);
                    }
                    fs.remove_entry(dp, &dn)?;
                    fs.adjust_nlink(dp, -1)?;
                    fs.drop_inode(dino)?;
                } else {
                    fs.remove_entry(dp, &dn)?;
                    if din.nlink <= 1 {
                        fs.drop_inode(dino)?
                    } else {
                        fs.adjust_nlink(dino, -1)?
                    }
                }
            }
            fs.remove_entry(sp, &sn)?;
            fs.add_entry(dp, &dn, sino)?;
            if is_dir && sp != dp {
                fs.set_entry(sino, "..", dp)?;
                fs.adjust_nlink(sp, -1)?;
                fs.adjust_nlink(dp, 1)?;
            }
            Ok(())
        })
    }
    fn stat(&mut self, path: &str) -> R<Stat> {
        let ino = self.resolve(path)?;
        let i = self.load_inode(ino)?;
        let blocks = self.file_blocks(&i)?.len();
        Ok(Stat {
            ino,
            kind: if i.kind == K_DIR {
                Kind::Dir
            } else {
                Kind::File
            },
            size: i.size,
            nlink: i.nlink,
            blocks,
            mtime: i.mtime,
            ctime: i.ctime,
        })
    }
    fn readdir(&mut self, path: &str) -> R<Vec<(String, u32)>> {
        let ino = self.resolve(path)?;
        let mut v: Vec<_> = self
            .dir_entries(ino)?
            .into_iter()
            .filter(|(_, _, n)| n != "." && n != "..")
            .map(|(_, i, n)| (n, i))
            .collect();
        v.sort();
        Ok(v)
    }
    /// whole-tree snapshot: path -> (kind, content, link count)
    fn dump(&mut self) -> R<BTreeMap<String, Node>> {
        let mut out = BTreeMap::new();
        out.insert(
            "/".to_string(),
            Node {
                kind: Kind::Dir,
                data: Vec::new(),
                nlink: self.stat("/")?.nlink,
            },
        );
        let mut stack = vec!["/".to_string()];
        let mut seen = HashSet::from([ROOT]);
        // a snapshot reads files whole, so it refuses a size that only a sparse or damaged inode can have
        let largest = self.sb.total_blocks * BS as u64;
        while let Some(d) = stack.pop() {
            for (name, _) in self.readdir(&d)? {
                let p = if d == "/" {
                    format!("/{name}")
                } else {
                    format!("{d}/{name}")
                };
                let st = self.stat(&p)?;
                if st.kind == Kind::Dir {
                    if !seen.insert(st.ino) {
                        return Err(FsError::Corrupt("a directory is reachable twice".into()));
                    }
                    out.insert(
                        p.clone(),
                        Node {
                            kind: Kind::Dir,
                            data: Vec::new(),
                            nlink: st.nlink,
                        },
                    );
                    stack.push(p);
                } else {
                    if st.size > largest {
                        return Err(FsError::FileTooBig);
                    }
                    let data = self.read(&p, 0, st.size as usize)?;
                    out.insert(
                        p,
                        Node {
                            kind: Kind::File,
                            data,
                            nlink: st.nlink,
                        },
                    );
                }
            }
        }
        Ok(out)
    }

    // ---------------------------------------------------------------------------------------------------------------------
    // 4b. FSCK
    // ---------------------------------------------------------------------------------------------------------------------
    /// Claims the inode's blocks. `false`: its block tree could not be read.
    fn fsck_blocks(
        &mut self,
        ino: u32,
        i: &Inode,
        owner: &mut HashMap<u32, u32>,
        problems: &mut Vec<String>,
    ) -> bool {
        match self.file_blocks(i) {
            Err(e) => {
                problems.push(format!("inode {ino}: bad block tree ({e})"));
                return false;
            }
            Ok(bs) => {
                for b in bs {
                    if let Some(prev) = owner.insert(b, ino) {
                        problems.push(format!(
                            "block {b} is cross-linked between inodes {prev} and {ino}"
                        ));
                    }
                }
            }
        }
        true
    }
    fn fsck(&mut self, repair: bool) -> R<FsckReport> {
        let sb = self.sb;
        let mut problems: Vec<String> = Vec::new();
        let mut fixes: Vec<Fix> = Vec::new();
        let mut refs: HashMap<u32, u32> = HashMap::new();
        let mut owner: HashMap<u32, u32> = HashMap::new();
        let mut reached: HashSet<u32> = HashSet::new();
        let mut seen_dirs: HashSet<u32> = HashSet::from([ROOT]);
        let mut queue = vec![(ROOT, ROOT)];
        // set when some part of the tree could not be followed: what looks unreachable may then be in use
        let mut incomplete = false;
        while let Some((d, parent)) = queue.pop() {
            let inode = match self.load_inode(d) {
                Ok(i) if i.kind == K_DIR => i,
                _ => {
                    problems.push(format!("directory inode {d} is damaged or not a directory"));
                    incomplete = true;
                    continue;
                }
            };
            reached.insert(d);
            incomplete |= !self.fsck_blocks(d, &inode, &mut owner, &mut problems);
            let entries = match self.dir_raw(&inode).and_then(|r| parse_dir(&r)) {
                Ok(e) => e,
                Err(e) => {
                    problems.push(format!("directory {d}: {e}"));
                    incomplete = true;
                    continue;
                }
            };
            let (mut dot, mut dotdot) = (false, false);
            let mut names = HashSet::new();
            for (_, e, name) in entries {
                if !names.insert(name.clone()) {
                    problems.push(format!("directory {d}: the name '{name}' is there twice"));
                }
                if e == 0 || e > sb.inode_count {
                    problems.push(format!(
                        "directory {d}: entry '{name}' has invalid inode {e}"
                    ));
                    incomplete = true;
                    continue;
                }
                // The inode itself says whether it is in use. A lost bitmap bit must not hide it: it is followed like any
                // other, and the bit is set again below.
                let child = match self.load_inode(e) {
                    Ok(c) if c.kind != K_FREE => c,
                    _ => {
                        problems.push(format!(
                            "directory {d}: entry '{name}' points to a free or damaged inode {e}"
                        ));
                        incomplete = true;
                        continue;
                    }
                };
                *refs.entry(e).or_default() += 1;
                if name == "." {
                    dot = true;
                    if e != d {
                        problems.push(format!("directory {d}: '.' points to {e}"));
                    }
                } else if name == ".." {
                    dotdot = true;
                    if e != parent {
                        problems.push(format!(
                            "directory {d}: '..' points to {e}, expected {parent}"
                        ));
                    }
                } else if child.kind == K_DIR {
                    if seen_dirs.insert(e) {
                        queue.push((e, d));
                    } else {
                        problems.push(format!("directory {e} is linked more than once (cycle or hard-linked directory)"));
                    }
                } else if reached.insert(e) {
                    incomplete |= !self.fsck_blocks(e, &child, &mut owner, &mut problems);
                }
            }
            if !dot || !dotdot {
                problems.push(format!("directory {d} lacks '.' or '..'"));
            }
        }
        // link counts
        let mut rs: Vec<u32> = reached.iter().copied().collect();
        rs.sort();
        for ino in &rs {
            let i = self.load_inode(*ino)?;
            let want = refs.get(ino).copied().unwrap_or(0);
            if i.nlink as u32 != want {
                problems.push(format!(
                    "inode {ino}: link count {} but {want} directory entries",
                    i.nlink
                ));
                fixes.push(Fix::Nlink(*ino, want.min(u16::MAX as u32) as u16));
            }
        }
        // inode bitmap vs reachability (orphans)
        for ino in 1..=sb.inode_count {
            let set = self.bit_get(sb.ibm_start, ino as u64)?;
            let r = reached.contains(&ino);
            if set && !r {
                problems.push(format!("orphan inode {ino}: allocated but not reachable"));
                fixes.push(Fix::FreeInode(ino));
            } else if !set && r {
                problems.push(format!(
                    "inode {ino} is in use but not marked in the inode bitmap"
                ));
                fixes.push(Fix::SetInodeBit(ino));
            }
        }
        // data bitmap vs reachable blocks
        for bi in 0..(sb.itable_start - sb.dbm_start) {
            let blk = self.rd(sb.dbm_start + bi)?;
            for o in 0..BITS {
                let b = bi * BITS + o;
                if b >= sb.total_blocks {
                    break;
                }
                let actual = blk[(o / 8) as usize] >> (o % 8) & 1 == 1;
                let expected = b < sb.data_start || owner.contains_key(&(b as u32));
                if expected && !actual {
                    problems.push(format!("block {b} is in use but marked free"));
                    fixes.push(Fix::SetBlockBit(b));
                } else if !expected && actual {
                    problems.push(format!("block {b} is marked used but unreachable (leak)"));
                    fixes.push(Fix::ClearBlockBit(b));
                }
            }
        }
        let mut fixed = 0;
        if repair {
            if incomplete {
                // only fixes that cannot lose anything: marking something as used
                fixes.retain(|f| matches!(f, Fix::SetInodeBit(_) | Fix::SetBlockBit(_)));
            }
            // a fix dirties at most two blocks (freeing an inode: its table block and the inode bitmap)
            for chunk in fixes.chunks(((sb.journal_blocks as usize - 2) / 2).min(16)) {
                self.tx(|fs| {
                    for f in chunk {
                        match *f {
                            Fix::Nlink(i, n) => {
                                let mut ino = fs.load_inode(i)?;
                                ino.nlink = n;
                                fs.store_inode(i, &ino)?;
                            }
                            Fix::FreeInode(i) => fs.free_inode(i)?,
                            Fix::SetInodeBit(i) => fs.bit_put(sb.ibm_start, i as u64, true)?,
                            Fix::SetBlockBit(b) => fs.bit_put(sb.dbm_start, b, true)?,
                            Fix::ClearBlockBit(b) => fs.bit_put(sb.dbm_start, b, false)?,
                        }
                    }
                    Ok(())
                })?;
                fixed += chunk.len();
            }
        }
        Ok(FsckReport {
            problems,
            fixed,
            inodes: reached.len(),
            blocks: owner.len(),
        })
    }
}

#[derive(Clone, Copy, Debug)]
enum Fix {
    Nlink(u32, u16),
    FreeInode(u32),
    SetInodeBit(u32),
    SetBlockBit(u64),
    ClearBlockBit(u64),
}
#[derive(Debug)]
struct FsckReport {
    problems: Vec<String>,
    fixed: usize,
    inodes: usize,
    blocks: usize,
}
impl FsckReport {
    fn is_clean(&self) -> bool {
        self.problems.is_empty()
    }
}

// ============================================================================================================================
// Verification: operations, in-memory model, generators
// ============================================================================================================================

#[derive(Clone, Debug)]
enum Op {
    Create(String),
    Mkdir(String),
    Write(String, u64, Vec<u8>),
    Read(String, u64, usize),
    Truncate(String, u64),
    Unlink(String),
    Rmdir(String),
    Rename(String, String),
    Link(String, String),
}
struct ReadResult(Option<Vec<u8>>);

fn apply_op<D: BlockDevice>(fs: &mut Fs<D>, op: &Op) -> (R<()>, ReadResult) {
    let mut rr = ReadResult(None);
    let r = match op {
        Op::Create(p) => fs.create(p).map(|_| ()),
        Op::Mkdir(p) => fs.mkdir(p).map(|_| ()),
        Op::Write(p, o, d) => fs.write(p, *o, d).map(|_| ()),
        Op::Read(p, o, l) => fs.read(p, *o, *l).map(|d| rr.0 = Some(d)),
        Op::Truncate(p, s) => fs.truncate(p, *s),
        Op::Unlink(p) => fs.unlink(p),
        Op::Rmdir(p) => fs.rmdir(p),
        Op::Rename(a, b) => fs.rename(a, b),
        Op::Link(a, b) => fs.link(a, b),
    };
    (r, rr)
}

#[derive(Clone, PartialEq, Debug)]
enum MNode {
    Dir,
    File(usize),
}
/// The reference implementation: a map from path to node, plus file contents by identity (so hard links share data).
#[derive(Default)]
struct Model {
    nodes: BTreeMap<String, MNode>,
    data: HashMap<usize, Vec<u8>>,
    next: usize,
}
impl Model {
    fn parent_ok(&self, p: &str) -> bool {
        match p.rfind('/') {
            Some(0) => true,
            Some(i) => self.nodes.get(&p[..i]) == Some(&MNode::Dir),
            None => false,
        }
    }
    fn has_children(&self, p: &str) -> bool {
        let pre = format!("{p}/");
        self.nodes.keys().any(|k| k.starts_with(&pre))
    }
    fn apply(&mut self, op: &Op) -> (Result<(), ()>, Option<Vec<u8>>) {
        let mut rd = None;
        let r = match op {
            Op::Create(p) | Op::Mkdir(p) => {
                if self.nodes.contains_key(p) || !self.parent_ok(p) {
                    Err(())
                } else if matches!(op, Op::Mkdir(_)) {
                    self.nodes.insert(p.clone(), MNode::Dir);
                    Ok(())
                } else {
                    self.next += 1;
                    self.data.insert(self.next, Vec::new());
                    self.nodes.insert(p.clone(), MNode::File(self.next));
                    Ok(())
                }
            }
            Op::Write(p, off, d) => match self.nodes.get(p) {
                Some(MNode::File(id)) => {
                    if !d.is_empty() {
                        let v = self.data.get_mut(id).unwrap();
                        let end = *off as usize + d.len();
                        if v.len() < end {
                            v.resize(end, 0);
                        }
                        v[*off as usize..end].copy_from_slice(d);
                    }
                    Ok(())
                }
                _ => Err(()),
            },
            Op::Read(p, off, len) => match self.nodes.get(p) {
                Some(MNode::File(id)) => {
                    let v = &self.data[id];
                    let s = (*off as usize).min(v.len());
                    rd = Some(v[s..(s + len).min(v.len())].to_vec());
                    Ok(())
                }
                _ => Err(()),
            },
            Op::Truncate(p, s) => match self.nodes.get(p) {
                Some(MNode::File(id)) => {
                    self.data.get_mut(id).unwrap().resize(*s as usize, 0);
                    Ok(())
                }
                _ => Err(()),
            },
            Op::Unlink(p) => match self.nodes.get(p) {
                Some(MNode::File(_)) => {
                    self.nodes.remove(p);
                    Ok(())
                }
                _ => Err(()),
            },
            Op::Rmdir(p) => {
                if self.nodes.get(p) == Some(&MNode::Dir) && !self.has_children(p) {
                    self.nodes.remove(p);
                    Ok(())
                } else {
                    Err(())
                }
            }
            Op::Link(a, b) => match self.nodes.get(a).cloned() {
                Some(MNode::File(id)) if !self.nodes.contains_key(b) && self.parent_ok(b) => {
                    self.nodes.insert(b.clone(), MNode::File(id));
                    Ok(())
                }
                _ => Err(()),
            },
            Op::Rename(a, b) => self.rename(a, b),
        };
        (r, rd)
    }
    fn rename(&mut self, a: &str, b: &str) -> Result<(), ()> {
        let src = self.nodes.get(a).cloned().ok_or(())?;
        if !self.parent_ok(b) {
            return Err(());
        }
        if src == MNode::Dir && b.starts_with(&format!("{a}/")) {
            return Err(());
        }
        if a == b {
            return Ok(());
        }
        if let Some(dst) = self.nodes.get(b).cloned() {
            match (&src, &dst) {
                (MNode::File(x), MNode::File(y)) if x == y => return Ok(()),
                (MNode::File(_), MNode::File(_)) => {}
                (MNode::Dir, MNode::Dir) if !self.has_children(b) => {}
                _ => return Err(()),
            }
            self.nodes.remove(b);
        }
        self.nodes.remove(a);
        self.nodes.insert(b.to_string(), src.clone());
        if src == MNode::Dir {
            let pre = format!("{a}/");
            let moved: Vec<String> = self
                .nodes
                .keys()
                .filter(|k| k.starts_with(&pre))
                .cloned()
                .collect();
            for k in moved {
                let v = self.nodes.remove(&k).unwrap();
                self.nodes.insert(format!("{b}/{}", &k[pre.len()..]), v);
            }
        }
        Ok(())
    }
    fn dump(&self) -> BTreeMap<String, Node> {
        let mut out = BTreeMap::new();
        let subdirs = |p: &str| {
            let pre = if p == "/" {
                "/".to_string()
            } else {
                format!("{p}/")
            };
            self.nodes
                .iter()
                .filter(|(k, v)| {
                    **v == MNode::Dir && k.starts_with(&pre) && !k[pre.len()..].contains('/')
                })
                .count()
        };
        out.insert(
            "/".to_string(),
            Node {
                kind: Kind::Dir,
                data: Vec::new(),
                nlink: 2 + subdirs("/") as u16,
            },
        );
        for (p, n) in &self.nodes {
            out.insert(
                p.clone(),
                match n {
                    MNode::Dir => Node {
                        kind: Kind::Dir,
                        data: Vec::new(),
                        nlink: 2 + subdirs(p) as u16,
                    },
                    MNode::File(id) => Node {
                        kind: Kind::File,
                        data: self.data[id].clone(),
                        nlink: self
                            .nodes
                            .values()
                            .filter(|v| **v == MNode::File(*id))
                            .count() as u16,
                    },
                },
            );
        }
        out
    }
}

struct GenCfg {
    max_len: u64,
    max_off: u64,
    deep_offsets: bool,
}
fn rand_name(rng: &mut Rng) -> &'static str {
    ["a", "b", "c", "d", "e"][rng.below(5) as usize]
}
fn rand_path(rng: &mut Rng, m: &Model) -> String {
    let keys: Vec<&String> = m.nodes.keys().collect();
    match rng.below(10) {
        0..=4 if !keys.is_empty() => keys[rng.below(keys.len() as u64) as usize].clone(),
        5..=7 => {
            // a new name under an existing directory (or the root)
            let dirs: Vec<&&String> = keys.iter().filter(|k| m.nodes[**k] == MNode::Dir).collect();
            let base = if dirs.is_empty() || rng.chance(30) {
                String::new()
            } else {
                (*dirs[rng.below(dirs.len() as u64) as usize]).clone()
            };
            format!("{base}/{}", rand_name(rng))
        }
        _ => (0..1 + rng.below(3))
            .map(|_| format!("/{}", rand_name(rng)))
            .collect(),
    }
}
fn rand_bytes(rng: &mut Rng, n: usize) -> Vec<u8> {
    let tag = 1 + rng.below(250) as u8;
    (0..n)
        .map(|i| tag.wrapping_add((i * 7) as u8) | 1)
        .collect()
}
fn gen_op(rng: &mut Rng, m: &Model, cfg: &GenCfg, allow_read: bool) -> Op {
    loop {
        let r = rng.below(100);
        let p = rand_path(rng, m);
        let off = |rng: &mut Rng| match rng.below(100) {
            0..=34 => 0,
            35..=79 => rng.below(cfg.max_off),
            80..=94 => (rng.below(16) * BS as u64)
                .saturating_sub(rng.below(3))
                .min(cfg.max_off),
            _ if cfg.deep_offsets && rng.chance(50) => {
                (ND as u64 + PPB) * BS as u64 + rng.below(3 * BS as u64)
            }
            _ => rng.below(cfg.max_off),
        };
        return match r {
            0..=17 => Op::Create(p),
            18..=27 => Op::Mkdir(p),
            28..=52 => {
                let o = off(rng);
                let len = if rng.chance(70) {
                    rng.below(cfg.max_len.min(2000))
                } else {
                    rng.below(cfg.max_len)
                } as usize;
                Op::Write(p, o, rand_bytes(rng, len))
            }
            53..=66 => {
                if !allow_read {
                    continue;
                }
                Op::Read(p, off(rng), rng.below(20000) as usize)
            }
            67..=73 => Op::Truncate(p, if rng.chance(30) { 0 } else { off(rng) }),
            74..=80 => Op::Unlink(p),
            81..=85 => Op::Rmdir(p),
            86..=93 => Op::Rename(p, rand_path(rng, m)),
            _ => Op::Link(p, rand_path(rng, m)),
        };
    }
}

fn new_mem_fs(total: u64, inodes: u32) -> Fs<MemDisk> {
    Fs::format(
        MemDisk::new(total),
        &Geometry {
            total_blocks: total,
            inode_count: inodes,
            journal_blocks: 64,
        },
    )
    .unwrap()
}

// ----- test: model comparison -----
#[derive(Default)]
struct ModelStats {
    ops: u64,
    ok: u64,
    fsck_runs: u64,
    remounts: u64,
}
fn model_test(n_ops: usize) -> (ModelStats, Fs<MemDisk>) {
    let mut fs = new_mem_fs(4096, 512);
    let mut model = Model::default();
    let mut rng = Rng::new(0xC0FFEE);
    let cfg = GenCfg {
        max_len: 30_000,
        max_off: 60_000,
        deep_offsets: true,
    };
    let mut st = ModelStats::default();
    for step in 0..n_ops {
        let op = gen_op(&mut rng, &model, &cfg, true);
        let (mr, mread) = model.apply(&op);
        let (fr, fread) = apply_op(&mut fs, &op);
        assert_eq!(
            mr.is_ok(),
            fr.is_ok(),
            "step {step}: {op:?}: model {mr:?} vs fs {fr:?}"
        );
        assert_eq!(mread, fread.0, "read mismatch at step {step}: {op:?}");
        st.ops += 1;
        st.ok += mr.is_ok() as u64;
        if step % 100 == 99 {
            assert_eq!(
                fs.dump().unwrap(),
                model.dump(),
                "tree mismatch at step {step}"
            );
        }
        if step % 250 == 249 {
            let rep = fs.fsck(false).unwrap();
            assert!(rep.is_clean(), "fsck after step {step}: {:?}", rep.problems);
            st.fsck_runs += 1;
        }
        if step % 1500 == 1499 {
            let (stats, hits, misses, wb) = (
                fs.stats.clone(),
                fs.cache.hits,
                fs.cache.misses,
                fs.cache.writebacks,
            );
            fs = Fs::mount(fs.into_device()).unwrap(); // unmount + mount: everything must come back from disk
            fs.stats = stats;
            fs.cache.hits = hits;
            fs.cache.misses = misses;
            fs.cache.writebacks = wb;
            assert_eq!(fs.dump().unwrap(), model.dump());
            st.remounts += 1;
        }
    }
    assert_eq!(fs.dump().unwrap(), model.dump());
    assert!(fs.fsck(false).unwrap().is_clean());
    (st, fs)
}

// ----- test: crash at every write boundary -----
#[derive(Default)]
struct CrashStats {
    sequences: u64,
    ops: u64,
    crash_points: u64,
    recovered_pre: u64,
    recovered_post: u64,
    replays: u64,
    replay_crashes: u64,
}
fn crash_test(seqs: u64) -> CrashStats {
    let geo = Geometry {
        total_blocks: 256,
        inode_count: 64,
        journal_blocks: 64,
    };
    let mut cs = CrashStats::default();
    let cfg = GenCfg {
        max_len: 9000,
        max_off: 70_000,
        deep_offsets: false,
    };
    for s in 0..seqs {
        let mut rng = Rng::new(7000 + s);
        let mut fs = Fs::format(FaultDevice::new(256), &geo).unwrap();
        let mut model = Model::default();
        cs.sequences += 1;
        for _ in 0..14 {
            let op = gen_op(&mut rng, &model, &cfg, false);
            let _ = model.apply(&op);
            let pre_img = fs.cache.dev.snapshot();
            let pre = fs.dump().unwrap();
            fs.cache.dev.arm(None);
            let (r, _) = apply_op(&mut fs, &op);
            let n = fs.cache.dev.writes;
            let post = fs.dump().unwrap();
            if r.is_err() {
                assert_eq!(pre, post, "a failed operation must leave no trace: {op:?}");
                assert_eq!(n, 0);
            }
            cs.ops += 1;
            for mode in [CrashMode::Prefix, CrashMode::Chaos(s * 31 + 5)] {
                for k in 0..=n {
                    let mut f2 = Fs::mount(FaultDevice::from_image(pre_img.clone())).unwrap();
                    f2.cache.dev.arm(Some(k));
                    let _ = apply_op(&mut f2, &op);
                    let img = f2.cache.dev.post_crash_image(mode);
                    cs.crash_points += 1;
                    // crash again while the recovery itself is running (first few boundaries only)
                    if k % 4 == 0 {
                        let mut d = FaultDevice::from_image(img.clone());
                        let probe_writes = {
                            let mut p = FaultDevice::from_image(img.clone());
                            let _ = Fs::<FaultDevice>::replay_journal(&mut p);
                            p.writes
                        };
                        // up to `probe_writes` itself: then the last sync is cut, with everything before it in doubt
                        for rk in 0..=probe_writes {
                            d.arm(Some(rk));
                            let _ = Fs::<FaultDevice>::replay_journal(&mut d);
                            // in order, and also reordered, dropped and torn: only the latter can lose a block that
                            // was written before one that survived
                            let cuts = [CrashMode::Prefix]
                                .into_iter()
                                .chain((0..4).map(|i| CrashMode::Chaos(s * 977 + rk * 31 + i)));
                            for cut in cuts {
                                let img2 = d.post_crash_image(cut);
                                let mut f4 = Fs::mount(FaultDevice::from_image(img2))
                                    .expect("mount after crash during replay");
                                let st = f4.dump().unwrap();
                                assert!(
                                    st == pre || st == post,
                                    "crash during replay broke atomicity: {op:?} k={k} rk={rk} {cut:?}"
                                );
                                assert!(
                                    f4.fsck(false).unwrap().is_clean(),
                                    "fsck after crash during replay: {op:?} k={k} rk={rk} {cut:?}"
                                );
                                cs.replay_crashes += 1;
                            }
                            d = FaultDevice::from_image(img.clone());
                        }
                    }
                    let mut f3 = Fs::mount(FaultDevice::from_image(img)).unwrap_or_else(|e| {
                        panic!("mount failed after crash: {e} ({op:?}, k={k}, {mode:?})")
                    });
                    cs.replays += f3.stats.replayed;
                    let rep = f3.fsck(false).unwrap();
                    assert!(
                        rep.is_clean(),
                        "fsck after crash {op:?} k={k} {mode:?}: {:?}",
                        rep.problems
                    );
                    let st = f3.dump().unwrap();
                    if st == post && st != pre {
                        cs.recovered_post += 1;
                    } else if st == pre {
                        cs.recovered_pre += 1;
                    } else {
                        panic!(
                            "ATOMICITY VIOLATION: {op:?} crash after {k}/{n} writes in {mode:?}: state is neither before nor after"
                        );
                    }
                }
            }
        }
    }
    cs
}

// ----- test: features -----
fn feature_tests() {
    let mut fs = new_mem_fs(1024, 128);
    // paths, ".", "..", readdir, nlink
    fs.mkdir("/usr").unwrap();
    fs.mkdir("/usr/lib").unwrap();
    fs.create("/usr/lib/libc.so").unwrap();
    fs.write("/usr/lib/libc.so", 0, b"hello libc").unwrap();
    assert_eq!(
        fs.read("/usr/lib/../lib/./libc.so", 0, 100).unwrap(),
        b"hello libc"
    );
    assert_eq!(
        fs.resolve("/usr/lib/..").unwrap(),
        fs.resolve("/usr").unwrap()
    );
    assert_eq!(fs.resolve("/..").unwrap(), ROOT, "root's parent is root");
    assert_eq!(fs.stat("/usr").unwrap().nlink, 3); // entry in /, own ".", and lib's ".."
    assert_eq!(fs.stat("/").unwrap().nlink, 3);
    assert_eq!(
        fs.readdir("/usr")
            .unwrap()
            .iter()
            .map(|e| e.0.as_str())
            .collect::<Vec<_>>(),
        vec!["lib"]
    );
    // typed errors
    assert_eq!(fs.create("/usr/lib/libc.so"), Err(FsError::Exists));
    assert_eq!(fs.create("/nope/x"), Err(FsError::NotFound));
    assert_eq!(fs.create("/usr/lib/libc.so/x"), Err(FsError::NotDir));
    assert_eq!(fs.rmdir("/usr"), Err(FsError::NotEmpty));
    assert_eq!(fs.unlink("/usr"), Err(FsError::IsDir));
    assert_eq!(fs.rmdir("/usr/lib/libc.so"), Err(FsError::NotDir));
    assert_eq!(
        fs.create(&format!("/{}", "x".repeat(60))),
        Err(FsError::NameTooLong)
    );
    assert_eq!(
        fs.create("relative"),
        Err(FsError::Invalid("path must be absolute"))
    );
    assert_eq!(
        fs.rename("/usr", "/usr/lib/inner"),
        Err(FsError::Invalid("cannot move a directory into itself"))
    );
    assert_eq!(fs.link("/usr", "/u2"), Err(FsError::IsDir));
    // hard links
    fs.link("/usr/lib/libc.so", "/libc-link").unwrap();
    assert_eq!(fs.stat("/libc-link").unwrap().nlink, 2);
    assert_eq!(
        fs.stat("/libc-link").unwrap().ino,
        fs.stat("/usr/lib/libc.so").unwrap().ino
    );
    fs.write("/libc-link", 5, b"LIBC").unwrap();
    assert_eq!(fs.read("/usr/lib/libc.so", 0, 100).unwrap(), b"helloLIBCc");
    let free_before = fs.free_blocks().unwrap();
    fs.unlink("/usr/lib/libc.so").unwrap();
    assert_eq!(fs.stat("/libc-link").unwrap().nlink, 1);
    assert_eq!(
        fs.free_blocks().unwrap(),
        free_before,
        "data survives while a link remains"
    );
    fs.unlink("/libc-link").unwrap();
    assert_eq!(
        fs.free_blocks().unwrap(),
        free_before + 1,
        "last unlink frees the block"
    );
    // rename directory across parents fixes ".." and link counts
    fs.mkdir("/other").unwrap();
    fs.rename("/usr/lib", "/other/lib").unwrap();
    assert_eq!(
        fs.resolve("/other/lib/..").unwrap(),
        fs.resolve("/other").unwrap()
    );
    assert_eq!(
        (
            fs.stat("/usr").unwrap().nlink,
            fs.stat("/other").unwrap().nlink
        ),
        (2, 3)
    );
    fs.rmdir("/other/lib").unwrap();
    fs.rmdir("/other").unwrap();
    fs.rmdir("/usr").unwrap();
    // sparse file: hole reads as zeros without allocating
    fs.create("/sparse").unwrap();
    fs.write("/sparse", 10 * BS as u64 + 5, b"X").unwrap();
    let s = fs.stat("/sparse").unwrap();
    assert_eq!((s.size, s.blocks), (10 * BS as u64 + 6, 1));
    assert!(
        fs.read("/sparse", 0, 10 * BS)
            .unwrap()
            .iter()
            .all(|b| *b == 0)
    );
    assert_eq!(
        fs.stat("/sparse").unwrap().blocks,
        1,
        "reading holes allocates nothing"
    );
    // truncate shrink then grow reads zeros (tail of the kept block was cleared)
    fs.create("/t").unwrap();
    fs.write("/t", 0, &vec![0xAB; 6000]).unwrap();
    fs.truncate("/t", 100).unwrap();
    fs.truncate("/t", 6000).unwrap();
    let t = fs.read("/t", 0, 6000).unwrap();
    assert!(t[..100].iter().all(|b| *b == 0xAB) && t[100..].iter().all(|b| *b == 0));
    assert_eq!(fs.stat("/t").unwrap().blocks, 1);
    // timestamps come from the injected clock and move forward
    let (m1, c1) = (fs.stat("/t").unwrap().mtime, fs.stat("/t").unwrap().ctime);
    fs.write("/t", 0, b"z").unwrap();
    assert!(fs.stat("/t").unwrap().mtime > m1 && fs.stat("/t").unwrap().ctime > c1);
    // overwrite in the middle of a block (read-modify-write) keeps neighbours
    fs.write("/t", 4090, &[7u8; 12]).unwrap();
    let t = fs.read("/t", 4088, 16).unwrap();
    assert_eq!(&t[..2], &[0, 0]);
    assert_eq!(&t[2..14], &[7u8; 12]);
    assert!(fs.fsck(false).unwrap().is_clean());
}

// ----- test: size limits, indirect and double-indirect blocks -----
fn limits_test() {
    assert_eq!(MAX_BLOCKS, 12 + 1024 + 1024 * 1024);
    assert_eq!(MAX_FILE_SIZE, 4_299_210_752);
    let mut fs = new_mem_fs(512, 32);
    let base = fs.free_blocks().unwrap();
    fs.create("/big").unwrap();
    // first byte of each region: last direct, first indirect, last indirect, first double-indirect
    for (lbn, expect_blocks) in [(11u64, 1usize), (12, 3), (12 + 1023, 3), (12 + 1024, 6)] {
        let off = lbn * BS as u64 + 17;
        fs.write("/big", off, &[lbn as u8 | 0x80; 3]).unwrap();
        assert_eq!(fs.read("/big", off, 3).unwrap(), vec![lbn as u8 | 0x80; 3]);
        let b = fs.stat("/big").unwrap().blocks;
        assert!(b >= expect_blocks - 1, "lbn {lbn}: {b} blocks");
    }
    // the very last byte a file may hold
    fs.write("/big", MAX_FILE_SIZE - 1, &[0x5A]).unwrap();
    assert_eq!(fs.read("/big", MAX_FILE_SIZE - 1, 10).unwrap(), vec![0x5A]);
    assert_eq!(fs.stat("/big").unwrap().size, MAX_FILE_SIZE);
    assert_eq!(
        fs.write("/big", MAX_FILE_SIZE, &[1]),
        Err(FsError::FileTooBig)
    );
    assert_eq!(
        fs.write("/big", MAX_FILE_SIZE - 1, &[1, 2]),
        Err(FsError::FileTooBig)
    );
    assert_eq!(
        fs.truncate("/big", MAX_FILE_SIZE + 1),
        Err(FsError::FileTooBig)
    );
    assert_eq!(
        fs.read("/big", 0, 3).unwrap(),
        vec![0, 0, 0],
        "holes in between"
    );
    assert!(fs.fsck(false).unwrap().is_clean());
    // shrinking into the double-indirect area frees the tail, truncating to 0 frees everything
    fs.truncate("/big", (12 + 1024 + 5) * BS as u64).unwrap();
    assert!(fs.fsck(false).unwrap().is_clean());
    assert_eq!(
        fs.read("/big", (12 + 1024) as u64 * BS as u64 + 17, 3)
            .unwrap(),
        vec![(12 + 1024) as u8 | 0x80; 3]
    );
    fs.truncate("/big", 0).unwrap();
    fs.unlink("/big").unwrap();
    assert_eq!(
        fs.free_blocks().unwrap(),
        base,
        "every direct, indirect and double-indirect block returned"
    );
    assert!(fs.fsck(false).unwrap().is_clean());
}

// ----- test: ENOSPC -----
fn enospc_test() {
    let mut fs = new_mem_fs(160, 40);
    let free0 = fs.free_blocks().unwrap();
    let mut made = Vec::new();
    let err = loop {
        let p = format!("/f{}", made.len());
        if let Err(e) = fs.create(&p) {
            break e;
        }
        made.push(p.clone());
        if let Err(e) = fs.write(&p, 0, &vec![0xEE; 3 * BS]) {
            break e;
        }
    };
    assert_eq!(err, FsError::NoSpace, "typed error");
    let rep = fs.fsck(false).unwrap();
    assert!(rep.is_clean(), "fsck after ENOSPC: {:?}", rep.problems);
    // a failed write leaves no partial garbage in the failing file and everything written earlier reads back
    for p in &made[..made.len().saturating_sub(1)] {
        assert_eq!(fs.read(p, 0, 3 * BS).unwrap(), vec![0xEE; 3 * BS]);
    }
    // a huge write on a full device fails with NoSpace and is rolled back (single transaction)
    fs.create("/extra").ok();
    let r = fs.write("/extra", 0, &vec![1u8; 15 * BS]);
    assert!(matches!(r, Err(FsError::NoSpace) | Err(FsError::NotFound)));
    assert!(fs.fsck(false).unwrap().is_clean());
    // free space again: it works
    for p in &made {
        fs.unlink(p).unwrap();
    }
    fs.unlink("/extra").ok();
    assert_eq!(fs.free_blocks().unwrap(), free0);
    fs.create("/again").unwrap();
    fs.write("/again", 0, &vec![9u8; 10 * BS]).unwrap();
    assert!(fs.fsck(false).unwrap().is_clean());
    // inode exhaustion is a different typed error
    let mut ifs = new_mem_fs(400, 8);
    let e = (0..)
        .map(|i| ifs.create(&format!("/n{i}")))
        .find_map(|r| r.err())
        .unwrap();
    assert_eq!(e, FsError::NoInodes);
    assert!(ifs.fsck(false).unwrap().is_clean());
    println!(
        "  ENOSPC: filled the device with {} files -> {:?}; fsck clean; space reusable after unlink",
        made.len(),
        err
    );
}

// ----- test: fsck detects and repairs -----
fn fsck_test() {
    let mut fs = new_mem_fs(512, 64);
    fs.mkdir("/d").unwrap();
    for i in 0..4 {
        fs.create(&format!("/d/f{i}")).unwrap();
        fs.write(&format!("/d/f{i}"), 0, &vec![i as u8 + 1; 5000])
            .unwrap();
    }
    assert!(fs.fsck(false).unwrap().is_clean());
    let v0 = fs.resolve("/d/f0").unwrap();
    let victim = fs.load_inode(v0).unwrap();
    let b0 = victim.direct[0] as u64;
    let sb = fs.sb;
    fs.tx(|fs| {
        fs.bit_put(sb.dbm_start, b0, false)?; // in-use block marked free
        fs.bit_put(sb.dbm_start, sb.total_blocks - 1, true)?; // leaked block
        let o = fs.alloc_inode(K_FILE, 1).map(|_| ())?; // orphan inode
        let ino = fs.resolve("/d/f1")?;
        let mut i = fs.load_inode(ino)?;
        i.nlink = 7; // wrong link count
        fs.store_inode(ino, &i)?;
        Ok(o)
    })
    .unwrap();
    let rep = fs.fsck(false).unwrap();
    assert!(rep.problems.len() >= 4, "found {:?}", rep.problems);
    let joined = rep.problems.join("|");
    for needle in ["marked free", "leak", "orphan inode", "link count"] {
        assert!(joined.contains(needle), "missing '{needle}' in {joined}");
    }
    let fixed = fs.fsck(true).unwrap().fixed;
    assert!(fixed >= 4);
    assert!(fs.fsck(false).unwrap().is_clean(), "repair converges");
    // a cross-linked block is detected (not auto-repaired)
    let a = fs.resolve("/d/f2").unwrap();
    let bnum = fs.resolve("/d/f3").unwrap();
    let ia = fs.load_inode(a).unwrap();
    let mut ib = fs.load_inode(bnum).unwrap();
    ib.direct[0] = ia.direct[0];
    fs.tx(|fs| fs.store_inode(bnum, &ib)).unwrap();
    let rep = fs.fsck(false).unwrap();
    assert!(
        rep.problems.iter().any(|p| p.contains("cross-linked")),
        "{:?}",
        rep.problems
    );
    println!(
        "  fsck: detected link-count, leak, orphan, free-but-used and cross-link problems; repaired {fixed} simple ones"
    );
}

// ----- test: garbage images never panic -----
fn garbage_test() {
    let mut rng = Rng::new(99);
    assert!(Fs::mount(MemDisk::new(0)).is_err());
    assert_eq!(Fs::mount(MemDisk::new(300)).err(), Some(FsError::BadMagic));
    let junk: Vec<u8> = (0..300 * BS).map(|_| rng.next() as u8).collect();
    assert_eq!(
        Fs::mount(MemDisk::from_image(junk)).err(),
        Some(FsError::BadMagic)
    );
    let good = {
        let mut fs = new_mem_fs(300, 32);
        fs.mkdir("/x").unwrap();
        fs.create("/x/y").unwrap();
        fs.write("/x/y", 0, &vec![3u8; 9000]).unwrap();
        fs.into_device().data
    };
    let mut bad = good.clone();
    bad[20] ^= 1; // geometry byte flipped: checksum no longer matches
    assert_eq!(
        Fs::mount(MemDisk::from_image(bad)).err(),
        Some(FsError::BadChecksum)
    );
    assert!(matches!(
        Fs::mount(MemDisk::from_image(good[..100 * BS].to_vec())).err(),
        Some(FsError::BadGeometry(_))
    ));
    // random corruption anywhere after the superblock: mount/fsck/dump may fail with errors, but must never panic
    let mut mounted = 0;
    for i in 0..300u64 {
        let mut img = good.clone();
        for _ in 0..1 + i % 6 {
            let blk = 1 + rng.below(299) as usize;
            let region = if rng.chance(70) {
                rng.below(160) as usize
            } else {
                rng.below(BS as u64) as usize
            };
            img[blk * BS + region % BS] = rng.next() as u8;
        }
        if let Ok(mut fs) = Fs::mount(MemDisk::from_image(img)) {
            mounted += 1;
            let _ = fs.fsck(false);
            let _ = fs.dump();
            let _ = fs.write("/x/y", 100, b"hi");
            let _ = fs.unlink("/x/y");
            let _ = fs.rmdir("/x");
        }
    }
    println!(
        "  garbage images: rejected with typed errors; {mounted}/300 randomly corrupted images mounted and survived fsck/dump/ops without panics"
    );
}

// ----- regressions: each of these failed before its fix -----
/// A disk whose n-th write from now fails once; every other call works.
struct FlakyDisk {
    disk: MemDisk,
    fail_in: Option<u64>,
}
impl BlockDevice for FlakyDisk {
    fn num_blocks(&self) -> u64 {
        self.disk.num_blocks()
    }
    fn read_block(&mut self, n: u64, buf: &mut Block) -> R<()> {
        self.disk.read_block(n, buf)
    }
    fn write_block(&mut self, n: u64, buf: &Block) -> R<()> {
        match self.fail_in {
            Some(0) => {
                self.fail_in = None;
                Err(FsError::Io("injected".into()))
            }
            other => {
                self.fail_in = other.map(|k| k - 1);
                self.disk.write_block(n, buf)
            }
        }
    }
    fn sync(&mut self) -> R<()> {
        self.disk.sync()
    }
}

fn two_files() -> Fs<MemDisk> {
    let mut fs = new_mem_fs(512, 64);
    fs.mkdir("/d").unwrap();
    fs.create("/d/f").unwrap();
    fs.write("/d/f", 0, &[7; 5000]).unwrap();
    fs
}

/// Blocks the data bitmap marks as used.
fn used_blocks<D: BlockDevice>(fs: &mut Fs<D>) -> u64 {
    fs.sb.total_blocks - fs.free_blocks().unwrap()
}

fn regression_tests() {
    // 1. an inode whose bitmap bit was lost is still in use: repair must set the bit, not free the file's blocks
    let mut fs = two_files();
    let used = used_blocks(&mut fs);
    let f = fs.resolve("/d/f").unwrap();
    let ibm = fs.sb.ibm_start;
    fs.tx(|fs| fs.bit_put(ibm, f as u64, false)).unwrap();
    fs.fsck(true).unwrap();
    assert!(fs.fsck(false).unwrap().is_clean(), "lost inode bit");
    assert_eq!(fs.read("/d/f", 0, 5000).unwrap(), vec![7; 5000]);
    assert_eq!(used_blocks(&mut fs), used);

    // 2. a directory that cannot be read hides its children: repair must not free what it could not see
    let mut fs = two_files();
    let used = used_blocks(&mut fs);
    let (d, f) = (fs.resolve("/d").unwrap(), fs.resolve("/d/f").unwrap());
    let dir_block = fs.load_inode(d).unwrap().direct[0] as u64;
    fs.tx(|fs| {
        let mut b = fs.rd(dir_block)?;
        b[4] = 200; // the name length of the first entry
        fs.wr(dir_block, &b);
        Ok(())
    })
    .unwrap();
    let rep = fs.fsck(true).unwrap();
    assert!(!rep.is_clean());
    assert_eq!(
        used_blocks(&mut fs),
        used,
        "repair freed blocks it could not account for"
    );
    assert!(
        fs.bit_get(fs.sb.ibm_start, f as u64).unwrap(),
        "repair freed a hidden file"
    );
    assert_eq!(fs.load_inode(f).unwrap().size, 5000);

    // 3. a directory cycle: dump and rename answer with an error instead of looping
    let mut fs = new_mem_fs(512, 64);
    fs.mkdir("/a").unwrap();
    fs.mkdir("/a/b").unwrap();
    fs.mkdir("/x").unwrap();
    let (a, b) = (fs.resolve("/a").unwrap(), fs.resolve("/a/b").unwrap());
    fs.tx(|fs| fs.add_entry(b, "again", a)).unwrap();
    assert!(
        matches!(fs.dump(), Err(FsError::Corrupt(_))),
        "dump of a cycle"
    );
    fs.tx(|fs| fs.set_entry(b, "..", b)).unwrap();
    assert!(
        matches!(fs.rename("/x", "/a/b/x"), Err(FsError::Corrupt(_))),
        "'..' cycle"
    );

    // 4. a directory claiming gigabytes is refused before anything that size is allocated
    let mut fs = two_files();
    let d = fs.resolve("/d").unwrap();
    let mut i = fs.load_inode(d).unwrap();
    i.size = 1 << 31;
    fs.tx(|fs| fs.store_inode(d, &i)).unwrap();
    assert!(
        matches!(fs.readdir("/d"), Err(FsError::Corrupt(_))),
        "huge directory"
    );

    // 5. every journal that format accepts can format and take a large write across the indirect boundaries
    for journal_blocks in 4..=20 {
        let g = Geometry {
            total_blocks: 1600,
            inode_count: 32,
            journal_blocks,
        };
        match Fs::format(MemDisk::new(1600), &g) {
            Err(FsError::BadGeometry(_)) => assert!(journal_blocks < MIN_JOURNAL),
            Err(e) => panic!("journal of {journal_blocks}: {e}"),
            Ok(mut fs) => {
                fs.create("/big").unwrap();
                for off in [10 * BS as u64 + 1, (ND as u64 + PPB - 3) * BS as u64 + 7] {
                    fs.write("/big", off, &vec![5; 40 * BS])
                        .unwrap_or_else(|e| panic!("journal of {journal_blocks}: {e}"));
                }
            }
        }
    }

    // 6. after a commit that failed on the device, nothing more is served until the next mount
    let g = Geometry {
        total_blocks: 256,
        inode_count: 32,
        journal_blocks: 32,
    };
    for k in 0..14 {
        let dev = FlakyDisk {
            disk: MemDisk::new(256),
            fail_in: None,
        };
        let mut fs = Fs::format(dev, &g).unwrap();
        fs.cache.dev.fail_in = Some(k);
        if fs.create("/a").is_ok() {
            continue;
        }
        assert_eq!(fs.create("/b"), Err(FsError::Poisoned), "write {k}");
        assert_eq!(fs.readdir("/"), Err(FsError::Poisoned), "write {k}");
        let mut fs = Fs::mount(fs.into_device()).unwrap();
        assert!(fs.fsck(false).unwrap().is_clean(), "write {k}");
    }

    // 7. a journal header carrying the largest sequence number must not overflow the next commit
    let mut dev = two_files().into_device();
    let mut h = zero_block();
    p32(&mut h[..], 0, JMAGIC);
    p64(&mut h[..], 8, u64::MAX);
    dev.write_block(1, &h).unwrap();
    Fs::mount(dev).unwrap().create("/after").unwrap();

    // 8. a power cut during a re-format leaves either an error at mount or an empty, clean filesystem
    let g = Geometry {
        total_blocks: 128,
        inode_count: 32,
        journal_blocks: 16,
    };
    let old = {
        let mut fs = Fs::format(FaultDevice::new(128), &g).unwrap();
        fs.mkdir("/old").unwrap();
        fs.create("/old/f").unwrap();
        fs.cache.dev.snapshot()
    };
    for k in 0..60 {
        for seed in 0..40 {
            let mut dev = FaultDevice::from_image(old.clone());
            dev.arm(Some(k));
            // format takes the device by value, so lend it through a handle that keeps it
            let result = Fs::format(&mut dev, &g).map(|_| ());
            if result.is_ok() {
                continue;
            }
            let img = dev.post_crash_image(CrashMode::Chaos(seed));
            if img == old {
                continue; // nothing reached the disk
            }
            if let Ok(mut fs) = Fs::mount(MemDisk::from_image(img)) {
                let tree = fs.dump().unwrap();
                let rep = fs.fsck(false).unwrap();
                assert!(
                    (tree.len() == 1 || tree.len() == 3) && rep.is_clean(),
                    "re-format cut after {k} writes (seed {seed}): {:?}",
                    rep.problems
                );
            }
        }
    }

    // 9. two entries with one name are reported
    let mut fs = two_files();
    let d = fs.resolve("/d").unwrap();
    let f = fs.resolve("/d/f").unwrap();
    fs.tx(|fs| {
        fs.adjust_nlink(f, 1)?;
        fs.add_entry(d, "f", f)
    })
    .unwrap();
    assert!(
        fs.fsck(false)
            .unwrap()
            .problems
            .iter()
            .any(|p| p.contains("twice"))
    );

    // 10. writing a file's data changes its change time too
    let mut fs = two_files();
    let before = fs.stat("/d/f").unwrap().ctime;
    fs.write("/d/f", 0, b"x").unwrap();
    assert!(fs.stat("/d/f").unwrap().ctime > before);

    // 11. pointer blocks that name each other cannot make a file own more blocks than the disk has
    let mut fs = two_files();
    let f = fs.resolve("/d/f").unwrap();
    let mut i = fs.load_inode(f).unwrap();
    let (x, y) = (500u32, 501u32);
    i.dind = x;
    fs.tx(|fs| {
        for (block, target) in [(x, y), (y, 502)] {
            let mut b = zero_block();
            for at in 0..PPB as usize {
                p32(&mut b[..], at * 4, target);
            }
            fs.wr(block as u64, &b);
        }
        fs.store_inode(f, &i)
    })
    .unwrap();
    assert!(
        matches!(fs.stat("/d/f"), Err(FsError::Corrupt(_))),
        "a tree bigger than the disk"
    );

    // 14. repair fits a small journal even when each fix dirties two blocks (freeing an inode: its table block and the bitmap)
    let g = Geometry {
        total_blocks: 600,
        inode_count: 520,
        journal_blocks: MIN_JOURNAL,
    };
    let mut fs = Fs::format(MemDisk::new(600), &g).unwrap();
    for i in 0..500 {
        fs.create(&format!("/{i}")).unwrap();
    }
    // sixteen orphans in sixteen different inode table blocks
    for i in (0..500).step_by(32) {
        fs.tx(|fs| fs.remove_entry(ROOT, &i.to_string())).unwrap();
    }
    assert_eq!(
        fs.fsck(true).map(|r| r.fixed),
        Ok(16),
        "repair of many orphans"
    );
    assert!(fs.fsck(false).unwrap().is_clean());

    // 13. a cut can fall on a sync, while every write before it is still in doubt
    let mut dev = FaultDevice::new(4);
    dev.arm(Some(1));
    dev.write_block(1, &[1; BS]).unwrap();
    assert_eq!(
        dev.sync(),
        Err(FsError::Crashed),
        "the sync after the last allowed write"
    );
    assert_eq!(dev.pending.len(), 1);

    // 12. a stored name with a slash is damage, and a block number near the end of u64 is only out of range
    assert!(parse_dir(&encode_entry(3, "a/b")).is_err());
    let mut buf = zero_block();
    assert!(MemDisk::new(2).read_block((1 << 52) - 1, &mut buf).is_err());
}

// ----- test: real file persistence -----
struct TempFile(PathBuf);
impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn persistence_test() {
    let tmp = TempFile(std::env::temp_dir().join(format!("c11_toyfs_{}.img", std::process::id())));
    let geo = Geometry {
        total_blocks: 1024,
        inode_count: 128,
        journal_blocks: 64,
    };
    let big: Vec<u8> = (0..150_000u32).map(|i| (i * 31 % 251) as u8).collect();
    {
        let mut fs = Fs::format(FileDevice::create(&tmp.0, 1024).unwrap(), &geo).unwrap();
        fs.mkdir("/docs").unwrap();
        fs.create("/docs/big.bin").unwrap();
        fs.write("/docs/big.bin", 0, &big).unwrap();
        fs.create("/docs/sparse").unwrap();
        fs.write("/docs/sparse", 5_000_000, b"tail").unwrap(); // lands in the double-indirect area
        fs.link("/docs/big.bin", "/alias").unwrap();
    } // dropped = unmounted
    let mut fs = Fs::mount(FileDevice::open(&tmp.0).unwrap()).unwrap();
    assert_eq!(fs.read("/alias", 0, 200_000).unwrap(), big);
    assert_eq!(fs.stat("/alias").unwrap().nlink, 2);
    assert_eq!(fs.read("/docs/sparse", 5_000_000, 10).unwrap(), b"tail");
    assert_eq!(fs.stat("/docs/sparse").unwrap().size, 5_000_004);
    assert!(fs.fsck(false).unwrap().is_clean());
    println!(
        "  persistence: {} bytes + sparse 5 MB file survived unmount/mount on a real file ({})",
        big.len(),
        tmp.0.file_name().unwrap().to_string_lossy()
    );
}

fn main() {
    println!(
        "toy filesystem: block size {BS}, inode {INODE_SIZE} B, dirent {DIRENT} B, max file size {MAX_FILE_SIZE} bytes ({:.3} GiB, {MAX_BLOCKS} blocks)",
        MAX_FILE_SIZE as f64 / (1u64 << 30) as f64
    );
    let t0 = std::time::Instant::now();
    feature_tests();
    println!("  features: paths, links, rename, sparse files, truncate, typed errors, clock: ok");
    limits_test();
    println!(
        "  limits: direct/indirect/double-indirect boundaries, max-size write, FileTooBig, full free on truncate: ok"
    );
    let (ms, mut fs) = model_test(6000);
    println!(
        "  model test: {} random ops ({} succeeded), {} fsck runs, {} remounts: filesystem == HashMap-style model",
        ms.ops, ms.ok, ms.fsck_runs, ms.remounts
    );
    assert!(ms.ok > ms.ops / 5);
    let cs = crash_test(40);
    println!(
        "  crash test: {} sequences, {} ops, {} crash points (Prefix + torn/reordered Chaos), {} double-crashes in recovery; recovered pre-state {}, post-state {}, journal replays {}; never in between, fsck always clean",
        cs.sequences,
        cs.ops,
        cs.crash_points,
        cs.replay_crashes,
        cs.recovered_pre,
        cs.recovered_post,
        cs.replays
    );
    assert!(cs.recovered_pre > 0 && cs.recovered_post > 0 && cs.replays > 0);
    persistence_test();
    regression_tests();
    println!(
        "  regressions: repair keeps what it cannot see, cycles and huge sizes are errors, failed commits stop the mount: ok"
    );
    enospc_test();
    fsck_test();
    garbage_test();
    // summary of the long-running model filesystem
    let free = fs.free_blocks().unwrap();
    let used = fs.sb.total_blocks - free;
    let rep = fs.fsck(false).unwrap();
    let (h, m) = (fs.cache.hits, fs.cache.misses);
    println!(
        "summary: {} of {} blocks used ({} metadata, {} reachable data/indirect), {} inodes, journal commits {} ({} block images, {} per commit), cache hit ratio {:.1}% ({h} hits / {m} misses), {} write-backs, {:.2}s",
        used,
        fs.sb.total_blocks,
        fs.sb.data_start,
        rep.blocks,
        rep.inodes,
        fs.stats.commits,
        fs.stats.journal_images,
        fs.stats.journal_images / fs.stats.commits.max(1),
        100.0 * h as f64 / (h + m).max(1) as f64,
        fs.cache.writebacks,
        t0.elapsed().as_secs_f64()
    );
    assert!(rep.is_clean());
    println!("all checks passed");
}
