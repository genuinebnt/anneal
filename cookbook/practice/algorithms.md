# Practice: algorithms

How to use this page: run the recipe first (`cargo run --release --example <name>`), read its `main()`, then close the file and do the tasks in a scratch crate, not in the recipe.
Every task names something you can check: an oracle, a measured exponent or a counterexample. If you cannot say how you would know you are right, you have not finished the task.
Order: [S] about 30 minutes, [M] about 2 hours, [A] a weekend evening or more. No solutions are given here; write the brute-force oracle BEFORE the clever version.

Note on FlowForge: the graph recipes touch ideas used by your stages (topological order, cycle detection, critical path). Attempt your stage first; use these recipes afterwards to compare designs, never to copy.

Differential-testing loop used throughout: seeded xorshift generates small random inputs, `fast(x) == oracle(x)` for thousands of seeds, and on failure you print the seed and shrink the input by hand (or see `dev_tools/shrinking`).

---

## aho_corasick

**Prove you got it**
1. Why is the total number of fail-link hops during `search` bounded by the text length, even though one byte can trigger many hops? Show it by counting hops on a text of 10^6 bytes with patterns `a`, `aa`, ..., `a^50` over input `a^10^6`.
2. Give two patterns where the dictionary (output) link is the only reason a match is reported at a position (the node itself ends no pattern). What breaks if you delete dictionary links?

**Try next**
- [S] Make `search` return only non-overlapping leftmost-longest matches. Oracle: a naive scan that tries every start position and every pattern, picks longest, then skips ahead.
- [M] Add case-insensitive ASCII matching without lowercasing the text (fold inside the automaton). Constraint: patterns and text up to 50 MB, memory under 4x the plain version; measure both.
- [A] Re-implement as a full DFA (precompute the goto for all 256 bytes per state) and compare against the NFA-with-fail-links version on 5,000 patterns of a real word list (`/usr/share/dict/words`). Report build time, memory, and scan MB/s; explain the crossover.

**Use in**: a log scanner that tags each line against hundreds of known error strings, or a secrets scanner over a repo. Same idea in [BurntSushi/aho-corasick](https://github.com/BurntSushi/aho-corasick) (used inside [regex](https://github.com/rust-lang/regex) for literal alternations).

**Pitfall**: building fail links in DFS order instead of BFS order. A node's fail target must already be finished; with DFS it sometimes is not and you get wrong links only on certain pattern sets, so test with random pattern sets, not three hand-written ones.

---

## backtracking_branch_bound

**Prove you got it**
1. In `Queens`/`Colouring` the `apply`/`undo` pair must be exact inverses. Write a debug assertion that hashes the state before `apply` and after `undo`; on what move sequence would a sloppy `undo` first fail?
2. For the knapsack branch and bound (`Knap::bound`), why must the bound be an UPPER bound on the best completion, and what happens to correctness (versus speed) if it is too loose? Too tight?

**Try next**
- [S] Change the variable ordering of graph colouring from fixed to "most constrained first" (smallest candidate set). Count nodes visited on 200 seeded random graphs of 30 nodes before and after.
- [M] Add a node budget and an "anytime" mode: return the best solution found when the budget runs out together with a proof status (optimal / not proven). Oracle: on instances where `knapsack_dp` finishes, an unlimited run must equal it.
- [A] Solve a real scheduling instance: assign 12 courses to 6 timeslots with room conflicts from a dataset you write down by hand (30 conflict pairs). Compare backtracking with propagation against encoding it as CNF for `sat_solver_cdcl` (same file family) and report time per instance size.

**Use in**: exact solvers for small timetabling and packing problems. Real-world analogue: the search core of [Google OR-Tools](https://github.com/google/or-tools) CP-SAT, and many puzzle solvers.

**Pitfall**: pruning with a bound that is only valid for the first branch (a stale incumbent). Test by shuffling the item order: the optimum value must not change.

---

## bipartite_matching

**Prove you got it**
1. Construct a graph where greedy assignment (take the first free right node) gets 2 matches but `max_matching` gets 3. Why does the augmenting path fix it?
2. Kuhn's algorithm is O(V*E). On what graph shape is it actually fast, and how would you demonstrate the difference by measurement?

**Try next**
- [S] Return the matching itself (pairs), not just its size. Invariant: no left or right node appears twice, every pair is an edge.
- [M] Add Hall's-theorem diagnostics: when no perfect matching exists, return a set S of left nodes with |N(S)| < |S| as the certificate. Oracle: brute-force all subsets of left nodes for n <= 12.
- [A] Compare against `max_flow_dinic::flow_matching` on 10^5 left nodes with average degree 5. Report speed-up and where Kuhn degrades; then try adding the "greedy initial matching" heuristic and measure again.

**Use in**: assigning reviewers to pull requests, workers to tasks with one-at-a-time constraints. Crate analogue: `kuhn_munkres` in [pathfinding](https://github.com/evenfurther/pathfinding) (weighted version).

**Pitfall**: forgetting to reset the `visited` array per left node; it silently turns the search into a wrong answer only on dense graphs.

---

## bit_tricks

**Prove you got it**
1. Write down, on paper, why `x & (x - 1)` clears the lowest set bit, then predict `isolate_lowest(0)` and `clear_lowest(0)` before running them.
2. `gosper` enumerates integers with k set bits in increasing order. What does it do when the next value would overflow the word width, and how does the recipe test that edge?

**Try next**
- [S] Add `select_in_word(x, k)` (position of the k-th set bit) with a loop oracle, then compare to the hardware `pdep` trick.
- [M] Implement a 64-bit bitboard knight-moves generator (shifts with file masks to avoid wrap-around). Oracle: coordinate-based generation, all 64 squares plus random occupancy.
- [A] Build a tiny bloom-style bitset with `u64` words and measure `count_ones` over 10^8 bits with the SWAR popcount versus `u64::count_ones` in release mode. Inspect the assembly (`cargo asm` or godbolt) and explain what the compiler turned the std call into.

**Use in**: chess engines (bitboards), roaring bitmaps, Morton codes for spatial indexes. See `bit_tricks` neighbours `data_structures/bitset_rank_select_roaring` and [roaring-rs](https://github.com/RoaringBitmap/roaring-rs).

**Pitfall**: shifts by the full width (`x << 64`) panic in debug and are UB-adjacent in other languages; Rust gives `checked_shl`. Test shift amounts 0, 63, 64.

---

## combinatorial_generation

**Prove you got it**
1. A Gray code changes exactly one bit per step. Write the assertion that checks it for `Gray` over 2^n values and say what a different ordering (plain counting) would change in the check.
2. `perm_rank`/`perm_unrank` must be inverse bijections onto 0..n!. How can you test that for n = 8 without storing all 40,320 permutations twice?

**Try next**
- [S] Add `k`-subsets in revolving-door order or colex order of your choice; assert count = C(n,k) and uniqueness via a `HashSet`.
- [M] Make a generator lazily skip: `nth(i)` for permutations in O(n^2) using unranking. Assert `iter().nth(i) == unrank(i)` for random i at n = 12.
- [A] Use the generators to build an exhaustive tester: enumerate all inputs up to size 7 (all arrays over {0,1,2}) and test your own sort or `dp_zoo::lis` against oracles. Count how many distinct bugs you can inject by hand and catch.

**Use in**: exhaustive test-case generation, puzzle solvers. Same patterns in [itertools](https://github.com/rust-itertools/itertools) (`permutations`, `combinations`).

**Pitfall**: yielding a `&mut Vec` or reused buffer while the caller collects it; every collected item then aliases. Know which generators clone and why.

---

## combo_algorithms

**Prove you got it**
1. In the O(1) LFU, which structure answers "what is the minimum frequency right now" and why does it never need a scan?
2. The Euler-tour + Fenwick subtree-sum trick reduces a tree query to a range query. State the invariant relating `tin`, `tout` and subtree membership.

**Try next**
- [S] Add `remove(key)` to the LFU. Oracle: `NaiveLfu` extended the same way, run on 20,000 random operations.
- [M] Make the LCA answer path queries "max edge weight on path(u,v)" with binary lifting. Oracle: walk-up naive on trees of up to 2,000 nodes.
- [A] Take a real tree (the directory tree of your `~/.cargo/registry`) and answer "total size under this directory after an update to one file" with Euler tour + Fenwick. Compare to recomputing and report both costs per update.

**Use in**: caches with frequency admission, hierarchy rollups (org charts, file sizes). Real crate: [moka](https://github.com/moka-rs/moka) (TinyLFU-style).

**Pitfall**: arena-based linked lists with stale indices after removal. Add a generation or a debug `check()` that walks every list.

---

## csr_graph

**Prove you got it**
1. After the counting-sort build, what exactly do `offsets[u]` and `offsets[n]` equal, and what test proves `neighbors(u)` slices never overlap?
2. Why does the recipe build `reverse()` instead of storing both directions up front, and what is the cost of PageRank "push" vs "pull"?

**Try next**
- [S] Keep each adjacency slice sorted and add `has_edge(u, v)` by binary search. Oracle: a `HashSet<(u32,u32)>`.
- [M] Add incremental updates through a small delta buffer plus periodic rebuild; define when you rebuild. Measure insert throughput against `Vec<Vec<u32>>` at 10^6 nodes and 5*10^6 edges.
- [A] Load a real edge list (a SNAP dataset such as a road network or web graph, downloaded by hand) into CSR, run BFS from 100 random sources, and report cache-miss behaviour via `perf stat` or wall-clock against `Vec<Vec>`. Explain with the memory layout.

**Use in**: read-mostly graph analytics, sparse matrix-vector products. Real crates: [petgraph `csr`](https://github.com/petgraph/petgraph), [sprs](https://github.com/sparsemat/sprs).

**Pitfall**: `u32` offsets overflow past 4.29 billion edges; decide the type on purpose and assert on build.

---

## dijkstra

**Prove you got it**
1. With lazy deletion the heap can hold more than V entries. Bound its maximum size and find an input (grid or dense) where you reach it.
2. Give the smallest graph on which Dijkstra gives a wrong answer once one edge weight is negative.

**Try next**
- [S] Return all shortest paths count (mod 1e9+7) alongside distances. Oracle: Floyd-Warshall plus path counting on graphs with <= 12 nodes.
- [M] Add early exit on target and a node-expansion counter; compare expansions with and without early exit on 1,000 random queries on a 200x200 grid with obstacles.
- [A] Download a road-network extract (e.g. a small DIMACS challenge graph), run 1,000 random queries, then implement A* with a landmark (ALT) heuristic and compare expansions. State when the heuristic is admissible and consistent.

**Use in**: any cheapest-route question (CI critical-path style cost, network routing). [petgraph::algo::dijkstra](https://github.com/petgraph/petgraph) is the production version.

**Pitfall**: comparing `f64` weights with `BinaryHeap` needs a total-order wrapper; NaN weights then break the heap silently. Reject them at input.

---

## dp_count_paths

Attempt your own FlowForge stage before this one; this recipe only shows the memo-vs-table SHAPE on a different domain.

**Prove you got it**
1. Both `routes_top_down` and `routes_bottom_up` compute the same numbers. Name the order requirement of the bottom-up version and what input makes top-down overflow the call stack.
2. Counts grow exponentially in a layered DAG. At what depth does `u64` overflow for a width-2 ladder, and how does the recipe (or you) detect it?

**Try next**
- [S] Count paths modulo 1e9+7 and also with checked arithmetic returning `None` on overflow. Test both against a brute-force path enumerator on small graphs.
- [M] Add "count paths passing through node x" using forward and backward counts. Oracle: enumerate all paths for graphs <= 15 nodes.
- [A] Apply the same DP to a different DAG: build the dependency DAG of a real Cargo workspace (`cargo metadata`) and count distinct dependency paths to the slowest-compiling crate. Report the number and the time.

**Use in**: counting build/lineage paths, combinatorial counts. Real code: [petgraph](https://github.com/petgraph/petgraph) topological iteration plus your own DP.

**Pitfall**: memoising on a mutable HashMap while recursing and forgetting the cycle case: a cyclic input recurses forever. Decide what the recipe should do for a cycle and test it.

---

## dp_edit_distance

**Prove you got it**
1. Fill the table for "kitten" vs "sitting" by hand; check your table equals the recipe's. Which cell gives the answer and which neighbour did each cell take its min from?
2. How much memory does the two-row version use, and what do you lose by using it (hint: reconstruction)?

**Try next**
- [S] Add transpositions (Damerau, restricted/OSA). Oracle: a recursive memoised definition on strings of length <= 8.
- [M] Implement a bounded version: stop and return `None` if distance exceeds k (banded DP, O(k*n)). Verify against the full DP on random strings, then measure at length 10^5 with k = 3.
- [A] Build the "did you mean" feature of a CLI: take your own shell command history, inject typos at rate 5%, and measure top-1 recovery accuracy using `suggest` with different tie-break rules.

**Use in**: friendly "unknown flag" errors (clap does this), dedup. Real crate: [strsim](https://github.com/rapidfuzz/strsim-rs).

**Pitfall**: byte distance versus char distance. Test with "e" vs "é" and with an emoji.

---

## dp_knapsack

**Prove you got it**
1. Why must the 1-D rolling-array version iterate capacity DOWNWARD, and which classic problem do you get by iterating upward?
2. Which entry of the table do you read for the answer, and how does the backtrack decide "item taken"?

**Try next**
- [S] Return all optimal item sets count (not just one). Oracle: brute force over 2^n subsets for n <= 20.
- [M] Capacities up to 10^9 make the table infeasible; implement the dual DP (min weight for each value) for small total value and choose automatically. Test on instances where both apply.
- [A] Use it for a real choice: pick a subset of crates to vendor under a size budget using real `.crate` sizes from `~/.cargo/registry` and made-up "importance" scores; compare to a greedy value/weight rule and report the worst gap you can find.

**Use in**: budgeted selection, resource-constrained packing. Close relative: the solvers inside [OR-Tools](https://github.com/google/or-tools).

**Pitfall**: capacity 0, zero-weight items, and items heavier than capacity: write these three tests first.

---

## dp_lcs_diff

**Prove you got it**
1. The LCS length equals (n + m - edit_distance_with_only_insert_delete) / 2. Verify this identity on 1,000 random pairs.
2. Why do two correct diffs of the same pair of files sometimes differ, and what property do all of them share?

**Try next**
- [S] Print unified-diff-style output with 3 lines of context. Oracle: apply your diff to A and assert you get B.
- [M] Reduce memory from O(nm) to O(min(n,m)) with Hirschberg's divide and conquer; assert it returns an LCS of the same length as the table version.
- [A] Implement Myers' O(ND) diff and compare speed with the DP on two real versions of a 5,000-line file from git history; report when each wins. Compare output with `git diff --minimal`.

**Use in**: showing what changed between two versions of a config or task list. Real crates: [similar](https://github.com/mitsuhiko/similar), [imara-diff](https://github.com/pascalkuthe/imara-diff).

**Pitfall**: diffing whole lines when the interesting changes are inside lines; decide the unit (line, word, char) on purpose.

---

## dp_memoization

**Prove you got it**
1. Find the smallest coin set and amount where `greedy` is wrong (a counterexample), and show both answers.
2. How deep does `min_coins_top_down` recurse for amount 10^6 with coin 1, and what happens to the stack?

**Try next**
- [S] Count the number of ways (combinations, then permutations) to make change; explain the loop-order difference with a 3-coin example.
- [M] Return the actual coins and prove minimality with a brute-force enumerator for amounts <= 60.
- [A] Reproduce the three DP styles (top-down HashMap, top-down Vec, bottom-up) on a problem of your own (e.g. minimum number of perfect squares summing to n) and benchmark each at n = 10^4 .. 10^6 with `criterion`-style repeated timing.

**Use in**: first-pass DP for any "minimum count" question. Compare with `interview/dp_and_backtracking_questions` (Coin Change).

**Pitfall**: using `usize::MAX` as infinity and then adding 1 to it. Use `Option` or a sentinel like `amount + 1`.

---

## dp_zoo

**Prove you got it**
1. For each shape in the table (LIS, matrix chain, TSP bitmask, weighted intervals, tree MIS, subset sum, digit DP), name the state in one line without looking.
2. `held_karp` is O(2^n n^2). For n = 16 how many table cells exist, and what does `tsp_oracle` cost?

**Try next**
- [S] Add the LIS reconstruction (the subsequence itself) and assert strictness and increasing order.
- [M] Add a new shape: counting numbers <= N with digit sum divisible by k, N up to 10^18, oracle for N <= 10^5.
- [A] Pick a problem you have not seen (e.g. "minimum cost to cut a rod into marked pieces"), classify it into a shape before coding, then write the DP and an oracle. Write down how you classified it.

**Use in**: recognising DP shapes in the wild (diffing, scheduling, route cost). Real code: the DP inside optimisers such as [rust-bio alignment](https://github.com/rust-bio/rust-bio) (sequence DP).

**Pitfall**: digit DP forgetting the "tight" and "leading zeros" flags; test numbers like 0, 10, 100.

---

## heuristics_zoo

**Prove you got it**
1. What guarantee does first-fit-decreasing give, and how does `check_heuristic` turn that guarantee into an assertion?
2. Why is MST-doubling a 2-approximation only on metric instances? Build a non-metric instance where it is bad.

**Try next**
- [S] Add the nearest-neighbour tour and plot (print) its ratio to optimal over 200 seeds for n = 9.
- [M] Add Or-opt to `two_opt` and report the improvement distribution on 100 random 200-point instances (mean and worst), relative to `held_karp` at n <= 14 and to the best seen at n = 200.
- [A] Take a real TSP dataset (TSPLIB `berlin52`, downloaded by hand), run annealing with 5 cooling schedules and report mean tour length over 30 seeds; compare to the known optimum 7542.

**Use in**: vehicle routing, bin packing in schedulers. Real: [OR-Tools](https://github.com/google/or-tools) routing uses local search metaheuristics.

**Pitfall**: judging a heuristic from one random seed. Always report a distribution.

---

## index_graph

Attempt your FlowForge stage before this one; the recipe uses BFS on a transit map precisely so it does not give the stage away.

**Prove you got it**
1. What is the type of `StopId` and why a newtype rather than a bare `usize`? Write a one-line misuse the compiler now rejects.
2. Interning maps names to dense ids once. Where would repeated `HashMap<String,_>` lookups show up in a profile of a hot BFS?

**Try next**
- [S] Add `reachable_within(start, k)` returning stops within k hops, sorted by id. Oracle: repeated relaxation (Bellman-Ford style) on small maps.
- [M] Add "avoid stop X" and "line changes cost extra" (0-1 BFS or Dijkstra, say which and why).
- [A] Build a map of a real transit feed (GTFS `stops.txt` + `stop_times.txt` from a small city), run 10,000 random trip queries and report time per query for the interned and the String-keyed versions.

**Use in**: any graph with named nodes. Real code: how [petgraph](https://github.com/petgraph/petgraph) uses `NodeIndex`.

**Pitfall**: intern table and adjacency vector growing out of sync when a node is first seen only as an edge target.

---

## max_flow_dinic

**Prove you got it**
1. Each edge is stored with a paired reverse edge at index `e ^ 1`. Explain why that single trick makes "undo flow" free, and what the residual capacity of the reverse edge means.
2. How do you extract the min cut from the final residual graph, and why is its capacity equal to the max flow?

**Try next**
- [S] Add `flow_on(edge)` and check conservation at every internal node.
- [M] Add lower bounds on edges (circulation with demands). Oracle: LP-free brute force on tiny graphs by enumerating integer flows <= 3 per edge.
- [A] Model a real selection problem: "which tasks to run given dependencies and profits" (`select_projects`) on a 2,000-task generated instance; compare Dinic with `edmonds_karp` for time and check both agree on the value.

**Use in**: assignment, project selection, image segmentation. Real: `dinic` and `edmonds_karp` in [pathfinding](https://github.com/evenfurther/pathfinding).

**Pitfall**: stack overflow in recursive blocking-flow on long chains; know why the recipe uses current-arc pointers and depth.

---

## mst_bridges

**Prove you got it**
1. Different tie-breaks give different MSTs but the same total weight. Produce a graph with two different MSTs and verify both with `kruskal` and `prim`.
2. State the low-link condition for an edge (u,v) to be a bridge, and for a vertex to be an articulation point; why does the root of the DFS need a special case?

**Try next**
- [S] Count the number of components after removing each bridge (the size of the two sides).
- [M] Add the second-best MST and verify by brute force over all spanning trees for n <= 7.
- [A] Take the dependency graph of a real project (undirected view) and list single points of failure (articulation points). Check your list by deleting each vertex and counting components with `union_find`.

**Use in**: network reliability and single-linkage clustering (`kruskal_clusters`). Real: `connected_components` and `articulation_points` in [petgraph](https://github.com/petgraph/petgraph).

**Pitfall**: multi-edges and self-loops in the low-link DFS (parent-edge skipped by node instead of by edge id).

---

## problem_modeling_reductions

**Prove you got it**
1. For each reduction in the file (water jug, 8-puzzle, 2-SAT, assignment, bipartite check, weighted intervals, Steiner), state "problem X is really problem Y because ...".
2. Why is IDA* used for the 15/8-puzzle instead of plain A*, and what does the heuristic need to be for correctness?

**Try next**
- [S] Add a new unsolvable-instance detector for the sliding puzzle (parity argument) and check it against the solver on 500 scrambles.
- [M] Reduce "can these meetings be scheduled in k rooms" to interval colouring and to max-flow; assert both match on random instances.
- [A] Pick one real problem you own (e.g. assign talks to rooms), write down the reduction BEFORE coding, and implement it. Include an oracle and a small counterexample search showing your modelling is faithful.

**Use in**: interview modeling, scheduling. Skiena's catalog is the reference.

**Pitfall**: modelling the wrong objective (min total vs min max). Check the reduction on 3 hand-made instances where the two objectives disagree.

---

## procedural_puzzles

**Prove you got it**
1. How does `assert_spanning_tree` prove a maze is perfect, and which of the four generators would fail it if you introduced an off-by-one in the wall carve?
2. Why does the Sudoku solver using exact cover (DLX) pick the column with the fewest ones?

**Try next**
- [S] Add a fifth maze generator (e.g. Aldous-Broder) and assert spanning tree property for 100 seeds.
- [M] Measure dead-end ratio and average solution length per generator over 500 mazes 40x40 and tabulate.
- [A] Implement Wave Function Collapse on a tiny tileset of your design, add a contradiction-recovery strategy, and report the contradiction rate per grid size. Reference: [gridbugs/wfc](https://github.com/gridbugs/wfc).

**Use in**: level generation, test-data generation, puzzle apps.

**Pitfall**: trusting "looks like a maze". Property checks (connected + acyclic) are the only reliable test.

---

## sat_solver_cdcl

**Prove you got it**
1. In 1UIP learning, what is the learned clause on a 3-literal conflict you draw by hand, and which decision level do you backjump to?
2. Why are watched literals faster than scanning every clause during propagation, and what invariant must two watches maintain?

**Try next**
- [S] Add an "UNSAT core size" statistic and print how many clauses were learned for `pigeonhole(6)` vs `pigeonhole(8)`.
- [M] Add clause deletion (keep the 50% lowest LBD); assert answers are unchanged on 500 random 3-SAT instances at the phase transition (ratio about 4.26, n = 100).
- [A] Encode package dependency resolution (versions, conflicts) as CNF and solve a set of 200 generated packages; compare your solver with [varisat](https://github.com/jix/varisat) on the same DIMACS files if you can run it, and read how [pubgrub](https://github.com/pubgrub-rs/pubgrub) explains failures.

**Use in**: dependency resolution, verification, planning.

**Pitfall**: forgetting to check the returned model against the original CNF (`check_model`); always verify SAT answers independently.

---

## shortest_path_zoo

**Prove you got it**
1. For each of Bellman-Ford, Floyd-Warshall, A*, bidirectional BFS and 0-1 BFS, give the weight condition and one input where another choice is wrong or slower.
2. How does Bellman-Ford report a negative cycle, and how would you extract the cycle's nodes?

**Try next**
- [S] Add an assertion that A* with `h = 0` equals Dijkstra and with Manhattan on a 4-connected grid is admissible.
- [M] Use Floyd-Warshall to detect currency arbitrage on 20 randomly generated exchange tables; oracle: Bellman-Ford with log weights.
- [A] Run bidirectional BFS versus plain BFS on a 1000x1000 maze, then on a social-graph-like random graph; report expanded nodes and explain why the savings differ.

**Use in**: pathfinding in games, routing, arbitrage. Real: [pathfinding](https://github.com/evenfurther/pathfinding) implements most of these.

**Pitfall**: running A* with an inconsistent heuristic and assuming each node is expanded once.

---

## sorting_networks_external_sort

**Prove you got it**
1. State the 0-1 principle and why testing 2^n binary inputs proves a network sorts all inputs. What does `zero_one_check` cost for n = 16?
2. Why does replacement selection produce runs of length about 2M on random input, and what does that save in an external sort?

**Try next**
- [S] Count comparators and depth of each generated network for n = 8, 16 and compare with the known optima you look up.
- [M] Implement a k-way merge with a tournament tree and count comparisons against the heap version.
- [A] Sort a 1 GB file of random `u64`s with a 64 MB memory budget using `external_sort`, validating with a streaming checker; compare to `sort` + mmap on your machine. Read how [extsort-rs](https://github.com/appaquet/extsort-rs) chunks input.

**Use in**: SIMD small sorts (see Voultapher's [sort-research-rs](https://github.com/Voultapher/sort-research-rs)), database run generation.

**Pitfall**: a network that sorts all permutations of 0..n but not inputs with duplicates; the 0-1 principle covers duplicates, a permutation-only test does not.

---

## sorting_searching

**Prove you got it**
1. Write the loop invariant of `lower_bound`; then explain which `<` vs `<=` mistake turns it into `upper_bound`.
2. Introsort limits recursion depth to about 2*log2(n). Craft the input that triggers the heapsort fallback and show it by counting depth.

**Try next**
- [S] Add a stability test: sort pairs by key only and assert equal keys keep input order for merge sort, but find an input where the quick-sort variant breaks it.
- [M] Implement a binary search on a floating-point answer with a fixed iteration count; prove termination and precision by checking against closed forms (sqrt, cube root).
- [A] Benchmark your radix sort vs `sort_unstable` for 10^6..10^8 u32 and u64 keys and string keys; find the crossover and compare to the std approach read in [Rust's slice sort sources](https://github.com/rust-lang/rust/tree/master/library/core/src/slice/sort).

**Use in**: "search on the answer" for capacity planning, binary search over time ranges in logs.

**Pitfall**: `(lo + hi) / 2` overflow with `usize`/`i32`; test with hi near `MAX`.

---

## string_algos

**Prove you got it**
1. What does `pi[i]` mean, and for the pattern `aabaaab` what is the whole array?
2. Rabin-Karp can return false positives. How does the recipe handle that and what modulus pitfalls exist (`mulmod`)?

**Try next**
- [S] Count occurrences of a pattern in O(n + m) with Z (concatenate pattern + separator + text). Oracle: `str::matches().count()` with overlap adjustments.
- [M] Compute the number of distinct substrings with the suffix array + LCP and compare with `naive_distinct` for lengths <= 200.
- [A] Index the text of a real book (Project Gutenberg) with a suffix array, answer 10^5 random substring queries via `sa_search`, and compare memory and time with `memchr::memmem` scans ([memchr](https://github.com/BurntSushi/memchr)).

**Use in**: text search, plagiarism, DNA (see [rust-bio](https://github.com/rust-bio/rust-bio)).

**Pitfall**: building the suffix array by sorting `&str` slices: O(n^2 log n) worst case on repetitive text. Measure it on `"a".repeat(50_000)`.

---

## tarjan_scc

Attempt your FlowForge cycle-detection stage first; Tarjan is the post-hoc generalisation.

**Prove you got it**
1. Why is the SCC condensation always a DAG? Give the one-line contradiction argument.
2. In the iterative version, what replaces the call stack frame, and what two pieces of state per node must be kept (index and lowlink)?

**Try next**
- [S] Print, for every SCC with more than one node or a self-loop, its members in id order. Oracle: pairwise reachability via BFS for graphs <= 30 nodes.
- [M] Build the condensation DAG, then count how many nodes can reach each SCC. Oracle: transitive closure by repeated BFS.
- [A] Run it on a 10^6-node random graph with 3*10^6 edges and a deep chain graph of 10^6 nodes; confirm no stack overflow and report time. Compare to Kosaraju (two passes) on the same inputs.

**Use in**: dependency-cycle diagnosis in module graphs. Real: [petgraph `tarjan_scc`](https://github.com/petgraph/petgraph).

**Pitfall**: using the discovery time of the wrong node to update lowlink for nodes already assigned to a component (must check `on_stack`).

---

## union_find

**Prove you got it**
1. Show with a measurement that path halving alone or union by size alone each give most of the speed-up. What is the worst case without either?
2. `connected(a, b)` takes `&mut self`. Why, and what would a `&self` version cost?

**Try next**
- [S] Add `size(x)` beside the group count; assert that the sizes of all groups add up to `n`.
- [M] Add rollback (union without path compression, with an undo stack) and use it in an offline dynamic-connectivity toy. Oracle: recompute components from scratch after each op.
- [A] Use union-find to solve image segmentation by region merging on a PNG you generate (`graphics/png_ppm_writer`): union neighbouring pixels with colour distance under a threshold and report component count versus threshold.

**Use in**: grouping connected items, image regions, merging matched records, percolation. Real: `UnionFind` in [petgraph](https://github.com/petgraph/petgraph).

**Pitfall**: recursive `find` blowing the stack on a chain of 10^6 nodes created by unlucky unions; make it iterative and test the chain.

---

## algorithmic_optimization_toolbox

**Prove you got it**
1. The asserts are on operation counters, not wall-clock. Pick two pairs (e.g. `sliding_max_naive` vs `sliding_max_deque`) and state the counter formula each should follow as a function of n and k.
2. For the meet-in-the-middle subset-sum pair (`subsets_naive` / `subsets_mitm`), how does 2^n become about 2^(n/2) and what do you pay in memory?

**Try next**
- [S] Add a new naive/optimised pair of your own (e.g. count inversions by O(n^2) loop vs merge sort) with counters, and assert the gap at n = 10^3 and 10^4.
- [M] Replace the counter by a simulated cache or by `perf stat` instruction counts for one pair and report whether the counter predicted the measured ratio within 2x.
- [A] Take a genuinely slow function from one of your own projects, find which idea in this file applies (prefix sums, two pointers, deque, divide and conquer, Knuth optimisation, convex hull trick, bitset shift-or), and write the before/after with an oracle test.

**Use in**: the "passes small tests, times out on large" moment. Real code: the [Rust sort sources](https://github.com/rust-lang/rust/tree/master/library/core/src/slice/sort) are a long study in choosing the right asymptotic idea and then tuning.

**Pitfall**: optimising before measuring which counter dominates; also forgetting that the optimised version has a bigger constant, so small n can favour the naive one.

---

## amortized_analysis_online_algorithms

**Prove you got it**
1. Doubling a dynamic array copies fewer than 2n elements in total, while growth by +k copies about n^2/(2k). Verify both by running `DynArray` and read the potential `phi` value after each push.
2. In ski rental, what is the competitive ratio of the deterministic break-even strategy, and why can no deterministic online strategy beat it?

**Try next**
- [S] Change the dynamic array growth factor to 1.5 and 3 and report total copies and wasted capacity for n = 10^6.
- [M] Add a splay tree access sequence test: show the sequential access lemma (n accesses in sorted order cost O(n) total) by counting rotations.
- [A] Evaluate LRU, FIFO and your own marking algorithm against Belady on three traces (random, looping, adversarial); show an adversarial sequence where LRU faults on every request when the cache has k pages and k+1 distinct pages are used cyclically.

**Use in**: justifying `Vec`/`HashMap` growth, cache policy choice, rent-or-buy cloud decisions. Real code: std [Vec](https://github.com/rust-lang/rust/blob/master/library/alloc/src/vec/mod.rs) growth policy; compare with `data_structures/cache_policy_zoo`.

**Pitfall**: claiming amortised O(1) for a structure with a persistent or shared version, where an expensive operation can be repeated on the same state.

---

## approximation_randomized_algorithms

**Prove you got it**
1. For maximal-matching vertex cover, why is the cover at most 2 times optimal? Verify the asserted ratio on the random instances and find the worst ratio observed.
2. Miller-Rabin is a Monte Carlo algorithm. For a composite n, what is the per-round error bound and how many rounds does the recipe use?

**Try next**
- [S] Print the empirical distribution of the ratio (greedy set cover / optimum) over 1,500 instances and compare with the harmonic bound H(d).
- [M] Run the randomised min-cut (Karger) with a number of repetitions you derive for failure probability under 1e-3; verify with Stoer-Wagner or brute force on graphs up to 12 nodes.
- [A] Apply Freivalds' check to verify 500x500 matrix products and measure the false-accept rate empirically with a deliberately wrong product; compare cost against recomputing.

**Use in**: prime generation, verifying expensive pipelines, scheduling. Real: [num-prime](https://github.com/cmpute/num-prime) for primality, [OR-Tools](https://github.com/google/or-tools) for heuristics with bounds.

**Pitfall**: running a randomised test with a fixed seed in production and treating the result as independent trials.

---

## bit_manipulation_real_world

**Prove you got it**
1. For the CIDR longest-prefix match, write the mask for /19 by hand and explain why `PrefixTrie` and a linear scan give the same result.
2. Explain zigzag encoding of -1, 1, -2 as numbers and why varints would waste space on negatives without it.

**Try next**
- [S] Add `Ipv6`-style 128-bit prefixes using `u128` and test containment against a bit-by-bit oracle.
- [M] Add a Snowflake-style id generator with a clock going backwards: define the policy (wait, error, or logical increment) and test it with a fake clock.
- [A] Implement a protobuf-style varint codec for a real message schema (a log record) and compare size and speed against a fixed-width layout on 10^6 records; see `encoding_compression/encodings_codecs`.

**Use in**: permission flags, routers, id generators, bitboards. See `algorithms/bit_tricks` for the toolbox and [BurntSushi/memchr](https://github.com/BurntSushi/memchr) for SIMD-in-register techniques.

**Pitfall**: signed shifts and sign extension (`sext`) when unpacking fields.

---

## greedy_and_interval_algorithms

**Prove you got it**
1. For interval scheduling, which of earliest-start, shortest-first and earliest-finish is correct? Run the recipe's counterexample finder and write down the minimal counterexample it finds for each wrong rule.
2. State the "stays ahead" argument for earliest finish in two sentences, then the exchange argument for Huffman.

**Try next**
- [S] Add the closed-versus-open interval toggle to a booking checker and write the 4 boundary cases (touching at one point) with expected answers.
- [M] Implement weighted interval scheduling with DP and show the greedy-by-weight failure; oracle: brute force over subsets for n <= 15.
- [A] Build a meeting-room allocator over a real calendar export (ICS) and verify against `rooms_sweep` depth; compare to `interview/stacks_queues_heaps_questions` Q259.

**Use in**: schedulers, calendars, Huffman coding (`encoding_compression/compression_lz77_huffman`). Real: [petgraph](https://github.com/petgraph/petgraph) Kruskal-style greedy MST.

**Pitfall**: sorting by the wrong key or breaking ties inconsistently; test with many equal start or end times.

---

## kway_merge_family

**Prove you got it**
1. Why does a loser tree need about log k comparisons per output while a binary heap needs about 2 log k? Reproduce with the recipe's counters for k = 4, 16, 256.
2. In LSM compaction, what does "newest wins" require of the tie-break, and how does `KMerge` make it stable?

**Try next**
- [S] Add tombstone handling in `lsm_compact` (delete markers dropped only at the bottom level) with an oracle that applies the writes to a `BTreeMap` in order.
- [M] Implement merge path partitioning to split merging of two arrays across 4 threads; assert the concatenation equals the sequential merge.
- [A] Merge 64 sorted files of 10^6 records each from disk with block buffers; report IO counts for different buffer sizes and relate to `algorithms/sorting_networks_external_sort`.

**Use in**: LSM compaction, external sort, log merging. Real: [extsort-rs](https://github.com/appaquet/extsort-rs) and std `BinaryHeap`-based merges; related interview file `interview/stacks_queues_heaps_questions` (Q256).

**Pitfall**: not stabilising ties by source index, so equal keys swap order between runs.

---

## linear_programming_simplex

**Prove you got it**
1. On Beale's example, which pivot rule cycles and how does the recipe detect it? Why does Bland's rule terminate?
2. State strong duality in one sentence and show it on `check_certificates` output (primal optimum equals dual optimum).

**Try next**
- [S] Add a sensitivity query: by how much can a right-hand side change before the basis changes (`rhs_range`)? Verify by re-solving at the endpoints.
- [M] Solve a 6x6 assignment as an LP and compare with `bipartite_matching` and `max_flow_dinic`; explain why the LP has integer vertices.
- [A] Model a real diet or production-planning problem with at least 15 variables, solve with your simplex and check against a known solver's answer (e.g. `scipy.optimize.linprog` run once by hand); report iterations for both pivot rules.

**Use in**: resource allocation, relaxations in MIP. Real: [OR-Tools](https://github.com/google/or-tools) (GLOP simplex).

**Pitfall**: floating-point tolerance in the ratio test; a pivot on a tiny element destroys accuracy.

## Chapter projects

### Small (about 2 hours): "diff and did-you-mean"
Combine `dp_lcs_diff`, `dp_edit_distance`, and `dev_tools/testing_patterns`.
- Milestones: (1) line diff of two text files; (2) word-level diff inside changed lines; (3) a `suggest` for misspelled sub-commands.
- Acceptance: applying the diff to A yields B on 10,000 random line-edit scripts; `suggest` returns the true command for 95% of single-typo inputs.
- Stretch: Hirschberg memory reduction; Myers diff and a timing table.

### Intermediate (about a weekend): "route planner on a real map"
Combine `csr_graph`, `dijkstra`, `shortest_path_zoo` (A*, bidirectional), `data_structures/heap_patterns` (d-ary and indexed heap) and `graphics/png_ppm_writer` to render the explored region.
- Milestones: (1) load a road graph into CSR; (2) Dijkstra with early exit; (3) A* with ALT landmarks; (4) PPM/PNG picture of expanded nodes; (5) a benchmark table.
- Acceptance: all algorithms return the same distance on 1,000 random queries (Dijkstra is the oracle); A* expands under 30% of Dijkstra's nodes on average.
- Stretch: contraction hierarchies; compare with [pathfinding](https://github.com/evenfurther/pathfinding).

### Advanced (about a week): "dependency resolver"
Combine `sat_solver_cdcl`, `problem_modeling_reductions`, `tarjan_scc` (to report cycles), `max_flow_dinic` (or matching) for an alternative formulation, plus `text_and_parsing/json_parser` and `errors/diagnostics_spans` for manifests and error messages.
- Milestones: (1) manifest parser; (2) CNF encoding with at-most-one per package; (3) solving with CDCL; (4) human-readable conflict explanation from an UNSAT core; (5) preference for newest versions via iterative solving.
- Acceptance: for 500 generated universes, the solution passes an independent checker, and UNSAT answers agree with a brute-force solver on universes of <= 12 packages.
- Stretch: compare your explanations with [pubgrub](https://github.com/pubgrub-rs/pubgrub); benchmark against [varisat](https://github.com/jix/varisat).
