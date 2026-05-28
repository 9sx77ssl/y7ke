#!/usr/bin/env bash
# Symmetric-NAT simulation for Y7KE — DCUtR fallback (Phase 3.4).
#
# Two clients sit behind separate MASQUERADE NAT routers around a shared
# "internet" bridge with a public relay/bootstrap:
#
#   cliA(192.168.10.2)─natA(192.168.10.1│10.0.0.2)─┐
#                                                   ├─ br0(10.0.0.1 = relay/bootstrap)
#   cliB(192.168.20.2)─natB(192.168.20.1│10.0.0.3)─┘
#
# IMPORTANT — what this actually simulates: stock Linux iptables/nftables
# MASQUERADE is NOT endpoint-independent. When a client opens flows from
# one source port to several destinations, the kernel allocates a *fresh*
# external port per destination — i.e. it behaves as a SYMMETRIC NAT.
# (Verified here: cliA's QUIC port maps to one external port toward the
# relay and a different one toward the peer; see the conntrack dump.)
# Full-cone / endpoint-independent NAT needs the out-of-tree FULLCONENAT
# kernel module, which we deliberately don't require.
#
# DCUtR cannot hole-punch a symmetric NAT — the peer dials the relay-
# observed port, but the punch egresses from a different one. So the
# CORRECT, asserted behaviour is: connect over relay, ATTEMPT the
# upgrade, fail to punch, and stay cleanly on the relay (the
# "stable relay beats reconnect chaos" invariant). The DCUtR *success*
# path is covered by crates/y7ke-net/tests/v2_dcutr_smoke.rs (loopback);
# an end-to-end NAT punch needs real-world NATs (the live smoke, #91).
#
# Run as root:  sudo bash scripts/nat-sim/run-symmetric.sh
# Build first:  cargo build -p y7ke-net --example nat_sim_node
set -uo pipefail

BOOT_BIN="${BOOT_BIN:-/home/rsz/Desktop/y7ke-bootstrap/target/release/y7ke-bootstrap}"
NODE_BIN="${NODE_BIN:-/home/rsz/Desktop/Y7KE/target/debug/examples/nat_sim_node}"
RUN=/tmp/y7ke-natsim-sym
SEED_A="0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a"
SEED_B="0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b"
DCUTR_WAIT="${DCUTR_WAIT:-22}"   # seconds to watch DCUtR attempt + give up

if [[ $EUID -ne 0 ]]; then echo "must run as root (sudo)"; exit 2; fi
for b in "$BOOT_BIN" "$NODE_BIN"; do [[ -x "$b" ]] || { echo "missing binary: $b"; exit 2; }; done

NSES=(cliA natA cliB natB pub)
cleanup() {
  set +e
  pkill -f nat_sim_node 2>/dev/null
  [[ -n "${BOOT_PID:-}" ]] && kill "$BOOT_PID" 2>/dev/null
  [[ -n "${RESP_PID:-}" ]] && kill "$RESP_PID" 2>/dev/null
  for ns in "${NSES[@]}"; do ip netns del "$ns" 2>/dev/null; done
}
trap cleanup EXIT
cleanup
mkdir -p "$RUN"; rm -f "$RUN"/*.log "$RUN"/*.key

echo "== building NAT topology =="
for ns in "${NSES[@]}"; do ip netns add "$ns"; ip -n "$ns" link set lo up; done
ip -n pub link add br0 type bridge
ip -n pub addr add 10.0.0.1/24 dev br0
ip -n pub link set br0 up

# natA: WAN 10.0.0.2 on the bridge, LAN 192.168.10.1 to cliA.
ip link add pa type veth peer name na_ext
ip link set pa netns pub;  ip link set na_ext netns natA
ip -n pub link set pa master br0; ip -n pub link set pa up
ip -n natA addr add 10.0.0.2/24 dev na_ext; ip -n natA link set na_ext up
ip link add na_int type veth peer name a_eth
ip link set na_int netns natA; ip link set a_eth netns cliA
ip -n natA addr add 192.168.10.1/24 dev na_int; ip -n natA link set na_int up
ip -n cliA addr add 192.168.10.2/24 dev a_eth; ip -n cliA link set a_eth up
ip -n cliA route add default via 192.168.10.1

# natB: WAN 10.0.0.3 on the bridge, LAN 192.168.20.1 to cliB.
ip link add pb type veth peer name nb_ext
ip link set pb netns pub;  ip link set nb_ext netns natB
ip -n pub link set pb master br0; ip -n pub link set pb up
ip -n natB addr add 10.0.0.3/24 dev nb_ext; ip -n natB link set nb_ext up
ip link add nb_int type veth peer name b_eth
ip link set nb_int netns natB; ip link set b_eth netns cliB
ip -n natB addr add 192.168.20.1/24 dev nb_int; ip -n natB link set nb_int up
ip -n cliB addr add 192.168.20.2/24 dev b_eth; ip -n cliB link set b_eth up
ip -n cliB route add default via 192.168.20.1

for r in natA natB; do
  ip netns exec "$r" sysctl -qw net.ipv4.ip_forward=1
  ip netns exec "$r" sysctl -qw net.ipv4.conf.all.rp_filter=0
  ip netns exec "$r" sysctl -qw net.ipv4.conf.default.rp_filter=0
done
ip netns exec natA sysctl -qw net.ipv4.conf.na_ext.rp_filter=0
ip netns exec natA sysctl -qw net.ipv4.conf.na_int.rp_filter=0
ip netns exec natB sysctl -qw net.ipv4.conf.nb_ext.rp_filter=0
ip netns exec natB sysctl -qw net.ipv4.conf.nb_int.rp_filter=0
ip netns exec cliA sysctl -qw net.ipv4.conf.all.rp_filter=0
ip netns exec cliB sysctl -qw net.ipv4.conf.all.rp_filter=0
ip netns exec natA iptables -t nat -A POSTROUTING -s 192.168.10.0/24 -o na_ext -j MASQUERADE
ip netns exec natB iptables -t nat -A POSTROUTING -s 192.168.20.0/24 -o nb_ext -j MASQUERADE
ip netns exec natA iptables -A FORWARD -i na_int -o na_ext -j ACCEPT
ip netns exec natA iptables -A FORWARD -i na_ext -o na_int -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
ip netns exec natB iptables -A FORWARD -i nb_int -o nb_ext -j ACCEPT
ip netns exec natB iptables -A FORWARD -i nb_ext -o nb_int -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT

ip netns exec cliA ping -c1 -W2 10.0.0.1 >/dev/null && echo "  cliA → relay OK" || { echo "  cliA → relay FAILED"; exit 1; }
ip netns exec cliB ping -c1 -W2 10.0.0.1 >/dev/null && echo "  cliB → relay OK" || { echo "  cliB → relay FAILED"; exit 1; }

echo "== starting relay/bootstrap in pub (QUIC on 4101) =="
ip netns exec pub env Y7KE_BOOTSTRAP_EXTERNAL_ADDR="/ip4/10.0.0.1/udp/4101/quic-v1" \
  "$BOOT_BIN" --listen-port 4101 --key-path "$RUN/boot.key" >"$RUN/boot.log" 2>&1 &
BOOT_PID=$!
BOOT_ID=""
for _ in $(seq 1 50); do
  BOOT_ID=$(grep -oP 'PeerId:\s*\K\S+' "$RUN/boot.log" 2>/dev/null | head -1)
  [[ -n "$BOOT_ID" ]] && break; sleep 0.2
done
[[ -n "$BOOT_ID" ]] || { echo "bootstrap PeerId never printed"; cat "$RUN/boot.log"; exit 1; }
echo "  bootstrap PeerId = $BOOT_ID"
BOOT_ADDR="/ip4/10.0.0.1/udp/4101/quic-v1/p2p/$BOOT_ID"

echo "== starting responder in cliA (behind natA) =="
ip netns exec cliA env RUST_LOG="warn,y7ke_net=info,libp2p_dcutr=info" \
  Y7KE_SEED="$SEED_A" Y7KE_ROLE=responder Y7KE_BOOTSTRAP="$BOOT_ADDR" \
  "$NODE_BIN" >"$RUN/respA.log" 2>&1 &
RESP_PID=$!
A_ID=""
for _ in $(seq 1 50); do
  A_ID=$(grep -oP 'PEER_ID=\K\S+' "$RUN/respA.log" 2>/dev/null | head -1)
  [[ -n "$A_ID" ]] && break; sleep 0.2
done
[[ -n "$A_ID" ]] || { echo "responder PeerId never printed"; cat "$RUN/respA.log"; exit 1; }
for _ in $(seq 1 100); do grep -q '^RESERVED' "$RUN/respA.log" && break; sleep 0.3; done
grep -q '^RESERVED' "$RUN/respA.log" || { echo "responder never reserved"; tail "$RUN/respA.log"; exit 1; }
echo "  responder $A_ID RESERVED"

# Snapshot NAT conntrack mid-punch to prove the symmetric (per-dest port)
# mapping that defeats the hole punch.
(
  sleep 8
  for r in natA natB; do
    echo "=== $r udp conntrack (excl. relay :4101) ==="
    ip netns exec "$r" cat /proc/net/nf_conntrack 2>/dev/null | grep -i udp | grep -v 'dport=4101'
  done
) >"$RUN/conntrack.log" 2>&1 &

echo "== starting initiator in cliB (behind natB), ${DCUTR_WAIT}s DCUtR window =="
ip netns exec cliB env RUST_LOG="warn,y7ke_net=info,libp2p_dcutr=info" \
  Y7KE_SEED="$SEED_B" Y7KE_ROLE=initiator Y7KE_HOLD_SECS="$DCUTR_WAIT" \
  Y7KE_BOOTSTRAP="$BOOT_ADDR" Y7KE_TARGET="$BOOT_ADDR/p2p-circuit/p2p/$A_ID" \
  "$NODE_BIN" >"$RUN/initB.log" 2>&1
INIT_RC=$?

echo "== result =="
echo "--- NAT conntrack mid-punch (same client port → different external ports = symmetric) ---"
cat "$RUN/conntrack.log" 2>/dev/null | sed 's/^/    /'
HS=$(grep -q '^HANDSHAKE_OK' "$RUN/initB.log" && echo yes || echo no)
RELAYED=$(grep -q '^KIND=Relayed' "$RUN/initB.log" && echo yes || echo no)
FELLBACK=$(grep -q 'direct upgrade failed (staying on relay)' "$RUN/respA.log" "$RUN/initB.log" && echo yes || echo no)
UPGRADED=$(grep -qE '^UPGRADED=Direct' "$RUN/initB.log" "$RUN/respA.log" && echo yes || echo no)
echo "  handshake over relay                        : $HS"
echo "  connection kind Relayed                     : $RELAYED"
echo "  DCUtR attempted + fell back cleanly to relay: $FELLBACK"
echo "  DCUtR punched (expected NO under symmetric) : $UPGRADED"

if [[ "$UPGRADED" == yes ]]; then
  echo "PASS (bonus): DCUtR punched — this kernel's NAT is endpoint-independent"
  exit 0
elif [[ "$HS" == yes && "$RELAYED" == yes && "$FELLBACK" == yes && $INIT_RC -eq 0 ]]; then
  echo "PASS: symmetric NAT — relay connect + handshake OK, DCUtR tried and fell back cleanly (relay held)"
  exit 0
else
  echo "FAIL: rc=$INIT_RC HS=$HS Relayed=$RELAYED fellback=$FELLBACK"
  grep -E '^KIND|^UPGRADED|^HANDSHAKE|^FAIL|dcutr' "$RUN/initB.log" | tail -10
  exit 1
fi
