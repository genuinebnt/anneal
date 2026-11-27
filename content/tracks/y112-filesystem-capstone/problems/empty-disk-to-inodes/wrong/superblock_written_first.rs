#![allow(dead_code)]

use std::fmt;

pub const BS: usize = 4096;
pub type Block = [u8; BS];

/// A block of zeros, on the heap.
pub fn zero_block() -> Box<Block> {
    Box::new([0u8; BS])
}

/// Every way the filesystem can say no. Later steps use more of these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsError {
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
    BadGeometry,
    Corrupt(&'static str),
    Io(&'static str),
    Crashed,
    Poisoned,
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for FsError {}

pub type R<T> = Result<T, FsError>;

/// A disk that moves whole blocks. A write may stay in the disk's own memory until `sync`.
pub trait BlockDevice {
    fn num_blocks(&self) -> u64;
    fn read_block(&mut self, n: u64, buf: &mut Block) -> R<()>;
    fn write_block(&mut self, n: u64, buf: &Block) -> R<()>;
    /// Everything written before this call survives a power cut after it.
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

/// A disk in memory.
#[derive(Clone)]
pub struct MemDisk {
    data: Vec<u8>,
}

impl MemDisk {
    pub fn new(blocks: u64) -> MemDisk {
        MemDisk {
            data: vec![0; blocks as usize * BS],
        }
    }

    /// A disk holding these bytes. Bytes after the last whole block are not part of it.
    pub fn from_image(data: Vec<u8>) -> MemDisk {
        MemDisk { data }
    }

    pub fn image(&self) -> &[u8] {
        &self.data
    }

    /// Where block `n` starts, if the whole block is on the disk.
    fn offset(&self, n: u64) -> Option<usize> {
        let start = usize::try_from(n).ok()?.checked_mul(BS)?;
        (start.checked_add(BS)? <= self.data.len()).then_some(start)
    }
}

impl BlockDevice for MemDisk {
    fn num_blocks(&self) -> u64 {
        (self.data.len() / BS) as u64
    }
    fn read_block(&mut self, n: u64, buf: &mut Block) -> R<()> {
        let at = self.offset(n).ok_or(FsError::Io("read past the end"))?;
        buf.copy_from_slice(&self.data[at..at + BS]);
        Ok(())
    }
    fn write_block(&mut self, n: u64, buf: &Block) -> R<()> {
        let at = self.offset(n).ok_or(FsError::Io("write past the end"))?;
        self.data[at..at + BS].copy_from_slice(buf);
        Ok(())
    }
    fn sync(&mut self) -> R<()> {
        Ok(())
    }
}

// ----- fields at fixed places, little-endian -----

fn g16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn g32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn g64(b: &[u8], o: usize) -> u64 {
    u64::from(g32(b, o)) | u64::from(g32(b, o + 4)) << 32
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
    let mut table = [0u32; 256];
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
        table[i] = c;
        i += 1;
    }
    table
};

/// CRC-32 of more bytes: `crc32_extend(crc32(a), b) == crc32(a followed by b)`.
pub fn crc32_extend(prev: u32, data: &[u8]) -> u32 {
    let mut c = !prev;
    for &byte in data {
        c = CRC_TABLE[((c ^ u32::from(byte)) & 0xff) as usize] ^ (c >> 8);
    }
    !c
}

pub fn crc32(data: &[u8]) -> u32 {
    crc32_extend(0, data)
}

// ----- the layout and the superblock -----

pub const MAGIC: u64 = 0x5459_4F46_5330_3031;
pub const VERSION: u32 = 1;
pub const INODE_SIZE: usize = 128;
pub const INODES_PER_BLOCK: u64 = (BS / INODE_SIZE) as u64;
/// Bits in one bitmap block.
pub const BITS: u64 = (BS * 8) as u64;
pub const MIN_JOURNAL: u32 = 12;
pub const MAX_JOURNAL: u32 = 1000;

/// What the person making a filesystem chooses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub total_blocks: u64,
    pub inode_count: u32,
    pub journal_blocks: u32,
}

/// Where everything is. All of it follows from the geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Super {
    pub total_blocks: u64,
    pub inode_count: u32,
    pub journal_blocks: u32,
    pub journal_start: u64,
    pub ibm_start: u64,
    pub dbm_start: u64,
    pub itable_start: u64,
    pub data_start: u64,
}

impl Super {
    pub fn layout(g: &Geometry) -> R<Super> {
        if g.inode_count == 0 || u64::from(g.inode_count) >= BITS {
            return Err(FsError::BadGeometry);
        }
        if g.journal_blocks < MIN_JOURNAL || g.journal_blocks > MAX_JOURNAL {
            return Err(FsError::BadGeometry);
        }
        if g.total_blocks > u64::from(u32::MAX) {
            return Err(FsError::BadGeometry);
        }
        let journal_start = 1;
        let ibm_start = journal_start + u64::from(g.journal_blocks);
        let dbm_start = ibm_start + 1;
        let itable_start = dbm_start + g.total_blocks.div_ceil(BITS).max(1);
        let itable_blocks = (u64::from(g.inode_count) + 1).div_ceil(INODES_PER_BLOCK);
        let data_start = itable_start + itable_blocks;
        if g.total_blocks < data_start + 8 {
            return Err(FsError::BadGeometry);
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

    pub fn encode(&self) -> Box<Block> {
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
        let sum = crc32(&b[..72]);
        p32(&mut b[..], 72, sum);
        b
    }

    pub fn decode(b: &Block) -> R<Super> {
        if g64(b, 0) != MAGIC {
            return Err(FsError::BadMagic);
        }
        if g32(b, 72) != crc32(&b[..72]) {
            return Err(FsError::BadChecksum);
        }
        if g32(b, 8) != VERSION || g32(b, 12) != BS as u32 {
            return Err(FsError::BadGeometry);
        }
        let sb = Super::layout(&Geometry {
            total_blocks: g64(b, 16),
            inode_count: g32(b, 24),
            journal_blocks: g32(b, 28),
        })?;
        // the stored places must be the ones that follow from the stored geometry
        if sb.encode()[..72] != b[..72] {
            return Err(FsError::BadGeometry);
        }
        Ok(sb)
    }
}

// ----- the cache: changes wait here until somebody flushes them -----

use std::collections::HashMap;

struct Entry {
    data: Box<Block>,
    dirty: bool,
    used: u64,
}

pub struct Cache<D> {
    dev: D,
    capacity: usize,
    entries: HashMap<u64, Entry>,
    tick: u64,
}

impl<D: BlockDevice> Cache<D> {
    pub fn new(dev: D, capacity: usize) -> Cache<D> {
        Cache {
            dev,
            capacity,
            entries: HashMap::new(),
            tick: 0,
        }
    }

    pub fn read(&mut self, n: u64) -> R<Box<Block>> {
        self.tick += 1;
        if let Some(e) = self.entries.get_mut(&n) {
            e.used = self.tick;
            return Ok(e.data.clone());
        }
        let mut data = zero_block();
        self.dev.read_block(n, &mut data)?;
        let entry = Entry {
            data: data.clone(),
            dirty: false,
            used: self.tick,
        };
        self.entries.insert(n, entry);
        self.evict();
        Ok(data)
    }

    pub fn write(&mut self, n: u64, block: &Block) {
        self.tick += 1;
        let entry = Entry {
            data: Box::new(*block),
            dirty: true,
            used: self.tick,
        };
        self.entries.insert(n, entry);
        self.evict();
    }

    /// Forgets the least recently used clean blocks until the cache fits, or only dirty ones are left.
    fn evict(&mut self) {
        while self.entries.len() > self.capacity {
            let oldest = self
                .entries
                .iter()
                .filter(|(_, e)| !e.dirty)
                .min_by_key(|(_, e)| e.used)
                .map(|(n, _)| *n);
            match oldest {
                Some(n) => self.entries.remove(&n),
                None => break,
            };
        }
    }

    /// The changed blocks, by rising block number.
    pub fn dirty_blocks(&self) -> Vec<(u64, Box<Block>)> {
        let mut dirty: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, e)| e.dirty)
            .map(|(n, e)| (*n, e.data.clone()))
            .collect();
        dirty.sort_by_key(|(n, _)| *n);
        dirty
    }

    /// Writes the changed blocks to the device, by rising block number. It does not sync.
    pub fn flush(&mut self) -> R<()> {
        let mut numbers: Vec<u64> = self
            .entries
            .iter()
            .filter(|(_, e)| e.dirty)
            .map(|(n, _)| *n)
            .collect();
        numbers.sort_unstable();
        for n in numbers {
            if let Some(e) = self.entries.get_mut(&n) {
                self.dev.write_block(n, &e.data)?;
                e.dirty = false;
            }
        }
        self.evict();
        Ok(())
    }

    /// Forgets every change that has not been flushed.
    pub fn discard(&mut self) {
        self.entries.retain(|_, e| !e.dirty);
    }

    /// How many blocks the cache holds now.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn device(&mut self) -> &mut D {
        &mut self.dev
    }

    pub fn into_device(self) -> D {
        self.dev
    }
}

// ----- inodes, allocation and transactions -----

pub const DIRECT: usize = 12;
/// Block numbers in one pointer block.
pub const PER_BLOCK: u64 = (BS / 4) as u64;
pub const MAX_FILE_BLOCKS: u64 = DIRECT as u64 + PER_BLOCK + PER_BLOCK * PER_BLOCK;
pub const MAX_FILE_SIZE: u64 = MAX_FILE_BLOCKS * BS as u64;
pub const K_FREE: u8 = 0;
pub const K_FILE: u8 = 1;
pub const K_DIR: u8 = 2;
pub const CACHE_BLOCKS: usize = 128;
/// The first four bytes of the journal's header block.
pub const JMAGIC: u32 = 0x4A52_4E4C;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inode {
    pub kind: u8,
    pub nlink: u16,
    pub mode: u16,
    pub size: u64,
    pub mtime: u64,
    pub ctime: u64,
    pub direct: [u32; DIRECT],
    pub indirect: u32,
    pub double: u32,
}

impl Inode {
    /// Writes the 128-byte record into the front of `out`.
    pub fn encode(&self, out: &mut [u8]) {
        out[..INODE_SIZE].fill(0);
        out[0] = self.kind;
        p16(out, 2, self.nlink);
        p16(out, 4, self.mode);
        p64(out, 8, self.size);
        p64(out, 16, self.mtime);
        p64(out, 24, self.ctime);
        for (i, block) in self.direct.iter().enumerate() {
            p32(out, 32 + 4 * i, *block);
        }
        p32(out, 80, self.indirect);
        p32(out, 84, self.double);
    }

    /// Reads a record as it is. Whether it makes sense is for the caller to check.
    pub fn decode(b: &[u8]) -> Inode {
        let mut direct = [0u32; DIRECT];
        for (i, block) in direct.iter_mut().enumerate() {
            *block = g32(b, 32 + 4 * i);
        }
        Inode {
            kind: b[0],
            nlink: g16(b, 2),
            mode: g16(b, 4),
            size: g64(b, 8),
            mtime: g64(b, 16),
            ctime: g64(b, 24),
            direct,
            indirect: g32(b, 80),
            double: g32(b, 84),
        }
    }
}

pub trait Clock {
    fn now(&mut self) -> u64;
}

/// A clock that moves one tick at each call.
pub struct TickClock(pub u64);

impl Clock for TickClock {
    fn now(&mut self) -> u64 {
        self.0 += 1;
        self.0
    }
}

pub struct Fs<D: BlockDevice> {
    cache: Cache<D>,
    sb: Super,
    clock: Box<dyn Clock>,
    /// The time of the open transaction.
    now: u64,
    in_tx: bool,
    poisoned: bool,
    next_inode: u64,
    next_block: u64,
    /// The number of the last transaction that went through the journal.
    sequence: u64,
}

impl<D: BlockDevice> Fs<D> {
    /// Makes the open transaction's changes durable.
    fn commit(&mut self) -> R<()> {
        let result = match self.cache.flush() {
            Ok(()) => self.cache.device().sync(),
            Err(e) => Err(e),
        };
        if result.is_err() {
            self.poisoned = true;
            self.cache.discard();
        }
        result
    }

    /// Reads the superblock. Returns it with the journal's sequence number.
    fn open_device(dev: &mut D) -> R<(Super, u64)> {
        let sb = read_super(dev)?;
        Ok((sb, 0))
    }
}

/// The superblock of a device that is large enough to hold what it describes.
fn read_super<D: BlockDevice>(dev: &mut D) -> R<Super> {
    if dev.num_blocks() == 0 {
        return Err(FsError::BadMagic);
    }
    let mut block = zero_block();
    dev.read_block(0, &mut block)?;
    let sb = Super::decode(&block)?;
    if dev.num_blocks() < sb.total_blocks {
        return Err(FsError::BadGeometry);
    }
    Ok(sb)
}

impl<D: BlockDevice> Fs<D> {
    /// Writes empty metadata: no inode and no data block is in use.
    pub fn format(mut dev: D, g: &Geometry) -> R<Fs<D>> {
        let sb = Super::layout(g)?;
        if dev.num_blocks() < sb.total_blocks {
            return Err(FsError::BadGeometry);
        }
        let zero = zero_block();
        let sb_block = sb.encode();
        let _ = &sb_block;
        dev.write_block(0, &sb_block)?;
        dev.sync()?;
        for n in 1..sb.data_start {
            dev.write_block(n, &zero)?;
        }
        let mut header = zero_block();
        p32(&mut header[..], 0, JMAGIC);
        dev.write_block(sb.journal_start, &header)?;
        let mut inode_map = zero_block();
        inode_map[0] = 1; // there is no inode 0
        dev.write_block(sb.ibm_start, &inode_map)?;
        for i in 0..sb.itable_start - sb.dbm_start {
            let mut map = zero_block();
            for bit in i * BITS..((i + 1) * BITS).min(sb.data_start) {
                map[((bit % BITS) / 8) as usize] |= 1 << (bit % 8);
            }
            dev.write_block(sb.dbm_start + i, &map)?;
        }
        dev.sync()?;
        Ok(Fs::assemble(dev, sb, 0, Box::new(TickClock(0))))
    }

    fn assemble(dev: D, sb: Super, sequence: u64, clock: Box<dyn Clock>) -> Fs<D> {
        Fs {
            cache: Cache::new(dev, CACHE_BLOCKS),
            sb,
            clock,
            now: 0,
            in_tx: false,
            poisoned: false,
            next_inode: 1,
            next_block: sb.data_start,
            sequence,
        }
    }

    pub fn mount(dev: D) -> R<Fs<D>> {
        Fs::mount_with_clock(dev, Box::new(TickClock(0)))
    }

    pub fn mount_with_clock(mut dev: D, clock: Box<dyn Clock>) -> R<Fs<D>> {
        let (sb, sequence) = Fs::open_device(&mut dev)?;
        Ok(Fs::assemble(dev, sb, sequence, clock))
    }

    pub fn superblock(&self) -> Super {
        self.sb
    }

    pub fn device(&mut self) -> &mut D {
        self.cache.device()
    }

    /// Unmounts. Everything committed is already on the device.
    pub fn into_device(self) -> D {
        self.cache.into_device()
    }

    /// Runs `change` as one transaction: all of its block writes are committed, or none of them.
    pub fn tx<T>(&mut self, change: impl FnOnce(&mut Fs<D>) -> R<T>) -> R<T> {
        if self.poisoned {
            return Err(FsError::Poisoned);
        }
        if self.in_tx {
            return Err(FsError::Invalid("a transaction is already open"));
        }
        self.in_tx = true;
        self.now = self.clock.now();
        let result = change(self);
        self.in_tx = false;
        match result {
            Ok(value) => {
                self.commit()?;
                Ok(value)
            }
            Err(e) => {
                self.cache.discard();
                Err(e)
            }
        }
    }

    // ----- blocks -----

    pub fn read_block(&mut self, n: u64) -> R<Box<Block>> {
        if self.poisoned {
            return Err(FsError::Poisoned);
        }
        if n >= self.sb.total_blocks {
            return Err(FsError::Corrupt("block number out of range"));
        }
        self.cache.read(n)
    }

    /// Changes a block, as part of the open transaction.
    pub fn write_block(&mut self, n: u64, block: &Block) -> R<()> {
        if !self.in_tx {
            return Err(FsError::Invalid("no transaction is open"));
        }
        if n >= self.sb.total_blocks {
            return Err(FsError::Corrupt("block number out of range"));
        }
        self.cache.write(n, block);
        Ok(())
    }

    // ----- bitmaps -----

    pub fn bit(&mut self, start: u64, i: u64) -> R<bool> {
        let map = self.read_block(start + i / BITS)?;
        let at = (i % BITS) as usize;
        Ok(map[at / 8] >> (at % 8) & 1 == 1)
    }

    pub fn set_bit(&mut self, start: u64, i: u64, value: bool) -> R<()> {
        let mut map = self.read_block(start + i / BITS)?;
        let at = (i % BITS) as usize;
        if value {
            map[at / 8] |= 1 << (at % 8);
        } else {
            map[at / 8] &= !(1 << (at % 8));
        }
        self.write_block(start + i / BITS, &map)
    }

    /// The first clear bit at or after `from`, going round once. It is set and returned.
    fn take_bit(&mut self, start: u64, bits: u64, from: u64) -> R<Option<u64>> {
        let mut at = from % bits;
        let mut left = bits;
        while left > 0 {
            let which = at / BITS;
            let end = ((which + 1) * BITS).min(bits).min(at + left);
            let mut map = self.read_block(start + which)?;
            for i in at..end {
                let byte = ((i % BITS) / 8) as usize;
                if map[byte] >> (i % 8) & 1 == 0 {
                    map[byte] |= 1 << (i % 8);
                    self.write_block(start + which, &map)?;
                    return Ok(Some(i));
                }
            }
            left -= end - at;
            at = if end == bits { 0 } else { end };
        }
        Ok(None)
    }

    /// A block number that a file may own.
    fn data_block(&self, n: u32) -> R<u64> {
        let wide = u64::from(n);
        if wide < self.sb.data_start || wide >= self.sb.total_blocks {
            return Err(FsError::Corrupt("pointer outside the data area"));
        }
        Ok(wide)
    }

    /// Takes a free data block and fills it with zeros.
    pub fn alloc_block(&mut self) -> R<u32> {
        if !self.in_tx {
            return Err(FsError::Invalid("no transaction is open"));
        }
        let n = self
            .take_bit(self.sb.dbm_start, self.sb.total_blocks, self.next_block)?
            .ok_or(FsError::NoSpace)?;
        // a damaged map may offer a metadata block: refuse it before it is wiped
        self.data_block(n as u32)?;
        self.next_block = n + 1;
        self.write_block(n, &zero_block())?;
        Ok(n as u32)
    }

    pub fn free_block(&mut self, n: u32) -> R<()> {
        let wide = self.data_block(n)?;
        if !self.bit(self.sb.dbm_start, wide)? {
            return Err(FsError::Corrupt("block freed twice"));
        }
        self.set_bit(self.sb.dbm_start, wide, false)
    }

    /// How many data blocks are free.
    pub fn free_blocks(&mut self) -> R<u64> {
        let mut used = 0;
        for i in 0..self.sb.itable_start - self.sb.dbm_start {
            let map = self.read_block(self.sb.dbm_start + i)?;
            for bit in 0..(self.sb.total_blocks - i * BITS).min(BITS) as usize {
                used += u64::from(map[bit / 8] >> (bit % 8) & 1);
            }
        }
        Ok(self.sb.total_blocks.saturating_sub(used))
    }

    // ----- inodes -----

    /// The block and the byte offset where an inode's record is.
    fn inode_place(&self, ino: u32) -> R<(u64, usize)> {
        if ino == 0 || ino > self.sb.inode_count {
            return Err(FsError::Corrupt("inode number out of range"));
        }
        let block = self.sb.itable_start + u64::from(ino) / INODES_PER_BLOCK;
        let offset = (u64::from(ino) % INODES_PER_BLOCK) as usize * INODE_SIZE;
        Ok((block, offset))
    }

    pub fn load_inode(&mut self, ino: u32) -> R<Inode> {
        let (block, at) = self.inode_place(ino)?;
        let bytes = self.read_block(block)?;
        let inode = Inode::decode(&bytes[at..at + INODE_SIZE]);
        if inode.kind > K_DIR || inode.size > MAX_FILE_SIZE {
            return Err(FsError::Corrupt("damaged inode"));
        }
        Ok(inode)
    }

    pub fn store_inode(&mut self, ino: u32, inode: &Inode) -> R<()> {
        let (block, at) = self.inode_place(ino)?;
        let mut bytes = self.read_block(block)?;
        inode.encode(&mut bytes[at..at + INODE_SIZE]);
        self.write_block(block, &bytes)
    }

    pub fn alloc_inode(&mut self, kind: u8, nlink: u16) -> R<u32> {
        if !self.in_tx {
            return Err(FsError::Invalid("no transaction is open"));
        }
        let bits = u64::from(self.sb.inode_count) + 1;
        let ino = self
            .take_bit(self.sb.ibm_start, bits, self.next_inode)?
            .ok_or(FsError::NoInodes)? as u32;
        self.next_inode = u64::from(ino) + 1;
        let inode = Inode {
            kind,
            nlink,
            mode: if kind == K_DIR { 0o755 } else { 0o644 },
            mtime: self.now,
            ctime: self.now,
            ..Inode::default()
        };
        self.store_inode(ino, &inode)?;
        Ok(ino)
    }

    pub fn free_inode(&mut self, ino: u32) -> R<()> {
        self.store_inode(ino, &Inode::default())?;
        self.set_bit(self.sb.ibm_start, u64::from(ino), false)
    }

    /// Adds `change` to an inode's link count.
    pub fn adjust_links(&mut self, ino: u32, change: i32) -> R<()> {
        let mut inode = self.load_inode(ino)?;
        inode.nlink = u16::try_from(i32::from(inode.nlink) + change)
            .map_err(|_| FsError::Invalid("link count out of range"))?;
        inode.ctime = self.now;
        self.store_inode(ino, &inode)
    }
}
