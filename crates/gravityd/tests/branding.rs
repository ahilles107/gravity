use gravityd::paths::write_mcp_config;

#[test]
fn mcp_config_uses_the_gravity_namespace() {
    let tmp = tempfile::tempdir().expect("tmp");
    write_mcp_config(tmp.path(), 7777, "GRAVITY_TOKEN").expect("write mcp config");
    let raw = std::fs::read_to_string(tmp.path().join("mcp.json")).expect("read mcp config");
    let mcp: serde_json::Value = serde_json::from_str(&raw).expect("parse mcp config");

    assert!(mcp["mcpServers"]["gravity-bus"].is_object());
    assert_eq!(
        mcp["mcpServers"]["gravity-bus"]["headers"]["Authorization"],
        "Bearer ${GRAVITY_TOKEN}"
    );
}
