from y17_parts.common import broken, TEMP_DIR

TYPES = r'''use std::io::{self, Read, Seek, SeekFrom};

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
'''

STARTER = TYPES + r'''
pub struct Bundle<R: Read + Seek> {
    reader: R,
}

impl<R: Read + Seek> Bundle<R> {
    pub fn open(reader: R) -> Result<Bundle<R>, BundleError> {
        todo!()
    }

    /// The names, in the order the directory lists them.
    pub fn names(&self) -> Vec<&str> {
        todo!()
    }

    /// The bytes stored under `name`, or `None` if there is no such entry.
    pub fn get(&mut self, name: &str) -> Result<Option<Vec<u8>>, BundleError> {
        todo!()
    }
}
'''

SOLUTION = TYPES + r'''
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
            if offset.checked_add(len).is_none_or(|end| end > start) || !seen.insert(name) {
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
'''

HARNESS = r'''
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
'''

VISIBLE = HARNESS + r'''
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
'''

HIDDEN = HARNESS + TEMP_DIR + r'''
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
'''

PROBLEM = dict(
    slug="read-from-the-back", title="A bundle you read from the back", level="medium", stage="files-that-survive",
    tags=["file formats", "Seek", "read_exact", "validation", "untrusted input"],
    statement=r'''
A game ships thousands of assets in one large file. The loader reads the whole file to find any one of them. It also crashes on a damaged download.

The file is a **bundle**. The data of all entries comes first. A directory follows it, and a fixed footer ends the file. All numbers are little-endian.

```
data       any bytes
directory  count     u32
           count times:
             name_len  u16
             name      UTF-8
             offset    u64
             len       u64
footer     directory_offset u64
           the bytes "BND1"
```

An entry's bytes are the `len` bytes at `offset` from the start of the file. A name or an entry may be empty. `names()` keeps the directory's order.

**`Bundle::open(reader)`** reads only what it needs: the footer and the directory, at most that many bytes in all. It returns `Corrupt` unless **all** of this holds:

- the file is long enough for a footer, and the footer ends in `BND1`;
- the directory starts at or before the footer;
- the directory holds exactly `count` entries, with no bytes left over;
- every name is UTF-8, and no name appears twice;
- every entry lies wholly in front of the directory: `offset + len` is at most the directory's offset, also when `len` is 0.

**`get(name)`** reads only that entry's bytes.

The reader may return **fewer bytes than asked** from a single `read`. An error from the reader is `Io` with its kind. The file may be hostile: no input may cause a panic, or an allocation much larger than the file.
''',
    starter=STARTER, solution=SOLUTION, visible=[VISIBLE], hidden=[HIDDEN], use="use solution::*;",
    hints=[
        ("approach", "The end is the only place with a known position. Each thing you read tells you where the next one is, and each of those numbers must be checked before it is used."),
        ("rust", "`seek(SeekFrom::End(0))` returns the file's length. `read_exact` repeats `read` until the buffer is full. Reading numbers from a slice that you shorten as you go keeps the bounds checks in one place."),
        ("edge", "`offset + len` can overflow. A `count` of four billion must not reserve room for four billion entries."),
    ],
    notes=("The footer is read first, then the directory it points to. Every number from the file is compared with something already known to be real before it sizes a buffer or becomes a position. Checking all ranges at open means `get` can trust them.", "O(directory) to open; O(entry) per get after the name lookup", "O(directory)"),
    follow_up="1) Nothing here detects a changed byte inside an entry. Where would you put checksums so that `get` can verify without reading the whole file? 2) Why do formats such as ZIP and Parquet put their directory at the end? 3) `Read::read` may return fewer bytes than asked. Name two kinds of reader that often do.",
    teaches=["Read a large file by following positions: footer, then directory, then the one thing you need.", "Check every length and offset from a file before using it, with arithmetic that cannot overflow.", "A single `read` may return part of what you asked for."],
    wrong={
        "reads_the_whole_file": broken(SOLUTION, ("        let size = reader.seek(SeekFrom::End(0))?;\n", "        reader.seek(SeekFrom::Start(0))?;\n        let mut all = Vec::new();\n        reader.read_to_end(&mut all)?;\n        let size = all.len() as u64;\n")),
        "one_read_is_enough": broken(SOLUTION, ("        let mut bytes = vec![0u8; len as usize];\n        self.reader.read_exact(&mut bytes)?;", "        let mut bytes = vec![0u8; len as usize];\n        let n = self.reader.read(&mut bytes)?;\n        bytes.truncate(n);")),
        "ranges_checked_against_the_file": broken(SOLUTION, ("is_none_or(|end| end > start)", "is_none_or(|end| end > size)")),
        "sum_may_wrap": broken(SOLUTION, ("offset.checked_add(len).is_none_or(|end| end > start)", "offset.wrapping_add(len) > start")),
        "reserves_by_the_count": broken(SOLUTION, ("let mut entries = Vec::new();", "let mut entries = Vec::with_capacity(count as usize);")),
        "leftover_directory_bytes_accepted": broken(SOLUTION, ("        if !rest.is_empty() {\n            return Err(BundleError::Corrupt);\n        }\n", "")),
        "duplicate_names_accepted": broken(SOLUTION, (" || !seen.insert(name)", " || !(seen.insert(name) || true)")),
    },
)
