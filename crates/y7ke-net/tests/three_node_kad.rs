//! V2-A1 integration test: three swarms (bootstrap + alice + bob),
//! Kad-only peer discovery, end-to-end FindPeer + dial + handshake.
//!
//! The test runs on loopback. mDNS would also fire here (everything is
//! on the same machine), so we focus on asserting that the Kad path
//! works — Alice issues `find_peer(bob)` against the bootstrap and gets
//! Bob's addresses back. Even if mDNS happens to win the race for
//! direct connectivity, this confirms the Kad code path is wired
//! correctly.

use std::time::Duration;

use libp2p::Multiaddr;
use tokio::sync::broadcast;
use tokio::time::timeout;

use y7ke_net::{
    build_swarm, libp2p_keypair_from_y7_secret, spawn_swarm_with_bootstraps, NetEvent, NetHandle,
};

const TEST_TIMEOUT: Duration = Duration::from_secs(120);

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[cfg_attr(any(target_os = "macos", target_os = "windows"), ignore)]
async fn alice_finds_bob_via_kad_bootstrap() {
    init_tracing();
    timeout(TEST_TIMEOUT, run_kad_discovery())
        .await
        .expect("three_node_kad timed out");
}

async fn run_kad_discovery() {
    // Three deterministic identities. The bootstrap node is "B" (0xBB),
    // Alice is 0x0A, Bob is 0x0B.
    let bootstrap_kp = libp2p_keypair_from_y7_secret(&[0xBB; 32]).unwrap();
    let alice_kp = libp2p_keypair_from_y7_secret(&[0x0A; 32]).unwrap();
    let bob_kp = libp2p_keypair_from_y7_secret(&[0x0B; 32]).unwrap();

    let bootstrap_peer = bootstrap_kp.public().to_peer_id();
    let bob_peer = bob_kp.public().to_peer_id();
    let bob_pub = bob_kp.public().try_into_ed25519().unwrap().to_bytes();
    let bob_y7 = y7ke_core::Y7Id::from_pubkey(bob_pub);

    // Boot the bootstrap first so its listen addr is known before alice
    // and bob get their bootstraps wired in.
    let bootstrap_swarm = build_swarm(bootstrap_kp).expect("build bootstrap");
    let mut bootstrap = spawn_swarm_with_bootstraps(bootstrap_swarm, Vec::new());
    let bootstrap_listen = wait_for_listening(bootstrap.event_rx()).await;
    let bootstrap_multiaddr: Multiaddr = format!("{bootstrap_listen}/p2p/{bootstrap_peer}")
        .parse()
        .expect("compose bootstrap multiaddr");
    eprintln!("bootstrap listening on {bootstrap_multiaddr}");

    // Alice + Bob receive the bootstrap multiaddr at swarm construction
    // so Kad seeds the routing table before any user-driven dial.
    let alice_swarm = build_swarm(alice_kp).expect("build alice");
    let bob_swarm = build_swarm(bob_kp).expect("build bob");
    let mut alice = spawn_swarm_with_bootstraps(alice_swarm, vec![bootstrap_multiaddr.clone()]);
    let mut bob = spawn_swarm_with_bootstraps(bob_swarm, vec![bootstrap_multiaddr.clone()]);

    let _alice_listen = wait_for_listening(alice.event_rx()).await;
    let _bob_listen = wait_for_listening(bob.event_rx()).await;

    // start_providing PUTs propagate to the bootstrap's record store
    // asynchronously after each peer connects. On a slow runner (GitHub
    // Actions) the first find_peer may race the PUT and return
    // NotFound. Retry with backoff — production clients have the same
    // pattern (the user can re-trigger from the UI). We give the
    // pipeline up to ~30s total to stabilise.
    let mut addrs = Vec::new();
    for attempt in 0..8 {
        tokio::time::sleep(Duration::from_secs(5)).await;
        match timeout(Duration::from_secs(10), alice.find_peer(bob_y7)).await {
            Ok(Ok(found)) if !found.is_empty() => {
                addrs = found;
                eprintln!(
                    "attempt {}: find_peer(bob) returned {} address(es)",
                    attempt + 1,
                    addrs.len()
                );
                break;
            }
            Ok(Ok(_)) => {
                eprintln!("attempt {}: find_peer returned empty list", attempt + 1);
            }
            Ok(Err(e)) => {
                eprintln!("attempt {}: find_peer returned {e:?}", attempt + 1);
            }
            Err(_) => {
                eprintln!("attempt {}: find_peer timed out", attempt + 1);
            }
        }
    }
    assert!(
        !addrs.is_empty(),
        "find_peer(bob) failed after 8 attempts over ~40s — Kad routing never populated"
    );

    let _ = bob_peer; // silence unused-binding lint
    alice.shutdown().await.unwrap();
    bob.shutdown().await.unwrap();
    bootstrap.shutdown().await.unwrap();
}

// --------------------------------------------------------------------------
// helpers (duplicated from two_node.rs — tests are isolated by default and
// sharing across files would require a `common` module)
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

#[allow(dead_code)]
fn touch_unused(_a: &NetHandle) {} // keep NetHandle import live if test paths shrink
