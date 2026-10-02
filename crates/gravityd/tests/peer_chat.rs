//! A linked bot's real chat, read across the peer link: the Mac's app asks
//! its daemon, which asks the PC's daemon that runs the bot.

mod common;

use std::io::Write;

use common::peers::team;
use common::tasks::drain_until;
use serde_json::json;

fn transcript(t: &common::peers::Team) -> std::path::PathBuf {
    let windev = t
        .win
        .app
        .db
        .get_bot(&t.windev_id)
        .expect("db")
        .expect("bot");
    let mangled: String = windev
        .workspace_path
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let dir = t
        .win
        .app
        .cfg
        .user_home
        .join(".claude/projects")
        .join(mangled);
    std::fs::create_dir_all(&dir).expect("dir");
    dir.join("session.jsonl")
}

fn append(path: &std::path::Path, record: serde_json::Value) {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("open");
    writeln!(file, "{record}").expect("write");
}

#[tokio::test]
async fn a_linked_bots_chat_is_read_from_its_machine_and_followed_live() {
    let mut t = team().await;
    // First contact creates the PC's linked `lead`, whose name the chat uses.
    t.lead
        .call(
            "send_message",
            json!({"to": "windev", "kind": "note", "body": "hello"}),
        )
        .await;
    drain_until(&mut t.windev, "hello").await;
    let path = transcript(&t);
    append(
        &path,
        json!({"type": "user", "uuid": "turn-1", "timestamp": "2026-10-01T10:00:00Z",
               "origin": {"kind": "peer"}, "isMeta": true,
               "message": {"content": "Another Claude session sent a message:\n[msg #3 from LEAD · task · task_id t1] sign the MSI"}}),
    );
    append(
        &path,
        json!({"type": "assistant", "uuid": "a1", "timestamp": "2026-10-01T10:00:01Z",
               "message": {"content": [{"type": "tool_use", "id": "sign", "name": "Bash",
                                        "input": {"command": "signtool sign app.msi"}}]}}),
    );

    let chat = t
        .mac_client
        .request(json!({"type": "list_chat", "bot_id": t.linked_windev}))
        .await;
    assert_eq!(chat["type"], "chat", "{chat}");
    let turn = &chat["turns"][0];
    assert_eq!(
        turn["bot_id"], t.linked_windev,
        "turns carry the Mac's id for the bot"
    );
    assert_eq!(turn["trigger"]["from"], "lead");
    assert_eq!(turn["items"][0]["title"], "Ran a command");

    let step = t
        .mac_client
        .request(json!({"type": "get_chat_step", "bot_id": t.linked_windev, "item_id": "sign"}))
        .await;
    assert_eq!(step["detail"]["command"], "signtool sign app.msi");

    append(
        &path,
        json!({"type": "assistant", "uuid": "a2", "timestamp": "2026-10-01T10:00:02Z",
               "message": {"content": [{"type": "text", "text": "Signed."}]}}),
    );
    let linked = t.linked_windev.clone();
    let pushed = t
        .mac_client
        .wait_for(|v| v["type"] == "chat_turns" && v["bot_id"] == json!(linked.clone()))
        .await;
    let items = pushed["turns"][0]["items"].as_array().expect("items");
    assert_eq!(items.last().expect("item")["markdown"], "Signed.");
}

#[tokio::test]
async fn a_linked_bots_files_are_read_from_its_machine_and_nothing_else() {
    let mut t = team().await;
    let windev = t
        .win
        .app
        .db
        .get_bot(&t.windev_id)
        .expect("db")
        .expect("bot");
    std::fs::write(
        std::path::Path::new(&windev.workspace_path).join("notes.md"),
        "# Build notes",
    )
    .expect("write");
    let read = t
        .mac_client
        .request(json!({"type": "read_file", "bot_id": t.linked_windev, "path": "notes.md"}))
        .await;
    assert_eq!(read["file"]["text"], "# Build notes", "{read}");
    let refused = t
        .mac_client
        .request(json!({"type": "read_file", "bot_id": t.linked_windev, "path": "/etc/hosts"}))
        .await;
    assert_eq!(refused["type"], "error");
}

#[tokio::test]
async fn an_unreachable_peer_says_so() {
    let mut t = team().await;
    let mut win_client = common::WsClient::connect(&t.win).await;
    win_client
        .request(json!({"type": "revoke_peer", "peer_id": t.win_peer_id}))
        .await;
    let mac = &t.mac;
    common::peers::wait_until("the link drops", || {
        mac.app
            .db
            .list_peers()
            .expect("peers")
            .iter()
            .all(|p| !mac.app.peers.is_online(&p.id))
    })
    .await;
    let chat = t
        .mac_client
        .request(json!({"type": "list_chat", "bot_id": t.linked_windev}))
        .await;
    assert_eq!(chat["type"], "error");
    assert_eq!(chat["code"], "unavailable");
}
