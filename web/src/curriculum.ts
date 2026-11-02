// The planned curriculum (docs/CURRICULUM.md), so section pages can show tracks
// that aren't written yet. Generated from the mockup's track data.
import type { Section, Tier } from "./api";

export interface PlannedTrack {
  code: string;
  name: string;
  /** Planned problem count. */
  planned: number;
  tier: Tier;
}

export const PLANNED: PlannedTrack[] = [
  { code: "D1", name: "Arrays & hashing", planned: 24, tier: "core" },
  { code: "D2", name: "Two pointers & windows", planned: 18, tier: "core" },
  { code: "D3", name: "Stacks & queues", planned: 14, tier: "core" },
  { code: "D4", name: "Binary search", planned: 14, tier: "core" },
  { code: "D5", name: "Linked lists", planned: 15, tier: "core" },
  { code: "D6", name: "Trees & BSTs", planned: 44, tier: "core" },
  { code: "D7", name: "Heaps & priority queues", planned: 13, tier: "core" },
  { code: "D8", name: "Intervals & greedy", planned: 29, tier: "core" },
  { code: "D9", name: "Graphs", planned: 58, tier: "core" },
  { code: "D10", name: "Tries & strings", planned: 25, tier: "core" },
  { code: "D11", name: "Recursion & backtracking", planned: 28, tier: "core" },
  { code: "D12", name: "Dynamic programming", planned: 57, tier: "core" },
  { code: "D13", name: "Matrix, bits & math", planned: 16, tier: "core" },
  { code: "D14", name: "Data-structure design", planned: 20, tier: "core" },
  { code: "L1", name: "Ownership & moves", planned: 20, tier: "core" },
  { code: "L2", name: "Borrowing", planned: 37, tier: "core" },
  { code: "L3", name: "Lifetimes", planned: 22, tier: "core" },
  { code: "L4", name: "Traits & dispatch", planned: 22, tier: "core" },
  { code: "L5", name: "Generics & associated types", planned: 16, tier: "core" },
  { code: "S1", name: "Option & Result", planned: 17, tier: "core" },
  { code: "S2", name: "Strings & text", planned: 18, tier: "core" },
  { code: "S3", name: "Vec & slices", planned: 20, tier: "core" },
  { code: "S4", name: "Maps & sets", planned: 11, tier: "core" },
  { code: "C1", name: "Threads & shared state", planned: 3, tier: "core" },
  { code: "C2", name: "Message passing", planned: 2, tier: "core" },
  { code: "C3", name: "Atomics & lock-free", planned: 7, tier: "sde3" },
  { code: "C5", name: "Async internals", planned: 3, tier: "sde3" },
  { code: "Y2", name: "Unsafe Rust", planned: 3, tier: "sde3" },
  { code: "Y3", name: "FFI: C and Rust together", planned: 4, tier: "sde3" },
  { code: "Y6", name: "WebAssembly from scratch", planned: 2, tier: "sde3" },
  { code: "Y7", name: "Raft consensus rules", planned: 3, tier: "sde3" },
  { code: "Y8", name: "Classic engines: regex and diff", planned: 2, tier: "sde3" },
  { code: "Y9", name: "Capstone builds: parts of real systems", planned: 3, tier: "sde3" },
  { code: "Y10", name: "Search engine core", planned: 2, tier: "sde3" },
  { code: "F2", name: "Data layout", planned: 14, tier: "sde3" },
  { code: "F3", name: "Memory & allocation", planned: 4, tier: "sde3" },
  { code: "F4", name: "Hashing & purpose-built structures", planned: 4, tier: "sde3" },
  { code: "F5", name: "CPU-level tricks", planned: 4, tier: "sde3" },
  { code: "F7", name: "I/O & serialization", planned: 2, tier: "sde3" },
];

export const SECTION_NAMES: Record<Section, string> = {
  D: "Data Structures & Algorithms",
  L: "Rust Language",
  S: "Rust Standard Library",
  C: "Concurrency & Async",
  Y: "Systems Rust",
  F: "Performance Rust",
  P: "DSA Practice",
};

/** Which nav area each section belongs to. */
export const NAV_SECTIONS = {
  dsa: ["D"],
  rust: ["L", "S", "C", "Y", "F"],
} as const satisfies Record<string, Section[]>;

export type NavArea = keyof typeof NAV_SECTIONS;

/** Is this section part of the Rust area? (The DSA tracks have their own area.) */
export const isRustSection = (s: Section) => (NAV_SECTIONS.rust as readonly Section[]).includes(s);

export const sectionOf = (code: string) => code[0] as Section;

/** One line per track: what interviewers probe. Written tracks show their own summary instead. */
export const BLURB: Record<string, string> = {
  D1: "entry API \u00b7 [u8; 26] counts \u00b7 get() vs [] \u00b7 total_cmp",
  D2: "bytes() \u00b7 checked_sub on usize \u00b7 monotonic VecDeque",
  D3: "Vec as a stack \u00b7 Option + ? on malformed input",
  D4: "partition_point \u00b7 binary_search_by \u00b7 BTreeMap::range",
  D5: "Option<Box<Node>> \u00b7 take() \u00b7 cursors \u00b7 index arenas",
  D6: "Rc<RefCell<TreeNode>> vs arenas \u00b7 iterators with lifetimes",
  D7: "BinaryHeap is a max-heap: Reverse<T>, Ord newtypes",
  D8: "sort_unstable_by_key \u00b7 sweep lines \u00b7 BTreeMap counts",
  D9: "index-based graphs \u00b7 union-find \u00b7 BinaryHeap",
  D10: "[Option<Box<Node>>; 26] \u00b7 get_or_insert_with \u00b7 bytes vs chars",
  D11: "&mut Vec push / recurse / pop \u00b7 bitmask pruning",
  D12: "rolling arrays \u00b7 owned memo keys \u00b7 recursive-closure pitfalls",
  D13: "count_ones \u00b7 wrapping_* and checked_* \u00b7 overflow in debug vs release",
  D14: "slab + index-linked lists \u00b7 injected clocks \u00b7 no Rc<RefCell> on hot paths",
  L1: "Whether you think in moves and drops, or reach for .clone() to make errors go away.",
  L2: "Whether you reason in aliasing and mutation rules instead of fighting the compiler.",
  L3: "Whether you can say why the compiler rejects code, not just add 'a until it compiles.",
  L4: "API taste: static vs dynamic dispatch, object safety, and when dyn is the right cost.",
  L5: "Bounds that say exactly what you need, and knowing what monomorphization costs.",
  L6: "Which Fn trait a closure gets, and returning closures without boxing by reflex.",
  L7: "Enums as state machines, let-else, and bindings that don't move by accident.",
  L8: "thiserror for libraries, anyhow for apps, and no unwrap on input you don't control.",
  L9: "Crate boundaries, features and semver: which changes break your users.",
  L10: "When a macro is warranted, hygiene, and reading someone else's macro_rules!.",
  S1: "Combinators over match, and keeping sentinels out of your APIs.",
  S2: "String vs &str, UTF-8 boundaries, and allocating only when you must.",
  S3: "The cost model: capacity, reallocation, and slices as views.",
  S4: "The entry API, BTreeMap ranges, and Hash + Eq consistency.",
  S5: "BinaryHeap ordering, Reverse<T>, and VecDeque's ring buffer.",
  S6: "Lazy chains, IntoIterator for references, and writing your own adapters.",
  S7: "Picking the cheapest correct tool: &T, Box, Rc, Arc, Arc<Mutex<T>>.",
  S8: "Which traits to derive, which to write by hand, and what Borrow is for.",
  S9: "Generic Read / Write code you can test without files.",
  S10: "Instant vs SystemTime, and making time injectable for tests.",
  S11: "size_of, niche optimisation, MaybeUninit: what std does under the hood.",
  C1: "Which primitive and which lock scope, not just 'add a Mutex'.",
  C2: "Channels, actors and shutdown without losing work.",
  C3: "Which Ordering and why, and when lock-free is not worth it.",
  C4: "The executor model: cooperative scheduling, why blocking is catastrophic, cancellation at .await.",
  C5: "Future, Waker and Pin: what .await compiles into.",
  C6: "When parallelism helps, and when false sharing eats the gain.",
  Y2: "Stating the safety contract you opt into, and writing // SAFETY: unprompted.",
  Y3: "Ownership across the C boundary: who allocates and who frees.",
  Y5: "Tests that find bugs: property tests, Miri, loom.",
  F1: "Measure before you guess: allocation counts, black_box, and reading the assembly.",
  F2: "Bytes per item: padding, niches, bitflags, Box<[T]> and struct-of-arrays.",
  F3: "Allocate less: buffer reuse, arenas with typed indices, interners, custom allocators.",
  F4: "The hasher and the structure that fit the workload, the way databases pick them.",
  F5: "Branchless code, SWAR, auto-vectorization and std::arch intrinsics.",
  F6: "False sharing, sharded counters, and lock-free structures that actually scale.",
  F7: "Zero-copy parsing, compact encodings and write-ahead logs.",
};

/** DSA tracks in the order CURRICULUM.md recommends (graphs before heaps and greedy). */
export const DSA_ORDER = ["D1", "D2", "D3", "D4", "D5", "D6", "D9", "D7", "D8", "D12", "D10", "D11", "D13", "D14"];
