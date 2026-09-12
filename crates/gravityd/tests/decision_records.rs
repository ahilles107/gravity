//! Rulings a bot heard at its terminal and filed for the owner.

mod common;

use common::tasks::{drain_until, error_text, project_with_bots};
use serde_json::json;

#[tokio::test]
async fn a_terminal_ruling_is_filed_as_a_relay_and_names_who_relayed_it() {
    // Paweł told Patch "let's bump forgejo to 16" at its own terminal, which
    // reverses what Argus has on record.
    let (_pair, mut c) = project_with_bots(&["argus", "patch"]).await;
    let recorded = c[1]
        .call(
            "record_decision",
            json!({
                "title": "Bump Forgejo to 16",
                "body": "Asked at my terminal; reverses Argus's note that it is not scheduled.",
                "ruling_text": "let's bump forgejo to 16",
                "tags": ["upgrades"],
                "notify": ["argus"]
            }),
        )
        .await;
    assert_eq!(recorded["decision"]["state"], json!("settled"));
    let answered_by = recorded["decision"]["ruling"]["answered_by"]
        .as_str()
        .unwrap();
    assert!(answered_by.starts_with("owner-via-bot:"), "{answered_by}");

    let delivered = drain_until(&mut c[0], "bump forgejo to 16").await;
    assert!(
        delivered.iter().any(|m| m["body"]
            .as_str()
            .is_some_and(|b| b.contains("bump forgejo to 16"))),
        "the lead's list must not go stale: {delivered:?}"
    );
}

#[tokio::test]
async fn recording_names_the_bots_it_could_not_tell_instead_of_failing() {
    // A typo in one name used to fail the call after the record was already
    // filed and the earlier bots already told — so the bot filed it again.
    let (_pair, mut c) = project_with_bots(&["chief", "auction"]).await;
    let recorded = c[1]
        .call(
            "record_decision",
            json!({
                "title": "Ship on Friday",
                "body": "asked at the terminal",
                "ruling_text": "Yes, ship it Friday.",
                "notify": ["chief", "nobody"]
            }),
        )
        .await;
    assert_eq!(recorded["decision"]["state"], json!("settled"));
    assert_eq!(recorded["notified"], json!(["chief"]));
    assert_eq!(recorded["not_notified"], json!(["nobody"]));
    let notes = drain_until(&mut c[0], "Ship on Friday").await;
    assert!(!notes.is_empty(), "the named bot is still told: {notes:?}");
}

#[tokio::test]
async fn relayed_rulings_stop_at_a_cap_until_the_owner_confirms() {
    let (_pair, mut c) = project_with_bots(&["auction"]).await;
    for i in 0..bus::MAX_UNCONFIRMED_RELAYS_PER_BOT {
        c[0].call(
            "record_decision",
            json!({"title": format!("ruling {i}"), "body": "x", "ruling_text": "yes"}),
        )
        .await;
    }
    let raw = c[0]
        .call_raw(
            "record_decision",
            json!({"title": "one more", "body": "x", "ruling_text": "yes"}),
        )
        .await;
    assert!(
        error_text(&raw).contains("not confirmed"),
        "{}",
        error_text(&raw)
    );
}
