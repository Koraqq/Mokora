//! Regression test for the "game stuck on loading screen" hang in MITM mode.
//!
//! When a proxy redirector (Proxifier &c.) matches the game executable for
//! *any* target host, it also sweeps up the game's own loopback self-connect —
//! the standard Windows `socketpair()` emulation, which binds and listens on
//! `127.0.0.1:0` and then connects to itself. Mokora then receives
//! `CONNECT 127.0.0.1:<ephemeral>`.
//!
//! Tunneling that request deadlocks the game. hudsucker's `process_connect`
//! answers `200` and then blocks reading the client's first 4 bytes *before* it
//! dials the upstream socket, but a self-connect's client side never speaks
//! first — it is waiting for its own `accept()` to complete, which can only
//! happen once Mokora dials. Neither side moves, the game's network init wedges,
//! and it hangs on the loading screen forever.
//!
//! So Mokora refuses loopback `CONNECT`s outright, in `handle_request`, which
//! short-circuits hudsucker's `proxy()` before that blocking read is reached.
//! This test drives a real proxy over a real socket to prove it.

use std::sync::Arc;
use std::time::Duration;

use mokora::config::{HttpCaptureConfig, Platform, ProxyConfig};
use mokora::event_bus::notify_bus;
use mokora::logger::Session;
use mokora::proxy::start_proxy;
use mokora::schema::NotifyLevel;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, oneshot, Notify};

/// Generous enough that a slow machine never flakes, short enough that the
/// pre-fix deadlock (which blocks forever) is still caught by the harness.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);

/// Ask the OS for a free port, then hand it to the proxy. The gap between
/// closing this listener and the proxy binding is racy in principle; in
/// practice the OS does not immediately hand the same port to another process.
async fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind a probe listener");
    listener
        .local_addr()
        .expect("probe listener has no local addr")
        .port()
}

/// Send `CONNECT {target} HTTP/1.1` to the proxy and return the status code of
/// the response, without ever writing a byte of tunnel payload. That silence is
/// the point: it is exactly how the game's self-connected socket behaves.
async fn connect_status(proxy_port: u16, target: &str) -> u16 {
    let mut stream = TcpStream::connect(("127.0.0.1", proxy_port))
        .await
        .expect("failed to reach the proxy");

    let req = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n");
    stream
        .write_all(req.as_bytes())
        .await
        .expect("failed to write CONNECT");

    let mut buf = Vec::new();
    let status_line = tokio::time::timeout(RESPONSE_TIMEOUT, async {
        let mut chunk = [0u8; 256];
        loop {
            let n = stream.read(&mut chunk).await.expect("failed to read");
            assert_ne!(n, 0, "proxy closed the connection without responding");
            buf.extend_from_slice(&chunk[..n]);
            if let Some(end) = buf.windows(2).position(|w| w == b"\r\n") {
                return String::from_utf8_lossy(&buf[..end]).into_owned();
            }
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!("proxy never answered CONNECT {target} — it is blocking on the client's first bytes")
    });

    status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("malformed status line: {status_line:?}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn loopback_connect_is_refused_without_blocking() {
    let tmp = TempDir::new().expect("failed to create tempdir");
    let session = Session::init(&tmp.path().join("logs"), "info", "info", &[])
        .expect("failed to init logging session");

    let port = free_port().await;
    let config = ProxyConfig {
        enabled: true,
        addr: format!("127.0.0.1:{port}"),
        ca_dir: tmp.path().join("ca"),
        mitm_domains: ProxyConfig::default().mitm_domains,
        rewrite_certificate_report: true,
        block_telemetry: true,
    };

    let notify = notify_bus();
    let mut toasts = notify.subscribe();

    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    let proxy = tokio::spawn(start_proxy(
        config,
        HttpCaptureConfig::default(),
        Platform::Majsoul,
        Arc::new(session),
        None,
        Some(notify),
        Arc::new(Notify::new()),
        async move {
            stop_rx.await.unwrap_or_default();
        },
    ));

    wait_until_listening(port).await;

    // The exact request the wedged Mahjong Soul client produced: its own
    // internal socketpair listener, handed to us by the redirector.
    assert_eq!(
        connect_status(port, "127.0.0.1:65423").await,
        403,
        "loopback CONNECT must be refused, not tunneled"
    );
    // Loopback by name and over IPv6 are the same misconfiguration.
    assert_eq!(connect_status(port, "localhost:65423").await, 403);
    assert_eq!(connect_status(port, "[::1]:65423").await, 403);

    // Control: a real game host must still be tunneled. hudsucker answers 200
    // and waits for the client's bytes before dialing upstream, so this needs
    // no network. We only assert it was not swept up by the refusal.
    //
    // 192.0.2.0/24 is TEST-NET-1 (RFC 5737) — reserved, never routed.
    assert_ne!(
        connect_status(port, "192.0.2.1:443").await,
        403,
        "non-loopback CONNECT must not be refused"
    );

    // The user is staring at a game that never loads, not at the log — so the
    // refusal must reach them as a toast.
    let toast = toasts
        .try_recv()
        .expect("expected a loopback warning toast");
    assert_eq!(toast.level, NotifyLevel::Warn);
    assert!(
        toast.sticky,
        "the game stays broken until the redirector is fixed, so the toast must persist"
    );
    assert_eq!(toast.id.as_deref(), Some("proxy-loopback-connect"));
    let body = toast.body.expect("toast must say how to fix it");
    assert!(
        body.contains("127.0.0.1"),
        "body should name what to exclude"
    );

    // Exactly one, though three loopback CONNECTs were refused: a misconfigured
    // redirector produces one per socket the game opens, and telling the user
    // once is enough.
    assert!(
        matches!(
            toasts.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ),
        "the loopback toast must not repeat per refused connection"
    );

    let _ = stop_tx.send(());
    let _ = tokio::time::timeout(RESPONSE_TIMEOUT, proxy).await;
}

/// Poll until the proxy has bound its port. `start_proxy` generates a CA on
/// first run, so it is not listening the instant the task is spawned.
async fn wait_until_listening(port: u16) {
    let deadline = tokio::time::Instant::now() + RESPONSE_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("proxy never bound 127.0.0.1:{port}");
}
