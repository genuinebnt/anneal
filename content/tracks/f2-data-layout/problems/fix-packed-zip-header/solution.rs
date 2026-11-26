/// Starts every local file header: "PK\x03\x04".
pub const SIGNATURE: u32 = 0x0403_4b50;

/// A ZIP local file header exactly as it sits in the archive: 30 bytes, little-endian, at any offset.
/// `packed` removes the padding before `crc32` (offset 14) and drops the alignment to 1, which is what
/// makes `view`'s cast sound at any address. The price: no references to fields, only copies.
#[repr(C, packed)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LocalHeader {
    signature: u32,
    version: u16,
    flags: u16,
    method: u16,
    mod_time: u16,
    mod_date: u16,
    crc32: u32,
    compressed_size: u32,
    uncompressed_size: u32,
    name_len: u16,
    extra_len: u16,
}

impl LocalHeader {
    /// The header at the start of `bytes`, read in place without copying. `None` if `bytes` is shorter
    /// than a header or doesn't start with the signature.
    pub fn view(bytes: &[u8]) -> Option<&LocalHeader> {
        if bytes.len() < std::mem::size_of::<LocalHeader>() {
            return None;
        }
        // SAFETY: `bytes` holds at least size_of::<LocalHeader>() bytes, every bit pattern is a valid
        // LocalHeader (it's all integers), and LocalHeader has alignment 1, so any address is aligned for it.
        let header = unsafe { &*bytes.as_ptr().cast::<LocalHeader>() };
        (header.signature() == SIGNATURE).then_some(header)
    }

    // The archive is little-endian; `from_le` is a no-op on little-endian machines.
    pub fn signature(&self) -> u32 {
        u32::from_le(self.signature)
    }

    pub fn method(&self) -> u16 {
        u16::from_le(self.method)
    }

    pub fn crc32(&self) -> u32 {
        u32::from_le(self.crc32)
    }

    pub fn compressed_size(&self) -> u32 {
        u32::from_le(self.compressed_size)
    }

    pub fn uncompressed_size(&self) -> u32 {
        u32::from_le(self.uncompressed_size)
    }

    pub fn name_len(&self) -> usize {
        u16::from_le(self.name_len) as usize
    }

    pub fn extra_len(&self) -> usize {
        u16::from_le(self.extra_len) as usize
    }

    /// Stored (method 0) entries aren't compressed.
    pub fn is_stored(&self) -> bool {
        self.method() == 0
    }

    /// `crc 0000abcd, 12 -> 34 bytes`, for logs. Accessors copy the fields out; `self.crc32` inside
    /// `format!` would take a reference to an unaligned field (E0793).
    pub fn describe(&self) -> String {
        format!(
            "crc {:08x}, {} -> {} bytes",
            self.crc32(),
            self.compressed_size(),
            self.uncompressed_size()
        )
    }
}

/// One entry of an archive: its name and its (still compressed) data, borrowed from the archive.
#[derive(Debug, PartialEq, Eq)]
pub struct Entry<'a> {
    pub name: &'a [u8],
    pub method: u16,
    pub crc32: u32,
    pub data: &'a [u8],
}

/// The entries at the front of `archive`: a header, the name, the extra field, the data, then the next
/// header, until the bytes no longer start with a local header (the central directory begins). `None` if
/// an entry is cut short.
pub fn entries(archive: &[u8]) -> Option<Vec<Entry<'_>>> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(h) = LocalHeader::view(&archive[at..]) {
        let name_at = at + std::mem::size_of::<LocalHeader>();
        let data_at = name_at + h.name_len() + h.extra_len();
        let end = data_at + h.compressed_size() as usize;
        if end > archive.len() {
            return None;
        }
        out.push(Entry {
            name: &archive[name_at..name_at + h.name_len()],
            method: h.method(),
            crc32: h.crc32(),
            data: &archive[data_at..end],
        });
        at = end;
    }
    Some(out)
}
