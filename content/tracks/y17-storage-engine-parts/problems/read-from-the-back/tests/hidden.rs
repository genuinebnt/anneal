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

#[allow(dead_code)]
struct TempDir(std::path::PathBuf);

#[allow(dead_code)]
impl TempDir {
    fn new(tag: &str) -> TempDir {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("anneal-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
    fn join(&self, name: &str) -> std::path::PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn no_entries() {
    let b = Bundle::open(Cursor::new(bundle(&[]))).unwrap();
    check!("count 0", b.names().len(), 0);
}

#[test]
fn short_reads_are_normal() {
    for chunk in [1, 2, 7] {
        let (reader, _) = counting(bundle(&[("one", b"0123456789"), ("two", b"abc")]), chunk);
        let got = Bundle::open(reader).and_then(|mut b| Ok((b.get("two")?, b.get("one")?)));
        check!(format!("at most {chunk} bytes per read"), got, Ok((Some(b"abc".to_vec()), Some(b"0123456789".to_vec()))));
    }
}

#[test]
fn every_shortened_file_is_corrupt() {
    let whole = bundle(&[("a", b"hello"), ("bb", b"world!")]);
    for keep in 0..whole.len() {
        check!(format!("first {keep} bytes"), error_of(whole[..keep].to_vec()), Some(BundleError::Corrupt));
    }
}

#[test]
fn wrong_magic_and_a_directory_past_the_end() {
    let mut bytes = bundle(&[("a", b"x")]);
    *bytes.last_mut().unwrap() = b'2';
    check!("BND2", error_of(bytes), Some(BundleError::Corrupt));
    for start in [u64::MAX, u64::MAX - 11, 1 << 40, 1000] {
        let mut bytes = bundle(&[("a", b"x")]);
        let at = bytes.len() - 12;
        bytes[at..at + 8].copy_from_slice(&start.to_le_bytes());
        check!(format!("directory offset {start}"), error_of(bytes), Some(BundleError::Corrupt));
    }
}

#[test]
fn entries_must_stay_in_front_of_the_directory() {
    for (offset, len) in [(0, 6), (5, 1), (6, 0), (1, u64::MAX), (u64::MAX, 2), (u64::MAX, u64::MAX), (0, 1 << 50)] {
        let bytes = with_directory(b"hello", &[(b"k", offset, len)]);
        check!(format!("offset {offset}, length {len}, data is 5 bytes"), error_of(bytes), Some(BundleError::Corrupt));
    }
    for (offset, len) in [(0, 5), (5, 0), (4, 1), (0, 0)] {
        let bytes = with_directory(b"hello", &[(b"k", offset, len)]);
        check!(format!("offset {offset}, length {len} is fine"), error_of(bytes), None);
    }
}

/// Remembers the largest single request for memory.
struct Largest;

static LARGEST: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

unsafe impl std::alloc::GlobalAlloc for Largest {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        LARGEST.fetch_max(layout.size(), std::sync::atomic::Ordering::SeqCst);
        unsafe { std::alloc::System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        unsafe { std::alloc::System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, size: usize) -> *mut u8 {
        LARGEST.fetch_max(size, std::sync::atomic::Ordering::SeqCst);
        unsafe { std::alloc::System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: Largest = Largest;

#[test]
fn no_allocation_much_larger_than_the_file() {
    // no test here handles more than a few kilobytes, so anything past 4 MB was sized by an unchecked number
    let mut counts = bundle(&[("a", b"x")]);
    counts[1..5].copy_from_slice(&(1u32 << 24).to_le_bytes());
    let _ = Bundle::open(Cursor::new(counts));
    let mut lengths = with_directory(b"x", &[(b"a", 0, 1 << 26)]);
    if let Ok(mut b) = Bundle::open(Cursor::new(lengths.clone())) {
        let _ = b.get("a");
    }
    let at = lengths.len() - 12;
    lengths[at..at + 8].copy_from_slice(&(1u64 << 26).to_le_bytes());
    let _ = Bundle::open(Cursor::new(lengths));
    let largest = LARGEST.load(std::sync::atomic::Ordering::SeqCst);
    check!(format!("largest single allocation: {largest} bytes"), largest < 4 << 20, true);
}

#[test]
fn a_count_the_directory_cannot_hold() {
    for count in [2u32, 1000, u32::MAX] {
        let mut bytes = bundle(&[("a", b"x")]);
        bytes[1..5].copy_from_slice(&count.to_le_bytes());
        check!(format!("count says {count}"), error_of(bytes), Some(BundleError::Corrupt));
    }
    let mut bytes = bundle(&[("a", b"x"), ("b", b"y")]);
    bytes[2..6].copy_from_slice(&1u32.to_le_bytes());
    check!("count says 1, bytes of a second entry are left over", error_of(bytes), Some(BundleError::Corrupt));
}

#[test]
fn bad_names() {
    check!("not UTF-8", error_of(with_directory(b"", &[(&[0xff], 0, 0)])), Some(BundleError::Corrupt));
    check!("the same name twice", error_of(with_directory(b"ab", &[(b"n", 0, 1), (b"n", 1, 1)])), Some(BundleError::Corrupt));
    let mut b = Bundle::open(Cursor::new(with_directory(b"ab", &[("é".as_bytes(), 1, 1), (b"", 0, 2)]))).unwrap();
    check!("an empty name and a two-byte one", (b.get("é"), b.get("")), (Ok(Some(b"b".to_vec())), Ok(Some(b"ab".to_vec()))));
}

struct Broken;
impl Read for Broken {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::from(io::ErrorKind::PermissionDenied))
    }
}
impl Seek for Broken {
    fn seek(&mut self, _: SeekFrom) -> io::Result<u64> {
        Ok(100)
    }
}

#[test]
fn reader_errors_are_not_called_corruption() {
    check!("every read fails", Bundle::open(Broken).err(), Some(BundleError::Io(io::ErrorKind::PermissionDenied)));
}

#[test]
fn a_real_file() {
    let dir = TempDir::new("bundle");
    let path = dir.join("b.bin");
    std::fs::write(&path, bundle(&[("x", b"from disk")])).unwrap();
    let mut b = Bundle::open(std::fs::File::open(&path).unwrap()).unwrap();
    check!("through a File, twice", (b.get("x"), b.get("x")), (Ok(Some(b"from disk".to_vec())), Ok(Some(b"from disk".to_vec()))));
}

#[test]
fn random_bundles_and_random_damage() {
    let mut rng = anneal_prelude::Rng::new(170401);
    for case in 0..400 {
        let n = rng.below(5);
        let mut blobs = Vec::new();
        for i in 0..n {
            let len = rng.below(20);
            let blob: Vec<u8> = rng.vec(len, 0, 255);
            blobs.push((format!("n{i}"), blob));
        }
        let list: Vec<(&str, &[u8])> = blobs.iter().map(|(n, b)| (n.as_str(), b.as_slice())).collect();
        let mut bytes = bundle(&list);
        let chunk = 1 + rng.below(9);
        let (reader, _) = counting(bytes.clone(), chunk);
        let mut b = Bundle::open(reader).unwrap();
        for (name, blob) in &blobs {
            check!(format!("case {case}: {name}"), b.get(name), Ok(Some(blob.clone())));
        }
        // damage a few bytes: any answer is fine except a panic or a huge allocation
        for _ in 0..3 {
            let at = rng.below(bytes.len());
            bytes[at] = rng.int(0, 255) as u8;
        }
        if let Ok(mut b) = Bundle::open(Cursor::new(bytes)) {
            let names: Vec<String> = b.names().into_iter().map(String::from).collect();
            for name in names {
                let _ = b.get(&name);
            }
        }
    }
}
