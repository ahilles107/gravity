use super::*;

const LOOPBACK: IpAddr = IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);

#[tokio::test]
async fn keeps_the_configured_port_when_it_is_available() {
    let reserved = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("reserve port");
    let port = reserved.local_addr().expect("reserved address").port();
    drop(reserved);

    let bound = bind(&[LOOPBACK], port, PortPolicy::Negotiate)
        .await
        .expect("bind configured port");

    assert_eq!(bound.port(), port);
    assert!(!bound.negotiated());
}

#[tokio::test]
async fn negotiates_when_the_configured_port_is_taken() {
    let reserved = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("reserve port");
    let configured = reserved.local_addr().expect("reserved address").port();

    let bound = bind_with_grace(
        &[LOOPBACK],
        configured,
        PortPolicy::Negotiate,
        Duration::ZERO,
    )
    .await
    .expect("bind fallback port");

    assert_ne!(bound.port(), configured);
    assert!(bound.negotiated());
}

#[tokio::test]
async fn waits_out_a_port_that_clears_during_the_grace_period() {
    let reserved = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("reserve port");
    let configured = reserved.local_addr().expect("reserved address").port();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        drop(reserved);
    });

    let bound = bind_with_grace(
        &[LOOPBACK],
        configured,
        PortPolicy::Negotiate,
        Duration::from_secs(5),
    )
    .await
    .expect("bind configured port");

    assert_eq!(bound.port(), configured);
    assert!(!bound.negotiated());
}

#[tokio::test]
async fn the_grace_period_never_delays_a_strict_launch() {
    let reserved = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("reserve port");
    let configured = reserved.local_addr().expect("reserved address").port();

    let started = std::time::Instant::now();
    let error = match bind(&[LOOPBACK], configured, PortPolicy::Strict).await {
        Ok(_) => panic!("a strict launch must fail on a collision"),
        Err(error) => error,
    };

    assert!(error.to_string().contains(&configured.to_string()));
    assert!(started.elapsed() < PORT_GRACE);
}

#[tokio::test]
async fn a_freed_configured_port_wakes_the_reclaim_watch() {
    let reserved = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("reserve port");
    let configured = reserved.local_addr().expect("reserved address").port();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        drop(reserved);
    });

    let waited = tokio::time::timeout(
        Duration::from_secs(5),
        wait_for_configured_port(vec![LOOPBACK], configured, Duration::from_millis(50)),
    )
    .await;

    assert!(waited.is_ok(), "the watch should end once the port is free");
}

#[tokio::test]
async fn refuses_to_move_aside_for_another_daemon() {
    let reserved = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("reserve port");
    let configured = reserved.local_addr().expect("reserved address").port();
    tokio::spawn(async move {
        let (stream, _) = reserved.accept().await.expect("occupant accept");
        let body = "{\"status\":\"ok\",\"version\":\"0.8.0\"}";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let mut stream = stream;
        tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes())
            .await
            .expect("occupant write");
    });

    let error = bind(&[LOOPBACK], configured, PortPolicy::Negotiate)
        .await
        .err()
        .expect("a live daemon must stop the launch");

    assert!(error.to_string().contains("0.8.0"), "{error}");
}

#[tokio::test]
async fn rejects_a_collision_without_negotiation() {
    let reserved = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("reserve port");
    let configured = reserved.local_addr().expect("reserved address").port();

    let error = bind(&[LOOPBACK], configured, PortPolicy::Strict)
        .await
        .err()
        .expect("collision must fail");

    assert!(
        error.to_string().contains(&configured.to_string()),
        "{error}"
    );
    assert_eq!(
        error
            .downcast_ref::<std::io::Error>()
            .map(std::io::Error::kind),
        Some(std::io::ErrorKind::AddrInUse)
    );
}

fn config_with(bind: Vec<IpAddr>, negotiate_port: bool) -> crate::config::Config {
    crate::config::Config {
        bind,
        negotiate_port,
        ..crate::config::Config::default()
    }
}

#[test]
fn only_a_loopback_daemon_asked_to_negotiate_may_negotiate() {
    let loopback = config_with(vec![LOOPBACK], true);
    assert_eq!(port_policy(&loopback, true), PortPolicy::Negotiate);
    assert_eq!(port_policy(&loopback, false), PortPolicy::Strict);

    let opted_out = config_with(vec![LOOPBACK], false);
    assert_eq!(port_policy(&opted_out, true), PortPolicy::Strict);

    let reachable = config_with(vec![LOOPBACK, IpAddr::from([100, 64, 0, 1])], true);
    assert_eq!(port_policy(&reachable, true), PortPolicy::Strict);
}

#[test]
fn reads_the_version_out_of_a_health_response() {
    let ok = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n\
              {\"status\":\"ok\",\"version\":\"0.9.0\"}";
    assert_eq!(health_version(ok).as_deref(), Some("0.9.0"));

    assert_eq!(health_version("HTTP/1.1 404 Not Found\r\n\r\n"), None);
    assert_eq!(health_version("SSH-2.0-OpenSSH_9.0\r\n"), None);
    assert_eq!(
        health_version("HTTP/1.1 200 OK\r\n\r\n{\"status\":\"ok\"}"),
        None
    );
}
