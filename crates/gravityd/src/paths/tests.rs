use super::*;

fn cfg_in(dir: &Path) -> Config {
    Config {
        home: dir.to_path_buf(),
        ..Config::default()
    }
}

fn spec<'a>(name: &'a str, instructions: &'a str) -> BotProvision<'a> {
    BotProvision {
        project_name: "proj",
        project_dir_name: "proj",
        bot_id: "bot-1",
        name,
        dir_name: "reviewer",
        description: "reviews code",
        instructions,
        daemon_port: 7777,
        bot_token_env: "GRAVITY_TOKEN",
        max_bots_per_project: 12,
        artifacts_dir: "/tmp/proj/artifacts".to_string(),
    }
}

#[test]
fn system_md_carries_instructions_and_is_regenerated() {
    let tmp = tempfile::tempdir().expect("tmp");
    let cfg = cfg_in(tmp.path());
    let dirs = provision_bot(&cfg, &spec("Reviewer", "be strict")).expect("provision");

    let first = fs::read_to_string(dirs.root.join("system.md")).expect("read");
    assert!(first.contains("be strict"), "instructions missing: {first}");

    write_system_md(&dirs.root, &spec("Reviewer", "be lenient")).expect("rewrite");
    let second = fs::read_to_string(dirs.root.join("system.md")).expect("read");
    assert!(second.contains("be lenient"));
    assert!(!second.contains("be strict"));
}

/// The bot owns `CLAUDE.md` and `FACTS.md`; re-provisioning must never
/// overwrite what it has written in either.
#[test]
fn memory_files_are_never_overwritten_after_creation() {
    let tmp = tempfile::tempdir().expect("tmp");
    let cfg = cfg_in(tmp.path());
    let dirs = provision_bot(&cfg, &spec("Reviewer", "v1")).expect("provision");

    let memory = dirs.workspace.join("CLAUDE.md");
    let facts = dirs.workspace.join("FACTS.md");
    fs::write(&memory, "# my living context\nremember this").expect("write");
    fs::write(&facts, "- the build runs on build-host").expect("write");

    provision_bot(&cfg, &spec("Reviewer", "v2")).expect("reprovision");
    assert_eq!(
        fs::read_to_string(&memory).expect("read"),
        "# my living context\nremember this"
    );
    assert_eq!(
        fs::read_to_string(&facts).expect("read"),
        "- the build runs on build-host"
    );
}

/// Bots created before `FACTS.md` existed have a `CLAUDE.md` of their own;
/// seeding must add the missing file and leave that one alone.
#[test]
fn seeding_backfills_only_the_missing_file() {
    let tmp = tempfile::tempdir().expect("tmp");
    let workspace = tmp.path().join("workspace");
    fs::create_dir_all(&workspace).expect("mkdir");
    fs::write(workspace.join("CLAUDE.md"), "# old bot").expect("write");

    seed_memory_files(&workspace, "Reviewer").expect("seed");

    assert_eq!(
        fs::read_to_string(workspace.join("CLAUDE.md")).expect("read"),
        "# old bot"
    );
    assert!(fs::read_to_string(workspace.join("FACTS.md"))
        .expect("read")
        .contains("Facts — Reviewer"));
}

#[test]
fn regeneration_is_idempotent() {
    let tmp = tempfile::tempdir().expect("tmp");
    let cfg = cfg_in(tmp.path());
    let dirs = provision_bot(&cfg, &spec("Reviewer", "steady")).expect("provision");
    let path = dirs.root.join("system.md");

    let before = fs::metadata(&path)
        .expect("meta")
        .modified()
        .expect("mtime");
    write_system_md(&dirs.root, &spec("Reviewer", "steady")).expect("rewrite");
    let after = fs::metadata(&path)
        .expect("meta")
        .modified()
        .expect("mtime");
    assert_eq!(before, after, "unchanged content should not rewrite");
}
