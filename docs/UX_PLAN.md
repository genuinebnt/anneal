# UX plan: ideas waiting for the owner's pick

Status: **proposed 2026-10-29, nothing here is built.** The owner picks the order; per CLAUDE.md, new screens and big UI changes get an
HTML mockup and approval first. Effort: S under an hour, M a few hours, L a day or more. Each item says what was seen, so a later agent
does not have to rediscover it. Screens were checked at 1440x900 against a throwaway database on 2026-10-29.

Already shipped from the same conversation (do not redo): the whole Next up card opens the problem; the log options On my own, With help,
Attempted, Instant; Reset progress; Undo after a log; the Review timer starts on demand; the Review checklist and sketch box are gone.

## 1. Review page rebuild (mockup: https://claude.ai/artifact/TCsTTBJEngmARxVzLAyoTy, not yet approved)

The owner asked to pick from several reviews and for a better UI. The mockup has a clickable queue (difficulty, how overdue, recall),
sort, "Review ahead", the manual timer and the four grades.

| # | Item | Effort | Note |
|---|---|---|---|
| 1.1 | Queue you can click, titles visible | M | The old page hid every title but the current one. |
| 1.2 | Sort: most forgotten, overdue, pattern; the choice is remembered | S | Remembered in browser prefs (`prefs.ts`). |
| 1.3 | "Review ahead": pull in a review that is not due yet | M | Needs the API to list upcoming reviews; early reviews already schedule from the real elapsed time. |
| 1.4 | Keyboard: `1`-`4` grade (exists), `J`/`K` or arrows move through the queue, `Space` shows the approach | S | Vim-friendly, as the owner uses Vim. |
| 1.5 | Undo on a grade | S | Reuse `POST /api/dsa/problems/{id}/undo`; the page has its own grade mutation, not the shared toast. |
| 1.6 | **Fill the empty space under the cards** (the owner saw it in the mockup) | S | Add a session strip under the stage: done / left, average time, grade split so far, and the keys legend. It is information the owner wants anyway, not filler. Also let the main card grow with the approach and code once shown. |
| 1.7 | Page width follows the app's width toggle (1180 / 1360) | S | The mockup was fixed at 1180; it now fills the screen, the real page stays on the toggle unless the owner says otherwise. |

## 2. Problem page

| # | Item | Effort | Note |
|---|---|---|---|
| 2.1 | One compact top card: Solve on LeetCode and the log tiles together | S | At 1440x900 the log tiles sit half below the fold because the Solve and video card come first. |
| 2.2 | Keys `1`-`4` log, `U` undoes | S | Same keys as Review; ignore while typing in the editor (Vim mode, notes). |
| 2.3 | State icon legend (tooltip or a small key) | S | Five icons (solved, help, attempted, due, not started) are only explained by hover. |
| 2.4 | Offer the focus timer's elapsed time as "time spent" on the log, kept per problem | M | Needs a small table; shows in the problem's history. Optional and off by default. |

## 3. DSA home

| # | Item | Effort | Note |
|---|---|---|---|
| 3.1 | **Next up above the fold** | M | The headline, stat tiles and three goal cards come first; the Next up card starts near y=590 and its Solve button is cut off at 900px. A shorter headline or Next up first fixes it. Mockup first (home layout). |
| 3.2 | One honest count | S | The header says 0/143, the goal card 0/150, the hero 943 problems, and the plan page explains 143 is the free ones. Label each ("143 free", "150 list") or use one number with a note. |
| 3.3 | Row actions | | Decided against a Reset button on dense rows; Undo covers a mis-click. Reset stays on the problem page. |

## 4. Whole app

| # | Item | Effort | Note |
|---|---|---|---|
| 4.1 | **Bottom status bar**: it scrolls text off the right edge ("FINISH AT THIS", "PROBLEMS" clipped), repeats the streak and solved counts the header already shows, and covers 30px of every page | S | Options: drop the duplicates, or let the owner hide it (a setting). Recommended: drop the duplicates and keep what only it shows (reviews due, today, next up, pace). |
| 4.2 | Smaller page headlines | S | Plan, Progress, Patterns, Rust and the DSA home spend 150-250px on a headline before any tool. A compact variant (or one-line title) on the working pages (Plan, Calendar, Review, Progress) and keep the big one on the two home pages. |
| 4.3 | Streaks disagree | M | **Needs the owner's decision.** The header streak (`activity::streak`) counts consecutive days with a solve, so a planned rest day breaks it; the calendar's streak (`calendar.rs`) skips planned rest and practice days. Pick one rule and use it everywhere. Recommended: the calendar's (a planned rest day does not break it). |
| 4.4 | Progress heatmap month labels collide ("OctNov" at the left) | S | Skip the label when the gap is under two columns. |
| 4.5 | Calendar targets: native `dd/mm/yyyy` date boxes and names cut to "Arrays &...", "Two Poi..." | S | Wider name column; a compact date picker in the app's own style. |
| 4.6 | Command palette actions (`Cmd K`): "log two sum on my own", "open review", "start timer" | M | The palette already searches tracks, stages and problems; add verbs. |

## 5. New features

| # | Item | Effort | Note |
|---|---|---|---|
| 5.1 | **Export my data** (one JSON file: attempts, reviews, solutions, settings) | S | Cheap insurance; the deploy already refuses to strand progress, but nothing gives the owner a copy. |
| 5.2 | Weak spots: a review mode over problems with two or more lapses or the lowest recall | M | `lapses` and recall are already stored per review. |
| 5.3 | The designed but unbuilt **Today** screen (see HANDOFF.md, `anneal-screens.html`): reviews due, the next problem, the timer, the week | L | Mockup exists; the home's Next up card is most of it. |

## Suggested order

1. 4.1 and 4.4 (small, visible everywhere), then 5.1.
2. Decide 4.3 (streak rule), then build it.
3. Review rebuild (section 1), after the owner approves the mockup with 1.6 in it.
4. 2.1 and 2.2, then 3.1 and 3.2 (needs a home mockup).
5. The rest by the owner's taste.
