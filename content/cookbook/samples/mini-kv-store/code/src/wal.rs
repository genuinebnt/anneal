//! The write-ahead log: a sequence of [len u32][crc32 u32][payload] frames.

use crate::codec::{crc32, decode_entry, encode_entry, Entry, KvError, Result};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;

pub(crate) struct Wal {
    file: File,
    pub(crate) len: u64, // the bytes of whole frames
    broken: bool,        // the file may hold something after `len`: nothing more may be appended
}

/// Parses frames from the start. The first short or checksum-failing frame is treated as the torn tail.
/// Returns (entries, length of the valid prefix).
fn replay(bytes: &[u8]) -> Result<(Vec<Entry>, u64)> {
    let (mut out, mut pos) = (vec![], 0usize);
    while let Some(h) = bytes.get(pos..pos + 8) {
        let len = u32::from_le_bytes([h[0], h[1], h[2], h[3]]) as usize;
        let crc = u32::from_le_bytes([h[4], h[5], h[6], h[7]]);
        // No real record is empty. A zero-filled tail reads as len 0 and crc 0, and crc32(b"") IS 0, so it would pass the checksum.
        if len == 0 {
            break;
        }
        let Some(payload) = bytes.get(pos + 8..pos + 8 + len) else { break };
        if crc32(payload) != crc {
            break;
        }
        // a frame with a VALID checksum that does not decode is a bug or real corruption, not a torn write: surface it
        out.push(decode_entry(payload, &mut 0)?);
        pos += 8 + len;
    }
    Ok((out, pos as u64))
}

impl Wal {
    pub(crate) fn open(path: &Path) -> Result<(Wal, Vec<Entry>)> {
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![],
            Err(e) => return Err(e.into()),
        };
        let (entries, valid) = replay(&bytes)?;
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        file.set_len(valid)?; // chop the torn tail so new appends never follow garbage
        Ok((Wal { file, len: valid, broken: false }, entries))
    }

    pub(crate) fn append(&mut self, key: &[u8], val: Option<&[u8]>) -> Result<()> {
        // no sync_data(): survives a dead process, not a dead machine
        self.append_with(key, val, |file, frame| file.write_all(frame))
    }

    /// `append` with the write passed in, so that a test can make it fail part-way
    pub(crate) fn append_with(
        &mut self,
        key: &[u8],
        val: Option<&[u8]>,
        write: impl FnOnce(&mut File, &[u8]) -> std::io::Result<()>,
    ) -> Result<()> {
        if self.broken {
            return Err(KvError::LogUnusable);
        }
        let mut payload = Vec::new();
        encode_entry(key, val, &mut payload);
        let mut frame = Vec::with_capacity(8 + payload.len());
        frame.extend((payload.len() as u32).to_le_bytes());
        frame.extend(crc32(&payload).to_le_bytes());
        frame.extend(payload);
        if let Err(e) = write(&mut self.file, &frame) {
            // part of the frame may be in the file. Replay would stop there and drop every LATER write, so cut it off now
            self.broken = self.file.set_len(self.len).is_err();
            return Err(e.into());
        }
        self.len += frame.len() as u64;
        Ok(())
    }

    pub(crate) fn reset(&mut self) -> Result<()> {
        if self.broken {
            return Err(KvError::LogUnusable);
        }
        if let Err(e) = self.file.set_len(0) {
            self.broken = true; // the real length is unknown now
            return Err(e.into());
        }
        self.len = 0;
        Ok(())
    }
}
