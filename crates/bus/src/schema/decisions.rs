//! Migration 12: the decision registry.
//!
//! A bot that needs the owner's ruling has had nowhere to put it. It asks in a
//! terminal the daemon cannot see, or writes "AWAITING PAWEŁ" into its memory
//! file and hopes a lead relays it — and a relayed ruling is a peer's word, not
//! authority. These tables make the ask durable and make the answer something
//! the daemon itself can deliver.
//!
//! Two things the shape here is working around:
//!
//! * `message.kind` is a CHECK constraint, and SQLite cannot alter one in
//!   place. Migration 5 already declined to rebuild `message` because six
//!   tables hold foreign keys into it. So a ruling travels as an ordinary
//!   `note` carrying the new nullable `message.decision_id`, the way
//!   `ref_message_id` already carries correlation.
//! * `source_message_id` and `source_task_id` point at rows retention deletes.
//!   They are deliberately plain columns with no `REFERENCES`: a decision
//!   outlives the exchange that prompted it, and the alternative is either a
//!   prune that fails or a registry that gets pruned away.
//!
//! `normalised_title` exists so the "you already have this open" check is an
//! indexed lookup rather than a scan over every open decision.
pub(super) const MIGRATION_12: &str = r#"
CREATE TABLE decision (
    id                   TEXT PRIMARY KEY,
    project_id           TEXT NOT NULL REFERENCES project(id),
    kind                 TEXT NOT NULL DEFAULT 'decision'
                         CHECK (kind IN ('question', 'decision')),
    title                TEXT NOT NULL,
    normalised_title     TEXT NOT NULL DEFAULT '',
    body                 TEXT NOT NULL,
    options_json         TEXT NOT NULL DEFAULT '[]',
    recommendation       TEXT,
    raised_by_bot_id     TEXT NOT NULL REFERENCES bot(id),
    on_behalf_of_bot_id  TEXT REFERENCES bot(id),
    origin_chain         TEXT NOT NULL DEFAULT '',
    source_message_id    TEXT,
    source_task_id       TEXT,
    priority             TEXT NOT NULL DEFAULT 'normal'
                         CHECK (priority IN ('normal', 'urgent')),
    deadline_at          TEXT,
    deadline_notified_at TEXT,
    state                TEXT NOT NULL DEFAULT 'open'
                         CHECK (state IN ('open', 'answered', 'held', 'settled', 'withdrawn')),
    held_until           TEXT,
    ruling_option        TEXT,
    ruling_text          TEXT,
    ruling_reason        TEXT,
    answered_at          TEXT,
    answered_by          TEXT,
    published_at         TEXT,
    supersedes_id        TEXT REFERENCES decision(id),
    superseded_by_id     TEXT REFERENCES decision(id),
    withdrawn_reason     TEXT,
    edited_at            TEXT,
    edited_by            TEXT,
    created_at           TEXT NOT NULL
);
CREATE INDEX idx_decision_pending ON decision(project_id, state, deadline_at, created_at);
CREATE INDEX idx_decision_raiser ON decision(raised_by_bot_id, state);
CREATE INDEX idx_decision_dup ON decision(project_id, raised_by_bot_id, normalised_title, state);

ALTER TABLE project ADD COLUMN lead_bot_id TEXT REFERENCES bot(id);
ALTER TABLE message ADD COLUMN decision_id TEXT REFERENCES decision(id);

CREATE TABLE decision_comment (
    id            TEXT PRIMARY KEY,
    decision_id   TEXT NOT NULL REFERENCES decision(id),
    author_kind   TEXT NOT NULL CHECK (author_kind IN ('bot', 'user')),
    author_bot_id TEXT REFERENCES bot(id),
    body          TEXT NOT NULL,
    created_at    TEXT NOT NULL
);
CREATE INDEX idx_decision_comment ON decision_comment(decision_id, created_at);

-- One taxonomy for every project: a bot filing 'spend' in one project means
-- what a bot filing 'spend' in another does, and the registry filters across
-- projects by tag. Names are lowercased on write, so plain UNIQUE is enough.
CREATE TABLE tag (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    color       TEXT NOT NULL DEFAULT '',
    created_by  TEXT NOT NULL,
    retired_at  TEXT,
    created_at  TEXT NOT NULL
);

CREATE TABLE decision_tag (
    decision_id TEXT NOT NULL REFERENCES decision(id),
    tag_id      TEXT NOT NULL REFERENCES tag(id),
    PRIMARY KEY (decision_id, tag_id)
);
CREATE INDEX idx_decision_tag_tag ON decision_tag(tag_id);

-- Who was told about a settled decision, and with which delivery. Doubles as
-- the guard against one publish notifying the same bot twice.
CREATE TABLE decision_notification (
    decision_id TEXT NOT NULL REFERENCES decision(id),
    bot_id      TEXT NOT NULL REFERENCES bot(id),
    delivery_id TEXT REFERENCES delivery(id),
    created_at  TEXT NOT NULL,
    PRIMARY KEY (decision_id, bot_id)
);

-- Unlike messages, decisions are edited: a title is corrected, a ruling lands
-- long after the body was written. So this index needs an update trigger that
-- `message_fts` never did, and `coalesce` because `ruling_text` is NULL until
-- the owner answers.
CREATE VIRTUAL TABLE decision_fts USING fts5(
    title,
    body,
    ruling_text,
    content='decision',
    content_rowid='rowid'
);

CREATE TRIGGER decision_ai AFTER INSERT ON decision BEGIN
    INSERT INTO decision_fts(rowid, title, body, ruling_text)
    VALUES (new.rowid, new.title, new.body, coalesce(new.ruling_text, ''));
END;
CREATE TRIGGER decision_ad AFTER DELETE ON decision BEGIN
    INSERT INTO decision_fts(decision_fts, rowid, title, body, ruling_text)
    VALUES ('delete', old.rowid, old.title, old.body, coalesce(old.ruling_text, ''));
END;
CREATE TRIGGER decision_au AFTER UPDATE ON decision BEGIN
    INSERT INTO decision_fts(decision_fts, rowid, title, body, ruling_text)
    VALUES ('delete', old.rowid, old.title, old.body, coalesce(old.ruling_text, ''));
    INSERT INTO decision_fts(rowid, title, body, ruling_text)
    VALUES (new.rowid, new.title, new.body, coalesce(new.ruling_text, ''));
END;
"#;
