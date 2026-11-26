# Practice: search_vectors

How to use this page: run the recipe (`cargo run --release --example <name>`), answer Prove you got it, then do Try next in order.
Each task names a check (brute-force oracle, recall number, counter); write the check before the change.
No solutions here. S about 30 min, M about 2 h, A about a day. Recipe files are in `examples/search_vectors/`.

## autocomplete_engine
**Prove you got it**
1. Why does caching top-k at each trie node make a one-letter prefix cheap, and what does an insert now cost?
2. How does decay change ranking over time and what must stay consistent in the cached lists?

**Try next**
- [S] Assert for 1000 random prefixes that the cached top-k equals a brute-force sort over all words.
- [M] Add a bounded fuzzy search (edit distance 1) and measure query latency against the number of words, 1e3 to 1e5.
- [A] Persist the trie to a compact byte format and query it without deserialising (zero-copy), checked against the in-memory one.

**Use it in:** search-box suggestions. Real code: [BurntSushi/fst](https://github.com/BurntSushi/fst), [meilisearch/meilisearch](https://github.com/meilisearch/meilisearch).
**Pitfall:** ties in score must be broken deterministically or tests flake.

## bloom_filter
**Prove you got it**
1. Given n items and m bits, what k minimises false positives and what rate results?
2. Why can you not delete from a plain Bloom filter?

**Try next**
- [S] Measure the observed false-positive rate for 5 sizes and compare with the formula.
- [M] Implement a counting variant with deletes and a test that insert-delete leaves the filter equal to a fresh one.
- [A] Build a blocked (cache-line) Bloom filter and report lookup speed against the plain one at 1e7 items.

**Use it in:** LSM reads, join pruning. Real code: [facebook/rocksdb](https://github.com/facebook/rocksdb), [apache/arrow-rs](https://github.com/apache/arrow-rs).
**Pitfall:** double hashing with a weak second hash makes bits correlate.

## hnsw_lite
**Prove you got it**
1. What do `M`, `ef_construction` and `ef` each trade between recall and cost?
2. Why do upper layers use few long hops?

**Try next**
- [S] Plot recall@10 against `ef` for 5 values using `knn_bruteforce` as ground truth.
- [M] Add deletion by tombstone and re-check recall after deleting 20% of points.
- [A] Compare with `ivf_flat` and `vamana_diskann_lite` on one dataset: recall at equal distance-computation budget.

**Use it in:** vector search services. Real code: [nmslib/hnswlib](https://github.com/nmslib/hnswlib), [qdrant/qdrant](https://github.com/qdrant/qdrant).
**Pitfall:** insertion order changes graph quality; test with several seeds.

## ivf_flat
**Prove you got it**
1. Why does recall fall when a query lies near a cell boundary?
2. How does `nprobe` trade accuracy against scan size?

**Try next**
- [S] Print recall and scanned fraction for nprobe 1,2,4,8,16.
- [M] Add a re-training trigger when the list-size imbalance exceeds a threshold and show the effect on tail latency.
- [A] Add residual encoding with `product_quantization` to build IVF-PQ and compare memory and recall to IVF-Flat.

**Use it in:** large vector stores. Real code: [facebookresearch/faiss](https://github.com/facebookresearch/faiss).
**Pitfall:** k-means on non-normalised vectors when using cosine.

## knn_bruteforce
**Prove you got it**
1. Why a min-heap of size k, and what is the complexity?
2. Why is cosine on normalised vectors a dot product?

**Try next**
- [S] Verify ties and k greater than n behaviour with tests.
- [M] Parallelise with scoped threads and merge heaps; assert result equality.
- [A] Add SIMD (portable or `std::arch`) for the dot product and report GFLOP/s against scalar.

**Use it in:** ground truth for ANN. Real code: [pgvector/pgvector](https://github.com/pgvector/pgvector) exact scan.
**Pitfall:** NaN scores poison the heap ordering.

## product_quantization
**Prove you got it**
1. Compute the compression ratio for D=128, m=16.
2. What is asymmetric distance computation and why is the lookup table built per query?

**Try next**
- [S] Vary m and report reconstruction error and recall.
- [M] Add optimised PQ via a rotation (even a random one) and show error drop on correlated data.
- [A] Add re-ranking with exact vectors for the top 100 and plot recall versus rerank size.

**Use it in:** memory-bounded ANN. Real code: [facebookresearch/faiss](https://github.com/facebookresearch/faiss).
**Pitfall:** training on too few vectors per centroid gives empty codewords.

## trigram_index
**Prove you got it**
1. Why are trigram results candidates and not matches?
2. What happens for queries shorter than 3 bytes?

**Try next**
- [S] Count candidates versus true matches over 20 queries.
- [M] Add case-insensitive search and a posting-list intersection that starts with the rarest trigram; show speedup.
- [A] Extend to regex by extracting required trigrams from a pattern (see `regex_nfa_engine`) and verify with the regex.

**Use it in:** code search. Real code: [quickwit-oss/tantivy](https://github.com/quickwit-oss/tantivy), [rust-lang/regex](https://github.com/rust-lang/regex).
**Pitfall:** an index that omits the document boundary produces cross-document trigrams.

## vamana_diskann_lite
**Prove you got it**
1. What does RobustPrune with alpha > 1 keep that plain closest-R does not?
2. Why count node reads rather than distances?

**Try next**
- [S] Print node reads per query versus recall for L = 20, 50, 100.
- [M] Add a streaming insert and show recall after 1000 inserts without rebuild.
- [A] Store the graph and vectors in a file with fixed-size sector records and count real page reads per query.

**Use it in:** disk-resident ANN. Real code: [microsoft/DiskANN](https://github.com/microsoft/DiskANN).
**Pitfall:** a poor entry point (not the medoid) lowers recall on far queries.

## Chapter projects

Other-chapter stems used: `serde_custom` (encoding_compression), `columnar_encodings` (database), `consistent_hashing` (distributed), `axum_router_state` (web), `typeahead_service` and `search_engine` (capstones).

### Small (about 2 h): recall harness
Combine `knn_bruteforce`, `ivf_flat`, `hnsw_lite`.
Milestones: (1) seeded dataset generator; (2) recall@k and latency harness; (3) a table of three indexes.
Acceptance: harness reports recall 1.0 for brute force against itself, and numbers are reproducible per seed.
Stretch: add `product_quantization` as a fourth row.

### Intermediate (a weekend): hybrid search
Combine `trigram_index`, `bloom_filter`, `hnsw_lite`, `axum_router_state`.
Milestones: (1) keyword candidates by trigram; (2) vector rerank; (3) reciprocal rank fusion; (4) HTTP endpoint.
Acceptance: on a labelled mini corpus, fusion beats either alone on nDCG@10; endpoint answers in-process via `oneshot` tests.
Stretch: filtered ANN by metadata.

### Advanced (a week): sharded vector store
Combine `ivf_flat`, `product_quantization`, `vamana_diskann_lite`, `consistent_hashing`, `serde_custom`.
Milestones: (1) shard by hash ring; (2) per-shard index chosen by size; (3) scatter-gather top-k merge; (4) compact on-disk format; (5) rebalance when a shard is added.
Acceptance: global recall within 2% of single-node, adding a shard moves about 1/N vectors, and the on-disk file loads in under a second for 1e5 vectors.
Stretch: compare with `search_engine` BM25 for a mixed workload.
