//! Migration 1: the baseline schema every later migration builds on.

pub(super) const MIGRATION_1: &str = r#"
CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE project (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
);

CREATE TABLE bot (
    id             TEXT PRIMARY KEY,
    project_id     TEXT NOT NULL REFERENCES project(id),
    name           TEXT NOT NULL,
    description    TEXT NOT NULL DEFAULT '',
    avatar         TEXT NOT NULL DEFAULT '',
    instructions   TEXT NOT NULL DEFAULT '',
    workspace_path TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    UNIQUE (project_id, name)
);

CREATE TABLE conversation (
    id         TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES project(id),
    kind       TEXT NOT NULL CHECK (kind IN ('dm', 'group')),
    bot_id     TEXT REFERENCES bot(id),
    group_id   TEXT,
    title      TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE message (
    id              TEXT PRIMARY KEY,
    num             INTEGER NOT NULL UNIQUE,
    conversation_id TEXT NOT NULL REFERENCES conversation(id),
    sender_kind     TEXT NOT NULL CHECK (sender_kind IN ('user', 'bot', 'routine')),
    sender_bot_id   TEXT REFERENCES bot(id),
    sender_name     TEXT NOT NULL,
    kind            TEXT NOT NULL CHECK (kind IN ('task', 'reply', 'done', 'note', 'chat')),
    body            TEXT NOT NULL,
    ref_message_id  TEXT REFERENCES message(id),
    created_at      TEXT NOT NULL
);
CREATE INDEX idx_message_conversation ON message(conversation_id, num);

CREATE TABLE delivery (
    id              TEXT PRIMARY KEY,
    message_id      TEXT NOT NULL REFERENCES message(id),
    bot_id          TEXT NOT NULL REFERENCES bot(id),
    state           TEXT NOT NULL DEFAULT 'queued'
                    CHECK (state IN ('queued', 'leased', 'delivered', 'acknowledged', 'failed')),
    attempt_count   INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TEXT NOT NULL,
    lease_until     TEXT,
    idempotency_key TEXT NOT NULL UNIQUE,
    last_error      TEXT,
    created_at      TEXT NOT NULL
);
CREATE INDEX idx_delivery_due ON delivery(state, next_attempt_at);
CREATE INDEX idx_delivery_bot ON delivery(bot_id, state);

CREATE TABLE bot_group (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL REFERENCES project(id),
    name            TEXT NOT NULL,
    conversation_id TEXT NOT NULL REFERENCES conversation(id),
    created_at      TEXT NOT NULL,
    UNIQUE (project_id, name)
);

CREATE TABLE group_member (
    group_id TEXT NOT NULL REFERENCES bot_group(id),
    bot_id   TEXT NOT NULL REFERENCES bot(id),
    PRIMARY KEY (group_id, bot_id)
);

CREATE TABLE task (
    id                TEXT PRIMARY KEY,
    origin_message_id TEXT NOT NULL REFERENCES message(id),
    from_bot_id       TEXT REFERENCES bot(id),
    to_bot_id         TEXT NOT NULL REFERENCES bot(id),
    state             TEXT NOT NULL DEFAULT 'open'
                      CHECK (state IN ('open', 'done', 'cancelled', 'expired')),
    deadline_at       TEXT,
    hop_count         INTEGER NOT NULL DEFAULT 0,
    origin_chain      TEXT NOT NULL DEFAULT '',
    created_at        TEXT NOT NULL
);
CREATE INDEX idx_task_to_bot ON task(to_bot_id, state);

CREATE TABLE routine (
    id             TEXT PRIMARY KEY,
    bot_id         TEXT NOT NULL REFERENCES bot(id),
    name           TEXT NOT NULL,
    trigger_json   TEXT NOT NULL,
    prompt         TEXT NOT NULL,
    overlap_policy TEXT NOT NULL DEFAULT 'skip'
                   CHECK (overlap_policy IN ('skip', 'queue_one', 'queue_all', 'replace')),
    enabled        INTEGER NOT NULL DEFAULT 1,
    created_at     TEXT NOT NULL,
    UNIQUE (bot_id, name)
);

CREATE TABLE routine_run (
    id            TEXT PRIMARY KEY,
    routine_id    TEXT NOT NULL REFERENCES routine(id),
    scheduled_for TEXT NOT NULL,
    state         TEXT NOT NULL DEFAULT 'scheduled'
                  CHECK (state IN ('scheduled', 'running', 'succeeded', 'failed', 'skipped', 'cancelled')),
    started_at    TEXT,
    finished_at   TEXT,
    error         TEXT,
    lease_until   TEXT,
    UNIQUE (routine_id, scheduled_for)
);
CREATE INDEX idx_routine_run_state ON routine_run(routine_id, state);

CREATE TABLE inbox (
    delivery_id TEXT PRIMARY KEY REFERENCES delivery(id),
    bot_id      TEXT NOT NULL REFERENCES bot(id),
    read        INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_inbox_unread ON inbox(bot_id, read);

CREATE VIRTUAL TABLE message_fts USING fts5(
    body,
    content='message',
    content_rowid='rowid'
);

CREATE TRIGGER message_ai AFTER INSERT ON message BEGIN
    INSERT INTO message_fts(rowid, body) VALUES (new.rowid, new.body);
END;
CREATE TRIGGER message_ad AFTER DELETE ON message BEGIN
    INSERT INTO message_fts(message_fts, rowid, body) VALUES ('delete', old.rowid, old.body);
END;
"#;
