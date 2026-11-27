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
        let start = n as usize * BS;
        (start + BS <= self.data.len()).then_some(start)
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
