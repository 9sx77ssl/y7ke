//! V2-A6 smoke test: confirm the swarm binds a `/quic-v1` listener in
//! parallel with the existing TCP listener.
//!
//! Skipped on macOS/Windows for parity with other mDNS-touching tests —
//! CI runners on those platforms don't surface UDP bind addresses
//! reliably in sandboxed environments.

use std::time::Duration;

use libp2p::multiaddr::Protocol;
use tokio::sync::broadcast;
use tokio::time::timeout;

use y7ke_net::{build_swarm, libp2p_keypair_from_y7_secret, spawn_swarm, NetEvent};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[cfg_attr(any(target_os = "macos", target_os = "windows"), ignore)]
async fn swarm_binds_a_quic_listener() {
    let kp = libp2p_keypair_from_y7_secret(&[0x7Au8; 32]).unwrap();
    let swarm = build_swarm(kp).expect("build_swarm");
    let mut handle = spawn_swarm(swarm);

    let mut saw_tcp = false;
    let mut saw_quic = false;

    // Both listeners post NewListenAddr almost immediately; wait up to
    // 5 s for both before declaring failure.
    let deadline = Duration::from_secs(5);
    let collect = async {
        while !(saw_tcp && saw_quic) {
            match handle.event_rx().recv().await {
                Ok(NetEvent::Listening { addr }) => {
                    let mut has_quic = false;
                    let mut has_tcp = false;
                    for p in addr.iter() {
                        match p {
                            Protocol::QuicV1 | Protocol::Quic => has_quic = true,
                            Protocol::Tcp(_) => has_tcp = true,
                            _ => {}
                        }
                    }
                    if has_quic {
                        saw_quic = true;
                    }
                    if has_tcp {
                        saw_tcp = true;
                    }
                }
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    timeout(deadline, collect)
        .await
        .expect("did not observe both TCP and QUIC Listening events within 5s");

    assert!(saw_tcp, "expected a /tcp/ Listening event");
    assert!(saw_quic, "expected a /quic-v1 Listening event");

    handle.shutdown().await.unwrap();
}
