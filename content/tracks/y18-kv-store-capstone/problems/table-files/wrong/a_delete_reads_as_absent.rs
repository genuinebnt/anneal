use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum KvError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("corrupt data: {0}")]
    Corrupt(&'static str),
    #[error("the log is unusable after an earlier failure")]
    Unusable,
}

pub type Result<T> = std::result::Result<T, KvError>;

pub type Key = Vec<u8>;
/// `None` says the key was deleted.
pub type Entry = (Key, Option<Vec<u8>>);

/// CRC-32 as used by zlib, Ethernet and PNG (reflected polynomial 0xEDB88320).
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

/// The next `n` bytes, moving `pos` past them. Every read of bytes from a file goes through here.
fn take<'a>(buf: &'a [u8], pos: &mut usize, n: usize) -> Result<&'a [u8]> {
    let end = pos
        .checked_add(n)
        .ok_or(KvError::Corrupt("length overflow"))?;
    let bytes = buf.get(*pos..end).ok_or(KvError::Corrupt("short buffer"))?;
    *pos = end;
    Ok(bytes)
}

fn take_u32(buf: &[u8], pos: &mut usize) -> Result<u32> {
    let bytes = take(buf, pos, 4)?;
    Ok(u32::from_le_bytes(
        bytes.try_into().map_err(|_| KvError::Corrupt("u32"))?,
    ))
}

pub fn encode_record(key: &[u8], value: Option<&[u8]>, out: &mut Vec<u8>) {
    out.push(u8::from(value.is_some()));
    out.extend((key.len() as u32).to_le_bytes());
    out.extend_from_slice(key);
    let value = value.unwrap_or(&[]);
    out.extend((value.len() as u32).to_le_bytes());
    out.extend_from_slice(value);
}

pub fn decode_record(buf: &[u8], pos: &mut usize) -> Result<Entry> {
    let mut at = *pos;
    let tag = take(buf, &mut at, 1)?[0];
    let key_len = take_u32(buf, &mut at)? as usize;
    let key = take(buf, &mut at, key_len)?;
    let value_len = take_u32(buf, &mut at)? as usize;
    let value = take(buf, &mut at, value_len)?;
    let value = match tag {
        1 => Some(value.to_vec()),
        0 if value.is_empty() => None,
        _ => return Err(KvError::Corrupt("bad record tag")),
    };
    *pos = at;
    Ok((key.to_vec(), value))
}

pub fn encode_frame(key: &[u8], value: Option<&[u8]>) -> Vec<u8> {
    let mut record = Vec::new();
    encode_record(key, value, &mut record);
    let mut frame = Vec::with_capacity(8 + record.len());
    frame.extend((record.len() as u32).to_le_bytes());
    frame.extend(crc32(&record).to_le_bytes());
    frame.extend(record);
    frame
}

/// The records of every whole frame at the front of `log`, and the number of bytes those frames take.
pub fn replay(log: &[u8]) -> Result<(Vec<Entry>, usize)> {
    let mut entries = Vec::new();
    let mut valid = 0;
    loop {
        let mut pos = valid;
        let (Ok(len), Ok(crc)) = (take_u32(log, &mut pos), take_u32(log, &mut pos)) else {
            break;
        };
        // no record is empty; eight zero bytes would otherwise pass, because crc32 of nothing is 0
        if len == 0 {
            break;
        }
        let Ok(payload) = take(log, &mut pos, len as usize) else {
            break;
        };
        if crc32(payload) != crc {
            break;
        }
        // the checksum holds, so these are the bytes that were written: if they make no sense, that is not a torn write
        let mut used = 0;
        let entry = decode_record(payload, &mut used)?;
        if used != payload.len() {
            return Err(KvError::Corrupt("bytes after the record"));
        }
        entries.push(entry);
        valid = pos;
    }
    Ok((entries, valid))
}

/// An append-only file, or a stand-in for one.
pub trait LogFile {
    /// Everything in it.
    fn read_all(&mut self) -> io::Result<Vec<u8>>;
    /// Adds `bytes` at the end. When it fails, any prefix of `bytes` may have been added.
    fn append(&mut self, bytes: &[u8]) -> io::Result<()>;
    fn truncate(&mut self, len: u64) -> io::Result<()>;
    /// Makes what was added so far survive a power cut.
    fn sync(&mut self) -> io::Result<()>;
}

/// For a file opened in append mode.
impl LogFile for File {
    fn read_all(&mut self) -> io::Result<Vec<u8>> {
        self.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        self.read_to_end(&mut bytes)?;
        Ok(bytes)
    }
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.write_all(bytes)
    }
    fn truncate(&mut self, len: u64) -> io::Result<()> {
        self.set_len(len)
    }
    fn sync(&mut self) -> io::Result<()> {
        self.sync_data()
    }
}

/// The write-ahead log.
pub struct Wal<F: LogFile = File> {
    file: F,
    len: u64,
    broken: bool,
}

impl Wal<File> {
    /// Opens the log at `path`, creating it if it is missing, and returns the records it holds.
    pub fn open(path: &Path) -> Result<(Wal<File>, Vec<Entry>)> {
        let file = OpenOptions::new()
            .read(true)
            .create(true)
            .append(true)
            .open(path)?;
        Wal::with_file(file)
    }
}

impl<F: LogFile> Wal<F> {
    pub fn with_file(mut file: F) -> Result<(Wal<F>, Vec<Entry>)> {
        let bytes = file.read_all()?;
        let (entries, valid) = replay(&bytes)?;
        // cut the torn tail off, so that new frames never land behind it
        if valid < bytes.len() {
            file.truncate(valid as u64)?;
        }
        let wal = Wal {
            file,
            len: valid as u64,
            broken: false,
        };
        Ok((wal, entries))
    }

    pub fn append(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<()> {
        self.usable()?;
        let frame = encode_frame(key, value);
        if let Err(error) = self.file.append(&frame) {
            if self.file.truncate(self.len).is_err() {
                self.broken = true;
            }
            return Err(error.into());
        }
        self.len += frame.len() as u64;
        Ok(())
    }

    pub fn sync(&mut self) -> Result<()> {
        self.usable()?;
        self.file.sync().map_err(|error| {
            self.broken = true;
            error.into()
        })
    }

    /// Empties the log.
    pub fn reset(&mut self) -> Result<()> {
        self.usable()?;
        if let Err(error) = self.file.truncate(0) {
            self.broken = true;
            return Err(error.into());
        }
        self.len = 0;
        Ok(())
    }

    /// The number of bytes in the log.
    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn usable(&self) -> Result<()> {
        if self.broken {
            Err(KvError::Unusable)
        } else {
            Ok(())
        }
    }
}

/// "Is this key possibly in the file?" (given: you built it in "Fix: a saved filter that forgets its keys")
struct Filter {
    bits: Vec<u8>,
    probes: u8,
}

fn filter_hash(key: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &byte in key {
        h = (h ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    h ^ (h >> 32)
}

fn filter_positions(key: &[u8], probes: u8, bits: usize) -> impl Iterator<Item = usize> {
    let hash = filter_hash(key);
    let step = hash.rotate_left(31) | 1;
    (0..u64::from(probes))
        .map(move |i| (hash.wrapping_add(i.wrapping_mul(step)) % (bits as u64 * 8)) as usize)
}

impl Filter {
    fn new<'a>(keys: impl ExactSizeIterator<Item = &'a [u8]>) -> Filter {
        let bytes = (keys.len() * 10).max(64).div_ceil(8);
        let mut bits = vec![0u8; bytes];
        for key in keys {
            for p in filter_positions(key, 4, bytes) {
                bits[p / 8] |= 1 << (p % 8);
            }
        }
        Filter { bits, probes: 4 }
    }

    fn may_contain(&self, key: &[u8]) -> bool {
        filter_positions(key, self.probes, self.bits.len())
            .all(|p| self.bits[p / 8] & (1 << (p % 8)) != 0)
    }

    fn save(&self, out: &mut Vec<u8>) {
        out.push(self.probes);
        out.extend((self.bits.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.bits);
    }

    fn load(bytes: &[u8]) -> Option<Filter> {
        let (&probes, rest) = bytes.split_first()?;
        let len = u32::from_le_bytes(rest.get(..4)?.try_into().ok()?) as usize;
        let bits = &rest[4..];
        (probes > 0 && len > 0 && bits.len() == len).then(|| Filter {
            bits: bits.to_vec(),
            probes,
        })
    }
}

fn take_u64(buf: &[u8], pos: &mut usize) -> Result<u64> {
    let bytes = take(buf, pos, 8)?;
    Ok(u64::from_le_bytes(
        bytes.try_into().map_err(|_| KvError::Corrupt("u64"))?,
    ))
}
/// Every this-many records, the index notes a key and where its record starts.
pub const BLOCK_RECORDS: usize = 16;
const MAGIC: u32 = 0x5353_5431;
const FOOTER: usize = 32;

/// An immutable file of sorted records. Only the index and the filter are kept in memory.
pub struct SsTable {
    id: u64,
    path: PathBuf,
    /// the first key of each block and the offset it starts at
    index: Vec<(Key, u64)>,
    data_end: u64,
    filter: Filter,
}

impl SsTable {
    /// Writes `entries`, which are sorted by key with no key twice, as table `id` in `dir`.
    pub fn write(dir: &Path, id: u64, entries: &[Entry]) -> Result<SsTable> {
        let mut bytes = Vec::new();
        let mut index = Vec::new();
        for (i, (key, value)) in entries.iter().enumerate() {
            if i % BLOCK_RECORDS == 0 {
                index.push((key.clone(), bytes.len() as u64));
            }
            encode_record(key, value.as_deref(), &mut bytes);
        }
        let data_end = bytes.len() as u64;
        bytes.extend((index.len() as u32).to_le_bytes());
        for (key, offset) in &index {
            bytes.extend((key.len() as u32).to_le_bytes());
            bytes.extend_from_slice(key);
            bytes.extend(offset.to_le_bytes());
        }
        let filter_start = bytes.len() as u64;
        let filter = Filter::new(entries.iter().map(|e| e.0.as_slice()));
        filter.save(&mut bytes);
        for number in [data_end, filter_start, entries.len() as u64] {
            bytes.extend(number.to_le_bytes());
        }
        let crc = crc32(&bytes);
        bytes.extend(crc.to_le_bytes());
        bytes.extend(MAGIC.to_le_bytes());

        let path = dir.join(format!("sst_{id:06}.sst"));
        let tmp = path.with_extension("tmp");
        let mut file = File::create(&tmp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, &path)?;
        File::open(dir)?.sync_all()?;
        Ok(SsTable {
            id,
            path,
            index,
            data_end,
            filter,
        })
    }

    pub fn open(path: &Path, id: u64) -> Result<SsTable> {
        let bytes = fs::read(path)?;
        let body = bytes
            .len()
            .checked_sub(FOOTER)
            .ok_or(KvError::Corrupt("file too short"))?;
        let mut pos = body;
        let data_end = take_u64(&bytes, &mut pos)?;
        let filter_start = take_u64(&bytes, &mut pos)?;
        let _records = take_u64(&bytes, &mut pos)?;
        let crc_at = pos;
        let crc = take_u32(&bytes, &mut pos)?;
        if take_u32(&bytes, &mut pos)? != MAGIC || crc != crc32(&bytes[..crc_at]) {
            return Err(KvError::Corrupt("bad footer or checksum"));
        }
        // a good checksum does not make the numbers sensible
        if data_end > filter_start || filter_start > body as u64 {
            return Err(KvError::Corrupt("bad offsets"));
        }
        let index_bytes = &bytes[data_end as usize..filter_start as usize];
        let mut pos = 0;
        let count = take_u32(index_bytes, &mut pos)?;
        let mut index: Vec<(Key, u64)> = Vec::new();
        for _ in 0..count {
            let len = take_u32(index_bytes, &mut pos)? as usize;
            let key = take(index_bytes, &mut pos, len)?.to_vec();
            let offset = take_u64(index_bytes, &mut pos)?;
            let in_order = match index.last() {
                None => offset == 0,
                Some((last_key, last)) => *last_key < key && *last < offset,
            };
            if !in_order || offset >= data_end {
                return Err(KvError::Corrupt("bad index"));
            }
            index.push((key, offset));
        }
        if pos != index_bytes.len() || index.is_empty() != (data_end == 0) {
            return Err(KvError::Corrupt("bad index"));
        }
        let filter = Filter::load(&bytes[filter_start as usize..body])
            .ok_or(KvError::Corrupt("bad filter"))?;
        Ok(SsTable {
            id,
            path: path.to_path_buf(),
            index,
            data_end,
            filter,
        })
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The records in `start..end` of the file.
    fn read_records(&self, start: u64, end: u64) -> Result<Vec<Entry>> {
        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(start))?;
        let mut bytes = vec![0u8; (end - start) as usize];
        file.read_exact(&mut bytes)?;
        let mut entries = Vec::new();
        let mut pos = 0;
        while pos < bytes.len() {
            entries.push(decode_record(&bytes, &mut pos)?);
        }
        Ok(entries)
    }

    /// `None`: this table says nothing about the key. `Some(None)`: it says the key was deleted.
    pub fn get(&self, key: &[u8]) -> Result<Option<Option<Vec<u8>>>> {
        let before = self.index.partition_point(|(first, _)| first.as_slice() <= key);
        if before == 0 || !self.filter.may_contain(key) {
            return Ok(None);
        }
        let start = self.index[before - 1].1;
        let end = self.index.get(before).map_or(self.data_end, |next| next.1);
        let block = self.read_records(start, end)?;
        Ok(block
            .into_iter()
            .find(|e| e.0 == key)
            .and_then(|e| e.1)
            .map(Some))
    }

    /// Every record, delete markers included, in key order.
    pub fn read_all(&self) -> Result<Vec<Entry>> {
        self.read_records(0, self.data_end)
    }
}
