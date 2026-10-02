//! `gravityd peer …`: pairing and linking from a terminal. The commands talk
//! to the running daemon over its control plane with the owner token, so they
//! work while it serves, and they are exactly what the app will send.

use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

use crate::config::Config;

const USAGE: &str = "usage:
  gravityd peer invite <name> [--url ws://host:port/peer]
  gravityd peer add <name> <invite>
  gravityd peer list
  gravityd peer bots <peer>
  gravityd peer link <peer> <bot> --project <project>
  gravityd peer revoke <peer>";

pub async fn run(cfg: &Config, args: &[String]) -> anyhow::Result<()> {
    let arg = |i: usize| args.get(i).map(String::as_str);
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
    };
    let mut daemon = Daemon::connect(cfg).await?;
    match (arg(0), arg(1), arg(2)) {
        (Some("invite"), Some(name), _) => {
            let mut req = json!({ "type": "create_peer_invite", "name": name });
            if let Some(url) = flag("--url") {
                req["url"] = json!(url);
            }
            let reply = daemon.request(req).await?;
            println!("Invite for {name} (shown once):\n");
            println!("{}", reply["invite"].as_str().unwrap_or_default());
            println!("\nOn {name}, run: gravityd peer add <name for this machine> <invite>");
        }
        (Some("add"), Some(name), Some(invite)) => {
            daemon
                .request(json!({ "type": "add_peer", "name": name, "invite": invite }))
                .await?;
            println!("Added {name}. This daemon now keeps a link open to it.");
        }
        (Some("list"), _, _) => {
            for peer in daemon.peers().await? {
                let state = if peer["revoked_at"].is_string() {
                    "revoked"
                } else if peer["online"].as_bool() == Some(true) {
                    "online"
                } else {
                    "offline"
                };
                let dials = peer["url"].as_str().map(|u| format!("  dials {u}"));
                println!(
                    "{}  {state}{}",
                    peer["name"].as_str().unwrap_or_default(),
                    dials.unwrap_or_default()
                );
            }
        }
        (Some("bots"), Some(peer), _) => {
            let peer = daemon.peer(peer).await?;
            for bot in daemon.peer_bots(&peer).await? {
                println!(
                    "{}  ({})  {}",
                    bot["name"].as_str().unwrap_or_default(),
                    bot["project"].as_str().unwrap_or_default(),
                    bot["description"].as_str().unwrap_or_default()
                );
            }
        }
        (Some("link"), Some(peer), Some(bot)) => {
            let project_name = flag("--project")
                .ok_or_else(|| anyhow::anyhow!("link needs --project <project>"))?;
            let peer = daemon.peer(peer).await?;
            let remote = daemon
                .peer_bots(&peer)
                .await?
                .into_iter()
                .find(|b| {
                    b["name"]
                        .as_str()
                        .is_some_and(|n| n.eq_ignore_ascii_case(bot))
                })
                .ok_or_else(|| anyhow::anyhow!("no bot named '{bot}' on that peer"))?;
            let projects = daemon.request(json!({ "type": "list_projects" })).await?;
            let project = projects["projects"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|p| {
                    p["name"]
                        .as_str()
                        .is_some_and(|n| n.eq_ignore_ascii_case(project_name))
                })
                .ok_or_else(|| anyhow::anyhow!("no project named '{project_name}'"))?;
            daemon
                .request(json!({
                    "type": "link_peer_bot",
                    "peer_id": peer,
                    "remote_bot_id": remote["id"],
                    "project_id": project["id"],
                }))
                .await?;
            println!("Linked {bot} into {project_name}. Bots there can message it by name.");
        }
        (Some("revoke"), Some(peer), _) => {
            let peer = daemon.peer(peer).await?;
            daemon
                .request(json!({ "type": "revoke_peer", "peer_id": peer }))
                .await?;
            println!("Revoked. Its token no longer works on this daemon.");
        }
        _ => anyhow::bail!("{USAGE}"),
    }
    Ok(())
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

struct Daemon {
    socket: Socket,
    next: u64,
}

impl Daemon {
    async fn connect(cfg: &Config) -> anyhow::Result<Self> {
        let port = crate::home::runtime_port(&cfg.home).unwrap_or(cfg.port);
        let token = std::fs::read_to_string(cfg.secrets_dir().join("client.token"))
            .map_err(|e| anyhow::anyhow!("cannot read the owner token: {e}"))?;
        let url = format!("ws://127.0.0.1:{port}/ws");
        let (socket, _) = tokio_tungstenite::connect_async(url.as_str())
            .await
            .map_err(|e| anyhow::anyhow!("gravityd is not answering on {url}: {e}"))?;
        let mut daemon = Self { socket, next: 1 };
        daemon
            .request(json!({
                "type": "hello",
                "protocol_version": crate::app::PROTOCOL_VERSION,
                "token": token.trim(),
                "client": "gravityd-cli"
            }))
            .await?;
        Ok(daemon)
    }

    /// Sends a request and returns its reply, skipping pushes in between.
    async fn request(&mut self, mut req: Value) -> anyhow::Result<Value> {
        let req_id = self.next;
        self.next += 1;
        req["req_id"] = json!(req_id);
        self.socket.send(Message::Text(req.to_string())).await?;
        while let Some(frame) = self.socket.next().await {
            let Message::Text(text) = frame? else {
                continue;
            };
            let reply: Value = serde_json::from_str(&text)?;
            if reply["req_id"].as_u64() != Some(req_id) {
                continue;
            }
            if reply["type"] == "error" {
                anyhow::bail!("{}", reply["message"].as_str().unwrap_or("request failed"));
            }
            return Ok(reply);
        }
        anyhow::bail!("gravityd closed the connection")
    }

    async fn peers(&mut self) -> anyhow::Result<Vec<Value>> {
        let reply = self.request(json!({ "type": "list_peers" })).await?;
        Ok(reply["peers"].as_array().cloned().unwrap_or_default())
    }

    /// The id of the live peer with this name.
    async fn peer(&mut self, name: &str) -> anyhow::Result<String> {
        self.peers()
            .await?
            .into_iter()
            .find(|p| {
                p["name"]
                    .as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
                    && !p["revoked_at"].is_string()
            })
            .and_then(|p| p["id"].as_str().map(str::to_string))
            .ok_or_else(|| anyhow::anyhow!("no peer named '{name}'"))
    }

    async fn peer_bots(&mut self, peer_id: &str) -> anyhow::Result<Vec<Value>> {
        let reply = self
            .request(json!({ "type": "list_peer_bots", "peer_id": peer_id }))
            .await?;
        Ok(reply["bots"].as_array().cloned().unwrap_or_default())
    }
}
