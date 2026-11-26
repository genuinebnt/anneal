-- Schema v1. Timestamps are unix milliseconds (INTEGER) written by the injected Clock,
-- never by SQL `now()`, so tests with a virtual clock stay deterministic.
-- Every tenant-owned table carries org_id: repositories ALWAYS filter on it (tenant scoping).

CREATE TABLE orgs (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE users (
    id            TEXT PRIMARY KEY,
    org_id        TEXT NOT NULL REFERENCES orgs(id),
    email         TEXT NOT NULL UNIQUE,          -- stored lower-cased
    password_hash TEXT NOT NULL,                 -- Argon2id PHC string
    role          TEXT NOT NULL CHECK (role IN ('owner','admin','member','viewer')),
    verified_at   INTEGER,
    created_at    INTEGER NOT NULL
);

-- One-time tokens (email verification). Only the hash is stored: a DB leak must not leak live tokens.
CREATE TABLE email_tokens (
    token_hash TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id),
    purpose    TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    used_at    INTEGER
);

-- Refresh tokens form a "family" per login. Rotating inside a family lets us detect reuse.
CREATE TABLE refresh_tokens (
    id          TEXT PRIMARY KEY,
    family_id   TEXT NOT NULL,
    user_id     TEXT NOT NULL REFERENCES users(id),
    token_hash  TEXT NOT NULL UNIQUE,
    expires_at  INTEGER NOT NULL,
    rotated_at  INTEGER,                         -- set when exchanged for a successor
    revoked_at  INTEGER
);
CREATE INDEX refresh_family ON refresh_tokens(family_id);

CREATE TABLE api_keys (
    id         TEXT PRIMARY KEY,
    org_id     TEXT NOT NULL REFERENCES orgs(id),
    user_id    TEXT NOT NULL REFERENCES users(id),
    name       TEXT NOT NULL,
    key_hash   TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,
    revoked_at INTEGER
);

CREATE TABLE projects (
    id          TEXT PRIMARY KEY,
    org_id      TEXT NOT NULL REFERENCES orgs(id),
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_by  TEXT NOT NULL,
    version     INTEGER NOT NULL DEFAULT 1,      -- optimistic lock, surfaced as ETag
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    deleted_at  INTEGER                          -- soft delete
);
CREATE INDEX projects_org ON projects(org_id, created_at, id);

CREATE TABLE tasks (
    id          TEXT PRIMARY KEY,
    org_id      TEXT NOT NULL REFERENCES orgs(id),
    project_id  TEXT NOT NULL REFERENCES projects(id),
    title       TEXT NOT NULL,
    body        TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open','in_progress','done')),
    priority    INTEGER NOT NULL DEFAULT 0,
    assignee_id TEXT,
    created_by  TEXT NOT NULL,
    version     INTEGER NOT NULL DEFAULT 1,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    deleted_at  INTEGER
);
CREATE INDEX tasks_org_created ON tasks(org_id, created_at, id);
CREATE INDEX tasks_project ON tasks(project_id, created_at, id);

-- Standalone FTS5 table kept in sync by the repository INSIDE the same transaction as the write.
CREATE VIRTUAL TABLE tasks_fts USING fts5(task_id UNINDEXED, title, body);

CREATE TABLE comments (
    id         TEXT PRIMARY KEY,
    org_id     TEXT NOT NULL,
    task_id    TEXT NOT NULL REFERENCES tasks(id),
    author_id  TEXT NOT NULL,
    body       TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX comments_task ON comments(task_id, created_at, id);

CREATE TABLE attachments (
    id           TEXT PRIMARY KEY,
    org_id       TEXT NOT NULL,
    task_id      TEXT NOT NULL REFERENCES tasks(id),
    object_key   TEXT NOT NULL,
    filename     TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size         INTEGER NOT NULL,
    sha256       TEXT NOT NULL,
    created_at   INTEGER NOT NULL
);
CREATE INDEX attachments_task ON attachments(task_id);

CREATE TABLE audit_log (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    org_id    TEXT NOT NULL,
    actor     TEXT NOT NULL,
    action    TEXT NOT NULL,
    entity    TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    at        INTEGER NOT NULL,
    detail    TEXT NOT NULL DEFAULT '{}'
);

-- Idempotency: (scope, key) is claimed with INSERT .. ON CONFLICT DO NOTHING, which is atomic.
CREATE TABLE idempotency (
    scope        TEXT NOT NULL,
    key          TEXT NOT NULL,
    fingerprint  TEXT NOT NULL,
    state        TEXT NOT NULL CHECK (state IN ('in_progress','done')),
    status       INTEGER,
    content_type TEXT,
    etag         TEXT,                           -- replayed, so the client can If-Match what it created
    response     BLOB,
    created_at   INTEGER NOT NULL,
    PRIMARY KEY (scope, key)
);

-- Durable job queue. `dedupe_key` makes scheduled enqueues idempotent across instances.
CREATE TABLE jobs (
    id           TEXT PRIMARY KEY,
    kind         TEXT NOT NULL,
    payload      TEXT NOT NULL,
    state        TEXT NOT NULL CHECK (state IN ('queued','running','done','dead')),
    run_at       INTEGER NOT NULL,
    attempts     INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL,
    lease_until  INTEGER,
    last_error   TEXT,
    dedupe_key   TEXT UNIQUE,
    created_at   INTEGER NOT NULL
);
CREATE INDEX jobs_ready ON jobs(state, run_at);

-- Transactional outbox: written in the SAME tx as the domain change, relayed later.
CREATE TABLE outbox (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    org_id       TEXT NOT NULL,
    topic        TEXT NOT NULL,
    payload      TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    published_at INTEGER
);

-- Published domain events: the SSE stream replays from here; `seq` is the Last-Event-ID.
CREATE TABLE events (
    seq     INTEGER PRIMARY KEY AUTOINCREMENT,
    org_id  TEXT NOT NULL,
    topic   TEXT NOT NULL,
    payload TEXT NOT NULL,
    at      INTEGER NOT NULL
);
CREATE INDEX events_org ON events(org_id, seq);

CREATE TABLE webhooks (
    id         TEXT PRIMARY KEY,
    org_id     TEXT NOT NULL,
    url        TEXT NOT NULL,
    secret     TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE schedule_state (
    name     TEXT PRIMARY KEY,
    last_run INTEGER NOT NULL
);
