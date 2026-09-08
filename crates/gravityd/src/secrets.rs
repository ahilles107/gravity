//! Token management: one client token for the control plane, one scoped token
//! per bot for the MCP bridge. Files live under `~/.gravity/secrets` with
//! mode 0700/0600 and are never exported.

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Context;
use rand::RngCore;

pub struct Secrets {
    dir: PathBuf,
    /// token -> bot_id, loaded at startup and kept in sync on issue/rotate.
    bot_tokens: Mutex<HashMap<String, String>>,
    /// token -> device_id for device-scoped client credentials.
    device_tokens: Mutex<HashMap<String, String>>,
    client_token: String,
}

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn write_secret(path: &Path, value: &str) -> anyhow::Result<()> {
    fs::write(path, value)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

impl Secrets {
    pub fn open(dir: &Path) -> anyhow::Result<Self> {
        fs::create_dir_all(dir)?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;

        let client_path = dir.join("client.token");
        let client_token = if client_path.exists() {
            fs::read_to_string(&client_path)?.trim().to_string()
        } else {
            let t = random_token();
            write_secret(&client_path, &t).context("writing client token")?;
            t
        };

        let mut bot_tokens = HashMap::new();
        let mut device_tokens = HashMap::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(bot_id) = name
                .strip_prefix("bot-")
                .and_then(|s| s.strip_suffix(".token"))
            {
                let token = fs::read_to_string(entry.path())?.trim().to_string();
                if !token.is_empty() {
                    bot_tokens.insert(token, bot_id.to_string());
                }
            } else if let Some(device_id) = name
                .strip_prefix("device-")
                .and_then(|s| s.strip_suffix(".token"))
            {
                let token = fs::read_to_string(entry.path())?.trim().to_string();
                if !token.is_empty() {
                    device_tokens.insert(token, device_id.to_string());
                }
            }
        }

        Ok(Self {
            dir: dir.to_path_buf(),
            bot_tokens: Mutex::new(bot_tokens),
            device_tokens: Mutex::new(device_tokens),
            client_token,
        })
    }

    /// Issue (or replace) the credential for a device. The token is returned
    /// once; only its file persists.
    pub fn issue_device_token(&self, device_id: &str) -> anyhow::Result<String> {
        let mut map = self.device_tokens.lock().unwrap_or_else(|e| e.into_inner());
        map.retain(|_, id| id.as_str() != device_id);
        let token = random_token();
        write_secret(&self.dir.join(format!("device-{device_id}.token")), &token)?;
        map.insert(token.clone(), device_id.to_string());
        Ok(token)
    }

    pub fn device_for_token(&self, token: &str) -> Option<String> {
        let map = self.device_tokens.lock().unwrap_or_else(|e| e.into_inner());
        map.iter()
            .find(|(t, _)| constant_time_eq(t.as_bytes(), token.as_bytes()))
            .map(|(_, id)| id.clone())
    }

    /// Delete a revoked device's credential so it can never reconnect.
    pub fn remove_device_token(&self, device_id: &str) -> anyhow::Result<()> {
        let mut map = self.device_tokens.lock().unwrap_or_else(|e| e.into_inner());
        map.retain(|_, id| id.as_str() != device_id);
        let path = self.dir.join(format!("device-{device_id}.token"));
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn client_token(&self) -> &str {
        &self.client_token
    }

    pub fn verify_client(&self, token: &str) -> bool {
        constant_time_eq(token.as_bytes(), self.client_token.as_bytes())
    }

    /// Existing or newly issued token for a bot.
    pub fn bot_token(&self, bot_id: &str) -> anyhow::Result<String> {
        let mut map = self.bot_tokens.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((token, _)) = map.iter().find(|(_, id)| id.as_str() == bot_id) {
            return Ok(token.clone());
        }
        let token = random_token();
        let path = self.dir.join(format!("bot-{bot_id}.token"));
        write_secret(&path, &token)?;
        map.insert(token.clone(), bot_id.to_string());
        Ok(token)
    }

    /// bot_id for a presented bearer token, if valid.
    pub fn bot_for_token(&self, token: &str) -> Option<String> {
        let map = self.bot_tokens.lock().unwrap_or_else(|e| e.into_inner());
        map.iter()
            .find(|(t, _)| constant_time_eq(t.as_bytes(), token.as_bytes()))
            .map(|(_, id)| id.clone())
    }

    /// Delete a bot's credential so an archived bot can never authenticate
    /// again. Without this the token stays valid in memory and on disk, and a
    /// deleted bot keeps full access to `/mcp` and `/hook`.
    pub fn remove_bot_token(&self, bot_id: &str) -> anyhow::Result<()> {
        let mut map = self.bot_tokens.lock().unwrap_or_else(|e| e.into_inner());
        map.retain(|_, id| id.as_str() != bot_id);
        let path = self.dir.join(format!("bot-{bot_id}.token"));
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn rotate_bot_token(&self, bot_id: &str) -> anyhow::Result<String> {
        let mut map = self.bot_tokens.lock().unwrap_or_else(|e| e.into_inner());
        map.retain(|_, id| id.as_str() != bot_id);
        let token = random_token();
        write_secret(&self.dir.join(format!("bot-{bot_id}.token")), &token)?;
        map.insert(token.clone(), bot_id.to_string());
        Ok(token)
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
