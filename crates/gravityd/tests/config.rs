//! The `get_config` / `set_config` control-plane surface.

mod common;

use common::*;
use futures::StreamExt;
use gravityd::overrides::AUTO_COMPACT_META_KEY;
use serde_json::json;

#[tokio::test]
async fn get_config_reports_the_launch_config() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;

    let reply = c.request(json!({"type": "get_config"})).await;
    assert_eq!(reply["type"], "config", "{reply}");
    let config = &reply["config"];
    assert_eq!(config["bind"], json!(["127.0.0.1"]));
    assert_eq!(config["port"], json!(d.addr.port()));
    assert_eq!(config["runtime"], json!("double"));
    assert_eq!(config["auto_compact_window"], json!(250_000));
}

#[tokio::test]
async fn set_config_updates_persists_and_validates_the_window() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;

    // A new value is applied, echoed back and written to the meta table.
    let reply = c
        .request(json!({"type": "set_config", "auto_compact_window": 300_000}))
        .await;
    assert_eq!(reply["type"], "config", "{reply}");
    assert_eq!(reply["config"]["auto_compact_window"], json!(300_000));
    assert_eq!(
        d.app.db.get_meta(AUTO_COMPACT_META_KEY).expect("meta"),
        Some("300000".to_string())
    );

    // Null clears the override down to the model default.
    let cleared = c
        .request(json!({"type": "set_config", "auto_compact_window": null}))
        .await;
    assert_eq!(cleared["config"]["auto_compact_window"], json!(null));
    assert_eq!(
        d.app.db.get_meta(AUTO_COMPACT_META_KEY).expect("meta"),
        Some("default".to_string())
    );

    // Out-of-range and wrong-typed values are rejected without effect.
    for bad in [json!(999), json!(2_000_000), json!("nope")] {
        let denied = c
            .request(json!({"type": "set_config", "auto_compact_window": bad}))
            .await;
        assert_eq!(denied["type"], "error", "{denied}");
        assert_eq!(denied["code"], "invalid_request");
    }
    let missing = c.request(json!({"type": "set_config"})).await;
    assert_eq!(missing["code"], "invalid_request");

    let after = c.request(json!({"type": "get_config"})).await;
    assert_eq!(after["config"]["auto_compact_window"], json!(null));
}

#[tokio::test]
async fn set_config_requires_the_control_capability() {
    let d = spawn_daemon().await;
    let mut owner = WsClient::connect(&d).await;

    let created = owner
        .request(json!({
            "type": "create_device", "name": "viewer", "capabilities": ["read"]
        }))
        .await;
    let token = created["token"].as_str().expect("token").to_string();

    let url = format!("ws://{}/ws", d.addr);
    let (socket, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("connect");
    let (tx, rx) = socket.split();
    let mut ro = WsClient {
        tx,
        rx,
        next_req: 1,
    };
    let hello = ro
        .request(json!({"type": "hello", "protocol_version": 2, "token": token}))
        .await;
    assert_eq!(hello["type"], "hello_ok");

    // Reading the config is fine on a read grant; writing it is not.
    let read = ro.request(json!({"type": "get_config"})).await;
    assert_eq!(read["type"], "config", "{read}");
    let denied = ro
        .request(json!({"type": "set_config", "auto_compact_window": 300_000}))
        .await;
    assert_eq!(denied["type"], "error", "{denied}");
    assert_eq!(denied["code"], "forbidden");
}
