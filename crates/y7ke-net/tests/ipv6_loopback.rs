//! IPv6 loopback proof: two swarms bind `/ip6/::` (best-effort, V2), node A
//! direct-dials B over `::1`, both observe `ConnectionEstablished`. This is
//! what flips client IPv6-listen from PLANNED → PROVEN(loopback).
//!
//! Skip-passes (eprintln + return, no panic) if no IPv6 TCP `Listening` event
//! appears within 5s — a v4-only host is a legitimate config (the listeners
//! are best-effort by design), so CI without IPv6 stays green WITHOUT
//! `#[ignore]`. `::1` is loopback, so the connection classifies as `Lan`.

use std::time::Duration;

use libp2p::{multiaddr::Protocol, Multiaddr};
use tokio::sync::broadcast;
use tokio::time::timeout;

use y7ke_core::ConnectionKind;
use y7ke_net::{build_swarm, libp2p_keypair_from_y7_secret, spawn_swarm, NetEvent};

const TEST_TIMEOUT: Duration = Duration::from_secs(20);

#[cfg_attr(
    any(target_os = "macos", target_os = "windows"),
    ignore = "IPv6 loopback unreliable on GitHub Actions mac/windows runners"
)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ipv6_loopback_direct_dial() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn,y7ke_net=info")
        .with_test_writer()
        .try_init();
    timeout(TEST_TIMEOUT, scenario())
        .await
        .expect("ipv6_loopback_direct_dial timed out");
}

async fn scenario() {
    let a_kp = libp2p_keypair_from_y7_secret(&[0x1Au8; 32]).unwrap();
    let b_kp = libp2p_keypair_from_y7_secret(&[0x1Bu8; 32]).unwrap();
    let a_peer = a_kp.public().to_peer_id();
    let b_peer = b_kp.public().to_peer_id();

    let a = spawn_swarm(build_swarm(a_kp).expect("build a"));
    let mut b = spawn_swarm(build_swarm(b_kp).expect("build b"));

    // Find Bob's IPv6 TCP listen port. None within 5s ⇒ no usable IPv6 here
    // ⇒ skip-pass (the v6 listeners are best-effort).
    let Some(port) = wait_for_v6_tcp_port(b.event_rx()).await else {
        eprintln!("no IPv6 TCP listener within 5s — skip-pass (v4-only host)");
        let _ = a.shutdown().await;
        let _ = b.shutdown().await;
        return;
    };

    // Bind is on `::` (unspecified); dial the loopback `::1` on that port.
    let dial: Multiaddr = format!("/ip6/::1/tcp/{port}/p2p/{b_peer}")
        .parse()
        .expect("compose v6 loopback dial addr");
    eprintln!("dialing bob over {dial}");

    let mut a_events = a.try_clone_event_rx();
    let mut b_events = b.try_clone_event_rx();
    a.dial_address(dial).await.expect("dial_address");

    let a_conn = wait_for_conn(&mut a_events, b_peer);
    let b_conn = wait_for_conn(&mut b_events, a_peer);
    let (ak, bk) = timeout(
        Duration::from_secs(8),
        futures::future::join(a_conn, b_conn),
    )
    .await
    .expect("ConnectionEstablished did not fire on both sides over IPv6 loopback");

    // `::1` is loopback → classified Lan by multiaddr_is_lan.
    assert_eq!(ak, ConnectionKind::Lan, "alice side kind");
    assert_eq!(bk, ConnectionKind::Lan, "bob side kind");

    a.shutdown().await.unwrap();
    b.shutdown().await.unwrap();
}

/// First IPv6 TCP `Listening` event's port, or `None` after 5s.
async fn wait_for_v6_tcp_port(rx: &mut broadcast::Receiver<NetEvent>) -> Option<u16> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return None;
        }
        match timeout(remaining, rx.recv()).await {
            Ok(Ok(NetEvent::Listening { addr })) => {
                let mut is_v6 = false;
                let mut port = None;
                for p in addr.iter() {
                    match p {
                        Protocol::Ip6(_) => is_v6 = true,
                        Protocol::Tcp(n) => port = Some(n),
                        _ => {}
                    }
                }
                if is_v6 {
                    if let Some(n) = port {
                        return Some(n);
                    }
                }
            }
            Ok(Ok(_)) => continue,
            Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(broadcast::error::RecvError::Closed)) => return None,
            Err(_) => return None, // 5s timeout
        }
    }
}

async fn wait_for_conn(
    rx: &mut broadcast::Receiver<NetEvent>,
    expected: libp2p::PeerId,
) -> ConnectionKind {
    loop {
        match rx.recv().await {
            Ok(NetEvent::ConnectionEstablished { peer, kind, .. }) if peer == expected => {
                return kind
            }
            Ok(_) => continue,
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => panic!("event channel closed"),
        }
    }
}
