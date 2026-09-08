//! Upgrading a database that predates bot self-management and group removal.
//!
//! Migration 3 adds provenance and archival columns to a live `bot` table. It
//! can only give `dir_name` a default of `''`, which would point every upgraded
//! bot at the shared `bots/` directory, so the daemon backfills it on open.
//! These tests run the real migration against a populated v2 database.

mod legacy;

use bus::schema::MIGRATIONS;
use gravityd::db::Db;
use legacy::{add_group, v2_database};
use rusqlite::Connection;

#[test]
fn upgrading_from_v2_preserves_bots_and_their_history() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("bus.sqlite");
    let bot_id = v2_database(
        &path,
        "/home/u/.gravity/projects/acme/bots/reviewer/workspace",
    );

    let db = Db::open(&path).expect("migrate");

    let bot = db.get_bot(&bot_id).expect("query").expect("bot survives");
    assert_eq!(bot.name, "Reviewer");
    assert_eq!(bot.instructions, "be strict");
    assert!(bot.deleted_at.is_none(), "existing bots must stay live");
    assert!(
        bot.created_by_bot_id.is_none(),
        "user-made bots have no creator"
    );
    assert_eq!(db.count_live_bots("p1").expect("count"), 1);
    assert!(db.integrity_check().expect("integrity"));
}

/// The backfill has to recover the real directory, not invent one, or an
/// upgraded bot would read another bot's workspace.
#[test]
fn the_directory_name_is_recovered_from_the_workspace_path() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("bus.sqlite");
    // Deliberately not what the name would sanitize to: the bot was created
    // when a different name mapped to this directory.
    let bot_id = v2_database(
        &path,
        "/home/u/.gravity/projects/acme/bots/old-handle/workspace",
    );

    let db = Db::open(&path).expect("migrate");

    let bot = db.get_bot(&bot_id).expect("query").expect("bot");
    assert_eq!(
        bot.dir_name, "old-handle",
        "must come from the stored path, not the current name"
    );
}

/// A malformed or empty path must still yield a usable directory rather than
/// an empty string that resolves to the shared `bots/` root.
#[test]
fn the_directory_name_falls_back_to_the_bot_name() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("bus.sqlite");
    let bot_id = v2_database(&path, "");

    let db = Db::open(&path).expect("migrate");

    let bot = db.get_bot(&bot_id).expect("query").expect("bot");
    assert_eq!(bot.dir_name, "reviewer");
    assert!(!bot.dir_name.is_empty());
}

#[test]
fn migrating_twice_is_a_no_op() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("bus.sqlite");
    v2_database(&path, "/tmp/acme/bots/reviewer/workspace");

    let first = Db::open(&path).expect("migrate");
    let dir_name = first.get_bot("b1").expect("q").expect("bot").dir_name;
    drop(first);

    let second = Db::open(&path).expect("reopen");
    assert_eq!(
        second.get_bot("b1").expect("q").expect("bot").dir_name,
        dir_name
    );
    assert!(second.integrity_check().expect("integrity"));
}

/// Archiving an upgraded bot must free its name for reuse.
#[test]
fn an_upgraded_bot_can_be_archived_and_its_name_reused() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("bus.sqlite");
    let bot_id = v2_database(&path, "/tmp/acme/bots/reviewer/workspace");
    let db = Db::open(&path).expect("migrate");

    db.archive_bot(&bot_id, "user").expect("archive");

    assert!(db.get_bot_by_name("p1", "Reviewer").expect("q").is_none());
    assert_eq!(db.count_live_bots("p1").expect("count"), 0);
    // The row and its messages survive, so history stays attributable.
    let archived = db.get_bot(&bot_id).expect("q").expect("row");
    assert!(archived.deleted_at.is_some());
    assert_eq!(Db::display_name(&archived), "Reviewer");

    let reborn = db
        .create_bot("p1", "Reviewer", "", "", "", "/tmp/x", "reviewer-2", None)
        .expect("name is free again");
    assert_eq!(reborn.name, "Reviewer");
}

/// Migration 4 retires the `emoji:` avatar form. An upgraded bot must come back
/// wearing a built-in icon, not an avatar the client can no longer render.
#[test]
fn upgrading_deals_an_icon_to_emoji_and_bare_avatars() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("bus.sqlite");
    // `b1` is created with an empty avatar, the other two alongside it.
    v2_database(&path, "/tmp/acme/bots/reviewer/workspace");
    let conn = Connection::open(&path).expect("reopen");
    for (id, name, avatar) in [
        ("b2", "Emojied", "emoji:\u{1F6E0}\u{FE0F}"),
        ("b3", "Coloured", "color:#4a90d9"),
    ] {
        conn.execute(
            "INSERT INTO bot(id, project_id, name, description, avatar, instructions,
                             workspace_path, created_at)
             VALUES (?1, 'p1', ?2, '', ?3, '', '/tmp/x', '2024-01-01T00:00:00Z')",
            rusqlite::params![id, name, avatar],
        )
        .expect("insert");
    }
    drop(conn);

    let db = Db::open(&path).expect("migrate");

    for id in ["b1", "b2"] {
        let avatar = db.get_bot(id).expect("q").expect("bot").avatar;
        let icon = avatar
            .strip_prefix("icon:")
            .unwrap_or_else(|| panic!("{id} kept a non-icon avatar: {avatar}"));
        assert!(
            bus::avatar::ICONS.contains(&icon),
            "{id} got an icon the client does not ship: {icon}"
        );
    }
    // A colour was always valid and stays the bot's own choice.
    assert_eq!(
        db.get_bot("b3").expect("q").expect("bot").avatar,
        "color:#4a90d9"
    );
}

/// Groups are gone: the tables, their conversations and everything hanging off
/// their messages. A bot's own DM thread has to survive intact.
#[test]
fn upgrading_drops_group_chats_and_keeps_dms() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("bus.sqlite");
    let bot_id = v2_database(&path, "/tmp/acme/bots/reviewer/workspace");
    add_group(&path, &bot_id);

    let db = Db::open(&path).expect("migrate");

    let conversations = db.list_conversations(None).expect("list");
    assert_eq!(conversations.len(), 1, "only the DM survives");
    assert_eq!(conversations[0].bot_id, bot_id);
    assert!(db.get_message("m1").expect("q").is_some(), "DM history");
    assert!(db.get_message("m2").expect("q").is_none(), "group history");
    assert!(db.list_deliveries(None, None).expect("q").is_empty());
    assert!(db.integrity_check().expect("integrity"));
}

/// Bots that predate the resume flag have already been running for as long as
/// the daemon has existed, so the upgrade must credit them with the
/// conversation they have rather than blanking it on the next start.
#[test]
fn upgrading_marks_existing_bots_as_having_a_session() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("bus.sqlite");
    let bot_id = v2_database(&path, "/tmp/acme/bots/reviewer/workspace");

    let db = Db::open(&path).expect("migrate");

    assert!(db.bot_has_session(&bot_id).expect("flag"));
}

/// The flag has to outlive the process: a daemon restart is exactly the case
/// it exists for.
#[test]
fn the_session_flag_survives_reopening_the_database() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("bus.sqlite");
    let bot_id = {
        let db = Db::open(&path).expect("open");
        let project = db.create_project("acme", "acme").expect("project");
        let bot = db
            .create_bot(
                &project.id,
                "Reviewer",
                "",
                "",
                "",
                "/tmp/x",
                "reviewer",
                None,
            )
            .expect("bot");
        assert!(
            !db.bot_has_session(&bot.id).expect("flag"),
            "a bot that never started has nothing to continue"
        );
        db.mark_bot_session(&bot.id).expect("mark");
        bot.id
    };

    let db = Db::open(&path).expect("reopen");
    assert!(db.bot_has_session(&bot_id).expect("flag"));
}

/// An event trigger no longer parses, so migration 8 has to disable the
/// routine rather than leave it looking live while it can never fire.
#[test]
fn a_legacy_event_routine_is_disabled_and_inert() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("bus.sqlite");
    let bot_id = v2_database(&path, "/tmp/acme/bots/reviewer/workspace");
    {
        let conn = Connection::open(&path).expect("open");
        conn.execute(
            "INSERT INTO routine(id, bot_id, name, trigger_json, prompt, created_at)
             VALUES ('r1', ?1, 'on-done',
                     '{\"kind\":\"event\",\"event\":\"bot_done\"}',
                     'react', '2024-01-01T00:00:00Z')",
            rusqlite::params![bot_id],
        )
        .expect("routine");
    }

    let db = Db::open(&path).expect("migrate");

    let routine = db.get_routine("r1").expect("query").expect("routine");
    assert!(!routine.enabled);
    assert_eq!(routine.trigger, bus::Trigger::unparsable());
    // The sentinel is inert: the scheduler can compute no occurrence from it.
    assert!(gravityd::scheduler::next_occurrence(
        &routine.trigger,
        chrono::Utc::now(),
        routine.created_at
    )
    .is_none());
    assert_eq!(routine.max_attempts, 1);
    assert!(routine.max_duration_seconds.is_none());
}

#[test]
fn upgrading_indexes_and_backfills_signal_subscriptions() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("bus.sqlite");
    let bot_id = v2_database(&path, "/tmp/acme/bots/reviewer/workspace");
    {
        let conn = Connection::open(&path).expect("open");
        conn.execute(
            "INSERT INTO routine(id, bot_id, name, trigger_json, prompt, created_at)
             VALUES ('r1', ?1, 'on-deploy',
                     '{\"kind\":\"signal\",\"name\":\"deploy.finished\"}',
                     'react', '2024-01-01T00:00:00Z')",
            rusqlite::params![bot_id],
        )
        .expect("routine");
    }

    let db = Db::open(&path).expect("migrate");
    let subscribers = db
        .signal_routines("p1", "deploy.finished", Some("b1"))
        .expect("subscribers");
    assert_eq!(subscribers.len(), 1);
    assert_eq!(subscribers[0].id, "r1");
    drop(db);

    let conn = Connection::open(&path).expect("reopen");
    for index in [
        "idx_routine_signal",
        "idx_routine_run_due",
        "idx_routine_run_expiry",
    ] {
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
                [index],
                |row| row.get(0),
            )
            .expect("index query");
        assert_eq!(count, 1, "missing {index}");
    }
}

/// Migration 10 gives existing open tasks a zero reply budget and leaves them
/// otherwise untouched: a legacy chain deeper than the new hop limit stays
/// open and valid (the limit binds new sends only), and a NULL deadline stays
/// NULL rather than being backfilled into a mass expiry on upgrade.
#[test]
fn upgrading_gives_open_tasks_a_reply_budget_and_keeps_them_open() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("bus.sqlite");
    let bot_id = v2_database(&path, "/tmp/acme/bots/reviewer/workspace");
    {
        let conn = Connection::open(&path).expect("open");
        for migration in MIGRATIONS.iter().skip(2).take(7) {
            conn.execute_batch(migration).expect("apply migration");
        }
        conn.execute(
            "INSERT OR REPLACE INTO meta(key, value) VALUES ('schema_version', '9')",
            [],
        )
        .expect("version");
        // Six hops was legal under the old limit of eight.
        conn.execute(
            "INSERT INTO task(id, origin_message_id, to_bot_id, hop_count,
                              origin_chain, created_at)
             VALUES ('t9', 'm1', ?1, 6, 'x,y,z,w,v,u', '2024-01-01T00:00:00Z')",
            rusqlite::params![bot_id],
        )
        .expect("task");
    }

    let db = Db::open(&path).expect("migrate");
    let task = db.get_task("t9").expect("query").expect("task survives");
    assert_eq!(task.reply_count, 0);
    assert_eq!(task.state, bus::TaskState::Open);
    assert!(
        task.deadline_at.is_none(),
        "no deadline backfill on upgrade"
    );
    assert_eq!(task.hop_count, 6, "legacy chains stay valid");
    assert!(db.integrity_check().expect("integrity"));
}
