# Search and Vectors

Inverted and trigram indexes, autocomplete, ANN (IVF, HNSW, Vamana, PQ), kNN.

Run any recipe from the `cookbook/` directory: `cargo run --release --example <name>` (the name is the file name without `.rs`). Each file starts with a header: what it shows, `LEVEL`, `COMMON USES` and `Read:` links.

8 examples.

| Example | What it shows | Level | Common uses | Crates |
|---|---|---|---|---|
| [`trigram_index`](trigram_index.rs) | trigram inverted index: filter with the index, then verify | advanced | log search, code search (ripgrep-style/zoekt), Postgres pg_trgm LIKE acceleration. | std only |
| [`bloom_filter`](bloom_filter.rs) | Bloom filter: "definitely not present" vs "probably present" | intermediate | LSM-tree SSTables (skip files that cannot contain a key), dedup caches, cache-penetration guards. | std only |
| [`knn_bruteforce`](knn_bruteforce.rs) | brute-force cosine kNN with a top-k heap: the baseline every ANN index is measured against | intermediate | semantic search, recommendations, dedup: exact kNN is the correctness baseline for every ANN index. | std only |
| [`ivf_flat`](ivf_flat.rs) | IVF-Flat: k-means partitions the vectors into inverted lists; search probes only the nearest few lists | advanced | FAISS IVF indexes (IndexIVFFlat, usually combined with product quantization as IVFPQ, see product_quantization); coarse | std only |
| [`hnsw_lite`](hnsw_lite.rs) | a simplified HNSW: layered proximity graphs, greedy descent, best-first search with `ef` | advanced | the default ANN index in Qdrant, pgvector (hnsw index type) and Lucene/Elasticsearch/OpenSearch dense-vector | std only |
| [`vamana_diskann_lite`](vamana_diskann_lite.rs) | a DiskANN / Vamana-style single-layer graph index with GreedySearch, RobustPrune and "node read" accounting | advanced | DiskANN (Microsoft Research) and its open implementations; Vamana-style graphs in large-scale vector databases | std only |
| [`product_quantization`](product_quantization.rs) | Product Quantization: compress vectors to m bytes and search with Asymmetric Distance Computation (ADC) | advanced | FAISS IVFPQ and PQ indexes; the in-RAM compressed sketch in DiskANN-style systems (see vamana_diskann_lite); vector databases | std only |
| [`autocomplete_engine`](autocomplete_engine.rs) | type-ahead autocomplete: trie with cached top-k, decayed scores, bounded fuzzy search | advanced | search-box suggestions, IDE/code completion, shell and command-palette ranking (frecency), | std only |
