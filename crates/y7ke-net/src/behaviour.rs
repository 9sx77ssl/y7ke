//! The Y7KE composite `NetworkBehaviour`.
//!
//! V1 aggregates six sub-behaviours behind a single derived
//! `NetworkBehaviour`. The derive macro generates [`Y7BehaviourEvent`],
//! whose variants the swarm task pattern-matches in `swarm.rs`.

use std::time::Duration;

use libp2p::{
    dcutr, identify,
    identity::Keypair,
    kad::{self, store::MemoryStore},
    mdns, ping, relay,
    request_response::{self, ProtocolSupport},
    swarm::NetworkBehaviour,
};

use crate::protocol::{
    HandshakeReq, HandshakeResp, MsgReq, MsgResp, SyncReq, SyncResp, HANDSHAKE_PROTOCOL,
    IDENTIFY_AGENT_VERSION, IDENTIFY_PROTOCOL_VERSION, KAD_PROTOCOL, MSG_PROTOCOL, SYNC_PROTOCOL,
};

/// Composite network behaviour driving the Y7KE swarm.
///
/// Field order matters only insofar as the derive macro generates the
/// [`Y7BehaviourEvent`] enum variants in this declared order.
#[derive(NetworkBehaviour)]
pub struct Y7Behaviour {
    /// Exchanges peer metadata (protocol set, listen addresses, public key)
    /// after each new connection. We use the `Received` event to harvest
    /// each peer's Ed25519 public key, which is what links a libp2p
    /// `PeerId` back to a `y7ke-core::Y7Id`.
    pub identify: identify::Behaviour,
    /// Liveness probe + RTT measurement. Defaults are fine.
    pub ping: ping::Behaviour,
    /// LAN discovery — Y7KE V1's sole discovery mechanism.
    pub mdns: mdns::tokio::Behaviour,
    /// `/y7ke/handshake/1.0.0`.
    pub handshake: request_response::cbor::Behaviour<HandshakeReq, HandshakeResp>,
    /// `/y7ke/msg/1.0.0`.
    pub msg: request_response::cbor::Behaviour<MsgReq, MsgResp>,
    /// `/y7ke/sync/1.0.0`.
    pub sync: request_response::cbor::Behaviour<SyncReq, SyncResp>,
    /// Kademlia DHT — V2-A1. Used for internet-mode peer discovery via
    /// bootstrap nodes. Server mode so routing replicates between all
    /// participating peers; each client `start_providing`s its own key so
    /// other clients can look it up via `get_providers`.
    pub kad: kad::Behaviour<MemoryStore>,
    /// Circuit Relay v2 client — V2-A4. Lets us reserve a slot at each
    /// configured bootstrap and accept inbound dials via `p2p-circuit`
    /// when both peers are NAT'd.
    pub relay_client: relay::client::Behaviour,
    /// DCUtR — V2-A5. Hole-punches a direct connection once a `Relayed`
    /// link to the peer exists and identify has advertised observed
    /// addresses on both sides. The upgraded direct stream re-runs Noise
    /// XX with the same Ed25519 keypair, so the libp2p PeerId still
    /// matches — no app-layer revalidation is required.
    pub dcutr: dcutr::Behaviour,
}

impl Y7Behaviour {
    /// Build a fresh `Y7Behaviour` configured with sensible V1 defaults.
    ///
    /// `local_keypair` is taken by reference because identify's `Config`
    /// only needs the public key (and the derive macro takes ownership of
    /// the values it builds).
    pub fn new(
        local_keypair: &Keypair,
        relay_client: relay::client::Behaviour,
    ) -> Result<Self, std::io::Error> {
        let local_peer_id = local_keypair.public().to_peer_id();

        let identify = identify::Behaviour::new(
            identify::Config::new(
                IDENTIFY_PROTOCOL_VERSION.to_string(),
                local_keypair.public(),
            )
            .with_agent_version(IDENTIFY_AGENT_VERSION.to_string())
            .with_interval(Duration::from_secs(60)),
        );

        let ping = ping::Behaviour::new(
            ping::Config::new()
                .with_interval(Duration::from_secs(20))
                .with_timeout(Duration::from_secs(10)),
        );

        // 30 s mDNS query interval — the default is 5 minutes which is far
        // too slow for a chat app where users expect to be discovered
        // within seconds of opening the app on the same Wi-Fi.
        let mdns = mdns::tokio::Behaviour::new(
            mdns::Config {
                query_interval: Duration::from_secs(30),
                ..mdns::Config::default()
            },
            local_peer_id,
        )?;

        let rr_config =
            request_response::Config::default().with_request_timeout(Duration::from_secs(15));

        let handshake = request_response::cbor::Behaviour::<HandshakeReq, HandshakeResp>::new(
            [(HANDSHAKE_PROTOCOL, ProtocolSupport::Full)],
            rr_config.clone(),
        );
        let msg = request_response::cbor::Behaviour::<MsgReq, MsgResp>::new(
            [(MSG_PROTOCOL, ProtocolSupport::Full)],
            rr_config.clone(),
        );
        let sync = request_response::cbor::Behaviour::<SyncReq, SyncResp>::new(
            [(SYNC_PROTOCOL, ProtocolSupport::Full)],
            rr_config,
        );

        let mut kad_cfg = kad::Config::new(KAD_PROTOCOL);
        // 5-minute periodic bootstrap keeps routing fresh without hammering
        // the network. Default is 5 min already in newer libp2p but pin it
        // here so future libp2p bumps don't change behaviour silently.
        kad_cfg.set_periodic_bootstrap_interval(Some(Duration::from_secs(300)));
        let mut kad =
            kad::Behaviour::with_config(local_peer_id, MemoryStore::new(local_peer_id), kad_cfg);
        kad.set_mode(Some(kad::Mode::Server));

        let dcutr = dcutr::Behaviour::new(local_peer_id);

        Ok(Self {
            identify,
            ping,
            mdns,
            handshake,
            msg,
            sync,
            kad,
            relay_client,
            dcutr,
        })
    }
}
