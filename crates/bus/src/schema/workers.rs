//! Migration 18: temporary workers and the queue that holds them.
//!
//! A worker is an ordinary `bot` row with `temporary` set: it exists for one
//! task and is archived once that task closes. Workers are capped separately
//! from permanent bots, so a project already at its bot cap can still fan
//! work out. The `worker` table is the asking daemon's queue: one row per
//! spawn, from the moment it is asked for until its task closes. It names
//! the bot once one is placed — here, or as a stand-in for one created on a
//! linked machine.

pub(super) const MIGRATION_18: &str = r#"
ALTER TABLE bot ADD COLUMN temporary INTEGER NOT NULL DEFAULT 0;

CREATE TABLE worker (
    id             TEXT PRIMARY KEY,
    project_id     TEXT NOT NULL REFERENCES project(id),
    parent_bot_id  TEXT NOT NULL REFERENCES bot(id),
    name           TEXT NOT NULL,
    brief          TEXT NOT NULL,
    description    TEXT NOT NULL DEFAULT '',
    instructions   TEXT NOT NULL DEFAULT '',
    runtime        TEXT,
    machine        TEXT,
    deadline_hours INTEGER NOT NULL,
    state          TEXT NOT NULL
        CHECK(state IN ('queued', 'running', 'done', 'cancelled', 'expired', 'failed')),
    bot_id         TEXT REFERENCES bot(id),
    task_id        TEXT,
    error          TEXT,
    created_at     TEXT NOT NULL,
    started_at     TEXT,
    finished_at    TEXT
);
CREATE INDEX idx_worker_project ON worker(project_id, state, created_at);
CREATE INDEX idx_worker_parent ON worker(parent_bot_id, state);
"#;
