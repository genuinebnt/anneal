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
