//! Device-scoped credentials, revocation and origin checks.

mod common;

use common::*;
use futures::StreamExt;
use serde_json::json;

#[tokio::test]
async fn device_scoped_credentials_and_revocation() {
    let d = spawn_daemon().await;
    let mut owner = WsClient::connect(&d).await;

    // Owner issues a read-only device credential; token is returned once.
    let created = owner
        .request(json!({
            "type": "create_device", "name": "laptop", "capabilities": ["read"]
        }))
        .await;
    assert_eq!(created["type"], "device", "{created}");
    let device_id = created["device"]["id"].as_str().expect("id").to_string();
    let token = created["token"].as_str().expect("token").to_string();

    // The device connects with scoped grants.
    let hello = raw_hello(&d, &token).await;
    assert_eq!(hello["type"], "hello_ok", "{hello}");
    assert_eq!(hello["grants"], json!(["read"]));
    assert_eq!(hello["device_id"], json!(device_id));

    // Read works, control is forbidden.
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
    let listed = ro.request(json!({"type": "list_projects"})).await;
    assert_eq!(listed["type"], "projects");
    let denied = ro
        .request(json!({"type": "create_project", "name": "nope"}))
        .await;
    assert_eq!(denied["type"], "error", "{denied}");
    assert_eq!(denied["code"], "forbidden");

    // Revocation: the device cannot reconnect; list shows it revoked.
    let revoked = owner
        .request(json!({"type": "revoke_device", "device_id": device_id}))
        .await;
    assert_eq!(revoked["type"], "device");
    assert!(revoked["device"]["revoked_at"].is_string());

    let hello = raw_hello(&d, token_str(&created)).await;
    assert_eq!(hello["type"], "error");
    assert_eq!(hello["code"], "auth_failed");

    let devices = owner.request(json!({"type": "list_devices"})).await;
    assert_eq!(devices["devices"].as_array().expect("arr").len(), 1);
}

#[tokio::test]
async fn ws_rejects_disallowed_origin() {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let d = spawn_daemon().await;
    let url = format!("ws://{}/ws", d.addr);
    let mut req = url.clone().into_client_request().expect("request");
    req.headers_mut().insert(
        "origin",
        "https://evil.example.com".parse().expect("header"),
    );
    let err = tokio_tungstenite::connect_async(req).await;
    assert!(err.is_err(), "connection with bad origin must be refused");

    // Localhost origins stay allowed.
    let mut req = url.into_client_request().expect("request");
    req.headers_mut()
        .insert("origin", "http://localhost:1420".parse().expect("header"));
    assert!(tokio_tungstenite::connect_async(req).await.is_ok());
}
