use solution::*;

use std::cell::RefCell;
use std::io;
use std::path::PathBuf;
use std::rc::Rc;

/// This checksum is written here too, so the tests do not trust the code under test for it.
#[allow(dead_code)]
fn crc(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 == 1 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
        }
    }
    !c
}

/// A frame built from the format in the statement, around any payload.
#[allow(dead_code)]
fn wrap(payload: &[u8]) -> Vec<u8> {
    let len = (payload.len() as u32).to_le_bytes();
    let mut out = len.to_vec();
    out.extend(crc(&len).to_le_bytes());
    out.extend(crc(payload).to_le_bytes());
    out.extend(payload);
    out
}

#[allow(dead_code)]
fn new_event(n: u64) -> NewEvent {
    NewEvent::current(CmdId::Client(n), 0, n, &Event::Deposited { amount: n as i64 })
}

/// A file in memory. Its next append can stop part way, and its next sync can fail.
#[allow(dead_code)]
#[derive(Default)]
struct Disk {
    bytes: Vec<u8>,
    short_write: Option<usize>,
    fail_sync: bool,
    calls: Vec<&'static str>,
}

#[allow(dead_code)]
#[derive(Clone, Default)]
struct Fake(Rc<RefCell<Disk>>);

impl Device for Fake {
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut d = self.0.borrow_mut();
        d.calls.push("append");
        if let Some(keep) = d.short_write.take() {
            let keep = keep.min(bytes.len());
            d.bytes.extend_from_slice(&bytes[..keep]);
            return Err(io::Error::from(io::ErrorKind::StorageFull));
        }
        d.bytes.extend_from_slice(bytes);
        Ok(())
    }
    fn sync(&mut self) -> io::Result<()> {
        let mut d = self.0.borrow_mut();
        d.calls.push("sync");
        if std::mem::take(&mut d.fail_sync) {
            return Err(io::Error::other("sync"));
        }
        Ok(())
    }
}

/// A log of batches of these sizes, and where each frame ends.
#[allow(dead_code)]
fn log_of(sizes: &[usize]) -> (Vec<u8>, Vec<usize>, Vec<Stored>) {
    let disk = Fake::default();
    let mut store = FileStore::over(MemStore::new(), disk.clone());
    let mut ends = Vec::new();
    let mut n = 0;
    for (i, &size) in sizes.iter().enumerate() {
        let stream = if i % 2 == 0 { "ann" } else { "bo" };
        let batch = (0..size).map(|_| {
            n += 1;
            new_event(n)
        }).collect();
        let at = store.version(stream);
        store.append(stream, at, batch).unwrap();
        ends.push(disk.0.borrow().bytes.len());
    }
    let bytes = disk.0.borrow().bytes.clone();
    (bytes, ends, store.read_all(0).to_vec())
}

#[allow(dead_code)]
fn lines(r: &Recovered) -> Vec<Stored> {
    r.store.read_all(0).to_vec()
}

#[allow(dead_code)]
fn failure(bytes: &[u8]) -> Option<LoadError> {
    recover(bytes).err()
}

/// A path in a directory that only this test uses.
#[allow(dead_code)]
struct Scratch(PathBuf);

#[allow(dead_code)]
impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("anneal-ledger-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
    fn path(&self) -> PathBuf {
        self.0.join("journal")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn every_cut_keeps_exactly_the_whole_batches() {
    let (bytes, ends, stored) = log_of(&[2, 1, 3]);
    let counts = [2, 3, 6];
    for cut in 0..=bytes.len() {
        let whole = ends.iter().filter(|&&e| e <= cut).count();
        let keep = if whole == 0 { 0 } else { ends[whole - 1] };
        let r = recover(&bytes[..cut]).unwrap_or_else(|e| panic!("cut at {cut}: {e:?}"));
        let n = if whole == 0 { 0 } else { counts[whole - 1] };
        let tail = if cut == keep { Tail::Clean } else { Tail::Torn { at: keep } };
        check!(format!("cut at {cut}"), (r.keep, r.tail), (keep, tail));
        check!(format!("cut at {cut}"), lines(&r), stored[..n].to_vec());
    }
}

#[test]
fn zeros_after_the_data_are_a_torn_end() {
    let (bytes, ends, _) = log_of(&[1, 1]);
    for zeros in [1, 11, 12, 13, 4096] {
        let mut b = bytes.clone();
        b.resize(b.len() + zeros, 0);
        let r = recover(&b).unwrap_or_else(|e| panic!("{zeros} zeros: {e:?}"));
        check!(format!("{zeros} zeros"), (r.keep, r.tail), (ends[1], Tail::Torn { at: ends[1] }));
    }
    check!("only zeros", recover(&[0; 40]).map(|r| (r.keep, r.tail)).ok(), Some((0, Tail::Torn { at: 0 })));
}

#[test]
fn a_header_whose_data_never_arrived_is_a_torn_end() {
    let (bytes, ends, _) = log_of(&[1, 2]);
    // the second frame's header and part of its payload arrived; the rest reads as zeros
    for have in [HEADER, HEADER + 1, HEADER + 20] {
        let mut b = bytes[..ends[0] + have].to_vec();
        b.resize(bytes.len() + 30, 0);
        let r = recover(&b).unwrap_or_else(|e| panic!("{have}: {e:?}"));
        check!(have, (r.keep, r.tail), (ends[0], Tail::Torn { at: ends[0] }));
    }
}

#[test]
fn every_flipped_bit_before_the_last_frame_is_damage() {
    let (bytes, ends, _) = log_of(&[1, 1]);
    for bit in 0..ends[0] * 8 {
        let mut b = bytes.clone();
        b[bit / 8] ^= 1 << (bit % 8);
        check!(format!("bit {bit}"), failure(&b), Some(LoadError::Damaged { at: 0 }));
    }
}

#[test]
fn flips_in_the_last_frame() {
    let (bytes, ends, _) = log_of(&[1, 1]);
    for byte in ends[0]..bytes.len() {
        let mut b = bytes.clone();
        b[byte] ^= 0x10;
        let got = recover(&b).map(|r| r.tail);
        if byte < ends[0] + 8 {
            // the header no longer vouches for itself, and it is not zeros
            check!(format!("byte {byte}"), got.err(), Some(LoadError::Damaged { at: ends[0] }));
        } else {
            check!(format!("byte {byte}"), got.ok(), Some(Tail::Torn { at: ends[0] }));
        }
    }
}

#[test]
fn a_huge_length_with_a_bad_header_checksum_is_damage() {
    let (mut bytes, ends, _) = log_of(&[1, 1]);
    bytes[ends[0] + 3] = 0xFF;
    check!("length", failure(&bytes), Some(LoadError::Damaged { at: ends[0] }));
}

#[test]
fn a_huge_length_that_vouches_for_itself_is_a_torn_end() {
    let len = u32::MAX.to_le_bytes();
    let mut b = len.to_vec();
    b.extend(crc(&len).to_le_bytes());
    b.extend([1; 20]);
    check!("4 GB promised", recover(&b).map(|r| r.tail).ok(), Some(Tail::Torn { at: 0 }));
}

#[test]
fn an_empty_frame_is_never_a_record() {
    let b = wrap(b"");
    check!("a length of zero, then nothing else", failure(&b), Some(LoadError::Damaged { at: 0 }));
}

#[test]
fn whole_frames_with_wrong_contents_are_refused() {
    let (bytes, ends, stored) = log_of(&[1, 1]);
    let first = &bytes[..ends[0]];
    let json = |batch: &[Stored]| wrap(&serde_json::to_vec(batch).unwrap());
    let mut newer = stored[1].clone();
    newer.schema = 3;
    let mut gap = stored[1].clone();
    gap.pos = 5;
    let cases = [
        ("the same batch twice", json(&stored[..1])),
        ("a gap", json(&[gap])),
        ("a newer schema", json(&[newer])),
        ("no events", json(&[])),
        ("not JSON", wrap(b"hello")),
    ];
    for (name, second) in cases {
        let all = [first, &second].concat();
        // last or not, a whole frame with the wrong thing inside is not a torn write
        check!(name, failure(&all), Some(LoadError::BadBatch { at: ends[0] }));
    }
}

#[test]
fn a_failed_sync_also_ends_the_store() {
    let disk = Fake::default();
    let mut store = FileStore::over(MemStore::new(), disk.clone());
    disk.0.borrow_mut().fail_sync = true;
    check!("io", matches!(store.append("ann", 0, vec![new_event(1)]), Err(StoreError::Io(_))), true);
    check!("not committed", (store.read_all(0).len(), store.version("ann")), (0, 0));
    disk.0.borrow_mut().calls.clear();
    check!("poisoned", matches!(store.append("ann", 0, vec![new_event(1)]), Err(StoreError::Poisoned)), true);
    check!("no calls, no second sync", disk.0.borrow().calls.len(), 0);
}

#[test]
fn every_short_write_leaves_a_log_that_opens() {
    let (whole, _, _) = log_of(&[1]);
    for keep in 0..whole.len() + 20 {
        let disk = Fake::default();
        let mut store = FileStore::over(MemStore::new(), disk.clone());
        store.append("ann", 0, vec![new_event(1)]).unwrap();
        disk.0.borrow_mut().short_write = Some(keep);
        store.append("ann", 1, vec![new_event(2)]).unwrap_err();
        let _ = store.append("ann", 1, vec![new_event(3)]);
        let r = recover(&disk.0.borrow().bytes).unwrap_or_else(|e| panic!("kept {keep}: {e:?}"));
        let n = r.store.read_all(0).len();
        check!(format!("kept {keep}"), n == 1 || n == 2, true);
    }
}

#[test]
fn refusals_do_not_touch_the_device_or_end_the_store() {
    let disk = Fake::default();
    let mut store = FileStore::over(MemStore::new(), disk.clone());
    check!("conflict", matches!(store.append("ann", 4, vec![new_event(1)]), Err(StoreError::Conflict { .. })), true);
    let mut bad = new_event(1);
    bad.schema = 7;
    check!("bad event", matches!(store.append("ann", 0, vec![bad]), Err(StoreError::BadEvent(_))), true);
    check!("empty", store.append("ann", 0, vec![]).map(|v| v.len()).ok(), Some(0));
    check!("no calls", disk.0.borrow().calls.len(), 0);
    check!("still usable", store.append("ann", 0, vec![new_event(1)]).is_ok(), true);
}

#[test]
fn open_cuts_a_torn_end_and_carries_on() {
    let (bytes, ends, stored) = log_of(&[1, 1]);
    let dir = Scratch::new("torn");
    std::fs::write(dir.path(), &bytes[..bytes.len() - 3]).unwrap();
    let (mut store, tail) = FileStore::open(&dir.path()).unwrap();
    check!("tail", tail, Tail::Torn { at: ends[0] });
    check!("file cut", std::fs::read(dir.path()).unwrap().len(), ends[0]);
    store.append("bo", 0, vec![new_event(9)]).unwrap();
    store.append("bo", 1, vec![new_event(10)]).unwrap();
    drop(store);
    let (store, tail) = FileStore::open(&dir.path()).unwrap();
    check!("tail", tail, Tail::Clean);
    check!("lines", store.read_all(0).len(), 3);
    check!("first", store.read_all(0)[0].clone(), stored[0].clone());
}

#[test]
fn open_leaves_a_damaged_file_alone() {
    let (mut bytes, _, _) = log_of(&[1, 1]);
    bytes[2] ^= 1;
    let dir = Scratch::new("damaged");
    std::fs::write(dir.path(), &bytes).unwrap();
    let got = FileStore::open(&dir.path()).err();
    check!("refused", matches!(got, Some(OpenError::Load(LoadError::Damaged { at: 0 }))), true);
    check!("file untouched", std::fs::read(dir.path()).unwrap(), bytes);
}

#[test]
fn random_crashes_keep_every_acknowledged_batch() {
    let mut rng = anneal_prelude::Rng::new(7204);
    for case in 0..200 {
        let disk = Fake::default();
        let mut store = FileStore::over(MemStore::new(), disk.clone());
        let mut acknowledged = 0;
        let mut a_write_failed = false;
        for n in 0..rng.below(8) as u64 {
            let at = store.version("ann");
            let batch = (0..1 + rng.below(3)).map(|_| new_event(n + 1)).collect();
            if !a_write_failed && rng.below(6) == 0 {
                disk.0.borrow_mut().short_write = Some(rng.below(150));
                a_write_failed = true;
            }
            if store.append("ann", at, batch).is_ok() {
                acknowledged = store.read_all(0).len();
            }
        }
        let mut bytes = disk.0.borrow().bytes.clone();
        // a power cut can also leave zeros where the file grew (here only after whole writes)
        if !a_write_failed {
            bytes.resize(bytes.len() + rng.below(3) * rng.below(60), 0);
        }
        let r = recover(&bytes).unwrap_or_else(|e| panic!("case {case}: {e:?}"));
        let kept = r.store.read_all(0).len();
        check!(format!("case {case}: no acknowledged line is lost"), kept >= acknowledged, true);
        check!(format!("case {case}"), r.store.read_all(0)[..acknowledged].to_vec(), store.read_all(0).to_vec());
    }
}

#[test]
fn hostile_bytes_never_panic() {
    let mut rng = anneal_prelude::Rng::new(7214);
    let (good, _, _) = log_of(&[1, 2]);
    for _ in 0..3000 {
        let mut bytes = good[..rng.below(good.len() + 1)].to_vec();
        let extra = rng.below(60);
        for _ in 0..extra {
            bytes.push(rng.below(256) as u8);
        }
        if let Ok(r) = recover(&bytes) {
            assert!(r.keep <= bytes.len());
        }
        // junk in a frame whose checksums are right
        let junk: Vec<u8> = (0..1 + rng.below(40)).map(|_| *rng.pick(b"[]{}\",:0123456789abnul ")).collect();
        let got = recover(&wrap(&junk)).err();
        check!(String::from_utf8_lossy(&junk), got, Some(LoadError::BadBatch { at: 0 }));
    }
    let deep = "[".repeat(200_000);
    check!("very deep JSON", failure(&wrap(deep.as_bytes())), Some(LoadError::BadBatch { at: 0 }));
}

#[test]
fn a_zeroed_header_in_front_of_good_frames_is_damage() {
    let (bytes, ends, _) = log_of(&[1, 1, 1]);
    for at in [0, ends[0]] {
        let mut b = bytes.clone();
        b[at..at + HEADER].fill(0);
        // cutting here would throw away batches that were acknowledged
        check!(format!("header at {at}"), failure(&b), Some(LoadError::Damaged { at }));
    }
}

#[test]
fn data_far_behind_a_bad_frame_still_counts() {
    let (bytes, ends, _) = log_of(&[1, 1]);
    let (first, second) = bytes.split_at(ends[0]);
    for gap in [0, 1, HEADER, HEADER + 1, 4096] {
        let mut b = first.to_vec();
        *b.last_mut().unwrap() ^= 1;
        b.resize(b.len() + gap, 0);
        b.extend_from_slice(second);
        check!(format!("bad frame, {gap} zeros, a good frame"), failure(&b), Some(LoadError::Damaged { at: 0 }));
    }
    let mut b = first.to_vec();
    *b.last_mut().unwrap() ^= 1;
    b.resize(b.len() + 4096, 0);
    b.push(1);
    check!("bad frame, long zeros, one byte", failure(&b), Some(LoadError::Damaged { at: 0 }));
}

#[test]
fn part_of_a_header_and_then_zeros_is_refused() {
    let (bytes, ends, _) = log_of(&[1, 1]);
    for have in [1, 4, 5, 7] {
        let mut b = bytes[..ends[0] + have].to_vec();
        b.resize(b.len() + 40, 0);
        check!(format!("{have} header bytes"), failure(&b), Some(LoadError::Damaged { at: ends[0] }));
    }
}
