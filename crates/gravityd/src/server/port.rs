//! Reserving the daemon's port: the collision policy, the negotiated
//! fallback, and the watch that takes the configured port back.

use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

#[cfg(test)]
mod tests;

const PORT_NEGOTIATION_ATTEMPTS: usize = 10;
/// How long whatever owns the preferred port gets to answer `/health`.
const OCCUPANT_PROBE_TIMEOUT: Duration = Duration::from_millis(500);
/// How long the configured port gets to clear before a fallback is negotiated.
const PORT_GRACE: Duration = Duration::from_secs(5);
const PORT_GRACE_INTERVAL: Duration = Duration::from_millis(250);
/// How often a daemon on a fallback port checks whether it can go home.
pub(super) const PORT_RECLAIM_INTERVAL: Duration = Duration::from_secs(30);

/// What a collision on the configured port means for this launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortPolicy {
    /// Fail, naming the port. The default, and the only behaviour a direct
    /// launch or a remotely reachable daemon ever gets.
    Strict,
    /// Fall back to an OS-assigned port, which the app rediscovers locally.
    Negotiate,
}

/// Listeners reserved before application startup, all serving on one port.
pub struct BoundServer {
    port: u16,
    pub(super) configured_port: u16,
    pub(super) listeners: Vec<(SocketAddr, tokio::net::TcpListener)>,
}

impl BoundServer {
    pub fn port(&self) -> u16 {
        self.port
    }

    /// True when the configured port was taken and a fallback was negotiated.
    pub fn negotiated(&self) -> bool {
        self.port != self.configured_port
    }
}

async fn bind_port(
    bind: &[IpAddr],
    port: u16,
    configured_port: u16,
) -> std::io::Result<BoundServer> {
    let mut listeners = Vec::with_capacity(bind.len());
    let mut selected_port = port;
    for ip in bind {
        let addr = SocketAddr::new(*ip, selected_port);
        let listener = tokio::net::TcpListener::bind(addr).await?;
        let actual_addr = listener.local_addr()?;
        selected_port = actual_addr.port();
        listeners.push((actual_addr, listener));
    }
    Ok(BoundServer {
        port: selected_port,
        configured_port,
        listeners,
    })
}

/// Whether this launch may negotiate its port away.
///
/// Negotiation is opt-in from the managed service (`--negotiate-port`), can be
/// switched off in `gravityd.toml`, and never applies to a daemon bound to
/// anything but loopback: a client on another machine is configured with a port
/// and has no way to learn that it moved.
pub fn port_policy(cfg: &crate::config::Config, requested: bool) -> PortPolicy {
    if !requested {
        return PortPolicy::Strict;
    }
    if !cfg.negotiate_port {
        tracing::info!("port negotiation disabled in gravityd.toml");
        return PortPolicy::Strict;
    }
    if let Some(addr) = cfg.bind.iter().find(|ip| !ip.is_loopback()) {
        tracing::info!(
            %addr,
            "reachable from other machines; keeping the configured port"
        );
        return PortPolicy::Strict;
    }
    PortPolicy::Negotiate
}

/// Reserves every configured address on the configured port, negotiating a
/// fallback only when [`PortPolicy::Negotiate`] allows it.
pub async fn bind(
    addresses: &[IpAddr],
    configured_port: u16,
    policy: PortPolicy,
) -> anyhow::Result<BoundServer> {
    bind_with_grace(addresses, configured_port, policy, PORT_GRACE).await
}

async fn bind_with_grace(
    addresses: &[IpAddr],
    configured_port: u16,
    policy: PortPolicy,
    grace: Duration,
) -> anyhow::Result<BoundServer> {
    anyhow::ensure!(!addresses.is_empty(), "no bind addresses configured");
    let error = match bind_port(addresses, configured_port, configured_port).await {
        Ok(bound) => return Ok(bound),
        Err(error) => error,
    };
    if policy == PortPolicy::Strict || error.kind() != std::io::ErrorKind::AddrInUse {
        return Err(anyhow::Error::new(error).context(format!(
            "binding port {configured_port}: stop whatever is using it, \
             or set a different `port` in gravityd.toml"
        )));
    }

    // A daemon already answering here is the one case negotiation must not
    // paper over: it owns the same state directory this one is about to open,
    // and moving aside would leave two of them running.
    let port = configured_port;
    if let Ok(Some(version)) =
        tokio::task::spawn_blocking(move || probe_health(port, OCCUPANT_PROBE_TIMEOUT)).await
    {
        anyhow::bail!(
            "port {configured_port} is already served by a daemon (version {version}); \
             stop it before starting this one"
        );
    }

    // A restart usually collides with its own predecessor's listener, or with
    // whatever raced it to the port during an update; both clear in about a
    // second. Waiting that out is cheaper than moving the bus off the
    // allowlisted /mcp URL and restarting later to take the port back.
    let deadline = tokio::time::Instant::now() + grace;
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(PORT_GRACE_INTERVAL).await;
        match bind_port(addresses, configured_port, configured_port).await {
            Ok(bound) => {
                tracing::info!(port = configured_port, "configured port cleared in time");
                return Ok(bound);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {}
            Err(error) => return Err(error.into()),
        }
    }

    tracing::warn!(
        port = configured_port,
        error = %error,
        "configured port unavailable; negotiating a fallback"
    );
    for _ in 0..PORT_NEGOTIATION_ATTEMPTS {
        match bind_port(addresses, 0, configured_port).await {
            Ok(bound) => return Ok(bound),
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {}
            Err(error) => return Err(error.into()),
        }
    }
    anyhow::bail!("could not reserve a fallback port after {PORT_NEGOTIATION_ATTEMPTS} attempts")
}

/// Asks whatever owns `port` on loopback whether it is a Gravity-family daemon,
/// returning the version it reports. Hand-rolled because this runs before the
/// app exists and the daemon has no HTTP client of its own.
pub(crate) fn probe_health(port: u16, timeout: Duration) -> Option<String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = std::net::TcpStream::connect_timeout(&addr, timeout).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    stream.set_write_timeout(Some(timeout)).ok()?;
    stream
        .write_all(b"GET /health HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .ok()?;
    let mut response = Vec::new();
    let mut chunk = [0_u8; 1024];
    while response.len() < 8192 {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => response.extend_from_slice(&chunk[..n]),
        }
    }
    health_version(&String::from_utf8_lossy(&response))
}

/// The `version` a 200 `/health` response reports, if it looks like one.
fn health_version(response: &str) -> Option<String> {
    let (status, body) = response.split_once("\r\n")?;
    if !status.starts_with("HTTP/1.") || !status.contains(" 200") {
        return None;
    }
    let (_, rest) = body.split_once("\"version\"")?;
    let (_, rest) = rest.split_once(':')?;
    let (_, rest) = rest.split_once('"')?;
    let (version, _) = rest.split_once('"')?;
    Some(version.to_string())
}

/// Resolves once the configured port can be bound again, so a daemon parked on
/// a fallback can go home.
pub(super) async fn wait_for_configured_port(
    addresses: Vec<IpAddr>,
    port: u16,
    interval: Duration,
) {
    loop {
        tokio::time::sleep(interval).await;
        // The listener is dropped right here; the restart re-binds it, and
        // loses the race only to something that would have taken it anyway.
        if bind_port(&addresses, port, port).await.is_ok() {
            return;
        }
    }
}
