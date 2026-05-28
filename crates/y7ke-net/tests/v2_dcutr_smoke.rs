//! V2-A5 smoke test: a relay-bootstrap + Alice + Bob both reserve at the
//! bootstrap; Alice dials Bob through `/p2p-circuit`; we then wait for
//! DCUtR to fire `NetEvent::ConnectionUpgraded` with `kind = Direct` on
//! at least one side (libp2p delivers the upgrade event to whichever
//! peer wins the simultaneous-open race; both sides see the new direct
//! connection, but only the initiator reliably gets the `dcutr::Event`).
//!
//! Mirrors the relay bootstrap setup from `four_node_relay.rs` — the
//! production `Y7Behaviour` is the relay *client*, so the bootstrap
//! still has to inline its own relay-server behaviour.
//!
//! Skipped on macOS/Windows for parity with other LAN-touching tests.

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
use tokio::time::timeout;

use y7ke_core::ConnectionKind;
use y7ke_net::{build_swarm, libp2p_keypair_from_y7_secret, spawn_swarm_with_bootstraps, NetEvent};

const TEST_TIMEOUT: Duration = Duration::from_secs(120);
const KAD_PROTOCOL: StreamProtocol = StreamProtocol::new("/y7ke/kad/1.0.0");
const IDENTIFY_PROTOCOL: &str = "/y7ke/0.1.0";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg_attr(any(target_os = "macos", target_os = "windows"), ignore)]
async fn dcutr_upgrades_relay_to_direct() {
    init_tracing();
    timeout(TEST_TIMEOUT, run_dcutr_upgrade())
        .await
        .expect("v2_dcutr_smoke timed out");
}

async fn run_dcutr_upgrade() {
    let bootstrap_kp = libp2p_keypair_from_y7_secret(&[0xCC; 32]).unwrap();
    let alice_kp = libp2p_keypair_from_y7_secret(&[0xA5; 32]).unwrap();
    let bob_kp = libp2p_keypair_from_y7_secret(&[0xB5; 32]).unwrap();

    let bootstrap_peer = bootstrap_kp.public().to_peer_id();
    let alice_peer = alice_kp.public().to_peer_id();
    let bob_peer = bob_kp.public().to_peer_id();

    let mut bootstrap_swarm = build_relay_bootstrap(bootstrap_kp);
    bootstrap_swarm
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .expect("bootstrap listen");
    let bootstrap_listen = wait_for_bootstrap_listening(&mut bootstrap_swarm).await;
    let bootstrap_multiaddr: Multiaddr = format!("{bootstrap_listen}/p2p/{bootstrap_peer}")
        .parse()
        .expect("compose bootstrap multiaddr");
    bootstrap_swarm.add_external_address(bootstrap_listen.clone());
    eprintln!("bootstrap listening on {bootstrap_multiaddr}");

    let _bootstrap_task = tokio::spawn(drive_bootstrap(bootstrap_swarm));

    let alice_swarm = build_swarm(alice_kp).expect("build alice");
    let bob_swarm = build_swarm(bob_kp).expect("build bob");
    let mut alice = spawn_swarm_with_bootstraps(alice_swarm, vec![bootstrap_multiaddr.clone()]);
    let mut bob = spawn_swarm_with_bootstraps(bob_swarm, vec![bootstrap_multiaddr.clone()]);

    let _alice_listen = wait_for_listening(alice.event_rx()).await;
    let _bob_listen = wait_for_listening(bob.event_rx()).await;

    // Give both clients ~5s to register a reservation on the bootstrap.
    tokio::time::sleep(Duration::from_secs(5)).await;

    let circuit_to_bob: Multiaddr = bootstrap_multiaddr
        .clone()
        .with(Protocol::P2pCircuit)
        .with(Protocol::P2p(bob_peer));

    let mut alice_events = alice.try_clone_event_rx();
    let mut bob_events = bob.try_clone_event_rx();

    eprintln!("alice.dial_address({circuit_to_bob}) — establishing relayed link");
    alice
        .dial_address(circuit_to_bob.clone())
        .await
        .expect("alice dial_address");

    // Confirm the relay path lit up first.
    let (alice_kind, bob_kind) = timeout(
        Duration::from_secs(20),
        futures::future::join(
            wait_for_connection_to(&mut alice_events, bob_peer),
            wait_for_connection_to(&mut bob_events, alice_peer),
        ),
    )
    .await
    .expect("never observed relayed ConnectionEstablished on both sides");
    assert_eq!(alice_kind, ConnectionKind::Relayed, "alice→bob via relay");
    assert_eq!(bob_kind, ConnectionKind::Relayed, "bob←alice via relay");

    // DCUtR should now race to a direct upgrade. The initiator side
    // (Alice in this layout) is the one that reliably observes the
    // `dcutr::Event::Result(Ok)`. We accept the test as passing if
    // EITHER side reports `ConnectionUpgraded { kind: Direct }` within
    // 30 s; on loopback this typically takes 1–3 s.
    let alice_upgraded = wait_for_upgraded_to(&mut alice_events, bob_peer);
    let bob_upgraded = wait_for_upgraded_to(&mut bob_events, alice_peer);
    let upgrade = timeout(
        Duration::from_secs(30),
        futures::future::select(Box::pin(alice_upgraded), Box::pin(bob_upgraded)),
    )
    .await
    .expect("DCUtR upgrade did not fire within 30s");
    let kind = match upgrade {
        futures::future::Either::Left((k, _)) => k,
        futures::future::Either::Right((k, _)) => k,
    };
    assert_eq!(
        kind,
        ConnectionKind::Direct,
        "ConnectionUpgraded must carry kind=Direct"
    );

    alice.shutdown().await.unwrap();
    bob.shutdown().await.unwrap();
}

// --------------------------------------------------------------------------
// Relay-bootstrap node — identical to four_node_relay.rs
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
// helpers
// --------------------------------------------------------------------------

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("warn,y7ke_net=info,libp2p_dcutr=info")
            }),
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
            Ok(NetEvent::ConnectionEstablished { peer, kind }) if peer == expected => {
                return kind;
            }
            Ok(_) => continue,
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => panic!("event channel closed"),
        }
    }
}

async fn wait_for_upgraded_to(
    rx: &mut broadcast::Receiver<NetEvent>,
    expected: PeerId,
) -> ConnectionKind {
    loop {
        match rx.recv().await {
            Ok(NetEvent::ConnectionUpgraded { peer, kind }) if peer == expected => {
                return kind;
            }
            Ok(_) => continue,
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => panic!("event channel closed"),
        }
    }
}
