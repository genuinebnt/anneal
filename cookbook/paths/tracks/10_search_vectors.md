# Track: Search and vector similarity

About 4 weeks.

**How to work each step:** read the header, predict the asserts, blank the file and rebuild it from memory (`./study.sh NAME`), then mutate it. Details in [HOW_TO_STUDY.md](../../HOW_TO_STUDY.md). Checkpoint = you can explain the step's goal in five lines without looking.

### Step 1: Text search

**Goal:** Index words and substrings.

| # | Recipe | Why here |
|---|---|---|
| 1.1 | [`trigram_index`](../../examples/search_vectors/trigram_index.rs) · advanced | Substring search. |
| 1.2 | [`autocomplete_engine`](../../examples/search_vectors/autocomplete_engine.rs) · advanced | Prefix completion. |
| 1.3 | [`bloom_filter`](../../examples/search_vectors/bloom_filter.rs) · intermediate | Skip lookups. |
| 1.4 | [`search_engine`](../../examples/capstones/search_engine.rs) · capstone | Capstone. |
| 1.5 | [`typeahead_service`](../../examples/capstones/typeahead_service.rs) · capstone | Capstone. |

### Step 2: Vector search

**Goal:** Similarity at scale.

| # | Recipe | Why here |
|---|---|---|
| 2.1 | [`knn_bruteforce`](../../examples/search_vectors/knn_bruteforce.rs) · intermediate | Baseline and recall. |
| 2.2 | [`ivf_flat`](../../examples/search_vectors/ivf_flat.rs) · advanced | Partitioning. |
| 2.3 | [`product_quantization`](../../examples/search_vectors/product_quantization.rs) · advanced | Compression. |
| 2.4 | [`hnsw_lite`](../../examples/search_vectors/hnsw_lite.rs) · advanced | Graph index. |
| 2.5 | [`vamana_diskann_lite`](../../examples/search_vectors/vamana_diskann_lite.rs) · advanced | Disk-friendly graph. |
| 2.6 | [`minhash_simhash_lsh`](../../examples/data_structures/minhash_simhash_lsh.rs) · advanced | Near duplicates. |
| 2.7 | `mini_vector_database` 🔜 | Capstone (planned). |
