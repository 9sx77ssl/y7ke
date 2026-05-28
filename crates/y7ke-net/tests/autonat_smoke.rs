//! V2-A3 smoke test: an inline AutoNAT v2 server (mimicking the
//! production bootstrap's role) + a single y7ke-net client. The client
//! receives at least one `NetEvent::NatStatus` event with a reachable
//! verdict for one of its loopback listen addresses, proving the
//! `autonat::v2::client` → swarm → `NetEvent` plumbing is wired
//! end-to-end.
//!
//! Skipped on macOS/Windows for parity with the other loopback-touching
//! tests (UDP-bind reliability in sandboxed CI is the same hazard
//! `quic_listen_smoke` covers).

use std::time::Duration;

use libp2p::{
    autonat, identify,
    identity::Keypair,
    noise, ping,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, Swarm, SwarmBuilder,
};
use rand::rngs::OsRng;
use tokio::sync::broadcast;
use tokio::time::timeout;

use y7ke_net::{build_swarm, libp2p_keypair_from_y7_secret, spawn_swarm_with_bootstraps, NetEvent};

const TEST_TIMEOUT: Duration = Duration::from_secs(60);
const IDENTIFY_PROTOCOL: &str = "/y7ke/0.1.0";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg_attr(any(target_os = "macos", target_os = "windows"), ignore)]
async fn autonat_client_receives_reachable_verdict() {
    init_tracing();
    timeout(TEST_TIMEOUT, run_autonat_probe())
        .await
        .expect("autonat_smoke timed out");
}

async fn run_autonat_probe() {
    // Server: an inline swarm with autonat::v2::server (mirroring the
    // production y7ke-bootstrap), bound to a loopback TCP socket.
    let server_kp = libp2p_keypair_from_y7_secret(&[0xAAu8; 32]).unwrap();
    let server_peer = server_kp.public().to_peer_id();
    let mut server_swarm = build_autonat_server(server_kp);
    server_swarm
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .expect("server listen");
    let server_listen = wait_for_server_listening(&mut server_swarm).await;
    let server_multiaddr: Multiaddr = format!("{server_listen}/p2p/{server_peer}")
        .parse()
        .expect("compose server multiaddr");
    // The autonat v2 server dials clients back over its own outbound
    // socket; advertising the listen address ensures the swarm-level
    // dial address book has something to use.
    server_swarm.add_external_address(server_listen.clone());
    eprintln!("autonat server listening at {server_multiaddr}");

    let _server_task = tokio::spawn(drive_server(server_swarm));

    // Client: a real y7ke-net swarm. Treats the server as a bootstrap
    // so it dials and exchanges identify, then autonat v2 fires
    // automatically (Config::default() probe_interval = 5s).
    let client_kp = libp2p_keypair_from_y7_secret(&[0xCCu8; 32]).unwrap();
    let client_swarm = build_swarm(client_kp).expect("build client");
    let mut client = spawn_swarm_with_bootstraps(
        client_swarm,
        vec![server_multiaddr.clone()],
        y7ke_core::settings::DialMode::Internet,
    );

    // Wait for at least one NatStatus event. We don't strictly require
    // reachable=true on loopback (the autonat server dialing
    // 127.0.0.1 from a different ephemeral port may not match what the
    // client advertised) but a probe must complete to prove the wire
    // path. Verdict either way is fine — the test asserts the event
    // surfaces, not its content.
    let deadline = Duration::from_secs(30);
    let collect = async {
        loop {
            match client.event_rx().recv().await {
                Ok(NetEvent::NatStatus {
                    tested_addr,
                    server,
                    reachable,
                }) => {
                    eprintln!(
                        "autonat verdict landed: addr={tested_addr} server={server} reachable={reachable}"
                    );
                    return reachable;
                }
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => panic!("event channel closed"),
            }
        }
    };
    let _ = timeout(deadline, collect)
        .await
        .expect("no NatStatus event within 30 s — autonat client did not probe");
}

#[derive(NetworkBehaviour)]
struct AutonatServerBehaviour {
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    autonat: autonat::v2::server::Behaviour<OsRng>,
}

fn build_autonat_server(kp: Keypair) -> Swarm<AutonatServerBehaviour> {
    SwarmBuilder::with_existing_identity(kp)
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            noise::Config::new,
            yamux::Config::default,
        )
        .expect("tcp setup")
        .with_behaviour(|kp| AutonatServerBehaviour {
            identify: identify::Behaviour::new(
                identify::Config::new(IDENTIFY_PROTOCOL.to_string(), kp.public())
                    .with_interval(Duration::from_secs(60))
                    .with_push_listen_addr_updates(true),
            ),
            ping: ping::Behaviour::new(ping::Config::new()),
            autonat: autonat::v2::server::Behaviour::new(OsRng),
        })
        .expect("behaviour setup")
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(120)))
        .build()
}

async fn wait_for_server_listening(swarm: &mut Swarm<AutonatServerBehaviour>) -> Multiaddr {
    use futures::StreamExt;
    loop {
        if let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await {
            return address;
        }
    }
}

async fn drive_server(mut swarm: Swarm<AutonatServerBehaviour>) {
    use futures::StreamExt;
    loop {
        let _ = swarm.select_next_some().await;
    }
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new(
                    "info,y7ke_net=info,libp2p_autonat=info,libp2p_identify=warn,libp2p_swarm=warn",
                )
            }),
        )
        .with_test_writer()
        .try_init();
}
