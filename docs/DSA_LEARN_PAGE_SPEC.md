# The Learn page and the problem tags: the specification

Status: **decided by the owner, 2026-10-10. Do not deviate.** Every rule below is the owner's own wording turned into a rule; a
change to a rule is the owner's decision, recorded here first. This supersedes the "listed techniques" and "group problems" ideas of
DSA.md decision 32's first build, and the options in DSA_MUST_SOLVE_PLAN.md (that file keeps the scoring and the data).

## 1. What the Learn page is

The Learn page of a pattern shows **all the common techniques of that topic, each with Python code and an explanation, and the
LeetCode problems that practise it**.

| rule | |
|---|---|
| L1 | A technique **that has LeetCode problems** is shown with its **implementation** (a Python template that runs and is tested, with the explanation: when to use it, the traps) **and its problems**. Whether the problems are in a NeetCode list or not makes no difference. |
| L2 | A technique that **has no LeetCode problem** is **not given a card**. It is only named, in a list **at the bottom of the page**. |
| L3 | Layout of a technique card: **left = the implementation** (the current left side: signals, template with its tabs, pitfalls), **right = the problems** (the current right side). |
| L4 | A technique never appears with problems and without an implementation. A technique without an implementation is either written first or has no problems attached. |
| L5 | Sections (groups) are only headings that organise the cards. They carry no problems of their own. |

## 2. The problems on the right of a card

| rule | |
|---|---|
| P1 | Every problem is a **catalog problem**: it opens the site's own **problem page** (statement, hints, the log of attempts) and that page links to LeetCode to solve it, exactly like the NeetCode ones. |
| P2 | A problem that is **not** in a NeetCode list has **no solution attached** (no `pages/<slug>.toml` is required). Solutions may be added to any problem later; the data model must allow it (rule S1). |
| P3 | A problem belongs to one technique (its `technique`) and appears under that technique's pattern. A technique card may also list a problem of another technique as an *example* (`examples` of an `[[extra]]`) when the same idea solves it. |
| P4 | Free problems only; a premium problem is never added (it cannot be solved without a subscription). |
| P5 | Problems that are not in a NeetCode list are **added to the catalog** (as `practice` problems in `content/dsa/practice.json`) so that they are tracked. |

## 3. The tags (every problem row, everywhere)

| tag | values | rule |
|---|---|---|
| **Difficulty** | `EASY`, `MEDIUM`, `HARD` | always |
| **Priority** | `MUST`, `STRONG`, `PRACTICE`, `WARM-UP` | always. In a NeetCode list: `MUST` is the list's must-learn problem, the rest are `PRACTICE`. Outside the lists: computed from company frequency and concept weight (DSA_MUST_SOLVE_PLAN.md section 2): tier A `MUST`, tier B `STRONG`, easy trivia `WARM-UP`, the rest `PRACTICE`. |
| **Recent** | `RECENT` | asked by a company in the last six months (shown beside the priority) |
| **List** | `BLIND 75`, `NEETCODE 150`, `NEETCODE 250`, `NEETCODE ALL`, or **nothing** | the **narrowest** list the problem is in: in Blind 75 and the 150 → `BLIND 75`; in the 150 and the 250 → `NEETCODE 150`. **Empty if it is in no list.** |
| **Solution** | `SOLUTION` | present when the site has a written solution (a page) for it. Absent otherwise. |
| **Companies** | the company names | **mandatory on every problem**, from the same company set the site has today (`company_groups` in `problems.json`). Shown as chips (first three, the rest on hover). |
| **Topics** | LeetCode's own topic tags | **mandatory on every problem**. |

A problem without a company or without a topic tag is an error (rule V1). A premium or SQL/shell/concurrency problem is never added.

## 4. Tracking

| rule | |
|---|---|
| T1 | A problem that is not in a NeetCode list is **tracked**: not solved / partly solved / solved, the log of attempts, activity. The same log and states as any problem. |
| T2 | It is **not part of completion**: it does not count in the NeetCode list totals, a track's readiness, the home counters or the goal. |
| T3 | It is **not part of repetition**: logging it never creates a review. |
| T4 | It is **not part of the calendar and plan**. |
| T5 | In the Practice page's right-hand list, problems of the lists and the others are **visibly different**: the list tag appears only on the list ones; the others show none (rule in section 3). |

This is what `is_practice` already does in the API; the new problems use it unchanged.

## 5. Where the data lives (one place each)

| what | where |
|---|---|
| techniques with implementations | `content/dsa/lessons/<pattern>.toml`: `[[technique]]` (from the lists) and `[[extra]]` (the others), each with `signals`, `template` (+ variants), `pitfalls`, and a `group` |
| the names of techniques that have no problem | `[[listed]]` in the same file, with `name` and `group` only (no `problems`) |
| problems that are not in a NeetCode list | `content/dsa/practice.json` (with `technique` = a technique or extra id, `priority`, `companies`, `tags`, `recent`) |
| priority scores and their inputs | `content/dsa/must_solve.json` (generated by `tools/neetcode/must_solve.py` from the weights file) |
| written solutions | `content/dsa/pages/<slug>.toml` (unchanged); `SOLUTION` is "a page exists" |

## 6. Rules the build enforces (CI fails otherwise)

| id | check |
|---|---|
| V1 | every catalog problem has at least one company (from the site's company set) and one topic tag |
| V2 | every `technique` of a practice problem is a technique of `problems.json` or an `[[extra]]` of its pattern |
| V3 | every technique or extra has signals, a template that **runs** and passes its behaviour test, and pitfalls |
| V4 | every technique that has a problem has a lesson (L4); a `[[listed]]` entry is never shown with problems (its `problems`, if any, are candidates waiting for an implementation) |
| V5 | no premium problem in `practice.json` |
| V6 | a problem's `lists` decide its list tag; nothing else sets it |
| V7 | progress safety: no problem id that has progress is removed or renamed without an entry in `retired.txt` / the renames |

## 8. The key insight of every problem (decided 2026-10-10; built 2026-10-10)

One short sentence that names the move that solves the problem: "Shortest path in an unweighted graph: BFS." "Pair lookup: a hash map of what you have passed." It is not a solution and not a hint; it is what a strong candidate says first.

| rule | |
|---|---|
| K1 | **Every catalog problem has one**: NeetCode-list and practice problems alike (about 1,660). |
| K2 | One sentence, at most 110 characters, no code, no spoilers of the final algorithm's details; it names the pattern or the observation. |
| K3 | Stored in one place: `content/dsa/insights.json`, `{ "<slug>": "<sentence>" }`. Problem pages (`pages/<slug>.toml`) keep their longer Idea text; the insight is separate. |
| K4 | Shown under the tags on the problem page, and as the second line of a problem row on the Learn and Practice pages (replacing nothing: companies and topics stay). |
| K5 | Build check V8: every problem of `problems.json` and `practice.json` has an insight within the length limit; a premium problem has one too (it is about the idea, not the statement). |
| K6 | Written per technique first (the card's signals are the starting point), then per problem; reviewed against the problem's technique so the sentence agrees with the card it sits under. |

Status: all 1,669 problems have one (`content/dsa/insights.json`). The loader (`crates/content/src/dsa.rs`, `load_insights`) reports a problem without one, a sentence over 110 characters or with a backtick or newline, and a key for a problem the site does not have. The check is on whenever the file exists, and a content test pins that the site's own file does. `ProblemRow.insight` carries it to the web; `Insight` (`web/src/components/dsaBits.tsx`) draws the second line of a row, `.pp-ins` the labelled line on the problem page.

**A problem added to the catalog (for example by `tools/neetcode/problem_catalog.py`) needs its insight in the same change; validation fails until it has one.** Write it as the first thing a strong candidate would say: the pattern or the observation, not the steps.

## 9. How the page is navigated (owner-approved mockup, 2026-10-30; built 2026-10-30)

Only the way you move through the page changed; rules L1 to L5, P1 to P5, the tags (section 3) and the key insight (section 8) are untouched.
Mockup: https://claude.ai/artifact/EXg2vDaHxiFMVHEkM2RXgh. The page was 44,947 px tall on Arrays & Hashing (32 cards open at once).

| rule | |
|---|---|
| N1 | A **rail** beside the cards lists every group (with a progress ring) and, for the group being read, its techniques; a search box finds a technique by name; "open all" and "fold all". On a phone it is a "Jump to" sheet. |
| N2 | Each technique is a **folded card**: a dot for how you are doing, the name, its first signal, `solved/in the lists · n problems`. One card is open when the page loads; a link `#t-<slug>` opens and scrolls to its card. Open, it is the card of L3: implementation left, problems right. |
| N3 | The group name sticks under the header while its cards scroll. |
| N4 | The problems box is **as tall as the implementation beside it** and scrolls inside itself (a fade and a "more" mark say so); a short list is not stretched. |
| N5 | A problem row is two quiet lines: difficulty and priority are the only loud tags; list, recent, solution and premium are small coloured words; companies and topics follow in plain text, so every tag of section 3 is on every row. The key idea has its own button on the row (or is shown when the setting says so). In the lists and the others are under labelled dividers (T5). |

## 7. Status

Kept in DSA_LEARN_PAGE_STATUS.md: per topic, how many techniques have a card, how many problems are attached, how many names are at
the bottom, what is still to write. It is regenerated by `tools/neetcode/learn_status.py`.
