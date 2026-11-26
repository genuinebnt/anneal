# Chapter `data_structures`: suggested reading order

Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. Cross-chapter order is in [the tracks](../tracks/).

| # | Recipe | Level | What it is |
|---|---|---|---|
| 1 | [`heap_tiebreak`](../../examples/data_structures/heap_tiebreak.rs) | simple | BinaryHeap as a min-heap with a deterministic tie-break. |
| 2 | [`count_min_sketch`](../../examples/data_structures/count_min_sketch.rs) | intermediate | Count-Min Sketch + heavy hitters (top-k) in sublinear memory. |
| 3 | [`hash_functions`](../../examples/data_structures/hash_functions.rs) | intermediate | hash functions from scratch (FNV-1a, Fx, fmix64, a wyhash-style mixer) and how to MEASURE their quality. |
| 4 | [`heap_patterns`](../../examples/data_structures/heap_patterns.rs) | intermediate | heaps from scratch (binary, d-ary, indexed) and the classic heap patterns. |
| 5 | [`ring_buffer_zoo`](../../examples/data_structures/ring_buffer_zoo.rs) | intermediate | a ring buffer zoo: fixed ring, growable deque, flight recorder, byte ring, sliding-window stats |
| 6 | [`sampling`](../../examples/data_structures/sampling.rs) | intermediate | sampling: reservoir (Algorithm R), weighted (A-Res), shuffles, replacement. |
| 7 | [`segment_fenwick`](../../examples/data_structures/segment_fenwick.rs) | intermediate | Fenwick tree, monoid segment tree, lazy range-add segment tree, sparse table |
| 8 | [`trie_radix`](../../examples/data_structures/trie_radix.rs) | intermediate | a trie and a compressed radix tree (Patricia) over byte strings, arena-based |
| 9 | [`adaptive_radix_tree`](../../examples/data_structures/adaptive_radix_tree.rs) | advanced | the Adaptive Radix Tree (ART): Node4/16/48/256, path compression, lazy leaf expansion, ordered scans. |
| 10 | [`b_epsilon_tree`](../../examples/data_structures/b_epsilon_tree.rs) | advanced | the B^epsilon-tree: a B-tree whose internal nodes carry a MESSAGE BUFFER, trading a little read cost for far fewer writes. |
| 11 | [`bdd_zdd`](../../examples/data_structures/bdd_zdd.rs) | advanced | reduced ordered BDDs (Bryant) and ZDDs (Minato): canonical boolean functions and families of sets. |
| 12 | [`bitset_rank_select_roaring`](../../examples/data_structures/bitset_rank_select_roaring.rs) | advanced | bitsets, rank/select, and Roaring-style compressed bitmaps. |
| 13 | [`bplus_tree`](../../examples/data_structures/bplus_tree.rs) | advanced | an in-memory B+ tree map with leaf chain, split, borrow and merge, arena-based |
| 14 | [`btree_bulk_load_prefix_compression`](../../examples/data_structures/btree_bulk_load_prefix_compression.rs) | advanced | production B+tree features: bulk loading, prefix compression, suffix truncation, duplicates, split policy. |
| 15 | [`bw_tree_latch_free`](../../examples/data_structures/bw_tree_latch_free.rs) | advanced | a Bw-tree style latch-free B+tree: mapping table, delta chains, consolidation, split deltas, epochs. |
| 16 | [`cache_policy_zoo`](../../examples/data_structures/cache_policy_zoo.rs) | advanced | a cache replacement policy zoo: FIFO, LRU, LFU, CLOCK, 2Q, ARC, SIEVE, W-TinyLFU-lite and Belady's OPT |
| 17 | [`cuckoo_hashing`](../../examples/data_structures/cuckoo_hashing.rs) | advanced | cuckoo hashing: a two-table cuckoo hash map AND a cuckoo filter (approximate membership with delete). |
| 18 | [`gist_generalized_search_tree`](../../examples/data_structures/gist_generalized_search_tree.rs) | advanced | GiST, the Generalized Search Tree: ONE balanced tree, two index types (B-tree-like ranges and an R-tree). |
| 19 | [`hyperloglog`](../../examples/data_structures/hyperloglog.rs) | advanced | HyperLogLog cardinality estimation from scratch. |
| 20 | [`learned_index_rmi`](../../examples/data_structures/learned_index_rmi.rs) | advanced | learned indexes: a two-stage Recursive Model Index (RMI), a PGM-style piecewise-linear index with a hard epsilon, and an updatable PGM with a delta buffer. |
| 21 | [`lru_cache`](../../examples/data_structures/lru_cache.rs) | advanced | an LRU cache: HashMap + a doubly linked list stored in a Vec with INDICES (no unsafe, no Rc<RefCell>). |
| 22 | [`minhash_simhash_lsh`](../../examples/data_structures/minhash_simhash_lsh.rs) | advanced | MinHash + banded LSH + SimHash for near-duplicate detection. |
| 23 | [`open_addressing_map`](../../examples/data_structures/open_addressing_map.rs) | advanced | an open-addressing hash map with linear probing, Robin Hood displacement and backward-shift deletion. |
| 24 | [`optimistic_lock_coupling_btree`](../../examples/data_structures/optimistic_lock_coupling_btree.rs) | advanced | B+tree concurrency control: latch crabbing, optimistic crabbing, and optimistic lock coupling (OLC). |
| 25 | [`priority_queue_zoo`](../../examples/data_structures/priority_queue_zoo.rs) | advanced | a priority queue zoo: pairing heap, leftist heap, indexed 4-ary heap, Dial's bucket queue, radix heap |
| 26 | [`skiplist`](../../examples/data_structures/skiplist.rs) | advanced | skip list: a probabilistically balanced ordered map (arena, no unsafe). |
| 27 | [`treap_interval`](../../examples/data_structures/treap_interval.rs) | advanced | a split/merge treap with pluggable augmentation: order statistics and an interval tree |
