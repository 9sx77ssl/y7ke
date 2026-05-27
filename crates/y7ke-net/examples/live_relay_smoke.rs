//! Live smoke test against a deployed Y7KE bootstrap node.
//!
//! Usage:
//!   Y7KE_BOOTSTRAP="/dns4/bootstrap1.y7v.lol/tcp/4101/p2p/12D3KooW..." \
//!     cargo run -p y7ke-net --example live_relay_smoke
//!
//! Boots a swarm with the V2-A4 relay client, dials the bootstrap,
//! waits for a `/p2p-circuit` listen address to appear (which means the
//! bootstrap accepted our reservation). Exits 0 on success, non-zero
//! after a timeout.

use std::env;
use std::time::Duration;

use libp2p::identity;
use libp2p::multiaddr::Protocol;
use libp2p::Multiaddr;
use tokio::time::timeout;
use tracing_subscriber::EnvFilter;

use y7ke_net::{build_swarm, spawn_swarm_with_bootstraps, NetEvent};

const RESERVATION_BUDGET: Duration = Duration::from_secs(30);
const POST_RESERVATION_HOLD: Duration = Duration::from_secs(90);

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("warn,y7ke_net=info,libp2p_relay=info,libp2p_swarm=info")
    });
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let bootstrap_str = env::var("Y7KE_BOOTSTRAP")
        .map_err(|_| "set Y7KE_BOOTSTRAP=<full /p2p/ multiaddr of the live bootstrap>")?;
    let bootstrap: Multiaddr = bootstrap_str.parse()?;

    let keypair = identity::Keypair::generate_ed25519();
    let local_peer = keypair.public().to_peer_id();
    println!("local peer = {local_peer}");
    println!("bootstrap  = {bootstrap}");

    let swarm = build_swarm(keypair)?;
    let mut net = spawn_swarm_with_bootstraps(swarm, vec![bootstrap.clone()]);

    let mut saw_connection = false;
    let outcome = timeout(RESERVATION_BUDGET, async {
        loop {
            match net.event_rx().recv().await {
                Ok(NetEvent::ConnectionEstablished { peer, kind }) => {
                    println!("connected peer={peer} kind={kind:?}");
                    saw_connection = true;
                }
                Ok(NetEvent::Listening { addr }) => {
                    println!("listening on {addr}");
                    if addr.iter().any(|p| matches!(p, Protocol::P2pCircuit)) {
                        return Ok::<(), Box<dyn std::error::Error>>(());
                    }
                }
                Ok(NetEvent::Error { message }) => {
                    eprintln!("net error: {message}");
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!("event rx closed: {e}");
                    return Err(e.into());
                }
            }
        }
    })
    .await;

    match outcome {
        Ok(Ok(())) => {
            println!("PASS: reservation confirmed (circuit listen address present)");
        }
        Ok(Err(e)) => {
            eprintln!("FAIL: event stream error: {e}");
            std::process::exit(2);
        }
        Err(_) => {
            eprintln!(
                "FAIL: no /p2p-circuit listen address within {:?} (saw_connection={saw_connection})",
                RESERVATION_BUDGET
            );
            std::process::exit(1);
        }
    }

    if env::var("Y7KE_HOLD").is_ok() {
        println!(
            "holding open for {:?} so you can observe reconnect / renewal behaviour…",
            POST_RESERVATION_HOLD
        );
        let _ = timeout(POST_RESERVATION_HOLD, async {
            loop {
                if let Ok(ev) = net.event_rx().recv().await {
                    println!("event: {ev:?}");
                }
            }
        })
        .await;
    }

    Ok(())
}
