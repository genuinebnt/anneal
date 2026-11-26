from y17_parts.common import broken, TEMP_DIR
from y18_parts.code import TYPES, CODEC, LOG_FILE_TRAIT, LOG_FILE_IMPL, WAL, FILTER, TABLE_HEAD, TABLE_IMPL

USES = r'''use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;
'''

BELOW = TYPES + CODEC + LOG_FILE_TRAIT + LOG_FILE_IMPL + WAL + FILTER

SOLUTION = USES + BELOW + TABLE_HEAD + TABLE_IMPL

STARTER = USES + BELOW + r'''
// ---------- step 3: your work starts here ----------
''' + TABLE_HEAD + r'''
impl SsTable {
    /// Writes `entries`, which are sorted by key with no key twice, as table `id` in `dir`.
    pub fn write(dir: &Path, id: u64, entries: &[Entry]) -> Result<SsTable> {
        todo!()
    }

    pub fn open(path: &Path, id: u64) -> Result<SsTable> {
        todo!()
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// `None`: this table says nothing about the key. `Some(None)`: it says the key was deleted.
    pub fn get(&self, key: &[u8]) -> Result<Option<Option<Vec<u8>>>> {
        todo!()
    }

    /// Every record, delete markers included, in key order.
    pub fn read_all(&self) -> Result<Vec<Entry>> {
        todo!()
    }
}
'''

HARNESS = r'''
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
'''

VISIBLE = HARNESS + TEMP_DIR + r'''
#[test]
fn write_open_and_get() {
    let dir = TempDir::new("sst-basic");
    let written = SsTable::write(dir.path(), 7, &sample(40)).unwrap();
    check!("the path", written.path().to_path_buf(), dir.join("sst_000007.sst"));
    let t = SsTable::open(written.path(), 7).unwrap();
    check!("a value, a delete, a key between and a key after", (looked_up(&t, b"k0000"), looked_up(&t, b"k0008"), looked_up(&t, b"k0001"), looked_up(&t, b"z")),
        (Some(Some(Some(b"value 0".to_vec()))), Some(Some(None)), Some(None), Some(None)));
}

#[test]
fn read_all_returns_what_was_written() {
    let dir = TempDir::new("sst-all");
    let written = SsTable::write(dir.path(), 1, &sample(35)).unwrap();
    check!("35 entries", SsTable::open(written.path(), 1).unwrap().read_all().unwrap(), sample(35));
}

#[test]
fn no_temporary_file_remains() {
    let dir = TempDir::new("sst-names");
    SsTable::write(dir.path(), 12, &sample(3)).unwrap();
    let names: Vec<String> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    check!("files after one write", names, vec!["sst_000012.sst".to_string()]);
}

#[test]
fn the_footer() {
    let dir = TempDir::new("sst-footer");
    let t = SsTable::write(dir.path(), 1, &sample(20)).unwrap();
    let bytes = std::fs::read(t.path()).unwrap();
    let f = &bytes[bytes.len() - 32..];
    let data_end = *record_offsets(&sample(20)).last().unwrap() as u64;
    // the index has two keys of five bytes: 4 + 2 * (4 + 5 + 8)
    let numbers = [u64::from_le_bytes(f[0..8].try_into().unwrap()), u64::from_le_bytes(f[8..16].try_into().unwrap()), u64::from_le_bytes(f[16..24].try_into().unwrap())];
    check!("data end, filter start, record count", numbers, [data_end, data_end + 38, 20]);
    check!("the last four bytes", f[28..].to_vec(), 0x5353_5431u32.to_le_bytes().to_vec());
}

#[test]
fn a_flipped_byte_is_noticed() {
    let dir = TempDir::new("sst-flip");
    let t = SsTable::write(dir.path(), 1, &sample(5)).unwrap();
    let mut bytes = std::fs::read(t.path()).unwrap();
    bytes[3] ^= 1;
    std::fs::write(t.path(), bytes).unwrap();
    check!("one bit in the data", matches!(SsTable::open(t.path(), 1), Err(KvError::Corrupt(_))), true);
}
'''

HIDDEN = HARNESS + TEMP_DIR + r'''
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
'''

PROBLEM = dict(
    slug="table-files", title="Capstone 3: table files", level="hard", stage="on-disk",
    tags=["SSTable", "sparse index", "Bloom filter", "atomic publish", "validation", "file io"],
    statement=r'''
**Capstone, step 3 of 7.** When enough has piled up in memory, the store writes it to a file that never changes again. The starter holds the reference code of steps 1 and 2, and the key filter, with `new`, `may_contain`, `save` and `load`.

**Spoiler warning.** The starter gives away work from these parts of the series. Solve them first if you have not.

- Part 4, *A key-value store with a write-ahead log that survives torn writes*: the torn-end rule, in the earlier code
- Part 5, *An append that fails half-way*: the failure handling, in the step 2 log
- Part 12, *Fix: a saved filter that forgets its keys*: the solution, in the filter

**The file** of table `id` is `sst_<id>.sst`, with the id padded with zeros to at least six digits. Its parts, in order (numbers little-endian):

```
data    each entry as a record
        (step 1), in key order
index   count u32
        count times:
          key_len u32
          key
          offset  u64
filter  the saved filter
        of all keys
footer  data_end     u64
        filter_start u64
        record_count u64
        crc          u32
        magic        u32
```

`magic` is `0x53535431`.

The index has an entry for records 0, 16, 32 and so on: the key, and the offset where that record starts. `crc` is the `crc32` of every byte before it.

**`write`** builds the file under the same name ending in `.tmp`, replacing any leftover of that name. It then syncs the file, renames it, and syncs the directory. A table is under its final name only when it is complete.

**`open`** returns `Corrupt` for a file that is too short, has a wrong magic or checksum, or does not make sense. A good checksum is not enough: no offset, length or count may lead to a panic later. Offsets in the index start at 0, increase, and lie in the data. Its keys increase. The index fills its part exactly, and is empty exactly when the data is.

**Memory and reads.** After `open`, a table keeps the index and the filter, not the records. `get` reads **at most one block** (the records from one index offset to the next) from the file. It reads nothing when the index or the filter already shows that the key is absent.
''',
    starter=STARTER, solution=SOLUTION, visible=[VISIBLE], hidden=[HIDDEN], use="use solution::*;", crates=["thiserror"],
    hints=[
        ("approach", "Build the whole file in a `Vec<u8>` and note the position where each part begins. In `open`, work backwards from the fixed-size footer and check each number before it is used."),
        ("rust", "`partition_point` finds the number of index keys at or before a key. `File::seek` and `read_exact` read one block. `path.with_extension(\"tmp\")` gives the temporary name."),
        ("edge", "A crafted file can have a valid checksum and offsets that go backwards: `end - start` then underflows. An empty table has no index entries."),
    ],
    notes=("The file is assembled in memory, written under a temporary name, synced and renamed. Opening checks the checksum once and then every offset, so later reads can trust the index. A point read uses the index and the filter to read one block or nothing.", "write and open O(file); get O(log blocks + block)", "O(index + filter) per open table"),
    follow_up="1) The checksum is verified only when the table is opened. What can go wrong later, and where would per-block checksums go? 2) `open` reads the whole file once. How would you change the format so that it does not need to? 3) What does the sync of the directory protect against?",
    teaches=["An immutable file with a small index in memory answers any lookup with one read.", "Publish a file by writing under another name, syncing, renaming and syncing the directory.", "A checksum shows that bytes are unchanged, not that they make sense: validate offsets anyway."],
    wrong={
        "writes_the_final_name_directly": broken(SOLUTION, ('        let tmp = path.with_extension("tmp");\n', "        let tmp = path.clone();\n"), ("        fs::rename(&tmp, &path)?;\n", "")),
        "index_offsets_are_trusted": broken(SOLUTION, ("            let in_order = match index.last() {\n                None => offset == 0,\n                Some((last_key, last)) => *last_key < key && *last < offset,\n            };\n            if !in_order || offset >= data_end {\n                return Err(KvError::Corrupt(\"bad index\"));\n            }\n", "")),
        "keeps_the_records_in_memory": broken(SOLUTION,
            ("    data_end: u64,\n    filter: Filter,\n}", "    data_end: u64,\n    filter: Filter,\n    cached: Vec<u8>,\n}"),
            ("            data_end,\n            filter,\n        })\n    }\n\n    pub fn open(", "            data_end,\n            filter,\n            cached: bytes,\n        })\n    }\n\n    pub fn open("),
            ("            data_end,\n            filter,\n        })\n    }\n\n    pub fn id(", "            data_end,\n            filter,\n            cached: bytes,\n        })\n    }\n\n    pub fn id("),
            ("        let mut file = File::open(&self.path)?;\n        file.seek(SeekFrom::Start(start))?;\n        let mut bytes = vec![0u8; (end - start) as usize];\n        file.read_exact(&mut bytes)?;\n", "        let bytes = self.cached[start as usize..end as usize].to_vec();\n")),
        "get_reads_all_the_data": broken(SOLUTION, ("        let start = self.index[before - 1].1;\n        let end = self.index.get(before).map_or(self.data_end, |next| next.1);\n", "        let (start, end) = (0, self.data_end);\n")),
        "get_ignores_the_filter": broken(SOLUTION, ("        if before == 0 || !self.filter.may_contain(key) {", "        if before == 0 {")),
        "checksum_leaves_out_the_footer": broken(SOLUTION, ("        for number in [data_end, filter_start, entries.len() as u64] {\n            bytes.extend(number.to_le_bytes());\n        }\n        let crc = crc32(&bytes);\n", "        let crc = crc32(&bytes);\n        for number in [data_end, filter_start, entries.len() as u64] {\n            bytes.extend(number.to_le_bytes());\n        }\n"), ("        let crc_at = pos;\n", "        let crc_at = body;\n")),
        "a_delete_reads_as_absent": broken(SOLUTION, ("        Ok(block.into_iter().find(|e| e.0 == key).map(|e| e.1))", "        Ok(block\n            .into_iter()\n            .find(|e| e.0 == key)\n            .and_then(|e| e.1)\n            .map(Some))")),
    },
)
