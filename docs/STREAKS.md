# Streaks, perfect streaks and freezes

The owner's rules (2026-10-30), as built. Engine and tests: `crates/api/src/streaks.rs`; tables: `crates/api/migrations/0021_streaks.sql`;
screens: `web/src/components/Streaks.tsx` (no new page: the repair dialog, the bell, the Calendar day panel, a Progress box, the header chip).

## The rules

- **Streak** (the one the app always had, now with marks): walking back from today, a planned solve day with a solve adds one; a planned
  solve day with none breaks it (today never does, it is not over); a day with nothing planned, a holiday or a sick day neither adds nor
  breaks; a frozen day adds one. With no marks it is exactly `activity::streak_by_plan` (a unit test pins that).
- **Perfect streak**: a day counts only if that day's planned problems and planned reviews were all done. A practice day (reviews only)
  is perfect when its reviews are done. A freeze never makes a day perfect: there is no way to repair a perfect streak. Holidays and sick
  days are neutral for it. Days from before the plan was saved (see below) can be kept but never perfect.
- **Freezes**: no cap ("a freeze means I am doing my tasks earlier, so it is encouraged"). **+1** for each new problem beyond that day's
  plan, **+0.5** for each review done ahead of its due date, but only **once in a row per problem**: a review ahead of schedule is
  credited unless the problem's previous log was also ahead, so reviewing the same problem again and again earns nothing. A day that is
  marked earns nothing. Practice problems (logged without a review) earn nothing. The balance is never stored: it is earned minus the
  live freeze marks, worked out each time.
- **Spending**: a freeze goes on a **missed planned day**, today excluded, on any later day (that is the repair, weeks later too; it
  joins the days before and after). Not on a rest day, not on a day with a solve.
- **Holiday and sick**: for **today or yesterday only**, only for a day with nothing solved. Free, neutral, no count limit.
- **History starts fresh**: the current streak and the longest are kept as they are. Freezes start at 0.
- No rest-day concept was added (not needed).

## The data (all additive; nothing existing is touched)

| Table | Holds |
|---|---|
| `day_plan` | What was planned for a day (new problems, reviews), saved the first time `/api/dsa` is opened that day. The plan can change later, so a perfect day is judged against what was saved. Reviews already done that day are added so opening the app late does not shrink the plan. |
| `review_events` | One row per scheduled first solve or review, written in `store::log_attempt`: whether it was ahead of its due date and whether it earned credit. An undo voids the day's latest event (`voided_at`). |
| `streak_marks` | A freeze, holiday or sick mark on a day. One live mark per day (unique index). Undoing voids it (the freeze comes back). **Rows are never deleted.** |

## API

- `GET /api/streaks`: `streak`, `perfect`, `longest`, `freezes`, `earned`, `spent`, the last 60 days with their state, and the missed days
  that can still be repaired with what the streak would become (`before + 1 + after`).
- `POST /api/streaks/marks {day, kind}`: validated by `streaks::check_mark` (400 with the reason).
- `DELETE /api/streaks/marks/{id}`: voids the mark.
- `/api/activity` and `/api/progress` use the same engine for `streak` and `longest_streak`, so the header, the bell, DSA home and Progress agree.

## Screens

- **Header** (DSA tab): chip `perfect n · freezes x`, linking to Progress.
- **Bell**: after a miss, "Your n-day streak broke on <day>" opens the repair dialog; the dialog also opens by itself once, the morning
  after (yesterday missed, a streak before it). "Not now" keeps the chance in the bell and the Calendar.
- **Calendar**: a day panel section (use a freeze, holiday, sick, or undo a mark) and FROZEN / HOLIDAY / SICK on the grid cells.
- **Progress**: the Streaks and freezes box (four numbers, freeze bar, last 28 days, the open repair, "How streaks work").

## Tests

Rust unit tests in `streaks.rs` (the rules, earning, marks, repair and undo, equivalence with the old streak); the API test
`freezes_are_earned_by_extra_new_problems_and_spent_on_a_missed_day` (needs the throwaway Postgres); `web/e2e/streaks.spec.ts` (the UI
against a stubbed API).
