# Chapter `search_vectors`: suggested reading order

Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. Cross-chapter order is in [the tracks](../tracks/).

| # | Recipe | Level | What it is |
|---|---|---|---|
| 1 | [`bloom_filter`](../../examples/search_vectors/bloom_filter.rs) | intermediate | Bloom filter: "definitely not present" vs "probably present". |
| 2 | [`knn_bruteforce`](../../examples/search_vectors/knn_bruteforce.rs) | intermediate | brute-force cosine kNN with a top-k heap: the baseline every ANN index is measured against. |
| 3 | [`autocomplete_engine`](../../examples/search_vectors/autocomplete_engine.rs) | advanced | type-ahead autocomplete: trie with cached top-k, decayed scores, bounded fuzzy search. |
| 4 | [`hnsw_lite`](../../examples/search_vectors/hnsw_lite.rs) | advanced | a simplified HNSW: layered proximity graphs, greedy descent, best-first search with `ef`. |
| 5 | [`ivf_flat`](../../examples/search_vectors/ivf_flat.rs) | advanced | IVF-Flat: k-means partitions the vectors into inverted lists; search probes only the nearest few lists. |
| 6 | [`product_quantization`](../../examples/search_vectors/product_quantization.rs) | advanced | Product Quantization: compress vectors to m bytes and search with Asymmetric Distance Computation (ADC). |
| 7 | [`trigram_index`](../../examples/search_vectors/trigram_index.rs) | advanced | trigram inverted index: filter with the index, then verify. |
| 8 | [`vamana_diskann_lite`](../../examples/search_vectors/vamana_diskann_lite.rs) | advanced | a DiskANN / Vamana-style single-layer graph index with GreedySearch, RobustPrune and "node read" accounting. |
