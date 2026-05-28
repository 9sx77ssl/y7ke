//! Headless Y7KE client node for the network-namespace NAT simulation
//! (`scripts/nat-sim/run.sh`). Net-level only — no storage, no GUI — so
//! it runs inside an `ip netns exec` shell.
//!
//! Two roles, selected by `Y7KE_ROLE`:
//!   * `responder` — reserve a relay slot, auto-accept any inbound
//!     handshake, print connection-kind transitions, hold open.
//!   * `initiator` — reserve a relay slot, dial the responder's
//!     `/p2p-circuit` address (`Y7KE_TARGET`), send a handshake, print
//!     the resulting connection kind, exit 0 on success.
//!
//! Env:
//!   Y7KE_SEED       32-byte hex identity seed (deterministic PeerId)
//!   Y7KE_BOOTSTRAP  this node's view of the relay/Kad bootstrap multiaddr
//!   Y7KE_ROLE       "responder" | "initiator"
//!   Y7KE_TARGET     initiator only: responder's full /p2p-circuit addr
//!
//! Output lines are machine-readable: PEER_ID=, RESERVED, KIND=,
//! UPGRADED=, HANDSHAKE_OK, FAIL=.

use std::env;
use std::time::Duration;

use libp2p::multiaddr::Protocol;
use libp2p::{Multiaddr, PeerId};
use tokio::time::timeout;
use tracing_subscriber::EnvFilter;

use y7ke_net::{
    build_swarm, libp2p_keypair_from_y7_secret, spawn_swarm_with_bootstraps, HandshakeReq,
    HandshakeResp, NetCommand, NetEvent, NetHandle,
};

const RESERVATION_BUDGET: Duration = Duration::from_secs(40);
const CONNECT_BUDGET: Duration = Duration::from_secs(60);

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::new("warn,y7ke_net=info,libp2p_relay=info,libp2p_dcutr=info")
        }))
        .init();

    let seed = hex32(&env::var("Y7KE_SEED").expect("Y7KE_SEED"));
    let bootstrap: Multiaddr = env::var("Y7KE_BOOTSTRAP")
        .expect("Y7KE_BOOTSTRAP")
        .parse()
        .expect("parse Y7KE_BOOTSTRAP");
    let role = env::var("Y7KE_ROLE").expect("Y7KE_ROLE");

    let kp = libp2p_keypair_from_y7_secret(&seed).expect("keypair from seed");
    let local = kp.public().to_peer_id();
    println!("PEER_ID={local}");

    let swarm = build_swarm(kp).expect("build swarm");
    let mut net = spawn_swarm_with_bootstraps(
        swarm,
        vec![bootstrap],
        y7ke_core::settings::DialMode::Internet,
    );

    // Both roles first wait for the relay reservation to land.
    if !wait_for_reservation(&mut net).await {
        println!("FAIL=no-reservation");
        std::process::exit(1);
    }
    println!("RESERVED");

    match role.as_str() {
        "responder" => run_responder(net).await,
        "initiator" => run_initiator(net).await,
        other => {
            println!("FAIL=unknown-role-{other}");
            std::process::exit(2);
        }
    }
}

/// Drain events until a `/p2p-circuit` listen address appears (relay
/// reservation accepted) or the budget expires.
async fn wait_for_reservation(net: &mut NetHandle) -> bool {
    timeout(RESERVATION_BUDGET, async {
        loop {
            match net.event_rx().recv().await {
                Ok(NetEvent::Listening { addr })
                    if addr.iter().any(|p| matches!(p, Protocol::P2pCircuit)) =>
                {
                    return true;
                }
                Ok(NetEvent::ConnectionEstablished { peer, kind, .. }) => {
                    eprintln!("(reservation phase) connected {peer} kind={kind:?}");
                }
                Ok(_) => {}
                Err(_) => return false,
            }
        }
    })
    .await
    .unwrap_or(false)
}

/// Auto-accept inbound handshakes; print connection-kind transitions.
async fn run_responder(mut net: NetHandle) -> ! {
    let cmd = net.clone_command_sender();
    loop {
        match net.event_rx().recv().await {
            Ok(NetEvent::HandshakeReceived {
                request, channel, ..
            }) => {
                println!("HANDSHAKE_RECV greeting={:?}", request.greeting);
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
            Ok(NetEvent::ConnectionEstablished { peer, kind, .. }) => {
                println!("KIND={kind:?} peer={peer}");
            }
            Ok(NetEvent::ConnectionUpgraded { peer, kind, .. }) => {
                println!("UPGRADED={kind:?} peer={peer}");
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("event rx closed: {e}");
                std::process::exit(3);
            }
        }
    }
}

/// Dial the responder via its circuit address, handshake, report kind.
async fn run_initiator(net: NetHandle) -> ! {
    let target: Multiaddr = env::var("Y7KE_TARGET")
        .expect("Y7KE_TARGET")
        .parse()
        .expect("parse Y7KE_TARGET");
    let target_peer = peer_id_in(&target).expect("Y7KE_TARGET must end in /p2p/<peer>");

    // Watch for the connection kind to the target on a cloned receiver
    // while we drive the dial + handshake on the main handle.
    let mut ev = net.try_clone_event_rx();
    let kind_task = tokio::spawn(async move {
        let mut last = None;
        let _ = timeout(CONNECT_BUDGET, async {
            loop {
                match ev.recv().await {
                    Ok(NetEvent::ConnectionEstablished { peer, kind, .. })
                        if peer == target_peer =>
                    {
                        println!("KIND={kind:?}");
                        last = Some(kind);
                    }
                    Ok(NetEvent::ConnectionUpgraded { peer, kind, .. }) if peer == target_peer => {
                        println!("UPGRADED={kind:?}");
                        last = Some(kind);
                    }
                    Ok(_) => {}
                    Err(_) => return,
                }
            }
        })
        .await;
        last
    });

    // A few attempts — the reservation on the far side may still be
    // settling when we fire the first circuit dial.
    let mut handshake_ok = false;
    for attempt in 1..=6 {
        eprintln!("attempt {attempt}: dial {target}");
        let _ = net.dial_address(target.clone()).await;
        let req = HandshakeReq {
            initiator_ed25519_pub: [0x01; 32],
            initiator_eph_x25519_pub: [0x77; 32],
            sig: [0x88; 64],
            greeting: Some("nat-sim hello".into()),
        };
        match timeout(
            Duration::from_secs(10),
            net.send_handshake(target_peer, req),
        )
        .await
        {
            Ok(Ok(resp)) if resp.accept => {
                handshake_ok = true;
                break;
            }
            Ok(Ok(_)) => eprintln!("attempt {attempt}: handshake rejected"),
            Ok(Err(e)) => eprintln!("attempt {attempt}: handshake err: {e}"),
            Err(_) => eprintln!("attempt {attempt}: handshake timed out"),
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }

    if handshake_ok {
        println!("HANDSHAKE_OK");
        // Hold open so DCUtR has time to attempt a relay→direct upgrade
        // (both peers must stay live during the simultaneous open). The
        // full-cone NAT sim sets a longer Y7KE_HOLD_SECS than the
        // blocked-path sim, which only needs to confirm Relayed.
        let hold = env::var("Y7KE_HOLD_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8);
        tokio::time::sleep(Duration::from_secs(hold)).await;
        std::process::exit(0);
    } else {
        let _ = kind_task.await;
        println!("FAIL=handshake");
        std::process::exit(1);
    }
}

fn peer_id_in(addr: &Multiaddr) -> Option<PeerId> {
    // Multiaddr::iter() isn't double-ended; walk forward, keep the last
    // /p2p component (the circuit target, after the relay's own /p2p).
    let mut found = None;
    for p in addr.iter() {
        if let Protocol::P2p(id) = p {
            found = Some(id);
        }
    }
    found
}

fn hex32(s: &str) -> [u8; 32] {
    let bytes: Vec<u8> = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect();
    bytes
        .try_into()
        .expect("Y7KE_SEED must be 32 bytes (64 hex chars)")
}
