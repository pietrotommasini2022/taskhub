/// SQLite DDL. Applied once on `taskhub init` via `PRAGMA user_version`.
pub const SCHEMA_VERSION: u32 = 1;

pub const SCHEMA_SQL: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS runs (
    id           TEXT PRIMARY KEY,
    workflow_id  TEXT NOT NULL,
    workflow_name TEXT NOT NULL,
    status       TEXT NOT NULL CHECK (status IN ('pending','running','success','failed','cancelled')),
    trigger_kind TEXT NOT NULL CHECK (trigger_kind IN ('schedule','webhook','filesystem','manual')),
    trigger_payload TEXT,
    started_at   INTEGER NOT NULL,
    finished_at  INTEGER,
    error        TEXT
);

CREATE TABLE IF NOT EXISTS step_runs (
    id          TEXT PRIMARY KEY,
    run_id      TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    step_id     TEXT NOT NULL,
    status      TEXT NOT NULL CHECK (status IN ('pending','running','success','failed','skipped')),
    output      TEXT,
    error       TEXT,
    attempt     INTEGER NOT NULL DEFAULT 1,
    started_at  INTEGER NOT NULL,
    finished_at INTEGER
);

CREATE TABLE IF NOT EXISTS events (
    id          TEXT PRIMARY KEY,
    kind        TEXT NOT NULL,
    source      TEXT NOT NULL,
    payload     TEXT NOT NULL,
    processed   INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS plugin_state (
    plugin_id   TEXT NOT NULL,
    key         TEXT NOT NULL,
    value       TEXT NOT NULL,
    updated_at  INTEGER NOT NULL,
    PRIMARY KEY (plugin_id, key)
);

CREATE TABLE IF NOT EXISTS credentials (
    key         TEXT PRIMARY KEY,
    ciphertext  BLOB NOT NULL,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_runs_workflow_id  ON runs(workflow_id);
CREATE INDEX IF NOT EXISTS idx_runs_status       ON runs(status);
CREATE INDEX IF NOT EXISTS idx_step_runs_run_id  ON step_runs(run_id);
CREATE INDEX IF NOT EXISTS idx_events_processed  ON events(processed, created_at);
"#;
