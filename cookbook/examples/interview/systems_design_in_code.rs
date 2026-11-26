//! RECIPE 651 (interview): "design it, then code the core": 16 systems-design questions, each with requirements, numbers, a tested core and failure modes.
//! LEVEL: advanced
//! Use in: system-design and "design + code" interview rounds; sketching the core data structure before writing a design doc.   Run: cargo run --release --example systems_design_in_code
//! COMMON USES: URL shorteners, API gateways (rate limits), Memcached/Redis clusters, WAL-backed stores, SQS-style queues, notification pipelines, autocomplete, crawlers, Kafka-style logs, job queues, leases, Snowflake IDs, trending lists, log shippers, feature flags, search.
#![allow(dead_code)]
//!
//! HOW TO USE A QUESTION: read only the question and REQUIREMENTS, say your NUMBERS and API aloud, design the core, then read the code.
//! Every core runs on a VIRTUAL CLOCK (`now: u64`) and a seeded RNG, so each run is identical. The capacity numbers are assumed
//! workloads chosen for practice, not measurements of any real service. "Disk", "network" and "node" are in-memory stand-ins.
//!
//! Read: https://sre.google/sre-book/table-of-contents/ (overload, leases, queues) and https://aws.amazon.com/builders-library/
//! Read: Kleppmann, "Designing Data-Intensive Applications" (title only, not re-read here).
//!
//! TRY NEXT:
//!   - [S] In Q2 change the sliding-window COUNTER's weighting to ignore the previous window. Predict the worst admitted count inside one window, then print it.
//!   - [S] In Q4 remove the checksum comparison from `replay`. Predict which crash test fails first: truncation or bit flip?
//!   - [M] In Q7 support DECREASING a count (a deleted query). Why does the cached top-k stop being enough, and what must a node remember or recompute?
//!   - [M] In Q9 add a "block the publisher" retention policy next to "trim and report a gap"; show with a stalled subscriber what each one costs.
//!   - [A] PROJECT: join Q5 (queue), Q6 (dedupe) and Q10 (delayed jobs) into one notification service, crash it between any two steps and keep the effect exactly once.
//!
//! QUESTION BANK
//! Q1. Design a URL shortener; code id generation, base62 and collision handling. [Medium] [asked: very common]
//! Q2. Design a rate limiter; compare fixed window, sliding log, sliding counter, token bucket and GCRA. [Medium] [asked: very common]
//! Q3. Design a distributed cache with consistent hashing and replication. [Medium] [asked: very common]
//! Q4. Design a key-value store that survives a crash; code the write-ahead log. [Hard] [asked: very common]
//! Q5. Design a message queue with at-least-once delivery. [Medium] [asked: very common]
//! Q6. Design a notification system with retries and deduplication. [Medium] [asked: very common]
//! Q7. Design typeahead: a trie with top-k per prefix. [Medium] [asked: very common]
//! Q8. Design a web crawler's URL frontier with politeness. [Medium] [asked: common]
//! Q9. Design pub/sub where slow subscribers do not stall fast ones. [Medium] [asked: common]
//! Q10. Design a job scheduler with delays, priorities and retries. [Medium] [asked: very common]
//! Q11. Design leader election with a lease. Why is the lease alone not enough? [Hard] [asked: common]
//! Q12. Design a distributed unique ID generator (Snowflake). [Medium] [asked: very common]
//! Q13. Design "top K trending items" over an unbounded stream. [Hard] [asked: common]
//! Q14. Design a log aggregation pipeline with backpressure. [Medium] [asked: common]
//! Q15. Design a feature-flag service. [Medium] [asked: common]
//! Q16. Design a search index: boolean, phrase and ranked queries. [Hard] [asked: common]

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::thread;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix(self.0)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}
fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
/// A stable hash (FNV-1a + a finalizer): placement must not change between processes, which rules out `RandomState`.
fn hash(s: &str) -> u64 {
    mix(s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    }))
}

// ---------------------------------------------------------------------------
// Q1. URL shortener
// ---------------------------------------------------------------------------
/// REQUIREMENTS: create a short code for a long URL (optional custom alias); redirect by code; codes are unique and not trivially enumerable.
/// NUMBERS: 100 M new links/month ~ 40 writes/s (peak 400); 100:1 reads ~ 4k/s (peak 40k). 5 years = 6e9 rows x ~500 B = 3 TB.
///   62^7 = 3.5e12 codes, so 7 characters leave ~580x headroom.
/// API: POST /links {url, alias?} -> {code}; GET /{code} -> 301 (cacheable) or 302 (if you must count every click).
/// DATA: links(code PK, url, owner, created_at, expires_at). Shard by hash(code): every read has the code.
/// CORE: ids come from a counter (each app server leases a BLOCK of ids, so no per-write coordination), then pass through a BIJECTION on
///   [0, 62^7): unique because it is invertible, and neighbours do not look sequential. This is obfuscation, NOT secrecy.
///   The alternative, hash(url) truncated, needs a read-before-write and a probe on collision (also coded).
/// FAILURES: a server dies holding a block (the ids are wasted, which is fine); hot link (CDN + cache); alias race (unique constraint).
/// TRADE-OFFS: 301 saves load but loses analytics and cannot be revoked in browser caches.
/// REALLY TESTING: uniqueness without a global lock. WRONG: "random 7 chars never collide" (birthday bound: some collision becomes likely after ~2 M links, about sqrt(3.5e12)).
/// FOLLOW-UP: custom aliases share the namespace: how do you stop an alias from squatting a future generated code?
const CODE_SPACE: u64 = 62u64.pow(7);
const ALPHABET: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const MULT: u64 = 1_580_030_173; // odd and not a multiple of 31, hence coprime with 62^7
fn mod_inverse(a: u64, m: u64) -> u64 {
    let (mut old_r, mut r, mut old_s, mut s) = (a as i128, m as i128, 1i128, 0i128);
    while r != 0 {
        let q = old_r / r;
        (old_r, r) = (r, old_r - q * r);
        (old_s, s) = (s, old_s - q * s);
    }
    assert_eq!(old_r, 1, "not coprime");
    old_s.rem_euclid(m as i128) as u64
}
fn mulmod(a: u64, b: u64) -> u64 {
    (u128::from(a) * u128::from(b) % u128::from(CODE_SPACE)) as u64
}
fn base62(mut n: u64) -> String {
    let mut out = [b'0'; 7];
    for slot in out.iter_mut().rev() {
        *slot = ALPHABET[(n % 62) as usize];
        n /= 62;
    }
    String::from_utf8(out.to_vec()).unwrap()
}
fn unbase62(s: &str) -> Option<u64> {
    if s.len() != 7 {
        return None;
    }
    s.bytes().try_fold(0u64, |n, b| {
        Some(n * 62 + ALPHABET.iter().position(|&a| a == b)? as u64)
    })
}
#[derive(Default)]
struct Shortener {
    links: HashMap<String, String>,
    next_id: u64,
}
impl Shortener {
    fn create(&mut self, url: &str, alias: Option<&str>) -> Result<String, &'static str> {
        if let Some(a) = alias {
            // aliases must not be exactly 7 chars: that keeps them out of the generated namespace for ever
            if a.len() == 7 {
                return Err("alias length 7 is reserved");
            }
            if self.links.contains_key(a) {
                return Err("alias taken");
            }
            self.links.insert(a.to_string(), url.to_string());
            return Ok(a.to_string());
        }
        let code = base62(mulmod(self.next_id, MULT));
        self.next_id += 1;
        let previous = self.links.insert(code.clone(), url.to_string());
        assert!(previous.is_none(), "a bijection cannot collide");
        Ok(code)
    }
}
/// Content-addressed variant: truncate a hash, probe with a salt on collision. Same URL gives the same code.
fn hashed_code(table: &mut HashMap<u64, String>, url: &str, space: u64) -> (u64, u32) {
    for probe in 0u32.. {
        let code = hash(&format!("{url}#{probe}")) % space;
        match table.get(&code) {
            Some(u) if u != url => continue,
            _ => {
                table.insert(code, url.to_string());
                return (code, probe);
            }
        }
    }
    unreachable!()
}
fn q1_url_shortener() {
    let inv = mod_inverse(MULT, CODE_SPACE);
    let mut s = Shortener::default();
    let mut seen = HashSet::new();
    for id in 0..50_000u64 {
        let code = s
            .create(&format!("https://example.com/{id}"), None)
            .unwrap();
        assert_eq!(
            mulmod(unbase62(&code).unwrap(), inv),
            id,
            "the code decodes back to the id"
        );
        assert!(seen.insert(code));
    }
    for edge in [0, 1, 61, 62, CODE_SPACE - 1] {
        assert_eq!(unbase62(&base62(edge)), Some(edge));
    }
    assert_eq!(unbase62("!!!!!!!"), None);
    assert!(s.create("u", Some("docs")).is_ok());
    assert_eq!(s.create("v", Some("docs")), Err("alias taken"));
    assert_eq!(
        s.create("v", Some("abcdefg")),
        Err("alias length 7 is reserved")
    );
    // a deliberately tiny space forces collisions in the hashed variant
    let (mut table, mut probes, mut codes) = (HashMap::new(), 0, HashMap::new());
    for i in 0..900 {
        let url = format!("u{i}");
        let (c, p) = hashed_code(&mut table, &url, 1000);
        probes += p;
        codes.insert(url, c);
    }
    assert!(
        probes > 0 && table.len() == 900,
        "collisions happened and every URL still got its own code"
    );
    assert!(
        codes
            .iter()
            .all(|(u, &c)| hashed_code(&mut table, u, 1000).0 == c),
        "idempotent per URL"
    );
    println!(
        "   50000 unique codes; hashed variant at 90% load needed {probes} extra probes for 900 URLs"
    );
}

// ---------------------------------------------------------------------------
// Q2. rate limiter
// ---------------------------------------------------------------------------
/// REQUIREMENTS: per-key limit "L requests per window W" with optional burst; decide in under 1 ms; state shared by many gateways.
/// NUMBERS: 1 M active keys x ~50 B = 50 MB (counter/GCRA) versus 1 M x L=1000 x 8 B = 8 GB for a sliding LOG.
/// API: allow(key, now) -> bool (plus Retry-After). DATA: key -> small state, with TTL; atomic update (Redis script or owner shard).
/// CORE: fixed window is cheap but admits 2L around a boundary; the log is exact but big; the weighted counter approximates it;
///   token bucket and GCRA are the SAME policy: GCRA stores one timestamp (the theoretical arrival time) instead of a level.
/// FAILURES: limiter store down (fail open or closed? decide per endpoint); clock skew between gateways (use the store's clock).
/// TRADE-OFFS: a local limit of L/N per gateway needs no shared store but is wrong under uneven balancing.
/// REALLY TESTING: boundary behaviour and memory per key. WRONG: read-then-write without atomicity.
/// FOLLOW-UP: weighted requests; a hierarchy of limits (user inside tenant inside global).
struct FixedWindow {
    limit: u32,
    window: u64,
    cur: u64,
    count: u32,
}
impl FixedWindow {
    fn allow(&mut self, now: u64) -> bool {
        if now / self.window != self.cur {
            (self.cur, self.count) = (now / self.window, 0);
        }
        let ok = self.count < self.limit;
        self.count += u32::from(ok);
        ok
    }
}
struct SlidingLog {
    limit: usize,
    window: u64,
    log: VecDeque<u64>,
}
impl SlidingLog {
    fn allow(&mut self, now: u64) -> bool {
        while self.log.front().is_some_and(|&t| t + self.window <= now) {
            self.log.pop_front();
        }
        let ok = self.log.len() < self.limit;
        if ok {
            self.log.push_back(now);
        }
        ok
    }
}
struct SlidingCounter {
    limit: u64,
    window: u64,
    cur: u64,
    count: u64,
    prev: u64,
}
impl SlidingCounter {
    fn allow(&mut self, now: u64) -> bool {
        let w = now / self.window;
        if w != self.cur {
            self.prev = if w == self.cur + 1 { self.count } else { 0 };
            (self.cur, self.count) = (w, 0);
        }
        // estimate = prev * (fraction of the previous window still inside the sliding window) + count, in integers
        let weighted = self.prev * (self.window - now % self.window) + self.count * self.window;
        let ok = weighted + self.window <= self.limit * self.window;
        self.count += u64::from(ok);
        ok
    }
}
/// Level measured in milliseconds of credit: one token = `every` ms, so the arithmetic is exact.
struct TokenBucket {
    burst: u64,
    every: u64,
    level: u64,
    last: u64,
}
impl TokenBucket {
    fn new(burst: u64, every: u64) -> Self {
        TokenBucket {
            burst,
            every,
            level: burst * every,
            last: 0,
        }
    }
    fn allow(&mut self, now: u64) -> bool {
        self.level = (self.level + (now - self.last)).min(self.burst * self.every);
        self.last = now;
        let ok = self.level >= self.every;
        if ok {
            self.level -= self.every;
        }
        ok
    }
}
struct Gcra {
    burst: u64,
    every: u64,
    tat: u64,
}
impl Gcra {
    fn allow(&mut self, now: u64) -> bool {
        let ok = self.tat.saturating_sub(now) <= (self.burst - 1) * self.every;
        if ok {
            self.tat = self.tat.max(now) + self.every;
        }
        ok
    }
}
/// The most admissions found in any half-open interval of length `window`.
fn worst_in_window(admitted: &[u64], window: u64) -> usize {
    admitted
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            admitted[i..]
                .iter()
                .take_while(|&&t| t < s + window)
                .count()
        })
        .max()
        .unwrap_or(0)
}
fn q2_rate_limiter() {
    let mut fixed = FixedWindow {
        limit: 10,
        window: 1000,
        cur: 0,
        count: 0,
    };
    let boundary: Vec<u64> = [999; 10]
        .into_iter()
        .chain([1000; 10])
        .filter(|&t| fixed.allow(t))
        .collect();
    assert_eq!(boundary.len(), 20, "fixed window admits 2L within 2 ms");
    let mut r = Rng(2);
    let mut worst_counter = 0;
    for _ in 0..200 {
        let mut log = SlidingLog {
            limit: 10,
            window: 1000,
            log: VecDeque::new(),
        };
        let mut counter = SlidingCounter {
            limit: 10,
            window: 1000,
            cur: 0,
            count: 0,
            prev: 0,
        };
        let (mut tb, mut gcra) = (
            TokenBucket::new(5, 100),
            Gcra {
                burst: 5,
                every: 100,
                tat: 0,
            },
        );
        let (mut now, mut by_log, mut by_counter, mut by_tb, mut all) =
            (0, vec![], vec![], vec![], vec![]);
        for _ in 0..600 {
            now += if r.below(10) == 0 {
                r.below(900)
            } else {
                r.below(30)
            };
            // oracle for the log: count earlier admissions inside (now - W, now]
            let inside = by_log.iter().filter(|&&t: &&u64| t + 1000 > now).count();
            let ok = log.allow(now);
            assert_eq!(ok, inside < 10);
            if ok {
                by_log.push(now);
            }
            if counter.allow(now) {
                by_counter.push(now);
            }
            let a = tb.allow(now);
            assert_eq!(
                a,
                gcra.allow(now),
                "token bucket and GCRA are the same policy (t={now})"
            );
            if a {
                by_tb.push(now);
            }
            all.push(now);
        }
        assert!(worst_in_window(&by_log, 1000) <= 10);
        worst_counter = worst_counter.max(worst_in_window(&by_counter, 1000));
        // token bucket bound: any interval of length d admits at most burst + d/every
        assert!(worst_in_window(&by_tb, 1000) <= 5 + 10);
    }
    assert!(
        worst_counter > 10 && worst_counter <= 20,
        "the weighted counter is an approximation: worst {worst_counter}"
    );
    println!(
        "   limit 10/s: fixed window admitted 20 at a boundary; sliding counter worst {worst_counter}; sliding log never above 10"
    );
}

// ---------------------------------------------------------------------------
// Q3. distributed cache
// ---------------------------------------------------------------------------
/// REQUIREMENTS: get/set with TTL across N nodes; adding or losing a node must not remap most keys; survive one node loss without a cold cache.
/// NUMBERS: 1 M reads/s, 100 GB hot set, 32 GB per node -> 4 nodes for data, x3 replicas -> 12+. At 100k ops/s/node, reads need 10+.
/// API: get(key), set(key, value, ttl), delete(key). DATA: ring of (hash -> node) with V virtual points per node; value + expiry per node.
/// CORE: walk clockwise from hash(key), collect the first R DISTINCT nodes. Modulo hashing remaps ~ n/(n+1) of keys on a resize.
/// FAILURES: dead node (read the next replica); stampede on a hot expired key (singleflight, jittered TTL); a replica that missed a
///   delete serves stale data after rejoining (flush on rejoin, or versions + TTL).
/// TRADE-OFFS: more virtual points = better balance but a bigger ring. A cache may lose data; do not turn it into the source of truth.
/// REALLY TESTING: why consistent hashing, with a number. WRONG: replicas chosen as "next points" without skipping the same node.
/// FOLLOW-UP: bounded-load consistent hashing; rendezvous hashing (no ring, O(nodes) per lookup).
struct Ring {
    points: BTreeMap<u64, u32>,
    vnodes: u32,
}
impl Ring {
    fn new(nodes: impl Iterator<Item = u32>, vnodes: u32) -> Self {
        let mut r = Ring {
            points: BTreeMap::new(),
            vnodes,
        };
        nodes.for_each(|n| r.add(n));
        r
    }
    fn add(&mut self, node: u32) {
        for v in 0..self.vnodes {
            self.points.insert(hash(&format!("node-{node}-{v}")), node);
        }
    }
    fn replicas(&self, key: &str, n: usize) -> Vec<u32> {
        let h = hash(key);
        let mut out = Vec::new();
        for (_, &node) in self.points.range(h..).chain(self.points.range(..h)) {
            if !out.contains(&node) {
                out.push(node);
                if out.len() == n {
                    break;
                }
            }
        }
        out
    }
}
fn q3_cache() {
    let keys: Vec<String> = (0..20_000).map(|i| format!("user:{i}")).collect();
    let mut ring = Ring::new(0..10, 160);
    let before: Vec<Vec<u32>> = keys.iter().map(|k| ring.replicas(k, 3)).collect();
    assert!(
        before
            .iter()
            .all(|r| r.len() == 3 && r.iter().collect::<HashSet<_>>().len() == 3)
    );
    let mut load = [0usize; 10];
    before.iter().for_each(|r| load[r[0] as usize] += 1);
    let (min, max) = (*load.iter().min().unwrap(), *load.iter().max().unwrap());
    assert!(
        min > 1200 && max < 3000,
        "160 vnodes keep primaries within a factor of the 2000 mean: {load:?}"
    );
    ring.add(10);
    let moved: Vec<usize> = (0..keys.len())
        .filter(|&i| ring.replicas(&keys[i], 1)[0] != before[i][0])
        .collect();
    assert!(
        moved.iter().all(|&i| ring.replicas(&keys[i], 1)[0] == 10),
        "keys move ONLY to the new node"
    );
    assert!(
        moved.len() > 20_000 / 22 && moved.len() < 20_000 / 6,
        "about 1/11 moved: {}",
        moved.len()
    );
    let modulo_moved = keys.iter().filter(|k| hash(k) % 10 != hash(k) % 11).count();
    assert!(modulo_moved > 17_000);
    // one dead node: every key still has two live replicas that already hold it
    for dead in 0..11 {
        assert!(
            keys.iter().step_by(37).all(|k| ring
                .replicas(k, 3)
                .iter()
                .filter(|&&n| n != dead)
                .count()
                >= 2)
        );
    }
    println!(
        "   add 11th node: ring moved {} of 20000 primaries, modulo hashing would move {modulo_moved}",
        moved.len()
    );
}

// ---------------------------------------------------------------------------
// Q4. key-value store with a write-ahead log
// ---------------------------------------------------------------------------
/// REQUIREMENTS: put/delete/get; an acknowledged write survives a crash; recovery tolerates a half-written last record.
/// NUMBERS: 20k writes/s x 200 B = 4 MB/s ~ 350 GB/day of log without compaction. One fsync per write caps you at device sync rate;
///   GROUP COMMIT shares one fsync among all writes waiting for it.
/// API: put(k, v) returns only after the record is durable. DATA: log of [len u32][checksum u32][op u8][klen u32][key][value]; map in memory.
/// CORE: append, sync, THEN apply and acknowledge. Replay stops at the first record that is short or fails its checksum, and truncates there.
/// FAILURES: torn tail (expected); corruption in the MIDDLE (this code also stops there; a real store must alarm, not silently drop acknowledged data);
///   fsync returning an error (do not retry and carry on: the page cache state is unknown).
/// TRADE-OFFS: checkpoint/compaction bounds recovery time at the cost of extra writes.
/// REALLY TESTING: the order of durability and acknowledgement. WRONG: apply to memory, reply, then write the log.
/// FOLLOW-UP: how do you make compaction itself crash safe? (new file, sync, atomic rename, sync directory).
fn checksum(b: &[u8]) -> u32 {
    // bitwise CRC-32 (IEEE, reflected)
    !b.iter().fold(!0u32, |c, &x| {
        (0..8).fold(c ^ u32::from(x), |c, _| {
            (c >> 1) ^ (0xEDB8_8320 & (c & 1).wrapping_neg())
        })
    })
}
fn encode_record(key: &str, value: Option<&str>) -> Vec<u8> {
    let mut body = vec![u8::from(value.is_some())];
    body.extend((key.len() as u32).to_le_bytes());
    body.extend(key.as_bytes());
    body.extend(value.unwrap_or("").as_bytes());
    let mut rec = (body.len() as u32).to_le_bytes().to_vec();
    rec.extend(checksum(&body).to_le_bytes());
    rec.extend(body);
    rec
}
/// Returns the recovered map and the length of the valid prefix.
fn replay(log: &[u8]) -> (BTreeMap<String, String>, usize) {
    let (mut map, mut pos) = (BTreeMap::new(), 0);
    let u32_at = |p: usize| Some(u32::from_le_bytes(log.get(p..p + 4)?.try_into().ok()?));
    loop {
        let parsed = (|| {
            let len = u32_at(pos)? as usize;
            let body = log.get(pos + 8..(pos + 8).checked_add(len)?)?;
            if checksum(body) != u32_at(pos + 4)? || body.len() < 5 {
                return None;
            }
            let klen = u32::from_le_bytes(body[1..5].try_into().ok()?) as usize;
            let key = std::str::from_utf8(body.get(5..5usize.checked_add(klen)?)?).ok()?;
            let value = std::str::from_utf8(&body[5 + klen..]).ok()?;
            Some((body[0] == 1, key.to_string(), value.to_string(), 8 + len))
        })();
        let Some((is_put, key, value, used)) = parsed else {
            return (map, pos);
        };
        if is_put {
            map.insert(key, value);
        } else {
            map.remove(&key);
        }
        pos += used;
    }
}
fn q4_wal() {
    let mut r = Rng(4);
    let (mut log, mut model) = (Vec::new(), BTreeMap::new());
    let mut states = vec![(0usize, model.clone())]; // (log length, state) after each record
    for i in 0..120 {
        let key = format!("k{}", r.below(15));
        if r.below(4) == 0 {
            log.extend(encode_record(&key, None));
            model.remove(&key);
        } else {
            let v = format!("v{i}");
            log.extend(encode_record(&key, Some(&v)));
            model.insert(key, v);
        }
        states.push((log.len(), model.clone()));
    }
    // CRASH at every byte offset: recovery equals the state after the last record that fits completely
    for cut in 0..=log.len() {
        let (got, valid) = replay(&log[..cut]);
        let want = states.iter().rev().find(|(len, _)| *len <= cut).unwrap();
        assert!(got == want.1 && valid == want.0, "crash at byte {cut}");
    }
    // flip one bit anywhere: replay stops at the damaged record and never invents data
    for byte in 0..log.len() {
        let mut bad = log.clone();
        bad[byte] ^= 1 << r.below(8);
        let (got, valid) = replay(&bad);
        let want = states.iter().rev().find(|(len, _)| *len <= byte).unwrap();
        assert!(got == want.1 && valid == want.0, "bit flip in byte {byte}");
    }
    let compacted: Vec<u8> = model
        .iter()
        .flat_map(|(k, v)| encode_record(k, Some(v)))
        .collect();
    assert_eq!(replay(&compacted).0, model);
    assert_eq!(
        checksum(b"123456789"),
        0xCBF4_3926,
        "the standard CRC-32 check value"
    );
    println!(
        "   {} crash points and {} bit flips recovered correctly; compaction {} -> {} bytes",
        log.len() + 1,
        log.len(),
        log.len(),
        compacted.len()
    );
}

// ---------------------------------------------------------------------------
// Q5. message queue with at-least-once delivery
// ---------------------------------------------------------------------------
/// REQUIREMENTS: publish, receive, ack; a consumer that dies must not lose its message; poison messages must not loop for ever.
/// NUMBERS: 10k msg/s x 1 KB = 10 MB/s; 4-day retention ~ 3.5 TB before replication.
/// API: send(body); receive(now) -> (id, receipt); ack(receipt). DATA: messages(id, body, visible_at, deliveries, receipt).
/// CORE: receive HIDES a message until now + visibility timeout and issues a fresh receipt; only the latest receipt can ack.
///   After MAX deliveries the message moves to the dead-letter queue.
/// FAILURES: slow consumer (redelivered while still working: consumers MUST be idempotent, or extend the timeout with a heartbeat).
/// TRADE-OFFS: long timeout = slow recovery; short timeout = duplicates. "Exactly once" is at-least-once + idempotent effect.
/// REALLY TESTING: where duplicates come from. WRONG: delete on receive.
/// FOLLOW-UP: FIFO per key (message groups: one in flight per group). The linear scan here must become two ordered indexes.
#[derive(Default)]
struct Queue {
    msgs: BTreeMap<u64, (u64, u32, u64)>, // id -> (visible_at, deliveries, current receipt)
    dead: Vec<u64>,
    next_id: u64,
    next_receipt: u64,
}
impl Queue {
    const VISIBILITY: u64 = 10;
    const MAX_DELIVERIES: u32 = 5;
    fn send(&mut self) -> u64 {
        self.next_id += 1;
        self.msgs.insert(self.next_id, (0, 0, 0));
        self.next_id
    }
    fn receive(&mut self, now: u64) -> Option<(u64, u64)> {
        loop {
            let id = *self.msgs.iter().find(|(_, m)| m.0 <= now)?.0;
            if self.msgs[&id].1 == Self::MAX_DELIVERIES {
                self.msgs.remove(&id);
                self.dead.push(id);
                continue;
            }
            self.next_receipt += 1;
            let m = self.msgs.get_mut(&id).unwrap();
            *m = (now + Self::VISIBILITY, m.1 + 1, self.next_receipt);
            return Some((id, self.next_receipt));
        }
    }
    fn ack(&mut self, id: u64, receipt: u64) -> bool {
        let current = self.msgs.get(&id).is_some_and(|m| m.2 == receipt);
        if current {
            self.msgs.remove(&id);
        }
        current
    }
}
fn q5_queue() {
    let mut r = Rng(5);
    let (mut redeliveries, mut stale_acks) = (0, 0);
    for _ in 0..200 {
        let mut q = Queue::default();
        let ids: Vec<u64> = (0..30).map(|_| q.send()).collect();
        let poison = ids[r.below(30) as usize];
        let (mut now, mut processed, mut acked, mut working) =
            (0, HashMap::<u64, u32>::new(), HashSet::new(), Vec::new());
        while !q.msgs.is_empty() {
            now += r.below(4);
            if let Some((id, rec)) = q.receive(now) {
                // some consumers die (never finish), some are slower than the visibility timeout
                if id != poison && r.below(5) != 0 {
                    working.push((now + r.below(16), id, rec));
                }
            }
            working.retain(|&(done, id, rec)| {
                if done > now {
                    return true;
                }
                *processed.entry(id).or_default() += 1;
                if q.ack(id, rec) {
                    assert!(acked.insert(id), "a message is acked at most once");
                } else {
                    stale_acks += 1;
                }
                false
            });
        }
        redeliveries += processed.values().filter(|&&n| n > 1).count();
        let dead: HashSet<u64> = q.dead.iter().copied().collect();
        assert!(dead.contains(&poison), "the poison message is parked");
        assert!(acked.is_disjoint(&dead));
        assert!(
            ids.iter().all(|i| acked.contains(i) != dead.contains(i)),
            "nothing is lost"
        );
        assert!(acked.iter().all(|i| processed[i] >= 1));
    }
    assert!(redeliveries > 0 && stale_acks > 0);
    println!(
        "   200 chaos runs: nothing lost; {redeliveries} messages processed more than once; {stale_acks} stale acks rejected"
    );
}

// ---------------------------------------------------------------------------
// Q6. notification system
// ---------------------------------------------------------------------------
/// REQUIREMENTS: send email/push once per business event, retry provider failures, never spam on retries.
/// NUMBERS: 50 M/day ~ 600/s average, 10x peaks for campaigns -> queue absorbs the peak; provider quotas set the drain rate.
/// API: notify(event_id, user, template). DATA: outbox(key=(event_id,user,channel), state, attempts, next_at); provider message ids.
/// CORE: dedupe the REQUEST by key; retry with exponential backoff + jitter; pass the same key to the provider, because a timeout
///   does not tell you whether it sent. Give up after N attempts and record it.
/// FAILURES: lost response after a successful send (the case that creates duplicates); provider outage (circuit breaker, second provider).
/// TRADE-OFFS: if the provider has no idempotency key you must choose: possible duplicate (retry) or possible loss (do not).
/// REALLY TESTING: do you see the ambiguous timeout? WRONG: "mark as sent, then send" or "send, then mark" claimed to be exactly once.
/// FOLLOW-UP: user preferences, quiet hours, per-user rate caps, priority lanes so campaigns cannot delay password resets.
#[derive(Default)]
struct Provider {
    delivered: Vec<u64>,
    keys: HashSet<u64>,
    supports_keys: bool,
}
impl Provider {
    /// Err means "no response": the message may or may not have been delivered.
    fn send(&mut self, key: u64, r: &mut Rng) -> Result<(), ()> {
        match r.below(10) {
            0..=2 => return Err(()), // failed before delivery
            n => {
                if !self.supports_keys || self.keys.insert(key) {
                    self.delivered.push(key);
                }
                if n <= 4 { Err(()) } else { Ok(()) } // delivered, but the response was lost
            }
        }
    }
}
/// Returns (keys given up on, send attempts).
fn deliver_all(requests: &[u64], p: &mut Provider, r: &mut Rng) -> (Vec<u64>, u32) {
    let (mut accepted, mut due) = (HashSet::new(), BinaryHeap::new());
    for &key in requests {
        if accepted.insert(key) {
            due.push(Reverse((0u64, key, 0u32)));
        }
    }
    let (mut gave_up, mut attempts) = (vec![], 0);
    while let Some(Reverse((at, key, n))) = due.pop() {
        attempts += 1;
        if p.send(key, r).is_err() {
            if n + 1 == 12 {
                gave_up.push(key);
            } else {
                let backoff = (1u64 << n.min(8)) * 100;
                due.push(Reverse((
                    at + backoff / 2 + r.below(backoff / 2 + 1),
                    key,
                    n + 1,
                )));
            }
        }
    }
    (gave_up, attempts)
}
fn q6_notifications() {
    let mut r = Rng(6);
    // upstream itself redelivers: every event arrives one to three times
    let requests: Vec<u64> = (0..2000u64)
        .flat_map(|k| std::iter::repeat_n(k, 1 + (k % 3) as usize))
        .collect();
    let mut with_keys = Provider {
        supports_keys: true,
        ..Default::default()
    };
    let (gave_up, attempts) = deliver_all(&requests, &mut with_keys, &mut r);
    let unique: HashSet<_> = with_keys.delivered.iter().collect();
    assert_eq!(
        unique.len(),
        with_keys.delivered.len(),
        "no user got the same notification twice"
    );
    assert!(
        (0..2000).all(|k| unique.contains(&k) || gave_up.contains(&k)),
        "every request is delivered or reported"
    );
    let mut without = Provider::default();
    deliver_all(&requests, &mut without, &mut r);
    let dupes = without.delivered.len() - without.delivered.iter().collect::<HashSet<_>>().len();
    assert!(dupes > 100);
    println!(
        "   {attempts} attempts for 2000 notifications, {} gave up; without a provider key: {dupes} duplicate sends",
        gave_up.len()
    );
}

// ---------------------------------------------------------------------------
// Q7. typeahead
// ---------------------------------------------------------------------------
/// REQUIREMENTS: for a prefix return the k most frequent queries, p99 under 20 ms per keystroke; counts may lag by minutes.
/// NUMBERS: 10 M distinct queries x ~20 chars ~ up to 2e8 trie nodes uncompressed -> compress paths, cap depth, shard by prefix.
///   5 B searches/day x ~10 keystrokes = 600k lookups/s: precompute, cache at the edge.
/// API: suggest(prefix) -> [query]. DATA: trie; each node caches its top k as (count, query).
/// CORE: reads walk the prefix and return the cache. When one count RISES, only that query can move up, so re-ranking it in each
///   ancestor's list is exact. (Falling counts break this: see TRY NEXT.)
/// FAILURES: offensive suggestions (filter list applied at serve time); stale index (build offline, swap atomically).
/// TRADE-OFFS: O(len x k) writes for O(len) reads.
/// REALLY TESTING: moving work to write time. WRONG: DFS of the whole subtree on every keystroke.
/// FOLLOW-UP: typo tolerance, personalisation, time-decayed counts.
#[derive(Default)]
struct TrieNode {
    children: BTreeMap<u8, TrieNode>,
    top: Vec<(Reverse<u64>, String)>,
}
struct Typeahead {
    root: TrieNode,
    counts: HashMap<String, u64>,
    k: usize,
}
impl Typeahead {
    fn record(&mut self, q: &str, times: u64) {
        let total = *self
            .counts
            .entry(q.to_string())
            .and_modify(|c| *c += times)
            .or_insert(times);
        let k = self.k;
        let update = |n: &mut TrieNode| {
            n.top.retain(|(_, s)| s != q);
            let item = (Reverse(total), q.to_string());
            let at = n.top.binary_search(&item).unwrap_err();
            n.top.insert(at, item);
            n.top.truncate(k);
        };
        let mut node = &mut self.root;
        update(node);
        for b in q.bytes() {
            node = node.children.entry(b).or_default();
            update(node);
        }
    }
    fn suggest(&self, prefix: &str) -> Vec<&str> {
        prefix
            .bytes()
            .try_fold(&self.root, |n, b| n.children.get(&b))
            .map_or_else(Vec::new, |n| {
                n.top.iter().map(|(_, s)| s.as_str()).collect()
            })
    }
}
fn q7_typeahead() {
    let mut r = Rng(7);
    let mut t = Typeahead {
        root: TrieNode::default(),
        counts: HashMap::new(),
        k: 4,
    };
    let word = |r: &mut Rng, max: u64| {
        (0..1 + r.below(max))
            .map(|_| char::from(b'a' + r.below(4) as u8))
            .collect::<String>()
    };
    for step in 0..4000 {
        let q = word(&mut r, 6);
        t.record(&q, 1 + r.below(5));
        if step % 10 == 0 {
            let prefix = if r.below(5) == 0 {
                String::new()
            } else {
                word(&mut r, 3)
            };
            let mut want: Vec<(Reverse<u64>, &str)> = t
                .counts
                .iter()
                .filter(|(q, _)| q.starts_with(&prefix))
                .map(|(q, &c)| (Reverse(c), q.as_str()))
                .collect();
            want.sort_unstable();
            assert_eq!(
                t.suggest(&prefix),
                want.iter().take(4).map(|x| x.1).collect::<Vec<_>>(),
                "prefix {prefix:?}"
            );
        }
    }
    println!(
        "   {} distinct queries: cached top-4 equals a brute-force scan at 400 checkpoints",
        t.counts.len()
    );
}

// ---------------------------------------------------------------------------
// Q8. crawler frontier
// ---------------------------------------------------------------------------
/// REQUIREMENTS: fetch each URL once, never hit one host more than once per delay or twice at the same time, use many workers.
/// NUMBERS: 1 B pages/month ~ 400 pages/s; at 100 KB each ~ 40 MB/s; politeness of 1 s/host means 400+ hosts must be active at once.
/// API: add(url); next(now) -> url?; done(host, now). DATA: per-host FIFO; heap of (earliest allowed time, host); seen-set (Bloom + store).
/// CORE: a host is in the heap only when it has work and no request in flight. `done` re-arms it at now + delay.
/// FAILURES: spider traps (cap depth and per-host URLs); a worker dies (lease the host, re-arm on expiry); robots.txt changes.
/// TRADE-OFFS: a Bloom filter seen-set saves memory but false positives skip pages for ever.
/// REALLY TESTING: scheduling by next-eligible time. WRONG: one global FIFO plus sleep().
/// FOLLOW-UP: priority (freshness, rank) inside politeness; URL canonicalisation before dedupe.
#[derive(Default)]
struct Frontier {
    seen: HashSet<(String, String)>,
    queues: HashMap<String, VecDeque<String>>,
    ready: BinaryHeap<Reverse<(u64, String)>>,
    busy: HashSet<String>,
    next_ok: HashMap<String, u64>,
}
impl Frontier {
    const DELAY: u64 = 20;
    fn add(&mut self, host: &str, path: &str) -> bool {
        if !self.seen.insert((host.into(), path.into())) {
            return false;
        }
        let q = self.queues.entry(host.into()).or_default();
        q.push_back(path.into());
        if q.len() == 1 && !self.busy.contains(host) {
            self.ready.push(Reverse((
                *self.next_ok.get(host).unwrap_or(&0),
                host.into(),
            )));
        }
        true
    }
    fn next(&mut self, now: u64) -> Option<(String, String)> {
        if self.ready.peek()?.0.0 > now {
            return None;
        }
        let Reverse((_, host)) = self.ready.pop()?;
        let path = self.queues.get_mut(&host)?.pop_front()?;
        self.busy.insert(host.clone());
        Some((host, path))
    }
    fn done(&mut self, host: &str, now: u64) {
        assert!(self.busy.remove(host));
        self.next_ok.insert(host.into(), now + Self::DELAY);
        if !self.queues[host].is_empty() {
            self.ready.push(Reverse((now + Self::DELAY, host.into())));
        }
    }
}
fn q8_frontier() {
    let mut r = Rng(8);
    let mut f = Frontier::default();
    let mut expected = HashSet::new();
    let add_random = |f: &mut Frontier, r: &mut Rng, expected: &mut HashSet<(String, String)>| {
        let (h, p) = (format!("h{}", r.below(12)), format!("/{}", r.below(60)));
        assert_eq!(
            f.add(&h, &p),
            expected.insert((h, p)),
            "dedupe agrees with a set"
        );
    };
    (0..150).for_each(|_| add_random(&mut f, &mut r, &mut expected));
    let (mut now, mut fetched, mut in_flight, mut finished_at) = (
        0u64,
        HashSet::new(),
        Vec::<(u64, String)>::new(),
        HashMap::<String, u64>::new(),
    );
    let mut discoveries = 400;
    while fetched.len() < expected.len() || !in_flight.is_empty() {
        now += 1;
        in_flight.retain(|(end, host)| {
            if *end > now {
                return true;
            }
            f.done(host, now);
            finished_at.insert(host.clone(), now);
            false
        });
        while in_flight.len() < 5 {
            let Some((h, p)) = f.next(now) else { break };
            assert!(
                !in_flight.iter().any(|(_, x)| *x == h),
                "one request per host at a time"
            );
            assert!(
                finished_at
                    .get(&h)
                    .is_none_or(|&t| now >= t + Frontier::DELAY),
                "politeness delay for {h}"
            );
            assert!(fetched.insert((h.clone(), p)), "each URL once");
            in_flight.push((now + 1 + r.below(8), h));
            if discoveries > 0 {
                discoveries -= 1;
                add_random(&mut f, &mut r, &mut expected);
            }
        }
        assert!(now < 100_000, "the frontier made no progress");
    }
    assert_eq!(fetched, expected);
    println!(
        "   {} distinct URLs on 12 hosts fetched once each by 5 workers in {now} ticks, politeness never violated",
        fetched.len()
    );
}

// ---------------------------------------------------------------------------
// Q9. pub/sub
// ---------------------------------------------------------------------------
/// REQUIREMENTS: every subscriber sees a topic's messages in order, at its own pace; memory is bounded.
/// NUMBERS: 100k msg/s x 500 B = 50 MB/s; one shared log, not one copy per subscriber: 1000 subscribers would be 50 GB/s of copies.
/// API: publish(msg) -> offset; poll(sub, max) -> Ok(batch) or Gap; seek(sub, offset). DATA: append-only log + one cursor per subscriber.
/// CORE: retention trims the head. A subscriber whose cursor is behind the head gets an explicit GAP, never silent loss.
/// FAILURES: slow subscriber (alert on lag); subscriber crash before committing its cursor (redelivery).
/// TRADE-OFFS: trim (publisher never blocks, laggards lose) versus backpressure (nobody loses, one laggard stalls all).
/// REALLY TESTING: log + cursor versus per-subscriber queues. WRONG: removing a message once ONE subscriber read it.
/// FOLLOW-UP: partitions and consumer groups: ordering holds only within a partition.
struct Topic {
    log: VecDeque<u64>,
    head: u64,
    retain: usize,
}
impl Topic {
    fn publish(&mut self, v: u64) {
        self.log.push_back(v);
        if self.log.len() > self.retain {
            self.log.pop_front();
            self.head += 1;
        }
    }
    /// Err(n) = n messages were trimmed before this subscriber read them; the cursor jumps to the head.
    fn poll(&self, cursor: &mut u64, max: usize) -> Result<Vec<u64>, u64> {
        if *cursor < self.head {
            let lost = self.head - *cursor;
            *cursor = self.head;
            return Err(lost);
        }
        let out: Vec<u64> = self
            .log
            .iter()
            .skip((*cursor - self.head) as usize)
            .take(max)
            .copied()
            .collect();
        *cursor += out.len() as u64;
        Ok(out)
    }
}
fn q9_pubsub() {
    let mut r = Rng(9);
    let mut t = Topic {
        log: VecDeque::new(),
        head: 0,
        retain: 64,
    };
    let speeds = [20usize, 6, 1];
    let (mut cursors, mut got, mut lost, mut next_expected) =
        ([0u64; 3], [0u64; 3], [0u64; 3], [0u64; 3]);
    let total = 20_000u64;
    let mut published = 0;
    while published < total || cursors.iter().any(|&c| c < total) {
        if published < total {
            for _ in 0..r.below(9) {
                if published < total {
                    t.publish(published);
                    published += 1;
                }
            }
        }
        for s in 0..3 {
            match t.poll(&mut cursors[s], speeds[s]) {
                Ok(batch) => {
                    for v in batch {
                        assert_eq!(v, next_expected[s], "in order and contiguous between gaps");
                        next_expected[s] += 1;
                        got[s] += 1;
                    }
                }
                Err(n) => {
                    lost[s] += n;
                    next_expected[s] += n;
                }
            }
        }
        assert!(t.log.len() <= 64);
    }
    assert!(
        (0..3).all(|s| got[s] + lost[s] == total),
        "every message is either delivered or reported lost"
    );
    assert!(lost[0] == 0 && lost[2] > lost[1]);
    println!("   delivered {got:?} / reported lost {lost:?} for fast, medium and slow subscribers");
}

// ---------------------------------------------------------------------------
// Q10. job scheduler
// ---------------------------------------------------------------------------
/// REQUIREMENTS: run jobs at or after run_at, higher priority first among due jobs, FIFO inside a priority, bounded retries.
/// NUMBERS: 10 M jobs/day ~ 115/s; 1 M pending delayed jobs x 200 B = 200 MB: an in-memory index over a durable table is enough.
/// API: submit(job, run_at, priority); poll(now) -> job?; fail(job, now). DATA: jobs table; index (state, run_at) and (priority, seq).
/// CORE: TWO heaps: `delayed` by time and `ready` by (priority, sequence). Promote everything due, then pop the best ready job.
///   One heap keyed (run_at, priority) is wrong: an old low-priority job would beat a newer high-priority one.
/// FAILURES: worker dies (lease + heartbeat); duplicate firing (idempotent jobs); starvation of low priority (aging or quotas).
/// TRADE-OFFS: strict priority is simple and can starve.
/// REALLY TESTING: choosing the two orderings. WRONG: polling the whole table every tick.
/// FOLLOW-UP: cron schedules and missed runs after downtime: run once, run all, or skip?
type ReadyKey = (u8, Reverse<u64>, u64); // (priority, older first, job)
#[derive(Default)]
struct Scheduler {
    delayed: BinaryHeap<Reverse<(u64, u64, u8, u64)>>, // (run_at, seq, priority, job)
    ready: BinaryHeap<ReadyKey>,
    seq: u64,
}
impl Scheduler {
    fn submit(&mut self, job: u64, run_at: u64, priority: u8) {
        self.seq += 1;
        self.delayed
            .push(Reverse((run_at, self.seq, priority, job)));
    }
    fn poll(&mut self, now: u64) -> Option<u64> {
        while self.delayed.peek().is_some_and(|d| d.0.0 <= now) {
            let Reverse((_, seq, priority, job)) = self.delayed.pop().unwrap();
            self.ready.push((priority, Reverse(seq), job));
        }
        self.ready.pop().map(|x| x.2)
    }
}
fn q10_scheduler() {
    let mut r = Rng(10);
    for _ in 0..200 {
        let mut s = Scheduler::default();
        let mut model: Vec<(u64, u64, u8, u64)> = vec![]; // (run_at, seq, priority, job), searched linearly
        let (mut now, mut next_job, mut seq) = (0, 0u64, 0);
        let mut attempts = HashMap::<u64, u32>::new();
        let mut prio_of = HashMap::new();
        for _ in 0..300 {
            now += r.below(3);
            if r.below(2) == 0 {
                let (run_at, p) = (now + r.below(20), r.below(3) as u8);
                s.submit(next_job, run_at, p);
                seq += 1;
                model.push((run_at, seq, p, next_job));
                prio_of.insert(next_job, p);
                next_job += 1;
                continue;
            }
            let best = (0..model.len())
                .filter(|&i| model[i].0 <= now)
                .max_by_key(|&i| (model[i].2, Reverse(model[i].1)));
            let want = best.map(|i| model.remove(i).3);
            assert_eq!(s.poll(now), want);
            if let Some(job) = want {
                let n = attempts.entry(job).or_default();
                *n += 1;
                if r.below(3) == 0 && *n < 3 {
                    let run_at = now + (1 << *n);
                    s.submit(job, run_at, prio_of[&job]);
                    seq += 1;
                    model.push((run_at, seq, prio_of[&job], job));
                }
            }
        }
        assert!(attempts.values().all(|&n| n <= 3));
    }
    println!(
        "   200 random runs: two-heap scheduler matched a linear-scan model, retries capped at 3 attempts"
    );
}

// ---------------------------------------------------------------------------
// Q11. leader election with a lease
// ---------------------------------------------------------------------------
/// REQUIREMENTS: at most one effective writer; failover within seconds when the leader dies.
/// NUMBERS: lease 10 s, renew every 3 s -> ~0.3 writes/s on the coordination store per group; worst failover ~10 s + election.
/// API: acquire(node, now) -> token?; write(token, value). DATA: lease(holder, expires_at, token) updated by compare-and-set.
/// CORE: the token increases on every NEW grant, and the PROTECTED RESOURCE rejects any token lower than the highest it has seen.
///   The lease alone is not safe: a leader can pause (GC, VM migration) after checking its lease and write after it expired.
/// FAILURES: clock skew (compare durations on the store's clock); the store itself must be linearisable (that is Raft/Paxos' job).
/// TRADE-OFFS: short leases fail over fast and flap under load.
/// REALLY TESTING: fencing. WRONG: "the leader checks the lease is still valid before writing".
/// FOLLOW-UP: what if the resource cannot check tokens? Then you only have probabilistic mutual exclusion.
#[derive(Default)]
struct LeaseStore {
    holder: Option<(u32, u64)>, // (node, expires_at)
    token: u64,
}
impl LeaseStore {
    fn acquire(&mut self, node: u32, now: u64) -> Option<u64> {
        match self.holder {
            Some((h, exp)) if exp > now && h != node => None,
            Some((h, exp)) if exp > now && h == node => {
                self.holder = Some((node, now + 10)); // renewal keeps the token
                Some(self.token)
            }
            _ => {
                self.token += 1;
                self.holder = Some((node, now + 10));
                Some(self.token)
            }
        }
    }
}
#[derive(Default)]
struct Resource {
    highest: u64,
    accepted: Vec<u64>,
}
impl Resource {
    fn write(&mut self, token: u64, fenced: bool) -> bool {
        if fenced && token < self.highest {
            return false;
        }
        self.highest = self.highest.max(token);
        self.accepted.push(token);
        true
    }
}
/// Returns (stale writes accepted, writes rejected).
fn lease_chaos(seed: u64, fenced: bool) -> (usize, usize) {
    let mut r = Rng(seed);
    let (mut store, mut res, mut now, mut rejected) =
        (LeaseStore::default(), Resource::default(), 0, 0);
    let mut believes = [None::<u64>; 3]; // the token each node thinks it holds; a paused node never re-checks
    for _ in 0..300 {
        now += r.below(5);
        let n = r.below(3) as usize;
        if r.below(2) == 0 {
            if let Some(t) = store.acquire(n as u32, now) {
                believes[n] = Some(t);
            }
        } else if let Some(t) = believes[n] {
            rejected += usize::from(!res.write(t, fenced));
        }
    }
    let stale = res.accepted.windows(2).filter(|w| w[1] < w[0]).count();
    (stale, rejected)
}
fn q11_lease() {
    let (mut unfenced_stale, mut fenced_rejected) = (0, 0);
    for seed in 0..500 {
        unfenced_stale += lease_chaos(seed, false).0;
        let (stale, rejected) = lease_chaos(seed, true);
        assert_eq!(
            stale, 0,
            "a fenced resource never goes back to an older leader"
        );
        fenced_rejected += rejected;
    }
    assert!(unfenced_stale > 0 && fenced_rejected > 0);
    println!(
        "   500 schedules: unfenced accepted {unfenced_stale} writes from an older leader after a newer one; fenced accepted 0"
    );
}

// ---------------------------------------------------------------------------
// Q12. Snowflake IDs
// ---------------------------------------------------------------------------
/// REQUIREMENTS: 64-bit ids, unique across nodes, roughly time ordered, no coordination per id.
/// NUMBERS: 41 bits of ms ~ 69 years; 10 bits = 1024 nodes; 12 bits = 4096 ids/ms/node ~ 4 M/s/node.
/// API: next_id(). DATA: (last_ms, sequence) per generator; node id assigned at boot (must be unique: lease it).
/// CORE: [0][41 ms][10 node][12 seq]. Sequence overflow waits for the next millisecond. A clock that moved BACKWARDS is refused.
/// FAILURES: NTP step backwards; two processes with the same node id; restart within the same ms (persist last_ms or wait).
/// TRADE-OFFS: time order helps B-tree inserts but makes a hot right-most page and leaks creation time and volume.
/// REALLY TESTING: clock assumptions. WRONG: "wall clocks only move forward". Ids are NOT totally ordered by creation across nodes.
/// FOLLOW-UP: compare UUIDv4, UUIDv7, database sequences with block allocation.
#[derive(Debug, PartialEq)]
enum IdError {
    ClockBackwards,
    SequenceExhausted,
}
struct Snowflake {
    node: u64,
    last_ms: u64,
    seq: u64,
}
impl Snowflake {
    fn next(&mut self, now_ms: u64) -> Result<u64, IdError> {
        if now_ms < self.last_ms {
            return Err(IdError::ClockBackwards);
        }
        if now_ms == self.last_ms {
            if self.seq == 4095 {
                return Err(IdError::SequenceExhausted); // the caller waits for the next millisecond
            }
            self.seq += 1;
        } else {
            (self.last_ms, self.seq) = (now_ms, 0);
        }
        assert!(now_ms < 1 << 41 && self.node < 1024);
        Ok(now_ms << 22 | self.node << 12 | self.seq)
    }
}
fn q12_snowflake() {
    let mut g = Snowflake {
        node: 5,
        last_ms: 0,
        seq: 0,
    };
    let first: Vec<u64> = (0..4096).map(|_| g.next(1000).unwrap()).collect();
    assert!(first.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(g.next(1000), Err(IdError::SequenceExhausted));
    assert!(g.next(1001).unwrap() > first[4095]);
    assert_eq!(g.next(900), Err(IdError::ClockBackwards));
    // 8 threads share 4 generators; a shared virtual clock advances only when a generator is exhausted
    let clock = Arc::new(std::sync::atomic::AtomicU64::new(1));
    let gens: Arc<Vec<Mutex<Snowflake>>> = Arc::new(
        (0..4)
            .map(|n| {
                Mutex::new(Snowflake {
                    node: n,
                    last_ms: 0,
                    seq: 0,
                })
            })
            .collect(),
    );
    let hs: Vec<_> = (0..8)
        .map(|t| {
            let (clock, gens) = (clock.clone(), gens.clone());
            thread::spawn(move || {
                let mut out = Vec::with_capacity(20_000);
                while out.len() < 20_000 {
                    let mut g = gens[t % 4].lock().unwrap();
                    // read the clock while holding the lock, so one generator never sees it go backwards
                    let now = clock.load(std::sync::atomic::Ordering::SeqCst);
                    match g.next(now) {
                        Ok(id) => out.push(id),
                        Err(IdError::SequenceExhausted) => {
                            clock.fetch_max(now + 1, std::sync::atomic::Ordering::SeqCst);
                        }
                        Err(e) => panic!("{e:?}"),
                    }
                }
                assert!(
                    out.windows(2).all(|w| w[0] < w[1]),
                    "each caller sees increasing ids"
                );
                out
            })
        })
        .collect();
    let all: Vec<u64> = hs.into_iter().flat_map(|h| h.join().unwrap()).collect();
    assert_eq!(all.iter().collect::<HashSet<_>>().len(), 160_000);
    println!("   160000 ids from 8 threads on 4 nodes: all unique, increasing per caller");
}

// ---------------------------------------------------------------------------
// Q13. top-K heavy hitters
// ---------------------------------------------------------------------------
/// REQUIREMENTS: the ~K most frequent items in a huge stream, bounded memory, approximate is acceptable.
/// NUMBERS: 1e9 events/day over 1e8 distinct items: exact counts ~ 1e8 x 50 B = 5 GB; a 4 x 65536 sketch of u64 = 2 MB.
/// API: add(item); top(k). DATA: Count-Min sketch (d rows x w counters) + a set of at most K candidates ordered by estimate.
/// CORE: add increments one counter per row; the estimate is the MINIMUM. It never undercounts, and overcounts by at most eN/w
///   with probability >= 1 - e^-d. Keep the K items with the highest estimates in an ordered set.
/// FAILURES: sliding windows need a sketch per time bucket (sketches add cell by cell); hash flooding (seed the hashes).
/// TRADE-OFFS: rare items are badly overestimated; only the heavy ones are trustworthy.
/// REALLY TESTING: do you know the direction and size of the error? WRONG: a size-K heap of exact counts that forgets evicted items.
/// FOLLOW-UP: Space-Saving / Misra-Gries give deterministic guarantees with a different structure.
struct TopK {
    rows: Vec<Vec<u64>>,
    best: BTreeSet<(u64, u64)>, // (estimate, item)
    estimate_of: HashMap<u64, u64>,
    k: usize,
}
impl TopK {
    fn new(depth: usize, width: usize, k: usize) -> Self {
        TopK {
            rows: vec![vec![0; width]; depth],
            best: BTreeSet::new(),
            estimate_of: HashMap::new(),
            k,
        }
    }
    fn cell(&self, row: usize, item: u64) -> usize {
        (mix(item ^ mix(row as u64 + 1)) % self.rows[row].len() as u64) as usize
    }
    fn estimate(&self, item: u64) -> u64 {
        (0..self.rows.len())
            .map(|d| self.rows[d][self.cell(d, item)])
            .min()
            .unwrap()
    }
    fn add(&mut self, item: u64) {
        for d in 0..self.rows.len() {
            let c = self.cell(d, item);
            self.rows[d][c] += 1;
        }
        let est = self.estimate(item);
        if let Some(old) = self.estimate_of.insert(item, est) {
            self.best.remove(&(old, item));
        }
        self.best.insert((est, item));
        if self.best.len() > self.k {
            let (_, evicted) = self.best.pop_first().unwrap();
            self.estimate_of.remove(&evicted);
        }
    }
}
fn q13_topk() {
    let mut r = Rng(13);
    // Zipf-like stream: item i has weight 1/(i+1)
    let cumulative: Vec<f64> = (0..20_000)
        .scan(0.0, |s, i| {
            *s += 1.0 / f64::from(i + 1);
            Some(*s)
        })
        .collect();
    let (mut sketch, mut exact) = (TopK::new(4, 2048, 20), HashMap::<u64, u64>::new());
    let n = 200_000u64;
    for _ in 0..n {
        let x = (r.next() as f64 / u64::MAX as f64) * cumulative[cumulative.len() - 1];
        let item = cumulative.partition_point(|&c| c < x) as u64;
        sketch.add(item);
        *exact.entry(item).or_default() += 1;
    }
    let bound = (std::f64::consts::E * n as f64 / 2048.0) as u64;
    let mut over = 0;
    for (&item, &c) in &exact {
        let est = sketch.estimate(item);
        assert!(est >= c, "Count-Min never undercounts");
        over += usize::from(est - c > bound);
    }
    assert!(
        (over as f64) < exact.len() as f64 * 0.02,
        "theory: at most e^-4 = 1.8% of items beyond the bound, got {over}"
    );
    let mut truth: Vec<(Reverse<u64>, u64)> =
        exact.iter().map(|(&i, &c)| (Reverse(c), i)).collect();
    truth.sort_unstable();
    let hits = truth
        .iter()
        .take(10)
        .filter(|(_, i)| sketch.estimate_of.contains_key(i))
        .count();
    assert_eq!(
        hits, 10,
        "all true top-10 items are among the 20 candidates"
    );
    assert!(sketch.estimate_of.len() == 20 && sketch.best.len() == 20);
    println!(
        "   200000 events, {} distinct: top-10 recall 10/10, {over} items beyond the eN/w bound of {bound}",
        exact.len()
    );
}

// ---------------------------------------------------------------------------
// Q14. log aggregation
// ---------------------------------------------------------------------------
/// REQUIREMENTS: ship logs from many hosts to storage; a storage outage must not exhaust host memory or stall the application.
/// NUMBERS: 5000 hosts x 200 lines/s x 300 B = 300 MB/s ~ 26 TB/day raw; ~10x compression; buffer 1 minute of an outage per host = 3.6 MB.
/// API: agent.log(line) (never blocks); sink.write(batch_id, lines). DATA: bounded buffer; batches (host, sequence); partitions by (service, hour).
/// CORE: bounded queue with a counted drop policy; batch by size or age; retry the SAME batch with the SAME id so the sink can dedupe.
/// FAILURES: sink down (buffer then drop, and report drops IN the log stream); lost ack (dedupe by batch id); agent crash (tail files from a saved offset).
/// TRADE-OFFS: block (no loss, app latency tied to logging) versus drop (app safe, gaps).
/// REALLY TESTING: backpressure as an explicit decision. WRONG: unbounded channel.
/// FOLLOW-UP: severity-aware dropping; the index cost versus grep over compressed chunks.
#[derive(Default)]
struct Agent {
    buffer: VecDeque<u64>,
    in_flight: Option<(u64, Vec<u64>)>,
    next_batch: u64,
    dropped: u64,
    oldest_since: u64,
}
impl Agent {
    const CAP: usize = 200;
    const BATCH: usize = 50;
    const MAX_AGE: u64 = 10;
    fn log(&mut self, line: u64, now: u64) {
        if self.buffer.len() == Self::CAP {
            self.dropped += 1; // drop the NEWEST: what is buffered usually explains how the trouble started
            return;
        }
        if self.buffer.is_empty() {
            self.oldest_since = now;
        }
        self.buffer.push_back(line);
    }
    fn flush(&mut self, now: u64, sink: &mut dyn FnMut(u64, &[u64]) -> bool) {
        let due = self.buffer.len() >= Self::BATCH
            || (!self.buffer.is_empty() && now >= self.oldest_since + Self::MAX_AGE);
        if self.in_flight.is_none() && due {
            let take = self.buffer.len().min(Self::BATCH);
            self.next_batch += 1;
            self.in_flight = Some((self.next_batch, self.buffer.drain(..take).collect()));
            self.oldest_since = now;
        }
        if let Some((id, lines)) = &self.in_flight {
            if sink(*id, lines) {
                self.in_flight = None;
            }
        }
    }
}
fn q14_logs() {
    let mut r = Rng(14);
    let mut agent = Agent::default();
    let (mut stored, mut seen_batches, mut redundant) = (Vec::new(), HashSet::new(), 0);
    let mut produced = 0u64;
    for now in 0..6000u64 {
        let producing = now < 5000;
        if producing {
            for _ in 0..r.below(12) {
                agent.log(produced, now);
                produced += 1;
            }
        }
        let outage = (1000..1400).contains(&now);
        agent.flush(now, &mut |id, lines| {
            if outage || r.below(10) == 0 {
                return false;
            }
            if seen_batches.insert(id) {
                stored.extend_from_slice(lines);
            } else {
                redundant += 1;
            }
            r.below(8) != 0 // sometimes the write succeeds but the ack is lost
        });
        let buffered = agent.buffer.len()
            + agent.in_flight.as_ref().map_or(0, |(id, l)| {
                if seen_batches.contains(id) {
                    0
                } else {
                    l.len()
                }
            });
        assert_eq!(
            produced,
            stored.len() as u64 + agent.dropped + buffered as u64,
            "every line is stored, dropped (and counted) or still held"
        );
        assert!(agent.buffer.len() <= Agent::CAP);
    }
    assert!(agent.buffer.is_empty() && agent.in_flight.is_none());
    assert!(
        stored.windows(2).all(|w| w[0] < w[1]),
        "order kept, no duplicates"
    );
    assert!(agent.dropped > 0 && redundant > 0);
    println!(
        "   {produced} lines: {} stored, {} dropped during the outage and counted, {redundant} retried batches deduplicated",
        stored.len(),
        agent.dropped
    );
}

// ---------------------------------------------------------------------------
// Q15. feature flags
// ---------------------------------------------------------------------------
/// REQUIREMENTS: evaluate locally in microseconds; same user keeps the same answer; ramp 1% -> 100%; kill switch propagates in seconds.
/// NUMBERS: 500 flags x 1 KB = 500 KB snapshot per SDK; 1 M evaluations/s cannot be remote calls: push or poll snapshots.
/// API: is_on(flag, user) -> bool. DATA: snapshot(version, flags{ enabled, allow list, percent in basis points }).
/// CORE: bucket = hash(flag, user) mod 10000; on iff bucket < basis_points. Raising the percentage only ADDS users. Including the
///   flag name makes different flags choose different users. SDKs ignore snapshots older than the one they hold.
/// FAILURES: control plane down (serve the last snapshot; ship a default in code); stale SDKs disagree for a while.
/// TRADE-OFFS: local evaluation exposes rules to the client; evaluate server side for secrets.
/// REALLY TESTING: sticky, monotonic bucketing. WRONG: random() per request.
/// FOLLOW-UP: flag debt (expiry and owners), audit log, experiment exposure logging.
#[derive(Clone, Default)]
struct Flag {
    killed: bool,
    allow: HashSet<u64>,
    basis_points: u64,
}
#[derive(Default)]
struct Flags {
    version: u64,
    flags: HashMap<String, Flag>,
}
impl Flags {
    fn apply(&mut self, version: u64, flags: HashMap<String, Flag>) -> bool {
        let newer = version > self.version;
        if newer {
            (self.version, self.flags) = (version, flags);
        }
        newer
    }
    fn is_on(&self, name: &str, user: u64) -> bool {
        self.flags.get(name).is_some_and(|f| {
            !f.killed
                && (f.allow.contains(&user)
                    || hash(&format!("{name}:{user}")) % 10_000 < f.basis_points)
        })
    }
}
fn q15_flags() {
    let snapshot = |bp: u64, killed: bool| {
        let f = Flag {
            killed,
            allow: HashSet::from([7]),
            basis_points: bp,
        };
        HashMap::from([
            ("new-ui".to_string(), f.clone()),
            ("new-api".to_string(), f),
        ])
    };
    let mut sdk = Flags::default();
    let mut previous = HashSet::new();
    for (v, bp) in [(1, 0), (2, 100), (3, 1000), (4, 5000), (5, 10_000)] {
        assert!(sdk.apply(v, snapshot(bp, false)));
        let on: HashSet<u64> = (0..50_000).filter(|&u| sdk.is_on("new-ui", u)).collect();
        assert!(previous.is_subset(&on), "ramping up never removes a user");
        assert!(on.contains(&7));
        let share = on.len() as f64 / 50_000.0;
        assert!(
            (share - bp as f64 / 10_000.0).abs() < 0.01,
            "{bp} bp gave {share}"
        );
        previous = on;
    }
    sdk.apply(6, snapshot(1000, false));
    let ui: HashSet<u64> = (0..50_000).filter(|&u| sdk.is_on("new-ui", u)).collect();
    let both = ui.iter().filter(|&&u| sdk.is_on("new-api", u)).count() as f64 / ui.len() as f64;
    assert!(
        both < 0.2,
        "two 10% flags overlap on ~10% of each other's users, not 100%: {both}"
    );
    sdk.apply(7, snapshot(10_000, true));
    assert!(
        !sdk.is_on("new-ui", 7),
        "the kill switch beats the allow list"
    );
    assert!(
        !sdk.apply(6, snapshot(10_000, false)),
        "a delayed older snapshot is ignored"
    );
    assert!(!sdk.is_on("new-ui", 1) && !sdk.is_on("unknown", 1));
    println!(
        "   ramp 0 -> 100% was monotonic; overlap of two independent 10% flags: {:.1}%",
        both * 100.0
    );
}

// ---------------------------------------------------------------------------
// Q16. search index
// ---------------------------------------------------------------------------
/// REQUIREMENTS: AND queries, exact phrases, ranked top-k, document deletion.
/// NUMBERS: 100 M docs x 500 tokens = 5e10 postings; at ~2 bytes each compressed ~ 100 GB + positions -> shard by document.
/// API: add(doc); delete(id); search(terms). DATA: term -> postings sorted by doc id, each with positions; doc lengths; tombstones.
/// CORE: ids only grow, so postings stay sorted by appending. AND intersects from the RAREST term. A phrase needs positions
///   p, p+1, ... in the same document. BM25 scores by rarity (idf) and saturating term frequency normalised by length.
/// FAILURES: a shard is slow (deadline, return partial results and say so); deletes pile up (merge segments to drop tombstones).
/// TRADE-OFFS: document sharding queries every shard but indexes simply; term sharding is the opposite.
/// REALLY TESTING: the inverted index and intersection cost. WRONG: LIKE '%term%'.
/// FOLLOW-UP: compression (delta + varint), skip pointers, near-real-time segments.
#[derive(Default)]
struct Index {
    postings: HashMap<String, Vec<(usize, Vec<u32>)>>,
    docs: Vec<Vec<String>>,
    deleted: HashSet<usize>,
}
impl Index {
    fn add(&mut self, text: &str) {
        let id = self.docs.len();
        let tokens: Vec<String> = text.split_whitespace().map(str::to_lowercase).collect();
        for (pos, t) in tokens.iter().enumerate() {
            let list = self.postings.entry(t.clone()).or_default();
            if list.last().is_none_or(|p| p.0 != id) {
                list.push((id, vec![]));
            }
            list.last_mut().unwrap().1.push(pos as u32);
        }
        self.docs.push(tokens);
    }
    fn positions(&self, term: &str, doc: usize) -> Option<&[u32]> {
        let list = self.postings.get(term)?;
        list.binary_search_by_key(&doc, |p| p.0)
            .ok()
            .map(|i| list[i].1.as_slice())
    }
    fn and(&self, terms: &[&str]) -> Vec<usize> {
        let Some(rarest) = terms
            .iter()
            .min_by_key(|t| self.postings.get(**t).map_or(0, Vec::len))
        else {
            return vec![];
        };
        let candidates = self
            .postings
            .get(*rarest)
            .into_iter()
            .flatten()
            .map(|p| p.0);
        candidates
            .filter(|&d| {
                !self.deleted.contains(&d) && terms.iter().all(|t| self.positions(t, d).is_some())
            })
            .collect()
    }
    fn phrase(&self, terms: &[&str]) -> Vec<usize> {
        let mut docs = self.and(terms);
        docs.retain(|&d| {
            self.positions(terms[0], d).unwrap().iter().any(|&start| {
                terms.iter().enumerate().all(|(i, t)| {
                    self.positions(t, d)
                        .unwrap()
                        .binary_search(&(start + i as u32))
                        .is_ok()
                })
            })
        });
        docs
    }
    /// BM25 (k1 = 1.2, b = 0.75) over live documents containing any query term.
    fn ranked(&self, terms: &[&str], k: usize) -> Vec<(usize, f64)> {
        let live = (self.docs.len() - self.deleted.len()) as f64;
        let total_len: usize = (0..self.docs.len())
            .filter(|d| !self.deleted.contains(d))
            .map(|d| self.docs[d].len())
            .sum();
        let average = total_len as f64 / live.max(1.0);
        let mut score = BTreeMap::<usize, f64>::new();
        for t in terms {
            let list: Vec<_> = self
                .postings
                .get(*t)
                .into_iter()
                .flatten()
                .filter(|p| !self.deleted.contains(&p.0))
                .collect();
            let idf = (1.0 + (live - list.len() as f64 + 0.5) / (list.len() as f64 + 0.5)).ln();
            for (d, pos) in list {
                let tf = pos.len() as f64;
                *score.entry(*d).or_default() += idf * tf * 2.2
                    / (tf + 1.2 * (0.25 + 0.75 * self.docs[*d].len() as f64 / average));
            }
        }
        let mut out: Vec<_> = score.into_iter().collect();
        out.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        out.truncate(k);
        out
    }
}
fn q16_search() {
    let mut r = Rng(16);
    let vocab = [
        "the", "rust", "queue", "lock", "log", "cache", "ring", "lease",
    ];
    let mut ix = Index::default();
    for _ in 0..600 {
        let n = 2 + r.below(14);
        let text: Vec<&str> = (0..n)
            .map(|_| {
                let rare_to_common = 1 + r.below(8); // skewed: early words are more frequent
                vocab[r.below(rare_to_common) as usize]
            })
            .collect();
        ix.add(&text.join(" "));
    }
    for _ in 0..100 {
        ix.deleted.insert(r.below(600) as usize);
    }
    let mut phrase_hits = 0;
    for _ in 0..400 {
        let terms: Vec<&str> = (0..1 + r.below(3))
            .map(|_| vocab[r.below(8) as usize])
            .collect();
        let live = || (0..ix.docs.len()).filter(|d| !ix.deleted.contains(d));
        let want_and: Vec<usize> = live()
            .filter(|&d| terms.iter().all(|t| ix.docs[d].iter().any(|x| x == t)))
            .collect();
        assert_eq!(ix.and(&terms), want_and);
        let want_phrase: Vec<usize> = live()
            .filter(|&d| {
                ix.docs[d]
                    .windows(terms.len())
                    .any(|w| w.iter().map(String::as_str).eq(terms.iter().copied()))
            })
            .collect();
        assert_eq!(ix.phrase(&terms), want_phrase, "phrase {terms:?}");
        phrase_hits += want_phrase.len();
    }
    assert!(phrase_hits > 0);
    let mut small = Index::default();
    [
        "lease lease cache",
        "lease cache cache cache cache cache cache",
        "cache log",
        "cache queue",
    ]
    .iter()
    .for_each(|d| small.add(d));
    let top = small.ranked(&["lease", "cache"], 4);
    assert_eq!(
        top[0].0, 0,
        "the short document with the rare term twice ranks first: {top:?}"
    );
    assert!(top.windows(2).all(|w| w[0].1 >= w[1].1) && top.len() == 4);
    assert!(small.ranked(&["absent"], 3).is_empty() && small.and(&[]).is_empty());
    println!(
        "   400 random AND and phrase queries matched a full scan ({phrase_hits} phrase hits); BM25 ordering checked"
    );
}

fn main() {
    let questions: [(&str, fn()); 16] = [
        ("Q1 URL shortener", q1_url_shortener),
        ("Q2 rate limiter", q2_rate_limiter),
        ("Q3 distributed cache", q3_cache),
        ("Q4 KV store + WAL", q4_wal),
        ("Q5 at-least-once queue", q5_queue),
        ("Q6 notifications", q6_notifications),
        ("Q7 typeahead", q7_typeahead),
        ("Q8 crawler frontier", q8_frontier),
        ("Q9 pub/sub", q9_pubsub),
        ("Q10 job scheduler", q10_scheduler),
        ("Q11 leader lease + fencing", q11_lease),
        ("Q12 Snowflake ids", q12_snowflake),
        ("Q13 top-K heavy hitters", q13_topk),
        ("Q14 log aggregation", q14_logs),
        ("Q15 feature flags", q15_flags),
        ("Q16 search index", q16_search),
    ];
    for (name, run) in questions {
        println!("{name}");
        run();
    }
    println!("all 16 design cores verified");
}
