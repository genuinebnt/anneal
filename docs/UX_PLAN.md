# UX plan: ideas waiting for the owner's pick

Status: **planning, 2026-10-29. Nothing here is built.** The owner said yes to every item below ("add all", twice) and keeps asking for more ideas (sections 6 and 7). For the two open decisions the plan assumes my recommendation unless the owner says otherwise: **4.1 = options (b) and (c), 4.3 = option (b)**. No build starts before the mockups in step 2 of the order are approved. per CLAUDE.md, new screens and big UI changes get an
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
| 4.1 | **Bottom status bar** | S | **Corrected:** it is a deliberate slow ticker that pauses under the pointer (`StatsBar.tsx`), so text leaving the right edge is by design, not clipping. What is left to decide: it repeats the streak the header already shows, and it covers 30px of every page. Options: (a) leave it, (b) drop the repeated streak item, (c) a small hide/show choice remembered in the browser. **Owner to choose.** |
| 4.2 | Smaller page headlines | S | Plan, Progress, Patterns, Rust and the DSA home spend 150-250px on a headline before any tool. A compact variant (or one-line title) on the working pages (Plan, Calendar, Review, Progress) and keep the big one on the two home pages. |
| 4.3 | Two different streaks | M | **Owner to choose; read this first.** The header and bottom bar show a *solve streak* (`activity::streak`: consecutive days with at least one solve, so a planned rest day breaks it). The calendar's History shows a *plan streak* (`calendar.rs`: days since tracking began until a planned solve day was missed, rest and practice days do not break it and are counted). They are different measures with the same name. Options: (a) keep both and label them ("solve streak", "plan streak"); (b) make the header plan-aware: count consecutive *planned solve days* that were kept, skipping rest days without counting them; (c) leave as is. Recommended: (b), because a streak that breaks on a day the plan says to rest punishes the plan. It changes a number on the live site, so it needs the owner's word. |
| 4.4 | Progress heatmap month labels collide ("OctNov" at the left) | S | Skip the label when the gap is under two columns. |
| 4.5 | Calendar targets: native `dd/mm/yyyy` date boxes and names cut to "Arrays &...", "Two Poi..." | S | Wider name column; a compact date picker in the app's own style. |
| 4.6 | Command palette actions (`Cmd K`): "log two sum on my own", "open review", "start timer" | M | The palette already searches tracks, stages and problems; add verbs. |

## 5. New features

| # | Item | Effort | Note |
|---|---|---|---|
| 5.1 | **Export my data** (one JSON file: attempts, reviews, solutions, settings) | S | Cheap insurance; the deploy already refuses to strand progress, but nothing gives the owner a copy. |
| 5.2 | Weak spots: a review mode over problems with two or more lapses or the lowest recall | M | `lapses` and recall are already stored per review. |
| 5.3 | The designed but unbuilt **Today** screen (see HANDOFF.md, `anneal-screens.html`): reviews due, the next problem, the timer, the week | L | Mockup exists; the home's Next up card is most of it. |

## 6. More ideas (second round, 2026-10-29)

| # | Item | Effort | Note |
|---|---|---|---|
| 6.1 | **Safety net for Reset**: keep what Reset deletes in an archive table for 30 days, with a Restore link in the toast | M | Reset is the one action that destroys logged data. Undo covers a log; nothing covers a reset after the confirm. |
| 6.2 | Import for the data export (5.1), so a backup can be restored | S | Only after 5.1. |
| 6.3 | Shortcut help overlay on `?` listing every key of every page | S | Pairs with 1.4 and 2.2. |
| 6.4 | Focus timer defaults to the problem's time estimate (~25 / 45 / 60 min) in Timer mode | S | The estimate is already on every card; today the timer is 25 min Pomodoro whatever the problem. Still never starts by itself. |
| 6.5 | Daily goal ring in the header: today's solves and reviews done against the plan | S | The plan already knows what today is (solve, practice, rest) and its capacity. |
| 6.6 | Heatmap: click a day to see what was solved or reviewed that day | M | Today only a hover count. |
| 6.7 | Optional "why" chip when logging Attempted or With help: idea, implementation, edge cases, complexity | M | One click, skippable, off by default. Feeds 5.2 (weak spots) and fills the gap PROGRESS.md names (no learning signal). Not a notes field; the owner keeps their own notes. |
| 6.8 | Log from a phone: installable app (PWA) with a one-screen "log a problem" (search, tap the outcome) | M | The owner solves on LeetCode, often away from the laptop; the header already has a mobile dock. Mockup first. |
| 6.9 | Loading skeletons in place of the plain "Loading..." text | S | Visible on the Review, problem and Progress pages. |
| 6.10 | Interview-company mode: pick a company and a date, the plan prefers its recent problems | L | Companies and "recent" flags already exist on every problem; needs the planner to take a weight. Mockup first. |
| 6.11 | Accessibility pass: focus rings, tab order on the new Review queue, contrast of the dim text in light theme | S | Do alongside the Review rebuild. |

## 7. More ideas (third round, 2026-10-29)

Pages looked at so far: DSA home, problem page, Plan, Calendar, Progress, Review, Rust home. **Not yet looked at:** the Rust workspace, course stage and concept pages, Mock interview, Pattern pages. Item 7.1 closes that gap.

| # | Item | Effort | Note |
|---|---|---|---|
| 7.1 | **UX audit of the pages not yet looked at** (workspace, course stage, concept, Mock, Pattern), written up in this file before any fix | S | Same method: 1440x900 screenshots on a throwaway database. |
| 7.2 | Check what a bad pattern code shows | S | `/dsa/patterns/two-pointers` rendered an empty page under the header (the real codes may be different); an unknown code should say "no such pattern" and link back. To verify first; may not be a bug. |
| 7.3 | After a log, the toast offers "Next up: <title> ->" beside Undo | S | Today you log, then go back to the home page to find the next problem. |
| 7.4 | Hints and "With help" connected: the problem page remembers how many LeetCode hints you opened, and With help is preselected once one is open | M | Also records hints used per log; feeds 5.2 and 6.7. The log stays your choice. |
| 7.5 | "My notes" link on a problem: one URL field (Notion, Obsidian, a file) shown as a link on the page | S | The owner keeps their own notes elsewhere; this only points to them, it stores no notes. |
| 7.6 | Weekly summary on the home page on Monday: solved, lapses, weakest topic, reviews kept | M | Built from data already stored; no new logging. |
| 7.7 | Calendar feed (`.ics` URL) of due reviews and solve days, to subscribe to in a phone calendar | M | Reminders without a notification service. Needs the login token in the URL; check how login is set up first. |
| 7.8 | Copy button on every Python solution block, if `PyCode` lacks one | S | The approaches say "pastes into LeetCode". To verify first. |
| 7.9 | Per-pattern completion ring and a "next problem in this pattern" button on the pattern page | S | The problem page has previous and next already. |
| 7.10 | A proper "not found" and error screen across the app (unknown problem, API down), with a way back | S | The problem page has one; confirm the others do. |

## Suggested order

1. Owner settles the two open decisions: 4.1 (status bar) and 4.3 (streak).
2. Mockups: Review page (with 1.6, the session strip) and the DSA home (3.1). Nothing is built before they are approved. In parallel, 7.1 (the audit of the pages not yet looked at) so its findings can join the plan.
3. Small fixes with no design question: 4.4, 4.5, 2.3, 6.9; then 5.1 with 6.2 and 6.1 (data safety together).
4. Review rebuild (section 1, with 6.3 and 6.11), then the problem page (2.1, 2.2, 6.4), then the DSA home (3.1, 3.2).
5. 6.5, 6.6, 5.2 and 6.7 together (what you did today, where you are weak), then 4.6.
6. Then 7.2 to 7.5, 7.8 to 7.10 (small), then 7.6 and 7.7.
7. Largest last: 5.3 (Today screen), 6.8 (phone), 6.10 (company mode).
