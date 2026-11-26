//! RECIPE 742 (database): Delta Lake style table format: a transaction log over an object store with optimistic concurrency.
//! LEVEL: advanced
//! Use in: lakehouse table formats, ACID tables on S3/GCS/Azure blobs, any log-structured metadata over put-if-absent storage.   Run: cargo run --release --example delta_lake_transaction_log
//! COMMON USES: Delta Lake (numbered JSON commits plus Parquet checkpoints), Apache Iceberg (snapshot/manifest lists with atomic
//! pointer swap), Apache Hudi timeline, Databricks Delta, Snowflake and BigQuery managed-table metadata, LSM manifests (RocksDB MANIFEST).
//! Course: CMU 15-721 Advanced Database Systems (Spring), lecture on lakehouse / table formats and cloud storage (Delta Lake paper).
#![allow(dead_code)]
//!
//! WHAT: a table is a DIRECTORY of immutable data files plus a LOG of numbered commit files `_delta_log/00000000000000000007.json`.
//! Each commit is a list of ACTIONS: `Protocol` (reader/writer versions), `Metadata` (schema), `Add` (file + stats),
//! `Remove` (file tombstone) and `CommitInfo`. The table at version v is the result of replaying commits 0..=v.
//!
//! WHY it works on a dumb object store: the store offers only PUT-IF-ABSENT for new keys and an atomic sorted LISTING. Writing
//! commit v+1 with put-if-absent is the single atomic step: exactly one writer wins each version number, so the log is a totally
//! ordered history, and a commit becomes visible all at once. Data files are written first under unique names; a crash before the
//! commit leaves harmless ORPHANS that no snapshot references.
//!
//! HOW: OPTIMISTIC CONCURRENCY. A transaction plans actions against snapshot S, then tries to commit S+1. If put-if-absent fails
//! another writer won; read the winning commits and test for CONFLICT: a winner removed a file I read, a winner added a file whose
//! stats overlap my predicate (a phantom), or both changed the schema. No conflict: REBASE (retry at the next version number with the
//! same actions, cheap). Conflict: re-plan against the fresh snapshot, or abort when the logic is now invalid (adding a column that
//! already exists). Blind appends never conflict. CHECKPOINTS every 10 commits store the whole state so a reader replays at most 10
//! commits. SNAPSHOT ISOLATION: a reader loads one version and only touches files it lists, so concurrent commits are invisible
//! to it; TIME TRAVEL is the same with an older version. DATA SKIPPING uses per-file min/max/null stats in the Add action; a
//! file lacking a column (written before ADD COLUMN) reads as all NULL and can be skipped for a range filter. OPTIMIZE rewrites small
//! files into one (a Remove set plus one Add, no data change). VACUUM deletes files that are unreferenced AND older than a
//! RETENTION period, measured from the file's removal time; the retention is what keeps long-running readers and in-flight writers safe.
//!
//! CHECKS: 8 writer threads (appends, range updates, optimize) with 2 concurrent reader threads; no lost commits (contiguous
//! versions, count of successes = commit files); the final table equals applying the committed operations in version order (serial
//! equivalence); every snapshot a reader saw matches the model at that version; checkpoint replay equals full replay at sampled
//! versions; crash injection leaves invisible orphans that VACUUM removes while young in-flight files survive; a reader inside
//! the retention window survives vacuum; schema evolution backfills NULL, rejects incompatible changes and resolves racing ADD COLUMNs.
//!
//! Read: Delta Lake: High-Performance ACID Table Storage over Cloud Object Stores https://www.vldb.org/pvldb/vol13/p3411-armbrust.pdf
//!       Delta Lake transaction log protocol https://github.com/delta-io/delta/blob/master/PROTOCOL.md
//!       CMU 15-721 course schedule https://15721.courses.cs.cmu.edu/spring2024/

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::SeqCst};

// ---------------------------------------------------------------- the object store

/// The only storage contract the format needs: put-if-absent, get, atomic sorted list, delete, plus modification times.
struct ObjectStore {
    objs: Mutex<BTreeMap<String, (String, u64)>>,
    clock: AtomicU64, // logical time, ticks on every write
    names: AtomicU64,
}

impl ObjectStore {
    fn new() -> Self {
        ObjectStore {
            objs: Mutex::new(BTreeMap::new()),
            clock: AtomicU64::new(1),
            names: AtomicU64::new(0),
        }
    }
    fn put_if_absent(&self, key: &str, val: String) -> bool {
        let mut m = self.objs.lock().unwrap();
        if m.contains_key(key) {
            return false;
        }
        let t = self.clock.fetch_add(1, SeqCst);
        m.insert(key.to_string(), (val, t));
        true
    }
    fn get(&self, key: &str) -> Option<String> {
        self.objs.lock().unwrap().get(key).map(|x| x.0.clone())
    }
    fn mtime(&self, key: &str) -> Option<u64> {
        self.objs.lock().unwrap().get(key).map(|x| x.1)
    }
    fn list(&self, prefix: &str) -> Vec<String> {
        let m = self.objs.lock().unwrap();
        m.range(prefix.to_string()..)
            .take_while(|(k, _)| k.starts_with(prefix))
            .map(|(k, _)| k.clone())
            .collect()
    }
    fn delete(&self, key: &str) -> bool {
        self.objs.lock().unwrap().remove(key).is_some()
    }
    fn now(&self) -> u64 {
        self.clock.load(SeqCst)
    }
    fn advance(&self, n: u64) {
        self.clock.fetch_add(n, SeqCst);
    }
    fn unique(&self) -> u64 {
        self.names.fetch_add(1, SeqCst)
    }
}

// ---------------------------------------------------------------- log actions and table state

type Row = Vec<Option<i64>>;
type Stats = BTreeMap<String, (Option<i64>, Option<i64>, u64)>; // column -> (min, max, null count)

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct AddFile {
    path: String,
    rows: u64,
    stats: Stats,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
enum Action {
    Protocol { min_reader: u32, min_writer: u32 },
    Metadata { columns: Vec<String> },
    Add(AddFile),
    Remove { path: String, ts: u64 },
    CommitInfo { op: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
struct State {
    version: i64,
    protocol: Option<(u32, u32)>,
    columns: Vec<String>,
    files: BTreeMap<String, AddFile>,
    tombstones: BTreeMap<String, u64>, // removed path -> removal time (vacuum uses it)
}

fn apply(st: &mut State, actions: &[Action]) {
    for a in actions {
        match a {
            Action::Protocol {
                min_reader,
                min_writer,
            } => st.protocol = Some((*min_reader, *min_writer)),
            Action::Metadata { columns } => st.columns = columns.clone(),
            Action::Add(f) => {
                st.files.insert(f.path.clone(), f.clone());
            }
            Action::Remove { path, ts } => {
                st.files.remove(path);
                st.tombstones.insert(path.clone(), *ts);
            }
            Action::CommitInfo { .. } => {}
        }
    }
}

#[derive(Serialize, Deserialize)]
struct DataFile {
    cols: Vec<String>,
    rows: Vec<Row>,
}

fn commit_key(v: u64) -> String {
    format!("_delta_log/{v:020}.json")
}
fn checkpoint_key(v: u64) -> String {
    format!("_delta_log/{v:020}.checkpoint.json")
}

fn parse_log(store: &ObjectStore) -> (BTreeSet<u64>, BTreeSet<u64>) {
    let (mut commits, mut cps) = (BTreeSet::new(), BTreeSet::new());
    for k in store.list("_delta_log/") {
        let name = &k["_delta_log/".len()..];
        if let Some(n) = name.strip_suffix(".checkpoint.json") {
            cps.insert(n.parse().unwrap());
        } else if let Some(n) = name.strip_suffix(".json") {
            commits.insert(n.parse().unwrap());
        }
    }
    (commits, cps)
}

fn read_commit(store: &ObjectStore, v: u64) -> Result<Vec<Action>, String> {
    let text = store
        .get(&commit_key(v))
        .ok_or(format!("missing commit {v}"))?;
    Ok(text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect())
}

fn latest_version(store: &ObjectStore) -> Option<u64> {
    parse_log(store).0.last().copied()
}

/// Reconstruct the table at `at` (default latest): newest checkpoint <= at, then replay the later commits.
fn load(store: &ObjectStore, at: Option<u64>, use_checkpoints: bool) -> Result<State, String> {
    let (commits, cps) = parse_log(store);
    let latest = *commits.last().ok_or("empty log")?;
    let at = at.unwrap_or(latest);
    if at > latest {
        return Err(format!("version {at} does not exist (latest {latest})"));
    }
    let cp = if use_checkpoints {
        cps.range(..=at).next_back().copied()
    } else {
        None
    };
    let (mut st, start) = match cp {
        Some(c) => (
            serde_json::from_str(&store.get(&checkpoint_key(c)).ok_or("missing checkpoint")?)
                .unwrap(),
            c + 1,
        ),
        None => (State::default(), 0),
    };
    for v in start..=at {
        apply(&mut st, &read_commit(store, v)?);
    }
    st.version = at as i64;
    Ok(st)
}

// ---------------------------------------------------------------- data files, stats, reads

fn stats_of(cols: &[String], rows: &[Row]) -> Stats {
    cols.iter()
        .enumerate()
        .map(|(i, c)| {
            let vals = rows.iter().filter_map(|r| r[i]);
            (
                c.clone(),
                (
                    vals.clone().min(),
                    vals.max(),
                    rows.iter().filter(|r| r[i].is_none()).count() as u64,
                ),
            )
        })
        .collect()
}

/// Write an immutable data file under a unique name. This is step 1 of every commit and is invisible until the log mentions it.
fn write_file(store: &ObjectStore, cols: &[String], rows: Vec<Row>) -> AddFile {
    let path = format!("data/part-{:08}.json", store.unique());
    let add = AddFile {
        path: path.clone(),
        rows: rows.len() as u64,
        stats: stats_of(cols, &rows),
    };
    let body = serde_json::to_string(&DataFile {
        cols: cols.to_vec(),
        rows,
    })
    .unwrap();
    assert!(store.put_if_absent(&path, body));
    add
}

/// Read a file in the TABLE's current column order; columns the file predates read as NULL (schema evolution without rewrites).
fn read_file(store: &ObjectStore, path: &str, table_cols: &[String]) -> Result<Vec<Row>, String> {
    let df: DataFile =
        serde_json::from_str(&store.get(path).ok_or(format!("file {path} is gone"))?).unwrap();
    let map: Vec<Option<usize>> = table_cols
        .iter()
        .map(|c| df.cols.iter().position(|x| x == c))
        .collect();
    Ok(df
        .rows
        .iter()
        .map(|r| map.iter().map(|m| m.and_then(|i| r[i])).collect())
        .collect())
}

fn overlaps(f: &AddFile, col: &str, lo: i64, hi: i64) -> bool {
    match f.stats.get(col) {
        Some((Some(mn), Some(mx), _)) => *mn <= hi && *mx >= lo,
        _ => false, // missing column or all NULL: a range predicate cannot match
    }
}

/// Scan with optional `col BETWEEN lo AND hi`; returns rows and the number of files skipped via stats.
fn scan(
    store: &ObjectStore,
    st: &State,
    filter: Option<(&str, i64, i64)>,
) -> Result<(Vec<Row>, usize), String> {
    let (mut out, mut skipped) = (vec![], 0);
    for f in st.files.values() {
        if filter.is_some_and(|(c, lo, hi)| !overlaps(f, c, lo, hi)) {
            skipped += 1;
            continue;
        }
        out.extend(read_file(store, &f.path, &st.columns)?);
    }
    if let Some((c, lo, hi)) = filter {
        let ci = st
            .columns
            .iter()
            .position(|x| x == c)
            .ok_or("no such column")?;
        out.retain(|r| r[ci].is_some_and(|v| v >= lo && v <= hi));
    }
    Ok((out, skipped))
}

// ---------------------------------------------------------------- transactions

#[derive(Clone, Debug)]
enum Op {
    Append(Vec<Row>),
    Update { lo: i64, hi: i64, val: i64 }, // UPDATE t SET v = val WHERE id BETWEEN lo AND hi
    Optimize,
    AddColumn(String),
}

struct Plan {
    actions: Vec<Action>,
    read_files: BTreeSet<String>,
    range: Option<(i64, i64)>, // id predicate, for phantom detection
    changes_metadata: bool,
}

/// Schema evolution rule: only appending new, uniquely named columns is compatible (old files stay valid and backfill NULL).
fn check_evolution(old: &[String], new: &[String]) -> Result<(), String> {
    if new.len() < old.len() || new[..old.len()] != *old {
        return Err("incompatible: columns may not be dropped, renamed or reordered".into());
    }
    let uniq: HashSet<&String> = new.iter().collect();
    if uniq.len() != new.len() {
        return Err("incompatible: duplicate column".into());
    }
    Ok(())
}

fn plan(store: &ObjectStore, st: &State, op: &Op) -> Result<Option<Plan>, String> {
    let mk = |actions: Vec<Action>, read: BTreeSet<String>, range, meta| Plan {
        actions,
        read_files: read,
        range,
        changes_metadata: meta,
    };
    match op {
        Op::Append(rows) => {
            let f = write_file(store, &st.columns, rows.clone());
            Ok(Some(mk(
                vec![
                    Action::Add(f),
                    Action::CommitInfo {
                        op: "APPEND".into(),
                    },
                ],
                BTreeSet::new(),
                None,
                false,
            )))
        }
        Op::Update { lo, hi, val } => {
            let vi = st.columns.iter().position(|c| c == "v").unwrap();
            let (mut actions, mut read, mut changed) = (vec![], BTreeSet::new(), false);
            for f in st.files.values().filter(|f| overlaps(f, "id", *lo, *hi)) {
                read.insert(f.path.clone());
                let mut rows = read_file(store, &f.path, &st.columns)?;
                let mut hit = false;
                for r in rows
                    .iter_mut()
                    .filter(|r| r[0].is_some_and(|id| id >= *lo && id <= *hi))
                {
                    r[vi] = Some(*val);
                    hit = true;
                }
                if hit {
                    changed = true;
                    actions.push(Action::Remove {
                        path: f.path.clone(),
                        ts: store.now(),
                    });
                    actions.push(Action::Add(write_file(store, &st.columns, rows)));
                }
            }
            if !changed {
                return Ok(None);
            }
            actions.push(Action::CommitInfo {
                op: "UPDATE".into(),
            });
            Ok(Some(mk(actions, read, Some((*lo, *hi)), false)))
        }
        Op::Optimize => {
            let small: Vec<&AddFile> = st.files.values().filter(|f| f.rows < 25).take(10).collect();
            if small.len() < 3 {
                return Ok(None);
            }
            let (mut actions, mut read, mut rows) = (vec![], BTreeSet::new(), vec![]);
            for f in &small {
                read.insert(f.path.clone());
                rows.extend(read_file(store, &f.path, &st.columns)?);
                actions.push(Action::Remove {
                    path: f.path.clone(),
                    ts: store.now(),
                });
            }
            actions.push(Action::Add(write_file(store, &st.columns, rows)));
            actions.push(Action::CommitInfo {
                op: "OPTIMIZE".into(),
            });
            Ok(Some(mk(actions, read, None, false)))
        }
        Op::AddColumn(name) => {
            let mut cols = st.columns.clone();
            cols.push(name.clone());
            check_evolution(&st.columns, &cols)?;
            Ok(Some(mk(
                vec![
                    Action::Metadata { columns: cols },
                    Action::CommitInfo {
                        op: "ADD COLUMN".into(),
                    },
                ],
                BTreeSet::new(),
                None,
                true,
            )))
        }
    }
}

/// Does any winning commit invalidate what this transaction read or assumed?
fn conflicts(p: &Plan, winners: &[Action]) -> bool {
    winners.iter().any(|a| match a {
        Action::Remove { path, .. } => p.read_files.contains(path),
        Action::Add(f) => p.range.is_some_and(|(lo, hi)| overlaps(f, "id", lo, hi)),
        Action::Metadata { .. } => p.changes_metadata,
        _ => false,
    })
}

#[derive(Default)]
struct Counters {
    commits: AtomicUsize,
    rebases: AtomicUsize,
    conflicts: AtomicUsize,
    aborts: AtomicUsize,
}

const CHECKPOINT_EVERY: u64 = 10;

/// Try to publish `p` (planned against `base`). Ok(Some(v)) = committed as v, Ok(None) = conflict, caller must re-plan.
fn commit_plan(store: &ObjectStore, c: &Counters, p: &Plan, mut base: u64) -> Option<u64> {
    let body = p
        .actions
        .iter()
        .map(|a| serde_json::to_string(a).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    loop {
        let v = base + 1;
        if store.put_if_absent(&commit_key(v), body.clone()) {
            c.commits.fetch_add(1, SeqCst);
            if v % CHECKPOINT_EVERY == 0 {
                // Any writer may write the checkpoint; put-if-absent makes duplicates harmless. Versions <= v are immutable.
                let st = load(store, Some(v), true).unwrap();
                store.put_if_absent(&checkpoint_key(v), serde_json::to_string(&st).unwrap());
            }
            return Some(v);
        }
        // Lost the race for version v: examine every commit that won since our snapshot.
        let latest = latest_version(store).unwrap();
        let winners: Vec<Action> = (base + 1..=latest)
            .flat_map(|w| read_commit(store, w).unwrap())
            .collect();
        if conflicts(p, &winners) {
            c.conflicts.fetch_add(1, SeqCst);
            return None;
        }
        c.rebases.fetch_add(1, SeqCst);
        base = latest; // logically commutes with the winners: retry on top of them with the same actions
    }
}

/// Run one transaction to completion: plan, commit, re-plan on conflict. Ok(None) means the op was a no-op.
fn run_txn(store: &ObjectStore, c: &Counters, op: &Op) -> Result<Option<u64>, String> {
    for _ in 0..500 {
        let st = load(store, None, true)?;
        let p = match plan(store, &st, op) {
            Ok(Some(p)) => p,
            Ok(None) => return Ok(None),
            Err(e) => {
                c.aborts.fetch_add(1, SeqCst);
                return Err(e);
            }
        };
        if let Some(v) = commit_plan(store, c, &p, st.version as u64) {
            return Ok(Some(v));
        }
    }
    Err("too many conflicts".into())
}

// ---------------------------------------------------------------- vacuum

/// Delete data files not referenced by the latest version whose removal (or, for orphans, creation) is older than `retention`.
fn vacuum(store: &ObjectStore, retention: u64) -> Vec<String> {
    let st = load(store, None, true).unwrap();
    let cutoff = store.now().saturating_sub(retention);
    let mut deleted = vec![];
    for key in store.list("data/") {
        if st.files.contains_key(&key) {
            continue;
        }
        // A removed file is protected from its REMOVAL time (readers of older snapshots may still use it);
        // an orphan from its write time (its writer may still be about to commit).
        let t = st
            .tombstones
            .get(&key)
            .copied()
            .or(store.mtime(&key))
            .unwrap();
        if t < cutoff && store.delete(&key) {
            deleted.push(key);
        }
    }
    deleted
}

// ---------------------------------------------------------------- model and helpers

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn model_apply(rows: &mut Vec<Row>, op: &Op) {
    match op {
        Op::Append(r) => rows.extend(r.iter().cloned()),
        Op::Update { lo, hi, val } => {
            for r in rows
                .iter_mut()
                .filter(|r| r[0].is_some_and(|id| id >= *lo && id <= *hi))
            {
                r[1] = Some(*val);
            }
        }
        Op::Optimize => {}
        Op::AddColumn(_) => rows.iter_mut().for_each(|r| r.push(None)),
    }
}

fn fingerprint(rows: &[Row]) -> (usize, i64, i64) {
    let sum = |i: usize| rows.iter().map(|r| r[i].unwrap_or(0)).sum::<i64>();
    (rows.len(), sum(0), sum(1))
}

fn sorted(mut rows: Vec<Row>) -> Vec<Row> {
    rows.sort();
    rows
}

fn random_op(rng: &mut Rng) -> Op {
    match rng.below(100) {
        0..=59 => {
            let base = rng.below(4900) as i64;
            let n = 5 + rng.below(16);
            Op::Append(
                (0..n)
                    .map(|_| {
                        vec![
                            Some(base + rng.below(60) as i64),
                            Some(rng.below(1000) as i64),
                        ]
                    })
                    .collect(),
            )
        }
        60..=94 => {
            let lo = rng.below(4900) as i64;
            Op::Update {
                lo,
                hi: lo + rng.below(40) as i64,
                val: rng.below(1000) as i64,
            }
        }
        _ => Op::Optimize,
    }
}

fn main() {
    let store = ObjectStore::new();
    let counters = Counters::default();

    // Version 0: protocol + schema. The commit is itself a put-if-absent.
    let v0 = vec![
        Action::Protocol {
            min_reader: 1,
            min_writer: 2,
        },
        Action::Metadata {
            columns: vec!["id".into(), "v".into()],
        },
    ];
    assert!(
        store.put_if_absent(
            &commit_key(0),
            v0.iter()
                .map(|a| serde_json::to_string(a).unwrap())
                .collect::<Vec<_>>()
                .join("\n")
        )
    );
    assert!(
        !store.put_if_absent(&commit_key(0), "x".into()),
        "second writer of version 0 must lose"
    );
    println!(
        "== 8 concurrent writers (appends, range updates, optimize) + 2 readers over put-if-absent"
    );

    let log: Mutex<Vec<(u64, Op)>> = Mutex::new(vec![]);
    let observations: Mutex<Vec<(u64, (usize, i64, i64))>> = Mutex::new(vec![]);
    let done = AtomicBool::new(false);
    let t0 = std::time::Instant::now();
    std::thread::scope(|s| {
        let mut writers = vec![];
        for w in 0..8u64 {
            let (store, counters, log) = (&store, &counters, &log);
            writers.push(s.spawn(move || {
                let mut rng =
                    Rng(0x1234_5678_9ABC_DEF1 ^ (w + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
                for _ in 0..40 {
                    let op = random_op(&mut rng);
                    if let Some(v) = run_txn(store, counters, &op).expect("txn") {
                        log.lock().unwrap().push((v, op));
                    }
                }
            }));
        }
        for _ in 0..2 {
            let (store, observations, done) = (&store, &observations, &done);
            s.spawn(move || {
                while !done.load(SeqCst) {
                    let st = load(store, None, true).expect("reader snapshot");
                    let (rows, _) =
                        scan(store, &st, None).expect("every file of a snapshot must exist");
                    observations
                        .lock()
                        .unwrap()
                        .push((st.version as u64, fingerprint(&rows)));
                    std::thread::yield_now();
                }
            });
        }
        for h in writers {
            h.join().unwrap();
        }
        done.store(true, SeqCst);
    });
    let latest = latest_version(&store).unwrap();
    let mut ops = log.into_inner().unwrap();
    ops.sort_by_key(|(v, _)| *v);
    println!(
        "  {:?}: {} committed txns, latest version {latest}, rebases {}, conflicts (re-planned) {}",
        t0.elapsed(),
        ops.len(),
        counters.rebases.load(SeqCst),
        counters.conflicts.load(SeqCst)
    );

    // No lost commits: every version 1..=latest was won by exactly one recorded transaction.
    let versions: Vec<u64> = ops.iter().map(|(v, _)| *v).collect();
    assert_eq!(
        versions,
        (1..=latest).collect::<Vec<_>>(),
        "versions are contiguous and each belongs to one txn"
    );
    assert_eq!(counters.commits.load(SeqCst) as u64, latest);

    // Serial equivalence + per-version snapshot consistency of readers.
    let mut model: Vec<Row> = vec![];
    let mut at_version: BTreeMap<u64, (usize, i64, i64)> = BTreeMap::new();
    at_version.insert(0, fingerprint(&model));
    for (v, op) in &ops {
        model_apply(&mut model, op);
        at_version.insert(*v, fingerprint(&model));
    }
    let head = load(&store, None, true).unwrap();
    let (final_rows, _) = scan(&store, &head, None).unwrap();
    assert_eq!(
        sorted(final_rows.clone()),
        sorted(model.clone()),
        "table == committed ops applied in version order"
    );
    let obs = observations.into_inner().unwrap();
    for (v, fp) in &obs {
        assert_eq!(
            *fp, at_version[v],
            "reader saw a torn or non-prefix snapshot at version {v}"
        );
    }
    let seen: BTreeSet<u64> = obs.iter().map(|(v, _)| *v).collect();
    println!(
        "  serial equivalence holds ({} rows, {} files); {} reader snapshots over {} distinct versions all consistent",
        final_rows.len(),
        head.files.len(),
        obs.len(),
        seen.len()
    );
    let kinds = |f: fn(&Op) -> bool| ops.iter().filter(|(_, o)| f(o)).count();
    println!(
        "  committed: {} appends, {} updates, {} optimizes",
        kinds(|o| matches!(o, Op::Append(_))),
        kinds(|o| matches!(o, Op::Update { .. })),
        kinds(|o| matches!(o, Op::Optimize))
    );

    println!("== checkpoints and time travel");
    let (_, cps) = parse_log(&store);
    assert_eq!(
        cps.len() as u64,
        latest / CHECKPOINT_EVERY,
        "one checkpoint per {CHECKPOINT_EVERY} commits"
    );
    let mut checked = 0;
    for v in (0..=latest).step_by(3).chain(std::iter::once(latest)) {
        let fast = load(&store, Some(v), true).unwrap();
        let slow = load(&store, Some(v), false).unwrap();
        assert_eq!(
            fast, slow,
            "checkpoint replay != full replay at version {v}"
        );
        let (rows, _) = scan(&store, &fast, None).unwrap();
        assert_eq!(
            fingerprint(&rows),
            at_version[&v],
            "time travel to version {v}"
        );
        checked += 1;
    }
    println!(
        "  {} checkpoints; {checked} versions loaded via checkpoint == full replay == model; replay length <= {CHECKPOINT_EVERY} commits",
        cps.len()
    );
    assert!(load(&store, Some(latest + 5), true).is_err());

    println!("== data skipping with per-file stats");
    let mut rng = Rng(5);
    let (mut skipped, mut total) = (0, 0);
    for _ in 0..200 {
        let lo = rng.below(4900) as i64;
        let hi = lo + rng.below(30) as i64;
        let (got, sk) = scan(&store, &head, Some(("id", lo, hi))).unwrap();
        let want: Vec<Row> = final_rows
            .iter()
            .filter(|r| r[0].is_some_and(|id| id >= lo && id <= hi))
            .cloned()
            .collect();
        assert_eq!(sorted(got), sorted(want));
        skipped += sk;
        total += head.files.len();
    }
    println!(
        "  200 range queries equal a full scan; {:.1}% of file reads skipped",
        100.0 * skipped as f64 / total as f64
    );

    println!("== crash injection between data-file write and log commit");
    let before = sorted(scan(&store, &head, None).unwrap().0);
    let data_before = store.list("data/").len();
    let mut orphans = vec![];
    for i in 0..5 {
        let st = load(&store, None, true).unwrap();
        let p = plan(
            &store,
            &st,
            &Op::Append(vec![vec![Some(9000 + i), Some(1)]; 3]),
        )
        .unwrap()
        .unwrap();
        if let Action::Add(f) = &p.actions[0] {
            orphans.push(f.path.clone()); // the process "dies" here: no commit
        }
    }
    let after_head = load(&store, None, true).unwrap();
    assert_eq!(
        after_head.version, head.version,
        "crashed writers committed nothing"
    );
    assert_eq!(
        sorted(scan(&store, &after_head, None).unwrap().0),
        before,
        "orphans are invisible to readers"
    );
    assert!(
        orphans
            .iter()
            .all(|o| store.get(o).is_some() && !after_head.files.contains_key(o))
    );
    println!(
        "  {} orphan files written ({} -> {} data files), table content unchanged",
        orphans.len(),
        data_before,
        store.list("data/").len()
    );

    // An in-flight writer: file written, commit not yet attempted. Vacuum with a sane retention must leave it alone.
    let st = load(&store, None, true).unwrap();
    let inflight = plan(&store, &st, &Op::Append(vec![vec![Some(9100), Some(2)]]))
        .unwrap()
        .unwrap();
    store.advance(50);
    vacuum(&store, 1000);
    let inflight_path = match &inflight.actions[0] {
        Action::Add(f) => f.path.clone(),
        _ => unreachable!(),
    };
    assert!(
        store.get(&inflight_path).is_some(),
        "young in-flight file survives vacuum"
    );
    let v = commit_plan(&store, &counters, &inflight, st.version as u64)
        .expect("no conflict for a blind append");
    assert!(
        load(&store, None, true)
            .unwrap()
            .files
            .contains_key(&inflight_path)
    );
    println!("  in-flight append survived vacuum(retention=1000) and committed as version {v}");

    println!("== OPTIMIZE and VACUUM with a reader inside / outside the retention window");
    let reader = load(&store, None, true).unwrap();
    let reader_rows = sorted(scan(&store, &reader, None).unwrap().0);
    let files_before = reader.files.len();
    while run_txn(&store, &counters, &Op::Optimize).unwrap().is_some() {}
    let compacted = load(&store, None, true).unwrap();
    assert_eq!(
        sorted(scan(&store, &compacted, None).unwrap().0),
        reader_rows,
        "optimize does not change contents"
    );
    println!(
        "  OPTIMIZE: {files_before} -> {} live files",
        compacted.files.len()
    );
    assert!(compacted.files.len() < files_before);
    assert!(
        vacuum(&store, 1_000_000).is_empty(),
        "nothing is old enough yet"
    );
    assert_eq!(
        sorted(scan(&store, &reader, None).unwrap().0),
        reader_rows,
        "old snapshot still readable inside retention"
    );
    store.advance(10_000);
    let deleted = vacuum(&store, 5_000);
    let remaining: BTreeSet<String> = store.list("data/").into_iter().collect();
    let live: BTreeSet<String> = compacted.files.keys().cloned().collect();
    assert_eq!(remaining, live, "after vacuum only referenced files remain");
    assert!(
        orphans.iter().all(|o| store.get(o).is_none()),
        "crash orphans are collected"
    );
    assert_eq!(
        sorted(scan(&store, &compacted, None).unwrap().0),
        reader_rows
    );
    assert!(
        scan(&store, &reader, None).is_err(),
        "a snapshot older than the retention window may break"
    );
    println!(
        "  vacuum removed {} files; head intact; stale snapshot beyond retention now fails (as documented)",
        deleted.len()
    );

    println!("== schema evolution");
    let c = Counters::default();
    assert!(check_evolution(&["id".into(), "v".into()], &["v".into(), "id".into()]).is_err());
    assert!(check_evolution(&["id".into(), "v".into()], &["id".into()]).is_err());
    assert!(
        run_txn(&store, &c, &Op::AddColumn("v".into())).is_err(),
        "duplicate column rejected"
    );
    let v = run_txn(&store, &c, &Op::AddColumn("score".into()))
        .unwrap()
        .unwrap();
    let st = load(&store, None, true).unwrap();
    assert_eq!(st.columns, ["id", "v", "score"]);
    let (rows, _) = scan(&store, &st, None).unwrap();
    assert!(
        rows.iter().all(|r| r.len() == 3 && r[2].is_none()),
        "old files backfill NULL"
    );
    assert_eq!(rows.len(), reader_rows.len());
    let new_rows: Vec<Row> = (0..30)
        .map(|i| vec![Some(100 + i), Some(7), Some(i * 10)])
        .collect();
    run_txn(&store, &c, &Op::Append(new_rows)).unwrap();
    let st = load(&store, None, true).unwrap();
    let (got, skipped) = scan(&store, &st, Some(("score", 50, 90))).unwrap();
    assert_eq!(got.len(), 5);
    assert_eq!(
        skipped,
        st.files.len() - 1,
        "every pre-evolution file is skipped: its column is all NULL"
    );
    // Old snapshots keep the old schema (time travel across the change).
    assert_eq!(load(&store, Some(v - 1), true).unwrap().columns.len(), 2);
    println!(
        "  ADD COLUMN at version {v}: old files read NULL, filter on the new column skipped {skipped} of {} files",
        st.files.len()
    );

    // Race: two different new columns concurrently -> both succeed (the loser re-plans); the same column -> exactly one wins.
    let c2 = Counters::default();
    let results: Vec<Result<Option<u64>, String>> = std::thread::scope(|s| {
        let hs: Vec<_> = ["a", "b", "c"]
            .iter()
            .map(|n| s.spawn(|| run_txn(&store, &c2, &Op::AddColumn(n.to_string()))))
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert!(results.iter().all(|r| r.is_ok()));
    let st = load(&store, None, true).unwrap();
    let extra: BTreeSet<&str> = st.columns[3..].iter().map(|s| s.as_str()).collect();
    assert_eq!(extra, BTreeSet::from(["a", "b", "c"]));
    let results: Vec<Result<Option<u64>, String>> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..2)
            .map(|_| s.spawn(|| run_txn(&store, &c2, &Op::AddColumn("dup".into()))))
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(
        results.iter().filter(|r| r.is_ok()).count(),
        1,
        "exactly one ADD COLUMN dup may win"
    );
    let st = load(&store, None, true).unwrap();
    assert_eq!(st.columns.iter().filter(|c| *c == "dup").count(), 1);
    println!(
        "  racing ADD COLUMNs: 3 distinct all succeed (conflicts re-planned: {}), 2 identical -> 1 winner + 1 rejected",
        c2.conflicts.load(SeqCst)
    );

    println!(
        "delta_lake_transaction_log: all checks passed (log has {} objects)",
        store.list("_delta_log/").len()
    );
}
