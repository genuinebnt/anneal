use std::io::{self, Read, Seek, SeekFrom};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum BundleError {
    /// The reader failed.
    Io(io::ErrorKind),
    /// The bytes are not a well-formed bundle.
    Corrupt,
}

impl From<io::Error> for BundleError {
    fn from(error: io::Error) -> BundleError {
        BundleError::Io(error.kind())
    }
}

const MAGIC: &[u8; 4] = b"BND1";
const FOOTER: u64 = 12;

pub struct Bundle<R: Read + Seek> {
    reader: R,
    /// name, offset, length; every range lies in front of the directory
    entries: Vec<(String, u64, u64)>,
}

/// The next `n` bytes of `bytes`, which is moved past them.
fn take<'a>(bytes: &mut &'a [u8], n: usize) -> Result<&'a [u8], BundleError> {
    if bytes.len() < n {
        return Err(BundleError::Corrupt);
    }
    let (front, rest) = bytes.split_at(n);
    *bytes = rest;
    Ok(front)
}

fn number<const N: usize>(bytes: &mut &[u8]) -> Result<[u8; N], BundleError> {
    take(bytes, N)?.try_into().map_err(|_| BundleError::Corrupt)
}

impl<R: Read + Seek> Bundle<R> {
    pub fn open(mut reader: R) -> Result<Bundle<R>, BundleError> {
        let size = reader.seek(SeekFrom::End(0))?;
        let body = size.checked_sub(FOOTER).ok_or(BundleError::Corrupt)?;
        reader.seek(SeekFrom::Start(body))?;
        let mut footer = [0u8; FOOTER as usize];
        reader.read_exact(&mut footer)?;
        let mut rest = &footer[..];
        let start = u64::from_le_bytes(number(&mut rest)?);
        if rest != MAGIC || start > body {
            return Err(BundleError::Corrupt);
        }
        // the size of this buffer is bounded by the size of the file, which is real
        let mut directory = vec![0u8; (body - start) as usize];
        reader.seek(SeekFrom::Start(start))?;
        reader.read_exact(&mut directory)?;
        let mut rest = &directory[..];
        let count = u32::from_le_bytes(number(&mut rest)?);
        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for _ in 0..count {
            let name_len = u16::from_le_bytes(number(&mut rest)?) as usize;
            let name = std::str::from_utf8(take(&mut rest, name_len)?)
                .map_err(|_| BundleError::Corrupt)?;
            let offset = u64::from_le_bytes(number(&mut rest)?);
            let len = u64::from_le_bytes(number(&mut rest)?);
            if offset.wrapping_add(len) > start || !seen.insert(name) {
                return Err(BundleError::Corrupt);
            }
            entries.push((name.to_string(), offset, len));
        }
        if !rest.is_empty() {
            return Err(BundleError::Corrupt);
        }
        Ok(Bundle { reader, entries })
    }

    /// The names, in the order the directory lists them.
    pub fn names(&self) -> Vec<&str> {
        self.entries.iter().map(|e| e.0.as_str()).collect()
    }

    /// The bytes stored under `name`, or `None` if there is no such entry.
    pub fn get(&mut self, name: &str) -> Result<Option<Vec<u8>>, BundleError> {
        let Some(&(_, offset, len)) = self.entries.iter().find(|e| e.0 == name) else {
            return Ok(None);
        };
        self.reader.seek(SeekFrom::Start(offset))?;
        let mut bytes = vec![0u8; len as usize];
        self.reader.read_exact(&mut bytes)?;
        Ok(Some(bytes))
    }
}
