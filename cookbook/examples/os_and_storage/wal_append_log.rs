//! RECIPE 98 (advanced): a write-ahead log: length-prefixed + checksummed records, torn-write detection, and crash recovery.
//! LEVEL: advanced
//! Use in: durable state (execution history, event sourcing), the DBMS roadmap (WAL, recovery); combines bytes, IO, errors, tests.   Run: cargo run --release --example wal_append_log
//! COMMON USES: databases (WAL), message brokers, event sourcing, filesystems: durable append with crash recovery.
#![allow(dead_code)]
//!
//! Record = [len: u32 LE][crc32: u32 LE][payload]. The checksum covers the length bytes AND the payload, so eight zero bytes (what
//! some filesystems leave at the end of a file after a crash) are not a valid empty record. Append-only: with one writer, syncing
//! every record, and storage that keeps what it has synced, an interrupted append damages only the end of the file. On startup,
//! call `recover`: it scans from the beginning, the first record that is truncated or fails its checksum marks the end of valid
//! data, the file is TRUNCATED there, and it is synced even if nothing was cut (the last writer may have stopped between its write
//! and its sync). Then open it and continue appending. `open` does not check the tail: records appended after a torn one are cut off by the next recovery.
//! If an append returns an error, the log refuses further appends: part of the record may be in the file (a later record would sit
//! behind it and be lost), and after a failed fsync nobody knows what reached the disk. Drop it, recover, open again. The failed
//! record may or may not be there afterwards. AFTER A FAILED FSYNC even that is not safe: some kernels (Linux among them) may drop
//! the unwritten pages' dirty mark, so `recover` can read the record from memory and its sync can succeed although the record
//! never reached the disk. A power cut then damages the middle of the log, and the next recovery deletes everything after it,
//! including records acknowledged later. This code does not rewrite the log; real systems stop and rebuild from what is known to
//! be durable. The payload is synced by `sync_data` (fsync) before `append` returns; sync every
//! record (safe, slow) or batch (group commit; not shown). The checksum detects torn writes and, with high probability, corruption.
//! LIMITS: recovery cannot tell a torn end from damage in the middle. Either way it DELETES everything from the first bad record,
//! including later records that were synced and are intact; a real log would keep a copy and report it. The parent directory is not
//! synced, so a power cut just after the file is created can lose the whole file (on Linux, fsync the directory too). `recover`
//! reads the whole file into memory. Nothing stops two writers from opening the same log.
//! Read: https://www.postgresql.org/docs/current/wal-intro.html , https://github.com/skyzh/mini-lsm (WAL chapter), and
//! https://github.com/tikv/raft-engine . Production crates compute CRC32 with hardware acceleration: https://docs.rs/crc32fast
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

fn crc32(data: &[u8]) -> u32 {
    !crc32_update(!0, data)
}

fn crc32_update(mut c: u32, data: &[u8]) -> u32 {
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = (c >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(c & 1));
        }
    }
    c
}

/// What the log needs from its file. A trait so that `main` can make a write or a sync fail on purpose.
trait LogFile {
    fn write_record(&mut self, record: &[u8]) -> io::Result<()>;
    fn cut(&mut self, len: u64) -> io::Result<()>;
    fn sync(&mut self) -> io::Result<()>;
}

impl LogFile for File {
    fn write_record(&mut self, record: &[u8]) -> io::Result<()> {
        self.write_all(record)
    }
    fn cut(&mut self, len: u64) -> io::Result<()> {
        self.set_len(len)
    }
    fn sync(&mut self) -> io::Result<()> {
        self.sync_data()
    }
}

struct Wal<F = File> {
    file: F,
    len: u64,
    /// Set when an append fails: the end of the file is unknown from then on.
    broken: bool,
}

impl Wal {
    /// open for appending (creates the file). Call `recover` first on existing logs.
    fn open(path: &Path) -> io::Result<Wal> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let len = file.metadata()?.len();
        Ok(Wal {
            file,
            len,
            broken: false,
        })
    }
}

/// The length field is 32 bits: a longer payload is refused rather than cut short.
fn length_field(n: usize) -> io::Result<[u8; 4]> {
    u32::try_from(n)
        .map(u32::to_le_bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "record too large"))
}

fn checksum(length: [u8; 4], payload: &[u8]) -> u32 {
    !crc32_update(crc32_update(!0, &length), payload)
}

impl<F: LogFile> Wal<F> {
    /// returns the offset of the record
    fn append(&mut self, payload: &[u8]) -> io::Result<u64> {
        if self.broken {
            return Err(io::Error::other(
                "an earlier append failed: recover and reopen",
            ));
        }
        let length = length_field(payload.len())?;
        let mut rec = Vec::with_capacity(8 + payload.len());
        rec.extend_from_slice(&length);
        rec.extend_from_slice(&checksum(length, payload).to_le_bytes());
        rec.extend_from_slice(payload);
        self.broken = true; // stays set unless both steps succeed
        self.file.write_record(&rec)?; // `write_all` may take several system calls
        self.file.sync()?; // the record's bytes are on disk once this returns
        self.broken = false;
        let at = self.len;
        self.len += rec.len() as u64;
        Ok(at)
    }
}

#[derive(Debug, PartialEq)]
enum Tail {
    Clean,
    TruncatedRecord { at: u64 },
    BadChecksum { at: u64 },
}

/// parse every valid record from the start; stop at the first problem
fn scan(bytes: &[u8]) -> (Vec<Vec<u8>>, u64, Tail) {
    let (mut records, mut pos) = (vec![], 0usize);
    loop {
        if pos == bytes.len() {
            return (records, pos as u64, Tail::Clean);
        }
        let Some(header) = bytes.get(pos..pos + 8) else {
            return (
                records,
                pos as u64,
                Tail::TruncatedRecord { at: pos as u64 },
            );
        };
        let length: [u8; 4] = header[..4].try_into().unwrap();
        let len = u32::from_le_bytes(length) as usize;
        let crc = u32::from_le_bytes(header[4..].try_into().unwrap());
        let Some(payload) = bytes.get(pos + 8..pos + 8 + len) else {
            return (
                records,
                pos as u64,
                Tail::TruncatedRecord { at: pos as u64 },
            );
        };
        if checksum(length, payload) != crc {
            return (records, pos as u64, Tail::BadChecksum { at: pos as u64 });
        }
        records.push(payload.to_vec());
        pos += 8 + len;
    }
}

/// read the log, truncate any invalid tail, return the valid records
fn recover(path: &Path) -> io::Result<(Vec<Vec<u8>>, Tail)> {
    let mut bytes = vec![];
    File::open(path)?.read_to_end(&mut bytes)?;
    let (records, valid_len, tail) = scan(&bytes);
    let mut f = OpenOptions::new().write(true).open(path)?;
    settle(&mut f, valid_len, bytes.len() as u64)?;
    Ok((records, tail))
}

/// Cut off an invalid tail, then sync. A tail that reads back clean may still be only in memory.
fn settle(file: &mut impl LogFile, valid_len: u64, found_len: u64) -> io::Result<()> {
    if valid_len != found_len {
        file.cut(valid_len)?;
    }
    file.sync()
}

/// A disk in memory that can run out of room in the middle of a record, or fail an fsync.
#[derive(Default)]
struct FaultyFile {
    bytes: Vec<u8>,
    room: Option<usize>,
    sync_fails: bool,
    syncs: usize,
}

impl LogFile for FaultyFile {
    fn write_record(&mut self, record: &[u8]) -> io::Result<()> {
        let n = self
            .room
            .map_or(record.len(), |room| room.min(record.len()));
        self.bytes.extend_from_slice(&record[..n]);
        if n < record.len() {
            return Err(io::ErrorKind::StorageFull.into());
        }
        Ok(())
    }
    fn cut(&mut self, len: u64) -> io::Result<()> {
        self.bytes.truncate(len as usize);
        Ok(())
    }
    fn sync(&mut self) -> io::Result<()> {
        self.syncs += 1;
        if self.sync_fails {
            return Err(io::Error::other("fsync failed"));
        }
        Ok(())
    }
}

/// Every append that returned `Ok` must come back from recovery, whatever failed in between.
fn failed_appends() {
    for (room, sync_fails) in [
        (Some(0), false),
        (Some(5), false),
        (Some(11), false),
        (None, true),
    ] {
        let mut wal = Wal {
            file: FaultyFile::default(),
            len: 0,
            broken: false,
        };
        let mut acknowledged = vec![b"before".to_vec()];
        wal.append(b"before").unwrap();
        (wal.file.room, wal.file.sync_fails) = (room, sync_fails);
        assert!(wal.append(b"failing").is_err());
        (wal.file.room, wal.file.sync_fails) = (None, false); // the disk works again
        if wal.append(b"after").is_ok() {
            acknowledged.push(b"after".to_vec());
        }
        let (recs, _, _) = scan(&wal.file.bytes);
        for record in &acknowledged {
            assert!(
                recs.contains(record),
                "an acknowledged record is gone ({room:?}, {sync_fails})"
            );
        }
        assert_eq!(acknowledged.len(), 1, "a failed log takes no more records");
    }
    // recovery syncs a clean log too, and reports it when that fails
    for (valid, found) in [(8, 8), (8, 11)] {
        let mut file = FaultyFile {
            bytes: vec![0; found as usize],
            ..FaultyFile::default()
        };
        settle(&mut file, valid, found).unwrap();
        assert_eq!((file.bytes.len() as u64, file.syncs), (valid, 1));
        file.sync_fails = true;
        assert!(settle(&mut file, valid, valid).is_err());
    }
    println!(
        "after a failed write or fsync the log refuses appends, and nothing acknowledged is lost"
    );
}

fn main() -> io::Result<()> {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926); // the standard CRC-32 check value

    let dir = std::env::temp_dir().join(format!("cookbook_wal_{}", std::process::id()));
    fs::create_dir_all(&dir)?;
    let path = dir.join("events.wal");
    let _ = fs::remove_file(&path);

    let mut wal = Wal::open(&path)?;
    for i in 0..5 {
        let off = wal.append(format!("event-{i}").as_bytes())?;
        println!("appended event-{i} at offset {off}");
    }
    drop(wal);
    let (recs, tail) = recover(&path)?;
    println!("clean recovery: {} records, tail {tail:?}", recs.len());
    assert_eq!((recs.len(), tail), (5, Tail::Clean));

    // simulate a crash in the middle of writing the 5th record: chop off its last 3 bytes
    let full = fs::read(&path)?;
    fs::write(&path, &full[..full.len() - 3])?;
    let (recs, tail) = recover(&path)?;
    println!(
        "after torn write: {} valid records, tail {tail:?}, file now {} bytes",
        recs.len(),
        fs::metadata(&path)?.len()
    );
    assert_eq!(recs.len(), 4);
    assert!(matches!(tail, Tail::TruncatedRecord { .. }));

    // the log is usable again: append after recovery
    let mut wal = Wal::open(&path)?;
    wal.append(b"event-after-crash")?;
    drop(wal);
    let (recs, tail) = recover(&path)?;
    assert_eq!((recs.len(), tail), (5, Tail::Clean));
    assert_eq!(recs.last().unwrap(), b"event-after-crash");

    // simulate silent corruption (bit flip) in record #2: everything from there on is untrusted
    let mut bytes = fs::read(&path)?;
    let second_payload = 8 + "event-0".len() + 8 + 2;
    bytes[second_payload] ^= 0x40;
    fs::write(&path, &bytes)?;
    let (recs, tail) = recover(&path)?;
    println!(
        "after bit flip: {} valid records, tail {tail:?}",
        recs.len()
    );
    assert_eq!(recs.len(), 1);
    assert!(matches!(tail, Tail::BadChecksum { .. }));

    // property: for EVERY possible truncation point, recovery yields a prefix of the records and never panics
    let mut wal = Wal::open(&dir.join("prop.wal"))?;
    let payloads: Vec<Vec<u8>> = (0..6).map(|i| vec![i as u8; i * 3]).collect();
    for p in &payloads {
        wal.append(p)?;
    }
    drop(wal);
    let all = fs::read(dir.join("prop.wal"))?;
    for cut in 0..=all.len() {
        let (recs, _, _) = scan(&all[..cut]);
        assert!(
            recs.len() <= payloads.len() && recs == payloads[..recs.len()],
            "cut at {cut}"
        );
    }
    println!(
        "all {} truncation points recover to a clean prefix of the log",
        all.len() + 1
    );

    // a tail of zeros (space the filesystem reserved but never filled) holds no records
    for zeros in 1..40 {
        let mut padded = all.clone();
        padded.resize(all.len() + zeros, 0);
        let (recs, valid, tail) = scan(&padded);
        assert!(
            recs == payloads && valid == all.len() as u64,
            "{zeros} zeros"
        );
        assert_ne!(tail, Tail::Clean);
    }
    // any one flipped bit, in a length, a checksum or a payload, leaves a shorter clean prefix
    for bit in 0..all.len() * 8 {
        let mut flipped = all.clone();
        flipped[bit / 8] ^= 1 << (bit % 8);
        let (recs, _, tail) = scan(&flipped);
        assert!(
            recs.len() < payloads.len() && recs == payloads[..recs.len()],
            "bit {bit}"
        );
        assert_ne!(tail, Tail::Clean);
    }
    assert!(length_field(u32::MAX as usize).is_ok());
    #[cfg(target_pointer_width = "64")]
    assert!(length_field(u32::MAX as usize + 1).is_err());
    failed_appends();
    fs::remove_dir_all(&dir)
}
