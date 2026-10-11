//! Bytes, endianness and positional file I/O: the first Rust a storage engine needs. A file of fixed-size pages is read and written
//! at byte offsets (no seeking, no `&mut`), and structures become bytes by hand.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::FileExt;
use std::path::Path;

/// The size of a page of the [`PageFile`].
pub const PAGE: usize = 256;

/// A header of 16 bytes: a magic number, a version, flags and a length, little-endian, in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub magic: u32,
    pub version: u16,
    pub flags: u16,
    pub len: u64,
}

impl Header {
    pub const SIZE: usize = 16;

    /// `magic` (4 bytes), `version` (2), `flags` (2), `len` (8), each little-endian.
    pub fn to_bytes(&self) -> [u8; Header::SIZE] {
        // @begin r-01
        let mut out = [0u8; Header::SIZE];
        out[0..4].copy_from_slice(&self.magic.to_le_bytes());
        out[4..6].copy_from_slice(&self.version.to_le_bytes());
        out[6..8].copy_from_slice(&self.flags.to_le_bytes());
        out[8..16].copy_from_slice(&self.len.to_le_bytes());
        out
        //~ todo!("r-01: the four fields, little-endian, one after the other (copy_from_slice into ranges of a [u8; 16])")
        // @end
    }

    /// The inverse of `to_bytes`: `None` if `bytes` is not exactly 16 bytes long.
    pub fn from_bytes(bytes: &[u8]) -> Option<Header> {
        // @begin r-01
        if bytes.len() != Header::SIZE {
            return None;
        }
        Some(Header {
            magic: u32::from_le_bytes(bytes[0..4].try_into().ok()?),
            version: u16::from_le_bytes(bytes[4..6].try_into().ok()?),
            flags: u16::from_le_bytes(bytes[6..8].try_into().ok()?),
            len: u64::from_le_bytes(bytes[8..16].try_into().ok()?),
        })
        //~ todo!("r-01: None unless there are exactly 16 bytes; otherwise the fields are read back in the order to_bytes wrote them")
        // @end
    }
}

/// Packs the numbers into bytes, four each, little-endian.
pub fn pack_u32s(values: &[u32]) -> Vec<u8> {
    // @begin r-01
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
    //~ todo!("r-01: every value as 4 little-endian bytes, in order")
    // @end
}

/// The inverse of `pack_u32s`; a last group of fewer than 4 bytes is ignored.
pub fn unpack_u32s(bytes: &[u8]) -> Vec<u32> {
    // @begin r-01
    bytes.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect()
    //~ todo!("r-01: chunks_exact(4) and u32::from_le_bytes")
    // @end
}

/// A file of pages of `PAGE` bytes, addressed by page number. Pages are read and written **at an offset** (`read_at`, `write_all_at`): no
/// seek, no `&mut self`, so many threads can use one file. A page that was never written reads as zeros; so does the part of a
/// page beyond the end of the file.
pub struct PageFile {
    // @begin r-01
    file: File,
    //~ _pages: (),
    //~ // TODO(r-01): your fields: the open file
    // @end
}

impl PageFile {
    /// Opens the file for reading and writing, creating it if it is missing and keeping its contents if it is there.
    pub fn open(path: impl AsRef<Path>) -> io::Result<PageFile> {
        // @begin r-01
        Ok(PageFile { file: OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path)? })
        //~ todo!("r-01: OpenOptions with read, write and create (and not truncate)")
        // @end
    }

    /// Writes page number `index` (at byte `index * PAGE`).
    pub fn write_page(&self, index: u64, data: &[u8; PAGE]) -> io::Result<()> {
        // @begin r-01
        self.file.write_all_at(data, index * PAGE as u64)
        //~ todo!("r-01: write_all_at the page's offset")
        // @end
    }

    /// Reads page number `index`; zeros where the file has nothing.
    pub fn read_page(&self, index: u64) -> io::Result<[u8; PAGE]> {
        // @begin r-01
        let mut page = [0u8; PAGE];
        let mut filled = 0;
        while filled < PAGE {
            // read_at may return fewer bytes than asked for, and 0 at the end of the file
            let n = self.file.read_at(&mut page[filled..], index * PAGE as u64 + filled as u64)?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        Ok(page)
        //~ todo!("r-01: the page's bytes, reading until the page is full or the file ends; what the file lacks is zero")
        // @end
    }

    /// The number of pages the file holds, counting a last partly written page.
    pub fn page_count(&self) -> io::Result<u64> {
        // @begin r-01
        Ok(self.file.metadata()?.len().div_ceil(PAGE as u64))
        //~ todo!("r-01: the file's length divided by PAGE, rounded up")
        // @end
    }
}
