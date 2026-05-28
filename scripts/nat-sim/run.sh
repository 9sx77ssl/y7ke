#!/usr/bin/env bash
# NAT-simulation harness for Y7KE relay fallback (Phase 3.2).
#
# Builds three network namespaces:
#
#     cliA (10.0.1.2) ──┐                          ┌── cliB (10.0.2.2)
#                       ├─ pub (10.0.1.1 / 10.0.2.1) ─┤
#     relay/bootstrap runs in `pub`; ip_forward is OFF so the two
#     clients can reach the bootstrap but NOT each other — exactly the
#     "both peers behind blocking/symmetric NAT, no direct path" case
#     where DCUtR cannot punch and the relay is the only route.
#
# Asserts: the initiator (cliB) reaches the responder (cliA) and
# completes a /y7ke/handshake/1.0.0 over the /p2p-circuit, reporting
# KIND=Relayed. Proves the relay fallback works across isolated NATs.
#
# Run as root (it shells into namespaces):  sudo bash scripts/nat-sim/run.sh
# Build the binaries first (non-root):
#   cargo build -p y7ke-net --example nat_sim_node
#   (and have y7ke-bootstrap built at the path below)
set -uo pipefail

BOOT_BIN="${BOOT_BIN:-/home/rsz/Desktop/y7ke-bootstrap/target/release/y7ke-bootstrap}"
NODE_BIN="${NODE_BIN:-/home/rsz/Desktop/Y7KE/target/debug/examples/nat_sim_node}"
RUN=/tmp/y7ke-natsim
SEED_A="0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a"
SEED_B="0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b"

if [[ $EUID -ne 0 ]]; then echo "must run as root (sudo)"; exit 2; fi
for b in "$BOOT_BIN" "$NODE_BIN"; do
  [[ -x "$b" ]] || { echo "missing binary: $b"; exit 2; }
done

cleanup() {
  set +e
  [[ -n "${BOOT_PID:-}" ]] && kill "$BOOT_PID" 2>/dev/null
  [[ -n "${RESP_PID:-}" ]] && kill "$RESP_PID" 2>/dev/null
  pkill -f nat_sim_node 2>/dev/null
  for ns in pub cliA cliB; do ip netns del "$ns" 2>/dev/null; done
}
trap cleanup EXIT
cleanup            # clear any leftovers from a prior aborted run
mkdir -p "$RUN"; rm -f "$RUN"/*.log "$RUN"/*.key

echo "== building namespaces =="
ip netns add pub; ip netns add cliA; ip netns add cliB
ip link add vethA0 type veth peer name vethA1
ip link add vethB0 type veth peer name vethB1
ip link set vethA0 netns pub;  ip link set vethA1 netns cliA
ip link set vethB0 netns pub;  ip link set vethB1 netns cliB
ip -n pub  addr add 10.0.1.1/24 dev vethA0
ip -n pub  addr add 10.0.2.1/24 dev vethB0
ip -n cliA addr add 10.0.1.2/24 dev vethA1
ip -n cliB addr add 10.0.2.2/24 dev vethB1
for ns in pub cliA cliB; do ip -n "$ns" link set lo up; done
ip -n pub  link set vethA0 up; ip -n pub link set vethB0 up
ip -n cliA link set vethA1 up; ip -n cliB link set vethB1 up
ip -n cliA route add default via 10.0.1.1
ip -n cliB route add default via 10.0.2.1
# Relay-only condition: pub does NOT route between the two clients.
ip netns exec pub sysctl -qw net.ipv4.ip_forward=0

# Sanity: cliA can reach the relay host, cliB cannot reach cliA.
ip netns exec cliA ping -c1 -W2 10.0.1.1 >/dev/null && echo "  cliA → relay OK" || { echo "  cliA → relay FAILED"; exit 1; }
if ip netns exec cliB ping -c1 -W2 10.0.1.2 >/dev/null 2>&1; then
  echo "  WARN: cliB can reach cliA directly — NAT isolation broken"; exit 1
else
  echo "  cliB → cliA blocked (relay required) OK"
fi

echo "== starting relay/bootstrap in pub =="
ip netns exec pub env Y7KE_BOOTSTRAP_EXTERNAL_ADDR="/ip4/10.0.1.1/tcp/4101,/ip4/10.0.2.1/tcp/4101" \
  "$BOOT_BIN" --listen-port 4101 --key-path "$RUN/boot.key" >"$RUN/boot.log" 2>&1 &
BOOT_PID=$!
BOOT_ID=""
for _ in $(seq 1 50); do
  BOOT_ID=$(grep -oP 'PeerId:\s*\K\S+' "$RUN/boot.log" 2>/dev/null | head -1)
  [[ -n "$BOOT_ID" ]] && break; sleep 0.2
done
[[ -n "$BOOT_ID" ]] || { echo "bootstrap PeerId never printed"; cat "$RUN/boot.log"; exit 1; }
echo "  bootstrap PeerId = $BOOT_ID"

echo "== starting responder in cliA =="
ip netns exec cliA env RUST_LOG="warn,y7ke_net=info" \
  Y7KE_SEED="$SEED_A" Y7KE_ROLE=responder \
  Y7KE_BOOTSTRAP="/ip4/10.0.1.1/tcp/4101/p2p/$BOOT_ID" \
  "$NODE_BIN" >"$RUN/respA.log" 2>&1 &
RESP_PID=$!
A_ID=""
for _ in $(seq 1 50); do
  A_ID=$(grep -oP 'PEER_ID=\K\S+' "$RUN/respA.log" 2>/dev/null | head -1)
  [[ -n "$A_ID" ]] && break; sleep 0.2
done
[[ -n "$A_ID" ]] || { echo "responder PeerId never printed"; cat "$RUN/respA.log"; exit 1; }
echo "  responder PeerId = $A_ID"
echo "  waiting for responder relay reservation…"
for _ in $(seq 1 100); do grep -q '^RESERVED' "$RUN/respA.log" && break; sleep 0.3; done
grep -q '^RESERVED' "$RUN/respA.log" || { echo "responder never reserved a circuit"; tail "$RUN/respA.log"; exit 1; }
echo "  responder RESERVED"

TARGET="/ip4/10.0.2.1/tcp/4101/p2p/$BOOT_ID/p2p-circuit/p2p/$A_ID"
echo "== starting initiator in cliB =="
echo "  dialing $TARGET"
ip netns exec cliB env RUST_LOG="warn,y7ke_net=info" \
  Y7KE_SEED="$SEED_B" Y7KE_ROLE=initiator \
  Y7KE_BOOTSTRAP="/ip4/10.0.2.1/tcp/4101/p2p/$BOOT_ID" \
  Y7KE_TARGET="$TARGET" \
  "$NODE_BIN" >"$RUN/initB.log" 2>&1
INIT_RC=$?

echo "== result =="
echo "--- initiator log ---"; cat "$RUN/initB.log"
KIND=$(grep -oP '^KIND=\K\S+' "$RUN/initB.log" | head -1)
UPGRADED=$(grep -oP '^UPGRADED=\K\S+' "$RUN/initB.log" | head -1)
if [[ $INIT_RC -eq 0 ]] && grep -q '^HANDSHAKE_OK' "$RUN/initB.log"; then
  echo "PASS: handshake completed over relay (KIND=${KIND:-?}${UPGRADED:+, later UPGRADED=$UPGRADED})"
  exit 0
else
  echo "FAIL: initiator rc=$INIT_RC (KIND=${KIND:-none})"
  echo "--- responder tail ---"; tail -15 "$RUN/respA.log"
  echo "--- bootstrap tail ---"; tail -15 "$RUN/boot.log"
  exit 1
fi
