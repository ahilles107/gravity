//! Authenticated Windows inbox for the deterministic runtime.
use anyhow::Context;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::windows::named_pipe::ServerOptions;
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

use super::SessionEvent;
use crate::channel::MsgSocket;

pub fn start(tx: mpsc::UnboundedSender<SessionEvent>) -> anyhow::Result<(MsgSocket, AbortHandle)> {
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    // Deliveries are synchronous: keep the receiver off the daemon's Tokio
    // thread so a full pipe cannot block the receiver that would drain it.
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        match runtime {
            Ok(runtime) => runtime.block_on(async {
                match spawn_inbox(tx) {
                    Ok((socket, task)) => {
                        if ready_tx.send(Ok((socket, task.abort_handle()))).is_err() {
                            task.abort();
                        }
                        let _ = task.await;
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                    }
                }
            }),
            Err(error) => {
                let _ = ready_tx.send(Err(error.into()));
            }
        }
    });
    ready_rx.recv().context("Windows inbox worker stopped")?
}

fn spawn_inbox(
    tx: mpsc::UnboundedSender<SessionEvent>,
) -> anyhow::Result<(MsgSocket, tokio::task::JoinHandle<()>)> {
    let path = format!(r"\\.\pipe\gravity-double-{}", uuid::Uuid::new_v4());
    let token = uuid::Uuid::new_v4().to_string();
    let server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(&path)?;
    let socket = MsgSocket {
        path: path.clone().into(),
        token: Some(token.clone()),
    };
    let task = tokio::spawn(async move {
        let mut server = server;
        loop {
            if server.connect().await.is_err() {
                break;
            }
            let next = match ServerOptions::new().create(&path) {
                Ok(next) => next,
                Err(_) => break,
            };
            let mut lines = BufReader::new(server).lines();
            let read = async {
                let auth = lines.next_line().await?.context("missing auth")?;
                let auth: serde_json::Value = serde_json::from_str(&auth)?;
                anyhow::ensure!(
                    auth["type"] == "auth" && auth["token"].as_str() == Some(&token),
                    "invalid auth"
                );
                while let Some(line) = lines.next_line().await? {
                    let value: serde_json::Value = serde_json::from_str(&line)?;
                    if value["type"] == "user" {
                        if let Some(content) =
                            value.pointer("/message/content").and_then(|c| c.as_str())
                        {
                            let _ = tx
                                .send(SessionEvent::Output(format!("{content}\r\n").into_bytes()));
                        }
                    }
                }
                Ok::<(), anyhow::Error>(())
            };
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), read).await;
            server = next;
        }
    });
    Ok((socket, task))
}
