from y17_parts.common import broken

TRAIT = r'''/// A key-value store under test, with one-byte keys and values.
pub trait Subject {
    fn put(&mut self, key: u8, value: u8);
    fn delete(&mut self, key: u8);
    fn get(&self, key: u8) -> Option<u8>;
    /// Moves what is only in the log into a table, and empties the log.
    fn flush(&mut self);
    /// Merges tables. It never changes what `get` returns.
    fn compact(&mut self);
    /// The process is killed and the store opened again.
    fn restart(&mut self);
    /// The same, but the newest write still in the log is cut short on disk first. With an empty log, nothing is cut.
    fn crash_tearing_last(&mut self);
}
'''

STARTER = TRAIT + r'''
/// Panics when the stores built by `make` break the rules; returns when they keep them.
pub fn check<S: Subject>(make: impl Fn() -> S) {
    let mut store = make();
    store.put(1, 10);
    assert_eq!(store.get(1), Some(10));
}
'''

SOLUTION = r'''use std::collections::BTreeMap;

''' + TRAIT + r'''
/// What the store must answer: what is in tables, and the writes that are only in the log.
#[derive(Default)]
struct Model {
    flushed: BTreeMap<u8, u8>,
    log: Vec<(u8, Option<u8>)>,
}

impl Model {
    fn contents(&self) -> BTreeMap<u8, u8> {
        let mut all = self.flushed.clone();
        for &(key, value) in &self.log {
            match value {
                Some(value) => all.insert(key, value),
                None => all.remove(&key),
            };
        }
        all
    }
}

fn compare<S: Subject>(store: &S, model: &Model, keys: u8) {
    let want = model.contents();
    for key in 0..keys {
        assert_eq!(store.get(key), want.get(&key).copied(), "get {key}");
    }
}

/// Panics when the stores built by `make` break the rules; returns when they keep them.
pub fn check<S: Subject>(make: impl Fn() -> S) {
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut below = move |n: u64| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state % n
    };
    for round in 0..300u64 {
        // few keys, so that they are overwritten and deleted across many tables
        let keys = [2u8, 3, 6, 12][(round % 4) as usize];
        // some histories flush often, others let the log grow long
        let flush_weight = [1, 4, 12][(round % 3) as usize];
        let mut store = make();
        let mut model = Model::default();
        for _ in 0..150 {
            let key = below(u64::from(keys)) as u8;
            let choice = below(40 + flush_weight);
            match choice {
                0..=13 => {
                    let value = below(256) as u8;
                    store.put(key, value);
                    model.log.push((key, Some(value)));
                }
                14..=21 => {
                    store.delete(key);
                    model.log.push((key, None));
                }
                22..=27 => store.compact(),
                28..=33 => store.restart(),
                34..=39 => {
                    store.crash_tearing_last();
                    model.log.pop();
                }
                _ => {
                    store.flush();
                    model.flushed = model.contents();
                    model.log.clear();
                }
            }
            compare(&store, &model, keys);
        }
    }
}
'''

HARNESS = r'''
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
'''

VISIBLE = HARNESS + r'''
#[test]
fn a_correct_store_passes() {
    check!("a correct store: check must return", accepts(Bug::None), true);
}

#[test]
fn a_delete_missing_from_the_log() {
    rejects(Bug::DeleteNotLogged, "delete changes memory but not the log");
}

#[test]
fn oldest_table_first() {
    rejects(Bug::ReadsOldestFirst, "get looks at the oldest table first");
}

#[test]
fn deletes_lost_by_flush() {
    rejects(Bug::FlushSkipsDeletes, "flush leaves delete markers out of the table");
}

#[test]
fn torn_bytes_left_in_the_log() {
    rejects(Bug::TornBytesStay, "the torn bytes stay in the log, so writes made after them are lost at a later restart");
}
'''

HIDDEN = HARNESS + r'''
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
'''

PROBLEM = dict(
    slug="test-your-store", title="Capstone 7: prove it survives", level="hard", stage="trust-it",
    tags=["model testing", "crash testing", "torn writes", "oracle", "testing"],
    statement=r'''
**Capstone, step 7 of 7.** The store you built passes its tests. Would the tests notice if it broke tomorrow? This time **you write the test**.

Write `check(make)`. It must **return normally for a correct store** and **panic for a broken one**. `make()` returns a new, empty store; call it as often as you like. The store never flushes by itself.

**A correct store** behaves like this:

- `get` reflects every `put` and `delete` so far, apart from the writes that a tear has removed.
- A write lives in the log until `flush` moves it, with the others, into a table. `flush` empties the log.
- `compact` never changes what `get` returns.
- `restart` changes nothing that `get` returns.
- `crash_tearing_last` loses **exactly** the newest write still in the log, if there is one, and restarts. Writes made afterwards are kept as usual. Tearing again removes the next newest write in the log.

The tests run your `check` on two correct stores that work differently inside, and on stores that each break one rule. Some bugs show only after a particular history: several tables, a restart at the wrong time, a long log, or a crash after a crash. Your check must be **deterministic** and fast enough for a debug build.
''',
    starter=STARTER, solution=SOLUTION, visible=[VISIBLE], hidden=[HIDDEN], use="use solution::*;",
    hints=[
        ("approach", "Build the smallest model that can say what `get` must return after any history, including a tear. It needs to know more than the current contents."),
        ("rust", "A few lines of xorshift give a seeded generator, so each run does the same thing. `assert_eq!` with a message naming the key is enough to panic."),
        ("edge", "Use very few keys, so they collide across tables. Vary how often you flush: some bugs need many tables, others a long log. Compare after every step."),
    ],
    notes=("The model keeps the flushed contents and the list of writes still in the log, so a tear is just removing the last of the list. Random histories over very few keys, with different flush rates, reach many tables, long logs and repeated crashes. Every key is compared after every step.", "O(rounds x steps x keys)", "O(keys + log)"),
    follow_up="1) Your check panics. What would you add so that the failure shows a short history that reproduces it? 2) These crashes happen between operations. How would you crash inside a flush or a compaction of the real store? 3) Which bugs can no test of this kind find, however long it runs?",
    teaches=["A model for crash testing must know what may be lost, not only what is stored.", "Random histories find bugs only if the generator reaches the states they need: tune it, and count what it reached.", "A test is only as good as the bugs it has been shown to catch."],
    wrong={
        "never_restarts": broken(SOLUTION, ("                28..=33 => store.restart(),", "                28..=33 => {}"), ("                34..=39 => {\n                    store.crash_tearing_last();\n                    model.log.pop();\n                }", "                34..=39 => {}")),
        "never_tears": broken(SOLUTION, ("                34..=39 => {\n                    store.crash_tearing_last();\n                    model.log.pop();\n                }", "                34..=39 => store.restart(),")),
        "never_compacts": broken(SOLUTION, ("                22..=27 => store.compact(),", "                22..=27 => {}")),
        "short_histories": broken(SOLUTION, ("        for _ in 0..150 {", "        for _ in 0..5 {")),
        "never_deletes": broken(SOLUTION, ("                14..=21 => {\n                    store.delete(key);\n                    model.log.push((key, None));\n                }", "                14..=21 => {}")),
        "forgets_what_a_tear_loses": broken(SOLUTION, ("                    model.log.pop();\n", "                    model.log.clear();\n")),
    },
)
