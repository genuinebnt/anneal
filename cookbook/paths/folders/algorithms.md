# Chapter `algorithms`: suggested reading order

Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. Cross-chapter order is in [the tracks](../tracks/).

| # | Recipe | Level | What it is |
|---|---|---|---|
| 1 | [`dp_memoization`](../../examples/algorithms/dp_memoization.rs) | simple | memoization vs tabulation on coin change, and why greedy is not enough. |
| 2 | [`bipartite_matching`](../../examples/algorithms/bipartite_matching.rs) | intermediate | bipartite matching (Kuhn's augmenting paths): assign each worker to at most one job. |
| 3 | [`bit_tricks`](../../examples/algorithms/bit_tricks.rs) | intermediate | bit-twiddling toolbox (Hacker's Delight / TAOCP 7.1.3), every trick checked against std or a naive loop. |
| 4 | [`csr_graph`](../../examples/algorithms/csr_graph.rs) | intermediate | CSR (compressed sparse row) graph: counting-sort build, reverse graph, BFS and PageRank vs Vec<Vec<u32>>. |
| 5 | [`dijkstra`](../../examples/algorithms/dijkstra.rs) | intermediate | Dijkstra's shortest paths with a binary heap (lazy deletion) and path reconstruction. |
| 6 | [`dp_count_paths`](../../examples/algorithms/dp_count_paths.rs) | intermediate | counting routes through a DAG: top-down memoization vs bottom-up, on the same recurrence. |
| 7 | [`dp_edit_distance`](../../examples/algorithms/dp_edit_distance.rs) | intermediate | edit distance (Levenshtein) and "did you mean ...?" suggestions. |
| 8 | [`dp_knapsack`](../../examples/algorithms/dp_knapsack.rs) | intermediate | 0/1 knapsack: pick items under a capacity to maximise value, and recover WHICH items. |
| 9 | [`dp_lcs_diff`](../../examples/algorithms/dp_lcs_diff.rs) | intermediate | longest common subsequence, and turning it into a diff between two versions. |
| 10 | [`index_graph`](../../examples/algorithms/index_graph.rs) | intermediate | index-based graph, interning names once, iterative BFS. |
| 11 | [`shortest_path_zoo`](../../examples/algorithms/shortest_path_zoo.rs) | intermediate | shortest-path zoo: Bellman-Ford, Floyd-Warshall, A*, bidirectional BFS, 0-1 BFS (and which to choose). |
| 12 | [`union_find`](../../examples/algorithms/union_find.rs) | intermediate | union-find (disjoint sets): "are these two connected?" in near-constant time. |
| 13 | [`aho_corasick`](../../examples/algorithms/aho_corasick.rs) | advanced | Aho-Corasick multi-pattern matching: trie + failure links + dictionary links. |
| 14 | [`backtracking_branch_bound`](../../examples/algorithms/backtracking_branch_bound.rs) | advanced | backtracking, branch and bound, and constraint propagation on NP-hard problems. |
| 15 | [`combinatorial_generation`](../../examples/algorithms/combinatorial_generation.rs) | advanced | generating combinatorial objects as iterators (Gray codes, combinations, permutations, partitions, Catalan, necklaces, de Bruijn). |
| 16 | [`combo_algorithms`](../../examples/algorithms/combo_algorithms.rs) | advanced | problems that need several data structures working together. |
| 17 | [`dp_zoo`](../../examples/algorithms/dp_zoo.rs) | advanced | a zoo of DP shapes, each with a brute-force oracle and seeded differential tests. |
| 18 | [`heuristics_zoo`](../../examples/algorithms/heuristics_zoo.rs) | advanced | heuristics and approximation zoo, each checked against an exact solver on small instances. |
| 19 | [`max_flow_dinic`](../../examples/algorithms/max_flow_dinic.rs) | advanced | max-flow with Dinic's algorithm, min-cut extraction, bipartite matching and project selection. |
| 20 | [`mst_bridges`](../../examples/algorithms/mst_bridges.rs) | advanced | minimum spanning tree (Prim, Kruskal) and bridges / articulation points via iterative low-link DFS. |
| 21 | [`problem_modeling_reductions`](../../examples/algorithms/problem_modeling_reductions.rs) | advanced | problem modeling by reduction (Skiena): puzzles as shortest paths, 2-SAT, assignment, bipartite, scheduling, Steiner, Kadane, sorting. |
| 22 | [`procedural_puzzles`](../../examples/algorithms/procedural_puzzles.rs) | advanced | procedural puzzles with ASCII pictures: mazes, Sudoku, queens, WFC, tours, Hanoi, Gray codes. |
| 23 | [`sat_solver_cdcl`](../../examples/algorithms/sat_solver_cdcl.rs) | advanced | a SAT solver, from DPLL to CDCL (watched literals, 1UIP learning, VSIDS, restarts), plus problem encodings. |
| 24 | [`sorting_networks_external_sort`](../../examples/algorithms/sorting_networks_external_sort.rs) | advanced | TAOCP Vol 3 sorting: sorting networks (0-1 principle), merge insertion, replacement selection, external k-way merge, optimal BST (Knuth). |
| 25 | [`sorting_searching`](../../examples/algorithms/sorting_searching.rs) | advanced | sorting and searching toolkit, differential-tested against std. |
| 26 | [`string_algos`](../../examples/algorithms/string_algos.rs) | advanced | KMP, Z-algorithm, Rabin-Karp and the suffix array + Kasai LCP, with differential tests. |
| 27 | [`tarjan_scc`](../../examples/algorithms/tarjan_scc.rs) | advanced | strongly connected components (iterative Tarjan): find circular groups in a directed graph. |
