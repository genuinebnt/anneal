# The Idea tab of a pattern

Status: **rollout in progress, one pattern per commit. Done: Sliding Window, Two Pointers, Binary Search, Stack, Linked List (5 of 18).** Approved by the owner from the mockup
(https://claude.ai/artifact/1pB91P8AtbqRu6fqmkSJdC). It is a **fourth tab** on a pattern's page (Idea, Learn, Problems, Practice). The Learn
tab and `docs/DSA_LEARN_PAGE_SPEC.md` are untouched, and 1, 2, 3 keep their meaning; `I` opens Idea. The tab only appears for a pattern that has a
file in `content/dsa/ideas/` (`has_idea` on the pattern in `GET /api/dsa`).

The owner's brief: explain the pattern with an animation, how to apply it, where to apply it, how to tell it is this pattern and which
variation, a general explanation, **how it works with other patterns**, **syntax highlighting, good animation, colours and effects**, and
**time and space complexity with the reason**. No interview framing (the owner removed the interview layer).

## What is on the page, in order

| section | field in the file | notes |
|---|---|---|
| The idea in five seconds | `tagline` | a looping drawing of the pattern (`components/PatternLoop.tsx`, always moving, still under Reduce motion) beside the pattern name and one sentence (at most 140 characters). The same drawing and sentence are meant for the pattern's card on the DSA home. A pattern with no drawing in `LOOP_OF` shows no header. |
| What it is | `what` | two short paragraphs: the idea, then the saving. `**bold**`, `*italic*` and backticks only. |
| Watch it run | `demos` | one or two animations, each with the Python beside it (running lines lit), the readings, a counter of what it saves, an input the learner can change, a step scrubber and keys. |
| Where it applies | `applies` | the shape of the question and the input. |
| Is it this pattern? | `yes`, `no` | when it fits, and when it does not and what to use instead. |
| Which one? | `[[which]]` | a row per variation: what the question says, a small picture (`glyph`), the kind, the move, and the technique card it links to (`technique` is a technique or extra id of the pattern; the link is `/dsa/patterns/<code>#t-<slug>`). |
| Time and space | `[[cost]]` | the plain way first for comparison, then each variation: time, space and **why** in a sentence or two. |
| With other patterns | `[[with]]` | how it meets another pattern, and a catalog problem (`example` is a slug) that shows the two together. The link goes to that pattern's Idea tab if it has one, else its Learn tab. |
| How to apply it | `apply` | numbered steps, ending with the invariant. |

## Files

- Text: `content/dsa/ideas/<pattern-slug>.toml`. Top-level keys first (`pattern`, `what`, `demos`, `applies`, `yes`, `no`, `apply`), then the
  `[[which]]`, `[[cost]]` and `[[with]]` tables (TOML puts any key after a table inside that table).
- Loader and checks: `load_ideas` in `crates/content/src/dsa.rs`. `anneal-cli verify` and CI fail on: an unknown pattern, two files for one
  pattern, an empty section, a demo or glyph the web app cannot draw (`IDEA_DEMOS`, `IDEA_GLYPHS`), a `which` row whose technique is not the
  pattern's, a `with` row for a pattern that does not exist or for itself, an example that is not a catalog problem, an empty `cost` field.
  Tests: `crates/content/tests/dsa.rs` (`an_idea_tab_loads_and_every_way_it_can_be_wrong_is_reported`, `the_sites_own_idea_tabs_load_without_issues`).
- API: `GET /api/dsa/patterns/{code}` carries `idea` (its `with` rows resolved to a pattern code, `has_idea`, and the example's title and number).
- Web: `pages/PatternIdea.tsx` (the page), `components/IdeaDemo.tsx` (the lab), `components/IdeaGlyph.tsx` (the pictures), `components/rich.tsx`,
  `ideaDemos.ts` (the simulators), `styles/idea.css`. Python highlighting is `highlightPy` in `components/dsaBits.tsx`, the same one as the Learn page.
- Tests: `web/src/ideaDemos.test.ts` (`node --test`), `web/e2e/idea.spec.ts`.

## Adding a pattern

1. Write `content/dsa/ideas/<slug>.toml` from the Sliding Window file. Say why in the cost rows; do not just list O().
2. If the pattern needs a new animation: add a simulator to `ideaDemos.ts` (a `Demo`: `title`, `code`, `initial`, `parse`, `simulate`; the
   frames say which cells are in the window, which entered or left, which code lines run, the readings and what it saves) and its name to
   `IDEA_DEMOS` in `crates/content/src/dsa.rs`. A new picture for the "which" table goes in `IdeaGlyph.tsx` and `IDEA_GLYPHS`. Add a unit
   test for the simulator.
3. `cargo run -q -p anneal-cli -- verify`, `cargo test -p anneal-content`, `pnpm exec playwright test e2e/idea.spec.ts` (extend it for the pattern).
4. Look at it in light and dark and at 390 px before shipping. Animations must still read with `data-motion="reduce"` (the global rule in
   `ui.css` removes the durations).

## The frame an animation produces

`Frame` in `web/src/ideaDemos.ts`: `win` (a brace over `[l, r)`), `pointers` (labelled markers, `above` or `below` the row; two on one cell stack), `dim` (ruled out), `cur` (being looked at), `hit` (the answer), `enter`/`leave`/`dup`, `cells` (new values when the array is written in place), `stack` (a second structure beside the row), `links` (the arrows of a linked list; a demo with `wide: true` spaces its cells to fit them, and a link that reaches back is drawn as an arc under the row), `lines` (code lines that run), `read` (readings), `work` (bars of what it saves, `ratio` for "n of total"), `say`, `result`. A new pattern also needs a drawing in `PatternLoop.tsx` (and its name in `LOOP_OF`) and a `tagline`. Add a field to the frame only when a pattern needs one, and draw it in `components/IdeaDemo.tsx`.

## Still to write (suggested animation)

Arrays & Hashing (a map filling as the scan goes), Trees (the order a DFS and a BFS visit), Heap / Priority Queue (sift up and
down), Backtracking (the decision tree growing and pruning), Tries (a word walking down), Graphs (BFS and DFS frontiers), Advanced Graphs
(Dijkstra settling nodes), 1-D and 2-D Dynamic Programming (the table filling, each cell lit with the cells it reads), Greedy (the choice made at
each step), Intervals (a sweep over the line), Math & Geometry, Bit Manipulation (the bits of a number changing).
