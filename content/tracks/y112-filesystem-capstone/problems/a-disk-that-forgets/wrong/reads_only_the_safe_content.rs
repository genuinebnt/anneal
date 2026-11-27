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
        // Take any old superblock away first. Until the new one is written, nothing mounts.
        dev.write_block(0, &zero)?;
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
        dev.write_block(0, &sb.encode())?;
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

// ----- the bytes of a file: 12 direct pointers, one indirect block, one double indirect block -----

impl<D: BlockDevice> Fs<D> {
    /// What slot `index` of a pointer block names, allocating it if asked. 0 is a hole.
    fn child(&mut self, parent: u32, index: u64, alloc: bool) -> R<u32> {
        let at = self.data_block(parent)?;
        let mut bytes = self.read_block(at)?;
        let slot = index as usize * 4;
        let found = g32(&bytes[..], slot);
        if found != 0 {
            self.data_block(found)?;
            return Ok(found);
        }
        if !alloc {
            return Ok(0);
        }
        let new = self.alloc_block()?;
        p32(&mut bytes[..], slot, new);
        self.write_block(at, &bytes)?;
        Ok(new)
    }

    /// The disk block that holds block `index` of the file, or 0 for a hole. With `alloc`, whatever is missing on
    /// the way is allocated. The inode is changed in memory only.
    pub fn block_of(&mut self, inode: &mut Inode, index: u64, alloc: bool) -> R<u32> {
        if index >= MAX_FILE_BLOCKS {
            return Err(FsError::FileTooBig);
        }
        if index < DIRECT as u64 {
            let slot = &mut inode.direct[index as usize];
            if *slot != 0 {
                let found = *slot;
                self.data_block(found)?;
                return Ok(found);
            }
            if alloc {
                inode.direct[index as usize] = self.alloc_block()?;
            }
            return Ok(inode.direct[index as usize]);
        }
        let index = index - DIRECT as u64;
        if index < PER_BLOCK {
            if inode.indirect == 0 {
                if !alloc {
                    return Ok(0);
                }
                inode.indirect = self.alloc_block()?;
            }
            return self.child(inode.indirect, index, alloc);
        }
        let index = index - PER_BLOCK;
        if inode.double == 0 {
            if !alloc {
                return Ok(0);
            }
            inode.double = self.alloc_block()?;
        }
        match self.child(inode.double, index / PER_BLOCK, alloc)? {
            0 => Ok(0),
            middle => self.child(middle, index % PER_BLOCK, alloc),
        }
    }

    fn collect(&mut self, block: u32, depth: u32, found: &mut Vec<u32>) -> R<()> {
        let at = self.data_block(block)?;
        // a sound tree cannot own more blocks than the disk has: this stops pointer blocks that name each other
        if found.len() as u64 >= self.sb.total_blocks {
            return Err(FsError::Corrupt(
                "a file owns more blocks than the disk has",
            ));
        }
        found.push(block);
        if depth == 0 {
            return Ok(());
        }
        let bytes = self.read_block(at)?;
        for i in 0..PER_BLOCK as usize {
            let child = g32(&bytes[..], i * 4);
            if child != 0 {
                self.collect(child, depth - 1, found)?;
            }
        }
        Ok(())
    }

    /// Every block the inode owns: data blocks and pointer blocks.
    pub fn file_blocks(&mut self, inode: &Inode) -> R<Vec<u32>> {
        let mut found = Vec::new();
        for block in inode.direct.into_iter().filter(|b| *b != 0) {
            self.collect(block, 0, &mut found)?;
        }
        if inode.indirect != 0 {
            self.collect(inode.indirect, 1, &mut found)?;
        }
        if inode.double != 0 {
            self.collect(inode.double, 2, &mut found)?;
        }
        Ok(found)
    }

    /// Up to `len` bytes from `offset`, stopping at the end of the file.
    pub fn read_at(&mut self, inode: &Inode, offset: u64, len: usize) -> R<Vec<u8>> {
        if offset >= inode.size {
            return Ok(Vec::new());
        }
        let len = (len as u64).min(inode.size - offset) as usize;
        let mut out = vec![0; len];
        let mut copy = inode.clone();
        let mut done = 0;
        while done < len {
            let at = offset + done as u64;
            let within = (at % BS as u64) as usize;
            let take = (BS - within).min(len - done);
            let block = self.block_of(&mut copy, at / BS as u64, false)?;
            if block != 0 {
                let bytes = self.read_block(u64::from(block))?;
                out[done..done + take].copy_from_slice(&bytes[within..within + take]);
            }
            done += take;
        }
        Ok(out)
    }

    /// Writes into a file and stores its inode, as part of the open transaction.
    pub fn write_at(&mut self, ino: u32, inode: &mut Inode, offset: u64, data: &[u8]) -> R<()> {
        let end = offset
            .checked_add(data.len() as u64)
            .filter(|end| *end <= MAX_FILE_SIZE)
            .ok_or(FsError::FileTooBig)?;
        if data.is_empty() {
            return Ok(());
        }
        let mut done = 0;
        while done < data.len() {
            let at = offset + done as u64;
            let within = (at % BS as u64) as usize;
            let take = (BS - within).min(data.len() - done);
            let block = u64::from(self.block_of(inode, at / BS as u64, true)?);
            let mut bytes = if take == BS {
                zero_block()
            } else {
                self.read_block(block)?
            };
            bytes[within..within + take].copy_from_slice(&data[done..done + take]);
            self.write_block(block, &bytes)?;
            done += take;
        }
        inode.size = inode.size.max(end);
        inode.mtime = self.now;
        inode.ctime = self.now;
        self.store_inode(ino, inode)
    }

    /// Frees, below one pointer block, every file block from `keep` on (counted from the block's first). Returns
    /// the pointer to keep: 0 if the pointer block itself went.
    fn cut_tree(&mut self, block: u32, depth: u32, keep: u64) -> R<u32> {
        if keep == 0 && depth == 0 {
            self.free_block(block)?;
            return Ok(0);
        }
        if depth == 0 {
            return Ok(block);
        }
        let at = self.data_block(block)?;
        let mut bytes = self.read_block(at)?;
        let span = PER_BLOCK.pow(depth - 1);
        let mut changed = false;
        for i in 0..PER_BLOCK {
            let slot = i as usize * 4;
            let child = g32(&bytes[..], slot);
            let first = i * span;
            if child == 0 || first + span <= keep {
                continue;
            }
            let left = self.cut_tree(child, depth - 1, keep.saturating_sub(first))?;
            if left != child {
                p32(&mut bytes[..], slot, left);
                changed = true;
            }
        }
        if keep == 0 {
            self.free_block(block)?;
            return Ok(0);
        }
        if changed {
            self.write_block(at, &bytes)?;
        }
        Ok(block)
    }

    /// Frees every block that covers only file blocks from `keep` on.
    fn free_from(&mut self, inode: &mut Inode, keep: u64) -> R<()> {
        for i in 0..DIRECT {
            if i as u64 >= keep && inode.direct[i] != 0 {
                self.free_block(inode.direct[i])?;
                inode.direct[i] = 0;
            }
        }
        if inode.indirect != 0 {
            inode.indirect =
                self.cut_tree(inode.indirect, 1, keep.saturating_sub(DIRECT as u64))?;
        }
        if inode.double != 0 {
            let before = DIRECT as u64 + PER_BLOCK;
            inode.double = self.cut_tree(inode.double, 2, keep.saturating_sub(before))?;
        }
        Ok(())
    }

    /// Changes a file's size and stores its inode, as part of the open transaction.
    pub fn resize(&mut self, ino: u32, inode: &mut Inode, size: u64) -> R<()> {
        if size > MAX_FILE_SIZE {
            return Err(FsError::FileTooBig);
        }
        if size < inode.size {
            let within = (size % BS as u64) as usize;
            if within != 0 {
                // zero the cut part of the last block, so that growing again reads zeros
                let block = self.block_of(inode, size / BS as u64, false)?;
                if block != 0 {
                    let mut bytes = self.read_block(u64::from(block))?;
                    bytes[within..].fill(0);
                    self.write_block(u64::from(block), &bytes)?;
                }
            }
            self.free_from(inode, size.div_ceil(BS as u64))?;
        }
        inode.size = size;
        inode.mtime = self.now;
        inode.ctime = self.now;
        self.store_inode(ino, inode)
    }
}

// ----- directories and paths -----

pub const ROOT: u32 = 1;
pub const ENTRY: usize = 64;
pub const MAX_NAME: usize = ENTRY - 5;
/// A long write is cut into transactions that each touch at most this many data blocks.
pub const MAX_PIECE_BLOCKS: u64 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stat {
    pub ino: u32,
    pub kind: Kind,
    pub size: u64,
    pub nlink: u16,
    /// Data blocks and pointer blocks together.
    pub blocks: u64,
    pub mtime: u64,
    pub ctime: u64,
}

struct DirEntry {
    offset: u64,
    ino: u32,
    name: String,
}

fn parse_directory(raw: &[u8]) -> R<Vec<DirEntry>> {
    if raw.len() % ENTRY != 0 {
        return Err(FsError::Corrupt("directory size"));
    }
    let mut entries = Vec::new();
    for (i, bytes) in raw.chunks_exact(ENTRY).enumerate() {
        let ino = g32(bytes, 0);
        if ino == 0 {
            continue;
        }
        let len = usize::from(bytes[4]);
        if len == 0 || len > MAX_NAME {
            return Err(FsError::Corrupt("entry name length"));
        }
        let name = std::str::from_utf8(&bytes[5..5 + len])
            .map_err(|_| FsError::Corrupt("entry name is not UTF-8"))?;
        if name.contains(['/', '\0']) {
            return Err(FsError::Corrupt("entry name holds a slash or a NUL"));
        }
        entries.push(DirEntry {
            offset: (i * ENTRY) as u64,
            ino,
            name: name.to_owned(),
        });
    }
    Ok(entries)
}

fn entry_bytes(ino: u32, name: &str) -> [u8; ENTRY] {
    let mut bytes = [0; ENTRY];
    p32(&mut bytes, 0, ino);
    bytes[4] = name.len() as u8;
    bytes[5..5 + name.len()].copy_from_slice(name.as_bytes());
    bytes
}

/// Whether a name may be given to something new.
fn check_name(name: &str) -> R<()> {
    if name == "." || name == ".." || name.contains('\0') {
        return Err(FsError::Invalid("this name cannot be used"));
    }
    if name.len() > MAX_NAME {
        return Err(FsError::NameTooLong);
    }
    Ok(())
}

fn components(path: &str) -> R<Vec<&str>> {
    let rest = path
        .strip_prefix('/')
        .ok_or(FsError::Invalid("a path starts with a slash"))?;
    Ok(rest.split('/').filter(|part| !part.is_empty()).collect())
}

impl<D: BlockDevice> Fs<D> {
    /// Makes a new filesystem with an empty root directory.
    pub fn mkfs(dev: D, g: &Geometry) -> R<Fs<D>> {
        let mut fs = Fs::format(dev, g)?;
        fs.tx(|fs| {
            let ino = fs.alloc_inode(K_DIR, 2)?;
            fs.start_directory(ino, ino)
        })?;
        Ok(fs)
    }

    fn start_directory(&mut self, ino: u32, parent: u32) -> R<()> {
        let mut inode = self.load_inode(ino)?;
        let mut raw = entry_bytes(ino, ".").to_vec();
        raw.extend_from_slice(&entry_bytes(parent, ".."));
        self.write_at(ino, &mut inode, 0, &raw)
    }

    /// A directory's whole content, with its inode.
    fn directory_bytes(&mut self, ino: u32) -> R<(Inode, Vec<u8>)> {
        let inode = self.load_inode(ino)?;
        if inode.kind != K_DIR {
            return Err(FsError::NotDir);
        }
        // a directory has no holes, so it cannot be larger than the data area: check before allocating its size
        if inode.size > (self.sb.total_blocks - self.sb.data_start) * BS as u64 {
            return Err(FsError::Corrupt("directory larger than the disk"));
        }
        let raw = self.read_at(&inode, 0, inode.size as usize)?;
        Ok((inode, raw))
    }

    fn entries(&mut self, dir: u32) -> R<Vec<DirEntry>> {
        let (_, raw) = self.directory_bytes(dir)?;
        parse_directory(&raw)
    }

    fn find_entry(&mut self, dir: u32, name: &str) -> R<Option<DirEntry>> {
        Ok(self.entries(dir)?.into_iter().find(|e| e.name == name))
    }

    /// The inode that `name` stands for in the directory `dir`.
    pub fn lookup(&mut self, dir: u32, name: &str) -> R<Option<u32>> {
        Ok(self.find_entry(dir, name)?.map(|e| e.ino))
    }

    /// Adds an entry in the first unused slot, or at the end. It does not check for a second use of the name.
    pub fn add_entry(&mut self, dir: u32, name: &str, ino: u32) -> R<()> {
        if name.is_empty() || name.len() > MAX_NAME {
            return Err(FsError::NameTooLong);
        }
        let (mut inode, raw) = self.directory_bytes(dir)?;
        let slot = raw
            .chunks_exact(ENTRY)
            .position(|bytes| g32(bytes, 0) == 0)
            .map_or(raw.len(), |i| i * ENTRY);
        self.write_at(dir, &mut inode, slot as u64, &entry_bytes(ino, name))
    }

    /// Writes over the slot of the entry called `name`.
    fn replace_slot(&mut self, dir: u32, name: &str, bytes: &[u8; ENTRY]) -> R<()> {
        let entry = self.find_entry(dir, name)?.ok_or(FsError::NotFound)?;
        let mut inode = self.load_inode(dir)?;
        self.write_at(dir, &mut inode, entry.offset, bytes)
    }

    /// Clears the entry called `name`. The directory does not shrink.
    pub fn remove_entry(&mut self, dir: u32, name: &str) -> R<()> {
        self.replace_slot(dir, name, &[0; ENTRY])
    }

    /// Points the entry called `name` at another inode.
    pub fn set_entry(&mut self, dir: u32, name: &str, ino: u32) -> R<()> {
        self.replace_slot(dir, name, &entry_bytes(ino, name))
    }

    fn walk(&mut self, parts: &[&str]) -> R<u32> {
        let mut at = ROOT;
        for part in parts {
            at = self.lookup(at, part)?.ok_or(FsError::NotFound)?;
        }
        Ok(at)
    }

    /// The inode a path leads to.
    pub fn resolve(&mut self, path: &str) -> R<u32> {
        self.walk(&components(path)?)
    }

    /// For a path that is about to be changed: the directory that holds its last part, and that part.
    fn parent_of<'p>(&mut self, path: &'p str) -> R<(u32, &'p str)> {
        let mut parts = components(path)?;
        let name = parts
            .pop()
            .ok_or(FsError::Invalid("the root has no parent"))?;
        check_name(name)?;
        let parent = self.walk(&parts)?;
        if self.load_inode(parent)?.kind != K_DIR {
            return Err(FsError::NotDir);
        }
        Ok((parent, name))
    }

    pub fn create(&mut self, path: &str) -> R<u32> {
        self.tx(|fs| {
            let (parent, name) = fs.parent_of(path)?;
            if fs.lookup(parent, name)?.is_some() {
                return Err(FsError::Exists);
            }
            let ino = fs.alloc_inode(K_FILE, 1)?;
            fs.add_entry(parent, name, ino)?;
            Ok(ino)
        })
    }

    pub fn mkdir(&mut self, path: &str) -> R<u32> {
        self.tx(|fs| {
            let (parent, name) = fs.parent_of(path)?;
            if fs.lookup(parent, name)?.is_some() {
                return Err(FsError::Exists);
            }
            let ino = fs.alloc_inode(K_DIR, 2)?; // the parent's entry and its own "."
            fs.start_directory(ino, parent)?;
            fs.add_entry(parent, name, ino)?;
            fs.adjust_links(parent, 1)?; // the new ".."
            Ok(ino)
        })
    }

    /// The inode of a path that must be a file.
    fn file(&mut self, path: &str) -> R<(u32, Inode)> {
        let ino = self.resolve(path)?;
        let inode = self.load_inode(ino)?;
        if inode.kind == K_DIR {
            return Err(FsError::IsDir);
        }
        Ok((ino, inode))
    }

    /// Writes `data` at `offset`. A long write is several transactions, each atomic by itself.
    pub fn write(&mut self, path: &str, offset: u64, data: &[u8]) -> R<()> {
        self.file(path)?;
        offset
            .checked_add(data.len() as u64)
            .filter(|end| *end <= MAX_FILE_SIZE)
            .ok_or(FsError::FileTooBig)?;
        let piece = MAX_PIECE_BLOCKS.min(u64::from(self.sb.journal_blocks) - 10) as usize;
        let mut done = 0;
        while done < data.len() {
            let at = offset + done as u64;
            // end on a block edge, so that a piece never touches more than `piece` data blocks
            let take = (piece * BS - (at % BS as u64) as usize).min(data.len() - done);
            let bytes = &data[done..done + take];
            self.tx(|fs| {
                let (ino, mut inode) = fs.file(path)?;
                fs.write_at(ino, &mut inode, at, bytes)
            })?;
            done += take;
        }
        Ok(())
    }

    pub fn read(&mut self, path: &str, offset: u64, len: usize) -> R<Vec<u8>> {
        let (_, inode) = self.file(path)?;
        self.read_at(&inode, offset, len)
    }

    pub fn truncate(&mut self, path: &str, size: u64) -> R<()> {
        self.tx(|fs| {
            let (ino, mut inode) = fs.file(path)?;
            fs.resize(ino, &mut inode, size)
        })
    }

    pub fn stat(&mut self, path: &str) -> R<Stat> {
        let ino = self.resolve(path)?;
        let inode = self.load_inode(ino)?;
        Ok(Stat {
            ino,
            kind: if inode.kind == K_DIR {
                Kind::Dir
            } else {
                Kind::File
            },
            size: inode.size,
            nlink: inode.nlink,
            blocks: self.file_blocks(&inode)?.len() as u64,
            mtime: inode.mtime,
            ctime: inode.ctime,
        })
    }

    /// The names in a directory and their inodes, sorted by name, without "." and "..".
    pub fn readdir(&mut self, path: &str) -> R<Vec<(String, u32)>> {
        let ino = self.resolve(path)?;
        let mut names: Vec<_> = self
            .entries(ino)?
            .into_iter()
            .filter(|e| e.name != "." && e.name != "..")
            .map(|e| (e.name, e.ino))
            .collect();
        names.sort();
        Ok(names)
    }
}

// ----- removing, linking and moving -----

impl<D: BlockDevice> Fs<D> {
    /// Frees an inode and every block it owns.
    fn drop_inode(&mut self, ino: u32) -> R<()> {
        let mut inode = self.load_inode(ino)?;
        self.free_from(&mut inode, 0)?;
        self.free_inode(ino)
    }

    /// Takes one name away from a file, and the file with it if that was the last.
    fn drop_file_name(&mut self, dir: u32, name: &str, ino: u32) -> R<()> {
        self.remove_entry(dir, name)?;
        if self.load_inode(ino)?.nlink <= 1 {
            self.drop_inode(ino)
        } else {
            self.adjust_links(ino, -1)
        }
    }

    /// Removes a directory that must be empty.
    fn drop_directory(&mut self, parent: u32, name: &str, ino: u32) -> R<()> {
        if self
            .entries(ino)?
            .iter()
            .any(|e| e.name != "." && e.name != "..")
        {
            return Err(FsError::NotEmpty);
        }
        self.remove_entry(parent, name)?;
        self.adjust_links(parent, -1)?; // its ".." is gone
        self.drop_inode(ino)
    }

    pub fn unlink(&mut self, path: &str) -> R<()> {
        self.tx(|fs| {
            let (parent, name) = fs.parent_of(path)?;
            let ino = fs.lookup(parent, name)?.ok_or(FsError::NotFound)?;
            if fs.load_inode(ino)?.kind == K_DIR {
                return Err(FsError::IsDir);
            }
            fs.drop_file_name(parent, name, ino)
        })
    }

    pub fn rmdir(&mut self, path: &str) -> R<()> {
        self.tx(|fs| {
            let (parent, name) = fs.parent_of(path)?;
            let ino = fs.lookup(parent, name)?.ok_or(FsError::NotFound)?;
            if fs.load_inode(ino)?.kind != K_DIR {
                return Err(FsError::NotDir);
            }
            fs.drop_directory(parent, name, ino)
        })
    }

    /// Gives a file one more name.
    pub fn link(&mut self, existing: &str, new: &str) -> R<()> {
        self.tx(|fs| {
            let (ino, _) = fs.file(existing)?;
            let (parent, name) = fs.parent_of(new)?;
            if fs.lookup(parent, name)?.is_some() {
                return Err(FsError::Exists);
            }
            fs.adjust_links(ino, 1)?;
            fs.add_entry(parent, name, ino)
        })
    }

    /// Whether `dir` is `ancestor` or lies below it, going up by "..".
    fn is_inside(&mut self, dir: u32, ancestor: u32) -> R<bool> {
        let mut at = dir;
        for _ in 0..=self.sb.inode_count {
            if at == ancestor {
                return Ok(true);
            }
            if at == ROOT {
                return Ok(false);
            }
            at = self
                .lookup(at, "..")?
                .ok_or(FsError::Corrupt("missing .."))?;
        }
        Err(FsError::Corrupt("'..' entries form a cycle"))
    }

    pub fn rename(&mut self, from: &str, to: &str) -> R<()> {
        self.tx(|fs| {
            let (old_dir, old_name) = fs.parent_of(from)?;
            let moved = fs.lookup(old_dir, old_name)?.ok_or(FsError::NotFound)?;
            let (new_dir, new_name) = fs.parent_of(to)?;
            let is_dir = fs.load_inode(moved)?.kind == K_DIR;
            if is_dir && fs.is_inside(new_dir, moved)? {
                return Err(FsError::Invalid("a directory cannot move into itself"));
            }
            if let Some(target) = fs.lookup(new_dir, new_name)? {
                if target == moved {
                    return Ok(()); // the same file under both names: nothing happens
                }
                match (is_dir, fs.load_inode(target)?.kind == K_DIR) {
                    (false, true) => return Err(FsError::IsDir),
                    (true, false) => return Err(FsError::NotDir),
                    (true, true) => fs.drop_directory(new_dir, new_name, target)?,
                    (false, false) => fs.drop_file_name(new_dir, new_name, target)?,
                }
            }
            fs.remove_entry(old_dir, old_name)?;
            fs.add_entry(new_dir, new_name, moved)?;
            if is_dir && old_dir != new_dir {
                fs.set_entry(moved, "..", new_dir)?;
                fs.adjust_links(old_dir, -1)?;
                fs.adjust_links(new_dir, 1)?;
            }
            Ok(())
        })
    }
}

// ----- a disk that forgets what it was not told to keep -----

pub const SECTOR: usize = 512;

/// What a power cut does to the writes that no `sync` has covered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashMode {
    /// All of them reached the disk, in order.
    Prefix,
    /// Chosen by the seed: each is lost, whole or torn, and they land in any order.
    Chaos(u64),
}

pub struct FaultDisk {
    durable: MemDisk,
    /// Writes since the last sync, oldest first.
    pending: Vec<(u64, Box<Block>)>,
    writes: u64,
    crash_after: Option<u64>,
    crashed: bool,
}

/// A small seeded generator (splitmix64).
fn scramble(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl FaultDisk {
    pub fn new(blocks: u64) -> FaultDisk {
        FaultDisk::from_image(vec![0; blocks as usize * BS])
    }

    pub fn from_image(image: Vec<u8>) -> FaultDisk {
        FaultDisk {
            durable: MemDisk::from_image(image),
            pending: Vec::new(),
            writes: 0,
            crash_after: None,
            crashed: false,
        }
    }

    /// Counts writes from here. With `Some(k)`, `k` more succeed, and the next write or sync cuts the power.
    pub fn arm(&mut self, crash_after: Option<u64>) {
        self.writes = 0;
        self.crash_after = crash_after;
    }

    /// Successful writes since the disk was made or last armed.
    pub fn writes(&self) -> u64 {
        self.writes
    }

    /// What is on the disk when it is started again after a power cut now.
    pub fn post_crash_image(&self, mode: CrashMode) -> Vec<u8> {
        let mut image = self.durable.image().to_vec();
        let mut order: Vec<usize> = (0..self.pending.len()).collect();
        let mut state = match mode {
            CrashMode::Prefix => 0,
            CrashMode::Chaos(seed) => seed,
        };
        let chaos = matches!(mode, CrashMode::Chaos(_));
        if chaos {
            for i in (1..order.len()).rev() {
                order.swap(i, (scramble(&mut state) % (i as u64 + 1)) as usize);
            }
        }
        for i in order {
            let (n, block) = &self.pending[i];
            let start = *n as usize * BS;
            let fate = if chaos { scramble(&mut state) % 3 } else { 1 };
            if fate == 0 {
                continue;
            }
            for (s, sector) in block.chunks_exact(SECTOR).enumerate() {
                if fate == 1 || scramble(&mut state) % 2 == 0 {
                    let at = start + s * SECTOR;
                    image[at..at + SECTOR].copy_from_slice(sector);
                }
            }
        }
        image
    }

    fn alive(&self) -> R<()> {
        if self.crashed {
            return Err(FsError::Crashed);
        }
        Ok(())
    }
}

impl BlockDevice for FaultDisk {
    fn num_blocks(&self) -> u64 {
        self.durable.num_blocks()
    }

    fn read_block(&mut self, n: u64, buf: &mut Block) -> R<()> {
        self.alive()?;
        match self
            .pending
            .iter()
            .rev()
            .find(|(m, _)| *m == u64::MAX && n == 0)
        {
            Some((_, newest)) => {
                buf.copy_from_slice(&newest[..]);
                Ok(())
            }
            None => self.durable.read_block(n, buf),
        }
    }

    fn write_block(&mut self, n: u64, buf: &Block) -> R<()> {
        self.alive()?;
        if n >= self.num_blocks() {
            return Err(FsError::Io("write past the end"));
        }
        if self.crash_after.is_some_and(|k| self.writes >= k) {
            self.crashed = true;
            return Err(FsError::Crashed);
        }
        self.writes += 1;
        self.pending.push((n, Box::new(*buf)));
        Ok(())
    }

    fn sync(&mut self) -> R<()> {
        self.alive()?;
        // the cut can come just before a sync: then everything since the last one is still in doubt
        if self.crash_after.is_some_and(|k| self.writes >= k) {
            self.crashed = true;
            return Err(FsError::Crashed);
        }
        for (n, block) in self.pending.drain(..) {
            self.durable.write_block(n, &block)?;
        }
        Ok(())
    }
}
