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
        todo!()
    }

    /// A disk holding these bytes. Bytes after the last whole block are not part of it.
    pub fn from_image(data: Vec<u8>) -> MemDisk {
        todo!()
    }

    pub fn image(&self) -> &[u8] {
        todo!()
    }
}

impl BlockDevice for MemDisk {
    fn num_blocks(&self) -> u64 {
        todo!()
    }
    fn read_block(&mut self, n: u64, buf: &mut Block) -> R<()> {
        todo!()
    }
    fn write_block(&mut self, n: u64, buf: &Block) -> R<()> {
        todo!()
    }
    fn sync(&mut self) -> R<()> {
        todo!()
    }
}

/// CRC-32 of more bytes: `crc32_extend(crc32(a), b) == crc32(a followed by b)`.
pub fn crc32_extend(prev: u32, data: &[u8]) -> u32 {
    todo!()
}

pub fn crc32(data: &[u8]) -> u32 {
    todo!()
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
        todo!()
    }

    pub fn encode(&self) -> Box<Block> {
        todo!()
    }

    pub fn decode(b: &Block) -> R<Super> {
        todo!()
    }
}
