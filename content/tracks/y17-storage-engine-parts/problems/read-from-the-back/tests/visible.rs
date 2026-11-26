use solution::*;

use std::cell::Cell;
use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::rc::Rc;

/// The directory and footer for `entries`, written after `data`.
#[allow(dead_code)]
fn with_directory(data: &[u8], entries: &[(&[u8], u64, u64)]) -> Vec<u8> {
    let mut out = data.to_vec();
    out.extend((entries.len() as u32).to_le_bytes());
    for (name, offset, len) in entries {
        out.extend((name.len() as u16).to_le_bytes());
        out.extend_from_slice(name);
        out.extend(offset.to_le_bytes());
        out.extend(len.to_le_bytes());
    }
    out.extend((data.len() as u64).to_le_bytes());
    out.extend_from_slice(b"BND1");
    out
}

/// A well-formed bundle of these entries, one after another.
#[allow(dead_code)]
fn bundle(blobs: &[(&str, &[u8])]) -> Vec<u8> {
    let mut data = Vec::new();
    let mut entries = Vec::new();
    for (name, blob) in blobs {
        entries.push((name.as_bytes(), data.len() as u64, blob.len() as u64));
        data.extend_from_slice(blob);
    }
    with_directory(&data, &entries)
}

/// Counts the bytes handed out, and hands out at most `chunk` at a time, as a real file may.
#[allow(dead_code)]
struct Counting {
    inner: Cursor<Vec<u8>>,
    read: Rc<Cell<u64>>,
    chunk: usize,
}

impl Read for Counting {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = buf.len().min(self.chunk);
        let got = self.inner.read(&mut buf[..n])?;
        self.read.set(self.read.get() + got as u64);
        Ok(got)
    }
}

impl Seek for Counting {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        self.inner.seek(to)
    }
}

#[allow(dead_code)]
fn counting(bytes: Vec<u8>, chunk: usize) -> (Counting, Rc<Cell<u64>>) {
    let read = Rc::new(Cell::new(0));
    (Counting { inner: Cursor::new(bytes), read: read.clone(), chunk }, read)
}

#[allow(dead_code)]
fn error_of(bytes: Vec<u8>) -> Option<BundleError> {
    Bundle::open(Cursor::new(bytes)).err()
}

#[test]
fn read_two_entries() {
    let mut b = Bundle::open(Cursor::new(bundle(&[("a.txt", b"alpha"), ("b", b"")]))).unwrap();
    check!("two entries", (b.get("a.txt"), b.get("b"), b.get("c")), (Ok(Some(b"alpha".to_vec())), Ok(Some(vec![])), Ok(None)));
}

#[test]
fn the_names_in_directory_order() {
    let b = Bundle::open(Cursor::new(bundle(&[("z", b"1"), ("a", b"2")]))).unwrap();
    check!("z then a", b.names(), vec!["z", "a"]);
}

#[test]
fn the_exact_bytes() {
    let bytes = [b'h', b'i', 1, 0, 0, 0, 1, 0, b'k', 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, b'B', b'N', b'D', b'1'];
    let mut b = Bundle::open(Cursor::new(bytes.to_vec())).unwrap();
    check!(format!("{bytes:?}"), b.get("k"), Ok(Some(b"hi".to_vec())));
}

#[test]
fn only_what_is_needed_is_read() {
    let big = vec![7u8; 1_000_000];
    let bytes = bundle(&[("big", &big), ("small", b"tiny")]);
    let (reader, read) = counting(bytes, usize::MAX);
    let mut b = Bundle::open(reader).unwrap();
    // 12 for the footer; the directory is 4 + (2+3+16) + (2+5+16)
    check!("open: footer and directory", read.get() <= 12 + 48, true);
    let before = read.get();
    check!("get small", b.get("small"), Ok(Some(b"tiny".to_vec())));
    check!("bytes read by that get", read.get() - before, 4);
}

#[test]
fn too_short_is_corrupt() {
    check!("an empty file", error_of(vec![]), Some(BundleError::Corrupt));
}
