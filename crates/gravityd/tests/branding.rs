use gravityd::paths::write_mcp_config;

#[test]
fn mcp_config_uses_the_gravity_namespace() {
    let tmp = tempfile::tempdir().expect("tmp");
    write_mcp_config(tmp.path(), 7777, "GRAVITY_TOKEN", None).expect("write mcp config");
    let raw = std::fs::read_to_string(tmp.path().join("mcp.json")).expect("read mcp config");
    let mcp: serde_json::Value = serde_json::from_str(&raw).expect("parse mcp config");

    assert!(mcp["mcpServers"]["gravity-bus"].is_object());
    assert_eq!(
        mcp["mcpServers"]["gravity-bus"]["headers"]["Authorization"],
        "Bearer ${GRAVITY_TOKEN}"
    );
}

#[test]
fn mcp_config_carries_the_bots_own_browser() {
    let tmp = tempfile::tempdir().expect("tmp");
    let browser = serde_json::json!({
        "command": "/opt/node/bin/npx", "args": ["-y", "@playwright/mcp@0.0.83"],
        "env": { "PATH": "/opt/node/bin" }
    });
    write_mcp_config(tmp.path(), 7777, "GRAVITY_TOKEN", Some(&browser)).expect("write");
    let raw = std::fs::read_to_string(tmp.path().join("mcp.json")).expect("read mcp config");
    let mcp: serde_json::Value = serde_json::from_str(&raw).expect("parse mcp config");
    let server = &mcp["mcpServers"]["playwright"];
    assert_eq!(server["type"], "stdio");
    assert_eq!(server["command"], "/opt/node/bin/npx");
    assert!(mcp["mcpServers"]["gravity-bus"].is_object());
}
