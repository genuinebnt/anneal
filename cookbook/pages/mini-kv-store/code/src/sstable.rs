//! An immutable sorted table file: [data][index][bloom][footer: data_end, bloom_off, count, crc32, magic]

use crate::bloom::Bloom;
use crate::codec::{crc32, decode_entry, encode_entry, take, take_u32, take_u64, Entry, Key, KvError, Result, Val};
use std::cmp::Ordering;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub const INDEX_EVERY: usize = 16;
const MAGIC: u32 = 0x5353_5431;
pub const FOOTER: usize = 32;

pub struct SsTable {
    pub(crate) id: u64,
    pub(crate) path: PathBuf,
    index: Vec<(Key, u64)>, // sparse: first key of every 16th record -> file offset
    data_end: u64,
    pub(crate) bloom: Bloom,
}

impl SsTable {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn bloom(&self) -> &Bloom {
        &self.bloom
    }

    fn path_for(dir: &Path, id: u64) -> PathBuf {
        dir.join(format!("sst_{id:06}.sst"))
    }

    /// Write to a temp file, fsync it, then rename: a reader never sees a half-written table under its final name.
    /// The directory itself is not synced.
    pub(crate) fn write(dir: &Path, id: u64, entries: &[Entry]) -> Result<SsTable> {
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
        Ok(SsTable { id, path, index, data_end, bloom })
    }

    /// Loads the index and bloom filter into memory; data stays on disk. The whole-file checksum is verified here, once.
    pub(crate) fn open(path: &Path, id: u64) -> Result<SsTable> {
        let bytes = fs::read(path)?;
        let body_len = bytes.len().checked_sub(FOOTER).ok_or(KvError::Corrupt("file too short"))?;
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
        let mut pos = data_end_u;
        let n = take_u32(&bytes, &mut pos)?;
        let mut index = Vec::new();
        for _ in 0..n {
            let klen = take_u32(&bytes, &mut pos)? as usize;
            let key = take(&bytes, &mut pos, klen)?.to_vec();
            index.push((key, take_u64(&bytes, &mut pos)?));
        }
        let bloom = Bloom::decode(&bytes[bloom_off_u..body_len])?;
        Ok(SsTable { id, path: path.to_path_buf(), index, data_end, bloom })
    }

    fn read_range(&self, start: u64, end: u64) -> Result<Vec<u8>> {
        let mut f = File::open(&self.path)?;
        f.seek(SeekFrom::Start(start))?;
        let mut buf = vec![0u8; (end - start) as usize];
        f.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// `Some(None)` = a tombstone lives here (stop searching older tables); `None` = the key is not in this table.
    pub(crate) fn get(&self, key: &[u8]) -> Result<Option<Val>> {
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

    /// Every record in the table, tombstones included, in key order.
    pub fn read_all(&self) -> Result<Vec<Entry>> {
        let buf = self.read_range(0, self.data_end)?;
        let (mut out, mut pos) = (Vec::new(), 0);
        while pos < buf.len() {
            out.push(decode_entry(&buf, &mut pos)?);
        }
        Ok(out)
    }
}
