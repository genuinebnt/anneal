# 09. Collaboration and editors

CRDTs, OT, text buffers, markdown, block editors and offline-first sync. Prerequisite: `paths/tracks/07_distributed_systems.md` (CRDT/OT sections) and `04_data_structures_algorithms.md`. Recipe stems are `folder/stem` under `examples/`. 🔜 = planned, not written yet (read the linked docs instead).

Test strategy for the whole theme: **convergence is an invariant, so fuzz it.** Generate random operation sequences on K replicas, deliver in every permitted order (shuffled, duplicated, delayed), and assert all replicas end byte-identical. Print the seed on failure; shrink the failing history (`dev_tools/shrinking`).

---

## 9.1 `undo`: a command-pattern undo/redo stack for a text buffer  `S`

**Goal.** A text buffer with insert/delete/replace commands, unlimited undo/redo, coalescing of typing runs, and a "dirty since save" flag, tested against snapshots.
**Learn.** Command pattern with inverse operations, memento vs command, coalescing rules, redo invalidation, memory growth.
**Read before.** `patterns/undo_redo_editors`, `patterns/design_patterns_high`. **During.** `language/drop_raii`, `language/pattern_matching`, `dev_tools/testing_patterns`.
**Milestones.**
1. `Command::{Insert{at,text}, Delete{at,len,removed}}` with `apply` and `invert`.
2. Undo/redo stacks; a new edit after undo clears redo.
3. Coalesce consecutive single-character inserts at adjacent positions into one undo step (stop at whitespace or 1 s gap, fake clock).
4. Save point tracking: `is_dirty()` is false after undo returns to the saved state.
5. Cap history at 1,000 steps or 10 MB; oldest dropped.
**Acceptance.** Random edit sequences (10,000 ops, mixed insert/delete): after k undos then k redos the text equals the original; full undo gives the initial text; every state equals a snapshot oracle (`Vec<String>` of states); memory stays under the cap.
**Data.** Seeded edit generator on a 5,000-char document with UTF-8 multi-byte characters (undo must respect char boundaries).
**Write-up.** What do you store in `Delete` and why? How do selection/cursor positions fit into undo?
**Stretch.** Branching history (undo tree) like Vim `:undotree`.
**Resembles.** [Helix](https://github.com/helix-editor/helix) transactions, [Zed](https://github.com/zed-industries/zed).

## 9.2 `gapbuf` -> `rope`: text buffer data structures  `M`

**Goal.** Implement the same `TextBuffer` trait with a gap buffer, a piece table and a rope (balanced tree of chunks), then compare their edit cost patterns and offsets APIs (byte, char, line).
**Learn.** Amortized insertion costs, cursor locality, tree balancing, chunk size tuning (e.g. 512 B), line index maintenance, UTF-8 boundaries, O(log n) offset translation.
**Read before.** `data_structures/treap_interval`, `data_structures/segment_fenwick`, `patterns/undo_redo_editors`. **During.** `data_structures/skiplist`, `language/strings`, 🔜 `data_structures/rope_piece_table` (planned; read the `ropey` source meanwhile).
**Milestones.**
1. Trait + a naive `String`-backed oracle.
2. Gap buffer with growth policy; test cursor-local typing vs random jumps.
3. Piece table with original + add buffer and a `Vec<Piece>`; coalesce adjacent pieces.
4. Rope as a B-tree/treap of chunks <= 512 bytes with cached (bytes, chars, lines) per node.
5. APIs: `byte_to_line`, `line_to_byte`, `slice(range)`, `char_at`, `insert`, `delete`.
**Acceptance.** 100,000 random operations equal the oracle for all three structures; on a 100 MB file the rope inserts in the middle in under 20 us p99 (the naive `String` takes milliseconds); gap buffer wins on sequential typing (report ratio); line index consistent after every op.
**Data.** Generated text with long lines, no trailing newline, CRLF mix, emoji and combining characters; a 100 MB file from repeated Gutenberg text.
**Write-up.** When is a gap buffer the better choice? How do chunk boundaries avoid splitting a UTF-8 character or a CRLF pair?
**Stretch.** Persistent (immutable) rope with `Arc` chunks enabling O(1) snapshots for undo and background tasks.
**Resembles.** [ropey](https://github.com/cessen/ropey), [Zed's rope](https://github.com/zed-industries/zed), VS Code piece tree.

## 9.3 `mdparse`: a CommonMark-subset parser to AST and HTML  `M`

**Goal.** Parse a CommonMark subset (ATX headings, paragraphs, emphasis/strong, inline code, fenced code blocks, links, images, block quotes, ordered/unordered nested lists, thematic breaks, hard breaks) to an AST, then render HTML and re-emit normalized Markdown.
**Learn.** Two-phase block then inline parsing, container block stack, delimiter-run algorithm for emphasis, escaping and XSS safety, source positions, the spec's example-driven testing.
**Read before.** `text_and_parsing/parser_combinators`, `text_and_parsing/zero_copy_lexer`, `errors/diagnostics_spans`. **During.** `text_and_parsing/pratt_parser`, `language/cow`, 🔜 `text_and_parsing/markdown_blocks_inlines` (planned; read the CommonMark spec appendix "A parsing strategy").
**Milestones.**
1. Block structure: headings, paragraphs, thematic breaks, fenced code; each line classified by a prefix check.
2. Lists and block quotes as container blocks with lazy continuation.
3. Inline parser: code spans, `*em*`/`**strong**` using the delimiter stack, links `[t](u "title")`, backslash escapes.
4. HTML renderer with escaping of `& < > "` and `javascript:` URL rejection.
5. Markdown re-emitter: `parse(emit(parse(x))) == parse(x)` (round trip on the AST).
**Acceptance.** The CommonMark spec's example JSON (`spec.json`) restricted to your subset: at least 90% of selected sections pass (report per section); 5,000 random-mutation fuzz inputs never panic or exceed 50 ms; XSS corpus of 20 payloads is neutralized.
**Data.** `spec.json` from commonmark.org; a corpus of READMEs (clone 20 Rust crates, parse their `README.md`).
**Write-up.** Why can inline parsing not be done in the same pass as block parsing? How do you represent source spans for each node?
**Stretch.** Tables and task lists (GFM); incremental reparse of only changed blocks.
**Resembles.** [pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark), [CommonMark reference](https://github.com/commonmark/commonmark-spec).

## 9.4 `blocks`: a block-editor document model with markdown import/export  `M`

**Goal.** A Notion-like document model: a tree of typed blocks (paragraph, heading 1-3, todo, bullet, code, quote, divider) each with rich text spans (bold, italic, link, code), operations (insert/move/delete/indent/convert), stable block ids, and lossless markdown import/export.
**Learn.** Tree + text hybrid models (ProseMirror-style), schema constraints, position addressing by id not index, operations as data, normalization, serialization versioning.
**Read before.** `patterns/undo_redo_editors`, `patterns/typestate_builder`, `encoding_compression/serde_custom`. **During.** `language/iterators`, `data_structures/trie_radix`, `memory/arena_ids`, 🔜 `text_and_parsing/markdown_blocks_inlines`.
**Milestones.**
1. Types: `BlockId` newtype, `Block{kind, text: Vec<Span>, children}`; schema validator (e.g. headings cannot nest).
2. Ops: `InsertBlock(parent, after, block)`, `MoveBlock`, `DeleteBlock`, `SetKind`, `EditText(block, range, new_spans)`; all invertible.
3. Markdown import via 9.3; export via a visitor; stable under export -> import -> export.
4. Rich-text normalization: adjacent spans with identical marks merge; empty spans vanish.
5. JSON snapshot with `schema_version` and a migration from v1 to v2 (add a field with default).
**Acceptance.** Property: 2,000 random op sequences leave the document schema-valid; undo of the entire sequence returns the empty document; `export(import(md)) == normalize(md)` for 100 README files; ids are stable across moves.
**Data.** 100 README.md files, plus a generator of valid random documents (depth <= 4).
**Write-up.** Why ids, not indexes, in operations? What is lost on markdown export (colors, block ids) and where do you stash it?
**Stretch.** Collaborative version on top of 9.6/9.8; slash-command menu logic as a pure function.
**Resembles.** [ProseMirror](https://github.com/ProseMirror/prosemirror) schema and transforms.

## 9.5 `tododoc`: a TODO app with CRDT sync and no database  `M`

**Goal.** A CLI/TUI TODO list stored in a single file (`todos.crdt`) that two devices edit independently and merge with `todo merge other.crdt`, with add/rename/complete/reorder/delete converging deterministically, and no server or DB.
**Learn.** State-based vs op-based CRDTs, OR-Set with tombstones, LWW-Register with HLC, fractional indexing for ordering, deterministic file format, merge idempotence, causal metadata size.
**Read before.** `distributed/crdts`, `distributed/vector_clocks_hlc`, `encoding_compression/serde_custom`. **During.** `os_and_storage/file_locking_atomic_replace`, `dev_tools/clap_cli`, `data_structures/btree_bulk_load_prefix_compression` (for key ordering ideas).
**Milestones.**
1. Item = id (replica id + counter) with LWW fields: title, done, position (fractional index string).
2. OR-Set of items with tombstones; `merge(a,b)` is a pure function.
3. Reorder using fractional indexing between neighbours (e.g. base-62 strings); tie-break by id.
4. File: write atomically (temp + rename); `merge` command reads another file and writes the result.
5. Tombstone GC once all known replicas have acknowledged (version vector per replica).
**Acceptance.** Merge is commutative, associative and idempotent on 2,000 random states (compare serialized bytes); simulate 3 replicas doing 500 random edits offline, merging in random pairs: final states byte-identical; a delete concurrent with a rename keeps the item (add-wins) and the doc explains this choice.
**Data.** Seeded op generator; three "devices" as directories.
**Write-up.** What happens on concurrent complete and delete? How large does the file grow after 100,000 edits and what do you GC?
**Stretch.** Sync over a shared folder (Dropbox) or `scp`; encrypt the file with `chacha20-poly1305`.
**Resembles.** [Automerge](https://github.com/automerge/automerge), [Yjs](https://github.com/yjs/yjs), Ink & Switch's [local-first software](https://www.inkandswitch.com/local-first/) essay.

## 9.6 `rga`: a sequence CRDT for collaborative text  `L`

**Goal.** Implement a text CRDT (RGA or Logoot/Fugue-like) with insert/delete by stable element ids, causal delivery, and convergence for 5 replicas, plus an efficient index (run-length merging of consecutive inserts) so a 100k-character document stays small.
**Learn.** Position identifiers, interleaving anomalies, tombstones and GC, run-length encoding of ops, causal broadcast with version vectors, memory vs ops trade-off.
**Read before.** `distributed/crdts`, `distributed/operational_transform`, `distributed/vector_clocks_hlc`. **During.** `data_structures/skiplist`, `data_structures/segment_fenwick`, `data_structures/treap_interval`, 🔜 `distributed/rga_text_crdt` (planned; read the Yjs internals doc).
**Milestones.**
1. Element = `(id{replica,counter}, origin_left, char, deleted)`; integrate rule that orders concurrent inserts after the same origin by id.
2. Local edit API by index (`insert_at(i, ch)`, `delete_at(i)`) translating to ids via an index structure.
3. Causal delivery buffer (hold ops until dependencies are present).
4. Random test: 5 replicas, 2,000 ops each, ops delivered in random causal orders with duplicates; all converge.
5. Run-length compress: a typed word of 8 chars is one run; measure items for a 100k-char document typed linearly (should be near 1) vs random inserts.
**Acceptance.** Convergence on 1,000 seeds; intention preservation tests (two users insert at the same place: both texts present, not interleaved character by character for runs typed consecutively, if you implement the Fugue/YATA fix, else demonstrate and document the anomaly); local edit to index translation O(log n) measured on 1M elements.
**Data.** Seeded multi-replica traces; the Martin Kleppmann "editing traces" dataset (`automerge-perf` / `editing-trace` repositories) for realistic 260k-op replays.
**Write-up.** How do tombstones affect memory and index lookups? What causes interleaving anomalies and how does your design address them?
**Stretch.** Binary encoding of ops (varint), snapshot + delta sync, GC with stability point.
**Resembles.** [Yjs](https://github.com/yjs/yjs), [Automerge](https://github.com/automerge/automerge).

## 9.7 `otsrv`: an operational-transform text server  `L`

**Goal.** A central-server OT system: clients send ops against a known revision, the server transforms against concurrent ops, assigns a total order, and broadcasts; clients transform pending local ops against incoming ones (Jupiter-style) to guarantee convergence.
**Learn.** Transformation functions (insert/delete pairs), TP1 property, client state machine (synchronized / awaiting ack / awaiting with buffer), why central ordering avoids TP2, composition of ops.
**Read before.** `distributed/operational_transform`, `patterns/state_machine`, `patterns/undo_redo_editors`. **During.** `networking/websocket_frames`, `web/axum_sse_replay`, `distributed/linearizability_checker`.
**Milestones.**
1. Op type as a sequence of retain/insert/delete components; `apply`, `compose`, `transform`.
2. Property tests: `apply(apply(d, a), transform(b, a).1) == apply(apply(d, b), transform(a, b).0)` (TP1) for 10,000 random pairs.
3. Server with revision log; client state machine with at most one op in flight.
4. In-process simulation of 5 clients with random delays: all converge to the server's text.
5. WebSocket transport and a tiny browser or terminal client.
**Acceptance.** TP1 holds on 100,000 random (doc, a, b) triples; 1,000 seeded simulations converge; the server's log replayed from scratch equals the final text; a client reconnecting with stale revision 50 catches up correctly.
**Data.** Random op generator over 100-char documents with Unicode scalar values.
**Write-up.** Why does a central server make OT tractable? Compare OT vs CRDT for memory, offline use and server complexity.
**Stretch.** Cursor/selection transformation and presence; undo in a collaborative setting (selective undo).
**Resembles.** Google Docs-style OT; [ProseMirror collab](https://github.com/ProseMirror/prosemirror).

## 9.8 `syncd`: an offline-first sync engine  `L`

**Goal.** A library + server providing local-first storage (SQLite or an append-only op log), an outbox of local ops, pull/push sync with cursors, conflict policy per field (LWW with HLC, set-union, or manual), tombstone GC, and resumable, idempotent sync after network failure.
**Learn.** Sync protocols (cursor-based, versions vectors), HLC timestamps, idempotent ops, delta compression, schema migrations across offline clients, partial replication.
**Read before.** `distributed/vector_clocks_hlc`, `distributed/crdts`, `web/axum_idempotency`. **During.** `os_and_storage/wal_append_log`, `distributed/retry_backoff_jitter`, `web/http_caching_conditional`, `distributed/clock_sync_and_trace_context`.
**Milestones.**
1. Local op log with HLC timestamps and device ids; state derived by replay.
2. Server stores per-user op log; `push(ops)` idempotent by op id; `pull(cursor)` returns ops after cursor.
3. Merge policies: LWW-per-field, add-wins set, counter (PN).
4. Failure handling: connection drop mid-push; resume without duplicates (assert via op-id set).
5. Compaction: snapshot every 10,000 ops, drop ops older than all clients' acked cursors.
6. Migration: client at schema v1 syncs with v2 peers (unknown fields preserved).
**Acceptance.** 5 simulated devices, 1,000 ops each with random offline periods and a clock skew up to 5 minutes: all converge to identical state hash; a network fault injector (drop/duplicate/reorder 10%) never causes a lost or duplicated op; after compaction a fresh device bootstraps from snapshot and equals the others.
**Data.** Generator of todo-like and note-like ops; skew and fault schedules from the seed.
**Write-up.** What do HLCs fix and not fix about clock skew? How do you handle a device that returns after 6 months (beyond compaction horizon)?
**Stretch.** End-to-end encryption of ops (server sees only ciphertext + ids); partial sync by collection.
**Resembles.** [Automerge sync](https://github.com/automerge/automerge), Replicache/Linear sync, Ink & Switch [local-first](https://www.inkandswitch.com/local-first/).

---

## Katas (30 minutes each)

1. Implement a G-Set and a 2P-Set; show the 2P-Set cannot re-add a removed element, with a test.
2. Build an LWW-Register whose ties break on replica id; property-test commutativity over 1,000 random update pairs.
3. Write fractional-index `between(a, b)` over base-62 strings; insert 1,000 items always at the front and report max key length (it shows why rebalancing exists).
4. Implement `transform(insert, insert)` for position ties with a replica-id tiebreak; test TP1 on 1,000 random cases.
5. Convert byte offsets to (line, UTF-16 column) for text with emoji and combining marks; 20 hand-checked cases.
6. Write a `diff` of two `Vec<&str>` using LCS (dp) and apply the diff as ops to the first, asserting equality with the second on 500 random pairs.
7. Parse a Markdown heading list into a nested outline tree and print it with indentation; 10 cases including skipped levels (`#` then `###`).
8. Implement a version vector with `dominates`, `concurrent`, `merge`; exhaustively test all 3-replica vectors with counters 0..2.
9. Write an HTML escaper and test it with 15 XSS payloads (`<script>`, `" onload=`, `javascript:`), asserting no raw `<` survives in text nodes.
10. Build a grapheme-aware `truncate(s, n)` using the `unicode-segmentation` crate and test with family emoji, flags and combining accents.
