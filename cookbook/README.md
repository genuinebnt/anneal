# FlowForge Cookbook

Small runnable Rust recipes for concepts you will meet while building FlowForge. Each recipe states the
pattern, where you will use it, and a real repo that does it for real. They deliberately use *other domains*
(recipes, transit maps, print queues) so they teach the technique without giving away stage solutions.

Run: `cd cookbook && cargo run --release --example index_graph`

| Recipe | Concept | Used in |
|---|---|---|
| serde_defaults | typed JSON, defaults, strict rejection | stages 1, 3 |
| error_enum | error enum, `?`, thin `main` | stages 2, 4 |
| newtype_ids | newtype ids, typed indices | stages 3, 5 |
| index_graph | interning, `Vec<Vec<id>>`, iterative BFS | stages 5, 7, 8 |
| heap_tiebreak | `BinaryHeap` + `Reverse`, deterministic ties | stage 7 |
| bufwriter | buffered output (about 200x faster here) | stages 6, 8 |
| joinset_semaphore | bounded concurrency with a proven limit | concurrent executor |
| arena_ids | arena of nodes with typed ids | stage 8, future DSL |

Add a recipe whenever a stage introduces a pattern you could not invent alone. Ask your mentor.
