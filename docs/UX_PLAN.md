# UX plan: ideas waiting for the owner's pick

Status: **planning, 2026-10-29. Nothing here is built.** The owner said yes to every idea in every section, including the ones first parked, and delegated the two open decisions (4.1, 4.3); the premium answers are in section 9. **Every item is in scope.** No build starts before the owner says go; mockups come first for screens (see "Suggested order").
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
| 4.1 | **Bottom status bar** | S | **Decided (owner delegated): (b) + (c).** It is a deliberate slow ticker that pauses under the pointer (`StatsBar.tsx`), so text leaving the edge is by design. Drop the streak item (the header already shows it) and add a hide/show choice, which lives on the Settings page (9.7) and is remembered (9.8). |
| 4.2 | Smaller page headlines | S | Plan, Progress, Patterns, Rust and the DSA home spend 150-250px on a headline before any tool. A compact variant (or one-line title) on the working pages (Plan, Calendar, Review, Progress) and keep the big one on the two home pages. |
| 4.3 | Two different streaks | M | **Decided (owner delegated): the header streak becomes plan-aware.** Today the header and bottom bar show a *solve streak* (`activity::streak`: consecutive days with a solve, so a planned rest day breaks it) while the calendar's History shows a *plan streak* (`calendar.rs`: rest and practice days do not break it and are counted). New rule everywhere, with one function: count consecutive *planned solve days* that had a solve; rest and practice days neither break the streak nor add to it; today never breaks it before the day is over. A completed premium problem always counts (section 9). Label it "streak" once, in one place, and keep the calendar's History line as "days kept". It changes a number on the live site, so the change note says old and new values side by side. |
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

**Owner's picks: all of this round, 7.1 to 7.10** (7.3, 7.4, 7.6, 7.8, 7.9, 7.10 first, then 7.1, 7.2, 7.5, 7.7 added later the same day). Pages looked at so far: DSA home, problem page, Plan, Calendar, Progress, Review, Rust home, Mock interview (setup screen), Courses home. Not looked at: the Rust workspace, course stage and concept pages, Pattern pages.

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

## 8. More ideas (fourth round, 2026-10-29)

**Owner's picks: all of this round, 8.1 to 8.8.** (8.3's DSA part is largely solved by section 9: with premium not counted, 143, 150 and 943 stop disagreeing; its Rust and course labels still need doing.) New findings from the Mock interview and Courses screens, and from how the owner works (solves on LeetCode in another tab, with anneal open beside it).

| # | Item | Effort | Note |
|---|---|---|---|
| 8.1 | **Timer visible from the LeetCode tab**: the page title shows the countdown (`24:10 · Two Sum`) while the focus timer runs, and an optional chime when a phase ends | S | The owner solves in another tab; today the timer ends silently where nobody is looking. The timer still never starts by itself. |
| 8.2 | **Welcome back from LeetCode**: after "Solve on LeetCode" is clicked and the anneal tab regains focus, a slim bar asks "Done with <problem>? On my own / With help / Attempted / Instant" and can be dismissed | M | Uses the tab's visibility change and one remembered problem id in browser storage. It never logs by itself and never starts the timer. Same log buttons and Undo as everywhere. Mockup first. |
| 8.3 | **One honest count, every area**: DSA says 143, 150 and 943; Rust says 197 (header chip) and 596 (hero); the course says 169 (header chip) and 317 (hero) | S | Extends 3.2. Say what each number counts ("169 core stages, 148 optional challenges"). |
| 8.4 | Mock interview: **Start round visible without scrolling** (the summary card with the Start button runs below the fold at 1440x900) | S | Sticky summary card, or move Start up. |
| 8.5 | Mock interview: a round history under the setup (date, shape, solved, time used against allowed) with a simple trend | M | The page says "0 rounds taken" but shows nothing of earlier rounds; verify what appears after one. |
| 8.6 | Phone-width pass (390px) of Review, problem page, Home, Mock | S | Not looked at yet; relevant because of 6.8. |
| 8.7 | Bulk "mark as already solved" on the Problems page (tick several, one confirm) | M | For problems the owner solved before anneal. **Decided (my call, owner can veto):** they are marked solved on a past date the owner picks (default: "before anneal", not today), so they do not add to today's activity or the streak, and they start no reviews; any one can be logged normally later to start its reviews. Undo for the whole batch. |
| 8.8 | Keep the timer manual everywhere: opening a problem, clicking Solve on LeetCode or switching modes must not start it | S | A rule to keep, with an e2e test, since the owner has asked twice that nothing starts by itself. |

## 9. Count premium problems or not (owner asked 2026-10-29)

**Ask:** a setting, **default off (premium not counted)**. Progress bars and every number are affected.

**What exists today.** One stored flag, `goal.free_only` (default `true`, so premium is already left out of the *goal*). The server uses it only for the goal list, the next-up queue and the plan (`dsa.rs`, `planner.rs`, `calendar.rs`); the plan page and the calendar's Scope box each have a "Premium problems: Left out / Included" switch bound to it. The DSA home has a separate view filter, "Hide LeetCode Premium". Everything else counts premium problems: track summaries and readiness, the Progress page, the goal cards on the home (150, 31 and 943), the hero counts, the pattern pages, the header chip (143 because it follows the goal) and the bottom bar. That is why the numbers disagree today.

**Plan.**

| # | Item | Effort | Note |
|---|---|---|---|
| 9.1 | One setting "Count premium problems", stored in the existing `goal.free_only` (inverted, so default off needs **no migration and no change to any stored data**) | M | The plan page and calendar switches stay and edit the same value; none is removed. |
| 9.2 | Every total, solved count, percentage and bar respects it: goal cards, hero stats, header chip, bottom bar, track summaries and readiness, Progress page, pattern pages, activity week, Next up, plan and calendar (already) | L | Server side first (one helper that says whether a problem counts, used by `views`, `progress`, `activity`, `dsa`), then the web's own totals (`dsa.ts`, `DsaPage`, `PatternPage`, `PatternProblems`). Mock interview already never draws premium. |
| 9.3 | Premium problems still appear in lists with the PREM badge and keep any progress already logged; they just are not in the completion totals. The home's "Hide LeetCode Premium" filter stays a view filter, off by default | S | Confirmed by the owner. |
| 9.4 | Switching shows what changes before it applies ("goal 150 -> 161, 4 premium solved would count") and never touches reviews, attempts or the plan's saved days | S | Reviews are unaffected, as decided. |
| 9.5 | Free substitute: on a premium problem's page and card, link the closest free problem of the same technique | S | Techniques and problem lists already exist; no new data. |
| 9.6 | A test pinning it: with the setting off, no premium problem is in any *completion* total on any endpoint while a completed premium problem still counts toward the streak, heatmap and reviews; with it on, all are in the totals | M | Throwaway database, as always. |

**Decided by the owner (2026-10-29).**
1. **Listed, not hidden.** Premium problems stay in every list with the PREM badge and can be logged.
2. **What you complete always counts as activity.** A premium problem that is completed counts toward the streak, the heatmap, "solved this year" and the week, whatever the setting says. The setting only decides whether premium problems are in the *totals and bars* (goal, readiness, track and pattern percentages, header chip). Progress logged on premium problems is kept either way, so switching the setting on later makes it count in the totals too.
3. **Reviews follow what you did.** A premium problem you completed keeps coming back for review; one you never did has none. Nothing about reviews depends on the setting.
4. **The switch lives on the new Settings page (9.7)**, and the plan page and calendar switches stay and edit the same value (nothing is removed).

This splits every number into two kinds, which the build must keep apart: *activity* (what you did: streak, heatmap, week, year, focus, reviews; premium always included) and *completion* (progress against a list or goal: totals, bars, readiness; premium only when the setting is on). 9.2's test pins both.

| # | More ideas (fifth round) | Effort | Note |
|---|---|---|---|
| 9.7 | **One Settings page**: count premium (9.1), the status-bar choice (4.1), timer defaults (6.4), export and import (5.1, 6.2), theme, editor and Vim, retention and capacity (today in the plan page) | M | Settings are scattered: the plan page, the editor, the calendar. A single page is where the new switches belong. Mockup first. |
| 9.8 | **Preferences that follow the owner across laptops**: timer lengths, sort choices, filters, the status-bar choice, stored on the server instead of in the browser | M | `prefs.ts` is browser-only, and the owner continues work on other laptops. |
| 9.9 | Theme choice with a third option, "System" | S | The header toggle only flips dark and light. |
| 9.10 | A "what changed" line after a deploy: the app shows once "New since you last visited: ..." from the commit titles | S | The owner ships after every batch and cannot tell what is live. |

## Suggested order (everything is in scope; this is the sequence)

0. Nothing is built until the owner says go. Mockups below are made first and approved.
1. **Mockups, one message:** Review page (with the session strip 1.6), DSA home (3.1), Settings page (9.7), welcome-back bar (8.2), phone quick log (6.8), Mock round history (8.5). Plus the page audit (7.1) written into this file.
2. **Batch A, safe fixes (no design question):** 4.4 heatmap labels, 4.5 calendar targets, 2.3 icon legend, 6.9 skeletons, 7.8 copy button, 7.10 error screens, 7.2 bad pattern code, 8.4 Start visible in Mock, 8.8 plus 8.1 timer rule and tab-title countdown, 9.9 System theme.
3. **Batch B, data safety:** 5.1 export, 6.2 import, 6.1 reset archive.
4. **Batch C, counting:** 9.1 to 9.6 (premium), then 4.3 (streak rule) in the same pass because both decide what a number means; 4.1 status bar; 9.7 Settings page; 9.8 prefs on the server.
5. **Batch D, Review and problem page:** section 1 (with 6.3 shortcut help, 6.11 accessibility), 2.1, 2.2, 6.4, 7.3 next-up in the toast, 7.4 hints and With help, 7.9 pattern ring.
6. **Batch E, home and across:** 3.1, 3.2 and 8.3 counts, 6.5 daily ring, 6.6 heatmap day, 5.2 weak spots with 6.7 why chip, 4.6 palette actions, 7.5 notes link, 8.6 phone fixes.
7. **Batch F, larger:** 7.6 weekly summary, 7.7 calendar feed, 8.2 welcome-back bar, 8.5 Mock history, 8.7 bulk mark solved, 6.8 phone quick log, 6.10 company mode, 9.5 free substitutes, 9.10 deploy note, 5.3 Today screen.

Each batch ships on its own (commit, push, CI deploys) with the progress file updated, as before.

## Status after the build run (2026-10-29)

The owner asked for everything to be built while away, without review, deciding minor things and noting open ones. Shipped in batches A to F
(see PROGRESS.md). The mockup-first rule was waived by the owner for this run. Everything below that is not listed as open is built and has a test.

**Open or only partly done, for the owner:**

| Item | State | Why / what is needed |
|---|---|---|
| 6.10 Company mode (plan prefers a company's recent problems) | not built | A planner change with a design question (how strongly to weight, what happens to the curriculum order). Needs the owner's wish and a mockup. |
| 5.3 Today screen | not built | The DSA home now opens on Next up with the day's chip, due reviews and the Monday summary, which is most of it. Say if a separate screen is still wanted. |
| 8.3 Rust and Courses counts | not done | The Courses hero says 317 stages while the header chip says 169 (planned modules counted in one place, see `withoutPlanned` in CoursePage); the Rust hero says 596 problems against 197 written. DSA is settled by the premium setting. Needs a decision on which number each place should show. |
| 2.4 Time spent on a log | not built | Optional and needs a table (additive). The timer's elapsed time is not yet offered when logging. |
| 4.5 Calendar target dates | partly | Names no longer cut off; the native date boxes are unchanged. |
| 7.1 Page audit | partly | Looked at: DSA home, problem, Plan, Calendar, Progress, Review, Mock setup, Courses home, Rust home. Not looked at: the Rust workspace, course stage and concept pages. |
| 6.8 Phone app | partly | `/log` and a web manifest (add to the home screen from the browser menu). No service worker, so no offline use. |
| 4.3 Streak | built, check the number | The header streak is now plan-aware, so it can differ from before. The calendar's own number is called "days kept in a row". |

**Decisions I took (the owner can veto any):** the numbers bar keeps everything but the streak and can be hidden; bulk "already solved" records
problems on 1 Jan 2020 by default and starts no reviews; a premium problem you complete always counts toward activity, the setting only changes
totals and bars; server-synced preferences are skipped in automated browsers so tests stay isolated; every delete archives to `data_archive` and
is never purged.
