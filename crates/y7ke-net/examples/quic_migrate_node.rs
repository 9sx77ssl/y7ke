//! Headless node for the QUIC connection-migration experiment
//! (`scripts/nat-sim/run-quic-migration.sh`, Phase 3.4). Direct dial, no
//! relay/bootstrap — two nodes on one segment connect over QUIC, then the
//! initiator's IP is changed underneath the live connection.
//!
//! The question: does libp2p-quic migrate the existing connection
//! (RFC 9000 §9 path validation — same ConnectionId persists, no new
//! ConnectionEstablished) or drop + rediscover? We surface every
//! connection event WITH its ConnectionId (added to NetEvent in the
//! per-connection-tracking commit) so the script can tell which happened.
//!
//! Env: Y7KE_SEED (hex32), Y7KE_ROLE (responder|initiator),
//!      Y7KE_TARGET (initiator: direct /quic-v1 multiaddr of responder),
//!      Y7KE_HOLD_SECS (initiator probe window, default 40).
//!
//! Output lines: PEER_ID=, LISTEN=, EVENT established/closed conn=<id>,
//! PROBE n=<i> ok|fail, HANDSHAKE_OK.

use std::env;
use std::time::Duration;

use libp2p::{Multiaddr, PeerId};
use libp2p::multiaddr::Protocol;
use tokio::time::timeout;
use tracing_subscriber::EnvFilter;

use y7ke_net::{
    build_swarm, libp2p_keypair_from_y7_secret, spawn_swarm_with_bootstraps, HandshakeReq,
    HandshakeResp, NetCommand, NetEvent, NetHandle,
};

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("warn,y7ke_net=info,libp2p_quic=info")),
        )
        .init();

    let seed = hex32(&env::var("Y7KE_SEED").expect("Y7KE_SEED"));
    let role = env::var("Y7KE_ROLE").expect("Y7KE_ROLE");
    let kp = libp2p_keypair_from_y7_secret(&seed).expect("keypair");
    println!("PEER_ID={}", kp.public().to_peer_id());

    let swarm = build_swarm(kp).expect("build swarm");
    // No bootstraps — pure direct dial, so no relay path can mask a drop.
    let net = spawn_swarm_with_bootstraps(swarm, vec![], y7ke_core::settings::DialMode::Internet);

    match role.as_str() {
        "responder" => run_responder(net).await,
        "initiator" => run_initiator(net).await,
        other => {
            println!("FAIL=unknown-role-{other}");
            std::process::exit(2);
        }
    }
}

/// Print listen addrs (so the script learns our QUIC port), auto-accept
/// handshakes, and log every connection event with its ConnectionId.
async fn run_responder(mut net: NetHandle) -> ! {
    let cmd = net.clone_command_sender();
    loop {
        match net.event_rx().recv().await {
            Ok(NetEvent::Listening { addr }) => println!("LISTEN={addr}"),
            Ok(NetEvent::ConnectionEstablished {
                connection_id, kind, ..
            }) => println!("EVENT established conn={connection_id:?} kind={kind:?}"),
            Ok(NetEvent::ConnectionClosed { connection_id, .. }) => {
                println!("EVENT closed conn={connection_id:?}")
            }
            Ok(NetEvent::HandshakeReceived { channel, .. }) => {
                if let Some(ch) = channel.take() {
                    let _ = cmd
                        .send(NetCommand::RespondHandshake {
                            channel: ch,
                            response: HandshakeResp {
                                responder_eph_x25519_pub: [0x66; 32],
                                sig: [0xAB; 64],
                                accept: true,
                            },
                        })
                        .await;
                }
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("rx closed: {e}");
                std::process::exit(3);
            }
        }
    }
}

/// Dial the target directly over QUIC, handshake, then probe every 3s for
/// the hold window while logging connection events with their
/// ConnectionId. The script changes our IP mid-window; if the same conn
/// id persists and probes keep succeeding, the connection migrated.
async fn run_initiator(net: NetHandle) -> ! {
    let target: Multiaddr = env::var("Y7KE_TARGET")
        .expect("Y7KE_TARGET")
        .parse()
        .expect("parse target");
    let target_peer = peer_id_in(&target).expect("target needs /p2p/<peer>");
    let hold: u64 = env::var("Y7KE_HOLD_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(40);

    // Event watcher: log establish/close with ConnectionId for the target.
    let mut ev = net.try_clone_event_rx();
    tokio::spawn(async move {
        loop {
            match ev.recv().await {
                Ok(NetEvent::ConnectionEstablished {
                    peer,
                    connection_id,
                    kind,
                    ..
                }) if peer == target_peer => {
                    println!("EVENT established conn={connection_id:?} kind={kind:?}")
                }
                Ok(NetEvent::ConnectionClosed {
                    peer,
                    connection_id,
                }) if peer == target_peer => println!("EVENT closed conn={connection_id:?}"),
                Ok(_) => {}
                Err(_) => return,
            }
        }
    });

    // Initial dial + handshake (retry while the responder settles).
    let mut ok = false;
    for attempt in 1..=6 {
        let _ = net.dial_address(target.clone()).await;
        let req = HandshakeReq {
            initiator_ed25519_pub: [0x01; 32],
            initiator_eph_x25519_pub: [0x77; 32],
            sig: [0x88; 64],
            greeting: Some("quic-migrate".into()),
        };
        match timeout(Duration::from_secs(8), net.send_handshake(target_peer, req)).await {
            Ok(Ok(r)) if r.accept => {
                ok = true;
                break;
            }
            _ => {
                eprintln!("attempt {attempt}: handshake not yet");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
    if !ok {
        println!("FAIL=initial-handshake");
        std::process::exit(1);
    }
    println!("HANDSHAKE_OK");

    // Probe loop: a handshake RPC every 3s. Survives the IP change iff the
    // connection migrated (or transparently re-dialed). The EVENT lines
    // above tell which: a new conn= after the IP change == re-dial.
    let probes = hold / 3;
    for i in 0..probes {
        tokio::time::sleep(Duration::from_secs(3)).await;
        let req = HandshakeReq {
            initiator_ed25519_pub: [0x01; 32],
            initiator_eph_x25519_pub: [0x77; 32],
            sig: [0x88; 64],
            greeting: Some(format!("probe{i}")),
        };
        match timeout(Duration::from_secs(5), net.send_handshake(target_peer, req)).await {
            Ok(Ok(r)) if r.accept => println!("PROBE n={i} ok"),
            Ok(Ok(_)) => println!("PROBE n={i} rejected"),
            Ok(Err(e)) => println!("PROBE n={i} fail err={e}"),
            Err(_) => println!("PROBE n={i} fail timeout"),
        }
    }
    std::process::exit(0);
}

fn peer_id_in(addr: &Multiaddr) -> Option<PeerId> {
    let mut found = None;
    for p in addr.iter() {
        if let Protocol::P2p(id) = p {
            found = Some(id);
        }
    }
    found
}

fn hex32(s: &str) -> [u8; 32] {
    let b: Vec<u8> = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect();
    b.try_into().expect("seed must be 32 bytes")
}
