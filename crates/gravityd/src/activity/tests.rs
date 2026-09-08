use super::*;

fn write_transcript(home: &Path, workspace: &Path, lines: &[&str]) -> PathBuf {
    let dir = transcript_dir(home, workspace);
    fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("session.jsonl");
    fs::write(&path, lines.join("\n")).expect("write");
    path
}

fn assistant(text: &str, at: &str) -> String {
    serde_json::json!({
        "type": "assistant",
        "timestamp": at,
        "message": { "content": [{ "type": "text", "text": text }] }
    })
    .to_string()
}

fn at(stamp: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(stamp)
        .expect("stamp")
        .with_timezone(&Utc)
}

#[test]
fn mangles_the_workspace_path_the_way_claude_code_does() {
    let dir = transcript_dir(
        Path::new("/Users/me"),
        Path::new("/Users/me/.gravity/projects/labs/bots/chief/workspace"),
    );
    assert_eq!(
        dir,
        Path::new("/Users/me/.claude/projects")
            .join("-Users-me--gravity-projects-labs-bots-chief-workspace")
    );
}

#[test]
fn reads_the_newest_assistant_turn() {
    let tmp = tempfile::tempdir().expect("tmp");
    let workspace = Path::new("/w/bot/workspace");
    write_transcript(
        tmp.path(),
        workspace,
        &[
            &assistant("older turn", "2026-01-01T00:00:00Z"),
            &assistant("newest turn", "2026-01-02T00:00:00Z"),
            &serde_json::json!({ "type": "system", "timestamp": "2026-01-03T00:00:00Z" })
                .to_string(),
        ],
    );

    let found = from_transcript(tmp.path(), workspace).expect("activity");
    assert_eq!(found.text, "newest turn");
    assert_eq!(found.at, at("2026-01-02T00:00:00Z"));
    assert_eq!(found.from, "");
}

#[test]
fn skips_turns_without_text_and_unparsable_lines() {
    let tmp = tempfile::tempdir().expect("tmp");
    let workspace = Path::new("/w/bot/workspace");
    write_transcript(
        tmp.path(),
        workspace,
        &[
            &assistant("spoke here", "2026-01-01T00:00:00Z"),
            &serde_json::json!({
                "type": "assistant",
                "timestamp": "2026-01-02T00:00:00Z",
                "message": { "content": [{ "type": "tool_use", "name": "Bash" }] }
            })
            .to_string(),
            "{ not json",
        ],
    );

    let found = from_transcript(tmp.path(), workspace).expect("activity");
    assert_eq!(found.text, "spoke here");
}

#[test]
fn returns_nothing_without_a_transcript() {
    let tmp = tempfile::tempdir().expect("tmp");
    assert!(from_transcript(tmp.path(), Path::new("/w/absent/workspace")).is_none());
}

#[test]
fn truncate_collapses_whitespace_and_caps_length() {
    assert_eq!(truncate("  two   lines\nhere "), "two lines here");
    let long = "x".repeat(MAX_CHARS + 50);
    let capped = truncate(&long);
    assert_eq!(capped.chars().count(), MAX_CHARS + 1);
    assert!(capped.ends_with('…'));
}

#[test]
fn newer_picks_the_later_stamp() {
    let early = Activity {
        from: String::new(),
        text: "early".into(),
        at: at("2026-01-01T00:00:00Z"),
    };
    let late = Activity {
        from: "you".into(),
        text: "late".into(),
        at: at("2026-01-02T00:00:00Z"),
    };
    assert_eq!(
        newer(Some(early.clone()), Some(late.clone())).map(|a| a.text),
        Some("late".to_string())
    );
    assert_eq!(
        newer(Some(late.clone()), Some(early.clone())).map(|a| a.text),
        Some("late".to_string())
    );
    assert_eq!(newer(None, Some(early.clone())), Some(early));
    assert_eq!(newer(None, None), None);
}

/// The bug this module's watcher exists for: stamps from the two sources
/// carry different RFC 3339 spellings of the same instant ("Z" against
/// "+00:00"), so comparing them as strings orders them wrongly.
#[test]
fn compares_stamps_as_instants_not_strings() {
    let transcript = Activity {
        from: String::new(),
        text: "from the transcript".into(),
        at: at("2026-01-02T00:00:00Z"),
    };
    let older_bus = Activity {
        from: "you".into(),
        text: "from the bus".into(),
        at: at("2026-01-01T23:59:59+00:00"),
    };
    assert_eq!(
        newer(Some(transcript), Some(older_bus)).map(|a| a.text),
        Some("from the transcript".to_string())
    );
}
