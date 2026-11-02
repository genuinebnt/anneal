use std::collections::BTreeMap;

/// FNV-1a, 32 bit: the record checksum. (Provided.)
pub fn checksum(data: &[u8]) -> u32 {
    let mut h: u32 = 0x811C_9DC5;
    for &b in data {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// `vlen` value that marks a delete (a tombstone): the record has no value bytes.
pub const TOMBSTONE: u32 = u32::MAX;

/// A key-value store that logs every change to a byte-vector write-ahead log before it applies it.
///
/// Log record: `[crc: u32][klen: u32][vlen: u32][key][value]`, integers little-endian. `vlen == TOMBSTONE` means delete (no value
/// bytes follow). `crc` is `checksum` of everything after the crc field (klen, vlen, key, value).
pub struct Store {
    map: BTreeMap<Vec<u8>, Vec<u8>>,
    log: Vec<u8>,
}

impl Store {
    pub fn new() -> Store {
        todo!()
    }

    /// Appends a put record to the log, then updates the map.
    pub fn put(&mut self, key: &[u8], value: &[u8]) {
        todo!()
    }

    /// Appends a tombstone record, then removes the key. (Deleting a missing key still logs it.)
    pub fn delete(&mut self, key: &[u8]) {
        todo!()
    }

    pub fn get(&self, key: &[u8]) -> Option<&[u8]> {
        todo!()
    }

    /// The number of live keys.
    pub fn len(&self) -> usize {
        todo!()
    }

    /// The write-ahead log: what would be on disk.
    pub fn log(&self) -> &[u8] {
        todo!()
    }

    /// Rebuilds a store by replaying `log` from the start. Stops at the first record that is incomplete (a torn write at the
    /// tail) or whose checksum is wrong, and ignores everything from there on. Returns the store and the number of bytes that
    /// were valid. The returned store's own log is exactly that valid prefix, so later writes append cleanly.
    pub fn recover(log: &[u8]) -> (Store, usize) {
        todo!()
    }

    /// Rewrites the log so it holds one put record per live key (in key order) and nothing else.
    pub fn compact(&mut self) {
        todo!()
    }
}
