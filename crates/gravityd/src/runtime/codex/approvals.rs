use super::worker::Worker;
use crate::runtime::{PermissionAnswer, SessionEvent};
use serde_json::{json, Value};

pub(super) struct Prompt {
    pub id: Value,
    pub method: String,
    pub params: Value,
}

/// Requests the owner can answer from a permission card.
fn is_approval(method: &str) -> bool {
    matches!(
        method,
        "item/commandExecution/requestApproval"
            | "item/fileChange/requestApproval"
            | "item/permissions/requestApproval"
    )
}

impl Worker {
    pub fn prompt(&mut self, id: Value, method: &str, params: Value) -> anyhow::Result<()> {
        let approval = is_approval(method);
        if self.native && !approval {
            // Questions and elicitations stay with the native terminal.
            self.hook(
                "Notification",
                Some("Codex needs your response in the terminal".into()),
            );
            return Ok(());
        }
        if !approval
            && !matches!(
                method,
                "item/tool/requestUserInput"
                    | "tool/requestUserInput"
                    | "mcpServer/elicitation/request"
            )
        {
            return self.write(&json!({ "id": id, "error": { "code": -32601, "message": "Gravity does not support this server request" } }));
        }
        self.prompt_serial += 1;
        let number = self.prompt_serial;
        if !self.native {
            let detail = serde_json::to_string_pretty(&params)?;
            self.output(&format!("\nRequest #{number}:\n{detail}\nUse /approve {number} or /deny {number}. For questions use /answer {number} <JSON response>.\n"));
        }
        self.hook(
            "Notification",
            Some(format!("Codex request #{number} needs your response")),
        );
        if approval {
            let (tool, input) = self.permission_view(method, &params);
            let _ = self.events.send(SessionEvent::Permission {
                key: number,
                tool,
                input,
            });
        }
        self.prompts.insert(
            number,
            Prompt {
                id,
                method: method.to_string(),
                params,
            },
        );
        Ok(())
    }

    /// How an approval reads on the owner's card.
    fn permission_view(&self, method: &str, params: &Value) -> (String, Value) {
        let reason = params.get("reason").cloned().unwrap_or(Value::Null);
        match method {
            "item/commandExecution/requestApproval" => (
                "Bash".to_string(),
                json!({ "command": params["command"], "cwd": params["cwd"], "reason": reason }),
            ),
            "item/fileChange/requestApproval" => {
                let files = params["itemId"]
                    .as_str()
                    .and_then(|item| self.file_changes.get(item))
                    .cloned()
                    .unwrap_or_default();
                let mut input =
                    json!({ "files": files, "reason": reason, "grantRoot": params["grantRoot"] });
                if let [only] = files.as_slice() {
                    input["file_path"] = json!(only);
                }
                ("Edit".to_string(), input)
            }
            _ => (
                "Permissions".to_string(),
                json!({ "reason": reason, "permissions": params["permissions"] }),
            ),
        }
    }

    /// The owner's answer from a card, sent back to the App Server.
    pub fn answer_card(&mut self, number: u64, answer: PermissionAnswer) -> anyhow::Result<()> {
        // Answered already, in the terminal or by a command.
        let Some(prompt) = self.prompts.remove(&number) else {
            return Ok(());
        };
        let result = match prompt.method.as_str() {
            "item/permissions/requestApproval" => json!({
                "permissions": if answer == PermissionAnswer::Deny { json!({}) } else { prompt.params.get("permissions").cloned().unwrap_or(json!({})) },
                "scope": if answer == PermissionAnswer::Session { "session" } else { "turn" }
            }),
            _ => json!({ "decision": match answer {
                PermissionAnswer::Once => "accept",
                PermissionAnswer::Session => "acceptForSession",
                PermissionAnswer::Deny => "decline",
            } }),
        };
        self.write(&json!({ "id": prompt.id, "result": result }))?;
        self.hook("PostToolUse", None);
        Ok(())
    }

    /// Requests the App Server says were resolved elsewhere: their cards go.
    pub fn resolved(&mut self, request: Option<&Value>) {
        let gone: Vec<u64> = self
            .prompts
            .iter()
            .filter(|(_, prompt)| Some(&prompt.id) == request)
            .map(|(number, _)| *number)
            .collect();
        for number in gone {
            self.prompts.remove(&number);
            let _ = self
                .events
                .send(SessionEvent::PermissionGone { key: number });
        }
    }

    pub fn answer(&mut self, text: &str) -> anyhow::Result<bool> {
        let mut parts = text.splitn(3, ' ');
        let command = parts.next().unwrap_or_default();
        if !matches!(command, "/approve" | "/deny" | "/answer") {
            return Ok(false);
        }
        let number: u64 = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("include the request number"))?
            .parse()?;
        let prompt = self
            .prompts
            .get(&number)
            .ok_or_else(|| anyhow::anyhow!("request #{number} is no longer pending"))?;
        let result = if command == "/answer" {
            let response: Value = serde_json::from_str(
                parts
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("include the JSON response"))?,
            )?;
            // Only question/elicitation answers accept structured payloads.
            anyhow::ensure!(
                matches!(
                    prompt.method.as_str(),
                    "item/tool/requestUserInput"
                        | "tool/requestUserInput"
                        | "mcpServer/elicitation/request"
                ),
                "use /approve or /deny for this request"
            );
            validate_answer(&prompt.method, &response)?;
            response
        } else {
            let accept = command == "/approve";
            match prompt.method.as_str() {
                "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
                    json!({ "decision": if accept { "accept" } else { "decline" } })
                }
                "item/permissions/requestApproval" => {
                    json!({ "permissions": if accept { prompt.params.get("permissions").cloned().unwrap_or(json!({})) } else { json!({}) }, "scope": "turn" })
                }
                "mcpServer/elicitation/request" if !accept => {
                    json!({ "action": "decline", "content": null })
                }
                _ => anyhow::bail!("answer the questions with /answer {number} <JSON response>"),
            }
        };
        self.write(&json!({ "id": prompt.id, "result": result }))?;
        self.prompts.remove(&number);
        // Answered by command: withdraw the card that asked the same thing.
        let _ = self
            .events
            .send(SessionEvent::PermissionGone { key: number });
        self.hook("PostToolUse", None);
        Ok(true)
    }
}

fn validate_answer(method: &str, response: &Value) -> anyhow::Result<()> {
    if method == "mcpServer/elicitation/request" {
        anyhow::ensure!(
            matches!(
                response.get("action").and_then(Value::as_str),
                Some("accept" | "decline" | "cancel")
            ),
            "answer needs action: accept, decline or cancel"
        );
        anyhow::ensure!(
            response
                .get("content")
                .is_some_and(|v| v.is_null() || v.is_object()),
            "answer needs content: an object or null"
        );
    } else {
        let answers = response
            .get("answers")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                anyhow::anyhow!("answer needs an answers object keyed by question ID")
            })?;
        anyhow::ensure!(
            answers.values().all(|answer| answer
                .get("answers")
                .and_then(Value::as_array)
                .is_some_and(|items| items.iter().all(Value::is_string))),
            "each question answer needs an answers array of strings"
        );
    }
    Ok(())
}
