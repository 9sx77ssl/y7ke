#!/usr/bin/env bash
# QUIC connection-migration experiment for Y7KE (Phase 3.4).
#
# Two nodes on one /24 segment, directly connected over QUIC:
#
#   ns1 responder 10.0.0.1  <── veth ──>  ns2 initiator 10.0.0.2 → 10.0.0.22
#
# After the QUIC connection + handshake are live, the initiator's IP is
# swapped (10.0.0.2 → 10.0.0.22) underneath the connection — the
# Wi-Fi↔cellular handoff in miniature. We watch the per-connection
# ConnectionId (carried on NetEvent since the per-connection-tracking
# commit) to answer RFC 9000 §9 empirically:
#   * same conn id persists + probes keep succeeding  → MIGRATED
#   * a new conn id appears after the swap             → dropped + re-dialed
#   * probes fail with no new conn id                  → dropped, no recovery
#
# Run as root:  sudo bash scripts/nat-sim/run-quic-migration.sh
# Build first:  cargo build -p y7ke-net --example quic_migrate_node
set -uo pipefail

NODE_BIN="${NODE_BIN:-/home/rsz/Desktop/Y7KE/target/debug/examples/quic_migrate_node}"
RUN=/tmp/y7ke-quicmig
SEED_R="0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c"
SEED_I="0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d"
HOLD="${HOLD:-42}"

if [[ $EUID -ne 0 ]]; then echo "must run as root (sudo)"; exit 2; fi
[[ -x "$NODE_BIN" ]] || { echo "missing binary: $NODE_BIN"; exit 2; }

cleanup() {
  set +e
  pkill -f quic_migrate_node 2>/dev/null
  ip netns del ns1 2>/dev/null; ip netns del ns2 2>/dev/null
}
trap cleanup EXIT
cleanup
mkdir -p "$RUN"; rm -f "$RUN"/*.log

echo "== building segment =="
ip netns add ns1; ip netns add ns2
ip link add v1 type veth peer name v2
ip link set v1 netns ns1; ip link set v2 netns ns2
ip -n ns1 addr add 10.0.0.1/24 dev v1; ip -n ns1 link set v1 up; ip -n ns1 link set lo up
ip -n ns2 addr add 10.0.0.2/24 dev v2; ip -n ns2 link set v2 up; ip -n ns2 link set lo up
ip netns exec ns1 sysctl -qw net.ipv4.conf.all.rp_filter=0
ip netns exec ns2 sysctl -qw net.ipv4.conf.all.rp_filter=0
ip netns exec ns2 ping -c1 -W2 10.0.0.1 >/dev/null && echo "  ns2 → ns1 OK" || { echo "  link FAILED"; exit 1; }

echo "== starting responder in ns1 =="
ip netns exec ns1 env RUST_LOG="warn,y7ke_net=info" \
  Y7KE_SEED="$SEED_R" Y7KE_ROLE=responder \
  "$NODE_BIN" >"$RUN/resp.log" 2>&1 &
R_ID=""; Q_PORT=""
for _ in $(seq 1 50); do
  R_ID=$(grep -oP 'PEER_ID=\K\S+' "$RUN/resp.log" 2>/dev/null | head -1)
  Q_PORT=$(grep -oP 'LISTEN=/ip4/10\.0\.0\.1/udp/\K[0-9]+' "$RUN/resp.log" 2>/dev/null | head -1)
  [[ -n "$R_ID" && -n "$Q_PORT" ]] && break; sleep 0.2
done
[[ -n "$R_ID" && -n "$Q_PORT" ]] || { echo "responder QUIC listen not found"; cat "$RUN/resp.log"; exit 1; }
TARGET="/ip4/10.0.0.1/udp/$Q_PORT/quic-v1/p2p/$R_ID"
echo "  responder $R_ID on QUIC udp/$Q_PORT"

echo "== starting initiator in ns2, target $TARGET =="
ip netns exec ns2 env RUST_LOG="warn,y7ke_net=info" \
  Y7KE_SEED="$SEED_I" Y7KE_ROLE=initiator Y7KE_HOLD_SECS="$HOLD" Y7KE_TARGET="$TARGET" \
  "$NODE_BIN" >"$RUN/init.log" 2>&1 &
INIT_PID=$!

# Wait for the connection + handshake, let a couple of probes land.
for _ in $(seq 1 60); do grep -q '^HANDSHAKE_OK' "$RUN/init.log" && break; sleep 0.3; done
grep -q '^HANDSHAKE_OK' "$RUN/init.log" || { echo "no handshake"; tail "$RUN/init.log"; exit 1; }
echo "  HANDSHAKE_OK; pre-swap probes…"; sleep 7

echo "== swapping initiator IP 10.0.0.2 → 10.0.0.22 (live connection) =="
ip -n ns2 addr add 10.0.0.22/24 dev v2
ip -n ns2 addr del 10.0.0.2/24 dev v2
echo "  swapped at $(grep -c '^PROBE' "$RUN/init.log") probes elapsed"

wait "$INIT_PID" 2>/dev/null

echo "== analysis =="
echo "--- initiator events + probes ---"; grep -E '^EVENT|^PROBE|^HANDSHAKE' "$RUN/init.log" | sed 's/^/    /'
CONN_IDS=$(grep -oP '^EVENT established conn=\KConnectionId\([0-9]+\)' "$RUN/init.log" | sort -u | wc -l)
PROBES_OK=$(grep -c '^PROBE.* ok' "$RUN/init.log")
PROBES_FAIL=$(grep -cE '^PROBE.* (fail|rejected)' "$RUN/init.log")
echo "distinct established conn ids on initiator : $CONN_IDS"
echo "probes ok / fail                           : $PROBES_OK / $PROBES_FAIL"
if [[ "$CONN_IDS" -le 1 && "$PROBES_FAIL" -eq 0 && "$PROBES_OK" -ge 3 ]]; then
  echo "RESULT: MIGRATED — one connection id throughout, probes kept succeeding across the IP swap"
elif [[ "$CONN_IDS" -ge 2 ]]; then
  echo "RESULT: RE-DIALED — a new connection id appeared after the swap (connection dropped, re-established)"
else
  echo "RESULT: INCONCLUSIVE/DROPPED — probes failed without a clean re-dial (see log above)"
fi