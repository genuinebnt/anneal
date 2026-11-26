# Practice: interview

How to use this page: each section is one file in `examples/interview/`; question numbers (Q1..Q298) are the rows of `examples/interview/QUESTIONS.md`.
Each file gets five deep questions (prove it, three tasks, pitfall) and one-line follow-ups for the rest. Practise by closing the file, restating the question aloud, coding it with a timer (25 minutes), and only then comparing with the recipe.
Always write the brute-force oracle first and fuzz your fast answer against it with 10^4 seeded random small inputs; interviewers love hearing "I would test it against brute force".
No solutions are given. Rust-concept questions (the `*_qa` files) are practised by writing a 20-line compiling or failing program that demonstrates the answer.

---

## arrays_strings_hashmap_questions

Use in: phone screens and log/text processing. Same ideas in real code: sliding windows and prefix sums appear in metrics libraries; see [itertools](https://github.com/rust-itertools/itertools) for window/chunk helpers.

### Deep: Q13 Subarray Sum Equals K
**Prove you got it**: (1) Why does the map need the entry `{0: 1}` before the loop? (2) What goes wrong if the array has negative numbers and you try a sliding window instead?
**Try next**: [S] return all (i, j) pairs, oracle O(n^2) double loop. [M] count subarrays whose sum is divisible by k (handle negative remainders) on 10^6 elements. [A] run on a real series (daily commit counts from `git log`) to find all windows with exactly N commits.
**Pitfall**: `HashMap<i32, usize>` overflow when sums exceed i32; use i64 and test `[i32::MAX, i32::MAX]`.

### Deep: Q16 Minimum Window Substring
**Prove you got it**: (1) State the two counters (`need`, `formed`) and the invariant for the left pointer move. (2) Why is it O(n + m) although there are nested loops?
**Try next**: [S] return all minimal windows. [M] support Unicode text by chars versus bytes; oracle: O(n^2) check each substring. [A] build a "find shortest log excerpt containing all of these keywords" tool and run it on a 100 MB file with a streaming window.
**Pitfall**: slicing a `&str` by byte index inside a multi-byte char; keep indices from `char_indices`.

### Deep: Q18 Longest Palindromic Substring (expand and Manacher)
**Prove you got it**: (1) Why does the transformed string with separators remove even/odd special cases? (2) Why is Manacher linear (what do the centre and right boundary guarantee)?
**Try next**: [S] count all palindromic substrings and compare to O(n^3) oracle. [M] return the shortest string to add at the front to make a palindrome (KMP or hashing). [A] benchmark expand-around-centre vs Manacher on `"a".repeat(10^5)` and random text.
**Pitfall**: returning byte ranges that are not valid UTF-8 boundaries.

### Deep: Q27 Binary search variants and search on the answer
**Prove you got it**: (1) Write the invariant for `lower_bound` in one sentence. (2) For Koko Eating Bananas, what monotonic predicate are you searching?
**Try next**: [S] sqrt(x) to integer floor with i64 overflow tests. [M] split array largest sum (binary search over answer plus greedy check) with brute-force oracle for n <= 12. [A] apply search-on-answer to a real capacity question: minimum number of workers to finish a task list within a deadline.
**Pitfall**: infinite loop with `mid = lo + (hi-lo)/2` when you move `lo = mid`; use the upper-mid trick or half-open ranges.

### Deep: Q24 First Missing Positive, Majority Element, Single Number
**Prove you got it**: (1) Why does swapping each value to index v-1 take O(n) total? (2) Why does Boyer-Moore need a verification pass in general?
**Try next**: [S] Single Number II with bit counting mod 3. [M] find all elements appearing more than n/3 times (two candidates). [A] implement a streaming Misra-Gries and compare with `data_structures/count_min_sketch` on the same stream.
**Pitfall**: mutating the input in place when the interviewer says read-only.

One-line follow-ups:
- Q1 Two Sum: return all pairs for duplicates; prove O(n) with one pass.
- Q2 Two Sum II/IV: two pointers on sorted; do it on a BST with two iterators.
- Q3 3Sum/4Sum: skip duplicates correctly; count of unique triplets versus brute force.
- Q4 Anagrams: sort versus count array; Unicode?
- Q5 Top K: bucket sort versus heap; stable tie-breaking.
- Q6 Product except self: do it with zero(s) in input.
- Q7 Kadane: return indices; circular follow-up with total minus min subarray.
- Q8 Max product subarray: track max and min.
- Q9 Stock I-IV: k transactions DP table versus state machine.
- Q10 Intervals: sort by start; test touching intervals `[1,2],[2,3]`.
- Q11 Rotate/move zeroes/dedupe in place: reversal proof.
- Q12 Water: two-pointer proof of why moving the lower side is safe.
- Q14 Longest consecutive: only start counting at sequence starts.
- Q15 Longest unique substring: last-seen array of 256 versus map.
- Q17 Replacement/permutation in string: fixed window counters.
- Q19 atoi: overflow, whitespace, sign; fuzz against `i64::from_str` where defined.
- Q20 Roman: subtractive forms; validate range 1..3999.
- Q21 Strings: KMP strStr (see `algorithms/string_algos`); encode/decode length-prefix.
- Q22 Matrix: rotate in place by transpose + reverse.
- Q23 Rotated array search: duplicates break O(log n); show a case.
- Q25 Bits: Count Bits DP; sum without plus handles negative via two's complement.
- Q26 3Sum closest and next permutation: compare to `algorithms/combinatorial_generation`.

---

## binary_trees_in_rust

Use in: ASTs, file-system trees, expression evaluators. Real code: std [BTreeMap node](https://github.com/rust-lang/rust/blob/master/library/alloc/src/collections/btree/node.rs) is a tree with a very different ownership design; read it after you build yours.

### Deep: Q36 Validate a BST
**Prove you got it**: (1) What is the counterexample where parent-only checks pass but the tree is invalid? (2) What is the bounds type when keys can be `i32::MIN`?
**Try next**: [S] iterative via inorder with previous value. [M] validate a BST stored in an arena (`Vec<Node>`), also detecting cycles. [A] generate 10^4 random trees from random insert sequences and mutate one value; show the validator catches every mutation that breaks order.
**Pitfall**: using `i32::MIN` and `MAX` as sentinels; use `Option<&K>` bounds.

### Deep: Q38 Lowest Common Ancestor (general and BST)
**Prove you got it**: (1) Why does the post-order version return a node when it finds either target? (2) What if one target is absent?
**Try next**: [S] LCA with parent pointers via `Weak`. [M] binary lifting for 10^5 queries on 10^5 nodes; oracle: naive walk-up. [A] reuse in `algorithms/combo_algorithms` Euler tour LCA and compare.
**Pitfall**: assuming both nodes exist.

### Deep: Q40 Serialize and deserialize a binary tree
**Prove you got it**: (1) Why is preorder plus null markers sufficient? (2) What is the size overhead?
**Try next**: [S] level-order format. [M] compact binary format with varint lengths. [A] fuzz `deserialize` with random bytes: never panic, always `Result`.
**Pitfall**: recursion depth on a 10^5-chain tree (see Q51).

### Deep: Q34 Diameter and maximum path sum
**Prove you got it**: (1) What does the helper return versus what it records globally? (2) Why can the best path bend at a node but the returned value cannot?
**Try next**: [S] with edge weights. [M] return the actual path. [A] compute the diameter of a real directory tree (`walkdir`-style) and compare to brute-force all pairs on small trees.
**Pitfall**: negative values, so the answer is not 0 for an all-negative tree.

### Deep: Q51 Stack overflow on a degenerate tree
**Prove you got it**: (1) At what depth does your recursive function overflow on your machine? Measure it. (2) What does `Drop` do for a 10^6-deep `Option<Box<Node>>`?
**Try next**: [S] iterative inorder with explicit stack. [M] write an iterative `Drop` and prove with a test at 10^6 nodes. [A] Morris traversal (O(1) extra space) versus the explicit-stack version on a 10^6-node chain and a balanced tree; report time and peak memory.
**Pitfall**: forgetting that the default derived `Drop` is recursive.

One-line follow-ups:
- Q28 Traversals: Morris restores the tree; assert tree unchanged afterwards.
- Q29 Level order: use `VecDeque`; zigzag by reversing alternate levels.
- Q30 Depth: min depth counts leaves only.
- Q31 Invert: iterative with stack; verify double invert is identity.
- Q32 Same/symmetric/subtree: serialize-and-search is wrong without null markers.
- Q33 Balanced: single pass returning -1 sentinel or `Option`.
- Q35 Path sum II: backtracking with push/pop; copy only at leaf.
- Q37 Kth smallest: augment with sizes for repeated queries.
- Q39 BST delete: three cases; fuzz with `BTreeSet`.
- Q41 Build from preorder + inorder: index map gives O(n).
- Q42 Sorted array to BST: choose middle consistently.
- Q43 Flatten: Morris-style pointer rewiring.
- Q44 Count complete tree nodes: O(log^2 n); verify against O(n).
- Q45 Tree to circular list: relink inorder.
- Q46 Vertical order: ties by row then value.
- Q47 Recover BST: find two inversions in inorder.
- Q48-Q50 Rust borrow patterns, `Rc<RefCell>` and `Weak` parents: write a panicking double `borrow_mut` and explain.

---

## dp_and_backtracking_questions

Use in: scheduling, text diffing, constraint solving. Pair with `algorithms/dp_zoo`, `algorithms/dp_memoization`.

### Deep: Q54 Coin Change I and II
**Prove you got it**: (1) Why does loop order (coins outer vs amount outer) decide combinations versus permutations? (2) What is the counterexample for greedy?
**Try next**: [S] reconstruct coins used. [M] bounded counts per coin (multiple knapsack with binary splitting). [A] check against `algorithms/dp_memoization` implementation on 10^4 random instances.
**Pitfall**: infinite sentinel overflow.

### Deep: Q67 Regex and wildcard matching
**Prove you got it**: (1) Define the state `dp[i][j]` and the `*` transition. (2) Why can wildcard matching be greedy but regex `*` not?
**Try next**: [S] add `+` and `?`. [M] compare with an NFA simulation; oracle: the [regex](https://github.com/rust-lang/regex) crate on translated patterns. [A] link with `capstones/regex_nfa_engine`.
**Pitfall**: `a*` matching empty prefix initialisation of row 0.

### Deep: Q69 Burst Balloons (interval DP)
**Prove you got it**: (1) Why choose the LAST balloon to burst instead of the first? (2) What is the iteration order by length?
**Try next**: [S] reconstruct order. [M] oracle by permutations for n <= 8. [A] adapt the pattern to matrix-chain multiplication (see `algorithms/dp_zoo`).
**Pitfall**: padding with 1 on both ends.

### Deep: Q77 N-Queens with bitmasks
**Prove you got it**: (1) Which three masks, and how do they shift per row? (2) Count for n = 8 is 92; for n = 12?
**Try next**: [S] boards output. [M] symmetry halving. [A] compare to `algorithms/sat_solver_cdcl` encoding speed (`queens_cnf`).
**Pitfall**: shifts overflowing the mask width; AND with the full mask.

### Deep: Q79 Sudoku solver
**Prove you got it**: (1) Why choose the cell with the fewest candidates? (2) How do bitmasks make candidate lookup O(1)?
**Try next**: [S] detect multiple solutions. [M] generate puzzles with unique solutions. [A] compare with the DLX solver in `algorithms/procedural_puzzles`.
**Pitfall**: not undoing state on backtrack.

One-line follow-ups:
- Q52 Stairs: rolling variables. Q53 House Robber I/II: break the cycle in two linear runs.
- Q55 LIS: n log n with patience tails; reconstruct.
- Q56 LCS: rolling rows versus reconstruction. Q57 Edit distance: compare `algorithms/dp_edit_distance`.
- Q58 Word break: memo plus trie. Q59 Decode ways: zeros.
- Q60 Grid DP with obstacles. Q61 Maximal square: min of three neighbours.
- Q62 Subset-sum family via bitset (`dp_zoo::Bits`). Q63 Palindromic subsequence via LCS with reverse.
- Q64 Cooldown: state machine. Q65 Jump game: greedy proof.
- Q66 Max product subarray: DP of max and min. Q68 Interleaving/distinct subsequences.
- Q70 Egg drop: the k-log-n idea. Q71 Triangle, perfect squares.
- Q72-Q76 Subsets, permutations, combination sums: dedupe by sorted skipping; Catalan counts for parentheses.
- Q78 Word search: in-place marking; Q80 K equal subsets: bitmask DP versus backtracking pruning.

---

## graphs_and_union_find_questions

Use in: routing, clustering, grid problems. Pair with `algorithms/union_find`, `algorithms/dijkstra`, `algorithms/mst_bridges`.

### Deep: Q88 Word Ladder (bidirectional BFS)
**Prove you got it**: (1) Why does bidirectional BFS expand fewer nodes, and when does it not? (2) How do wildcard buckets (`h*t`) avoid O(n^2) neighbour checks?
**Try next**: [S] return the path. [M] return all shortest paths with a parent DAG; oracle: DFS enumeration on small dictionaries. [A] run on a real 5-letter word list; measure expansion counts, relate to `algorithms/shortest_path_zoo`.
**Pitfall**: swapping the smaller frontier but forgetting to merge visited sets correctly.

### Deep: Q97 Evaluate Division (weighted union-find)
**Prove you got it**: (1) What invariant relates `weight[x]` to its root, and how does path compression update it? (2) What do you return for unknown variables?
**Try next**: [S] cycle contradictions detection. [M] compare to Floyd-Warshall on logs. [A] build currency conversion with inconsistency detection (arbitrage) and relate to `shortest_path_zoo`.
**Pitfall**: floating-point equality for contradiction check.

### Deep: Q99 Cheapest Flights Within K Stops
**Prove you got it**: (1) Why must Bellman-Ford copy the distance array each round? (2) Why does plain Dijkstra fail here?
**Try next**: [S] return the route. [M] Dijkstra on (node, stops) states. [A] compare time on a 10^5-edge random graph.
**Pitfall**: updating in place which lets a path use more than k edges.

### Deep: Q102 Critical Connections (bridges)
**Prove you got it**: (1) State the low-link rule. (2) Why handle the parent edge by id on multi-edges?
**Try next**: [S] articulation points. [M] iterative version proving no stack overflow on a path of 10^6 nodes. [A] check bridges by removing each edge and counting components with union-find.
**Pitfall**: recursion depth.

### Deep: Q104 Reconstruct Itinerary (Hierholzer)
**Prove you got it**: (1) Why is the answer built in reverse post-order? (2) When does an Eulerian path exist (degree conditions)?
**Try next**: [S] Eulerian circuit in undirected graphs. [M] lexicographically smallest path with heaps. [A] de Bruijn sequence via Eulerian cycle and compare with `algorithms/combinatorial_generation`.
**Pitfall**: removing edges with `Vec::remove(0)` (quadratic).

One-line follow-ups:
- Q81 Islands: DFS stack, BFS, union-find; compare. Q82 Flood fill: same-colour start.
- Q83 Clone graph: arena versus `Rc<RefCell>`. Q84 Rotting oranges: level counting.
- Q85 01 Matrix: multi-source BFS. Q86 Pacific/Atlantic: reverse search.
- Q87 Surrounded regions: border flood. Q89 Binary matrix 8 directions.
- Q90 Open the lock: implicit graph. Q91 Snakes and ladders: board numbering.
- Q92 Shortest bridge: DFS then BFS. Q93 Bipartite: 2-colouring; odd cycle certificate.
- Q94 Components and valid tree: edges = n-1 plus connected. Q95 Redundant connection.
- Q96 Accounts merge. Q98 Network delay: Dijkstra. Q100 Minimax Dijkstra/binary search.
- Q101 Prim versus Kruskal. Q103 Town judge. Q105 Graph representations and `checked_add_signed` neighbour helper.

---

## implement_std_from_scratch

Use in: understanding std; building custom collections. Read the real thing after: std [Vec](https://github.com/rust-lang/rust/blob/master/library/alloc/src/vec/mod.rs), [LinkedList](https://github.com/rust-lang/rust/blob/master/library/alloc/src/collections/linked_list.rs), [Cow](https://github.com/rust-lang/rust/blob/master/library/alloc/src/borrow.rs).

### Deep: Q106 Implement MyVec<T>
**Prove you got it**: (1) What is the growth policy and why amortised O(1)? (2) How do you drop exactly each element once after a panic in the middle of `insert`?
**Try next**: [S] add `shrink_to_fit`. [M] zero-sized types support; test with `()` and `[u8; 0]`. [A] run under Miri (`cargo +nightly miri test`) with 10^4 random operations compared against `Vec`.
**Pitfall**: `ptr::write` over an initialised slot leaks; `ptr::read` then double-drops.

### Deep: Q108 MyRc and MyWeak
**Prove you got it**: (1) When is the value dropped versus the allocation freed? (2) Why `!Send`?
**Try next**: [S] `Rc::ptr_eq` and `get_mut`. [M] make an `MyArc` with atomics and justify orderings (see `concurrency/own_arc_refcount`). [A] build a cycle and use Weak to break it; verify with a drop counter.
**Pitfall**: upgrade when the strong count is 0.

### Deep: Q111 MyHashMap with chaining and `&str` lookup
**Prove you got it**: (1) Which trait bound lets `HashMap<String, V>::get(&str)` work? (2) When do you resize and why does it rehash?
**Try next**: [S] `entry` API. [M] differential test vs `HashMap` with 10^5 ops. [A] swap in open addressing from `data_structures/open_addressing_map` and compare.
**Pitfall**: iterating while resizing.

### Deep: Q115 Spinlock MyMutex
**Prove you got it**: (1) Which orderings on lock and unlock and why? (2) Why can spinning waste CPU and how would you improve (hint)?
**Try next**: [S] `try_lock`. [M] loom-style exhaustive test idea (see `concurrency/memory_ordering_litmus`). [A] benchmark versus `std::sync::Mutex` at 1..16 threads.
**Pitfall**: forgetting `Send/Sync` impl bounds (`T: Send`).

### Deep: Q113 Iterator adapters Map, Filter, Zip, Take, Chain, Peekable
**Prove you got it**: (1) Why are adapters lazy and zero-cost? (2) What does `size_hint` do?
**Try next**: [S] `Skip`, `StepBy`. [M] implement `DoubleEndedIterator` for Map. [A] compare generated assembly of a chained pipeline versus a hand loop; read std's [Map adapter](https://github.com/rust-lang/rust/blob/master/library/core/src/iter/adapters/map.rs).
**Pitfall**: Peekable and side-effecting closures.

One-line follow-ups:
- Q107 MyBox: why `*b` can move out of a real Box. Q109 Queue from two stacks: amortised proof.
- Q110 Linked stack with iterative Drop (compare `unsafe_ffi/nonnull_linked_list`).
- Q112 MyString: UTF-8 invariant. Q114 Option combinators.
- Q116 Scoped threads. Q117 Cow. Q118 Error enum with `source` (`errors/error_enum`).
- Q119 Registry of closures. Q120 Matrix Display with precision. Q121 Complex operators and `Sum`.

---

## linked_lists_in_rust

Use in: undo stacks, LRU-style deques. Read [Learn Rust With Entirely Too Many Linked Lists](https://rust-unofficial.github.io/too-many-lists/) alongside.

### Deep: Q122 Reverse a singly linked list
**Prove you got it**: (1) Write the three-pointer loop invariant. (2) Why does the recursive version overflow at 10^5?
**Try next**: [S] reverse a prefix. [M] oracle: collect to Vec, reverse, compare, on 10^4 random lists. [A] benchmark iterative reverse for 10^7 nodes and discuss cache behaviour versus `Vec`.
**Pitfall**: `Option::take` misuse leaving a dangling copy.

### Deep: Q126 Merge k sorted lists
**Prove you got it**: (1) Compare heap and divide-and-conquer in comparisons. (2) What ordering wrapper does `BinaryHeap` need for list nodes?
**Try next**: [S] stability. [M] count comparisons for both and plot against k. [A] merge 100 sorted files lazily (see `data_structures/heap_patterns::k_way_merge`).
**Pitfall**: cloning nodes instead of moving them.

### Deep: Q136 Cycle detection (Floyd) and why Box cannot make a cycle
**Prove you got it**: (1) Prove the meeting point math for finding the cycle start. (2) Why is a cycle impossible with unique-owning `Box`?
**Try next**: [S] cycle length. [M] same in an arena-list. [A] apply Floyd/Brent to detect cycles in a pseudo-random function iteration (compare with `math_science/number_theory_primes` Pollard-Brent).
**Pitfall**: only checking `fast.next` and not `fast.next.next`.

### Deep: Q139 Dropping a 1,000,000-node list
**Prove you got it**: (1) Where is the recursion in derived Drop? (2) What does a manual iterative Drop do with `take`?
**Try next**: [S] reproduce the overflow. [M] fix and test. [A] same for a binary tree (Q51).
**Pitfall**: fix only the outer list but not nested structures.

### Deep: Q141 Doubly linked list with Rc<RefCell> and Weak
**Prove you got it**: (1) Which link is Weak and why? (2) How can borrow_mut panic at runtime?
**Try next**: [S] iterator. [M] Miri-clean test. [A] arena rewrite (Q142) and benchmark, relating to `data_structures/lru_cache`.
**Pitfall**: reference cycles leaking.

One-line follow-ups:
- Q123 k-group reversal. Q124 reverse m..n. Q125 merge two lists (dummy head).
- Q127 middle node slow/fast. Q128 nth from end with gap pointers.
- Q129 delete duplicates. Q130 partition. Q131 rotate by k with length modulo.
- Q132 palindrome with O(1) space (restore the list). Q133 add two numbers with carry.
- Q134 odd-even regroup. Q135 intersection (swap heads trick).
- Q137 deep copy with random pointer using an interleave trick or map.
- Q138 flatten multilevel. Q140 generic stack with iter/iter_mut/into_iter. Q142 arena DLL and when `NonNull` is warranted.

---

## rust_async_qa

Use in: network services. Read: [tokio tutorial and source](https://github.com/tokio-rs/tokio). Pair with `async/` recipes.

### Deep: Q152 Cancel safety in select!
**Prove you got it**: (1) Give a cancel-unsafe example (a `read_exact` into a partial buffer) and why data is lost. (2) What does `tokio::sync::mpsc::Receiver::recv` promise?
**Try next**: [S] demonstrate with a test that drops a future mid-await. [M] fix with a state struct kept outside the select. [A] add a stress test with random cancellation (`async/async_cancel_safety`).
**Pitfall**: assuming `select!` branches are fair (they are randomised by default).

### Deep: Q149 Blocking inside async
**Prove you got it**: (1) How does one `thread::sleep` stall other tasks on a current-thread runtime? (2) What do `spawn_blocking` and `block_in_place` change?
**Try next**: [S] measure tail latency with and without. [M] a watchdog that logs tasks polling longer than 10 ms. [A] bridge a CPU-bound job pool (`rayon`) to async via oneshot.
**Pitfall**: blocking on a mutex held across `.await`.

### Deep: Q144 Write a Future and a block_on by hand
**Prove you got it**: (1) What is the contract of `Waker::wake` if `poll` returns `Pending`? (2) What happens on a spurious wake?
**Try next**: [S] timer future. [M] join of two futures. [A] mini executor with a task queue (`async/waker_executor`, `capstones/mini_async_runtime`).
**Pitfall**: forgetting to register the waker on every poll.

### Deep: Q148 Why a future holding Rc or MutexGuard is not Send
**Prove you got it**: (1) Which values live across the `.await` in the generated state machine? (2) How does scoping a block fix it?
**Try next**: [S] reproduce the compile error. [M] refactor to drop the guard before awaiting. [A] design a shared-state actor so no guard crosses an await (`concurrency/actor_model`).
**Pitfall**: `tokio::sync::Mutex` hiding a design problem.

### Deep: Q153 Async recursion needs Box::pin
**Prove you got it**: (1) Why is the state machine infinite-sized? (2) What allocation cost do you pay?
**Try next**: [S] recursive directory walk. [M] iterative version with an explicit stack. [A] compare time and allocations with a counting allocator (`perf/counting_allocator`).
**Pitfall**: stack depth for deeply nested futures.

One-line follow-ups:
- Q143 async fn returns a lazy future. Q145 Pin/Unpin/pin!. Q146 spawn vs join vs select.
- Q147 task panics via JoinHandle. Q150 tokio Mutex vs std Mutex. Q151 cancellation meaning.
- Q154 Streams vs iterators. Q155 async fn in traits and dyn. Q156 CPU-bound work. Q157 When not to use async.

---

## rust_errors_iterators_closures_qa

Use in: library APIs, CLI error reporting. Read [thiserror](https://github.com/dtolnay/thiserror) and [anyhow](https://github.com/dtolnay/anyhow).

### Deep: Q159 `?` desugaring and From
**Prove you got it**: (1) Write the `match` that `?` expands to. (2) How does `From` conversion pick the error type?
**Try next**: [S] add a custom `From`. [M] `Box<dyn Error + Send + Sync>` conversion tests. [A] port a function from `anyhow` to a hand-rolled enum and compare ergonomics.
**Pitfall**: `?` in a function returning `Option` mixing `Result`.

### Deep: Q162 thiserror vs anyhow
**Prove you got it**: (1) What do they generate? (2) When does the caller need to match on variants?
**Try next**: [S] add `#[source]`. [M] error chain printing. [A] build a CLI with both layers (`errors/error_chain_retryable`).
**Pitfall**: exposing `anyhow::Error` in a library API.

### Deep: Q165 Iterators are lazy; prove it
**Prove you got it**: (1) Show with a side-effect closure that nothing runs without a consumer. (2) What does `iter().map(...)` without collect warn?
**Try next**: [S] counter test. [M] short-circuit proof with `take`/`find`. [A] write a pipeline benchmark versus a loop (Q172).
**Pitfall**: `collect::<Result<Vec<_>,_>>` stops at the first error.

### Deep: Q173 Closure capture modes
**Prove you got it**: (1) Which of Fn, FnMut, FnOnce does a closure implement given its body? (2) What does `move` change?
**Try next**: [S] reproduce E0373. [M] return closures boxed and `impl Fn`. [A] build an event registry (`implement_std_from_scratch` Q119).
**Pitfall**: moving a non-Copy value into a closure called twice.

### Deep: Q163 unwrap/expect policy
**Prove you got it**: (1) Where is `unwrap` acceptable? (2) What message style does `expect` need?
**Try next**: [S] grep your own code. [M] clippy lint `unwrap_used` in CI. [A] convert a crate to deny it.
**Pitfall**: panics inside `Drop`.

One-line follow-ups:
- Q158 Result/Option/panic. Q160 Box<dyn Error>. Q161 custom Error with source. Q164 catch_unwind.
- Q166 iter/iter_mut/into_iter. Q167 collect targets and `sum` inference. Q168 map/for_each/filter_map/flat_map.
- Q169 adapter tour. Q170 custom Iterator and IntoIterator for &T. Q171 invalidation impossibility. Q172 zero-cost. Q174 returning closures.

---

## rust_ownership_borrowing_qa

Use in: every Rust review. Read [Rustonomicon](https://doc.rust-lang.org/nomicon/) on ownership.

### Deep: Q178 NLL
**Prove you got it**: (1) Show code that compiles only with NLL. (2) What is a borrow's liveness region?
**Try next**: [S] take a failing E0502 snippet and shrink it to under 6 lines. [M] write the 'conditional return of a borrow' case that NLL still rejects, then fix it using an index or the entry API. [A] collect 10 real borrow-check errors from your own projects and write the minimal reproduction and fix for each.
**Pitfall**: believing scope end equals borrow end.

### Deep: Q180 Lifetime elision
**Prove you got it**: (1) State the three rules. (2) Where do they fail for two reference params?
**Try next**: [S] annotate `longest` and show one caller where the output lifetime is shorter than one input. [M] a struct holding `&str` with a method returning a slice of it, and prove (by compile-fail test) that it cannot outlive the source. [A] write an iterator over lines of a `&str` buffer that yields `&str` without allocating and check zero allocations with a counting allocator (`perf/counting_allocator`).
**Pitfall**: `'static` bound confusion (Q181).

### Deep: Q187 mem::take/replace/swap
**Prove you got it**: (1) Why can't you move out of `&mut`? (2) What does `Option::take` do?
**Try next**: [S] write a state-machine `enum` transition `fn advance(&mut self)` that moves data out of the old state with `mem::replace`. [M] implement `pop` for a Box linked list using `take`, tested against `Vec`. [A] rewrite a consuming builder as `&mut self` methods using `mem::take` and show the two APIs side by side with a benchmark of clones avoided.
**Pitfall**: `Default` not implemented.

### Deep: Q190 Two-phase borrows
**Prove you got it**: (1) Why does `v.push(v.len())` compile but `v.push({v.push(1); 2})` not? (2) What are the two phases?
**Try next**: [S] write five two-phase-borrow cases that compile and five near-misses that do not, with the error code of each. [M] explain one case with `-Zunpretty=mir` or `cargo rustc -- -Zdump-mir` output (nightly) and point at the `&two_phase` borrow. [A] write a lint-style test corpus of 20 snippets with expected compile/fail results using `trybuild`.
**Pitfall**: assuming argument evaluation order lets any nested mutable call compile.

### Deep: Q188 Mutating while iterating
**Prove you got it**: (1) Why does the borrow checker reject it? (2) What are `retain`, `drain`, indices alternatives?
**Try next**: [S] remove all even numbers from a `Vec<i32>` with `retain`, with `drain(..).filter`, and with index loop; assert same result on 10^4 random vectors. [M] remove entries from a `HashMap` while computing a sum of removed values, with `retain`. [A] explain and measure (10^6 elements) why `remove(i)` in a loop is quadratic and `retain` is linear.
**Pitfall**: invalidated indices.

One-line follow-ups:
- Q175 move/Copy/Clone. Q176 String vs i32 assignment. Q177 one `&mut` XOR many `&`.
- Q179 dangling prevention. Q182 String vs &str param. Q183 `split_at_mut`.
- Q184 shadowing. Q185 drop order. Q186 partial moves. Q189 Option<&mut T> helpers. Q191 clone is not always the fix.

---

## rust_smart_pointers_send_sync_qa

Use in: shared state design. Read [tokio sync sources](https://github.com/tokio-rs/tokio) and `concurrency/shared_state`.

### Deep: Q198 Send and Sync
**Prove you got it**: (1) Why `&T: Send` iff `T: Sync`? (2) Which std types opt out and why?
**Try next**: [S] write a `compile_fail` doctest proving each of `Rc`, `Cell` and `*const T` fields prevents Send or Sync. [M] write a raw-pointer wrapper with `unsafe impl Send` and a written safety argument, then break it with a test that Miri flags. [A] grep a real dependency tree (`cargo vendor`) for `unsafe impl Send` and review three of them for soundness arguments.
**Pitfall**: `unsafe impl Sync` without a proof.

### Deep: Q195 Rc cycles and Weak
**Prove you got it**: (1) Show the leak with a drop counter. (2) Fix with Weak.
**Try next**: [S] build a tree with `Weak` parents and assert strong/weak counts after construction and after dropping the root. [M] write a graph with cycles and an explicit `dispose` that breaks them; assert a drop counter reaches the node count. [A] run both versions under a counting allocator (`perf/counting_allocator`) or `valgrind` and report leaked bytes.
**Pitfall**: upgrading Weak too late.

### Deep: Q201 Arc<Mutex<T>> everywhere is a smell
**Prove you got it**: (1) What are better designs (message passing, sharding, immutable snapshots)? (2) Show lock contention numerically.
**Try next**: [S] replace one global `Arc<Mutex<HashMap>>` with 16 shards and measure at 8 threads. [M] replace readers' locking with `Arc` snapshot swap on a read-heavy (99%) workload. [A] benchmark the three designs at 1, 2, 4, 8 threads with a read/write ratio sweep and plot throughput.
**Pitfall**: holding locks across calls.

### Deep: Q199 Poisoning
**Prove you got it**: (1) When is a mutex poisoned? (2) What does `into_inner` on a poison error give?
**Try next**: [S] poison a mutex with a panicking thread and print the `PoisonError`. [M] implement a recovery strategy (re-validate state via `into_inner`) and test it. [A] port the example to `parking_lot::Mutex` (no poisoning) and write down what invariant check you now owe yourself.
**Pitfall**: ignoring poison silently.

### Deep: Q206 spawn needs 'static + Send; scoped threads
**Prove you got it**: (1) Why 'static? (2) How does scope guarantee joins?
**Try next**: [S] borrow a local `Vec` in two scoped threads and show the same code failing with `thread::spawn`. [M] parallel sum of 10^8 numbers in 8 chunks, compared against the sequential result. [A] re-implement `scope` yourself (Q116) and test panic propagation from a child thread.
**Pitfall**: leaking threads.

One-line follow-ups:
- Q192 Box uses. Q193 Rc vs Arc cost. Q194 Cell/RefCell/Mutex. Q196 tree with parents.
- Q197 Rc not Send. Q200 Mutex vs Condvar vs channels. Q202 RwLock writer starvation.
- Q203 Arc::make_mut/try_unwrap. Q204 Cow. Q205 Box/Rc/Arc dyn.

---

## rust_traits_generics_dispatch_qa

Use in: plugin systems and API design. Read the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/).

### Deep: Q209 Object safety
**Prove you got it**: (1) Which three features break dyn compatibility? (2) How does `where Self: Sized` rescue a method?
**Try next**: [S] take a trait with a generic method and a `Self`-returning method and make it dyn-compatible with `where Self: Sized`. [M] type-erase a trait with a generic method using a helper object-safe trait and a blanket impl. [A] read `patterns/type_erasure_vtable` and write your own vtable by hand for a two-method trait.
**Pitfall**: generic methods.

### Deep: Q207 Static vs dynamic dispatch
**Prove you got it**: (1) What is in a fat pointer? (2) What does monomorphisation cost?
**Try next**: [S] print `size_of` for `&T`, `&dyn Trait`, `Box<dyn Trait>`, `&[T]`. [M] benchmark a 10^7-call loop through generics versus `dyn` and report ns per call. [A] inspect the assembly of both (godbolt or `cargo asm`) and point at the indirect `call`.
**Pitfall**: assuming dyn is slow always.

### Deep: Q212 Associated types vs generic parameters
**Prove you got it**: (1) Why `Iterator::Item` but `From<T>`? (2) What can a type implement twice?
**Try next**: [S] write one trait with an associated type and one with a generic parameter and implement each for a `Vec` wrapper. [M] show a type that implements the generic version twice and explain why the associated-type version cannot. [A] design a `Storage` trait both ways and write down which makes call sites shorter.
**Pitfall**: ambiguity.

### Deep: Q218 PartialEq/Eq/PartialOrd/Ord and f64
**Prove you got it**: (1) Why not `Ord` for f64? (2) What does `total_cmp` do?
**Try next**: [S] sort 10^5 random f64 with `total_cmp` including NaN and -0.0; state the resulting order. [M] write an `OrderedF64` newtype implementing `Ord` consistently with `Eq`, and a property test of transitivity on 10^5 triples. [A] build a min-heap of f64 distances for Dijkstra using your newtype and compare with `data_structures/heap_tiebreak` approach.
**Pitfall**: derive Ord field order.

### Deep: Q215 Fn/FnMut/FnOnce
**Prove you got it**: (1) Hierarchy? (2) Which can be called twice?
**Try next**: [S] write one closure each for Fn, FnMut and FnOnce and a function accepting each bound; show which calls compile. [M] store a `Vec<Box<dyn FnMut()>>` and call each 3 times with a counter. [A] build a callback registry with unregister handles and test no callback fires after removal.
**Pitfall**: capturing by move into FnMut.

One-line follow-ups:
- Q208 generics vs dyn choice. Q210 impl Trait. Q211 where clauses. Q213 orphan rule/newtype.
- Q214 blanket impls and supertraits. Q216 storing closures. Q217 operators. Q219 Default/From/TryFrom.
- Q220 AsRef/Borrow/Deref. Q221 Sized. Q222 dyn Any. Q223 marker traits.

---

## rust_unsafe_memory_layout_qa

Use in: FFI and containers. Read the [Rustonomicon](https://doc.rust-lang.org/nomicon/).

### Deep: Q225 Soundness of an unsafe abstraction
**Prove you got it**: (1) State the safety contract of one function. (2) What input breaks soundness?
**Try next**: [S] write the `# Safety` section for a function taking a raw pointer and length. [M] write a Miri test that passes for valid input and a second one (expected to fail under Miri) that violates the contract. [A] audit `unsafe_ffi/maybeuninit_fixedvec`: list every `unsafe` block and the invariant that justifies it.
**Pitfall**: safe API calling unsafe with unchecked invariants.

### Deep: Q227 Sizes, alignment, niches
**Prove you got it**: (1) `size_of::<Option<Box<T>>>()`? (2) Why?
**Try next**: [S] print `size_of` and `align_of` for 10 types including `Option<&T>`, `Option<u32>`, `Option<Box<T>>`, `char`, `(u8, u32, u8)`. [M] reorder or `repr(C)` a struct and show size change. [A] work through `perf/struct_packing` and shrink a 100-byte struct from your own code, with a before/after benchmark.
**Pitfall**: assuming layout.

### Deep: Q232-Q233 Overflow and `as` casts
**Prove you got it**: (1) Debug versus release behaviour. (2) Which `as` cast truncates silently?
**Try next**: [S] demonstrate `checked_add`, `wrapping_add`, `saturating_add` on `u8::MAX`. [M] write a property test comparing `x as u8` with `(x & 0xFF) as u8` for 10^5 random `i64`. [A] enable clippy `cast_possible_truncation` on one of your crates and fix or justify every hit.
**Pitfall**: `as usize` from negative.

### Deep: Q234 transmute dangers
**Prove you got it**: (1) What invariants does transmute not check? (2) Safe replacements?
**Try next**: [S] replace a `transmute::<[u8;4], u32>` with `from_ne_bytes` and show equal results. [M] write a safe `cast` for a `repr(C)` struct of two `u32` with a size and alignment assertion. [A] write a transmute that creates an invalid `bool` or `char` and run it under Miri to see the diagnostic.
**Pitfall**: padding bytes.

### Deep: Q237 Leaking is safe
**Prove you got it**: (1) Why is `mem::forget` safe? (2) What does that imply for scope guards?
**Try next**: [S] use `ManuallyDrop` to prevent a drop and assert the drop counter stays 0. [M] use `Box::leak` to make a `&'static mut` config and justify when it is acceptable. [A] write a scope-guard type and show how `mem::forget` bypasses it, explaining why soundness must not depend on Drop running.
**Pitfall**: relying on Drop for soundness.

One-line follow-ups:
- Q224 what unsafe permits. Q226 raw pointers and MaybeUninit. Q228 repr variants. Q229 stack/heap.
- Q230 Vec growth. Q231 UTF-8 and graphemes. Q235 Pin. Q236 zero-cost and monomorphisation bloat.
- Q238 static/const/thread-local. Q239 FFI. Q240 data race versus race condition.

---

## stacks_queues_heaps_questions

Use in: parsers, schedulers. Pair with `data_structures/heap_patterns` and `data_structures/ring_buffer_zoo`.

### Deep: Q249 Largest rectangle in histogram
**Prove you got it**: (1) Why does each bar get popped exactly once and what does the stack hold? (2) What is the sentinel trick?
**Try next**: [S] indices of the rectangle. [M] maximal rectangle in a binary matrix via histograms; oracle: O(n^2 m^2). [A] benchmark vs divide-and-conquer.
**Pitfall**: equal heights.

### Deep: Q251 Sliding window maximum
**Prove you got it**: (1) Invariant of the deque. (2) Amortised cost proof.
**Try next**: [S] minimum. [M] oracle brute force windows. [A] streaming version over a 10^8 series.
**Pitfall**: storing values not indices.

### Deep: Q257 Median of a data stream
**Prove you got it**: (1) Balance invariant of two heaps. (2) Sliding window median problem?
**Try next**: [S] percentile p. [M] lazy deletion heaps. [A] compare with order-statistic treap (`data_structures/treap_interval`).
**Pitfall**: rebalancing after each add.

### Deep: Q245 Basic calculator
**Prove you got it**: (1) How does the stack handle `-(2+3)`? (2) Unary minus?
**Try next**: [S] add `*`/`/` (precedence). [M] Pratt parser version (`text_and_parsing/pratt_parser`). [A] fuzz vs `eval` oracle on generated expressions.
**Pitfall**: overflow.

### Deep: Q256 Merge k sorted arrays
**Prove you got it**: (1) Heap vs divide and conquer comparisons. (2) Why `Reverse`?
**Try next**: [S] lists. [M] count comparisons. [A] external merge (`algorithms/sorting_networks_external_sort`).
**Pitfall**: empty inputs.

One-line follow-ups:
- Q241 balanced brackets. Q242 min stack. Q243 queue via stacks. Q244 RPN. Q246 decode string.
- Q247 Unix path. Q248 next greater. Q250 rain water two ways. Q252 circular queue. Q253 naive LRU baseline.
- Q254 kth largest heap versus quickselect. Q255 top k. Q258 scheduler cooldown. Q259 meeting rooms.
- Q260 k closest. Q261 connect ropes. Q262 smallest range. Q263 heap from scratch. Q264 min-heap and f64 in Rust. Q265 peek_mut and VecDeque.

---

## tries_advanced_questions

Use in: boggle/keyword scanning. Pair with `algorithms/aho_corasick` and `data_structures/trie_radix`.

### Deep: Q266 Word Search II
**Prove you got it**: (1) Why prune exhausted trie branches? (2) What does the trie save over per-word DFS?
**Try next**: [S] dedupe results. [M] 12x12 board with 1,000 words, oracle per-word DFS. [A] measure speed-up vs board size.
**Pitfall**: marking visited cells.

### Deep: Q267 Maximum XOR pair
**Prove you got it**: (1) Why greedy per bit? (2) Complexity?
**Try next**: [S] indices. [M] Q268 offline queries. [A] compare with a hash-set prefix method.
**Pitfall**: 32 versus 31 bits.

### Deep: Q272 Aho-Corasick stream
**Prove you got it**: (1) How do fail links preserve linear time? (2) Overlaps?
**Try next**: [S] counts. [M] oracle naive. [A] feed a 1 GB log.
**Pitfall**: dictionary links.

### Deep: Q277 Palindrome pairs
**Prove you got it**: (1) Which three cases? (2) Complexity?
**Try next**: [S] hash map version. [M] oracle O(n^2). [A] word list.
**Pitfall**: empty string and duplicates.

### Deep: Q275 Autocomplete system
**Prove you got it**: (1) State machine for `input(c)`. (2) Ranking ties?
**Try next**: [S] top-3 cache. [M] updates with `#`. [A] `search_vectors/autocomplete_engine`.
**Pitfall**: stale caches (Q280).

One-line follow-ups:
- Q268 offline XOR queries. Q269 subarray XOR. Q270 pairs XOR range with counts. Q271 stream of characters.
- Q273 prefix and suffix. Q274 concatenated words. Q276 file system trie. Q278 shortest unique prefix.
- Q279 word squares. Q280 cache invalidation.

---

## tries_core_questions

Use in: autocomplete, routing. Pair with `data_structures/trie_radix` and [fst](https://github.com/BurntSushi/fst).

### Deep: Q283 Arena trie
**Prove you got it**: (1) Why u32 indices? (2) Memory per node?
**Try next**: [S] add `delete` that prunes now-empty branches and a model test against `HashSet<String>` of prefixes. [M] measure bytes per node for array children (26 or 256) versus `HashMap` children versus sorted `Vec<(u8,u32)>` on 10^5 words. [A] compress it into a radix tree (Q295) and compare node counts on a real dictionary.
**Pitfall**: dangling ids after delete.

### Deep: Q284 Wildcard `.` search
**Prove you got it**: (1) Worst case? (2) Memoisation?
**Try next**: [S] extend `.` to also support `*` (any run) and test on 20 handwritten cases. [M] oracle: translate patterns to regex and compare against the `regex` crate on 10^4 random words/patterns. [A] time worst-case patterns like `.......` against 10^5 words and show the blow-up, then add pruning by remaining length.
**Pitfall**: backtracking blow-up.

### Deep: Q294 Spell checker with Levenshtein row
**Prove you got it**: (1) Why pass one DP row per trie node? (2) Pruning rule?
**Try next**: [S] implement k = 1 and compare to a brute-force over the dictionary on 1,000 typos. [M] compare with BK-tree from `math_science/spatial_search_structures` on queries/second at k = 1, 2. [A] run on `/usr/share/dict/words` with typos injected at rate 10% and report top-1 recovery.
**Pitfall**: row off-by-one.

### Deep: Q295 Radix compression
**Prove you got it**: (1) Why at most 2n nodes? (2) Split on insert?
**Try next**: [S] count nodes before and after compression on 10 word lists. [M] oracle: results of `with_prefix` equal for plain trie and radix tree on 10^4 random queries. [A] measure bytes per key for both on 10^6 URL paths and relate to the 2n bound.
**Pitfall**: edges.

### Deep: Q297 Unicode
**Prove you got it**: (1) Bytes versus chars? (2) Normalisation?
**Try next**: [S] write "e" plus a combining acute accent (U+0301) and the precomposed U+00E9, and show they are different byte strings in your trie; decide how to normalise. [M] add case-insensitive lookup via full case folding of 20 tricky words (German sharp s, Turkish dotted I) and list what your approach gets wrong. [A] add NFC normalisation (the `unicode-normalization` crate) and test that lookups agree for every pair of canonically equivalent forms in a small table.
**Pitfall**: grapheme clusters.

One-line follow-ups:
- Q281 basic trie. Q282 map children. Q285 LCP. Q286 shortest root. Q287 longest buildable word.
- Q288 map sum. Q289 distinct substrings. Q290 top-k autocomplete. Q291 suggestions. Q292 word break.
- Q293 T9. Q296 trie vs hash set vs sorted Vec. Q298 IPv4 longest-prefix match.

---

## Chapter projects

### Small (about 2 hours): "standard library mini-set"
Combine `implement_std_from_scratch`, `linked_lists_in_rust`, and `unsafe_ffi/nonnull_linked_list`.
- Milestones: (1) MyVec with growth; (2) stack with iterative drop; (3) MyRc/MyWeak; (4) Miri run.
- Acceptance: each type passes a model test against std on 10^4 ops; Miri reports no UB.
- Stretch: ZST support.

### Intermediate (about a weekend): "typeahead service"
Combine `tries_core_questions`, `tries_advanced_questions`, `data_structures/trie_radix`, `search_vectors/autocomplete_engine`, and `capstones/typeahead_service`.
- Milestones: (1) arena trie; (2) top-k per node; (3) typo tolerance via Levenshtein row; (4) HTTP endpoint; (5) latency histogram.
- Acceptance: results equal a brute-force sort over the dictionary for 10^4 random prefixes; p99 under 1 ms for 10^5 words.
- Stretch: Aho-Corasick for banned words; cache invalidation (Q280).

### Advanced (about a week): "practice judge"
Combine `dp_and_backtracking_questions`, `graphs_and_union_find_questions`, `arrays_strings_hashmap_questions`, `dev_tools/shrinking`, `os_and_storage/process_pipes_signals`, and `text_and_parsing/json_parser`.
- Milestones: (1) problem files in JSON with reference solutions; (2) runner executing submissions in child processes with time and memory limits; (3) random test generation plus brute-force comparison; (4) automatic shrinking of failing inputs; (5) leaderboard of runtime.
- Acceptance: a deliberately wrong solution is caught, shrunk to under 10 elements, and a timeout is reported; the correct solutions of 20 problems pass.
- Stretch: sandbox with resource limits and async job queue (`async/worker_pool`).
