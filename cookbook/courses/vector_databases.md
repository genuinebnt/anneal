# Vector databases and similarity search

Source: Topics from the HNSW, DiskANN, IVF/PQ literature and the vector-search lecture in CMU 15-721 (Fall 2024, industry speaker). No dedicated CMU vector-database course page was found.

Coverage: 7 of 15 recipe slots exist.

| Lecture / topic | Recipes |
|---|---|
| Exact kNN baseline and distance kernels | [`knn_bruteforce`](../examples/search_vectors/knn_bruteforce.rs) ✅<br>`vector_distance_kernels_simd` 🔜 |
| IVF (k-means partitioning) and product quantization | [`ivf_flat`](../examples/search_vectors/ivf_flat.rs) ✅<br>[`product_quantization`](../examples/search_vectors/product_quantization.rs) ✅ |
| Graph indexes: HNSW and Vamana/DiskANN | [`hnsw_lite`](../examples/search_vectors/hnsw_lite.rs) ✅<br>[`vamana_diskann_lite`](../examples/search_vectors/vamana_diskann_lite.rs) ✅ |
| Scalar and binary quantization with re-scoring | `scalar_binary_quantization_rescoring` 🔜 |
| Dimensionality reduction: Johnson-Lindenstrauss, PCA | `dimensionality_reduction_jl_pca` 🔜 |
| Disk-based IVF (SPANN-style) | `spann_disk_ivf_lite` 🔜 |
| Filtered search and hybrid (BM25 + vector) search with rank fusion | `filtered_ann_hybrid_search` 🔜<br>[`search_engine`](../examples/capstones/search_engine.rs) ✅ |
| Updates, deletes, segments, persistence | `vector_index_updates_persistence` 🔜 |
| Evaluation: recall@k, QPS curves, hardness of datasets | `ann_evaluation_harness` 🔜 |
| LSH, MinHash, SimHash | [`minhash_simhash_lsh`](../examples/data_structures/minhash_simhash_lsh.rs) ✅ |
| A whole mini vector database | `mini_vector_database` 🔜 |

How to use: read the lecture's papers, attempt the recipe's asserts yourself (see ../HOW_TO_STUDY.md), then compare.
