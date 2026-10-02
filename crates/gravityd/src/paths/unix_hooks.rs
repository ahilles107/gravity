//! Claude Code hook settings on Unix: lifecycle events and permission prompts
//! posted to the daemon with curl.

/// Cooperative permission rules and lifecycle hooks. Hooks POST lifecycle
/// events to the daemon; failures are swallowed (`|| true`) so a daemon
/// hiccup never blocks the session, and hook failure is never interpreted as
/// approval or denial.
pub fn settings(daemon_port: u16, bot_token_env: &str) -> serde_json::Value {
    let hook_cmd = |event: &str| {
        serde_json::json!([{
            "hooks": [{
                "type": "command",
                "command": format!(
                    "curl -fsS -m 3 -X POST http://127.0.0.1:{daemon_port}/hook \
                     -H \"Authorization: Bearer ${{{bot_token_env}}}\" \
                     -H 'Content-Type: application/json' \
                     -d '{{\"event\":\"{event}\"}}' >/dev/null 2>&1 || true"
                )
            }]
        }])
    };
    // Notification and Stop carry Claude Code's stdin payload through
    // untouched: Notification so the daemon can read its `message` and tell a
    // permission prompt apart from the "waiting for your input" idle ping,
    // Stop so it learns the `transcript_path` a finished routine run is read
    // from. The event name rides in the query string because the body is no
    // longer ours to shape.
    let forwarding = |event: &str| {
        serde_json::json!([{
            "hooks": [{
                "type": "command",
                "command": format!(
                    "curl -fsS -m 3 -X POST \
                     'http://127.0.0.1:{daemon_port}/hook?event={event}' \
                     -H \"Authorization: Bearer ${{{bot_token_env}}}\" \
                     -H 'Content-Type: application/json' --data-binary @- \
                     >/dev/null 2>&1 || true"
                )
            }]
        }])
    };
    // PermissionRequest waits for the owner's answer from the app and prints
    // the daemon's decision for Claude Code to apply. Anything going wrong
    // prints nothing, which leaves Claude Code's own prompt in the terminal:
    // a failed hook never allows anything.
    let permission = serde_json::json!([{
        "hooks": [{
            "type": "command",
            "timeout": crate::approval::HOOK_TIMEOUT_SECS,
            "command": format!(
                "curl -fsS -m {} -X POST 'http://127.0.0.1:{daemon_port}/hook/permission' \
                 -H \"Authorization: Bearer ${{{bot_token_env}}}\" \
                 -H 'Content-Type: application/json' --data-binary @- 2>/dev/null || true",
                crate::approval::HOOK_TIMEOUT_SECS - 10
            )
        }]
    }]);
    // SessionStart additionally reports the session's inbox socket (and its
    // messaging token) so the daemon can deliver bus messages through it
    // instead of the terminal.
    let session_start = serde_json::json!([{
        "hooks": [{
            "type": "command",
            "command": format!(
                "curl -fsS -m 3 -X POST http://127.0.0.1:{daemon_port}/hook \
                 -H \"Authorization: Bearer ${{{bot_token_env}}}\" \
                 -H 'Content-Type: application/json' \
                 -d \"{{\\\"event\\\":\\\"SessionStart\\\",\\\"socket\\\":\\\"$CLAUDE_CODE_MESSAGING_SOCKET\\\",\\\"msg_token\\\":\\\"$CLAUDE_CODE_MESSAGING_TOKEN\\\"}}\" \
                 >/dev/null 2>&1 || true"
            )
        }]
    }]);
    serde_json::json!({
        // Bus deliveries arrive over the cross-session inbox socket; accept
        // them unattended so bot-to-bot traffic flows without approval stops.
        "crossSessionInbound": "accept",
        // Artifacts access is granted at spawn time (`artifacts_allow_rules`
        // via `--allowedTools`), never here: allow rules in a folder's
        // settings.json make Claude Code's trust dialog warn about
        // pre-approved permissions.
        "permissions": {
            "allow": [],
            "deny": [
                "Read(../**)",
                "Read(~/.gravity/secrets/**)",
                "Bash(rm -rf /*)"
            ]
        },
        "hooks": {
            "SessionStart": session_start,
            "UserPromptSubmit": hook_cmd("UserPromptSubmit"),
            // A tool that has finished running is proof the session is
            // executing again: nothing else reports that a pending permission
            // prompt was answered.
            "PostToolUse": hook_cmd("PostToolUse"),
            "Stop": forwarding("Stop"),
            "Notification": forwarding("Notification"),
            "PermissionRequest": permission,
            "SessionEnd": hook_cmd("SessionEnd")
        }
    })
}
