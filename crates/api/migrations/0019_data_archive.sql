-- A copy of any rows the app is about to delete (a track or course reset, a deleted saved solution or plan, a discarded
-- draft, a retired problem), taken in the same transaction as the delete. Nothing deletes from this table: whatever
-- the owner made can be recovered from it with SQL.
CREATE TABLE data_archive (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    source      text        NOT NULL,
    table_name  text        NOT NULL,
    archived_at timestamptz NOT NULL DEFAULT now(),
    rows        jsonb       NOT NULL
);
CREATE INDEX data_archive_by_source ON data_archive (source, id DESC);
