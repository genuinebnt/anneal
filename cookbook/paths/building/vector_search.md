# Building: A vector database

The capstone is planned; these are its parts.

Use this when you have a real project in front of you. For each stage: **attempt your design first**, then read the listed recipes to compare, steal what is better and note what you missed.

### Step 1: Distances and baseline

**Goal:** Exact first.

| # | Recipe | Why here |
|---|---|---|
| 1.1 | [`knn_bruteforce`](../../examples/search_vectors/knn_bruteforce.rs) · intermediate | Baseline. |
| 1.2 | [`simd_intrinsics`](../../examples/perf/simd_intrinsics.rs) · advanced | Fast distance kernels. |
| 1.3 | `vector_distance_kernels_simd` 🔜 | Distance kernels (planned). |

### Step 2: Indexes

**Goal:** Trade recall for speed.

| # | Recipe | Why here |
|---|---|---|
| 2.1 | [`ivf_flat`](../../examples/search_vectors/ivf_flat.rs) · advanced | Partitioning. |
| 2.2 | [`product_quantization`](../../examples/search_vectors/product_quantization.rs) · advanced | Compression. |
| 2.3 | [`hnsw_lite`](../../examples/search_vectors/hnsw_lite.rs) · advanced | Graph. |
| 2.4 | [`vamana_diskann_lite`](../../examples/search_vectors/vamana_diskann_lite.rs) · advanced | Disk graph. |

### Step 3: Database features

**Goal:** Make it a product.

| # | Recipe | Why here |
|---|---|---|
| 3.1 | `scalar_binary_quantization_rescoring` 🔜 | Rescoring (planned). |
| 3.2 | `filtered_ann_hybrid_search` 🔜 | Filters and hybrid (planned). |
| 3.3 | `mini_vector_database` 🔜 | Capstone (planned). |
| 3.4 | [`trigram_index`](../../examples/search_vectors/trigram_index.rs) · advanced | Hybrid keyword leg. |
