-- Streaks, perfect streaks and freezes (docs/STREAKS.md). Everything here is new and additive: nothing existing is touched or deleted.
--
-- What was planned for a day, saved the first time the app is opened that day, because the plan can change later and a perfect day
-- is "that day's plan, met". Days before this table existed simply have no row (they count as kept or missed, never perfect).
CREATE TABLE day_plan (
    day             date PRIMARY KEY,
    planned_new     int         NOT NULL CHECK (planned_new >= 0),
    planned_reviews int         NOT NULL CHECK (planned_reviews >= 0),
    saved_at        timestamptz NOT NULL DEFAULT now()
);

-- One row per logged review or first solve, written at the moment it is logged, because only then is it known whether a review was
-- done ahead of its due date. `credited` is set for a review ahead of schedule that earns half a freeze: at most one in a row per
-- problem (an on-time review has to come between two), so reviewing the same problem over and over earns nothing.
-- An undo voids the row; rows are never deleted.
CREATE TABLE review_events (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    problem_id text        NOT NULL,
    day        date        NOT NULL,
    at         timestamptz NOT NULL DEFAULT now(),
    is_new     boolean     NOT NULL,
    solved     boolean     NOT NULL,
    ahead      boolean     NOT NULL,
    credited   boolean     NOT NULL,
    voided_at  timestamptz
);
CREATE INDEX review_events_by_day ON review_events (day) WHERE voided_at IS NULL;
CREATE INDEX review_events_by_problem ON review_events (problem_id, id DESC);

-- A freeze spent on a missed day, or a day marked as a holiday or as sick. Undoing a mark voids it (the freeze comes back);
-- rows are never deleted. One live mark per day.
CREATE TABLE streak_marks (
    id        bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    day       date        NOT NULL,
    kind      text        NOT NULL CHECK (kind IN ('freeze', 'holiday', 'sick')),
    made_at   timestamptz NOT NULL DEFAULT now(),
    voided_at timestamptz
);
CREATE UNIQUE INDEX streak_marks_one_live ON streak_marks (day) WHERE voided_at IS NULL;
