use solution::*;

/// Which one thing is wrong, if any.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq)]
enum Bug {
    None,
    /// A correct store that never merges anything.
    LazyCompaction,
    DeleteNotLogged,
    TornBytesStay,
    MergeKeepsOlder,
    MergeNewestTwoDropsDeletes,
    ReadsOldestFirst,
    RestartReusesTableId,
    FlushSkipsDeletes,
    CompactEmptiesLog,
    FlushKeepsLog,
    ReplayReversed,
    TearTakesTwo,
    ReplayStopsAtEight,
    TornDeleteStillApplies,
    TornLogLosesAll,
}

type Table = std::collections::BTreeMap<u8, Option<u8>>;

/// A log, a memtable and numbered tables, all in memory.
#[allow(dead_code)]
struct Store {
    bug: Bug,
    tables: std::collections::BTreeMap<u32, Table>,
    memtable: Table,
    log: Vec<(u8, Option<u8>)>,
    /// How many whole records come before bytes that cannot be read.
    junk_after: Option<usize>,
    next_id: u32,
}

#[allow(dead_code)]
impl Store {
    fn new(bug: Bug) -> Store {
        Store { bug, tables: Default::default(), memtable: Table::new(), log: Vec::new(), junk_after: None, next_id: 1 }
    }
    fn write(&mut self, key: u8, value: Option<u8>) {
        if !(self.bug == Bug::DeleteNotLogged && value.is_none()) {
            self.log.push((key, value));
        }
        self.memtable.insert(key, value);
    }
    fn replay(&mut self) {
        let mut records = self.log.clone();
        if let Some(whole) = self.junk_after {
            records.truncate(whole);
        }
        if self.bug == Bug::ReplayStopsAtEight {
            records.truncate(8);
        }
        if self.bug == Bug::ReplayReversed {
            records.reverse();
        }
        self.memtable = records.into_iter().collect();
    }
}

impl Subject for Store {
    fn put(&mut self, key: u8, value: u8) {
        self.write(key, Some(value));
    }
    fn delete(&mut self, key: u8) {
        self.write(key, None);
    }
    fn get(&self, key: u8) -> Option<u8> {
        if let Some(v) = self.memtable.get(&key) {
            return *v;
        }
        let mut tables: Vec<&Table> = self.tables.values().rev().collect();
        if self.bug == Bug::ReadsOldestFirst {
            tables.reverse();
        }
        tables.iter().find_map(|t| t.get(&key).copied()).flatten()
    }
    fn flush(&mut self) {
        if !self.memtable.is_empty() {
            let mut table = std::mem::take(&mut self.memtable);
            if self.bug == Bug::FlushSkipsDeletes {
                table.retain(|_, v| v.is_some());
            }
            self.tables.insert(self.next_id, table);
            self.next_id += 1;
        }
        if self.bug != Bug::FlushKeepsLog {
            self.log.clear();
            self.junk_after = None;
        }
    }
    fn compact(&mut self) {
        if self.bug == Bug::CompactEmptiesLog {
            self.log.clear();
            self.junk_after = None;
        }
        if self.tables.len() < 2 || self.bug == Bug::LazyCompaction {
            return;
        }
        let ids: Vec<u32> = self.tables.keys().copied().collect();
        let first = if self.bug == Bug::MergeNewestTwoDropsDeletes { ids.len() - 2 } else { 0 };
        let mut merged = Table::new();
        let mut chosen: Vec<u32> = ids[first..].to_vec();
        if self.bug == Bug::MergeKeepsOlder {
            chosen.reverse();
        }
        for id in &chosen {
            merged.extend(self.tables.remove(id).unwrap());
        }
        merged.retain(|_, v| v.is_some());
        if !merged.is_empty() {
            self.tables.insert(self.next_id, merged);
        }
        self.next_id += 1;
    }
    fn restart(&mut self) {
        self.memtable.clear();
        self.replay();
        if self.bug == Bug::RestartReusesTableId {
            // counts the files instead of looking at their numbers
            self.next_id = self.tables.len() as u32 + 1;
        }
    }
    fn crash_tearing_last(&mut self) {
        let torn = self.log.last().copied();
        if torn.is_some() {
            match self.bug {
                Bug::TornBytesStay => {
                    // the unreadable bytes stay where they are, and later frames land behind them
                    if self.junk_after.is_none() {
                        self.log.pop();
                        self.junk_after = Some(self.log.len());
                    }
                }
                Bug::TearTakesTwo => {
                    self.log.pop();
                    self.log.pop();
                }
                Bug::TornLogLosesAll => self.log.clear(),
                _ => {
                    self.log.pop();
                }
            }
        }
        self.restart();
        if self.bug == Bug::TornDeleteStillApplies {
            if let Some((key, None)) = torn {
                self.memtable.insert(key, None);
            }
        }
    }
}

#[allow(dead_code)]
fn accepts(bug: Bug) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(|| Store::new(bug)))).is_ok()
}

#[allow(dead_code)]
fn rejects(bug: Bug, what: &str) {
    check!(format!("a store where {what}: check must panic"), accepts(bug), false);
}

#[test]
fn a_correct_store_that_never_merges_also_passes() {
    check!("compact does nothing: that is allowed", accepts(Bug::LazyCompaction), true);
}

#[test]
fn merge_keeps_the_older_value() {
    rejects(Bug::MergeKeepsOlder, "compaction keeps the older version of a key");
}

#[test]
fn partial_merge_drops_deletes() {
    rejects(Bug::MergeNewestTwoDropsDeletes, "compaction merges only the two newest tables and still drops delete markers");
}

#[test]
fn table_ids_reused_after_restart() {
    rejects(Bug::RestartReusesTableId, "after a restart a new table can overwrite an old one");
}

#[test]
fn compaction_throws_the_log_away() {
    rejects(Bug::CompactEmptiesLog, "compaction empties the log");
}

#[test]
fn flush_keeps_the_log() {
    rejects(Bug::FlushKeepsLog, "flush does not empty the log, which shows when its end is torn");
}

#[test]
fn replay_in_the_wrong_order() {
    rejects(Bug::ReplayReversed, "the log is replayed backwards");
}

#[test]
fn a_tear_loses_two_writes() {
    rejects(Bug::TearTakesTwo, "a torn last record also loses the one before it");
}

#[test]
fn only_the_start_of_the_log_is_replayed() {
    rejects(Bug::ReplayStopsAtEight, "only the first eight log records are replayed");
}

#[test]
fn a_torn_delete_is_applied() {
    rejects(Bug::TornDeleteStillApplies, "a torn delete is applied anyway, until the next restart");
}

#[test]
fn a_torn_end_empties_the_log() {
    rejects(Bug::TornLogLosesAll, "a torn end throws away the whole log");
}

#[test]
fn same_answer_every_time() {
    let first: Vec<bool> = [Bug::None, Bug::MergeNewestTwoDropsDeletes, Bug::RestartReusesTableId].map(accepts).to_vec();
    let again: Vec<bool> = [Bug::None, Bug::MergeNewestTwoDropsDeletes, Bug::RestartReusesTableId].map(accepts).to_vec();
    check!("check is deterministic", first, again);
}
