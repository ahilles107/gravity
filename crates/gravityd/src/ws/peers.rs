//! Peer requests: pairing with another daemon, and linking its bots into a
//! project here. See `docs/peer-bots.md`.

use std::net::SocketAddr;
use std::sync::Arc;

use bus::RemoteBot;
use serde_json::{json, Value};

use crate::app::AppState;
use crate::events::Push;

use super::Conn;

impl Conn {
    pub(super) fn list_peers(&self, req_id: &Value) -> anyhow::Result<()> {
        let peers: Vec<Value> = self
            .app
            .db
            .list_peers()?
            .into_iter()
            .map(|peer| peer_view(&self.app, &peer))
            .collect();
        self.send(json!({ "type": "peers", "req_id": req_id, "peers": peers }));
        Ok(())
    }

    /// Pairing, listening side: registers the machine that will dial in and
    /// returns the code to paste into it. The code holds the token, so it is
    /// shown once, like a device token.
    pub(super) fn create_peer_invite(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let name = peer_name(&self.app, Self::str_field(req, "name")?)?;
        let url = match req.get("url").and_then(Value::as_str) {
            Some(url) => url.to_string(),
            None => listening_url(&self.app)?,
        };
        let peer = self.app.db.create_peer(&name, None)?;
        let token = self.app.secrets.issue_peer_token(&peer.id)?;
        self.send(json!({
            "type": "peer", "req_id": req_id,
            "peer": peer_view(&self.app, &peer),
            "invite": format!("{url}#{token}")
        }));
        Ok(())
    }

    /// Pairing, dialing side: stores the other daemon's invite and starts
    /// keeping a link open to it.
    pub(super) fn add_peer(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let name = peer_name(&self.app, Self::str_field(req, "name")?)?;
        let invite = Self::str_field(req, "invite")?.trim();
        let (url, token) = invite
            .rsplit_once('#')
            .filter(|(url, token)| {
                (url.starts_with("ws://") || url.starts_with("wss://")) && !token.is_empty()
            })
            .ok_or_else(|| {
                anyhow::anyhow!("not a peer invite: expected ws://host:port/peer#token")
            })?;
        let peer = self.app.db.create_peer(&name, Some(url))?;
        self.app.secrets.store_peer_token(&peer.id, token)?;
        crate::peer::spawn_dialer(self.app.clone(), peer.id.clone());
        self.send(json!({ "type": "peer", "req_id": req_id, "peer": peer_view(&self.app, &peer) }));
        Ok(())
    }

    pub(super) fn revoke_peer(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let peer_id = Self::str_field(req, "peer_id")?;
        if !self.app.db.revoke_peer(peer_id)? {
            self.reply_err(req_id, "not_found", "peer not found or already revoked");
            return Ok(());
        }
        self.app.secrets.remove_peer_token(peer_id)?;
        self.app.peers.disconnect(peer_id);
        let peer = self
            .app
            .db
            .get_peer(peer_id)?
            .ok_or_else(|| anyhow::anyhow!("peer vanished"))?;
        self.send(json!({ "type": "peer", "req_id": req_id, "peer": peer_view(&self.app, &peer) }));
        Ok(())
    }

    /// The bots running on a peer, for choosing one to link.
    pub(super) fn list_peer_bots(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let peer_id = Self::str_field(req, "peer_id")?.to_string();
        let (app, out, req_id) = (self.app.clone(), self.out.clone(), req_id.clone());
        tokio::spawn(async move {
            let reply = match app
                .peers
                .request(&peer_id, json!({ "type": "list_bots" }))
                .await
            {
                Ok(result) => json!({
                    "type": "peer_bots", "req_id": req_id, "peer_id": peer_id,
                    "bots": result.get("bots").cloned().unwrap_or_else(|| json!([]))
                }),
                Err(e) => error(&req_id, "unavailable", &e.to_string()),
            };
            let _ = out.send(reply);
        });
        Ok(())
    }

    /// Stands a peer's bot in as a linked bot in a project here.
    pub(super) fn link_peer_bot(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let peer_id = Self::str_field(req, "peer_id")?.to_string();
        let remote_bot_id = Self::str_field(req, "remote_bot_id")?.to_string();
        let project_id = Self::str_field(req, "project_id")?.to_string();
        let (app, out, req_id) = (self.app.clone(), self.out.clone(), req_id.clone());
        tokio::spawn(async move {
            let reply = match link(&app, &peer_id, &remote_bot_id, &project_id).await {
                Ok(bot) => json!({
                    "type": "bot", "req_id": req_id, "bot": super::views::bot_view(&app, &bot)
                }),
                Err(e) => error(&req_id, "conflict", &format!("{e:#}")),
            };
            let _ = out.send(reply);
        });
        Ok(())
    }
}

async fn link(
    app: &Arc<AppState>,
    peer_id: &str,
    remote_bot_id: &str,
    project_id: &str,
) -> anyhow::Result<bus::Bot> {
    let project = app
        .db
        .get_project(project_id)?
        .filter(|p| p.deleted_at.is_none())
        .ok_or_else(|| anyhow::anyhow!("project not found"))?;
    if let Some(bot) = app.db.linked_bot(peer_id, remote_bot_id, &project.id)? {
        return Ok(bot);
    }
    let result = app
        .peers
        .request(peer_id, json!({ "type": "link", "bot_id": remote_bot_id }))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let remote: RemoteBot = serde_json::from_value(result["bot"].clone())?;
    // Bots address each other by name, so a clash would make one of the two
    // unreachable. The owner renames one and links again.
    anyhow::ensure!(
        app.db.get_bot_by_name(&project.id, &remote.name)?.is_none(),
        "a bot named '{}' already exists in this project; rename it first",
        remote.name
    );
    let bot = app.db.create_linked_bot(&project.id, &remote, peer_id)?;
    app.events.push(Push::BotUpdated { bot: bot.clone() });
    Ok(bot)
}

fn peer_name(app: &AppState, raw: &str) -> anyhow::Result<String> {
    let name = bus::names::validate_project(raw).map_err(anyhow::Error::msg)?;
    anyhow::ensure!(
        app.db.get_peer_by_name(&name)?.is_none(),
        "a peer named '{name}' already exists"
    );
    Ok(name)
}

/// The URL another daemon dials to reach this one: its first non-loopback
/// bind address, which is the Tailscale interface in the documented setup.
fn listening_url(app: &AppState) -> anyhow::Result<String> {
    let ip = app
        .cfg
        .bind
        .iter()
        .find(|ip| !ip.is_loopback())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "this daemon only listens on localhost; add its Tailscale address to \
                 `bind` in gravityd.toml and restart it, or pass the url to dial"
            )
        })?;
    Ok(format!("ws://{}/peer", SocketAddr::new(*ip, app.cfg.port)))
}

pub(crate) fn peer_view(app: &AppState, peer: &bus::Peer) -> Value {
    json!({
        "id": peer.id,
        "name": peer.name,
        "url": peer.url,
        "daemon_id": peer.daemon_id,
        "online": app.peers.is_online(&peer.id),
        "created_at": peer.created_at.to_rfc3339(),
        "last_seen_at": peer.last_seen_at.map(|t| t.to_rfc3339()),
        "revoked_at": peer.revoked_at.map(|t| t.to_rfc3339())
    })
}

fn error(req_id: &Value, code: &str, message: &str) -> Value {
    json!({ "type": "error", "req_id": req_id, "code": code, "message": message })
}
