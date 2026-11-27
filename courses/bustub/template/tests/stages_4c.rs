//! Tests for module 4c: the write-ahead log and crash recovery.

#[path = "common/pool.rs"]
mod pool;
#[path = "common/crash.rs"]
mod crash;

use std::collections::{BTreeMap, HashMap};

use bustub::common::config::PageId;
use bustub::common::rid::Rid;
use bustub::recovery::log_manager::{LogManager, Lsn};
use bustub::recovery::log_record::{parse_log, LogRecord, TxnId};
use bustub::recovery::recover::{analyse, recover, redo, undo, Analysis};
use bustub::recovery::store::{Store, RECORD_LEN};
use crash::{CrashDisk, World};
use proptest::prelude::*;

fn rid(page: i32, slot: u32) -> Rid {
    Rid::new(PageId(page), slot)
}

/// A record of the store: 16 bytes that say which number it is.
fn rec(n: u64) -> Vec<u8> {
    let mut v = n.to_le_bytes().to_vec();
    v.extend_from_slice(&(!n).to_le_bytes());
    v
}

fn key(r: Rid) -> (i32, u32) {
    (r.page_id().0, r.slot_num())
}

fn contents(store: &Store<'_>) -> BTreeMap<(i32, u32), Vec<u8>> {
    store.scan().into_iter().map(|(r, v)| (key(r), v)).collect()
}

fn pconfig() -> ProptestConfig {
    ProptestConfig { cases: 48, max_shrink_iters: 2000, failure_persistence: None, ..ProptestConfig::default() }
}

// ---- 4c-01 · log records on the wire -----------------------------------------------------------------------------------------------

fn samples() -> Vec<LogRecord> {
    vec![
        LogRecord::Begin { txn: 7 },
        LogRecord::Change { txn: 7, rid: rid(3, 9), before: None, after: Some(rec(1)) },
        LogRecord::Change { txn: 7, rid: rid(3, 9), before: Some(rec(1)), after: Some(rec(2)) },
        LogRecord::Change { txn: 8, rid: rid(0, 0), before: Some(rec(2)), after: None },
        LogRecord::Commit { txn: 7 },
        LogRecord::Abort { txn: 8 },
        LogRecord::Checkpoint { active: vec![] },
        LogRecord::Checkpoint { active: vec![3, 4, 99] },
    ]
}

#[test]
fn s4c_01_every_kind_of_record_reads_back_as_it_was_written() {
    for r in samples() {
        let bytes = r.serialize();
        assert_eq!(LogRecord::deserialize(&bytes), Some((r.clone(), bytes.len())), "{r:?}");
    }
}

#[test]
fn s4c_01_records_follow_each_other_and_each_says_how_long_it_was() {
    let all = samples();
    let mut bytes = vec![];
    for r in &all {
        bytes.extend(r.serialize());
    }
    let mut at = 0;
    for r in &all {
        let (got, n) = LogRecord::deserialize(&bytes[at..]).expect("a record starts here");
        assert_eq!(&got, r);
        at += n;
    }
    assert_eq!(at, bytes.len(), "the lengths add up to the whole stream");
    assert!(LogRecord::deserialize(&[]).is_none(), "nothing is not a record");
}

#[test]
fn s4c_01_a_record_that_was_cut_short_is_not_a_record() {
    for r in samples() {
        let bytes = r.serialize();
        for cut in 0..bytes.len() {
            assert!(LogRecord::deserialize(&bytes[..cut]).is_none(), "{r:?} cut to {cut} of {} bytes", bytes.len());
        }
    }
}

#[test]
fn s4c_01_a_damaged_record_is_not_a_record() {
    for r in samples() {
        let bytes = r.serialize();
        for i in 0..bytes.len() {
            let mut bad = bytes.clone();
            bad[i] ^= 0x10;
            // a damaged record must never be read as a *different* record
            if let Some((got, _)) = LogRecord::deserialize(&bad) {
                assert_eq!(got, r, "byte {i} of {r:?} changed and the record read back as something else");
            }
        }
    }
}

#[test]
fn s4c_01_the_log_ends_where_the_first_bad_record_starts() {
    let all = samples();
    let mut bytes = vec![];
    let mut starts = vec![];
    for r in &all {
        starts.push(bytes.len() as u64);
        bytes.extend(r.serialize());
    }
    let (good, intact) = parse_log(&bytes);
    assert_eq!(good.len(), all.len());
    assert_eq!(intact, bytes.len());
    assert_eq!(good.iter().map(|(at, _)| *at).collect::<Vec<_>>(), starts, "each record comes with where it starts");
    // damage the fourth record: the first three survive, nothing after
    let mut torn = bytes.clone();
    torn[starts[3] as usize + 9] ^= 0xff;
    let (good, intact) = parse_log(&torn);
    assert_eq!(good.len(), 3, "the log ends at the damaged record");
    assert_eq!(intact as u64, starts[3]);
}

fn record_strategy() -> impl Strategy<Value = LogRecord> {
    let bytes = || prop::collection::vec(any::<u8>(), 0..40);
    let state = move || prop::option::of(bytes());
    prop_oneof![
        1 => any::<u64>().prop_map(|txn| LogRecord::Begin { txn }),
        4 => (any::<u64>(), -3..40i32, any::<u32>(), state(), state()).prop_map(|(txn, p, s, before, after)| LogRecord::Change { txn, rid: rid(p, s), before, after }),
        1 => any::<u64>().prop_map(|txn| LogRecord::Commit { txn }),
        1 => any::<u64>().prop_map(|txn| LogRecord::Abort { txn }),
        1 => prop::collection::vec(any::<u64>(), 0..6).prop_map(|active| LogRecord::Checkpoint { active }),
    ]
}

proptest! {
    #![proptest_config(pconfig())]

    /// Any record reads back as it was written; and a stream of records cut at any byte, or with any one byte changed, parses to a
    /// **prefix** of what was written: never to a record that was not there.
    #[test]
    fn s4c_01_cut_or_damaged_streams_give_a_prefix_of_what_was_written(records in prop::collection::vec(record_strategy(), 0..12), cut in any::<usize>(), at in any::<usize>(), flip in 1u8..=255) {
        let mut bytes = vec![];
        let mut ends = vec![];
        for r in &records {
            let b = r.serialize();
            prop_assert_eq!(LogRecord::deserialize(&b), Some((r.clone(), b.len())));
            bytes.extend(b);
            ends.push(bytes.len());
        }
        let cut_at = if bytes.is_empty() { 0 } else { cut % (bytes.len() + 1) };
        let (got, intact) = parse_log(&bytes[..cut_at]);
        let whole = ends.iter().filter(|e| **e <= cut_at).count();
        prop_assert_eq!(got.len(), whole, "cut at {} of {}", cut_at, bytes.len());
        prop_assert_eq!(intact, ends.get(whole.wrapping_sub(1)).copied().filter(|_| whole > 0).unwrap_or(0));
        for ((_, g), w) in got.iter().zip(&records) {
            prop_assert_eq!(g, w);
        }
        if !bytes.is_empty() {
            let mut bad = bytes.clone();
            let i = at % bad.len();
            bad[i] ^= flip;
            let (got, _) = parse_log(&bad);
            prop_assert!(got.len() <= records.len());
            for ((_, g), w) in got.iter().zip(&records) {
                prop_assert_eq!(g, w, "a damaged byte {} must not change a record", i);
            }
        }
    }
}

// ---- 4c-02 · the log manager -----------------------------------------------------------------------------------------------------------

fn begin(txn: TxnId) -> LogRecord {
    LogRecord::Begin { txn }
}

#[test]
fn s4c_02_a_lsn_is_where_the_record_starts_in_the_whole_log() {
    let disk = CrashDisk::new();
    let log = LogManager::new(disk.clone()).unwrap();
    let records = samples();
    let mut at = 0u64;
    for r in &records {
        assert_eq!(log.append(r), at, "the LSN of {r:?}");
        at += r.serialize().len() as u64;
        assert_eq!(log.end_lsn(), at, "the next record goes after it");
    }
    assert_eq!(log.flushed_lsn(), 0, "nothing was flushed");
}

#[test]
fn s4c_02_what_was_not_flushed_does_not_survive_a_crash() {
    let disk = CrashDisk::new();
    let log = LogManager::new(disk.clone()).unwrap();
    log.append(&begin(1));
    log.append(&begin(2));
    assert!(log.records().unwrap().is_empty(), "records() is the durable log: nothing yet");
    assert!(disk.crash().log_bytes().is_empty(), "a crash now loses both");
    log.flush().unwrap();
    log.append(&begin(3));
    let after = World::restart(disk.crash(), &[]);
    assert_eq!(after.log.records().unwrap().iter().map(|(_, r)| r.clone()).collect::<Vec<_>>(), vec![begin(1), begin(2)], "the flushed ones survive, the buffered one does not");
}

#[test]
fn s4c_02_flush_moves_the_durable_end_to_the_end_of_the_log() {
    let disk = CrashDisk::new();
    let log = LogManager::new(disk.clone()).unwrap();
    let l1 = log.append(&begin(1));
    let l2 = log.append(&begin(2));
    log.flush().unwrap();
    assert_eq!(log.flushed_lsn(), log.end_lsn());
    assert!(l1 < log.flushed_lsn() && l2 < log.flushed_lsn(), "a record is durable when its LSN is below the durable end");
    let l3 = log.append(&begin(3));
    assert!(l3 >= log.flushed_lsn(), "the new record is not durable yet");
    assert_eq!(disk.log_appends.load(std::sync::atomic::Ordering::SeqCst), 1, "one flush, one write to the device");
    log.flush().unwrap();
    log.flush().unwrap();
    assert_eq!(disk.log_appends.load(std::sync::atomic::Ordering::SeqCst), 2, "a flush with nothing buffered writes nothing");
}

#[test]
fn s4c_02_flush_to_only_flushes_when_the_record_is_not_durable_yet() {
    let disk = CrashDisk::new();
    let log = LogManager::new(disk.clone()).unwrap();
    let l1 = log.append(&begin(1));
    log.flush_to(l1).unwrap();
    let writes = disk.log_appends.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(writes, 1);
    assert!(l1 < log.flushed_lsn());
    log.flush_to(l1).unwrap();
    log.flush_to(0).unwrap();
    assert_eq!(disk.log_appends.load(std::sync::atomic::Ordering::SeqCst), writes, "the record is durable already: no write");
    let l2 = log.append(&begin(2));
    log.flush_to(l2).unwrap();
    assert!(l2 < log.flushed_lsn(), "now the second one is durable too");
}

#[test]
fn s4c_02_after_a_restart_the_log_goes_on_after_the_last_intact_record() {
    let disk = CrashDisk::new();
    let log = LogManager::new(disk.clone()).unwrap();
    log.append(&begin(1));
    log.append(&begin(2));
    log.flush().unwrap();
    let end = log.flushed_lsn();
    let restarted = LogManager::new(disk.crash()).unwrap();
    assert_eq!(restarted.flushed_lsn(), end);
    assert_eq!(restarted.append(&begin(3)), end, "the next LSN continues the numbering");
    restarted.flush().unwrap();
    assert_eq!(restarted.records().unwrap().len(), 3);
}

#[test]
fn s4c_02_a_torn_tail_is_cut_away_before_new_records_are_added() {
    let disk = CrashDisk::new();
    let log = LogManager::new(disk.clone()).unwrap();
    for t in 1..=3 {
        log.append(&begin(t));
    }
    log.flush().unwrap();
    let one = begin(1).serialize().len() as u64;
    let torn = disk.crash_tearing(5); // the last record was cut in the middle
    let restarted = LogManager::new(torn.clone()).unwrap();
    assert_eq!(restarted.records().unwrap().len(), 2, "the cut record is gone");
    assert_eq!(restarted.flushed_lsn(), 2 * one);
    assert_eq!(restarted.append(&begin(9)), 2 * one, "and the new one starts where it was");
    restarted.flush().unwrap();
    let again = LogManager::new(torn).unwrap();
    assert_eq!(again.records().unwrap().iter().map(|(_, r)| r.clone()).collect::<Vec<_>>(), vec![begin(1), begin(2), begin(9)], "no garbage between the old records and the new one");
}

#[derive(Clone, Debug)]
enum LogOp {
    Append(u64),
    Flush,
    FlushTo(usize),
    Restart,
    Tear(usize),
}

proptest! {
    #![proptest_config(pconfig())]

    /// Any run of appends, flushes, crashes and restarts: the durable log is exactly the records appended before the last flush, in order,
    /// each record's LSN is the sum of the sizes of the ones before it, and after a crash that cuts the tail the log is a prefix.
    #[test]
    fn s4c_02_the_durable_log_is_what_was_flushed(ops in prop::collection::vec(prop_oneof![4 => any::<u64>().prop_map(LogOp::Append), 2 => Just(LogOp::Flush), 2 => any::<usize>().prop_map(LogOp::FlushTo), 1 => Just(LogOp::Restart), 1 => (1usize..12).prop_map(LogOp::Tear)], 1..60)) {
        let mut disk = CrashDisk::new();
        let mut log = LogManager::new(disk.clone()).unwrap();
        let (mut durable, mut buffered): (Vec<(Lsn, LogRecord)>, Vec<(Lsn, LogRecord)>) = (vec![], vec![]);
        let mut end = 0u64;
        for op in ops {
            match op {
                LogOp::Append(t) => {
                    let r = begin(t);
                    let lsn = log.append(&r);
                    prop_assert_eq!(lsn, end);
                    end += r.serialize().len() as u64;
                    buffered.push((lsn, r));
                }
                LogOp::Flush => { log.flush().unwrap(); durable.append(&mut buffered); }
                LogOp::FlushTo(i) => {
                    let all: Vec<&(Lsn, LogRecord)> = durable.iter().chain(buffered.iter()).collect();
                    if !all.is_empty() {
                        let lsn = all[i % all.len()].0;
                        log.flush_to(lsn).unwrap();
                        prop_assert!(lsn < log.flushed_lsn(), "flush_to makes the record durable");
                        // flushing may write more than asked, never less
                        if lsn >= durable.last().map_or(0, |(l, r)| l + r.serialize().len() as u64) { durable.append(&mut buffered); }
                    }
                }
                LogOp::Restart => {
                    disk = disk.crash();
                    log = LogManager::new(disk.clone()).unwrap();
                    buffered.clear();
                    end = durable.last().map_or(0, |(l, r)| l + r.serialize().len() as u64);
                }
                LogOp::Tear(n) => {
                    disk = disk.crash_tearing(n);
                    log = LogManager::new(disk.clone()).unwrap();
                    buffered.clear();
                    // the cut may have destroyed the last durable records: what is left must be a prefix
                    let kept = log.records().unwrap();
                    prop_assert!(kept.len() <= durable.len());
                    durable.truncate(kept.len());
                    end = durable.last().map_or(0, |(l, r)| l + r.serialize().len() as u64);
                }
            }
            prop_assert_eq!(log.records().unwrap(), durable.clone());
            prop_assert_eq!(log.end_lsn(), end);
        }
    }
}

// ---- 4c-03 · putting a slot in a state ------------------------------------------------------------------------------------------------

#[test]
fn s4c_03_the_next_slot_is_created_and_an_existing_one_is_overwritten() {
    let w = World::fresh(2);
    let s = w.store(1);
    let p = w.pages[0];
    s.set_state(Rid::new(p, 0), &Some(rec(1))).unwrap();
    s.set_state(Rid::new(p, 1), &Some(rec(2))).unwrap();
    assert_eq!(s.get(Rid::new(p, 0)), Some(rec(1)));
    s.set_state(Rid::new(p, 0), &Some(rec(9))).unwrap();
    assert_eq!(s.get(Rid::new(p, 0)), Some(rec(9)));
    assert_eq!(s.scan().len(), 2, "overwriting adds no slot");
}

#[test]
fn s4c_03_none_marks_a_slot_deleted_and_some_brings_it_back() {
    let w = World::fresh(1);
    let s = w.store(1);
    let r = Rid::new(w.pages[0], 0);
    s.set_state(r, &Some(rec(1))).unwrap();
    s.set_state(r, &None).unwrap();
    assert_eq!(s.get(r), None);
    assert!(s.scan().is_empty());
    s.set_state(r, &Some(rec(2))).unwrap();
    assert_eq!(s.get(r), Some(rec(2)), "a deleted slot lives again with the new bytes");
}

#[test]
fn s4c_03_doing_it_twice_is_the_same_as_doing_it_once() {
    let w = World::fresh(1);
    let s = w.store(1);
    let p = w.pages[0];
    for _ in 0..3 {
        s.set_state(Rid::new(p, 0), &Some(rec(1))).unwrap();
        s.set_state(Rid::new(p, 1), &Some(rec(2))).unwrap();
        s.set_state(Rid::new(p, 1), &None).unwrap();
    }
    assert_eq!(contents(&s), BTreeMap::from([((p.0, 0), rec(1))]), "three rounds leave what one round leaves");
}

#[test]
fn s4c_03_a_slot_cannot_be_skipped_and_a_missing_one_cannot_be_deleted() {
    let w = World::fresh(1);
    let s = w.store(1);
    let p = w.pages[0];
    assert!(s.set_state(Rid::new(p, 3), &Some(rec(1))).is_err(), "slot 3 of an empty page");
    assert!(s.set_state(Rid::new(p, 0), &None).is_err(), "deleting a slot that does not exist");
    assert!(s.scan().is_empty(), "the failed calls changed nothing");
    s.set_state(Rid::new(p, 0), &Some(rec(1))).unwrap();
    assert!(s.set_state(Rid::new(p, 2), &Some(rec(2))).is_err());
}

#[test]
fn s4c_03_a_record_has_the_fixed_length() {
    let w = World::fresh(1);
    let s = w.store(1);
    let r = Rid::new(w.pages[0], 0);
    assert_eq!(RECORD_LEN, 16);
    assert!(s.set_state(r, &Some(vec![1; 5])).is_err());
    assert!(s.set_state(r, &Some(vec![1; 17])).is_err());
    assert!(s.scan().is_empty());
}

#[test]
fn s4c_03_pages_are_independent_and_the_state_survives_a_page_write_and_a_restart() {
    let w = World::fresh(2);
    let s = w.store(1);
    s.set_state(Rid::new(w.pages[0], 0), &Some(rec(1))).unwrap();
    s.set_state(Rid::new(w.pages[1], 0), &Some(rec(2))).unwrap();
    s.set_state(Rid::new(w.pages[1], 0), &None).unwrap();
    for p in &w.pages {
        w.bpm.flush_page(*p);
    }
    let w2 = w.crash_and_restart();
    let s2 = w2.store(1);
    assert_eq!(contents(&s2), BTreeMap::from([((w.pages[0].0, 0), rec(1))]), "what was written to the pages is what a restart reads");
}

proptest! {
    #![proptest_config(pconfig())]

    /// Slots of one page against a vector of `Option<record>`: setting slot `i` to a state does what the vector does (create the next slot,
    /// overwrite or delete an existing one) and refuses what the vector cannot (skipping a slot, deleting one that is not there).
    #[test]
    fn s4c_03_set_state_behaves_like_a_vector_of_optional_records(ops in prop::collection::vec((0usize..9, prop::option::of(any::<u64>())), 1..60)) {
        let w = World::fresh(1);
        let s = w.store(1);
        let p = w.pages[0];
        let mut model: Vec<Option<Vec<u8>>> = vec![];
        for (slot, state) in ops {
            let state = state.map(rec);
            let ok = match (slot.cmp(&model.len()), &state) {
                (std::cmp::Ordering::Less, _) => true,
                (std::cmp::Ordering::Equal, Some(_)) => true,
                _ => false,
            };
            let result = s.set_state(Rid::new(p, slot as u32), &state);
            prop_assert_eq!(result.is_ok(), ok, "slot {} of {} to {:?}", slot, model.len(), state);
            if ok {
                if slot == model.len() { model.push(state); } else { model[slot] = state; }
            }
            let want: BTreeMap<(i32, u32), Vec<u8>> = model.iter().enumerate().filter_map(|(i, v)| v.clone().map(|v| ((p.0, i as u32), v))).collect();
            prop_assert_eq!(contents(&s), want);
        }
    }
}

// ---- 4c-04 · logged writes -----------------------------------------------------------------------------------------------------------

fn durable(w: &World) -> Vec<LogRecord> {
    w.log.records().unwrap().into_iter().map(|(_, r)| r).collect()
}

#[test]
fn s4c_04_inserts_fill_the_first_page_slot_by_slot_and_read_back() {
    let w = World::fresh(2);
    let s = w.store(1);
    let t = s.begin();
    let rids: Vec<Rid> = (0..5).map(|i| s.insert(t, &rec(i)).unwrap()).collect();
    assert_eq!(rids, (0..5).map(|i| Rid::new(w.pages[0], i)).collect::<Vec<_>>());
    for (i, r) in rids.iter().enumerate() {
        assert_eq!(s.get(*r), Some(rec(i as u64)));
    }
    s.commit(t).unwrap();
}

#[test]
fn s4c_04_a_write_is_described_to_the_log_before_it_is_made() {
    let w = World::fresh(1);
    let s = w.store(1);
    let t = s.begin();
    let r = s.insert(t, &rec(1)).unwrap();
    s.update(t, r, &rec(2)).unwrap();
    s.delete(t, r).unwrap();
    w.log.flush().unwrap();
    assert_eq!(
        durable(&w),
        vec![
            LogRecord::Begin { txn: t },
            LogRecord::Change { txn: t, rid: r, before: None, after: Some(rec(1)) },
            LogRecord::Change { txn: t, rid: r, before: Some(rec(1)), after: Some(rec(2)) },
            LogRecord::Change { txn: t, rid: r, before: Some(rec(2)), after: None },
        ],
        "an insert is None -> Some, an update Some -> Some, a delete Some -> None, each with the record's old and new bytes"
    );
}

#[test]
fn s4c_04_commit_makes_the_transaction_durable_and_nothing_before_it_is_lost() {
    let w = World::fresh(1);
    let s = w.store(1);
    let (a, b) = (s.begin(), s.begin());
    let ra = s.insert(a, &rec(1)).unwrap();
    s.insert(b, &rec(2)).unwrap();
    s.commit(a).unwrap();
    let crashed = World::restart(w.disk.crash(), &w.pages);
    let log = durable(&crashed);
    assert!(log.contains(&LogRecord::Commit { txn: a }), "the commit record is on disk when commit returns");
    assert!(log.contains(&LogRecord::Change { txn: a, rid: ra, before: None, after: Some(rec(1)) }), "so is everything a transaction logged before it");
    assert!(log.contains(&LogRecord::Change { txn: b, rid: Rid::new(w.pages[0], 1), before: None, after: Some(rec(2)) }), "and what was logged before it by others");
    assert!(!log.contains(&LogRecord::Commit { txn: b }));
}

#[test]
fn s4c_04_a_page_reaches_the_disk_only_after_its_log_records_did() {
    let w = World::fresh(2);
    let s = w.store(1);
    let t = s.begin();
    let r0 = s.insert(t, &rec(1)).unwrap();
    s.update(t, r0, &rec(2)).unwrap();
    assert!(durable(&w).is_empty(), "nothing has been flushed yet");
    s.flush_page(w.pages[0]).unwrap();
    let crashed = World::restart(w.disk.crash(), &w.pages);
    assert!(durable(&crashed).contains(&LogRecord::Change { txn: t, rid: r0, before: Some(rec(1)), after: Some(rec(2)) }), "the page with the update is on disk, so the log record of the update must be too");
    assert!(w.disk.stored(w.pages[0].0).iter().any(|b| *b != 0), "and the page was written");
}

#[test]
fn s4c_04_abort_undoes_the_changes_newest_first_and_logs_each_undo() {
    let w = World::fresh(1);
    let s = w.store(1);
    let setup = s.begin();
    let r = s.insert(setup, &rec(1)).unwrap();
    s.commit(setup).unwrap();
    let t = s.begin();
    let new = s.insert(t, &rec(5)).unwrap();
    s.update(t, r, &rec(2)).unwrap();
    s.update(t, r, &rec(3)).unwrap();
    s.abort(t).unwrap();
    assert_eq!(contents(&s), BTreeMap::from([(key(r), rec(1))]), "the old record is back, the new one is gone");
    w.log.flush().unwrap();
    let log = durable(&w);
    let mine: Vec<&LogRecord> = log.iter().filter(|x| x.txn() == Some(t)).collect();
    assert_eq!(
        mine,
        vec![
            &LogRecord::Begin { txn: t },
            &LogRecord::Change { txn: t, rid: new, before: None, after: Some(rec(5)) },
            &LogRecord::Change { txn: t, rid: r, before: Some(rec(1)), after: Some(rec(2)) },
            &LogRecord::Change { txn: t, rid: r, before: Some(rec(2)), after: Some(rec(3)) },
            &LogRecord::Change { txn: t, rid: r, before: Some(rec(3)), after: Some(rec(2)) },
            &LogRecord::Change { txn: t, rid: r, before: Some(rec(2)), after: Some(rec(1)) },
            &LogRecord::Change { txn: t, rid: new, before: Some(rec(5)), after: None },
            &LogRecord::Abort { txn: t },
        ],
        "each undo is a change with the two states swapped, newest first, and Abort comes last"
    );
}

#[test]
fn s4c_04_a_record_written_by_another_active_transaction_is_refused_until_it_ends() {
    let w = World::fresh(1);
    let s = w.store(1);
    let (a, b) = (s.begin(), s.begin());
    let r = s.insert(a, &rec(1)).unwrap();
    assert!(s.update(b, r, &rec(2)).is_err(), "a has written it");
    assert!(s.delete(b, r).is_err());
    assert_eq!(s.get(r), Some(rec(1)), "the refused calls changed nothing");
    s.update(a, r, &rec(3)).unwrap();
    s.commit(a).unwrap();
    s.update(b, r, &rec(4)).unwrap();
    assert_eq!(s.get(r), Some(rec(4)));
    s.abort(b).unwrap();
    assert_eq!(s.get(r), Some(rec(3)));
}

#[test]
fn s4c_04_bad_calls_are_errors_and_leave_no_trace_in_the_log() {
    let w = World::fresh(1);
    let s = w.store(1);
    let t = s.begin();
    let r = s.insert(t, &rec(1)).unwrap();
    s.delete(t, r).unwrap();
    assert!(s.update(t, r, &rec(2)).is_err(), "updating a deleted record");
    assert!(s.delete(t, r).is_err(), "deleting twice");
    assert!(s.update(t, Rid::new(w.pages[0], 40), &rec(2)).is_err(), "no such slot");
    assert!(s.insert(t, &[1, 2, 3]).is_err(), "a record has 16 bytes");
    assert!(s.commit(999).is_err(), "no such transaction");
    s.commit(t).unwrap();
    assert!(s.commit(t).is_err(), "a transaction ends once");
    w.log.flush().unwrap();
    let changes = durable(&w).iter().filter(|x| matches!(x, LogRecord::Change { .. })).count();
    assert_eq!(changes, 2, "only the insert and the delete were logged");
}

/// What the committed and the uncommitted work should have made of the store, by record: committed state overlaid with every active
/// transaction's pending state (the store applies a write at once, and nobody else may touch the record).
#[derive(Default, Clone)]
struct Model {
    committed: BTreeMap<(i32, u32), Vec<u8>>,
    pending: HashMap<TxnId, BTreeMap<(i32, u32), Option<Vec<u8>>>>,
    counter: u64,
    known: Vec<Rid>,
    committed_txns: Vec<TxnId>,
}

impl Model {
    fn current(&self) -> BTreeMap<(i32, u32), Vec<u8>> {
        let mut m = self.committed.clone();
        for p in self.pending.values() {
            for (k, v) in p {
                match v {
                    Some(v) => m.insert(*k, v.clone()),
                    None => m.remove(k),
                };
            }
        }
        m
    }
    fn holder(&self, k: (i32, u32)) -> Option<TxnId> {
        self.pending.iter().find(|(_, p)| p.contains_key(&k)).map(|(t, _)| *t)
    }
    fn view(&self, t: TxnId, k: (i32, u32)) -> Option<Vec<u8>> {
        match self.pending.get(&t).and_then(|p| p.get(&k)) {
            Some(v) => v.clone(),
            None => self.committed.get(&k).cloned(),
        }
    }
    fn next_value(&mut self) -> Vec<u8> {
        self.counter += 1;
        rec(self.counter)
    }
    fn commit(&mut self, t: TxnId) {
        for (k, v) in self.pending.remove(&t).unwrap_or_default() {
            match v {
                Some(v) => self.committed.insert(k, v),
                None => self.committed.remove(&k),
            };
        }
    }
}

#[derive(Clone, Debug)]
enum Op {
    Begin,
    Insert(usize),
    Update(usize, usize),
    Delete(usize, usize),
    Commit(usize),
    Abort(usize),
    FlushPage(usize),
    FlushLog,
    Checkpoint,
}

fn op_strategy(checkpoints: bool) -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => Just(Op::Begin),
        4 => any::<usize>().prop_map(Op::Insert),
        4 => (any::<usize>(), any::<usize>()).prop_map(|(t, r)| Op::Update(t, r)),
        2 => (any::<usize>(), any::<usize>()).prop_map(|(t, r)| Op::Delete(t, r)),
        3 => any::<usize>().prop_map(Op::Commit),
        2 => any::<usize>().prop_map(Op::Abort),
        3 => any::<usize>().prop_map(Op::FlushPage),
        1 => Just(Op::FlushLog),
        if checkpoints { 1 } else { 0 } => Just(Op::Checkpoint),
    ]
}

/// Runs `ops` on `store` and the model side by side. Returns the model (and checks the store against it after every step).
fn run_ops(w: &World, store: &Store<'_>, ops: &[Op]) -> Result<Model, TestCaseError> {
    let mut model = Model::default();
    let mut active: Vec<TxnId> = vec![];
    for op in ops {
        match op {
            Op::Begin => {
                let t = store.begin();
                model.pending.insert(t, BTreeMap::new());
                active.push(t);
            }
            Op::Insert(t) if !active.is_empty() => {
                let t = active[t % active.len()];
                let v = model.next_value();
                match store.insert(t, &v) {
                    Ok(r) => {
                        prop_assert!(!model.current().contains_key(&key(r)), "an insert must not land on a live record");
                        model.pending.get_mut(&t).unwrap().insert(key(r), Some(v));
                        model.known.push(r);
                    }
                    Err(_) => { /* the store is full: fine */ }
                }
            }
            Op::Update(t, r) | Op::Delete(t, r) if !active.is_empty() && !model.known.is_empty() => {
                let t = active[t % active.len()];
                let rid = model.known[r % model.known.len()];
                let k = key(rid);
                let allowed = model.holder(k).is_none_or(|h| h == t) && model.view(t, k).is_some();
                let result = if let Op::Update(..) = op {
                    let v = model.next_value();
                    let res = store.update(t, rid, &v);
                    if res.is_ok() { model.pending.get_mut(&t).unwrap().insert(k, Some(v)); }
                    res
                } else {
                    let res = store.delete(t, rid);
                    if res.is_ok() { model.pending.get_mut(&t).unwrap().insert(k, None); }
                    res
                };
                prop_assert_eq!(result.is_ok(), allowed, "{:?} of {:?} by {} (held by {:?})", op, rid, t, model.holder(k));
            }
            Op::Commit(t) if !active.is_empty() => {
                let t = active.remove(t % active.len());
                store.commit(t).unwrap();
                model.commit(t);
                model.committed_txns.push(t);
            }
            Op::Abort(t) if !active.is_empty() => {
                let t = active.remove(t % active.len());
                store.abort(t).unwrap();
                model.pending.remove(&t);
            }
            Op::FlushPage(p) => store.flush_page(w.pages[p % w.pages.len()]).unwrap(),
            Op::FlushLog => w.log.flush().unwrap(),
            Op::Checkpoint => { store.checkpoint().unwrap(); }
            _ => continue,
        }
        prop_assert_eq!(contents(store), model.current(), "after {:?}", op);
    }
    Ok(model)
}

proptest! {
    #![proptest_config(pconfig())]

    /// Transactions begin, write, commit and roll back in any order, with page and log flushes in between: the store always holds the
    /// committed records overlaid with the active transactions' writes, a write to a record another active transaction has written is
    /// refused, and a rollback leaves exactly what the other transactions have.
    #[test]
    fn s4c_04_the_store_agrees_with_a_model_of_committed_and_pending_writes(ops in prop::collection::vec(op_strategy(false), 1..70)) {
        let w = World::fresh(2);
        let s = w.store(1);
        run_ops(&w, &s, &ops)?;
    }

    /// The write-ahead rule over random runs: at any moment, every page that is on disk has its log records on disk too, so that a crash
    /// can always be explained: no record on a disk page is missing from the durable log.
    #[test]
    fn s4c_04_no_page_on_disk_has_a_change_that_the_durable_log_does_not_know(ops in prop::collection::vec(op_strategy(false), 1..70)) {
        let w = World::fresh(2);
        let s = w.store(1);
        run_ops(&w, &s, &ops)?;
        let crashed = World::restart(w.disk.crash(), &w.pages);
        let known: Vec<LogRecord> = durable(&crashed);
        // every live record on the disk pages must be the `after` state of some durable change of that slot
        let on_disk = contents(&crashed.store(1));
        for (k, v) in on_disk {
            let explained = known.iter().any(|r| matches!(r, LogRecord::Change { rid, after: Some(a), .. } if key(*rid) == k && *a == v));
            prop_assert!(explained, "slot {:?} holds {:?} on disk but no durable log record put it there", k, v);
        }
    }
}

// ---- 4c-05 · after the crash: who finished, and repeating history ------------------------------------------------------------------

/// The state the durable records describe, applied in order from nothing: what "repeating history" must produce.
fn replay(records: &[(Lsn, LogRecord)]) -> BTreeMap<(i32, u32), Vec<u8>> {
    let mut m = BTreeMap::new();
    for (_, r) in records {
        if let LogRecord::Change { rid, after, .. } = r {
            match after {
                Some(v) => m.insert(key(*rid), v.clone()),
                None => m.remove(&key(*rid)),
            };
        }
    }
    m
}

fn numbered(records: Vec<LogRecord>) -> Vec<(Lsn, LogRecord)> {
    records.into_iter().enumerate().map(|(i, r)| (i as Lsn * 100, r)).collect()
}

#[test]
fn s4c_05_a_transaction_with_a_commit_or_an_abort_record_finished() {
    let log = numbered(vec![
        begin(1),
        LogRecord::Change { txn: 1, rid: rid(0, 0), before: None, after: Some(rec(1)) },
        begin(2),
        LogRecord::Commit { txn: 1 },
        begin(3),
        LogRecord::Change { txn: 3, rid: rid(0, 1), before: None, after: Some(rec(2)) },
        LogRecord::Abort { txn: 3 },
    ]);
    assert_eq!(analyse(&log), Analysis { redo_from: 0, finished: vec![1, 3], losers: vec![2] });
}

#[test]
fn s4c_05_losers_are_listed_in_the_order_they_began() {
    let log = numbered(vec![begin(5), begin(2), begin(9), LogRecord::Commit { txn: 2 }, begin(1)]);
    assert_eq!(analyse(&log).losers, vec![5, 9, 1]);
    assert_eq!(analyse(&log).finished, vec![2]);
}

#[test]
fn s4c_05_a_change_with_no_begin_still_belongs_to_a_loser() {
    // the Begin may be before a checkpoint, or its record lost with the buffer: the change is what counts
    let log = numbered(vec![LogRecord::Change { txn: 4, rid: rid(0, 0), before: None, after: Some(rec(1)) }]);
    assert_eq!(analyse(&log).losers, vec![4]);
    assert_eq!(analyse(&[]), Analysis::default(), "an empty log: nothing happened");
}

#[test]
fn s4c_05_redo_puts_back_what_no_page_ever_saw() {
    let w = World::fresh(2);
    let s = w.store(1);
    let (a, b) = (s.begin(), s.begin());
    let ra = s.insert(a, &rec(1)).unwrap();
    let rb = s.insert(b, &rec(2)).unwrap();
    s.update(a, ra, &rec(3)).unwrap();
    s.commit(a).unwrap(); // b is still active; the log is durable up to a's commit, no page was written
    let w2 = w.crash_and_restart();
    let s2 = w2.store(100);
    assert!(contents(&s2).is_empty(), "the pages on disk are empty");
    let records = w2.log.records().unwrap();
    let n = redo(&s2, &records, 0).unwrap();
    assert_eq!(n, 3, "three changes were logged durably");
    assert_eq!(contents(&s2), BTreeMap::from([(key(ra), rec(3)), (key(rb), rec(2))]), "history is repeated: b's insert comes back too (undoing it is the next stage)");
}

#[test]
fn s4c_05_redo_is_harmless_where_the_pages_already_have_the_change() {
    let w = World::fresh(1);
    let s = w.store(1);
    let t = s.begin();
    let r = s.insert(t, &rec(1)).unwrap();
    s.update(t, r, &rec(2)).unwrap();
    s.flush_all_pages().unwrap(); // the page has both changes already
    let r2 = s.insert(t, &rec(3)).unwrap(); // this one it does not have
    s.commit(t).unwrap();
    let w2 = w.crash_and_restart();
    let s2 = w2.store(100);
    assert_eq!(contents(&s2), BTreeMap::from([(key(r), rec(2))]));
    let records = w2.log.records().unwrap();
    redo(&s2, &records, 0).unwrap();
    assert_eq!(contents(&s2), BTreeMap::from([(key(r), rec(2)), (key(r2), rec(3))]));
}

#[test]
fn s4c_05_redo_twice_gives_what_redo_once_gives() {
    let w = World::fresh(2);
    let s = w.store(1);
    let t = s.begin();
    let r = s.insert(t, &rec(1)).unwrap();
    s.update(t, r, &rec(2)).unwrap();
    let r2 = s.insert(t, &rec(4)).unwrap();
    s.delete(t, r2).unwrap();
    s.commit(t).unwrap();
    let w2 = w.crash_and_restart();
    let s2 = w2.store(100);
    let records = w2.log.records().unwrap();
    redo(&s2, &records, 0).unwrap();
    let once = contents(&s2);
    redo(&s2, &records, 0).unwrap();
    assert_eq!(contents(&s2), once);
    assert_eq!(once, BTreeMap::from([(key(r), rec(2))]));
}

#[test]
fn s4c_05_redo_starts_where_it_is_told_to() {
    let w = World::fresh(1);
    let s = w.store(1);
    let t = s.begin();
    let r0 = s.insert(t, &rec(1)).unwrap();
    s.flush_all_pages().unwrap();
    let r1 = s.insert(t, &rec(2)).unwrap();
    s.commit(t).unwrap();
    let w2 = w.crash_and_restart();
    let s2 = w2.store(100);
    let records = w2.log.records().unwrap();
    let from = records.iter().position(|(_, r)| matches!(r, LogRecord::Change { rid, .. } if *rid == r1)).unwrap();
    assert_eq!(redo(&s2, &records, from).unwrap(), 1, "only the second change is re-applied");
    assert_eq!(contents(&s2), BTreeMap::from([(key(r0), rec(1)), (key(r1), rec(2))]));
}

proptest! {
    #![proptest_config(pconfig())]

    /// After any run (commits, rollbacks, transactions left open, page and log flushes) and a crash, analysis and redo bring the pages
    /// to **exactly the state the durable log describes** (repeating history, losers included), whichever pages had been written, and
    /// doing it again changes nothing.
    #[test]
    fn s4c_05_redo_reproduces_the_state_the_durable_log_describes(ops in prop::collection::vec(op_strategy(false), 1..70)) {
        let w = World::fresh(2);
        let s = w.store(1);
        run_ops(&w, &s, &ops)?;
        let w2 = w.crash_and_restart();
        let s2 = w2.store(10_000);
        let records = w2.log.records().unwrap();
        let analysis = analyse(&records);
        prop_assert_eq!(analysis.redo_from, 0);
        redo(&s2, &records, analysis.redo_from).unwrap();
        let want = replay(&records);
        prop_assert_eq!(contents(&s2), want.clone());
        redo(&s2, &records, 0).unwrap();
        prop_assert_eq!(contents(&s2), want);
    }
}

// ---- 4c-06 · undoing the losers --------------------------------------------------------------------------------------------------------

fn full_recovery(crashed: &World) -> (World, bustub::recovery::recover::Recovery) {
    let w2 = World::restart(crashed.disk.crash(), &crashed.pages);
    let report = {
        let s2 = w2.store(10_000);
        recover(&s2, &w2.log).unwrap()
    };
    (w2, report)
}

#[test]
fn s4c_06_a_losers_insert_update_and_delete_are_all_rolled_back() {
    let w = World::fresh(1);
    let s = w.store(1);
    let setup = s.begin();
    let keep = s.insert(setup, &rec(1)).unwrap();
    let edit = s.insert(setup, &rec(2)).unwrap();
    let drop_me = s.insert(setup, &rec(3)).unwrap();
    s.commit(setup).unwrap();
    let t = s.begin();
    s.insert(t, &rec(10)).unwrap();
    s.update(t, edit, &rec(20)).unwrap();
    s.delete(t, drop_me).unwrap();
    w.log.flush().unwrap(); // the loser's records are durable, its commit never happened
    let (w2, report) = full_recovery(&w);
    let s2 = w2.store(99_000);
    assert_eq!(contents(&s2), BTreeMap::from([(key(keep), rec(1)), (key(edit), rec(2)), (key(drop_me), rec(3))]));
    assert_eq!(report.losers, vec![t]);
    assert_eq!(report.finished, vec![setup]);
    assert_eq!(report.undone, 3);
}

#[test]
fn s4c_06_undo_logs_what_it_does_and_ends_each_loser_with_abort() {
    let w = World::fresh(1);
    let s = w.store(1);
    let t = s.begin();
    let r = s.insert(t, &rec(1)).unwrap();
    s.update(t, r, &rec(2)).unwrap();
    w.log.flush().unwrap();
    let (w2, _) = full_recovery(&w);
    let log = w2.log.records().unwrap();
    let mine: Vec<LogRecord> = log.into_iter().map(|(_, r)| r).filter(|r| r.txn() == Some(t)).collect();
    assert_eq!(mine[mine.len() - 3..], [
        LogRecord::Change { txn: t, rid: r, before: Some(rec(2)), after: Some(rec(1)) },
        LogRecord::Change { txn: t, rid: r, before: Some(rec(1)), after: None },
        LogRecord::Abort { txn: t },
    ], "newest change first, each undo logged as the swapped change, then Abort, all durable");
}

#[test]
fn s4c_06_several_losers_and_a_winner_in_the_same_pages() {
    let w = World::fresh(2);
    let s = w.store(1);
    let (a, b, c) = (s.begin(), s.begin(), s.begin());
    let ra = s.insert(a, &rec(1)).unwrap();
    let rb = s.insert(b, &rec(2)).unwrap();
    let rc = s.insert(c, &rec(3)).unwrap();
    s.update(b, rb, &rec(22)).unwrap();
    s.commit(c).unwrap();
    s.flush_page(w.pages[0]).unwrap(); // the loser's data is on disk
    let (w2, report) = full_recovery(&w);
    assert_eq!(contents(&w2.store(1)), BTreeMap::from([(key(rc), rec(3))]), "only c's record is left");
    assert_eq!(report.losers, vec![a, b]);
    let _ = ra;
}

#[test]
fn s4c_06_recovery_leaves_the_pages_on_disk_so_a_second_crash_needs_no_log() {
    let w = World::fresh(1);
    let s = w.store(1);
    let t = s.begin();
    let r = s.insert(t, &rec(1)).unwrap();
    s.commit(t).unwrap();
    let u = s.begin();
    s.insert(u, &rec(2)).unwrap();
    w.log.flush().unwrap();
    let (w2, _) = full_recovery(&w);
    // crash again at once and read the pages without any recovery
    let w3 = World::restart(w2.disk.crash(), &w.pages);
    assert_eq!(contents(&w3.store(1)), BTreeMap::from([(key(r), rec(1))]));
}

#[test]
fn s4c_06_recovering_twice_changes_nothing_the_second_time() {
    let w = World::fresh(2);
    let s = w.store(1);
    let (a, b) = (s.begin(), s.begin());
    s.insert(a, &rec(1)).unwrap();
    let rb = s.insert(b, &rec(2)).unwrap();
    s.commit(b).unwrap();
    s.update(b.max(a), rb, &rec(3)).ok();
    w.log.flush().unwrap();
    let (w2, first) = full_recovery(&w);
    let state = contents(&w2.store(1));
    assert_eq!(first.losers, vec![a]);
    let (w3, second) = full_recovery(&w2);
    assert_eq!(contents(&w3.store(1)), state);
    assert!(second.losers.is_empty(), "the loser has an Abort record now");
    assert_eq!(second.undone, 0);
}

#[test]
fn s4c_06_a_rollback_that_was_half_logged_when_the_crash_came_is_finished() {
    let w = World::fresh(1);
    let s = w.store(1);
    let t = s.begin();
    let r0 = s.insert(t, &rec(1)).unwrap();
    let r1 = s.insert(t, &rec(2)).unwrap();
    // the rollback began: the newest change was undone and logged, then the machine died (no Abort record)
    w.log.append(&LogRecord::Change { txn: t, rid: r1, before: Some(rec(2)), after: None });
    w.log.flush().unwrap();
    let (w2, report) = full_recovery(&w);
    assert!(contents(&w2.store(1)).is_empty(), "both inserts are rolled back");
    assert_eq!(report.losers, vec![t]);
    assert_eq!(w2.store(1).get(r0), None);
}

proptest! {
    #![proptest_config(pconfig())]

    /// Any run, then a crash at that moment (whichever pages and log records had reached the disk): recovery leaves exactly the records
    /// of the transactions that committed. Every committed transaction is reported as finished, and recovering again is a no-op.
    #[test]
    fn s4c_06_recovery_leaves_exactly_the_committed_transactions(ops in prop::collection::vec(op_strategy(false), 1..80)) {
        let w = World::fresh(2);
        let s = w.store(1);
        let mut model = run_ops(&w, &s, &ops)?;
        // what is still pending belongs to transactions that never finished
        model.pending.clear();
        let (w2, report) = full_recovery(&w);
        prop_assert_eq!(contents(&w2.store(1)), model.committed.clone());
        for t in &report.losers {
            prop_assert!(!model.committed_txns.contains(t), "{} committed, so it cannot be a loser", t);
        }
        for t in &model.committed_txns {
            prop_assert!(report.finished.contains(t), "{} committed and must be finished", t);
        }
        let (w3, second) = full_recovery(&w2);
        prop_assert_eq!(contents(&w3.store(1)), model.committed);
        prop_assert!(second.losers.is_empty() && second.undone == 0);
    }
}

// ---- 4c-07 · checkpoints ---------------------------------------------------------------------------------------------------------------

#[test]
fn s4c_07_a_checkpoint_writes_the_log_and_every_page() {
    let w = World::fresh(2);
    let s = w.store(1);
    let t = s.begin();
    let r = s.insert(t, &rec(1)).unwrap();
    let lsn = s.checkpoint().unwrap();
    let crashed = World::restart(w.disk.crash(), &w.pages);
    assert_eq!(contents(&crashed.store(1)), BTreeMap::from([(key(r), rec(1))]), "the page is on disk, with an uncommitted record in it");
    let log = crashed.log.records().unwrap();
    assert!(log.iter().any(|(l, rec)| *l == lsn && matches!(rec, LogRecord::Checkpoint { .. })), "the checkpoint record is durable at the LSN returned");
    assert!(crashed.log.records().unwrap().iter().any(|(_, x)| matches!(x, LogRecord::Change { rid, .. } if *rid == r)), "and so are the changes the pages contain");
}

#[test]
fn s4c_07_a_checkpoint_names_the_transactions_that_are_active() {
    let w = World::fresh(1);
    let s = w.store(1);
    let (a, b, c) = (s.begin(), s.begin(), s.begin());
    s.commit(b).unwrap();
    s.abort(c).unwrap();
    s.checkpoint().unwrap();
    let last = w.log.records().unwrap().into_iter().rev().find_map(|(_, r)| match r { LogRecord::Checkpoint { active } => Some(active), _ => None }).unwrap();
    assert_eq!(last, vec![a], "only a is still running");
}

#[test]
fn s4c_07_analysis_starts_redo_at_the_last_checkpoint() {
    let log = numbered(vec![begin(1), LogRecord::Checkpoint { active: vec![1] }, begin(2), LogRecord::Checkpoint { active: vec![1, 2] }, LogRecord::Commit { txn: 1 }]);
    let a = analyse(&log);
    assert_eq!(a.redo_from, 3, "the second checkpoint");
    assert_eq!(a.losers, vec![2]);
    assert_eq!(a.finished, vec![1]);
    assert_eq!(analyse(&numbered(vec![begin(1)])).redo_from, 0, "no checkpoint: from the start");
}

#[test]
fn s4c_07_a_transaction_active_at_the_checkpoint_is_a_loser_even_if_its_begin_is_not_read() {
    let log = numbered(vec![LogRecord::Checkpoint { active: vec![7, 8] }, LogRecord::Commit { txn: 8 }]);
    let a = analyse(&log);
    assert_eq!(a.losers, vec![7], "7 was running at the checkpoint and never finished");
    assert_eq!(a.finished, vec![8]);
}

#[test]
fn s4c_07_recovery_after_a_checkpoint_redoes_only_what_came_after_it() {
    let w = World::fresh(1);
    let s = w.store(1);
    let t = s.begin();
    for i in 0..10 {
        s.insert(t, &rec(i)).unwrap();
    }
    s.commit(t).unwrap();
    s.checkpoint().unwrap();
    let u = s.begin();
    for i in 10..13 {
        s.insert(u, &rec(i)).unwrap();
    }
    s.commit(u).unwrap();
    let (w2, report) = full_recovery(&w);
    assert_eq!(report.redone, 3, "ten changes are before the checkpoint, three after it");
    assert_eq!(contents(&w2.store(1)).len(), 13);
}

#[test]
fn s4c_07_a_transaction_that_spans_a_checkpoint_is_rolled_back_whole() {
    let w = World::fresh(1);
    let s = w.store(1);
    let setup = s.begin();
    let base = s.insert(setup, &rec(0)).unwrap();
    s.commit(setup).unwrap();
    let t = s.begin();
    s.insert(t, &rec(1)).unwrap();
    s.update(t, base, &rec(9)).unwrap();
    s.checkpoint().unwrap();
    s.insert(t, &rec(2)).unwrap();
    w.log.flush().unwrap();
    let (w2, report) = full_recovery(&w);
    assert_eq!(contents(&w2.store(1)), BTreeMap::from([(key(base), rec(0))]), "the changes before the checkpoint (on disk already) and after it are all undone");
    assert_eq!(report.undone, 3);
}

#[test]
fn s4c_07_a_transaction_that_began_before_the_checkpoint_and_committed_after_it_is_kept() {
    let w = World::fresh(1);
    let s = w.store(1);
    let t = s.begin();
    let r = s.insert(t, &rec(1)).unwrap();
    s.checkpoint().unwrap();
    let r2 = s.insert(t, &rec(2)).unwrap();
    s.commit(t).unwrap();
    let (w2, _) = full_recovery(&w);
    assert_eq!(contents(&w2.store(1)), BTreeMap::from([(key(r), rec(1)), (key(r2), rec(2))]));
}

proptest! {
    #![proptest_config(pconfig())]

    /// Runs with checkpoints at random moments: recovery leaves exactly the committed records, **the same as recovery that ignores the
    /// checkpoints** (redo from the start), and redoes no more changes than were logged after the last checkpoint.
    #[test]
    fn s4c_07_checkpoints_change_how_much_is_redone_never_what_the_result_is(ops in prop::collection::vec(op_strategy(true), 1..80)) {
        let w = World::fresh(2);
        let s = w.store(1);
        let mut model = run_ops(&w, &s, &ops)?;
        model.pending.clear();
        let (w2, report) = full_recovery(&w);
        prop_assert_eq!(contents(&w2.store(1)), model.committed.clone());
        // the same crash, recovered by hand without the checkpoints
        let w3 = World::restart(w.disk.crash(), &w.pages);
        let s3 = w3.store(10_000);
        let records = w3.log.records().unwrap();
        let analysis = analyse(&records);
        let after = records[analysis.redo_from..].iter().filter(|(_, r)| matches!(r, LogRecord::Change { .. })).count();
        prop_assert_eq!(report.redone, after);
        redo(&s3, &records, 0).unwrap();
        undo(&s3, &w3.log, &records, &analysis.losers).unwrap();
        prop_assert_eq!(contents(&s3), model.committed);
    }
}

// ---- 4c-08 · boss: crash at every moment -----------------------------------------------------------------------------------------

/// A bank: four accounts of 100 in one page. A balance is the first eight bytes of a record.
fn account(balance: u64) -> Vec<u8> {
    let mut v = balance.to_le_bytes().to_vec();
    v.extend_from_slice(&[0; 8]);
    v
}

fn balance(v: &[u8]) -> u64 {
    u64::from_le_bytes(v[0..8].try_into().unwrap())
}

#[derive(Clone, Copy, Debug)]
enum Bank {
    Begin,
    Debit(usize, u64),
    Credit(usize, u64),
    Commit,
    Abort,
}

/// Transfers: some commit, one aborts, one is cut short by the crash (the last, if the crash comes before its end).
fn bank_script() -> Vec<Bank> {
    use Bank::*;
    vec![
        Begin, Debit(0, 30), Credit(1, 30), Commit,
        Begin, Debit(1, 50), Credit(2, 50), Abort,
        Begin, Debit(2, 10), Credit(3, 10), Commit,
        Begin, Debit(3, 70), Credit(0, 70), Commit,
        Begin, Debit(0, 5), Credit(1, 5),
    ]
}

#[test]
fn s4c_08_money_is_neither_lost_nor_made_whenever_the_crash_comes() {
    let script = bank_script();
    for flush_mode in 0..3 {
        for crash_after in 0..=script.len() {
            let w = World::fresh(1);
            let s = w.store(1);
            let setup = s.begin();
            let rids: Vec<Rid> = (0..4).map(|_| s.insert(setup, &account(100)).unwrap()).collect();
            s.commit(setup).unwrap();
            let mut balances = [100u64; 4];
            let mut committed = balances;
            let mut txn = None;
            for (i, step) in script.iter().take(crash_after).enumerate() {
                match *step {
                    Bank::Begin => txn = Some(s.begin()),
                    Bank::Debit(a, n) => { balances[a] -= n; s.update(txn.unwrap(), rids[a], &account(balances[a])).unwrap(); }
                    Bank::Credit(a, n) => { balances[a] += n; s.update(txn.unwrap(), rids[a], &account(balances[a])).unwrap(); }
                    Bank::Commit => { s.commit(txn.take().unwrap()).unwrap(); committed = balances; }
                    Bank::Abort => { s.abort(txn.take().unwrap()).unwrap(); balances = committed; }
                }
                match flush_mode {
                    1 => s.flush_all_pages().unwrap(),
                    2 if i % 2 == 1 => s.flush_page(w.pages[0]).unwrap(),
                    _ => {}
                }
            }
            let (w2, _) = full_recovery(&w);
            let after: Vec<u64> = rids.iter().map(|r| balance(&w2.store(1).get(*r).expect("the account exists"))).collect();
            assert_eq!(after.iter().sum::<u64>(), 400, "money was lost or made (crash after step {crash_after}, flush mode {flush_mode}): {after:?}");
            assert_eq!(after, committed.to_vec(), "exactly the committed transfers survive (crash after step {crash_after}, flush mode {flush_mode})");
        }
    }
}

#[test]
fn s4c_08_a_garbage_tail_on_the_log_is_ignored_and_does_not_get_in_the_way_of_new_work() {
    let w = World::fresh(1);
    let s = w.store(1);
    let t = s.begin();
    let r = s.insert(t, &rec(1)).unwrap();
    s.commit(t).unwrap();
    let u = s.begin();
    s.insert(u, &rec(2)).unwrap();
    w.log.flush().unwrap();
    let crashed = w.disk.crash();
    let mut bytes = crashed.log_bytes();
    bytes.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef, 3, 0, 0, 0, 1]); // a record that was being written when the power went
    crashed.set_log_bytes(bytes);
    let w2 = World::restart(crashed, &w.pages);
    {
        let s2 = w2.store(500);
        recover(&s2, &w2.log).unwrap();
        assert_eq!(contents(&s2), BTreeMap::from([(key(r), rec(1))]));
        let t2 = s2.begin();
        let r2 = s2.insert(t2, &rec(7)).unwrap();
        s2.commit(t2).unwrap();
        assert_eq!(r2, Rid::new(w.pages[0], 2), "the aborted insert left its slot behind");
    }
    let (w3, _) = full_recovery(&w2);
    assert_eq!(contents(&w3.store(1)).len(), 2, "the new transaction survived a second crash: the log after the garbage is readable");
}

#[test]
fn s4c_08_a_crash_between_redo_and_undo_is_recovered_like_any_other() {
    let w = World::fresh(2);
    let s = w.store(1);
    let (a, b) = (s.begin(), s.begin());
    let ra = s.insert(a, &rec(1)).unwrap();
    s.insert(b, &rec(2)).unwrap();
    s.update(a, ra, &rec(3)).unwrap();
    s.commit(a).unwrap();
    // recovery gets as far as repeating history and writing the pages; then the power goes again
    let w2 = World::restart(w.disk.crash(), &w.pages);
    {
        let s2 = w2.store(10_000);
        let records = w2.log.records().unwrap();
        redo(&s2, &records, analyse(&records).redo_from).unwrap();
        s2.flush_all_pages().unwrap();
    }
    let (w3, report) = full_recovery(&w2);
    assert_eq!(contents(&w3.store(1)), BTreeMap::from([(key(ra), rec(3))]));
    assert_eq!(report.losers, vec![b]);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 60, max_shrink_iters: 3000, failure_persistence: None, ..ProptestConfig::default() })]

    /// The whole thing: random runs with checkpoints, flushes, rollbacks and open transactions; a crash; a garbage tail on the log; and
    /// a crash in the middle of recovery (after redo, before undo). After the final recovery the store holds exactly the committed records.
    #[test]
    fn s4c_08_crash_recovery_leaves_exactly_the_committed_state(ops in prop::collection::vec(op_strategy(true), 1..100), garbage in prop::collection::vec(any::<u8>(), 0..24), die_after_redo in any::<bool>()) {
        let w = World::fresh(2);
        let s = w.store(1);
        let mut model = run_ops(&w, &s, &ops)?;
        model.pending.clear();
        let crashed = w.disk.crash();
        let mut bytes = crashed.log_bytes();
        bytes.extend_from_slice(&garbage);
        crashed.set_log_bytes(bytes);
        let mut w2 = World::restart(crashed, &w.pages);
        if die_after_redo {
            {
                let s2 = w2.store(10_000);
                let records = w2.log.records().unwrap();
                redo(&s2, &records, analyse(&records).redo_from).unwrap();
                s2.flush_all_pages().unwrap();
            }
            w2 = World::restart(w2.disk.crash(), &w.pages);
        }
        let report = {
            let s2 = w2.store(10_000);
            recover(&s2, &w2.log).unwrap()
        };
        for t in &model.committed_txns {
            prop_assert!(report.finished.contains(t), "{} committed", t);
        }
        prop_assert_eq!(contents(&w2.store(1)), model.committed.clone());
        // and the log is usable afterwards: new work, another crash, another recovery
        let after = {
            let s2 = w2.store(20_000);
            let t = s2.begin();
            let r = s2.insert(t, &rec(424_242)).ok();
            if r.is_some() { s2.commit(t).unwrap(); } else { s2.abort(t).unwrap(); }
            r
        };
        let (w3, _) = full_recovery(&w2);
        let mut want = model.committed;
        if let Some(r) = after { want.insert(key(r), rec(424_242)); }
        prop_assert_eq!(contents(&w3.store(1)), want);
    }
}

// @@ challenge 4c-c1 begin
mod ch_4c_c1 {
    use proptest::prelude::*;

    use bustub::recovery::truncation::truncation_lsn;

    #[test]
    fn s4c_c1_each_bound_can_be_the_one_that_matters() {
        assert_eq!(truncation_lsn(100, &[], &[]), 100);
        assert_eq!(truncation_lsn(100, &[90, 120], &[110]), 90, "a page dirty since 90 needs redo from 90");
        assert_eq!(truncation_lsn(100, &[150], &[95]), 95, "an active transaction may need undo back to 95");
        assert_eq!(truncation_lsn(100, &[150], &[180]), 100, "the checkpoint start itself");
    }

    #[test]
    fn s4c_c1_a_checkpoint_at_zero_keeps_everything() {
        assert_eq!(truncation_lsn(0, &[5], &[7]), 0);
    }

    #[test]
    fn s4c_c1_removing_the_oldest_dirty_page_raises_the_bound() {
        assert_eq!(truncation_lsn(100, &[40, 60], &[]), 40);
        assert_eq!(truncation_lsn(100, &[60], &[]), 60);
        assert_eq!(truncation_lsn(100, &[], &[]), 100);
    }

    #[test]
    fn s4c_c1_the_smallest_of_all_three_wins_whatever_the_order_of_the_lists() {
        assert_eq!(truncation_lsn(100, &[70, 90], &[80, 60]), 60);
        assert_eq!(truncation_lsn(100, &[90, 70], &[60, 80]), 60);
        assert_eq!(truncation_lsn(50, &[70, 90], &[80, 60]), 50);
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: the minimum of all, never above any input, and adding an element only lowers it.
        #[test]
        fn s4c_c1_property_it_is_the_minimum_of_everything(cp in 0u64..100, dirty in proptest::collection::vec(0u64..200, 0..5), active in proptest::collection::vec(0u64..200, 0..5), extra in 0u64..200) {
            let r = truncation_lsn(cp, &dirty, &active);
            prop_assert_eq!(r, dirty.iter().chain(&active).copied().chain(Some(cp)).min().unwrap());
            let mut more = dirty.clone();
            more.push(extra);
            prop_assert!(truncation_lsn(cp, &more, &active) <= r);
            prop_assert!(truncation_lsn(cp + 1, &dirty, &active) >= r);
        }
    }
}
// @@ challenge 4c-c1 end

// @@ challenge 4c-c2 begin
mod ch_4c_c2 {
    use proptest::prelude::*;

    use bustub::recovery::group_commit::GroupCommit;

    #[test]
    fn s4c_c2_a_full_queue_is_flushed_at_once() {
        let mut g = GroupCommit::new(3, 10);
        g.submit(1, 0);
        g.submit(2, 1);
        assert_eq!(g.poll(2), None);
        g.submit(3, 2);
        assert_eq!(g.poll(2), Some(vec![1, 2, 3]));
        assert_eq!(g.pending(), 0);
    }

    #[test]
    fn s4c_c2_the_oldest_commit_never_waits_longer_than_the_limit() {
        let mut g = GroupCommit::new(10, 10);
        g.submit(4, 5);
        assert_eq!(g.poll(14), None);
        assert_eq!(g.poll(15), Some(vec![4]));
    }

    #[test]
    fn s4c_c2_a_newer_commit_rides_with_an_older_one_that_timed_out() {
        let mut g = GroupCommit::new(10, 10);
        g.submit(1, 0);
        g.submit(2, 9);
        assert_eq!(g.poll(10), Some(vec![1, 2]), "the oldest has waited 10: the whole queue goes");
    }

    #[test]
    fn s4c_c2_polling_an_empty_queue_and_polling_twice() {
        let mut g = GroupCommit::new(2, 5);
        assert_eq!(g.poll(100), None);
        g.submit(1, 0);
        g.submit(2, 0);
        assert!(g.poll(0).is_some());
        assert_eq!(g.poll(0), None);
    }

    #[test]
    fn s4c_c2_flush_all_returns_everything_in_order() {
        let mut g = GroupCommit::new(100, 100);
        for l in [5, 6, 7] {
            g.submit(l, 0);
        }
        assert_eq!(g.flush_all(), vec![5, 6, 7]);
        assert_eq!(g.flush_all(), Vec::<u64>::new());
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: every submission is acknowledged exactly once and in order, and only when a trigger held.
        #[test]
        fn s4c_c2_property_everything_is_acknowledged_once_in_order(max_batch in 1usize..5, max_wait in 0u64..8, steps in proptest::collection::vec((0u64..4, 0u8..3), 0..40)) {
            let mut g = GroupCommit::new(max_batch, max_wait);
            let (mut now, mut next_lsn) = (0u64, 1u64);
            let mut submitted: Vec<(u64, u64)> = Vec::new();
            let mut acked: Vec<u64> = Vec::new();
            for (dt, op) in steps {
                now += dt;
                if op < 2 {
                    g.submit(next_lsn, now);
                    submitted.push((next_lsn, now));
                    next_lsn += 1;
                }
                if let Some(batch) = g.poll(now) {
                    let waiting = &submitted[acked.len()..];
                    prop_assert!(!batch.is_empty());
                    prop_assert!(waiting.len() >= max_batch || now - waiting[0].1 >= max_wait, "flushed with no trigger");
                    acked.extend(batch);
                }
                prop_assert_eq!(g.pending(), submitted.len() - acked.len());
            }
            acked.extend(g.flush_all());
            prop_assert_eq!(acked, submitted.iter().map(|s| s.0).collect::<Vec<_>>());
        }
    }
}
// @@ challenge 4c-c2 end

// @@ challenge 4c-c3 begin
mod ch_4c_c3 {
    use proptest::prelude::*;

    use bustub::recovery::redo_pass::redo;

    #[test]
    fn s4c_c3_a_page_that_is_ahead_of_a_record_is_left_alone() {
        let mut pages = vec![(5, 100)];
        redo(&mut pages, &[(3, 0, 1), (7, 0, 2)]);
        assert_eq!(pages, vec![(7, 2)]);
    }

    #[test]
    fn s4c_c3_redo_twice_is_the_same_as_once() {
        let log = [(1, 0, 10), (2, 1, 20), (3, 0, 30)];
        let mut once = vec![(0, 0), (0, 0)];
        redo(&mut once, &log);
        let mut twice = once.clone();
        redo(&mut twice, &log);
        assert_eq!(once, twice);
        assert_eq!(once, vec![(3, 30), (2, 20)]);
    }

    #[test]
    fn s4c_c3_a_page_flushed_at_the_record_is_not_applied_again() {
        let mut pages = vec![(4, 40)];
        redo(&mut pages, &[(4, 0, 999)]);
        assert_eq!(pages, vec![(4, 40)], "the same LSN has already been applied");
    }

    #[test]
    fn s4c_c3_unrelated_pages_are_independent() {
        let mut pages = vec![(10, 1), (0, 0)];
        redo(&mut pages, &[(2, 0, 7), (3, 1, 8)]);
        assert_eq!(pages, vec![(10, 1), (3, 8)]);
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: starting from pages that already reflect a prefix of the log gives the same result as redoing the whole log from empty pages.
        #[test]
        fn s4c_c3_property_a_prefix_already_applied_changes_nothing(recs in proptest::collection::vec((0usize..3, -9i64..9), 0..20), cut in 0usize..21) {
            let log: Vec<(u64, usize, i64)> = recs.iter().enumerate().map(|(i, &(p, v))| (i as u64 + 1, p, v)).collect();
            let mut full = vec![(0u64, 0i64); 3];
            redo(&mut full, &log);
            let cut = cut.min(log.len());
            let mut partial = vec![(0u64, 0i64); 3];
            redo(&mut partial, &log[..cut]);
            redo(&mut partial, &log);
            prop_assert_eq!(&partial, &full);
            let before = full.clone();
            redo(&mut full, &log);
            prop_assert_eq!(full, before);
        }
    }
}
// @@ challenge 4c-c3 end

// @@ challenge 4c-c4 begin
mod ch_4c_c4 {
    use proptest::prelude::*;

    use bustub::recovery::pitr::{recover_until, Rec::*};
    use std::collections::BTreeMap;

    fn log() -> Vec<(u64, bustub::recovery::pitr::Rec)> {
        vec![(1, Begin(1)), (2, Set(1, 7, 1)), (4, Commit(1)), (5, Begin(2)), (6, Set(2, 7, 2)), (7, Set(2, 8, 5)), (8, Commit(2))]
    }

    #[test]
    fn s4c_c4_a_commit_after_the_point_does_not_count() {
        assert_eq!(recover_until(&log(), 5), BTreeMap::from([(7, 1)]));
        assert_eq!(recover_until(&log(), 7), BTreeMap::from([(7, 1)]), "writes of an uncommitted transaction are not applied");
    }

    #[test]
    fn s4c_c4_the_whole_log_gives_the_full_recovery() {
        assert_eq!(recover_until(&log(), 8), BTreeMap::from([(7, 2), (8, 5)]));
        assert_eq!(recover_until(&log(), u64::MAX), BTreeMap::from([(7, 2), (8, 5)]));
    }

    #[test]
    fn s4c_c4_before_the_first_commit_nothing_is_there() {
        assert_eq!(recover_until(&log(), 3), BTreeMap::new());
        assert_eq!(recover_until(&[], 100), BTreeMap::new());
    }

    #[test]
    fn s4c_c4_aborted_transactions_never_appear() {
        let l = vec![(1, Begin(1)), (2, Set(1, 1, 1)), (3, Abort(1)), (4, Begin(2)), (5, Set(2, 2, 2)), (6, Commit(2))];
        assert_eq!(recover_until(&l, 100), BTreeMap::from([(2, 2)]));
    }

    #[test]
    fn s4c_c4_interleaved_transactions_apply_in_log_order() {
        let l = vec![(1, Set(2, 5, 20)), (2, Set(1, 5, 10)), (3, Commit(1)), (4, Commit(2))];
        assert_eq!(recover_until(&l, 100), BTreeMap::from([(5, 10)]), "the writes are applied in log order: transaction 1 wrote last, though transaction 2 committed last");
        assert_eq!(recover_until(&l, 3), BTreeMap::from([(5, 10)]));
        assert_eq!(recover_until(&l, 2), BTreeMap::new());
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: the state as of a point equals recovering a log cut at that point, and raising the point only adds committed transactions.
        #[test]
        fn s4c_c4_property_recovery_depends_only_on_the_prefix(events in proptest::collection::vec((0u32..3, 0u8..4, 0i64..3, 0i64..9), 0..20), cut in 0u64..25) {
            let mut log = Vec::new();
            let mut finished = std::collections::BTreeSet::new();
            for (i, (t, kind, k, v)) in events.into_iter().enumerate() {
                let lsn = i as u64 + 1;
                if finished.contains(&t) { continue; }
                match kind {
                    0 | 1 => log.push((lsn, Set(t, k, v))),
                    2 => { log.push((lsn, Commit(t))); finished.insert(t); }
                    _ => { log.push((lsn, Abort(t))); finished.insert(t); }
                }
            }
            let prefix: Vec<_> = log.iter().filter(|(l, _)| *l <= cut).cloned().collect();
            prop_assert_eq!(recover_until(&log, cut), recover_until(&prefix, cut));
            prop_assert_eq!(recover_until(&log, 1000), recover_until(&log, u64::MAX));
        }
    }
}
// @@ challenge 4c-c4 end

// @@ challenge 4c-c5 begin
mod ch_4c_c5 {
    use proptest::prelude::*;

    use bustub::recovery::analysis::{analyze, Analysis, LogRec::*};
    use std::collections::{BTreeMap, BTreeSet};

    fn a(active: &[u32], dirty: &[(u32, u64)]) -> Analysis {
        Analysis { active: active.iter().copied().collect::<BTreeSet<_>>(), dirty: dirty.iter().copied().collect::<BTreeMap<_, _>>() }
    }

    #[test]
    fn s4c_c5_the_checkpoint_state_is_carried_forward() {
        let cp = a(&[1], &[(0, 10)]);
        let after = vec![(12, Update(1, 0)), (13, Begin(2)), (14, Update(2, 5)), (15, Commit(1))];
        assert_eq!(analyze(&cp, &after), a(&[2], &[(0, 10), (5, 14)]));
    }

    #[test]
    fn s4c_c5_a_page_keeps_the_lsn_of_its_first_dirtying() {
        let after = vec![(1, Update(1, 7)), (2, Update(1, 7)), (3, Update(2, 7))];
        assert_eq!(analyze(&Analysis::default(), &after).dirty, BTreeMap::from([(7, 1)]));
    }

    #[test]
    fn s4c_c5_a_transaction_first_seen_in_an_update_is_active() {
        let after = vec![(5, Update(9, 1))];
        assert_eq!(analyze(&Analysis::default(), &after).active, BTreeSet::from([9]));
    }

    #[test]
    fn s4c_c5_commit_and_abort_both_end_a_transaction() {
        let after = vec![(1, Begin(1)), (2, Begin(2)), (3, Commit(1)), (4, Abort(2))];
        assert_eq!(analyze(&Analysis::default(), &after).active, BTreeSet::new());
    }

    #[test]
    fn s4c_c5_no_records_after_the_checkpoint_gives_the_checkpoint() {
        let cp = a(&[3], &[(1, 2)]);
        assert_eq!(analyze(&cp, &[]), cp);
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: analysing a whole log equals analysing the part after a checkpoint taken in the middle of it.
        #[test]
        fn s4c_c5_property_a_checkpoint_in_the_middle_changes_nothing(events in proptest::collection::vec((0u8..4, 0u32..3, 0u32..3), 0..20), cut in 0usize..21) {
            let log: Vec<(u64, bustub::recovery::analysis::LogRec)> = events.iter().enumerate().map(|(i, &(k, t, p))| (i as u64 + 1, match k { 0 => Begin(t), 1 | 2 => Update(t, p), _ => Commit(t) })).collect();
            let cut = cut.min(log.len());
            let whole = analyze(&Analysis::default(), &log);
            let checkpoint = analyze(&Analysis::default(), &log[..cut]);
            prop_assert_eq!(analyze(&checkpoint, &log[cut..]), whole);
        }
    }
}
// @@ challenge 4c-c5 end
