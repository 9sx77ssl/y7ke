//! V2-A5/A6 dial-priority ordering.
//!
//! When discovery returns a mix of multiaddrs we want the swarm to try
//! the cheapest direct path first and only fall back to relay if nothing
//! else worked. The order is:
//!
//! 1. Direct + QUIC (`/udp/.../quic-v1`)
//! 2. Direct + TCP
//! 3. Relay multiaddrs (containing `/p2p-circuit`)
//!
//! Within each tier the original order is preserved (stable sort).
//!
//! "Direct" here means *not* a circuit address — LAN vs Internet
//! prioritisation happens at the higher layer via the dial-mode filter
//! upstream of this call.

use libp2p::{multiaddr::Protocol, Multiaddr};

/// Sort `addrs` into preferred dial order. See module docs for the tiers.
pub fn sort_addrs_for_dial(mut addrs: Vec<Multiaddr>) -> Vec<Multiaddr> {
    addrs.sort_by_key(tier_for);
    addrs
}

fn tier_for(addr: &Multiaddr) -> u8 {
    let mut is_circuit = false;
    let mut is_quic = false;
    for proto in addr.iter() {
        match proto {
            Protocol::P2pCircuit => is_circuit = true,
            Protocol::QuicV1 | Protocol::Quic => is_quic = true,
            _ => {}
        }
    }
    match (is_circuit, is_quic) {
        (true, _) => 3,
        (false, true) => 1,
        (false, false) => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(s: &str) -> Multiaddr {
        s.parse().unwrap()
    }

    #[test]
    fn quic_beats_tcp_beats_relay() {
        let tcp = m("/ip4/1.2.3.4/tcp/4101");
        let quic = m("/ip4/1.2.3.4/udp/4102/quic-v1");
        let relay = m("/ip4/9.9.9.9/tcp/4101/p2p/12D3KooWEVq9A1w4xk1paGxywwPNy4vz8D92wxE4XKBh8DpA8fSo/p2p-circuit");
        let sorted = sort_addrs_for_dial(vec![relay.clone(), tcp.clone(), quic.clone()]);
        assert_eq!(sorted, vec![quic, tcp, relay]);
    }

    #[test]
    fn stable_within_tier() {
        let a = m("/ip4/1.0.0.1/tcp/1");
        let b = m("/ip4/1.0.0.2/tcp/2");
        let sorted = sort_addrs_for_dial(vec![a.clone(), b.clone()]);
        assert_eq!(sorted, vec![a, b]);
    }

    #[test]
    fn empty_input_returns_empty() {
        assert!(sort_addrs_for_dial(vec![]).is_empty());
    }
}
