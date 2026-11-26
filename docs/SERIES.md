# Series

A series is an order over existing problems that ends in a capstone. It copies and moves nothing and stores nothing:
progress is worked out from the attempts and review dates the app already keeps. Mockup: `docs/mockups/series.html`.

| Where | What |
|---|---|
| `/rust` | one band per series: progress, what is next, a link (no header tab) |
| `/rust/series/<id>` | the path by stage, planned repeats, the capstone, readiness, idea threads, side paths |
| `/p/<problem>` of a series | the pane layout of the mockup (`.sx-ws` in `series.css`; every workspace control stays), breadcrumb "Part n of m", the practice clock set to the part's minutes, a Connections block in the problem pane, and after a solve a "Solved" tab beside the code (it replaces the celebration popup for these problems). Until it is solved, "What this teaches" and the follow-up questions are not shown, and the tags wait for "Show tags" (or the "Show the key idea" setting) |

Code: `crates/content/src/series.rs` (loader and checks), `crates/api/src/series.rs` (`GET /api/series`,
`/api/series/{id}`, `/api/problems/{id}/series?series=`), `web/src/pages/SeriesPage.tsx`, `web/src/components/seriesBits.tsx`.
Tests: `crates/content/tests/series.rs`, `crates/api/tests/series.rs`, `web/e2e/series.spec.ts`.

## A problem in more than one series

The series being read is in the address: `/p/<problem>?series=<id>`. Every link into a problem from a series sets it
(the series page, Connections, the completion view and its next part, the workspace's own next links and pager), so it
survives a reload, Back and Forward. Nothing is stored. A link to a problem outside the series ("Another problem",
optional side paths) sets none. The series list and the Rust home lead to series pages, not straight to problems.

`GET /api/problems/{id}/series?series=<id>` picks, in this order:

1. the series asked for, if it has the problem as a written part or step (otherwise it is passed over);
2. the first series, in the stable order, where the reader has solved something that the problem's other series don't
   have. What they all share never counts, so a Submit on a shared problem can't change the answer;
3. the first in the stable order: `rank` (lowest first; unranked last), then id. `web-server` has `rank = 1`.

The answer lists the problem's `others`, and the problem pane offers "Switch to ..." for each under "Part n of m". It
changes `?series=` in place (no new history entry); the code, clock and tabs stay.

## The file: `content/series/<id>.toml`

The file name is the address (a-z, 0-9, `-`). Unknown fields are errors. A file with any mistake is left out, and
`anneal validate` lists the mistakes (the API refuses to start with content issues, as for all content).

`[series]`: `title`; optional `rank` (see above), `summary`, `note` (foot of the path), `id` (not used), `capstone_title`,
`capstone_summary`, `capstone_note`, `capstone_cookbook` (the page behind "Show the Cookbook solution"; without it,
the first page a capstone step refers to).

`[[stage]]`: `id`, `name`. A path heading.

`[[part]]`, in order:

| Field | Notes |
|---|---|
| `id` | a problem id. DSA (LeetCode) problems can't be in a series |
| `stage` | a stage id; required; a stage's parts stay together |
| `status` | `existing`, `new` or `to-write`. Needed only as `to-write`: a part whose problem doesn't exist must say so, and then needs a `title` (the `id` is optional). Once a ready problem has that id, the part is written, whatever the status says. A draft problem counts as not written |
| `title` | default: the problem's |
| `number` | optional; checked against the position |
| `minutes` | shown on the path; sets the practice clock |
| `new_idea` | the path's line, the Next band, and "What you practised" |
| `blurb` | optional: used on the path and Next band instead of `new_idea` when that gives the method away |
| `reuses` | "Again" under "What you practised" (left out if it starts with "nothing") |
| `required` | ids of **earlier** items; shown openly under "Know first"; no cycles |
| `know_first` | optional plain text under "Know first" |
| `helpful` | `{ id, why, reveals_method, hidden }`; an item of the series or any problem; always closed until asked (open once solved) |
| `cookbook` | `{ level = "exact" \| "similar" \| "related", page, note, source, hidden }`; the page must exist; always closed; `note` and `source` are not shown |
| `used_again`, `real_uses` | "Where it returns" |
| `solved_title`, `practised = [{ text, from }]`, `returns = [{ where, text }]` | optional hand-written completion view, instead of the lines made from the fields above |
| `adapted_from` | accepted, not shown |

`[[capstone]]`: the same fields with `step` instead of `number`, no `stage`, plus `prepared_by` (part ids).

`[[revisit]]`: `part`, exactly one of `after` / `before`, optional `how` and `minutes`. The repeat must come after a
later item than the part itself.

`[[readiness]]`: `group`, `concept`, optional `parts` (part ids), `steps` (capstone step numbers), `note`. The page
shows one closed line per group, with all its part numbers.

`[[connection]]`: `idea`, `learn`, optional `again`, `apply` (ids).

`[[side_path]]`: `id` (any problem), `why`, optional `before` / `after`.

## Progress and repeats

- A part is solved when any attempt at it is solved; a re-solve in progress keeps the earlier credit (as elsewhere).
- "Next" is the first written part, then capstone step, not yet solved. Nothing is locked.
- A repeat is **not** scheduled "after part n": the app's re-solve schedule (FSRS, docs/SPACED_REPETITION.md) already
  gives every solved problem its own date. A repeat row shows: waiting (part not solved), scheduled with that date,
  due (links to Re-solves), or done (a solved `resolve` attempt later than the first solves of the part and of the item
  it follows). An early re-solve of your own is not offered, because the app has no such control.
- A capstone step's starter is the problem's own; carrying the reader's code between steps does not exist.
