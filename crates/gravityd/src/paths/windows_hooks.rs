//! Serialize hook payloads with PowerShell so Windows pipe paths remain valid JSON.
use std::path::Path;

pub fn settings(workspace: &Path, port: u16, token_env: &str) -> anyhow::Result<serde_json::Value> {
    let script = workspace.join(".claude/gravity-hook.ps1");
    let body = format!(
        r#"param([string]$Event)
$ErrorActionPreference = 'Stop'
try {{
    if ($Event -eq 'PermissionRequest') {{
        # Waits for the owner's answer and prints the daemon's decision. On any
        # failure nothing is printed, so Claude Code shows its own prompt.
        $raw = [Console]::In.ReadToEnd()
        $response = Invoke-WebRequest -UseBasicParsing -Uri 'http://127.0.0.1:{port}/hook/permission' -Method Post -TimeoutSec {wait} -ContentType 'application/json; charset=utf-8' -Headers @{{ Authorization = "Bearer $env:{token_env}" }} -Body ([Text.Encoding]::UTF8.GetBytes($raw))
        if ($response.Content) {{ [Console]::Out.Write($response.Content) }}
        exit 0
    }}
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
"#,
        wait = crate::approval::HOOK_TIMEOUT_SECS - 10
    );
    std::fs::write(&script, body)?;
    let events = [
        "SessionStart",
        "UserPromptSubmit",
        "PostToolUse",
        "Stop",
        "Notification",
        "SessionEnd",
        "PermissionRequest",
    ];
    let mut hooks = serde_json::Map::new();
    for event in events {
        let mut hook = serde_json::json!({
            "type": "command",
            "command": format!("powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"{}\" {event}", script.display())
        });
        if event == "PermissionRequest" {
            hook["timeout"] = serde_json::json!(crate::approval::HOOK_TIMEOUT_SECS);
        }
        hooks.insert(event.to_string(), serde_json::json!([{ "hooks": [hook] }]));
    }
    Ok(serde_json::json!({
        "crossSessionInbound": "accept",
        "permissions": { "allow": [], "deny": ["Read(../**)", "Read(~/.gravity/secrets/**)", "Bash(rm -rf /*)"] },
        "hooks": hooks
    }))
}
