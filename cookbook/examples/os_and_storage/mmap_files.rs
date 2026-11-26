//! RECIPE 175 (os): memory-mapped files with `memmap2`: zero-copy reads, durable writes, growth, and an mmap-backed record store
//! LEVEL: intermediate
//! Use in: databases and indexes, log/segment stores, big read-mostly data files, shared-memory IPC.   Run: cargo run --release --example mmap_files
#![allow(dead_code)]
//!
//! WHAT: `mmap(2)` maps a file into the address space. Loads and stores on the returned slice become page
//! faults served from the OS page cache: no `read` syscalls, no copy into a user buffer, and the kernel
//! decides what stays resident. `Mmap` is a read-only `&[u8]`; `MmapMut` is a writable `&mut [u8]`.
//!
//! DURABILITY: stores into an `MmapMut` only dirty page-cache pages. `flush()` is `msync(MS_SYNC)`: it blocks
//! until the dirty pages are written to the device (like fsync). `flush_async()` is `MS_ASYNC`: it just
//! schedules the writeback. A crash of YOUR process loses nothing already stored (the kernel owns the pages);
//! a power loss / kernel crash can lose anything not msync'd. Order matters: write the record, flush, THEN
//! publish it by bumping the count in the header, flush again.
//!
//! GROWTH: a mapping has a fixed length. To grow: flush, `File::set_len(bigger)` (sparse zeros), drop the old
//! map, map again. Slices from the old map must not outlive it (the borrow checker enforces that).
//!
//! ALIGNMENT: a map starts on a page boundary, so offset `k * align_of::<T>()` is aligned for `T`. A `#[repr(C)]`
//! struct of plain integers can be viewed in place with `align_to`, but byte order and padding make that
//! non-portable; reading fields with `from_le_bytes` is always safe and costs nothing measurable.
//!
//! WHY `Mmap::map` IS `unsafe`: Rust requires that memory behind a `&[u8]` is not mutated while borrowed. The
//! OS gives no such promise for files: another process (or thread via a second map) can write to the file
//! under you (data race, torn values), and if it TRUNCATES the file, touching a page beyond the new end
//! delivers SIGBUS and kills your process: no `Result`, no unwinding. Only map files you own exclusively
//! (e.g. protect with an advisory lock, see file_locking_atomic_replace), never untrusted or shared-writable ones. Also: IO errors
//! (disk failure, a network filesystem hiccup) surface as SIGBUS, not `Err`.
//!
//! Compared to `read`: mmap wins for random access, repeated scans (page cache stays warm, no copies) and
//! shared maps; plain sequential `read` into a reusable buffer is as fast for one-pass streaming and has
//! sane error handling. Measure; the timing below is only illustrative.
//!
//! COMMON USES: LMDB-style embedded key-value stores, search-engine index segments, columnar data files,
//! loading large read-only models/assets, ring buffers shared between processes.
//! Read: https://docs.rs/memmap2 and https://man7.org/linux/man-pages/man2/mmap.2.html and https://man7.org/linux/man-pages/man2/msync.2.html
use memmap2::{Mmap, MmapMut, MmapOptions};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn page_size() -> usize {
    // SAFETY: sysconf with a valid name has no memory-safety preconditions.
    unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize }
}

fn tmp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("cb_r175_{}_{name}", std::process::id()))
}

/// 16-byte fixed record, little-endian on disk.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct Record {
    id: u64,
    value: u64,
}
const REC: usize = std::mem::size_of::<Record>();

// ---------- the store: [Header 32 bytes][Record; capacity] ----------
const MAGIC: u64 = 0x5245_4353_544F_5245; // "RECSTORE"
const HDR: usize = 32;
const VERSION: u32 = 1;

/// Layout: magic u64 | version u32 | pad u32 | count u64 | capacity u64 (all little-endian).
struct Store {
    file: File,
    map: MmapMut,
}

impl Store {
    fn create(path: &Path, capacity: u64) -> io::Result<Store> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)?;
        file.set_len(HDR as u64 + capacity * REC as u64)?;
        // SAFETY: we just created this file and hold the only handle; nobody else truncates or resizes it.
        let mut map = unsafe { MmapMut::map_mut(&file)? };
        map[0..8].copy_from_slice(&MAGIC.to_le_bytes());
        map[8..12].copy_from_slice(&VERSION.to_le_bytes());
        map[16..24].copy_from_slice(&0u64.to_le_bytes());
        map[24..32].copy_from_slice(&capacity.to_le_bytes());
        map.flush()?;
        Ok(Store { file, map })
    }

    fn open(path: &Path) -> io::Result<Store> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let len = file.metadata()?.len();
        if len < HDR as u64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "file shorter than header",
            ));
        }
        // SAFETY: the caller owns the file for the store's lifetime (documented contract of this type).
        let map = unsafe { MmapMut::map_mut(&file)? };
        let s = Store { file, map };
        if s.u64_at(0) != MAGIC || s.map[8..12] != VERSION.to_le_bytes() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "bad magic/version",
            ));
        }
        // validate before trusting: a corrupt header must not make us index out of the map
        if s.count() > s.capacity() || len < HDR as u64 + s.capacity() * REC as u64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "inconsistent header",
            ));
        }
        Ok(s)
    }

    fn u64_at(&self, off: usize) -> u64 {
        u64::from_le_bytes(self.map[off..off + 8].try_into().unwrap())
    }
    fn count(&self) -> u64 {
        self.u64_at(16)
    }
    fn capacity(&self) -> u64 {
        self.u64_at(24)
    }

    fn grow(&mut self, new_cap: u64) -> io::Result<()> {
        self.map.flush()?;
        self.file.set_len(HDR as u64 + new_cap * REC as u64)?;
        // SAFETY: same exclusivity argument as `create`; the old map is replaced (and dropped) right here,
        // so no slice into it can survive the assignment.
        self.map = unsafe { MmapMut::map_mut(&self.file)? };
        self.map[24..32].copy_from_slice(&new_cap.to_le_bytes());
        self.map.flush()
    }

    fn append(&mut self, r: Record) -> io::Result<u64> {
        let n = self.count();
        if n == self.capacity() {
            self.grow((self.capacity() * 2).max(4))?;
        }
        let off = HDR + n as usize * REC;
        self.map[off..off + 8].copy_from_slice(&r.id.to_le_bytes());
        self.map[off + 8..off + 16].copy_from_slice(&r.value.to_le_bytes());
        // publish AFTER the record bytes are in place (and, for crash safety, flushed)
        self.map.flush_range(off, REC)?;
        self.map[16..24].copy_from_slice(&(n + 1).to_le_bytes());
        self.map.flush_range(16, 8)?;
        Ok(n)
    }

    fn get(&self, i: u64) -> Option<Record> {
        if i >= self.count() {
            return None;
        }
        let off = HDR + i as usize * REC;
        Some(Record {
            id: u64::from_le_bytes(self.map[off..off + 8].try_into().unwrap()),
            value: u64::from_le_bytes(self.map[off + 8..off + 16].try_into().unwrap()),
        })
    }

    /// Zero-copy view of all records in place. Only valid for the host's byte order (we assert LE).
    fn records(&self) -> &[Record] {
        assert!(
            cfg!(target_endian = "little"),
            "on-disk format is little-endian"
        );
        let bytes = &self.map[HDR..HDR + self.count() as usize * REC];
        // SAFETY: `Record` is repr(C) with two u64 (no padding, every bit pattern valid). `align_to` returns
        // the correctly aligned middle part; we assert prefix/suffix are empty so nothing is dropped.
        let (pre, mid, post) = unsafe { bytes.align_to::<Record>() };
        assert!(pre.is_empty() && post.is_empty(), "misaligned view");
        mid
    }
}

fn main() -> io::Result<()> {
    let ps = page_size();
    println!(
        "page size = {ps} bytes (mappings are page-granular; offsets must be multiples of it)"
    );
    assert!(ps.is_power_of_two() && ps >= 4096);

    // --- 1. read-only map + zero-copy fixed record parsing ---
    let path = tmp_path("records.bin");
    let n_rec = (2 * ps / REC + 10) as u64; // spans more than two pages
    {
        let mut w = BufWriter::new(File::create(&path)?);
        for i in 0..n_rec {
            w.write_all(&i.to_le_bytes())?;
            w.write_all(&(i * i).to_le_bytes())?;
        }
        w.flush()?;
    }
    let file = File::open(&path)?;
    // SAFETY: a private temp file nobody else touches while mapped.
    let map = unsafe { Mmap::map(&file)? };
    assert_eq!(map.len(), n_rec as usize * REC);
    assert_eq!(map.as_ptr() as usize % ps, 0, "maps start page aligned");
    let mut sum = 0u64;
    for (i, chunk) in map.chunks_exact(REC).enumerate() {
        // no allocation, no copy of the file: `chunk` points into the page cache
        let id = u64::from_le_bytes(chunk[..8].try_into().unwrap());
        let val = u64::from_le_bytes(chunk[8..].try_into().unwrap());
        assert_eq!(id, i as u64);
        assert_eq!(val, (i * i) as u64);
        sum += val;
    }
    println!("1. parsed {n_rec} records zero-copy, sum of squares = {sum}");
    // mapping a sub-range: offset must be page aligned
    let tail = unsafe_sub_map(&file, ps as u64, 64)?;
    assert_eq!(tail.len(), 64);
    assert_eq!(&tail[..8], &map[ps..ps + 8]);
    drop((map, tail));
    fs::remove_file(&path)?;

    // --- 2. MmapMut writes + flush ---
    let path = tmp_path("rw.bin");
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)?;
    f.set_len(ps as u64)?;
    // SAFETY: exclusive private file.
    let mut m = unsafe { MmapMut::map_mut(&f)? };
    m[..5].copy_from_slice(b"hello");
    m.flush()?; // msync(MS_SYNC): durable on return
    let seen = fs::read(&path)?; // a plain read() sees mmap stores (same page cache)
    assert_eq!(&seen[..5], b"hello");
    m[5..10].copy_from_slice(b"world");
    m.flush_async()?; // just schedules writeback; visibility via the page cache is already immediate
    assert_eq!(&fs::read(&path)?[..10], b"helloworld");
    println!("2. writes through the map are visible to read(); flush() = msync");

    // --- 3. grow with set_len then remap ---
    f.set_len(4 * ps as u64)?;
    drop(m);
    // SAFETY: still exclusive.
    let mut m = unsafe { MmapMut::map_mut(&f)? };
    assert_eq!(m.len(), 4 * ps);
    assert_eq!(&m[..10], b"helloworld"); // old contents preserved
    assert!(m[ps..].iter().all(|&b| b == 0), "new space reads as zeros");
    m[4 * ps - 1] = 42;
    m.flush()?;
    drop(m);
    drop(f);
    fs::remove_file(&path)?;
    println!("3. grew file 1 -> 4 pages and remapped; old data kept, new data zeroed");

    // --- 4. repr(C) view with alignment check ---
    println!(
        "4. Record size={REC} align={} (maps are page-aligned so offset 32 stays 8-aligned)",
        std::mem::align_of::<Record>()
    );

    // --- 5. the mmap-backed append-only store ---
    let path = tmp_path("store.db");
    {
        let mut s = Store::create(&path, 2)?; // tiny capacity forces growth
        for i in 0..100u64 {
            assert_eq!(
                s.append(Record {
                    id: i,
                    value: i * 3
                })?,
                i
            );
        }
        assert_eq!(s.count(), 100);
        assert!(s.capacity() >= 100);
        assert_eq!(s.get(7), Some(Record { id: 7, value: 21 }));
        assert_eq!(s.get(100), None);
    } // dropped: unmapped; data lives in the file
    let s = Store::open(&path)?;
    assert_eq!(s.count(), 100);
    let all = s.records();
    assert_eq!(all.len(), 100);
    assert!(
        all.iter()
            .enumerate()
            .all(|(i, r)| r.id == i as u64 && r.value == 3 * i as u64)
    );
    println!(
        "5. store reopened: {} records, zero-copy slice verified",
        all.len()
    );
    drop(s);
    // corrupt header is rejected, not trusted
    {
        let mut bytes = fs::read(&path)?;
        bytes[16..24].copy_from_slice(&u64::MAX.to_le_bytes()); // absurd count
        fs::write(&path, &bytes)?;
        assert!(Store::open(&path).is_err());
    }
    fs::remove_file(&path)?;

    // --- 6. read_to_end vs mmap over 64 MB ---
    let path = tmp_path("big.bin");
    {
        let mut w = BufWriter::new(File::create(&path)?);
        let block: Vec<u8> = (0..1 << 20).map(|i: u32| (i % 253) as u8).collect();
        for _ in 0..64 {
            w.write_all(&block)?;
        }
        w.flush()?;
    }
    let count_sevens = |b: &[u8]| b.iter().filter(|&&x| x == 7).count();
    let t = Instant::now();
    let mut buf = Vec::new();
    File::open(&path)?.read_to_end(&mut buf)?;
    let a = count_sevens(&buf);
    let t_read = t.elapsed();
    drop(buf);
    let t = Instant::now();
    let f = File::open(&path)?;
    // SAFETY: private temp file, not modified while mapped.
    let m = unsafe { Mmap::map(&f)? };
    let b = count_sevens(&m);
    let t_mmap = t.elapsed();
    assert_eq!(a, b);
    assert_eq!(m.len(), 64 << 20);
    println!("6. 64 MB scan: read_to_end {t_read:?} vs mmap {t_mmap:?} (both found {a} sevens)");
    drop(m);
    fs::remove_file(&path)?;

    println!("all mmap assertions passed");
    Ok(())
}

fn unsafe_sub_map(file: &File, offset: u64, len: usize) -> io::Result<Mmap> {
    // SAFETY: private temp file, not modified while mapped; `offset` is a multiple of the page size
    // (required by mmap) and offset+len lies inside the file (else the kernel would SIGBUS on access).
    unsafe { MmapOptions::new().offset(offset).len(len).map(file) }
}
