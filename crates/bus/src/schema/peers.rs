//! Migration 14: peer daemons and linked bots.
//!
//! A linked bot is an ordinary `bot` row with `peer_id` set, so every foreign
//! key that assumes a local bot (message senders, task ends, deliveries) keeps
//! holding. The mapping tables carry the other side's ids: a message or task
//! that crosses the link has an id on each daemon, and threading, dedupe and
//! closing a mirrored task all need to translate between them.

pub(super) const MIGRATION_14: &str = r#"
CREATE TABLE peer (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL UNIQUE,
    daemon_id    TEXT UNIQUE,
    url          TEXT,
    created_at   TEXT NOT NULL,
    last_seen_at TEXT,
    revoked_at   TEXT
);

ALTER TABLE bot ADD COLUMN peer_id TEXT REFERENCES peer(id);
ALTER TABLE bot ADD COLUMN remote_bot_id TEXT;
CREATE UNIQUE INDEX idx_bot_remote ON bot(peer_id, remote_bot_id, project_id)
    WHERE peer_id IS NOT NULL AND deleted_at IS NULL;

CREATE TABLE peer_link (
    peer_id TEXT NOT NULL REFERENCES peer(id),
    bot_id  TEXT NOT NULL REFERENCES bot(id),
    PRIMARY KEY (peer_id, bot_id)
);

CREATE TABLE peer_message (
    peer_id           TEXT NOT NULL REFERENCES peer(id),
    remote_message_id TEXT NOT NULL,
    message_id        TEXT NOT NULL,
    PRIMARY KEY (peer_id, remote_message_id)
);
CREATE INDEX idx_peer_message_local ON peer_message(peer_id, message_id);

CREATE TABLE peer_task (
    peer_id        TEXT NOT NULL REFERENCES peer(id),
    remote_task_id TEXT NOT NULL,
    task_id        TEXT NOT NULL,
    PRIMARY KEY (peer_id, remote_task_id)
);
CREATE INDEX idx_peer_task_local ON peer_task(peer_id, task_id);
"#;

/// Migration 15: a revoked peer frees its name and its daemon.
///
/// The name is tombstoned the way an archived bot's is, and the daemon id
/// released, so pairing the same machine again can reuse both. New
/// revocations do the same in `Db::revoke_peer`.
pub(super) const MIGRATION_15: &str = r#"
UPDATE peer SET name = name || '#' || substr(id, 1, 8)
    WHERE revoked_at IS NOT NULL AND instr(name, '#') = 0;
UPDATE peer SET daemon_id = NULL WHERE revoked_at IS NOT NULL;
"#;

/// Migration 16: projects linked across peers.
///
/// A link is recorded on both daemons, each holding the other's project id.
/// A project links with at most one project per peer, and the same remote
/// project with at most one project here.
pub(super) const MIGRATION_16: &str = r#"
CREATE TABLE project_link (
    project_id          TEXT NOT NULL REFERENCES project(id),
    peer_id             TEXT NOT NULL REFERENCES peer(id),
    remote_project_id   TEXT NOT NULL,
    remote_project_name TEXT NOT NULL,
    linked_at           TEXT NOT NULL,
    PRIMARY KEY (project_id, peer_id)
);
CREATE UNIQUE INDEX idx_project_link_remote ON project_link(peer_id, remote_project_id);
"#;
