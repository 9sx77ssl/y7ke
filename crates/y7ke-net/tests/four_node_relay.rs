//! V2-A4 integration test: a relay-bootstrap plus two clients
//! (Alice + Bob). Both clients reserve a slot on the bootstrap and
//! Alice dials Bob through it using the `/p2p-circuit` multiaddr.
//!
//! The bootstrap node here is a stripped-down relay server — it
//! carries `relay::Behaviour` (server side) alongside identify, ping,
//! and kad. The production `Y7Behaviour` carries only the relay
//! *client*, which is why this test inlines its own server behaviour
//! rather than reusing `build_swarm`.

use std::time::Duration;

use libp2p::{
    identify,
    identity::Keypair,
    kad::{self, store::MemoryStore},
    multiaddr::Protocol,
    noise, ping, relay,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder,
};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio::time::timeout;

use y7ke_core::ConnectionKind;
use y7ke_net::{
    build_swarm, libp2p_keypair_from_y7_secret, spawn_swarm_with_bootstraps, HandshakeReq,
    HandshakeResp, NetCommand, NetEvent, NetHandle,
};

const TEST_TIMEOUT: Duration = Duration::from_secs(120);
const KAD_PROTOCOL: StreamProtocol = StreamProtocol::new("/y7ke/kad/1.0.0");
const IDENTIFY_PROTOCOL: &str = "/y7ke/0.1.0";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg_attr(any(target_os = "macos", target_os = "windows"), ignore)]
async fn alice_relays_to_bob_via_bootstrap() {
    init_tracing();
    timeout(TEST_TIMEOUT, run_relay_round_trip())
        .await
        .expect("four_node_relay timed out");
}

async fn run_relay_round_trip() {
    let bootstrap_kp = libp2p_keypair_from_y7_secret(&[0xBB; 32]).unwrap();
    let alice_kp = libp2p_keypair_from_y7_secret(&[0x0A; 32]).unwrap();
    let bob_kp = libp2p_keypair_from_y7_secret(&[0x0B; 32]).unwrap();

    let bootstrap_peer = bootstrap_kp.public().to_peer_id();
    let alice_peer = alice_kp.public().to_peer_id();
    let bob_peer = bob_kp.public().to_peer_id();
    let alice_secret = [0x0Au8; 32];

    // Boot the relay-bootstrap first.
    let mut bootstrap_swarm = build_relay_bootstrap(bootstrap_kp);
    bootstrap_swarm
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .expect("bootstrap listen");
    let bootstrap_listen = wait_for_bootstrap_listening(&mut bootstrap_swarm).await;
    let bootstrap_multiaddr: Multiaddr = format!("{bootstrap_listen}/p2p/{bootstrap_peer}")
        .parse()
        .expect("compose bootstrap multiaddr");
    eprintln!("bootstrap listening on {bootstrap_multiaddr}");

    // The relay-server needs to know its own external address so it can
    // hand that address back to clients in the reservation response.
    // Without this `outbound_hop::make_reservation` rejects with
    // `NoAddressesInReservation`.
    bootstrap_swarm.add_external_address(bootstrap_listen.clone());

    let _bootstrap_task = tokio::spawn(drive_bootstrap(bootstrap_swarm));

    // Alice + Bob run as full y7ke-net clients with the bootstrap as their
    // single relay/Kad bootstrap.
    let alice_swarm = build_swarm(alice_kp).expect("build alice");
    let bob_swarm = build_swarm(bob_kp).expect("build bob");
    let mut alice = spawn_swarm_with_bootstraps(
        alice_swarm,
        vec![bootstrap_multiaddr.clone()],
        y7ke_core::settings::DialMode::Internet,
    );
    let mut bob = spawn_swarm_with_bootstraps(
        bob_swarm,
        vec![bootstrap_multiaddr.clone()],
        y7ke_core::settings::DialMode::Internet,
    );

    let _alice_listen = wait_for_listening(alice.event_rx()).await;
    let _bob_listen = wait_for_listening(bob.event_rx()).await;

    // Give both clients up to ~25s to register a reservation on the
    // bootstrap. Without a programmatic signal we conservatively wait;
    // the actual relay handshake completes in <1s on loopback.
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Bob's circuit-reachable address — `<relay>/p2p/<relay-peer>/p2p-circuit/p2p/<bob>`.
    let circuit_to_bob: Multiaddr = bootstrap_multiaddr
        .clone()
        .with(Protocol::P2pCircuit)
        .with(Protocol::P2p(bob_peer));
    eprintln!("alice will dial bob via {circuit_to_bob}");

    let mut alice_events = alice.try_clone_event_rx();
    let mut bob_events = bob.try_clone_event_rx();

    // Bob auto-replies to any inbound handshake.
    let bob_responder = spawn_handshake_responder(&mut bob);

    // Retry the relay dial a few times — on a slow runner the
    // reservation can still be settling when we fire the first dial.
    let mut alice_kind: Option<ConnectionKind> = None;
    let mut bob_kind: Option<ConnectionKind> = None;
    for attempt in 1..=6 {
        eprintln!("attempt {attempt}: alice.dial_address(circuit_to_bob)");
        alice
            .dial_address(circuit_to_bob.clone())
            .await
            .expect("alice dial_address");

        let wait = futures::future::join(
            wait_for_connection_to(&mut alice_events, bob_peer),
            wait_for_connection_to(&mut bob_events, alice_peer),
        );
        match timeout(Duration::from_secs(10), wait).await {
            Ok((ak, bk)) => {
                alice_kind = Some(ak);
                bob_kind = Some(bk);
                break;
            }
            Err(_) => {
                eprintln!("attempt {attempt}: ConnectionEstablished did not fire on both sides");
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }
    }
    let alice_kind = alice_kind.expect("alice never observed ConnectionEstablished");
    let bob_kind = bob_kind.expect("bob never observed ConnectionEstablished");
    assert_eq!(alice_kind, ConnectionKind::Relayed, "alice→bob via relay");
    assert_eq!(bob_kind, ConnectionKind::Relayed, "bob←alice via relay");

    // End-to-end: handshake over the relay.
    let req = HandshakeReq {
        initiator_ed25519_pub: pubkey_for_secret(&alice_secret),
        initiator_eph_x25519_pub: [0x77; 32],
        sig: [0x88; 64],
        greeting: Some("hello via relay".into()),
    };
    let resp = timeout(
        Duration::from_secs(15),
        alice.send_handshake(bob_peer, req.clone()),
    )
    .await
    .expect("alice handshake timed out")
    .expect("alice handshake errored");
    assert!(resp.accept);
    assert_eq!(resp.responder_eph_x25519_pub, [0x66; 32]);
    assert_eq!(resp.sig, [0xAB; 64]);

    let observed = timeout(Duration::from_secs(5), bob_responder)
        .await
        .expect("bob responder did not return")
        .expect("bob responder panicked");
    assert_eq!(observed.greeting.as_deref(), Some("hello via relay"));

    alice.shutdown().await.unwrap();
    bob.shutdown().await.unwrap();
}

// --------------------------------------------------------------------------
// Relay-bootstrap node (server side of circuit-relay v2)
// --------------------------------------------------------------------------

#[derive(NetworkBehaviour)]
struct RelayBootstrapBehaviour {
    relay: relay::Behaviour,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    kad: kad::Behaviour<MemoryStore>,
}

fn build_relay_bootstrap(kp: Keypair) -> Swarm<RelayBootstrapBehaviour> {
    let peer_id = kp.public().to_peer_id();
    SwarmBuilder::with_existing_identity(kp)
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            noise::Config::new,
            yamux::Config::default,
        )
        .expect("tcp/noise/yamux")
        .with_behaviour(|kp| {
            let relay = relay::Behaviour::new(peer_id, relay::Config::default());
            let identify = identify::Behaviour::new(
                identify::Config::new(IDENTIFY_PROTOCOL.to_string(), kp.public())
                    .with_agent_version("y7ke-test-bootstrap".to_string())
                    .with_interval(Duration::from_secs(30)),
            );
            let ping = ping::Behaviour::new(ping::Config::new());
            let mut kad_cfg = kad::Config::new(KAD_PROTOCOL);
            kad_cfg.set_periodic_bootstrap_interval(Some(Duration::from_secs(60)));
            let mut kad = kad::Behaviour::with_config(peer_id, MemoryStore::new(peer_id), kad_cfg);
            kad.set_mode(Some(kad::Mode::Server));
            RelayBootstrapBehaviour {
                relay,
                identify,
                ping,
                kad,
            }
        })
        .expect("behaviour")
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(300)))
        .build()
}

async fn wait_for_bootstrap_listening(swarm: &mut Swarm<RelayBootstrapBehaviour>) -> Multiaddr {
    use futures::StreamExt;
    loop {
        if let Some(SwarmEvent::NewListenAddr { address, .. }) = swarm.next().await {
            return address;
        }
    }
}

async fn drive_bootstrap(mut swarm: Swarm<RelayBootstrapBehaviour>) {
    use futures::StreamExt;
    loop {
        let ev = swarm.select_next_some().await;
        if let SwarmEvent::Behaviour(RelayBootstrapBehaviourEvent::Identify(
            identify::Event::Received { peer_id, info, .. },
        )) = ev
        {
            for addr in info.listen_addrs {
                swarm.behaviour_mut().kad.add_address(&peer_id, addr);
            }
        }
    }
}

// --------------------------------------------------------------------------
// helpers (duplicated from two_node.rs / three_node_kad.rs)
// --------------------------------------------------------------------------

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn,y7ke_net=info")),
        )
        .with_test_writer()
        .try_init();
}

async fn wait_for_listening(rx: &mut broadcast::Receiver<NetEvent>) -> Multiaddr {
    loop {
        match rx.recv().await {
            Ok(NetEvent::Listening { addr }) => return addr,
            Ok(_) => continue,
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => panic!("event channel closed"),
        }
    }
}

async fn wait_for_connection_to(
    rx: &mut broadcast::Receiver<NetEvent>,
    expected: PeerId,
) -> ConnectionKind {
    loop {
        match rx.recv().await {
            Ok(NetEvent::ConnectionEstablished { peer, kind, .. }) if peer == expected => {
                return kind;
            }
            Ok(_) => continue,
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => panic!("event channel closed"),
        }
    }
}

fn spawn_handshake_responder(bob: &mut NetHandle) -> JoinHandle<HandshakeReq> {
    let cmd_sender = bob.clone_command_sender();
    let mut rx = bob.try_clone_event_rx();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(NetEvent::HandshakeReceived {
                    request, channel, ..
                }) => {
                    let ch = channel.take().expect("response channel already taken");
                    cmd_sender
                        .send(NetCommand::RespondHandshake {
                            channel: ch,
                            response: HandshakeResp {
                                responder_eph_x25519_pub: [0x66; 32],
                                sig: [0xAB; 64],
                                accept: true,
                            },
                        })
                        .await
                        .expect("forward respond command");
                    return request;
                }
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => panic!("event channel closed"),
            }
        }
    })
}

fn pubkey_for_secret(secret: &[u8; 32]) -> [u8; 32] {
    libp2p_keypair_from_y7_secret(secret)
        .unwrap()
        .public()
        .try_into_ed25519()
        .unwrap()
        .to_bytes()
}
