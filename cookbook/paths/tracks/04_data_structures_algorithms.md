# Track: Data structures and algorithms in Rust

About 6 weeks; interview files double as timed practice.

**How to work each step:** read the header, predict the asserts, blank the file and rebuild it from memory (`./study.sh NAME`), then mutate it. Details in [HOW_TO_STUDY.md](../../HOW_TO_STUDY.md). Checkpoint = you can explain the step's goal in five lines without looking.

### Step 1: Linear and hashing

**Goal:** The workhorses.

| # | Recipe | Why here |
|---|---|---|
| 1.1 | [`heap_patterns`](../../examples/data_structures/heap_patterns.rs) · intermediate | Heaps, tie-breaking, top-k. |
| 1.2 | [`ring_buffer_zoo`](../../examples/data_structures/ring_buffer_zoo.rs) · intermediate | Circular buffers. |
| 1.3 | [`hash_functions`](../../examples/data_structures/hash_functions.rs) · intermediate | Why hash choice matters. |
| 1.4 | [`open_addressing_map`](../../examples/data_structures/open_addressing_map.rs) · advanced | Build a hash map. |
| 1.5 | [`lru_cache`](../../examples/data_structures/lru_cache.rs) · advanced | Hash map + list. |
| 1.6 | [`cache_policy_zoo`](../../examples/data_structures/cache_policy_zoo.rs) · advanced | FIFO/LRU/LFU/CLOCK/2Q/ARC/SIEVE compared. |

### Step 2: Trees and tries

**Goal:** Ordered and prefix structures.

| # | Recipe | Why here |
|---|---|---|
| 2.1 | [`binary_trees_in_rust`](../../examples/interview/binary_trees_in_rust.rs) · intermediate | Ownership of recursive types. |
| 2.2 | [`trie_radix`](../../examples/data_structures/trie_radix.rs) · intermediate | Tries and radix trees. |
| 2.3 | [`treap_interval`](../../examples/data_structures/treap_interval.rs) · advanced | Balanced and interval trees. |
| 2.4 | [`segment_fenwick`](../../examples/data_structures/segment_fenwick.rs) · intermediate | Range queries. |
| 2.5 | [`skiplist`](../../examples/data_structures/skiplist.rs) · advanced | Probabilistic ordered list. |
| 2.6 | [`bplus_tree`](../../examples/data_structures/bplus_tree.rs) · advanced | Database-style trees. |

### Step 3: Graphs and DP

**Goal:** Modelling problems.

| # | Recipe | Why here |
|---|---|---|
| 3.1 | [`index_graph`](../../examples/algorithms/index_graph.rs) · intermediate | Vec-index graphs (the Rust way). |
| 3.2 | [`csr_graph`](../../examples/algorithms/csr_graph.rs) · intermediate | Compressed sparse rows. |
| 3.3 | [`dijkstra`](../../examples/algorithms/dijkstra.rs) · intermediate | Shortest paths. |
| 3.4 | [`union_find`](../../examples/algorithms/union_find.rs) · intermediate | Disjoint sets. |
| 3.5 | [`dp_memoization`](../../examples/algorithms/dp_memoization.rs) · simple | Memo vs tabulation. |
| 3.6 | [`dp_zoo`](../../examples/algorithms/dp_zoo.rs) · advanced | Recognise the DP shape. |
| 3.7 | [`max_flow_dinic`](../../examples/algorithms/max_flow_dinic.rs) · advanced | Flows and matching. |

### Step 4: Strings and search

**Goal:** Text structure.

| # | Recipe | Why here |
|---|---|---|
| 4.1 | [`string_algos`](../../examples/algorithms/string_algos.rs) · advanced | KMP, Z, rolling hash. |
| 4.2 | [`aho_corasick`](../../examples/algorithms/aho_corasick.rs) · advanced | Multi-pattern search. |
| 4.3 | [`tries_core_questions`](../../examples/interview/tries_core_questions.rs) · intermediate | Interview trie set. |
| 4.4 | [`tries_advanced_questions`](../../examples/interview/tries_advanced_questions.rs) · advanced | Word Search II, XOR tries. |
| 4.5 | [`bloom_filter`](../../examples/search_vectors/bloom_filter.rs) · intermediate | Probabilistic membership. |

### Step 5: Interview practice

**Goal:** Retrieval practice: timer, scratch file, oracle.

| # | Recipe | Why here |
|---|---|---|
| 5.1 | [`arrays_strings_hashmap_questions`](../../examples/interview/arrays_strings_hashmap_questions.rs) · intermediate | Arrays, strings, hashing. |
| 5.2 | [`stacks_queues_heaps_questions`](../../examples/interview/stacks_queues_heaps_questions.rs) · intermediate | Stacks, queues, heaps. |
| 5.3 | [`linked_lists_in_rust`](../../examples/interview/linked_lists_in_rust.rs) · intermediate | Why linked lists are hard in Rust. |
| 5.4 | [`graphs_and_union_find_questions`](../../examples/interview/graphs_and_union_find_questions.rs) · advanced | Graph questions. |
| 5.5 | [`dp_and_backtracking_questions`](../../examples/interview/dp_and_backtracking_questions.rs) · advanced | DP and backtracking. |
| 5.6 | [`implement_std_from_scratch`](../../examples/interview/implement_std_from_scratch.rs) · advanced | Implement Vec, Rc, HashMap. |
