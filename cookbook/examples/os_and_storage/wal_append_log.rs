//! RECIPE 98 (advanced): a write-ahead log: length-prefixed + checksummed records, torn-write detection, and crash recovery.
//! LEVEL: advanced
//! Use in: durable state (execution history, event sourcing), the DBMS roadmap (WAL, recovery); combines bytes, IO, errors, tests.   Run: cargo run --release --example wal_append_log
//! COMMON USES: databases (WAL), message brokers, event sourcing, filesystems: durable append with crash recovery.
#![allow(dead_code)]
//!
//! Record = [len: u32 LE][crc32: u32 LE][payload]. Append-only: the only operation that can be half-done by a crash is the LAST write.
//! On startup, scan from the beginning; the first record that is truncated or fails its checksum marks the end of valid data;
//! TRUNCATE the file there and continue appending. A record is durable only after `sync_data` (fsync), so decide per workload
//! whether to sync every record (safe, slow), or batch (group commit). The checksum catches torn writes AND silent corruption.
//! Read: https://www.postgresql.org/docs/current/wal-intro.html , https://github.com/skyzh/mini-lsm (WAL chapter), and
//! https://github.com/tikv/raft-engine . Production crates compute CRC32 with hardware acceleration: https://docs.rs/crc32fast
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = (c >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(c & 1));
        }
    }
    !c
}

struct Wal {
    file: File,
    len: u64,
}

impl Wal {
    /// open for appending (creates the file). Call `recover` first on existing logs.
    fn open(path: &Path) -> io::Result<Wal> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let len = file.metadata()?.len();
        Ok(Wal { file, len })
    }

    /// returns the offset of the record
    fn append(&mut self, payload: &[u8]) -> io::Result<u64> {
        let mut rec = Vec::with_capacity(8 + payload.len());
        rec.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        rec.extend_from_slice(&crc32(payload).to_le_bytes());
        rec.extend_from_slice(payload);
        self.file.write_all(&rec)?; // ONE write call keeps the record contiguous
        self.file.sync_data()?; // durable once this returns
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
        let len = u32::from_le_bytes(header[..4].try_into().unwrap()) as usize;
        let crc = u32::from_le_bytes(header[4..].try_into().unwrap());
        let Some(payload) = bytes.get(pos + 8..pos + 8 + len) else {
            return (
                records,
                pos as u64,
                Tail::TruncatedRecord { at: pos as u64 },
            );
        };
        if crc32(payload) != crc {
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
    if valid_len != bytes.len() as u64 {
        let f = OpenOptions::new().write(true).open(path)?;
        f.set_len(valid_len)?;
        f.sync_all()?;
    }
    Ok((records, tail))
}

fn main() -> io::Result<()> {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926); // the standard CRC-32 check value

    let dir = std::env::temp_dir().join("flowforge_cookbook_wal");
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
    fs::remove_dir_all(&dir)
}
