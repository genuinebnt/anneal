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

fn record(key: &[u8], value: Option<&[u8]>) -> Vec<u8> {
    let vlen = value.map_or(TOMBSTONE, |v| v.len() as u32);
    let mut body = Vec::new();
    body.extend_from_slice(&(key.len() as u32).to_le_bytes());
    body.extend_from_slice(&vlen.to_le_bytes());
    body.extend_from_slice(key);
    if let Some(v) = value {
        body.extend_from_slice(v);
    }
    let mut rec = checksum(&body[8..]).to_le_bytes().to_vec();
    rec.extend_from_slice(&body);
    rec
}

impl Store {
    pub fn new() -> Store {
        Store { map: BTreeMap::new(), log: Vec::new() }
    }

    pub fn put(&mut self, key: &[u8], value: &[u8]) {
        self.log.extend_from_slice(&record(key, Some(value))); // log first, then apply
        self.map.insert(key.to_vec(), value.to_vec());
    }

    pub fn delete(&mut self, key: &[u8]) {
        self.log.extend_from_slice(&record(key, None));
        self.map.remove(key);
    }

    pub fn get(&self, key: &[u8]) -> Option<&[u8]> {
        self.map.get(key).map(|v| v.as_slice())
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn log(&self) -> &[u8] {
        &self.log
    }

    pub fn recover(log: &[u8]) -> (Store, usize) {
        let mut map = BTreeMap::new();
        let mut at = 0usize;
        while log.len() - at >= 12 {
            let crc = u32::from_le_bytes(log[at..at + 4].try_into().unwrap());
            let klen = u32::from_le_bytes(log[at + 4..at + 8].try_into().unwrap());
            let vlen = u32::from_le_bytes(log[at + 8..at + 12].try_into().unwrap());
            let value_len = if vlen == TOMBSTONE { 0 } else { vlen as usize };
            let body_len = 8 + klen as usize + value_len; // klen and vlen fields, key, value
            let end = at + 4 + body_len;
            if end > log.len() {
                break; // torn: the record claims more bytes than exist
            }
            if checksum(&log[at + 4..end]) != crc {
                break; // corrupt
            }
            let key = log[at + 12..at + 12 + klen as usize].to_vec();
            if vlen == TOMBSTONE {
                map.remove(&key);
            } else {
                map.insert(key, log[at + 12 + klen as usize..end].to_vec());
            }
            at = end;
        }
        (Store { map, log: log[..at].to_vec() }, at)
    }

    pub fn compact(&mut self) {
        let mut log = Vec::new();
        for (k, v) in &self.map {
            log.extend_from_slice(&record(k, Some(v)));
        }
        self.log = log;
    }
}
