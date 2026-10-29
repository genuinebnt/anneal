-- What "Reset progress" removes from a problem is kept here first, so a reset can be taken back. Nothing deletes from this
-- table: a reset is archived forever, and a restore only marks the entry restored.
CREATE TABLE reset_archive (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    problem_id  text        NOT NULL,
    archived_at timestamptz NOT NULL DEFAULT now(),
    attempts    jsonb       NOT NULL,
    review      jsonb,
    restored_at timestamptz
);
CREATE INDEX reset_archive_by_problem ON reset_archive (problem_id, id DESC);
