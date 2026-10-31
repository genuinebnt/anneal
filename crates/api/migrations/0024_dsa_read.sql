-- Reading to remember (DSA): an idea-memory schedule per problem, apart from the coding reviews in `reviews`. Reading a solution
-- never moves a coding review. A new table only; rows are made and updated, never deleted.
CREATE TABLE dsa_read (
    problem_id  text PRIMARY KEY,
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
CREATE INDEX dsa_read_due ON dsa_read (due);
