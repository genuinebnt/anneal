-- System design prep (docs/SYSTEM_DESIGN.md). New tables only: nothing existing is touched, and nothing here is ever deleted.
--
-- One finished drill: each step's answer and time, the diagram, the hints opened, and the marks given against the rubric
-- (`{"<point id>": "got" | "part" | "miss"}`, empty until marked).
CREATE TABLE sd_attempts (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    problem_id  text        NOT NULL,
    started_at  timestamptz NOT NULL,
    finished_at timestamptz NOT NULL DEFAULT now(),
    seconds     int         NOT NULL CHECK (seconds >= 0),
    steps       jsonb       NOT NULL DEFAULT '{}'::jsonb,
    diagram     jsonb       NOT NULL DEFAULT '{}'::jsonb,
    hints       int         NOT NULL DEFAULT 0 CHECK (hints >= 0),
    marks       jsonb       NOT NULL DEFAULT '{}'::jsonb,
    marked_at   timestamptz
);
CREATE INDEX sd_attempts_by_problem ON sd_attempts (problem_id, id DESC);

-- A review card: a rubric point missed or only partly made (`point:<problem>:<point>`) or a concept check missed
-- (`check:<concept>:<n>`), with its FSRS memory. A card is made once and then only updated; `history` keeps every grade.
CREATE TABLE sd_cards (
    card_id     text PRIMARY KEY,
    kind        text        NOT NULL CHECK (kind IN ('point', 'check')),
    created_at  timestamptz NOT NULL DEFAULT now(),
    due         date        NOT NULL,
    stability   real,
    difficulty  real,
    last_review date,
    last_grade  text CHECK (last_grade IN ('again', 'hard', 'good', 'easy')),
    reps        int         NOT NULL DEFAULT 0,
    lapses      int         NOT NULL DEFAULT 0,
    history     jsonb       NOT NULL DEFAULT '[]'::jsonb
);
CREATE INDEX sd_cards_due ON sd_cards (due);
