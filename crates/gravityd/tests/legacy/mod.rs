//! Fixtures that build a pre-upgrade database by hand, so the migration tests
//! run against schema versions the current code can no longer create.

#![allow(dead_code)]

use bus::schema::MIGRATIONS;
use rusqlite::Connection;

/// Build a database at schema version 2 with a project, a bot, and a message,
/// exactly as a pre-upgrade install would have it.
pub fn v2_database(path: &std::path::Path, workspace: &str) -> String {
    let conn = Connection::open(path).expect("open");
    conn.pragma_update(None, "foreign_keys", "ON").expect("fk");
    for migration in MIGRATIONS.iter().take(2) {
        conn.execute_batch(migration).expect("apply migration");
    }
    conn.execute(
        "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
        [],
    )
    .ok();
    conn.execute(
        "INSERT OR REPLACE INTO meta(key, value) VALUES ('schema_version', '2')",
        [],
    )
    .expect("version");

    conn.execute(
        "INSERT INTO project(id, name, created_at) VALUES ('p1', 'acme', '2024-01-01T00:00:00Z')",
        [],
    )
    .expect("project");
    conn.execute(
        "INSERT INTO bot(id, project_id, name, description, avatar, instructions,
                         workspace_path, created_at)
         VALUES ('b1', 'p1', 'Reviewer', 'reviews', '', 'be strict', ?1,
                 '2024-01-01T00:00:00Z')",
        rusqlite::params![workspace],
    )
    .expect("bot");
    conn.execute(
        "INSERT INTO conversation(id, project_id, kind, bot_id, group_id, title, created_at)
         VALUES ('c1', 'p1', 'dm', 'b1', NULL, 'Reviewer', '2024-01-01T00:00:00Z')",
        [],
    )
    .expect("conversation");
    conn.execute(
        "INSERT INTO message(id, num, conversation_id, sender_kind, sender_bot_id, sender_name,
                             kind, body, created_at)
         VALUES ('m1', 1, 'c1', 'bot', 'b1', 'Reviewer', 'note', 'hello',
                 '2024-01-01T00:00:00Z')",
        [],
    )
    .expect("message");
    "b1".to_string()
}

/// Add a group chat to a v2 database, with the delivery, task and inbox rows a
/// real fan-out would leave behind. Migration 5 has to unwind all of it.
pub fn add_group(path: &std::path::Path, bot_id: &str) {
    let conn = Connection::open(path).expect("open");
    conn.pragma_update(None, "foreign_keys", "ON").expect("fk");
    conn.execute(
        "INSERT INTO conversation(id, project_id, kind, bot_id, group_id, title, created_at)
         VALUES ('c2', 'p1', 'group', NULL, 'g1', 'team', '2024-01-01T00:00:00Z')",
        [],
    )
    .expect("conversation");
    conn.execute(
        "INSERT INTO bot_group(id, project_id, name, conversation_id, created_at)
         VALUES ('g1', 'p1', 'team', 'c2', '2024-01-01T00:00:00Z')",
        [],
    )
    .expect("group");
    conn.execute(
        "INSERT INTO group_member(group_id, bot_id) VALUES ('g1', ?1)",
        rusqlite::params![bot_id],
    )
    .expect("member");
    conn.execute(
        "INSERT INTO message(id, num, conversation_id, sender_kind, sender_name,
                             kind, body, created_at)
         VALUES ('m2', 2, 'c2', 'user', 'user', 'chat', '@reviewer look',
                 '2024-01-01T00:00:00Z')",
        [],
    )
    .expect("message");
    conn.execute(
        "INSERT INTO delivery(id, message_id, bot_id, next_attempt_at, idempotency_key, created_at)
         VALUES ('d1', 'm2', ?1, '2024-01-01T00:00:00Z', 'm2:b1', '2024-01-01T00:00:00Z')",
        rusqlite::params![bot_id],
    )
    .expect("delivery");
    conn.execute(
        "INSERT INTO inbox(delivery_id, bot_id) VALUES ('d1', ?1)",
        rusqlite::params![bot_id],
    )
    .expect("inbox");
    conn.execute(
        "INSERT INTO task(id, origin_message_id, to_bot_id, created_at)
         VALUES ('t1', 'm2', ?1, '2024-01-01T00:00:00Z')",
        rusqlite::params![bot_id],
    )
    .expect("task");
}
