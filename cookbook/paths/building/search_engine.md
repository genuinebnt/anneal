# Building: A search engine or autocomplete



Use this when you have a real project in front of you. For each stage: **attempt your design first**, then read the listed recipes to compare, steal what is better and note what you missed.

### Step 1: Indexing

**Goal:** Postings and tokens.

| # | Recipe | Why here |
|---|---|---|
| 1.1 | [`trie_radix`](../../examples/data_structures/trie_radix.rs) · intermediate | Prefix structures. |
| 1.2 | [`trigram_index`](../../examples/search_vectors/trigram_index.rs) · advanced | Substring. |
| 1.3 | [`bitset_rank_select_roaring`](../../examples/data_structures/bitset_rank_select_roaring.rs) · advanced | Compressed postings. |
| 1.4 | [`encodings_codecs`](../../examples/encoding_compression/encodings_codecs.rs) · intermediate | Varint and delta. |

### Step 2: Ranking and fuzzy

**Goal:** Order results.

| # | Recipe | Why here |
|---|---|---|
| 2.1 | [`heap_patterns`](../../examples/data_structures/heap_patterns.rs) · intermediate | Top-k. |
| 2.2 | [`autocomplete_engine`](../../examples/search_vectors/autocomplete_engine.rs) · advanced | Completion. |
| 2.3 | [`string_algos`](../../examples/algorithms/string_algos.rs) · advanced | Edit distance/automata. |
| 2.4 | [`search_engine`](../../examples/capstones/search_engine.rs) · capstone | Capstone. |
| 2.5 | [`typeahead_service`](../../examples/capstones/typeahead_service.rs) · capstone | Service version. |

### Step 3: Scale

**Goal:** Many shards.

| # | Recipe | Why here |
|---|---|---|
| 3.1 | [`consistent_hashing`](../../examples/distributed/consistent_hashing.rs) · intermediate | Shard. |
| 3.2 | [`bloom_filter`](../../examples/search_vectors/bloom_filter.rs) · intermediate | Skip shards. |
| 3.3 | [`sharded_map`](../../examples/concurrency/sharded_map.rs) · intermediate | Concurrent updates. |
