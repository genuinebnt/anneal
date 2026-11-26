use solution::*;

/// `n` entries with keys k0000, k0002, k0004, ... Every fifth is a delete.
#[allow(dead_code)]
fn sample(n: usize) -> Vec<Entry> {
    (0..n)
        .map(|i| (format!("k{:04}", i * 2).into_bytes(), if i % 5 == 4 { None } else { Some(format!("value {i}").into_bytes()) }))
        .collect()
}

#[allow(dead_code)]
fn looked_up(t: &SsTable, key: &[u8]) -> Option<Option<Option<Vec<u8>>>> {
    t.get(key).ok()
}

/// Where each record starts in the data, and where the data ends.
#[allow(dead_code)]
fn record_offsets(entries: &[Entry]) -> Vec<usize> {
    let mut bytes = Vec::new();
    let mut offsets = vec![0];
    for (k, v) in entries {
        encode_record(k, v.as_deref(), &mut bytes);
        offsets.push(bytes.len());
    }
    offsets
}

/// Overwrites every data byte of a table file in place. It stays the same file, so a reader that holds it open sees
/// the change, as does one that opens it again.
#[allow(dead_code)]
fn ruin_data(path: &std::path::Path) {
    let mut bytes = std::fs::read(path).unwrap();
    let footer = bytes.len() - 32;
    let data_end = u64::from_le_bytes(bytes[footer..footer + 8].try_into().unwrap()) as usize;
    bytes[..data_end].fill(0xee);
    std::fs::write(path, bytes).unwrap();
}

/// Recomputes the checksum of a changed file, so that only the change itself is wrong.
#[allow(dead_code)]
fn reseal(bytes: &mut [u8]) {
    let at = bytes.len() - 8;
    let crc = crc32(&bytes[..at]);
    bytes[at..at + 4].copy_from_slice(&crc.to_le_bytes());
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
fn the_exact_file_for_one_record() {
    let dir = TempDir::new("sst-golden");
    let t = SsTable::write(dir.path(), 1, &[(b"a".to_vec(), Some(b"A".to_vec()))]).unwrap();
    let mut want = vec![1, 1, 0, 0, 0, b'a', 1, 0, 0, 0, b'A'];
    want.extend([1, 0, 0, 0, 1, 0, 0, 0, b'a', 0, 0, 0, 0, 0, 0, 0, 0]);
    // the filter of the single key "a": 8 bytes, 4 probes
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    h = (h ^ u64::from(b'a')).wrapping_mul(0x0000_0100_0000_01b3);
    h ^= h >> 32;
    let mut bits = [0u8; 8];
    for i in 0..4u64 {
        let p = (h.wrapping_add(i.wrapping_mul(h.rotate_left(31) | 1)) % 64) as usize;
        bits[p / 8] |= 1 << (p % 8);
    }
    want.extend([4, 8, 0, 0, 0]);
    want.extend(bits);
    for n in [11u64, 28, 1] {
        want.extend(n.to_le_bytes());
    }
    let crc = crc32(&want);
    want.extend(crc.to_le_bytes());
    want.extend(0x5353_5431u32.to_le_bytes());
    check!("sst_000001.sst", std::fs::read(t.path()).unwrap(), want);
}

#[test]
fn every_block_boundary() {
    for n in [1, 15, 16, 17, 32, 33, 100] {
        let dir = TempDir::new("sst-bounds");
        let entries = sample(n);
        let w = SsTable::write(dir.path(), 1, &entries).unwrap();
        for t in [SsTable::open(w.path(), 1).unwrap(), w] {
            for (k, v) in &entries {
                check!(format!("{n} records, get {k:?}"), looked_up(&t, k), Some(Some(v.clone())));
                let mut missing = k.clone();
                missing.push(0);
                check!(format!("{n} records, get {missing:?}"), looked_up(&t, &missing), Some(None));
            }
            check!(format!("{n} records, before the first"), looked_up(&t, b""), Some(None));
        }
    }
}

#[test]
fn an_empty_table() {
    let dir = TempDir::new("sst-empty");
    let w = SsTable::write(dir.path(), 3, &[]).unwrap();
    let t = SsTable::open(w.path(), 3).unwrap();
    check!("no records", (looked_up(&t, b"x"), t.read_all().ok(), t.id()), (Some(None), Some(vec![]), 3));
}

#[test]
fn get_reads_only_one_block() {
    let dir = TempDir::new("sst-oneblock");
    let entries = sample(64);
    let offsets = record_offsets(&entries);
    let written = SsTable::write(dir.path(), 1, &entries).unwrap();
    let good = std::fs::read(written.path()).unwrap();
    for block in 0..4 {
        std::fs::write(written.path(), &good).unwrap();
        let t = SsTable::open(written.path(), 1).unwrap();
        // after opening, ruin every data byte outside this block
        let mut ruined = good.clone();
        for (i, byte) in ruined.iter_mut().enumerate().take(offsets[64]) {
            if i < offsets[block * 16] || i >= offsets[block * 16 + 16] {
                *byte = 0xee;
            }
        }
        std::fs::write(written.path(), ruined).unwrap();
        for (k, v) in &entries[block * 16..block * 16 + 16] {
            check!(format!("block {block} intact, the rest ruined: get {k:?}"), looked_up(&t, k), Some(Some(v.clone())));
        }
    }
}

#[test]
fn what_the_memory_part_can_answer_reads_nothing() {
    let dir = TempDir::new("sst-noread");
    let w = SsTable::write(dir.path(), 1, &sample(100)).unwrap();
    let t = SsTable::open(w.path(), 1).unwrap();
    // from here on, anything that reads a record finds rubbish
    ruin_data(t.path());
    check!("a key before the first", looked_up(&t, b"a"), Some(None));
    let refused = (0..200).filter(|i| t.get(format!("k{:04}x", i).as_bytes()).is_ok_and(|r| r.is_none())).count();
    check!(format!("{refused} of 200 absent keys answered without reading a record; the filter should settle at least 180"), refused >= 180, true);
    // the table keeps no records in memory, so it cannot still know the old value
    check!("a present key after the data was ruined", looked_up(&t, b"k0000") == Some(Some(Some(b"value 0".to_vec()))), false);
}

#[test]
fn every_shorter_file_is_rejected() {
    let dir = TempDir::new("sst-short");
    let w = SsTable::write(dir.path(), 1, &sample(20)).unwrap();
    let good = std::fs::read(w.path()).unwrap();
    for keep in 0..good.len() {
        std::fs::write(w.path(), &good[..keep]).unwrap();
        check!(format!("first {keep} of {} bytes", good.len()), matches!(SsTable::open(w.path(), 1), Err(KvError::Corrupt(_))), true);
    }
}

#[test]
fn every_damaged_byte_is_rejected() {
    let dir = TempDir::new("sst-damage");
    let w = SsTable::write(dir.path(), 1, &sample(20)).unwrap();
    let good = std::fs::read(w.path()).unwrap();
    for at in 0..good.len() {
        let mut bad = good.clone();
        bad[at] ^= 0x10;
        std::fs::write(w.path(), bad).unwrap();
        check!(format!("byte {at}"), matches!(SsTable::open(w.path(), 1), Err(KvError::Corrupt(_))), true);
    }
}

/// Opens a changed and resealed copy, and then uses it: an error is fine, a panic is not.
#[allow(dead_code)]
fn crafted(path: &std::path::Path, bytes: &[u8]) -> bool {
    std::fs::write(path, bytes).unwrap();
    match SsTable::open(path, 1) {
        Err(KvError::Corrupt(_)) => true,
        Err(_) => false,
        Ok(t) => {
            for i in 0..40 {
                let _ = t.get(format!("k{:04}", i * 2).as_bytes());
            }
            let _ = t.read_all();
            false
        }
    }
}

#[test]
fn a_good_checksum_does_not_make_offsets_sensible() {
    let dir = TempDir::new("sst-crafted");
    let w = SsTable::write(dir.path(), 1, &sample(40)).unwrap();
    let good = std::fs::read(w.path()).unwrap();
    let footer = good.len() - 32;
    let data_end = u64::from_le_bytes(good[footer..footer + 8].try_into().unwrap());
    // the two footer offsets
    for (field, value) in [(0, u64::MAX), (0, data_end + 1_000_000), (1, u64::MAX), (1, 0), (1, footer as u64 + 1)] {
        let mut bad = good.clone();
        bad[footer + 8 * field..footer + 8 * field + 8].copy_from_slice(&value.to_le_bytes());
        reseal(&mut bad);
        check!(format!("footer number {field} set to {value}"), crafted(w.path(), &bad), true);
    }
    // the index: count, then (length 4, key 5, offset 8) three times
    let index = data_end as usize;
    let offset_at = |i: usize| index + 4 + i * 17 + 9;
    for (what, entry, value) in [("first offset not 0", 0, 5u64), ("offsets go back", 2, 1), ("offsets repeat", 1, 0), ("offset past the data", 2, data_end), ("a huge offset", 2, u64::MAX)] {
        let mut bad = good.clone();
        bad[offset_at(entry)..offset_at(entry) + 8].copy_from_slice(&value.to_le_bytes());
        reseal(&mut bad);
        check!(what, crafted(w.path(), &bad), true);
    }
    for count in [0u32, 2, 4, u32::MAX] {
        let mut bad = good.clone();
        bad[index..index + 4].copy_from_slice(&count.to_le_bytes());
        reseal(&mut bad);
        check!(format!("index count {count} instead of 3"), crafted(w.path(), &bad), true);
    }
    let mut bad = good.clone();
    bad[index + 4 + 17 + 4] = b'a';
    reseal(&mut bad);
    check!("index keys out of order", crafted(w.path(), &bad), true);
}

#[test]
fn a_failed_write_publishes_nothing() {
    let dir = TempDir::new("sst-failtmp");
    // something is already where the temporary file goes
    std::fs::create_dir(dir.join("sst_000001.tmp")).unwrap();
    check!("the temporary file cannot be made", matches!(SsTable::write(dir.path(), 1, &sample(5)), Err(KvError::Io(_))), true);
    check!("no table appears", dir.join("sst_000001.sst").exists(), false);
    std::fs::create_dir_all(dir.join("sst_000002.sst/in-the-way")).unwrap();
    check!("the final name cannot be taken", matches!(SsTable::write(dir.path(), 2, &sample(5)), Err(KvError::Io(_))), true);
}

#[test]
fn a_leftover_temporary_file_does_not_matter() {
    let dir = TempDir::new("sst-stale");
    std::fs::write(dir.join("sst_000001.tmp"), vec![1u8; 100_000]).unwrap();
    let w = SsTable::write(dir.path(), 1, &sample(5)).unwrap();
    check!("old leftover, new table", SsTable::open(w.path(), 1).and_then(|t| t.read_all()).ok(), Some(sample(5)));
}

#[test]
fn random_tables() {
    let mut rng = anneal_prelude::Rng::new(180301);
    let dir = TempDir::new("sst-random");
    for case in 0..80 {
        let mut map = std::collections::BTreeMap::new();
        let n = rng.below(90);
        for _ in 0..n {
            let len = rng.below(5);
            let key = rng.string(len, "abc").into_bytes();
            let value = if rng.below(4) == 0 { None } else {
                let vlen = rng.below(10);
                Some(rng.vec::<u8>(vlen, 0, 255))
            };
            map.insert(key, value);
        }
        let entries: Vec<Entry> = map.clone().into_iter().collect();
        let w = SsTable::write(dir.path(), case, &entries).unwrap();
        let t = SsTable::open(w.path(), case).unwrap();
        check!(format!("case {case}: read_all of {n}"), t.read_all().unwrap(), entries);
        for _ in 0..30 {
            let len = rng.below(6);
            let key = rng.string(len, "abcd").into_bytes();
            check!(format!("case {case}: get {key:?}"), looked_up(&t, &key), Some(map.get(&key).cloned()));
        }
    }
}
