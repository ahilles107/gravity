//! Versioned SQLite schema migrations.
//!
//! Migrations are applied in order inside a transaction; the current version
//! is stored in `meta(key='schema_version')`.

pub const MIGRATIONS: &[&str] = &[
    MIGRATION_1,
    MIGRATION_2,
    MIGRATION_3,
    MIGRATION_4,
    MIGRATION_5,
    MIGRATION_6,
    MIGRATION_7,
    MIGRATION_8,
    MIGRATION_9,
    MIGRATION_10,
    MIGRATION_11,
];

/// Device-scoped client credentials (Phase 5 hardening).
const MIGRATION_2: &str = r#"
CREATE TABLE device (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    capabilities TEXT NOT NULL DEFAULT 'read',
    created_at   TEXT NOT NULL,
    revoked_at   TEXT,
    last_seen_at TEXT
);
"#;

/// Bot self-management: provenance, archival deletion, and an audit trail.
///
/// Deletion is archival rather than destructive because `message.sender_bot_id`
/// and `conversation.bot_id` are foreign keys into `bot` — removing the row
/// would orphan every message the bot ever sent.
///
/// `UNIQUE (project_id, name)` from migration 1 still applies to archived rows,
/// and SQLite cannot drop a table constraint without a full rebuild (unsafe
/// here, since six tables hold foreign keys into `bot`). So rather than
/// replacing the constraint with a partial index, archiving tombstones the name
/// to `<name>#<id-prefix>` and records the original in `bot_revision`. `#` is
/// outside the validated name charset, so a tombstone can never collide with a
/// live name, and the original is immediately reusable — which matters because
/// create/delete/create is an expected pattern now that bots manage each other.
const MIGRATION_3: &str = r#"
ALTER TABLE bot ADD COLUMN created_by_bot_id TEXT REFERENCES bot(id);
ALTER TABLE bot ADD COLUMN dir_name TEXT NOT NULL DEFAULT '';
ALTER TABLE bot ADD COLUMN deleted_at TEXT;
ALTER TABLE bot ADD COLUMN deleted_by TEXT;
ALTER TABLE conversation ADD COLUMN archived_at TEXT;
CREATE INDEX idx_bot_live ON bot(project_id, deleted_at);

CREATE TABLE bot_revision (
    id         TEXT PRIMARY KEY,
    bot_id     TEXT NOT NULL REFERENCES bot(id),
    changed_by TEXT NOT NULL,
    field      TEXT NOT NULL
               CHECK (field IN ('name', 'avatar', 'description', 'instructions',
                                'created', 'deleted')),
    old_value  TEXT NOT NULL,
    new_value  TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_bot_revision ON bot_revision(bot_id, created_at DESC);
"#;

/// Built-in icons replace emoji avatars.
///
/// The `emoji:` form is gone from the avatar contract, and every bot now gets a
/// face at creation, so the two states this leaves stranded — an emoji a bot
/// picked for itself and the empty default — are dealt an icon here. `random()`
/// spreads the deal so an upgraded project does not come back wearing ten
/// identical faces. `color:` avatars are still valid and are left alone.
///
/// `abs(random() % 10)` rather than `abs(random()) % 10`: `random()` can return
/// i64::MIN, whose absolute value overflows and makes SQLite raise.
const MIGRATION_4: &str = r#"
UPDATE bot
   SET avatar = CASE abs(random() % 10)
    WHEN 0 THEN 'icon:orbit'
    WHEN 1 THEN 'icon:ember'
    WHEN 2 THEN 'icon:moss'
    WHEN 3 THEN 'icon:nova'
    WHEN 4 THEN 'icon:tide'
    WHEN 5 THEN 'icon:quartz'
    WHEN 6 THEN 'icon:volt'
    WHEN 7 THEN 'icon:dusk'
    WHEN 8 THEN 'icon:copper'
    WHEN 9 THEN 'icon:frost'
   END
 WHERE avatar = '' OR avatar LIKE 'emoji:%';
"#;

/// Drop group chats. Bots address each other by name over DMs; the group
/// timeline, its `@`-mention fan-out and the tables behind it are gone.
///
/// This is destructive by necessity: a group conversation's transcript hangs
/// off `bot_group`, and every message in it has deliveries, tasks and inbox
/// rows pointing at it. Those are deleted in foreign-key order. DM history is
/// untouched.
///
/// `conversation.kind` survives as a legacy column: it appears in a CHECK
/// constraint, which SQLite refuses to `DROP COLUMN` past, and rebuilding the
/// table would mean dropping one that `message` holds a foreign key into.
/// Every remaining row is `'dm'`.
const MIGRATION_5: &str = r#"
-- `bot_group.conversation_id` is a foreign key into `conversation`, so the
-- group tables have to go before the rows they point at.
DROP TABLE group_member;
DROP TABLE bot_group;

DELETE FROM inbox WHERE delivery_id IN (
    SELECT d.id FROM delivery d
    JOIN message m ON m.id = d.message_id
    JOIN conversation c ON c.id = m.conversation_id
    WHERE c.kind = 'group');
DELETE FROM delivery WHERE message_id IN (
    SELECT m.id FROM message m
    JOIN conversation c ON c.id = m.conversation_id
    WHERE c.kind = 'group');
DELETE FROM task WHERE origin_message_id IN (
    SELECT m.id FROM message m
    JOIN conversation c ON c.id = m.conversation_id
    WHERE c.kind = 'group');
UPDATE message SET ref_message_id = NULL WHERE ref_message_id IN (
    SELECT m.id FROM message m
    JOIN conversation c ON c.id = m.conversation_id
    WHERE c.kind = 'group');
DELETE FROM message WHERE conversation_id IN (
    SELECT id FROM conversation WHERE kind = 'group');
DELETE FROM conversation WHERE kind = 'group';

ALTER TABLE conversation DROP COLUMN group_id;
"#;

/// Bots are one continuous conversation across restarts.
///
/// A bot's session dies whenever the daemon does, and a fresh `claude` loses
/// everything the bot was in the middle of. `has_session` records that the
/// workspace holds a conversation worth resuming, so every start after the
/// first passes `--continue` instead of opening a blank session. Existing bots
/// are always-on and have therefore already run at least once, so they are
/// marked here rather than losing their context on the upgrade.
const MIGRATION_6: &str = r#"
ALTER TABLE bot ADD COLUMN has_session INTEGER NOT NULL DEFAULT 0;
UPDATE bot SET has_session = 1;
"#;

/// Projects are renameable and deletable.
///
/// `dir_name` freezes the on-disk directory the same way migration 3 did for
/// bots: the directory was previously derived from the project name on every
/// lookup, so a rename would have silently pointed every bot in the project at
/// a directory that does not exist. It defaults to `''` here and is backfilled
/// on open, because the sanitizing is not expressible in SQL.
///
/// Deletion is archival for the same reason it is for bots: `bot.project_id`
/// and `conversation.project_id` are foreign keys into `project`. The name is
/// tombstoned on archive so it can be reused immediately.
const MIGRATION_7: &str = r#"
ALTER TABLE project ADD COLUMN dir_name TEXT NOT NULL DEFAULT '';
ALTER TABLE project ADD COLUMN deleted_at TEXT;
ALTER TABLE project ADD COLUMN deleted_by TEXT;
"#;

/// Signals replace event triggers, and an occurrence becomes accountable.
///
/// `delivery_id` connects an occurrence to its durable delivery. The delivery
/// envelope carries the run id, allowing the `Stop` hook transcript to retire
/// that exact run instead of guessing from the bot's running occurrences.
///
/// `scheduled_for` stays the occurrence's identity — it is half the uniqueness
/// constraint — so a retry moves `next_attempt_at` instead, and leasing reads
/// `COALESCE(next_attempt_at, scheduled_for)`.
///
/// Legacy `{"kind":"event"}` triggers no longer deserialize, so they are
/// disabled here rather than left reading as live routines that never fire.
const MIGRATION_8: &str = r#"
CREATE TABLE signal (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    source       TEXT NOT NULL CHECK (source IN ('bot', 'manual')),
    project_id   TEXT NOT NULL REFERENCES project(id),
    from_bot_id  TEXT REFERENCES bot(id),
    payload_json TEXT NOT NULL DEFAULT '{}',
    origin_chain TEXT NOT NULL DEFAULT '',
    hop_count    INTEGER NOT NULL DEFAULT 0,
    emitted_at   TEXT NOT NULL
);
CREATE INDEX idx_signal_name ON signal(project_id, name, emitted_at DESC);

ALTER TABLE routine_run ADD COLUMN source TEXT NOT NULL DEFAULT 'schedule'
    CHECK (source IN ('schedule', 'manual', 'signal'));
ALTER TABLE routine_run ADD COLUMN attempt INTEGER NOT NULL DEFAULT 0;
ALTER TABLE routine_run ADD COLUMN next_attempt_at TEXT;
ALTER TABLE routine_run ADD COLUMN deadline_at TEXT;
ALTER TABLE routine_run ADD COLUMN signal_id TEXT REFERENCES signal(id);
ALTER TABLE routine_run ADD COLUMN message_id TEXT REFERENCES message(id);
ALTER TABLE routine_run ADD COLUMN delivery_id TEXT REFERENCES delivery(id);

ALTER TABLE routine ADD COLUMN max_duration_seconds INTEGER;
ALTER TABLE routine ADD COLUMN max_attempts INTEGER NOT NULL DEFAULT 1;

CREATE UNIQUE INDEX idx_routine_run_signal
    ON routine_run(routine_id, signal_id) WHERE signal_id IS NOT NULL;

UPDATE routine SET enabled = 0 WHERE trigger_json LIKE '%"kind":"event"%';
"#;

/// Indexed scheduler lookups and normalized signal subscriptions.
///
/// Signal fields remain in `trigger_json` on the wire, but keeping their
/// matching keys in columns avoids deserializing every routine for every
/// emitted signal. Expression indexes cover the scheduler's two clock-based
/// hot paths without changing occurrence identity.
const MIGRATION_9: &str = r#"
ALTER TABLE routine ADD COLUMN signal_name TEXT;
ALTER TABLE routine ADD COLUMN signal_from_bot_id TEXT REFERENCES bot(id);

UPDATE routine
   SET signal_name = json_extract(trigger_json, '$.name'),
       signal_from_bot_id = json_extract(trigger_json, '$.from_bot_id')
 WHERE CASE WHEN json_valid(trigger_json)
            THEN json_extract(trigger_json, '$.kind') = 'signal'
            ELSE 0 END;

CREATE INDEX idx_routine_signal
    ON routine(signal_name, signal_from_bot_id, enabled);
CREATE INDEX idx_routine_run_due
    ON routine_run(state, COALESCE(next_attempt_at, scheduled_for));
CREATE INDEX idx_routine_run_expiry
    ON routine_run(state, lease_until);
"#;

/// Reply budgets on tasks.
///
/// A task now carries a counter of the replies exchanged on it, so the daemon
/// can refuse ping-pong past `MAX_TASK_REPLIES` instead of hoping prompt text
/// holds. Existing open tasks start at zero — their conversations get the full
/// budget from here on. The fan-out index serves the "how many open tasks did
/// this bot delegate from this chain position" count on every `kind: task`
/// send.
const MIGRATION_10: &str = r#"
ALTER TABLE task ADD COLUMN reply_count INTEGER NOT NULL DEFAULT 0;
CREATE INDEX idx_task_fanout ON task(from_bot_id, state, origin_chain);
"#;

/// The model a bot's session runs.
///
/// Claude Code's own `/model` choice is stored in the user-global settings, so
/// a bot inherited whatever that file said at spawn time and silently changed
/// model on the next restart. The column pins the choice per bot; NULL means
/// "no choice observed yet", which still inherits the global default.
const MIGRATION_11: &str = r#"
ALTER TABLE bot ADD COLUMN model TEXT;
"#;

const MIGRATION_1: &str = r#"
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
