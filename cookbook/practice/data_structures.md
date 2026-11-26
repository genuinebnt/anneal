# Practice: data structures

How to use this page: run the recipe, read its `main()` and `check_invariants`/`check` functions, then build your own version from a blank file using only the recipe's header as a spec.
Every structure here deserves two things you write yourself: an invariant checker (walks the whole structure and asserts its rules) and a model oracle (a std collection that receives the same operations; compare after every op).
Order: [S] about 30 minutes, [M] about 2 hours, [A] a weekend evening or more. No solutions are given; decide the oracle before the code.

Model-based test recipe used below: seeded xorshift generates 10^5 random operations (insert/get/remove/range), apply them to your structure and to `BTreeMap`/`HashMap`/`Vec`, compare outputs per op, call `check_invariants()` every 100 ops, print the seed on failure.

---

## adaptive_radix_tree

**Prove you got it**
1. Node4 holds up to 4 children, Node16 up to 16, Node48 up to 48, Node256 all. At which insert does each growth happen, and what is the "shrink" threshold on remove? Confirm with the `Census` output.
2. Path compression stores a prefix in the node. What must `insert` do when a new key diverges inside that prefix?

**Try next**
- [S] Add `longest_prefix_match(key)`; oracle: scan all stored keys.
- [M] Add range scans `[lo, hi)` returning sorted entries; oracle: `BTreeMap::range` over the same keys, including i64 and f64 keys through the order-preserving encodings (`enc_i64`, `enc_f64`, check negative zero and NaN policy).
- [A] Load 10^6 URLs or 10^7 random u64s, measure bytes per key and lookup ns against `BTreeMap` and `HashMap`; read [congee](https://github.com/XiangpengHao/congee) (an ART with concurrency) and list three differences from your recipe.

**Use in**: in-memory database primary-key indexes and string dictionaries.

**Pitfall**: encoding signed integers by their raw big-endian bytes, which sorts negatives after positives.

---

## b_epsilon_tree

**Prove you got it**
1. A message is appended to the root buffer and later flushed. Explain why a point `get` must look in every buffer on the root-to-leaf path and which message wins when two disagree.
2. What is the expected write-amplification difference between this and a B+ tree in `cost_table`, and which assumption (page size, buffer fraction) drives it?

**Try next**
- [S] Add a counter for messages pushed per level and print the distribution after 10^5 inserts.
- [M] Add range delete messages; oracle: `BTreeMap` with a retain.
- [A] Implement on-disk pages (fixed 4 KiB, serialise nodes) and count bytes written per update for B+ tree versus B^e tree on 10^6 random inserts. Compare to the design in [PerconaFT](https://github.com/percona/PerconaFT).

**Use in**: write-heavy ordered stores (file systems, TokuDB).

**Pitfall**: applying an `Upsert` message out of order with a later `Put`; messages must carry sequence numbers or retain order on flush.

---

## bdd_zdd

**Prove you got it**
1. Why do the two reduction rules (no node with lo == hi, unique table) make equal functions the same node id, and so make equivalence `==` on ids?
2. For `satcount`, why must you account for skipped variables?

**Try next**
- [S] Build the BDD of x1 xor x2 xor ... xor x20 and compare node count under variable order natural versus interleaved for a 2-adder.
- [M] Add `compose` (substitute a function for a variable); oracle: truth table evaluation for n <= 10.
- [A] Verify that two different 8-bit adder designs (ripple-carry and carry-lookahead) are equivalent via BDDs, and show how bad ordering blows up multiplier BDDs (measure nodes for 4,5,6 bit multiplier). Compare with [OxiDD](https://github.com/OxiDD/oxidd).

**Use in**: hardware equivalence checking, feature-model counting, solving puzzles with ZDDs.

**Pitfall**: forgetting that variable order is part of the identity; two BDDs under different orders are incomparable.

---

## bitset_rank_select_roaring

**Prove you got it**
1. Rank is O(1) with sampling every 8 words. What is the worst-case number of popcounts and what does it cost in extra memory (percent)?
2. Roaring switches container kinds at 4096 elements. Derive that threshold from the byte sizes.

**Try next**
- [S] Add `iter_ones_rev` and `next_set_bit(i)`; oracle: `Vec<bool>` scan.
- [M] Add run-length containers and a conversion rule; assert `to_bitmap(from_bitmap(c)) == c` over 10^4 random sets with varied density.
- [A] Build an inverted index over a corpus of 10^5 documents using your roaring and `Vec<u32>` postings; benchmark AND of two lists at densities 0.1%, 1%, 10%. Cross-check with [roaring-rs](https://github.com/RoaringBitmap/roaring-rs); see also [sucds](https://github.com/kampersanda/sucds) for succinct structures.

**Use in**: search posting lists and bitmap indexes.

**Pitfall**: off-by-one on `rank(i)` (exclusive vs inclusive); pin the definition in a doc test.

---

## bplus_tree

**Prove you got it**
1. After a delete causes underflow, when do you borrow and when do you merge? State the min-keys rule (`min_keys`) and which siblings you consider first.
2. Why are leaves linked, and what do `range` scans save?

**Try next**
- [S] Add `first_key_value`/`last_key_value`; oracle `BTreeMap`.
- [M] Make `order` a runtime parameter and run the differential test at orders 3, 4, 5, 8, 64; tiny orders catch split bugs fast. Report which order exposed bugs.
- [A] Add a page cache and serialise nodes to a file with a fixed page size; count page reads for point lookups and range scans of 10^6 keys. Compare to std [BTreeMap](https://github.com/rust-lang/rust/blob/master/library/alloc/src/collections/btree/map.rs) (note std is a B-tree, not B+) and to [redb](https://github.com/cberner/redb).

**Use in**: database indexes.

**Pitfall**: separator keys going stale after borrow/merge; `check_invariants` must verify separators bound each subtree.

---

## btree_bulk_load_prefix_compression

**Prove you got it**
1. Bulk loading packs leaves to a fill factor. What are the cost differences between building by 10^6 inserts and bulk load (count node splits and bytes)?
2. What is the shortest-separator rule (`shortest_separator`) and why is it still a valid router?

**Try next**
- [S] Print compression ratio (`bytes_compressed / bytes_raw`) for URLs, UUIDs and random bytes.
- [M] Add duplicate keys with a posting-list overflow; oracle: `BTreeMap<K, Vec<V>>`.
- [A] Bulk load a sorted sample of 10^6 real file paths (`find /usr`) and measure scan speed against an uncompressed version; read the page-format ideas in [redb](https://github.com/cberner/redb).

**Use in**: `CREATE INDEX` builders.

**Pitfall**: separator truncation failing when keys are prefixes of each other ("a" and "ab").

---

## bw_tree_latch_free

**Prove you got it**
1. Why is a page never updated in place, and what does a compare-and-swap on the mapping table slot replace?
2. When may a retired delta chain be freed? Explain the role of epochs (`Guard`, `tick`).

**Try next**
- [S] Add a stats counter of failed CAS retries under 4 threads versus 1.
- [M] Run a linearizability-style test: 8 threads, per-thread-disjoint keys plus a final merge check; then overlapping keys with a sequential specification and `cookbook/distributed/linearizability_checker` ideas.
- [A] Compare throughput on a 90/10 read/write mix against `optimistic_lock_coupling_btree` schemes and a global `RwLock<BTreeMap>` for 1, 2, 4, 8 threads. Study [OpenBwTree](https://github.com/wangziqi2013/BwTree) for what the paper leaves out.

**Use in**: latch-free indexes for main-memory/flash engines.

**Pitfall**: reclaiming memory while another thread still holds a pointer from before the CAS. See `concurrency/epoch_hazard_pointer_reclamation`.

---

## cache_policy_zoo

**Prove you got it**
1. Why does Belady's OPT give an upper bound on hit ratio, and what does the recipe's caveat (SIEVE / TinyLFU on shifting traces) say about trusting a single trace?
2. Which policies are scan-resistant and why (2Q, ARC, TinyLFU admission)?

**Try next**
- [S] Add a trace of a sequential scan larger than the cache plus a hot set; predict each policy's hit ratio before running.
- [M] Implement LRU-K (K=2); verify against a brute-force `Vec` implementation on 10^4 ops, and show a trace where it beats LRU.
- [A] Replay a real trace (an open dataset such as an MSR Cambridge or Wikipedia CDN trace, or your own `strace`/access log) through all policies and make a table of hit ratio by cache size; relate to [Caffeine](https://github.com/ben-manes/caffeine) design and [moka](https://github.com/moka-rs/moka).

**Use in**: page caches, CDNs, memoisation caches.

**Pitfall**: benchmarking policies only on Zipf traces; they hide scan and shift behaviour.

---

## count_min_sketch

**Prove you got it**
1. Why is `estimate >= true count` always, and what is the error bound in terms of `width` and `depth`?
2. What does conservative update change and why does it keep the one-sided guarantee?

**Try next**
- [S] Plot (print) error percentiles versus width for a Zipf stream of 10^6 items.
- [M] Add decay (halve all counters every N events) and test hot-key detection when popularity shifts.
- [A] Feed a real access log (nginx or a generated one) through `TopK`; compare heavy hitters with exact counts and report precision/recall at k = 10, 100.

**Use in**: hot-key detection, rate-limiter and TinyLFU admission sketches.

**Pitfall**: using one hash function with different seeds that are correlated; rows must be independent enough (see `hash_functions`).

---

## cuckoo_hashing

**Prove you got it**
1. Why does a two-table cuckoo map stay below ~50% load, and what does the recipe do when an insert cycles?
2. What is the false-positive rate of the cuckoo filter in terms of fingerprint bits and bucket size, and why must `delete` only be called for items that were inserted?

**Try next**
- [S] Measure the load factor at first rebuild over 100 seeds.
- [M] Switch to 4-way buckets; oracle `HashMap`; report achievable load factor.
- [A] Use the filter to deduplicate 10^7 URLs from a real crawl list; report FP rate observed against theory and memory versus a `HashSet`. See [rust-cuckoofilter](https://github.com/axiomhq/rust-cuckoofilter).

**Use in**: worst-case two-probe lookups and deletable membership filters.

**Pitfall**: inserting a duplicate into a cuckoo filter twice; it stores two fingerprints and deletes only one.

---

## gist_generalized_search_tree

**Prove you got it**
1. The extension methods (`consistent`, `union`, `penalty`, `pick_split`) must satisfy what property for `search` to have no false negatives?
2. How does best-first `nearest` stay correct (what does `min_dist` have to be)?

**Try next**
- [S] Add a third extension: 1-D intervals with stabbing queries; oracle: linear scan.
- [M] Add delete with condensing and re-insertion; the `check` must still pass after 10^4 mixed ops.
- [A] Implement a trigram-set extension for substring prefilter and measure false positives; compare ideas with PostgreSQL's [GiST sources](https://github.com/postgres/postgres/tree/master/src/backend/access/gist).

**Use in**: PostGIS-style spatial indexes, range types.

**Pitfall**: a `union` that is not a true bounding key (does not contain children), silently dropping results.

---

## hash_functions

**Prove you got it**
1. Why does `SumHash` fail the avalanche test and what does the chi-square statistic say about it?
2. What does `crafted_keys` show about a hash with no seed?

**Try next**
- [S] Add a bit-bias matrix (64x64) for each hasher and print the max deviation.
- [M] Plug each hasher into `HashMap` and measure insertion of 10^6 sequential, random and strided keys; explain which hasher loses on which keys.
- [A] Implement SipHash-1-3 from the spec with test vectors, then compare to [rustc-hash](https://github.com/rust-lang/rustc-hash) and [ahash](https://github.com/tkaitchuck/aHash) for HashDoS resistance and throughput.

**Use in**: choosing a `HashMap` hasher, sharding, sketches.

**Pitfall**: using a fast non-keyed hash for attacker-controlled keys.

---

## heap_patterns

**Prove you got it**
1. Why is `heapify` O(n) rather than O(n log n)? Count sift steps empirically for n = 2^k.
2. What does the indexed heap give Dijkstra that lazy deletion does not, and what extra memory does it need?

**Try next**
- [S] Change arity d = 2, 4, 8 and report Dijkstra time on a 10^6-edge graph.
- [M] Add `top_k_stream` with a ties-and-duplicates policy; oracle: sort the full stream.
- [A] Use `k_way_merge` to merge 100 sorted files of 10^6 numbers (disk) and compare comparisons/time to a flat sort. Compare with std [BinaryHeap](https://github.com/rust-lang/rust/blob/master/library/alloc/src/collections/binary_heap/mod.rs) (read its sift-down-to-bottom trick and why it exists).

**Use in**: schedulers, shortest paths, top-k.

**Pitfall**: sift-up/down off-by-one for 0-based children (2i+1, 2i+2).

---

## heap_tiebreak

**Prove you got it**
1. Why does `Reverse((priority, sequence, name))` give FIFO among equals? What happens if you drop `sequence`?
2. Why must you never rely on `HashMap` iteration order for output?

**Try next**
- [S] Prove determinism: run the program 100 times in a shell loop and `diff` outputs.
- [M] Add priority aging (priority decreases with wait); oracle: sort all items by effective priority at each pop for 10^3 ops.
- [A] Write a small job-queue simulator with 3 classes and report starvation without aging versus with aging over 10^5 jobs.

**Use in**: deterministic schedulers. (FlowForge stage 7 uses this pattern; attempt your own stage first.)

**Pitfall**: `f64` priorities (not `Ord`); wrap in a total-order type and decide NaN policy.

---

## hyperloglog

**Prove you got it**
1. Why does the relative error go as 1.04/sqrt(m)? Verify for p = 10, 12, 14 over 100 seeded trials.
2. Why is `merge` just the register-wise max, and what does that imply for distributed counting?

**Try next**
- [S] Show the small-range correction in action: error with and without it for cardinalities 10, 100, 1000.
- [M] Implement sparse representation for small cardinalities; assert estimates equal the dense version.
- [A] Count distinct IPs in a real access log in 4 shards and merge; compare to exact count and to what [DataFusion](https://github.com/apache/datafusion) `approx_distinct` provides.

**Use in**: unique visitor counts, `COUNT(DISTINCT)`.

**Pitfall**: poor hash (see `hash_functions`) biasing leading zeros; test with sequential ids.

---

## learned_index_rmi

**Prove you got it**
1. What is the max position error epsilon of the PGM-style index and how is the last-mile search bounded by it?
2. When does the learned index lose to a plain binary search or a static B-tree (look at `Stat`)?

**Try next**
- [S] Print model size in bytes versus the sorted array and static B-tree for three key distributions.
- [M] Test on adversarial keys (clusters, huge gaps, duplicates); report error and fallback behaviour.
- [A] Evaluate on a real dataset (e.g. OSM longitudes, or Wikipedia timestamps) with 10^8 keys if memory allows and reproduce the trade-off curve; read the [PGM-index](https://github.com/gvinciguerra/PGM-index) code.

**Use in**: static SSTable or column indexes.

**Pitfall**: floating-point prediction error near 2^53 for u64 keys.

---

## lru_cache

**Prove you got it**
1. Which operations touch the list and which the map, and what does the `Model` oracle compare?
2. How does an index-based list avoid `Rc<RefCell>`, and what is the dangling-index risk?

**Try next**
- [S] Add `peek` (no recency update) and `pop_lru`.
- [M] Add per-entry weight (bytes) and capacity in bytes; oracle: naive Vec version.
- [A] Make it thread-safe in two ways (single mutex, N shards); measure with 1..8 threads; compare to [lru-rs](https://github.com/jeromefroe/lru-rs) and [moka](https://github.com/moka-rs/moka).

**Use in**: compiled-plan and parsed-file caches.

**Pitfall**: forgetting to remove the key from the map on eviction, so the map leaks stale indices.

---

## minhash_simhash_lsh

**Prove you got it**
1. What is the exact probability that two MinHash slots agree, and what does the S-curve `s_curve(bands, rows)` say about the threshold?
2. When is SimHash better than MinHash and why does Hamming distance approximate cosine?

**Try next**
- [S] Plot estimate error versus signature size 16..512.
- [M] Tune bands/rows for threshold 0.8 and report precision/recall against all-pairs Jaccard on 2,000 documents.
- [A] Dedup a real corpus (Wikipedia sample, your Markdown notes) and measure speed-up over all-pairs; compare with [datasketch](https://github.com/ekzhu/datasketch).

**Use in**: near-duplicate detection.

**Pitfall**: shingle size too small, calling everything similar.

---

## open_addressing_map

**Prove you got it**
1. After a backward-shift delete the table must satisfy what invariant (check `check_invariants`), and why is no tombstone needed?
2. What do `probe_stats` show about Robin Hood versus plain linear probing at load 0.9?

**Try next**
- [S] Add a configurable max load and measure probe length distribution at 0.5, 0.7, 0.9, 0.95.
- [M] Plug `WeakFnv` and observe degenerate clustering; add an assertion-level test that detects pathological probe lengths.
- [A] Benchmark against std `HashMap` (hashbrown, [source](https://github.com/rust-lang/hashbrown)) and explain the SIMD group probing gap.

**Use in**: interners and symbol tables.

**Pitfall**: resizing in the middle of insert and invalidating a displaced element.

---

## optimistic_lock_coupling_btree

**Prove you got it**
1. In OLC what does a reader do instead of taking a latch, and when must it restart?
2. Why do reader latches hurt scaling (cache-line bouncing) even for `RwLock` in shared mode?

**Try next**
- [S] Count restarts per 10^6 reads under a concurrent writer.
- [M] Stress with a model: each thread owns a key range, afterwards compare against merged `BTreeMap` results.
- [A] Plot throughput versus thread count for the four schemes and explain the curve; read [crossbeam-skiplist](https://github.com/crossbeam-rs/crossbeam/tree/master/crossbeam-skiplist) for a different concurrent ordered map.

**Use in**: shared ordered indexes in main-memory databases.

**Pitfall**: dereferencing a node pointer read optimistically before validating the version.

---

## priority_queue_zoo

**Prove you got it**
1. Dial's queue needs bounded integer weights; what is the bound and memory cost?
2. Why is a radix heap valid only for monotone queues?

**Try next**
- [S] Add `len` and invariant checks for all queues; compare pops with `BinaryHeap` on 10^5 random ops.
- [M] Benchmark Dijkstra with each queue on grids with weights 1..10 versus 1..10^6.
- [A] Add a bucketed timing wheel (`os_and_storage/timing_wheel` for reference) as another monotone queue and compare for a simulated event scheduler.

**Use in**: Dijkstra/Prim, event simulation.

**Pitfall**: decrease-key on an element already popped.

---

## ring_buffer_zoo

**Prove you got it**
1. Why power-of-two capacity with masks, and what breaks for `MyDeque::grow`?
2. Why is `is_full` ambiguous when head == tail, and which two fixes exist?

**Try next**
- [S] Assert `as_slices()` concatenation equals logical order after random push/pop ops.
- [M] Make the byte ring support wrap-around `read_exact` and `write_all` with partial writes; oracle `VecDeque<u8>`.
- [A] Turn the fixed ring into a lock-free SPSC queue and benchmark it; compare to `concurrency/spsc_ring_buffer` and [ringbuf](https://github.com/agerasev/ringbuf); read std [VecDeque](https://github.com/rust-lang/rust/blob/master/library/alloc/src/collections/vec_deque/mod.rs).

**Use in**: flight recorders, I/O buffers.

**Pitfall**: index arithmetic using monotonic counters overflowing; use wrapping math deliberately.

---

## sampling

**Prove you got it**
1. Prove by induction that Algorithm R keeps each item with probability k/n.
2. Why is `naive_shuffle` biased and how do you show it (count permutations of 3 or 4 elements)?

**Try next**
- [S] Chi-square test of reservoir inclusion probabilities over 10^5 trials.
- [M] Implement a weighted sample without replacement and check inclusion probabilities against a closed form for small cases.
- [A] Sample 1,000 lines from a 10 GB log stream in one pass, then verify uniformity by line-number histogram. Compare with `rand`'s [seq](https://github.com/rust-random/rand/blob/master/src/seq/mod.rs).

**Use in**: log sampling, A/B selection.

**Pitfall**: using `rng % n` (modulo bias); see `math_science/prng_zoo`.

---

## segment_fenwick

**Prove you got it**
1. Why does the Fenwick tree need an invertible operation for range queries but the segment tree does not?
2. In the lazy segment tree, what must `push` do before descending?

**Try next**
- [S] Add `kth` for Fenwick; oracle: prefix scan.
- [M] Implement range-assign + range-sum lazy (assignment tags compose differently from add); oracle: naive array.
- [A] Solve a real problem: "number of inversions in a 10^6 sequence" and "rank of each score in a leaderboard with updates"; report time versus naive O(n^2).

**Use in**: leaderboards, time-series windows.

**Pitfall**: lazy tags not composing in order (assign then add vs add then assign).

---

## skiplist

**Prove you got it**
1. What is the expected height and average tower size for p = 1/4, measured via `height`, `avg_tower`?
2. Why does `insert` need the `predecessors` array?

**Try next**
- [S] Make p configurable; measure search steps for p = 1/2, 1/4, 1/8.
- [M] Add `range` iteration with exclusive/inclusive bounds; oracle `BTreeMap::range`.
- [A] Make a concurrent version with per-level atomics or locks and compare with [crossbeam-skiplist](https://github.com/crossbeam-rs/crossbeam/tree/master/crossbeam-skiplist).

**Use in**: LSM memtables, sorted sets.

**Pitfall**: height random draw using the same low bits repeatedly (biased).

---

## treap_interval

**Prove you got it**
1. How do `split` and `merge` build `insert` and `remove`?
2. What extra field makes overlap queries possible (`MaxHi`) and when is it recomputed (`pull`)?

**Try next**
- [S] Add `lower_bound_by_rank`; oracle: sorted Vec.
- [M] Add implicit-key treap (sequence with insert-at-position, reverse range); oracle: Vec.
- [A] Build a calendar double-booking detector for 10^6 events and compare with sorting plus sweep; read [ropey](https://github.com/cessen/ropey) for a different balanced sequence design and [rust-bio's interval tree](https://github.com/rust-bio/rust-bio) for genomic overlaps.

**Use in**: calendar overlaps, rope-like editors.

**Pitfall**: forgetting to `pull` after a rotation or split, giving stale aggregates.

---

## trie_radix

**Prove you got it**
1. How many nodes does a radix tree need for n keys at most, versus a trie?
2. How does `with_prefix` return keys in sorted order without sorting?

**Try next**
- [S] Add `remove` that merges a single-child node with its child; oracle `BTreeMap`.
- [M] Add wildcard path segments for routing (`/users/:id`) with precedence rules; test against a table of 50 routes.
- [A] Build a router for 5,000 generated paths and compare lookup speed with [matchit](https://github.com/ibraheemdev/matchit), and a sorted-Vec binary search; see `interview/tries_core_questions` for the same ideas as interview questions.

**Use in**: HTTP routing, autocomplete.

**Pitfall**: splitting an edge label at a non-UTF-8 boundary when keys are text; decide bytes vs chars.

---

## Chapter projects

### Small (about 2 hours): "tiny cache lab"
Combine `lru_cache`, `cache_policy_zoo`, `count_min_sketch`, and `perf/bench_harness`.
- Milestones: (1) generate Zipf, scan, loop and shifting traces; (2) replay through LRU, LFU and a TinyLFU-admission variant; (3) print hit-ratio tables.
- Acceptance: LRU of `lru_cache` equals the zoo's LRU on 10^5 accesses; OPT is the top row in every table.
- Stretch: weighted entries; compare with [moka](https://github.com/moka-rs/moka).

### Intermediate (about a weekend): "ordered key-value store"
Combine `bplus_tree` or `skiplist`, `btree_bulk_load_prefix_compression`, `ring_buffer_zoo` (WAL buffer), and `os_and_storage/wal_append_log`.
- Milestones: (1) memtable (skip list) + WAL; (2) flush to sorted file with prefix compression; (3) point and range reads across memtable and file; (4) crash recovery by replaying WAL.
- Acceptance: model test against `BTreeMap` with random crashes injected (kill after k ops) loses no acknowledged write.
- Stretch: bloom filter per file (`search_vectors/bloom_filter`), compaction (`database/lsm_compaction_strategies`).

### Advanced (about a week): "search index"
Combine `bitset_rank_select_roaring`, `trie_radix` or `adaptive_radix_tree` (term dictionary), `hyperloglog` (distinct terms), `minhash_simhash_lsh` (duplicates) and `search_vectors/trigram_index` plus `text_and_parsing/zero_copy_lexer`.
- Milestones: (1) tokenise and build inverted index; (2) roaring postings with AND/OR/NOT queries; (3) BM25 ranking; (4) near-duplicate pruning; (5) on-disk format.
- Acceptance: every query result equals a brute-force scan on a 10^4-document corpus; index size under 30% of raw text.
- Stretch: compare with [fst](https://github.com/BurntSushi/fst)-based dictionaries; measure query latency percentiles.
