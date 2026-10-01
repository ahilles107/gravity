//! Serialize hook payloads with PowerShell so Windows pipe paths remain valid JSON.
use std::path::Path;

pub fn settings(workspace: &Path, port: u16, token_env: &str) -> anyhow::Result<serde_json::Value> {
    let script = workspace.join(".claude/gravity-hook.ps1");
    let body = format!(
        r#"param([string]$Event)
$ErrorActionPreference = 'Stop'
try {{
    $payload = @{{ event = $Event }}
    if ($Event -eq 'SessionStart') {{
        $payload.socket = $env:CLAUDE_CODE_MESSAGING_SOCKET
        $payload.msg_token = $env:CLAUDE_CODE_MESSAGING_TOKEN
    }}
    if ($Event -eq 'Notification' -or $Event -eq 'Stop') {{
        $inputJson = [Console]::In.ReadToEnd() | ConvertFrom-Json
        $payload.message = $inputJson.message
        $payload.transcript_path = $inputJson.transcript_path
    }}
    $json = $payload | ConvertTo-Json -Compress
    Invoke-RestMethod -Uri 'http://127.0.0.1:{port}/hook' -Method Post -TimeoutSec 3 -ContentType 'application/json; charset=utf-8' -Headers @{{ Authorization = "Bearer $env:{token_env}" }} -Body ([Text.Encoding]::UTF8.GetBytes($json)) | Out-Null
}} catch {{ }}
exit 0
"#
    );
    std::fs::write(&script, body)?;
    let events = [
        "SessionStart",
        "UserPromptSubmit",
        "PostToolUse",
        "Stop",
        "Notification",
        "SessionEnd",
    ];
    let mut hooks = serde_json::Map::new();
    for event in events {
        hooks.insert(event.to_string(), serde_json::json!([{ "hooks": [{
            "type": "command",
            "command": format!("powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"{}\" {event}", script.display())
        }] }]));
    }
    Ok(serde_json::json!({
        "crossSessionInbound": "accept",
        "permissions": { "allow": [], "deny": ["Read(../**)", "Read(~/.gravity/secrets/**)", "Bash(rm -rf /*)"] },
        "hooks": hooks
    }))
}
