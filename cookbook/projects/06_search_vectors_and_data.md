# 06. Search, vectors and data

Prerequisite tracks: `paths/tracks/10_search_vectors.md`, `04_data_structures_algorithms.md`. Recipe stems are `folder/stem` under `examples/`. 🔜 = planned, not written yet.

Test strategy: **brute force is the oracle.** Every index returns results you can compare against a linear scan; for approximate structures measure **recall@k** against exact results and **latency percentiles**, not just correctness. Always fix the seed of the data generator.

---

## 6.1 `typeahead`: a prefix autocomplete service  `S`

**Goal.** Given 100,000 phrases with popularity scores, return the top 10 completions for a prefix in under 1 ms, with typo tolerance of one edit after the first 3 characters.
**Learn.** Trie vs radix tree vs sorted array + binary search, top-k per node caching, edit distance, ranking.
**Read before.** `search_vectors/autocomplete_engine`, `data_structures/trie_radix`, `capstones/typeahead_service`. **During.** `interview/tries_core_questions`, `algorithms/dp_edit_distance`, `data_structures/priority_queue_zoo`.
**Milestones.**
1. Sorted-array baseline: binary search range then top-k by score.
2. Radix tree storing the top 10 ids at each node.
3. Update path: increment score for a phrase and keep node top-k correct.
4. One-edit fuzzy fallback using a bounded Levenshtein DFS over the trie.
5. HTTP endpoint `GET /suggest?q=` returning JSON; p99 measured with 10,000 requests.
**Acceptance.** Oracle = full scan with sort; 1,000 random prefixes match exactly (ties broken by phrase asc); median lookup under 50 us in-process; fuzzy returns every phrase within distance 1 (checked by brute force).
**Data.** Generate 100k phrases from a Zipf-weighted vocabulary of 20k words (1-4 words each) or use `/usr/share/dict/words` with random scores.
**Write-up.** How do you keep per-node top-k correct on updates and deletions? Memory per phrase?
**Stretch.** Personalized boosting; FST-based dictionary (`BurntSushi/fst`, see below).
**Resembles.** [fst crate](https://github.com/BurntSushi/fst), [Meilisearch](https://github.com/meilisearch/meilisearch) prefix search.

## 6.2 `sketchd`: approximate-stats sidecar (Bloom, Count-Min, HyperLogLog)  `S`

**Goal.** A small library + CLI reading a stream of events and answering: "have I seen this id?" (Bloom), "how often did key K occur?" (Count-Min), "how many distinct users?" (HLL), "top 20 heavy hitters?" (Space-Saving), in fixed memory.
**Learn.** Error bounds from formulas, sizing from (epsilon, delta), mergeable sketches, hash quality, relative error measurement.
**Read before.** `search_vectors/bloom_filter`, `data_structures/count_min_sketch`, `data_structures/hyperloglog`. **During.** `data_structures/hash_functions`, `data_structures/sampling`, `perf/fast_hashing`.
**Milestones.**
1. Bloom for 1M items at 1% FPR; verify FPR empirically.
2. Count-Min with width `ceil(e/eps)` and depth `ceil(ln(1/delta))` for eps=0.001, delta=0.01; measure overestimate distribution.
3. HLL with p=14 (16 KiB); standard error 1.04/sqrt(m) ~ 0.8%; test 1k, 100k, 10M cardinalities.
4. Merge: split a stream into 8 shards, merge sketches, equal to the single-sketch result (HLL max-merge, CMS add).
5. CLI `sketchd < events.ndjson` printing a stats report.
**Acceptance.** HLL relative error under 3% in at least 99% of 200 trials per cardinality; CMS never underestimates; merged equals unsharded exactly.
**Data.** Event generator with 5M events over 500k users, Zipf(1.1) keys.
**Write-up.** When does Count-Min fail badly (low-frequency keys)? What breaks if your hash is weak or correlated across rows?
**Stretch.** Sliding-window counts via rotating sketches; t-digest for quantiles.
**Resembles.** Redis HyperLogLog (`PFADD`) in [Redis/Valkey](https://github.com/valkey-io/valkey), Apache DataSketches.

## 6.3 `trigrep`: a trigram index for code search  `M`

**Goal.** Index a source tree (try the Rust standard library or your `~/.cargo/registry`) with a trigram inverted index so a regex or substring query first narrows candidate files, then verifies with the real matcher.
**Learn.** Inverted indexes, posting-list intersection, varint/delta compression, regex-to-trigram query planning (Cox's article), index file format, incremental update.
**Read before.** `search_vectors/trigram_index`, `data_structures/bitset_rank_select_roaring`, `encoding_compression/encodings_codecs`. **During.** `algorithms/string_algos`, `os_and_storage/mmap_files`, `perf/cache_locality`.
**Milestones.**
1. Build index: file ids x trigrams (lowercased bytes), postings delta+varint encoded.
2. Query `"fn main"` -> trigram intersection (smallest list first) -> verify candidates.
3. Regex literal extraction: for `foo.*bar` AND the trigrams of both literals; for alternation OR them.
4. Persist to one file; `mmap` and query without loading it all.
5. Incremental: re-index only files whose mtime/hash changed.
**Acceptance.** Equal to `rg -l` results on 200 literal and 50 regex queries over the same tree; the index narrows to under 5% of files for queries with a literal of at least 4 chars; index size under 40% of source size.
**Data.** `rustc` library sources (`rustup component add rust-src`) or the Linux kernel tree; queries sampled from identifiers in the corpus.
**Write-up.** Why does the index return only a superset of matches? Which queries can the index not help (e.g. `.*`)?
**Stretch.** Position-aware postings to avoid verification; sharded index built with rayon.
**Resembles.** [Zoekt](https://github.com/sourcegraph/zoekt) (trigram code search); [ripgrep](https://github.com/BurntSushi/ripgrep) is the no-index cousin.

## 6.4 `bm25d`: a full-text search engine with ranking  `M`

**Goal.** Tokenize, normalize and index documents with positions; rank with BM25; support phrase queries, boolean AND/OR/NOT, field boosts and snippet highlighting; segment-based index with merges.
**Learn.** Analyzers, postings with positions, BM25 parameters (k1=1.2, b=0.75), skip lists/block-max for top-k, segment merging, evaluation with nDCG.
**Read before.** `capstones/search_engine`, `search_vectors/trigram_index`. **During.** `data_structures/skiplist`, `data_structures/bitset_rank_select_roaring`, `algorithms/sorting_searching`.
**Milestones.**
1. Tokenizer (Unicode words, lowercase), stopwords, simple stemmer (Porter-lite or suffix rules).
2. Inverted index with term frequency; BM25 scoring and top-10 via heap.
3. Phrase query using positions; boolean queries parsed from `+must -not should`.
4. Block-max WAND (or MaxScore) skipping; compare against exhaustive scoring (identical top-10).
5. Immutable segments flushed every 10,000 docs, merged tiered; deletes via a bitmap.
**Acceptance.** Top-10 equals exhaustive scoring on 500 queries; on a 1M-document corpus, p99 under 20 ms for 2-term queries; nDCG@10 computed on a small judged set (use the Cranfield collection) and reported.
**Data.** Wikipedia abstracts dump (`enwiki-latest-abstract` slices) or 1M synthetic docs from a Zipf vocabulary; Cranfield for relevance.
**Write-up.** Why do segments help writes and hurt reads? How does WAND guarantee not to miss a top-k doc?
**Stretch.** Fuzzy terms with Levenshtein automata; faceting.
**Resembles.** [Tantivy](https://github.com/quickwit-oss/tantivy), [Meilisearch](https://github.com/meilisearch/meilisearch).

## 6.5 `knnbench`: exact kNN, quantization and a recall harness  `M`

**Goal.** Exact kNN over 1M x 128-dim vectors with SIMD-friendly layout, then scalar (int8) and product quantization, with a harness reporting recall@10, QPS, memory and build time on the same data.
**Learn.** Distance metrics, norm-trick for cosine, cache blocking, int8 dot-products, PQ codebooks (m=16, k=256) and asymmetric distance tables, evaluation methodology.
**Read before.** `search_vectors/knn_bruteforce`, `search_vectors/product_quantization`, `perf/simd_intrinsics`. **During.** `perf/data_layout`, `perf/matmul_optimization`, `math_science/numerics_and_math`.
**Milestones.**
1. Generators: random Gaussian and clustered (50 centers) data; ground truth via exact search for 1,000 queries.
2. Exact search with batched queries, f32 SoA/AoS comparison; QPS reported.
3. int8 scalar quantization per dimension; recall@10 vs f32.
4. PQ: k-means training on 100k sample, ADC lookup tables; recall vs code size (16 B, 32 B, 64 B per vector).
5. Rerank top-100 from PQ with exact distances; show recall recovery.
**Acceptance.** Recall@10 of exact = 1.0; int8 over 0.97 on clustered data; PQ+rerank over 0.9 with 16x compression; a table of memory (MB), QPS, recall for each variant in your write-up.
**Data.** SIFT1M (`ann-benchmarks.com` datasets) or the synthetic sets above.
**Write-up.** Why does recall on random Gaussian data look worse than on real data? What is the curse of dimensionality doing here?
**Stretch.** `std::simd` or `wide` implementation; multi-threaded batch with rayon.
**Resembles.** [Qdrant quantization](https://github.com/qdrant/qdrant), [LanceDB](https://github.com/lancedb/lancedb).

## 6.6 `hnsw`: an HNSW index tuned for recall/latency  `L`

**Goal.** Implement HNSW (layers, `M=16`, `efConstruction=200`, `efSearch` tunable), compare to your exact search, and produce a recall-vs-QPS curve for efSearch in {10, 20, 50, 100, 200, 400}.
**Learn.** Small-world graphs, greedy search with a candidate heap, neighbor selection heuristic, entry point maintenance, memory layout (CSR-like adjacency), concurrent insertion pitfalls.
**Read before.** `search_vectors/hnsw_lite`, `algorithms/csr_graph`, `data_structures/priority_queue_zoo`. **During.** `search_vectors/vamana_diskann_lite`, `search_vectors/ivf_flat`, `data_structures/bitset_rank_select_roaring` (visited set).
**Milestones.**
1. Single-layer navigable graph (NSW) with greedy beam search; recall measured.
2. Add layers with level sampling `floor(-ln(U)*1/ln(M))`.
3. Neighbor selection heuristic (diversify) vs simple closest-M; show recall difference.
4. Deletion by tombstones + a repair strategy; `save/load`.
5. Plot recall@10 vs QPS vs efSearch; compare with IVF and with Vamana.
**Acceptance.** Recall@10 >= 0.95 on SIFT-like 100k data at efSearch <= 100; at least 10x QPS of exact search at that recall; deterministic build given seed (same graph hash twice).
**Data.** 100k-1M vectors from 6.5 (clustered synthetic or SIFT/GloVe from ann-benchmarks).
**Write-up.** Why does greedy search get stuck in local minima and how do layers and ef reduce it? Memory per vector as a function of M?
**Stretch.** Filtered search; disk-resident graph with a neighbor cache.
**Resembles.** [Qdrant](https://github.com/qdrant/qdrant), [hnswlib](https://github.com/nmslib/hnswlib), [LanceDB](https://github.com/lancedb/lancedb).

## 6.7 `dedupe`: near-duplicate detection with MinHash LSH  `M`

**Goal.** Given 1M documents (or git commit messages/logs), find pairs with Jaccard similarity >= 0.8 using shingling, MinHash signatures (128 permutations) and LSH banding, without comparing all pairs.
**Learn.** Jaccard, shingles, MinHash estimator error, LSH band/row trade-off (S-curve), SimHash alternative, precision/recall vs thresholds.
**Read before.** `data_structures/minhash_simhash_lsh`, `data_structures/hash_functions`. **During.** `data_structures/bitset_rank_select_roaring`, `algorithms/union_find` (cluster merging), `perf/fast_hashing`.
**Milestones.**
1. Word 3-shingles hashed to u64; exact Jaccard function (oracle).
2. MinHash with 128 hash functions; estimate error under 0.1 for 95% of pairs.
3. LSH with b=16 bands x r=8 rows (threshold ~ (1/b)^(1/r) ~ 0.71); verify candidates by exact Jaccard.
4. Cluster duplicates with union-find; output clusters sorted by size.
5. Tune (b, r) to hit recall >= 0.95 at threshold 0.8; compute precision.
**Acceptance.** On a corpus with 5% injected near-duplicates (random 0-20% word edits), recall >= 0.95 and the candidate pairs checked are < 0.1% of n^2/2; runtime for 1M docs under 2 minutes release.
**Data.** Generate base docs from a Zipf vocabulary, then create mutated copies at known similarity to know the ground truth.
**Write-up.** Draw the S-curve for your (b, r): where is the 50% collision similarity? How would you handle a 100x larger corpus?
**Stretch.** Streaming dedupe with a rolling index; SimHash for 64-bit fingerprints with Hamming distance <= 3.
**Resembles.** [datasketch](https://github.com/ekzhu/datasketch) (Python MinHash/LSH reference).

## 6.8 `hybrid`: hybrid search with filters and reciprocal rank fusion  `L`

**Goal.** One query API combining BM25 (6.4), vector similarity (6.6) and structured filters (`price < 50 AND category IN (...)`), fused by reciprocal rank fusion (k=60), with a filter-aware strategy (pre-filter, post-filter, in-graph filter chosen by selectivity).
**Learn.** Filter selectivity estimation, bitmap filters, post-filter recall collapse, RRF, evaluation of fused rankers, query planning.
**Read before.** `data_structures/bitset_rank_select_roaring`, `search_vectors/hnsw_lite`, `search_vectors/trigram_index`. **During.** `database/cardinality_estimation_lab`, `database/hash_and_bitmap_indexes`, `capstones/search_engine`.
**Milestones.**
1. Metadata store with roaring-style bitmaps per (field, value) and range indexes.
2. Pre-filter: bitmap -> brute-force exact over survivors; post-filter: ANN then filter (show recall collapse at 1% selectivity).
3. In-graph filtered HNSW (skip non-matching in result set but still traverse).
4. Planner: selectivity < 2% -> brute force over bitmap; else filtered ANN; verify threshold by experiment.
5. RRF merging BM25 and vector lists; evaluate on a labelled query set.
**Acceptance.** For selectivity in {50%, 10%, 1%, 0.1%}: recall@10 >= 0.9 at every level using the planner (post-filter alone must be shown to fail at 1%); fused ranking has higher nDCG@10 than either single ranker on your labelled set.
**Data.** 200k products: title text (Zipf vocab), 64-d embedding from a generator that ties embedding clusters to categories, price log-normal, 20 categories.
**Write-up.** Why does RRF avoid score normalization? What does your planner do when stats are stale?
**Stretch.** Reranking with a cross-scorer; per-tenant isolation with per-tenant indexes vs filters.
**Resembles.** [Qdrant filtering](https://github.com/qdrant/qdrant), [Meilisearch hybrid search](https://github.com/meilisearch/meilisearch).

---

## Katas (30 minutes each)

1. Write cosine similarity three ways (naive, pre-normalized dot, f32 SIMD-friendly chunks of 8) and assert results agree within 1e-5 on 10,000 random pairs.
2. Compute recall@10 between two result lists and test it on hand-made cases (identical, disjoint, half overlap = 0.5).
3. Build a posting list intersection for sorted `u32` lists using galloping search; compare speed with merge intersection for lists of size 100 vs 1,000,000.
4. Delta + varint encode a sorted posting list of 1M doc ids with average gap 20; report bytes per posting (expect about 1.1).
5. Implement Levenshtein with a banded DP (max distance 2) and test against full DP on 5,000 random word pairs.
6. Generate 100,000 random 3-shingles of a text and measure the false positive rate of a Bloom filter sized for 1% at insertion counts 50%, 100%, 150% of capacity.
7. Write a k-means (k=16, Lloyd, 20 iterations, seeded k-means++ init) on 2-D data and assert the inertia never increases between iterations.
8. Implement top-k selection with a size-k heap vs full sort for n=1M, k=10; print the speedup and assert the same result.
9. Implement reservoir sampling (k=100) and test uniformity: each of 1,000 items chosen with frequency within 3 sigma over 20,000 runs.
10. Compute BM25 by hand for a 3-document collection, put the expected scores in a test, and match them to 1e-9 (`k1=1.2`, `b=0.75`).
