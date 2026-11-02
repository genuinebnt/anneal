# anneal — curriculum plan

> **2026-10-07:** the Build section (Backend B1–B6, Design M1–M2) was dropped and DSA became a NeetCode tracker; see
> [DSA.md](DSA.md). System design (LLD, HLD, API design) and behavioral are deferred.

What to study, in what order, and how to know you're ready for **SDE-2 and SDE-3 Rust backend loops**.
The platform is organised as **sections → tracks → stages → problems**. Every track climbs from easy to hard,
and every problem is written for how Rust does it, not ported from another language.

Sources: `rust_learning_platform_curriculum.html` (track list, use → understand → implement, project ladder) and the
old packet in `docs/rust template/`, reused by ID: W1–W98 write exercises, B1–B9 bug-finds, Blind 75, LLD, DP, SD.
`PLAN.md` covers how the app delivers it.

---

## 1. Shape

### Sections

| Section | Code | Tracks | Problems | What a loop tests with it |
|---|---|---|---|---|
| Rust Language | L | 10 | ~177 | "Why won't this compile?", ownership, traits, API taste |
| Rust Standard Library | S | 11 | ~178 | knowing std's types, their costs, and how they work inside |
| Data Structures & Algorithms | D | 14 | 275 | the two DSA coding rounds, solved in idiomatic Rust |
| Concurrency & Async | C | 6 | ~98 | threads, channels, atomics, Tokio: most backend Rust roles |
| Systems Rust | Y | 3 | ~40 | unsafe, FFI, verification (SDE-3 depth). Y1 and Y4 moved into F |
| Performance Rust | F | 7 | ~96 | layout, allocation, hashing, CPU-level tricks, lock-free, serialization, graded on real measurements (SDE-3) |
| Backend Rust | B | 6 | ~80 | axum, tower, sqlx, protocols, resilience, observability |
| Design in Rust | M | 2 | 24 | the machine-coding / LLD round |
| Projects | P | 5 | 38 stages | take-homes and "build a small service" |
| System design | H | 3 | reading + mocks | the HLD round. Its practice mode is deferred (§9) |

### Inside a track

- **Stages** are grouped into three **bands**, always in this order: **Easy → Medium → Hard**.
  Problems inside a stage are also ordered easy to hard.
- In the Rust sections (L, S, C, Y, B) the bands follow the curriculum's learning model:
  **Easy = use it** (the normal API), **Medium = understand it** (the invariants, costs and compiler reasons),
  **Hard = implement it** (build a simplified version: MiniVec, MiniRc, a channel, an executor).
  The fourth layer, *read the real implementation*, is deferred.
- **Problem types:** *write it* (signature + tests) · *fix this* (broken program + rules like "no `.clone()`") ·
  *project stage* (tests only, repo carries forward).
- **Counts.** In D the tables list every problem. In L, S, C, Y and B they list each stage's anchor problems; the heading number
  is the planned total, and the extra problems are variations of those anchors (mostly fix-this versions and the old packet's
  harden subs), written wave by wave.
- **Notation:** `Fix:` = fix-this · `†` = Blind 75 · `W12`, `B3` = reused from the old packet · `(★)` = the stage's capstone problem.

### Paths
- **SDE-2 path** = the Easy and Medium bands of every track marked **core**, plus shortline.
- **SDE-3 path** = everything.
- Tracks marked **SDE-3** are skipped on the SDE-2 path.

### Rules the app enforces
- *Solved* = all visible and hidden tests pass, no rule broken.
- Any hint or early solution = *assisted* → re-solve in 3 days. Re-solves step out 3 → 7 → 21 → 60 days.
- **Test-out:** solve a stage's `(★)` problem and one other unassisted, inside the time budget, and the stage counts as done.
  Week 1 of the schedule is a test-out sweep so you skip what you already know.
- **Time budgets:** Easy 10 min · Medium 25 min · Hard 40 min (fix-this: 5 / 10 / 15). Machine coding: 60–90 min.
  Going over still counts as solved but schedules a 7-day re-solve.

---

## 2. Section L · Rust Language

### L1 · Ownership & moves: core · 18
| Band | Stage | Problems |
|---|---|---|
| Easy | Moves & Copy | Fix: use after move (E0382) · Copy vs move for your own struct · Fix: value moved in a loop · return ownership from a function |
| Easy | Passing values | by value vs `&` vs `&mut` · Fix: borrowed where owned was needed (E0308) · (★) `&str` vs `String` parameters |
| Medium | Clones & drops | Remove every `.clone()` (rule: none allowed) · W39 `Cow` normaliser · predict the drop order · (★) RAII timing guard (DP18) |
| Medium | Closures take ownership | Fix: closure may outlive (E0373) · `move` into `thread::spawn` · (★) capture only what you need |
| Hard | Partial moves & `mem` | Fix: move out of index (E0507) · `mem::take` / `replace` · `Option::take` state machine · (★) guard that survives `mem::forget` misuse |

### L2 · Borrowing: core · 35
Fully written out in the mockup (Concepts → Borrowing).
| Band | Stage | Problems |
|---|---|---|
| Easy | Shared vs unique | Fix: push while holding a ref (E0502) · many readers, one writer · Fix: `&mut` from `&self` (E0596) · B1 · (★) count words without cloning |
| Easy | Where borrows end (NLL) | Fix: borrow kept alive by a later use · scope a borrow with a block · (★) entry API instead of get-then-insert |
| Medium | Reborrows | Fix: moved `&mut` after passing it · `vec.push(vec.len())` two-phase borrows · (★) Fix: `&mut` field reused twice |
| Medium | Iterator invalidation | Fix: remove while iterating · `retain` · collect indices then mutate · (★) Fix: map mutation during iteration |
| Medium | Split borrows | (★) Fix: two `&mut` of self (E0499) · `split_at_mut` · destructure self into field borrows |
| Hard | Borrow-checker limits | get-or-insert without entry (NLL case 3) · view structs over two fields · `get_disjoint_mut` · (★) Fix: RefCell where a split borrow suffices |

### L3 · Lifetimes: core · 20
| Band | Stage | Problems |
|---|---|---|
| Easy | Elision | when elision picks `&self` · Fix: returning a ref to a local (E0515) · (★) `longest(a, b)` |
| Easy | Structs holding refs | parser over `&str` · Fix: struct outlives its source (E0597) |
| Medium | Two lifetimes | W37 why `'a` everywhere fails · (★) W36 zero-copy tokenizer · W40 binary header parser |
| Medium | `'static` and `T: 'a` | `'static` bound vs `&'static` · why `thread::spawn` needs `'static` · `Box<dyn Error + 'static>` |
| Hard | Variance & HRTBs | `&mut T` invariance bug · `for<'a> Fn(&'a str)` · (★) W38 self-referential struct without unsafe |

### L4 · Traits & dispatch: core · 22
| Band | Stage | Problems |
|---|---|---|
| Easy | Define & implement | a `Shape` trait · default methods · `impl Display` · (★) `impl Trait` arguments |
| Medium | Static vs dynamic | W25 shapes both ways · W21 closures generic vs `dyn` · (★) W6 plugin logger with trait objects |
| Medium | Object safety | Fix: generic method in `Box<dyn>` (E0038) · `where Self: Sized` escape hatch |
| Medium | Operator traits | `Add`, `Mul`, `Index` · (★) W47 `Matrix<T>` with operator overloading |
| Hard | Coherence & extension | Fix: orphan rule (E0117) → newtype · blanket impls · extension traits on `Iterator` · (★) GAT-based `LendingIterator` |

### L5 · Generics & associated types: core · 16
| Band | Stage | Problems |
|---|---|---|
| Easy | Generic code | W1 `Stack<T>` · Fix: missing bound (E0369) · (★) generic `max_by_key` |
| Medium | Bounds & associated types | `where` clauses · associated type vs type parameter · (★) W11 typestate HTTP builder |
| Hard | Type-level design | DP17 newtype + typestate · `PhantomData` units of measure · monomorphization cost and the inner-fn trick · (★) API that makes invalid states unrepresentable |

### L6 · Closures & functional Rust: core · 14
| Band | Stage | Problems |
|---|---|---|
| Easy | Closure basics | capture by ref vs value · closures as arguments · (★) sort with a closure comparator |
| Medium | `Fn` / `FnMut` / `FnOnce` | Fix: calling an `FnOnce` twice · stateful `FnMut` counter · (★) return a closure (`impl Fn` vs `Box<dyn Fn>`) |
| Hard | Higher-order Rust | memoize any `Fn(u64) -> u64` · Fix: recursive closure (E0499) · (★) event bus of `Box<dyn FnMut(&Event)>` |

### L7 · Enums & pattern matching: core · 14
| Band | Stage | Problems |
|---|---|---|
| Easy | Enums & exhaustiveness | Fix: non-exhaustive match (E0004) · `Option` matching · (★) command parser to an enum |
| Medium | Patterns in depth | bindings, `@`, `ref` · guards · slice patterns `[first, .., last]` · (★) `let-else` refactor |
| Hard | Enums as design | (★) DP14 connection state machine · visitor over an enum AST · enum vs trait object trade-off |

### L8 · Error design: core · 16
| Band | Stage | Problems |
|---|---|---|
| Easy | Custom errors | error enum + `Display` · (★) `?` through two layers |
| Medium | Conversion & crates | W10 `From` + `?` · Fix: `?` with mismatched errors (E0277) · `thiserror` library errors · `anyhow` + `.context()` in a binary · (★) library vs application error choice |
| Hard | Errors at scale | (★) one error enum → HTTP status + JSON (feeds shortline) · `source()` chains & downcasting · B4 panic in production · unwind boundaries |

### L9 · Modules, crates & Cargo: light · 10
| Band | Stage | Problems |
|---|---|---|
| Easy | Modules & visibility | `pub(crate)` · re-exports · (★) split a file into modules |
| Medium | Cargo | workspaces · features & `cfg` · (★) optional dependency behind a feature |
| Hard | Crate APIs | semver-safe changes · sealed traits · (★) `#[non_exhaustive]` evolution |

### L10 · Macros: SDE-3 · 12
| Band | Stage | Problems |
|---|---|---|
| Easy | `macro_rules!` | fragment specifiers · (★) `hashmap!{}` literal |
| Medium | Repetition & hygiene | `$(...),*` · counting tokens · `$crate` · (★) `vec_of_strings!` |
| Hard | Procedural | (★) `#[derive(Builder)]` with `syn` + `quote` · attribute macro that times a fn |

---

## 3. Section S · Rust Standard Library

Each track follows **use it → understand it → build it**. The build is a simplified std type you write yourself.

### S1 · Option & Result: core · 16
| Band | Stage | Problems |
|---|---|---|
| Easy | Use | `map`, `and_then`, `ok_or`, `unwrap_or_else` · `?` on `Option` · (★) sentinel → `Option` at the API boundary |
| Medium | Understand | `as_ref` / `as_mut` / `as_deref` · `Option<&T>` vs `&Option<T>` · `transpose` · collect into `Result<Vec<_>, _>` · (★) Fix: `unwrap` on user input |
| Hard | Build | (★) `MyOption<T>` with ten combinators · `MyResult` + a `?`-like macro |

### S2 · Strings & text: core · 18
| Band | Stage | Problems |
|---|---|---|
| Easy | Use | `String` vs `&str` · `split` / `trim` / `parse` · `format!` width & precision · (★) word frequency (W22) |
| Medium | Understand | bytes vs `chars` vs `char_indices` · Fix: slicing mid-UTF-8 panics · `Cow<str>` · `Display` vs `Debug` · (★) Unicode-safe truncate |
| Hard | Build | (★) zero-copy `split` iterator with lifetimes · a tiny `{name}` template formatter |

### S3 · Vec & slices: core · 20
| Band | Stage | Problems |
|---|---|---|
| Easy | Use | push / pop / insert / `retain` / `dedup` / `sort` · slices as views · (★) remove duplicates in place |
| Medium | Understand | capacity & reallocation (W41) · `drain`, `splice`, `split_off` · `chunks` / `windows` · `split_at_mut` · (★) predict allocation counts |
| Hard | Build | (★) MiniVec with `std::alloc` (after Y2 stage 1) · inline small-vector · `Vec<T>` → `Box<[T]>` trade-offs |

### S4 · Maps & sets: core · 18
| Band | Stage | Problems |
|---|---|---|
| Easy | Use | `HashMap` counting · entry API · `HashSet` ops · (★) group by key |
| Medium | Understand | `BTreeMap` / `BTreeSet` ranges · `Hash` + `Eq` consistency · `Borrow<str>` lookups with `String` keys · iteration order · (★) floor / ceiling queries with `range` |
| Hard | Build | (★) open-addressing `HashMap<K, V>` · `BTreeMap`-backed interval map |

### S5 · Queues & heaps: core · 14
| Band | Stage | Problems |
|---|---|---|
| Easy | Use | `VecDeque` as queue and deque · `BinaryHeap` basics · (★) sliding-window max with `VecDeque` |
| Medium | Understand | max-heap + `Reverse<T>` · custom `Ord` for heap entries · Fix: heap of `f64` · (★) lazy deletion instead of decrease-key |
| Hard | Build | (★) ring-buffer deque · binary heap with sift up / down · indexed priority queue with decrease-key |

### S6 · Iterators: core · 22
| Band | Stage | Problems |
|---|---|---|
| Easy | Use | `map` / `filter` / `enumerate` / `zip` / `fold` / `collect` · (★) rewrite three loops as iterator chains |
| Medium | Understand | laziness (B7) · `IntoIterator` for `T`, `&T`, `&mut T` · `peekable`, `by_ref`, `DoubleEndedIterator` · Fix: `iter()` vs `into_iter()` · (★) W9 sliding-window iterator |
| Hard | Build | (★) your own `Map`, `Filter`, `Flatten` adapters · tree iterator without allocation · why std has no lending iterator |

### S7 · Smart pointers & interior mutability: core · 20
| Band | Stage | Problems |
|---|---|---|
| Easy | Use | `Box` recursive types (Fix: E0072) · `Rc` shared ownership · `RefCell` basics · (★) `Box<dyn Trait>` list |
| Medium | Understand | B2 `Rc` cycle → `Weak` · `Cell` vs `RefCell` vs `OnceCell` · Fix: `Rc` across threads · `Deref` coercions · (★) pick the cheapest correct pointer (&T → Box → Rc → Arc → Arc<Mutex>) |
| Hard | Build | (★) W3 MiniRc (+ harden subs) · MiniRefCell with borrow flags · MiniArc with atomics (after C3) |

### S8 · The core traits: core · 16
| Band | Stage | Problems |
|---|---|---|
| Easy | Derive | `Debug`, `Clone`, `Copy`, `PartialEq`, `Default` · (★) when a type can't be `Copy` |
| Medium | Implement by hand | `Eq` + `Hash` consistency · `PartialOrd` / `Ord` with `total_cmp` · `From` / `TryFrom` / `FromStr` · `AsRef` vs `Borrow` · `Deref` misuse · (★) `Drop` order |
| Hard | Build | (★) a newtype that behaves like a std type (full trait set) · `Borrow`-compatible map key |

### S9 · I/O & filesystem: core · 14
| Band | Stage | Problems |
|---|---|---|
| Easy | Use | read a file · `BufReader::lines` · `write!` · (★) CSV-ish summariser |
| Medium | Understand | W27 generic over `Read` / `Write` · W30 testable REPL · I/O errors · (★) bounded-memory line reader |
| Hard | Build | (★) W29 implement `Read` yourself · `BufReader` from scratch |

### S10 · Time, env & processes: light · 8
| Band | Stage | Problems |
|---|---|---|
| Easy | Use | `Instant` / `Duration` timing · `env::args` / `var` · (★) CLI with exit codes |
| Medium | Understand | `SystemTime` vs `Instant` · `Command` with pipes · (★) injectable `Clock` trait |
| Hard | Build | (★) run a command with a timeout |

### S11 · mem, ptr & alloc: SDE-3 · 12
| Band | Stage | Problems |
|---|---|---|
| Medium | Understand | `mem::swap` / `replace` / `take` · `size_of` / `align_of` quizzes · `ManuallyDrop` · (★) niche optimisation: `Option<Box<T>>` is one word |
| Hard | Build | `MaybeUninit` array init · `Layout` + `alloc` · `NonNull` · (★) W17 bump allocator |

---

## 4. Section D · Data Structures & Algorithms

Every problem is solved in idiomatic Rust, and each stage names the Rust habit it trains.
All 75 Blind 75 problems are included (†).
Order to work through: D1 → D2 → D3 → D4 → D5 → D6 → D9 → D7 → D8 → D12 → D10 → D11 → D13 → D14.

### D1 · Arrays & hashing: core · 24
| Band | Stage | Problems | Rust habit |
|---|---|---|---|
| Easy | Vec & slices | Running sum · Concatenation of array · Two sum† · Contains duplicate† · Valid anagram† · Fix: index panic → `get()` | `[u8; 26]`, `get` vs `[]` |
| Easy | Counting | Majority element · Ransom note · Isomorphic strings · Fix: get-then-insert → entry | entry API |
| Medium | Grouping & prefix sums | Group anagrams† · Top K frequent† (W32) · Product except self† · Subarray sum equals K · Encode & decode strings† · Fix: overflow in prefix sums (B5) | `i64`, `checked_add` |
| Medium | Sorting & order | Sort colors · Largest number · Kth largest (`select_nth_unstable`) · Longest consecutive† · Fix: `sort_by` on `f64` | `total_cmp`, `sort_unstable_by` |
| Hard | In place | First missing positive · Quicksort in place (W33) · Max points on a line | index as hash, `swap` |

### D2 · Two pointers & sliding window: core · 18
| Band | Stage | Problems | Rust habit |
|---|---|---|---|
| Easy | Two ends | Valid palindrome† · Reverse string · Merge sorted array · Move zeroes · Fix: `usize` underflow on `r - 1` | bytes, `checked_sub` |
| Medium | Pointers | Two sum II · 3Sum† · Container with most water† · 3Sum closest | skipping duplicates |
| Medium | Windows | Best time to buy/sell† · Longest substring without repeats† · Char replacement† · Permutation in string · Find all anagrams | `[usize; 128]` last-seen |
| Hard | Hard windows | Min window substring† · Trapping rain water · Sliding window maximum · 4Sum | monotonic `VecDeque`, `i64` |

### D3 · Stacks & queues: core · 14
| Band | Stage | Problems | Rust habit |
|---|---|---|---|
| Easy | Stack basics | Valid parentheses† · Queue using stacks · Baseball game · Min stack | `Vec` as stack, `match` on bytes |
| Medium | Monotonic & parsing | Evaluate RPN · Daily temperatures · Car fleet · Asteroid collision · Decode string · Fix: `pop().unwrap()` on bad input | `Option` + `?` |
| Hard | Hard stacks | Largest rectangle in histogram · Basic calculator · Max frequency stack · Shortest subarray with sum ≥ K | stacks of tuples |

### D4 · Binary search: core · 14
| Band | Stage | Problems | Rust habit |
|---|---|---|---|
| Easy | On an index | Binary search · Search insert position · First bad version · Sqrt(x) | `partition_point`, `binary_search_by` |
| Medium | On structure & answer | First & last position · Min in rotated† · Search rotated† · Search a 2-D matrix · Koko eating bananas · Time-based KV store | `BTreeMap::range` as an alternative |
| Hard | Hard searches | Median of two sorted arrays · Split array largest sum · K-th smallest pair distance · Fix: mid overflow & infinite loop | `lo + (hi - lo) / 2` |

### D5 · Linked lists, the Rust way: core · 15
| Band | Stage | Problems | Rust habit |
|---|---|---|---|
| Easy | Owned lists | Reverse linked list† · Merge two sorted† · Remove duplicates · Middle of the list · Fix: move out of borrowed `Box` (E0507) | `Option<Box<Node>>`, `take()` |
| Medium | Cursors | Remove Nth from end† · Reorder list† · Add two numbers · Palindrome list · Linked list cycle† (index arena) · Copy list with random pointer | `&mut Option<Box<_>>` cursors, arenas |
| Hard | Beyond Box | Merge k sorted† · Reverse nodes in k-group · Doubly linked deque with `Rc` / `Weak` · Unsafe doubly linked list with `NonNull` (after Y2) | why doubly-linked is hard in Rust |

### D6 · Trees & BSTs: core · 44
Extended 2026-09-27 to cover the LeetCode 250 tree problems. LeetCode's `Option<Rc<RefCell<TreeNode>>>` shape with shared
`tree(&[Option<i32>])` / `level_order_values` helpers; the hard band moves to Rust-native shapes.
| Band | Stage | Problems | Rust habit |
|---|---|---|---|
| Easy | Basics | Max depth† · Min depth · Same tree† · Invert tree† · Symmetric tree · Path sum · Sorted array → BST · Fix: `BorrowMutError` in a tree walk | `Option<Rc<RefCell<TreeNode>>>` (LeetCode's shape), `borrow()` scopes |
| Easy | Traversals | Preorder · Inorder · Postorder · Leaf-similar trees · Count nodes | recursive helpers with `&mut Vec` |
| Medium | Levels & recursion | Average of levels · Level order† · Zigzag level order · Right side view · Iterative inorder · Diameter · Balanced tree · Subtree of another† · Count good nodes · Path sum II · Sum root-to-leaf numbers · Max width · Flatten to linked list · Vertical order traversal | `VecDeque`, tuples returned up the stack |
| Medium | BSTs | Search in a BST · Insert into a BST · Validate BST† · Kth smallest† · LCA of BST† · LCA of binary tree · Build from preorder / inorder† · Delete node in a BST · BST iterator | `Option<i64>` bounds |
| Hard | Ownership-shaped trees | Max path sum† · Binary tree cameras · Recover BST · Serialize / deserialize† · Insert & delete in an `Option<Box<Node>>` BST · In-order iterator with lifetimes (W43) · Arena tree with typed indices (W55) · Parent pointers with `Weak` | arenas vs `Rc<RefCell>` |

### D7 · Heaps & priority queues: core · 13
| Band | Stage | Problems | Rust habit |
|---|---|---|---|
| Easy | Heap basics | Kth largest in a stream · Last stone weight · Fix: `BinaryHeap<f64>` won't compile | `Reverse<T>`, `Ord` newtype |
| Medium | Heaps at work | K closest points · Task scheduler (W46) · Reorganize string · Merge k sorted arrays · Top K frequent words | tuple ordering, custom `Ord` |
| Hard | Two heaps & merges | Find median from data stream† · IPO · Smallest range covering k lists · External merge sort (W64) · Single-threaded CPU | lazy deletion |

### D8 · Intervals & greedy: core · 29
Extended 2026-09-27 to cover the LeetCode 250 greedy and interval problems.
| Band | Stage | Problems | Rust habit |
|---|---|---|---|
| Easy | First greedy | Assign cookies · Lemonade change · Can place flowers · Meeting rooms† · Maximum subarray† · Best time to buy/sell II | `sort_unstable_by_key` |
| Medium | Intervals | Merge intervals† (W45) · Insert interval† · Non-overlapping† · Min arrows to burst balloons · Meeting rooms II† · Interval list intersections · Car pooling | sweep lines, `(i32, i32)` tuples |
| Medium | Greedy choices | Jump game† · Jump game II · Gas station · Partition labels · Boats to save people · Two city scheduling · Min add to make parentheses valid · Valid parenthesis string · Queue reconstruction by height | exchange arguments, `Vec::insert` |
| Hard | Hard greedy | Hand of straights · Remove K digits · Candy · Min interval to include each query · Employee free time · Min refueling stops · Course schedule III | `BTreeMap` counts, heaps |

### D9 · Graphs: core · 58
The designed seed (32, in `TrackGraphs.dc.html`) plus three Blind 75 problems, extended 2026-09-27 with 23 LeetCode 250
problems so every stage opens gently and the common patterns (grid BFS, BFS on states, Floyd–Warshall, Prim, Euler paths)
are covered. New problems in *italics*.
| Band | Stage | Problems |
|---|---|---|
| Easy | Representation | *Find center of star graph* · Build an adjacency list · Degree counts with iterators · *Find if path exists* · Edge list to CSR · Fix: a graph that owns its nodes |
| Easy | Grid & graph traversal | *Flood fill* · *Island perimeter* · Number of islands† · *Max area of island* · *Number of provinces* · *Keys and rooms* · Iterative DFS · Fix: recursive closure DFS · Clone graph† |
| Medium | BFS patterns | Rotting oranges · *01 matrix* · *Shortest path in binary matrix* · *Surrounded regions* · **Pacific Atlantic†** · *Open the lock* · Word ladder · *Evaluate division* |
| Medium | Topological sort | Course schedule† · *Course schedule II* · Build order with cycle report · Fix: invalidation in Kahn's · *Minimum height trees* · Alien dictionary† |
| Medium | Shortest paths | Network delay time · Fix: heap ordering with a custom Ord · Path with minimum effort · Cheapest flights within K stops · *City with the fewest reachable neighbours (Floyd–Warshall)* · *Swim in rising water* · 0-1 BFS · Dijkstra over generic weights (W42) |
| Medium | Union-find & MST | **Connected components†** · **Graph valid tree†** · Redundant connection · Union-find as a struct (W20) · *Accounts merge* · Kruskal's MST · *Min cost to connect all points (Prim)* · Fix: two `&mut` into one parent Vec |
| Hard | Hard traversals | *Sliding puzzle* · *Bus routes* · *Making a large island* · *Shortest path to get all keys* · *Reconstruct itinerary (Hierholzer)* |
| Hard | SCC, bridges & arenas | Tarjan's SCC · Critical connections · Arena-allocated graph · Fix: `Rc<RefCell<Node>>` cycle leak |
| Hard | Flows & matching | Bipartite check · Edmonds–Karp · Hopcroft–Karp · Min-cost flow |

### D10 · Tries & strings: core · 25
Extended 2026-09-27 with the LeetCode 250 trie problems; autocomplete comes twice, once as a medium (top three
suggestions per typed prefix) and once as the full hard design with hot counts.
| Band | Stage | Problems | Rust habit |
|---|---|---|---|
| Easy | First tries | Longest common prefix · Implement trie† · Fix: indexing a `String` by position (E0277) · Longest word in dictionary · Map sum pairs | `[Option<Box<Node>>; 26]` |
| Medium | Tries at work | Replace words · Fix: recursive trie insert vs the borrow checker · Add & search words† · Search suggestions system (autocomplete) · Trie with counts and erase · Prefix and suffix search | `get_or_insert_with` cursors |
| Medium | String algorithms | Find the first occurrence (KMP) · Repeated substring pattern · Longest palindromic substring† · Palindromic substrings† · String to integer · Repeated DNA sequences | rolling hash with `wrapping_mul` |
| Hard | Hard tries & strings | Word search II† · Autocomplete system with hot counts (W31) · Max XOR (bit trie) · Stream of characters · Concatenated words · Palindrome pairs · Shortest palindrome (KMP) · Zero-copy tokenizer (W36) | lifetimes on `&str` slices |

### D11 · Recursion & backtracking: core · 28
Extended 2026-09-27: a pure-recursion stage first (base case, trust the recursive call, divide and conquer), then
backtracking, built up from the LeetCode 250 problems.
| Band | Stage | Problems | Rust habit |
|---|---|---|---|
| Easy | Recursion | Pow(x, n) (fast power) · K-th symbol in grammar · Tower of Hanoi · Merge sort · Fix: unbounded recursion (base case) | recursion on slices, `split_at` |
| Easy | First backtracking | Subsets · Combinations · Letter combinations · Binary watch | `&mut Vec` push / recurse / pop |
| Medium | Choices & grids | Subsets II · Permutations · Permutations II · Combination sum† · Combination sum II · Combination sum III · Generate parentheses · Different ways to add parentheses · Word search† · Palindrome partitioning · Restore IP addresses · Fix: recursive closure can't borrow the grid mutably | inner `fn` with explicit `&mut` params |
| Hard | Constraints & pruning | N-Queens (bitmasks) · N-Queens II · Sudoku solver · Matchsticks to square · Partition to K equal subsets · Word break II · Expression add operators | `u16` masks, sort-desc pruning |

### D12 · Dynamic programming: core · 57
Extended 2026-09-27 to cover the LeetCode 250 DP problems. Each stage opens with the smallest version of its idea
(memoised recursion, then the table, then the rolling variables).
| Band | Stage | Problems | Rust habit |
|---|---|---|---|
| Easy | 1-D basics | Fibonacci (memo vs table) · Climbing stairs† · Min cost climbing stairs · Pascal's triangle · Counting bits · House robber† · House robber II† | rolling variables |
| Medium | 1-D choices | Decode ways† · Delete and earn · Coin change† · Perfect squares · Coin change II · Word break† · Max product subarray† · LIS† (O(n²)) · LIS in O(n log n) · Russian doll envelopes | `partition_point` for LIS |
| Medium | 2-D grids | Unique paths† · Unique paths II · Minimum path sum · Triangle · Maximal square · Dungeon game | one-row rolling `Vec` |
| Medium | Strings | LCS† · Delete operation for two strings · Longest palindromic subsequence · Edit distance · Interleaving string · Distinct subsequences | `as_bytes()`, `u64` counts |
| Medium | Knapsack | 0/1 knapsack · Partition equal subset sum · Target sum · Last stone weight II · Ones and zeroes | iterate capacity backwards |
| Medium | State machines | Paint house · Stock with cooldown · Stock with fee · Stock III · Stock IV | state enums, `[i64; K]` |
| Hard | Intervals & games | Unique BSTs · Predict the winner · Stone game · Palindrome partitioning II · Min cost to cut a stick · Burst balloons | `dp[i][j]` by length |
| Hard | Bitmasks & digits | Count numbers with unique digits · Numbers at most N from a digit set · Can I win · Shortest path visiting all nodes · Ways to wear hats | `u32` masks, digit DP |
| Hard | Hard strings | Longest valid parentheses · Wildcard matching · Regular expression matching | `Vec<Vec<bool>>` vs rolling rows |
| Hard | DP the Rust way | Generic memoization engine (W44) · Fix: recursive memo closure (E0499) · Top-down → bottom-up rewrite · House robber III on a tree | owned `HashMap` keys |

### D13 · Matrix, bits & math: core · 16
| Band | Stage | Problems | Rust habit |
|---|---|---|---|
| Easy | Bits | Number of 1 bits† · Counting bits† · Missing number† · Reverse bits† · Single number · Power of two | `count_ones`, `reverse_bits` |
| Medium | Matrix & arithmetic | Rotate image† · Spiral matrix† · Set matrix zeroes† · Sum of two integers† · Pow(x, n) · GCD, LCM & sieve (W48) · Fix: overflow in debug vs release (B5) | `wrapping_*`, `checked_*` |
| Hard | Number theory | Modular exponent & inverse (W48) · Count primes, segmented sieve · Valid sudoku with bitmasks | `u64` / `u128` |

### D14 · Data-structure design: core · 20
The "design this class" round. Concurrent versions live in C1 and C2.
| Band | Stage | Problems | Rust habit |
|---|---|---|---|
| Easy | Small designs | Logger rate limiter · Moving average from a stream · Hit counter · HashSet with fixed buckets | `VecDeque`, `HashMap` |
| Medium | Classics | LRU cache (W8) · Token bucket (W14) · Sliding-window limiter (W90) · Cache with TTL · Snapshot array · My calendar · Insert / delete / getRandom O(1) · Peeking iterator · Flatten nested iterator · Stock price fluctuation | slab + index-linked list, `swap_remove`, injected clock |
| Hard | Hard designs | LFU cache · LRU-K (W59) · Consistent hashing ring (W85) · Hash map with open addressing · Range module · Snowflake IDs (W84) | no `Rc<RefCell>` on hot paths |

---

## 5. Section C · Concurrency & Async

### C1 · Threads & shared state: core · 22
| Band | Stage | Problems |
|---|---|---|
| Easy | Use | spawn & join · `move` closures · W70 `thread::scope` fork-join · (★) W26 parallel chunked sum with `Arc` |
| Medium | Understand | B9 Send / Sync misuse · Fix: `Rc` across threads · W2 thread-safe counter · B3 mutex deadlock & lock ordering · poisoning · `RwLock` for read-heavy data · (★) sharded map to cut contention |
| Hard | Build | W4 bounded blocking queue (`Condvar`, `while` not `if`) · W19 semaphore · W57 blocking connection pool · (★) W69 writer-preferring `RwLock` from scratch |

### C2 · Message passing: core · 16
| Band | Stage | Problems |
|---|---|---|
| Easy | Use | `mpsc` producer / consumer · (★) worker loop that exits when senders drop |
| Medium | Understand | W18 actor · W35 pub/sub broadcast · W5 thread pool · W34 pool that returns results · (★) graceful shutdown with no lost jobs |
| Hard | Build | (★) bounded channel from `Mutex` + `Condvar` · `select`-style receive across channels · backpressure policy: block, drop or error |

### C3 · Atomics & lock-free: SDE-3 · 14

*Written 2026-11-02 (7 of 14, a deeper set than the anchors below; see PROGRESS): atomic-stats, once-cell, spinlock, mini-arc, spsc-ring, index-stack (ABA), bounded-mpmc. The Treiber stack became the tagged-index stack; the others in the table are still to do.*
| Band | Stage | Problems |
|---|---|---|
| Medium | Understand | atomic counters · W77 Acquire / Release / Relaxed / SeqCst · `compare_exchange` loops · (★) Fix: `Relaxed` flag publishes stale data |
| Hard | Build | spinlock with correct ordering · W50 SPSC ring buffer · (★) W7 Treiber stack · the ABA problem and reclamation · MiniArc |

### C4 · Async & Tokio: core · 26
| Band | Stage | Problems |
|---|---|---|
| Easy | Use | `async fn` & `.await` · `#[tokio::main]` · `join!` vs sequential awaits · `tokio::spawn` · (★) concurrent HTTP-style fan-out with a limit |
| Medium | Understand | Fix: non-`Send` future across `.await` · B6 blocking the runtime → `spawn_blocking` · W76 std vs Tokio `Mutex` · `JoinSet` · W74 `select!` & cancellation · timeouts · W13 retry with backoff · (★) W75 mpsc pipeline with backpressure & graceful shutdown |
| Hard | Design | cancel-safety audit of a loop · structured concurrency with `CancellationToken` · (★) rate limiter shared across tasks · bounded work queue with worker tasks |

### C5 · Async internals: SDE-3 · 12
| Band | Stage | Problems |
|---|---|---|
| Medium | Understand | W12 hand-written `Future` · W24 async fn in traits · W73 desugaring to `Box<dyn Future>` · (★) W72 why self-referential state machines need `Pin` |
| Hard | Build | W71 timer with a real `Waker` · W58 single-threaded reactor · (★) mini executor → continues in project minirt |

### C6 · Data parallelism: light · 8
| Band | Stage | Problems |
|---|---|---|
| Medium | Use | rayon `par_iter` · parallel sort · (★) when parallelism makes it slower |
| Hard | Build | (★) chunked parallel pipeline without false sharing · work-stealing by hand (sketch) |

---

## 6. Section Y · Systems Rust (SDE-3)

### Y1 · Memory & layout: moved to F2 (layout) and F3 (arenas, mmap) on 2026-09-27

### Y2 · Unsafe Rust: 16
| Band | Stage | Problems |
|---|---|---|
| Easy | Use | raw pointer basics · `unsafe` blocks vs `unsafe fn` · (★) write the `// SAFETY:` comment |
| Medium | Understand | aliasing rules · B8 UB via aliasing · run it under Miri · `MaybeUninit` · (★) spot the unsound safe API |
| Hard | Build | (★) MiniVec with raw allocation · unsafe doubly linked list · W17 bump allocator |

### Y3 · FFI: 10
| Band | Stage | Problems |
|---|---|---|
| Easy | Use | call a C function · `#[repr(C)]` structs · (★) `CString` / `CStr` ownership |
| Medium | Understand | callbacks from C · panics across the boundary · (★) who frees this pointer? |
| Hard | Build | (★) safe Rust wrapper over a small C library (bindgen) |

### Y4 · Performance engineering: moved to F1 (measure, inlining, asm) and F5 (SIMD, locality) on 2026-09-27

### Y5 · Testing & verification: 14
| Band | Stage | Problems |
|---|---|---|
| Easy | Use | unit, integration & doc tests · (★) `#[should_panic]` vs `Result` tests |
| Medium | Understand | testable design with injected traits · proptest on an earlier solution · (★) find the bug the unit tests missed |
| Hard | Build | Miri on your unsafe code · loom model-check of a lock · (★) cargo-fuzz a parser |

---

## 6.1 Section F · Performance Rust (SDE-3)

What high-performance Rust looks like in production: databases, compilers, HPC, kernels and low-latency backends.
Every problem is a scenario taken from real code (the source is named where there is one), not a micro-benchmark
for its own sake. Owner's bar applies (HANDOFF §4): senior level, and every technique written by hand at least once.

**How these problems are graded.** Timing is the least reliable signal, so each problem uses the most exact check
that fits, in this order:
1. **Layout:** `size_of` / `align_of` assertions (exact).
2. **Work counters:** a counting global allocator (`anneal_prelude::allocs(|| ..)`), an instrumented `Hasher`,
   comparison counters. Exact and machine-independent.
3. **Assembly:** the runner emits the solution's release assembly and tests assert on one function's body: no
   `panic_bounds_check`, vector registers present, a `#[cold]` path moved out of line. Checks are written to pass on
   both x86_64 (the cloud runner) and aarch64 (the owner's Mac under OrbStack).
4. **Relative timing:** reference baseline and solution run in the same process, median of N, and the solution must
   beat the baseline by a wide margin (≥ 3–5×). Only for big wins, so machine load can't flip the result.

Problems opt in with `[perf]` in problem.toml (`release = true`, `asm = true`, `count_allocs = true`); release
problems run with `--test-threads=1`. Correctness tests (visible/hidden, `wrong/`) still apply as everywhere else.

### F1 · Measure & read the machine: SDE-3 · 12
| Band | Stage | Problems | Production source |
|---|---|---|---|
| Easy | Measure | `black_box` and the benchmark that measures nothing · count allocations in a hot loop · (★) Fix: a request handler that allocates 9 times per call → 0 | criterion, dhat |
| Medium | Read the code | bounds-check elimination (`assert!` up front, `chunks_exact`, iterators) · `#[inline]` / `#[inline(never)]` / `#[cold]` on an error path · generic vs `dyn` in a hot loop, read in the asm · (★) Fix: JSON field extractor 8× slower than it should be (bounds checks + `format!`) | rustc's `#[cold]` diagnostics paths, serde_json |
| Hard | Build settings | LTO / `codegen-units` / `panic = "abort"` / PGO: predict and explain · monomorphization bloat → the inner-fn trick, counted in asm symbols · (★) profile-guided fix of a log-scanner hot spot (counter-graded) | ripgrep, cargo-bloat |

### F2 · Data layout: SDE-3 · 14
| Band | Stage | Problems | Production source |
|---|---|---|---|
| Easy | Size it | padding and field order (`size_of` quiz, then shrink) · niches: `Option<NonZeroU32>`, `Option<Box<T>>`, `Option<&T>` · (★) bitflags in a `u8` vs six `bool`s, with a `const fn` API | bitflags, the Linux kernel's flag words |
| Medium | Pick the representation | box the large enum variant (`large_enum_variant`) · `Box<[T]>` / `Arc<str>` / `Arc<[T]>` vs `Vec` / `String` / `Arc<Vec<T>>` · SmallVec for mostly-small lists · inline small strings · `repr(C)` vs `repr(packed)` and E0793 (reference to a packed field) · (★) shrink an order-book price level from 96 B to 32 B with the same API | rustc's `TyKind` / `SmallVec` operands, compact_str, exchange order books |
| Hard | Locality | AoS → SoA for a particle update (timing-graded) · hot/cold field split · (★) a slotted page for a B-tree node: header, slot array, cell heap in one `[u8; 4096]` | SQLite and Postgres page layout |

### F3 · Memory & allocation: SDE-3 · 14
| Band | Stage | Problems | Production source |
|---|---|---|---|
| Easy | Allocate less | reuse buffers (`clear` vs new, `with_capacity`, W41 capacity planning) · `Cow` in a normaliser · (★) zero-copy request-line parser (`&'a str` out, 0 allocations) | hyper / httparse |
| Medium | Arenas and pools | bump arena for an AST (bumpalo) · typed-index arena with `struct ExprId(u32)` (W55 generational arena) · slab for connection state · string interner returning `Symbol(u32)` · (★) `Bytes`-style refcounted slices: split a buffer without copying | rustc `TypedArena` / `IndexVec` / `Symbol`, la-arena, tokio slab, the bytes crate |
| Hard | Allocators | counting `GlobalAlloc` · bump allocator behind `GlobalAlloc` (W17) · object pool with `Drop` returning to the pool · (★) buddy page allocator over a bitmap (kernel style) · W23 mmap scanner | Linux buddy allocator, jemalloc arenas |

### F4 · Hashing & purpose-built structures: SDE-3 · 16
| Band | Stage | Problems | Production source |
|---|---|---|---|
| Easy | Hashing | SipHash vs FxHash vs aHash: when each is right (HashDoS) · identity hasher for integer keys · (★) a `BuildHasher` for a hot `HashMap<u32, _>`, counter-graded | rustc-hash in rustc, nohash-hasher |
| Medium | Right structure for the job | precomputed hashes and hashbrown's raw-entry lookups · sorted `Vec` + binary search vs `BTreeMap` for read-mostly data · bitset and Roaring-style bitmap for row selection · bloom filter per SST · radix sort for `u64` keys · LRU on a slab with index links · (★) open addressing with SwissTable-style control bytes | hashbrown, Arrow/DataFusion selection vectors, RocksDB/sled bloom filters |
| Hard | Database cores | skiplist memtable · FST / trie route table · (★) buffer pool with CLOCK eviction and pin counts | RocksDB memtable, tantivy FST, Postgres buffer manager |

### F5 · CPU-level tricks: SDE-3 · 14
| Band | Stage | Problems | Production source |
|---|---|---|---|
| Easy | Branches | branchless select and clamp · lookup table for a classifier · (★) popcount / `trailing_zeros` iteration over a bitset | hashbrown's bitmask iteration |
| Medium | Vectorize | SWAR "has zero byte" newline scan · auto-vectorized sum with 8 accumulators, and why float order blocks it · division by a constant · prefix sums · (★) `std::arch` intrinsics (AVX2 and NEON behind `cfg`, runtime detection, scalar fallback) | memchr in ripgrep, simd-json |
| Hard | Cache and SIMD | loop tiling / cache blocking for matmul (timing-graded) · Arrow-style filter kernel with a selection bitmap · (★) CSV/log tokenizer at GB/s: SIMD-classify structural bytes | simd-json / simdjson stage 1, Polars kernels |

### F6 · Concurrency performance: SDE-3 · 16 (after C3)
| Band | Stage | Problems | Production source |
|---|---|---|---|
| Medium | Contention | false sharing and `CachePadded` / `repr(align(64))` · per-thread sharded counters · sharded `RwLock<HashMap>` · what `Relaxed` vs `SeqCst` / `fetch_add` vs a CAS loop costs · rayon granularity · (★) Fix: a metrics counter that doesn't scale past 2 threads | crossbeam-utils, dashmap, the Linux kernel's per-CPU counters |
| Medium | Waiting | `Condvar` vs spinning vs `thread::park`: a bounded work queue three ways, and when each wins · spin-then-park backoff · batching wake-ups · (★) Fix: a `Condvar` queue with lost wake-ups and a thundering herd | parking_lot, crossbeam's `Backoff`, tokio's blocking pool |
| Hard | Lock-free | SPSC ring buffer (submission-queue style) · seqlock · RCU config reload with arc-swap · Treiber stack with epoch reclamation · (★) lock-free HdrHistogram-style latency histogram | io_uring SQ/CQ, crossbeam-epoch, arc-swap |

### F7 · I/O & serialization: SDE-3 · 10
| Band | Stage | Problems | Production source |
|---|---|---|---|
| Easy | Output | `BufWriter` and `write!` vs `format!` · (★) itoa-style integer formatting into a stack buffer | itoa / ryu |
| Medium | Encodings | varint / LEB128 · delta + bit-packing for posting lists · serde borrowing (`&'de str`, `#[serde(borrow)]`) · bytemuck casts of `#[repr(C)]` records · (★) columnar vs row encoding of a batch | protobuf, tantivy, Arrow |
| Hard | Storage formats | (★) write-ahead-log records: length prefix, CRC32, zero-copy reader, torn-write recovery | RocksDB / etcd WAL |

**Capstones** (one per domain, each the ★ of its track above or a stage of kvlite): compiler front end (lexer +
interner + arena AST, F3/F5), database scan (columnar filter with a selection bitmap, F4/F5), HPC kernel (SoA +
tiled matmul, F2/F5), kernel memory (buddy + slab, F3), low-latency service (zero-alloc parser + sharded metrics, F1/F6).

**Runner support this section needs** (ROADMAP §7 step 3b): `[perf]` in problem.toml, release builds, the counting
allocator and timing helpers in the test prelude, assembly emission, and new vendored crates (bumpalo, smallvec,
hashbrown, rustc-hash, ahash, memchr, bytemuck, arc-swap, crossbeam-utils).

---

## 7. Section B · Backend Rust

### B1 · HTTP services with axum: core · 16
| Band | Stage | Problems |
|---|---|---|
| Easy | Use | router & handlers · `Path` / `Query` / `Json` extractors · shared `State` · (★) CRUD over an in-memory store |
| Medium | Understand | errors → `IntoResponse` · validation · W93 cursor pagination · API versioning · (★) custom extractor |
| Hard | Build | streaming responses & SSE · WebSockets · (★) long-polling endpoint with cancellation |

### B2 · Tower & middleware: core · 12
| Band | Stage | Problems |
|---|---|---|
| Easy | Use | timeout and trace layers · (★) request-id middleware |
| Medium | Understand | `Service` and `Layer` traits · API-key auth middleware · (★) per-route layers |
| Hard | Build | (★) a `Service` implemented by hand · rate-limit layer · concurrency limit & load shedding |

### B3 · Databases with sqlx: core · 14
| Band | Stage | Problems |
|---|---|---|
| Easy | Use | pool & `query_as` · migrations · (★) insert & fetch a row |
| Medium | Understand | transactions · map unique violations to 409 · `#[sqlx::test]` · N+1 queries · (★) optimistic locking with a version column |
| Hard | Build | (★) `FOR UPDATE SKIP LOCKED` work queue · W88 transactional outbox |

### B4 · Networking & protocols: core · 14
| Band | Stage | Problems |
|---|---|---|
| Easy | Use | W28 TCP echo server · (★) per-connection Tokio tasks |
| Medium | Understand | W89 length-prefixed codec · timeouts & keepalive · (★) W96 HTTP/1.1 request parser |
| Hard | Build | W97 L4 load-balancer strategies · TLS with rustls · (★) DoS-safe parser limits |

### B5 · Resilience: core · 12
| Band | Stage | Problems |
|---|---|---|
| Medium | Understand | W13 / W98 retries with backoff & jitter · W87 idempotency keys · W92 HMAC webhook verification · (★) timeouts on every hop |
| Hard | Build | (★) circuit breaker · W86 fencing-token lease · outbox relay |

### B6 · Observability & production: core · 12
| Band | Stage | Problems |
|---|---|---|
| Easy | Use | `tracing` spans & fields · (★) structured JSON logs |
| Medium | Understand | spans across `.await` · metrics · config from env · (★) graceful shutdown on SIGTERM |
| Hard | Build | (★) end-to-end request tracing · W91 sticky feature flags · health vs readiness |

---

## 8. Sections M, P, H · Build and design

### M1 · Idioms & patterns: core · 12
One per Rust-shaped pattern, from DP1–DP18 in the old packet, ordered easy → hard:
Newtype · Builder · RAII guard · Strategy (trait vs closure) · Iterator · Factory → `Box<dyn Trait>` ·
Adapter / decorator · Observer via channels · Command with undo · State machine enum · Visitor over an enum AST ·
Typestate builder.

### M2 · Machine coding: core · 12
Runnable, multi-file, tests given. Each has a core stage plus 1–2 **harden / extend** stages (the old packet's harden spec).
| Band | Problem | Source | Harden / extend |
|---|---|---|---|
| Medium | Rate limiter library | LLD3 | per-key limits · injected clock |
| Medium | In-process pub/sub bus | LLD6 | backpressure · unsubscribe on drop |
| Medium | Append-only logger | LLD4 | rotation · concurrent writers |
| Medium | Parking lot | LLD1 | concurrent entry & exit |
| Medium | Splitwise-lite ledger | LLD8 | debt simplification |
| Medium | Circuit breaker | LLD7 | half-open probes · async version |
| Hard | Movie ticket booking | LLD5 | no double booking under concurrency |
| Hard | Elevator system | LLD2 | scheduling policy as a trait |
| Hard | Price-time order book | TR1 | cancel / modify in O(log n) |
| Hard | Matching engine | TR2 | market orders · self-trade prevention |
| Hard | Pre-trade risk checks | TR4 | composable limit rules |
| Hard | Cached document store | W49 | query API · eviction |

### P · Projects
Only tests are given per stage. The repo carries forward and earlier stages' tests must stay green.
Each project's stages are ordered easy → hard.
| Project | Stages | Path | Built from |
|---|---|---|---|
| **shortline**: URL shortener API | health route → typed errors → create link → persistence → redirects → API-key auth → rate limiting → background jobs & graceful shutdown → tracing & metrics (9) | SDE-2 | B1–B3, B6 |
| **jobq**: Postgres job queue | schema & enqueue → `SKIP LOCKED` workers → leases & heartbeats (W86) → retries (W13, W98) → idempotency keys (W87) → outbox (W88) → shutdown & metrics (7) | SDE-3 | B3, B5, C4 |
| **kvlite**: key-value store | in-memory + CLI → WAL & recovery (W51) → compaction → memtable + SSTables (W56) → bloom filters → range scans → concurrent readers → benchmarks (8) | SDE-3 | S9, F1, F4, C1 |
| **minirt**: async executor | future by hand → single-threaded executor + Waker → timers → I/O reactor (W58) → spawn & JoinHandle → work-stealing pool (6) | SDE-3 | C5 |
| **raft-lite**: consensus | partitionable network → leader election (W82) → log replication → commit & apply → persistence → snapshots → linearizable reads → chaos tests (8) | stretch | C4, W80–W83 |

For SDE-3, finish shortline plus **one** of jobq (API / backend roles) or kvlite (storage / infra roles), plus minirt stages 1–4.

### H · System design
H1 fundamentals (old HLD1–12) → H2 API design (API1–10) → H3 scenarios (SD1–24, starting with URL shortener,
rate limiter, job queue, KV store, chat fan-out, notifications, news feed, payments ledger).
Its practice mode is deferred (§9); until then it's reading plus spoken mock rounds.

---

## 9. Mock rounds

| Round | Unlocks when | Format |
|---|---|---|
| Track capstone (each D track) | 75 % of the track solved | 45 min · 2 problems + 1 fix-this from the track |
| DSA round | D1–D3 done | 45 min · 2 unseen problems from finished D tracks |
| Rust depth round | L1, L2, S1, S6 at Medium | 45 min · 1 fix-this, 1 write-it, 3 spoken "why" questions |
| Machine coding round | three M1 problems | 75 min · one M2 problem, follow-ups arrive on a timer |
| Full loop | SDE-2 gate passed | 2 DSA + Rust depth + machine coding in one day |

Rubric: problem framing · representation · complexity stated first · correctness · Rust fluency · the fix-this · communication.
3.0 / 4 meets the SDE-2 bar, 3.3 the SDE-3 bar.

---

## 10. Order, schedule and gates

### Prerequisites
A stage opens when its prerequisite *stage* is solved or tested out, not the whole track.
| Track | Needs first |
|---|---|
| L2 Borrowing | L1 Easy |
| L3 Lifetimes | L2 Easy |
| S3–S5 collections | L1 Easy · S3 Hard needs Y2 Easy |
| S6 Iterators | L6 Easy · S6 Medium needs L5 Easy |
| S7 Smart pointers | L2 Easy · S7 Hard (MiniArc) needs C3 Medium |
| D1–D4 | L1 Easy, S3 Easy, S4 Easy |
| D5 Linked lists, D6 Trees | L2 Medium, S7 Easy |
| D7 Heaps, D9 shortest paths | S5 Medium |
| D12 DP (last stage) | L6 Medium |
| D14 Design | D1, S4 Medium |
| C1 | L1 Medium (closures), S7 Medium |
| C4 Async | C1 Easy, L3 Easy, L4 Easy |
| C5, P minirt | C4 Medium, Y2 Easy |
| B1–B3, shortline | L8 Medium, C4 Easy |
| M2 Machine coding | three M1 problems, L8 Medium |

### Schedule
Assumes about **12 hours a week**: five weekday sessions of 90 minutes plus a 4.5-hour weekend block.
The order holds at any pace. Rough effort: **SDE-2 path about 170 hours (~14 weeks)**, **SDE-3 adds about 120 (~10 weeks)**.
The week-1 test-out sweep usually removes 15–25 %.

**Weekday session:** 10 min warm-up (two syntax drills with rust-analyzer off) → re-solves due → one D problem → two or three L / S / C problems.
**Weekend block:** machine coding or project stages, plus a mock every other weekend from week 4.

**SDE-2 path: weeks 1–14**
| Week | DSA | Rust (L, S, C, B) | Build / mock |
|---|---|---|---|
| 1 | test-out sweep: D1–D4 | test-out sweep: L1, L2, S1, S3, S4 | baseline readiness |
| 2 | D1 | L1, S1, S3 Easy | — |
| 3 | D2 | L2 Easy–Medium, S4 Easy–Medium | D1 capstone |
| 4 | D3 | S2, S6 Easy, L7 | M1 × 2 · DSA mock |
| 5 | D4 | L4 Easy–Medium, S8 | M1 × 2 |
| 6 | D5 | S7 Easy–Medium, L5 Easy–Medium | shortline 1–2 · D2 capstone |
| 7 | D6 | L3 Easy–Medium, S6 Medium, L6 | shortline 3 · M1 × 2 · DSA mock |
| 8 | D9 Easy–Medium (stages 1–3) | L8, S5 | shortline 4 · M2 rate limiter |
| 9 | D9 stages 4–5, D7 Easy–Medium | C1 Easy–Medium, C2 Easy | shortline 5 · Rust depth mock |
| 10 | D8, D12 Easy | C4 Easy–Medium, B1 Easy | shortline 6 · M2 pub/sub bus |
| 11 | D12 Medium | C2 Medium, B2–B3 Easy–Medium | shortline 7 · DSA mock |
| 12 | D10 Easy–Medium, D11 Easy–Medium | S9, B6 Easy–Medium, L9 | shortline 8 · M2 logger |
| 13 | D13, D14 Easy–Medium | B4–B5 Medium, Y5 Easy–Medium | shortline 9 · full loop |
| 14 | re-solve backlog | weakest two tracks | **full loop × 2** · SDE-2 gate |

**SDE-3 path: weeks 15–24**
| Week | DSA | Rust | Build / mock |
|---|---|---|---|
| 15 | D14 Hard | L3 Hard, C1 Hard | jobq or kvlite 1–2 |
| 16 | D9 Hard (SCC) | C3, S11 | project 3–4 · M2 × 2 |
| 17 | D12 Hard | C4 Hard, C5 Medium | project 5–6 · machine coding mock |
| 18 | D6 Hard, D5 Hard | F2, Y2 | project 7–8 · full loop |
| 19 | D7 Hard, D8 Hard | C5 Hard, S3/S7 Hard builds | minirt 1–2 · M2 × 2 |
| 20 | D10 Hard, D11 Hard | F1, F3, L4–L5 Hard | minirt 3–4 · Rust depth mock |
| 21 | D9 flows, D13 Hard | B1–B6 Hard, Y3 | M2 × 2 |
| 22 | capstones for D7–D14 | L10, Y5 Hard, C6 | full loop |
| 23 | weakest stages by readiness | weakest tracks | M2 × 2 |
| 24 | re-solve backlog | weakest tracks | **full loop × 2** · SDE-3 gate |

System design (H) runs alongside from week 10 at about 1.5 hours a week.

### Gates
**SDE-2 ready** when all of these hold at once:
- ≥ 80 % of Easy + Medium problems in D1–D9 and D12 solved unassisted
- every core L and S track at ≥ 70 % of Easy + Medium; C1, C2, C4 at ≥ 50 %
- shortline complete
- in the last 3 weeks: two DSA mocks ≥ 3.0 and one Rust depth mock ≥ 3.0
- no re-solve overdue by more than 3 days

**SDE-3 ready** when all of these hold at once:
- the SDE-2 gate, still holding
- ≥ 70 % of Hard stages across D, L, S; C1–C5 ≥ 75 %; F1–F3, Y2 ≥ 50 %
- jobq or kvlite complete; minirt stages 1–4
- four M2 problems solved unassisted inside 75 minutes
- two full-loop mocks ≥ 3.3

---

## 11. Content inventory and authoring

| Section | Problems | Reused from the old packet | New to write |
|---|---|---|---|
| L | ~177 | B1, B4, DP14, DP17, DP18, W1, W6, W10, W11, W21, W25, W36–W40, W47 | ~140 |
| S | ~178 | W3, W9, W17, W22, W27, W29, W30, W32, W41 · ~490 harden subs as Medium drills | ~110 |
| D | 275 | Blind 75 · graphs seed (32) · W8, W14, W20, W31, W33, W42–W46, W48, W55, W59, W64, W84, W85, W90 | ~150 |
| C | ~98 | B3, B6, B9, W2, W4, W5, W7, W12, W13, W18, W19, W24, W26, W34, W35, W50, W57, W58, W69–W77 | ~70 |
| Y | ~40 | B8 | ~38 |
| F | ~96 | W17, W23, W41, W55 | ~92 |
| B | ~80 | W28, W86–W93, W96–W98 | ~70 |
| M | 24 | LLD1–8, TR1, TR2, TR4, DP1–18, W49 | tests + harden stages |
| P | 38 stages | W51, W56, W58, W71, W82, W86–W88 | tests for every stage |
| Warm-ups | 31 drills + 22 cheat-sheet pages | all | — |

**Authoring stays about two weeks ahead of the schedule.** Wave 1 is everything weeks 1–3 need: L1, L2, S1, S3, S4, D1, D2.
After that, one wave every two weeks. A problem leaves `draft` only when it has visible and hidden tests, 1–3 hints,
a reference solution with complexity, and an interview follow-up question.

**Old material set aside for a later "Depth" section:** W52, W54, W61–W63, W65–W68 (database and compiler internals),
W78–W81 and W83 (CRDTs and gossip), Web3 W3-1–12, Trading TR3 and TR5–TR8, and curriculum tracks 12, 14, 20–22, 24, 25.

## 12. Open decisions

- **System design (H) practice mode**: talk it through, then self-grade against a rubric? Until decided, H is reading plus spoken mocks.
- **"Read the real implementation" layer**: deferred.
- **Behavioral rounds**: out of scope.
- **App navigation and readiness** need re-cutting for sections. See `PLAN.md`.
