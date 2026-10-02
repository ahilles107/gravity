//! A private, authenticated App Server shared by Gravity and the native TUI.
use super::Wire;
use anyhow::Context;
use futures::{SinkExt, StreamExt};
use rand::RngCore;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{BufWriter, Write};
use std::process::ChildStdin;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedSender;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

pub(super) struct Remote {
    pub address: String,
    pub token: String,
}

impl Remote {
    pub fn new() -> anyhow::Result<Self> {
        let reservation = std::net::TcpListener::bind("127.0.0.1:0")?;
        let address = format!("ws://{}", reservation.local_addr()?);
        let mut bytes = [0; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        Ok(Self {
            address,
            token: hex::encode(bytes),
        })
    }

    pub fn verifier(&self) -> String {
        hex::encode(Sha256::digest(self.token.as_bytes()))
    }
}

pub(super) enum RpcWriter {
    Stdio(BufWriter<ChildStdin>),
    Socket(UnboundedSender<String>),
}

impl RpcWriter {
    pub fn write(&mut self, value: &Value) -> anyhow::Result<()> {
        match self {
            Self::Stdio(input) => {
                serde_json::to_writer(&mut *input, value)?;
                input.write_all(b"\n")?;
                input.flush()?;
            }
            Self::Socket(input) => input
                .send(serde_json::to_string(value)?)
                .map_err(|_| anyhow::anyhow!("Codex connection closed"))?,
        }
        Ok(())
    }
}

pub(super) fn connect(
    remote: &Remote,
    tx: mpsc::Sender<Wire>,
    child: std::sync::Arc<std::sync::Mutex<std::process::Child>>,
) -> anyhow::Result<RpcWriter> {
    let mut request = remote.address.clone().into_client_request()?;
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {}", remote.token).parse()?);
    let (out, mut outgoing) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut run = || -> anyhow::Result<()> {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            runtime.block_on(async {
                let deadline = Instant::now() + Duration::from_secs(10);
                let socket = loop {
                    if child.lock().unwrap_or_else(|e| e.into_inner()).try_wait()?.is_some() {
                        anyhow::bail!("Codex App Server exited before accepting connections");
                    }
                    match tokio_tungstenite::connect_async(request.clone()).await {
                        Ok((socket, _)) => break socket,
                        Err(error) if Instant::now() >= deadline => return Err(error.into()),
                        Err(_) => tokio::time::sleep(Duration::from_millis(40)).await,
                    }
                };
                let (mut sink, mut stream) = socket.split();
                let _ = ready_tx.send(Ok(()));
                loop {
                    tokio::select! {
                        value = outgoing.recv() => {
                            let Some(value) = value else { break; };
                            sink.send(Message::Text(value)).await?;
                        }
                        frame = stream.next() => {
                            match frame {
                                Some(Ok(Message::Text(text))) => {
                                    if tx.send(Wire::Server(serde_json::from_str(&text)?)).is_err() { break; }
                                }
                                Some(Ok(Message::Close(_))) | None => break,
                                Some(Err(error)) => return Err(error.into()),
                                _ => {}
                            }
                        }
                    }
                }
                Ok(())
            })
        };
        if let Err(error) = run() {
            let _ = ready_tx.send(Err(error));
        }
        let _ = tx.send(Wire::Closed);
    });
    ready_rx
        .recv_timeout(Duration::from_secs(12))
        .context("connect to Codex App Server")??;
    Ok(RpcWriter::Socket(out))
}
