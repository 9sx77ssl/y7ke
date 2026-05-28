# Y7KE live cross-network smoke (Phase 3.5)

The canonical real-world acceptance gate: two Y7KE clients on **different
ISPs behind different NATs** exchange Y7 IDs and chat with zero router
configuration. This is the one test that can't be simulated locally —
the netns sims (`scripts/nat-sim/`) cover the relay-fallback and
QUIC-IP-change paths, but only real NATs prove the direct-first
hole-punch end to end. Run it with the home Wi-Fi PC and a phone-tethered
or mobile-4G laptop.

## Setup

Both machines run the same release build against the production
bootstrap/relay `bootstrap1.y7v.lol` (already the hardcoded
`DEFAULT_RELAY_BOOTSTRAP`, so no config needed).

```bash
# on each machine
pnpm --dir ui build && cargo build --release -p y7ke-tauri
# launch with networking logs captured
RUST_LOG="warn,y7ke_net=info,libp2p_dcutr=info,libp2p_relay=info,libp2p_autonat=info" \
  Y7KE_DATA_DIR=~/y7ke-live ./target/release/y7ke 2>&1 | tee ~/y7ke-live.log
```

Keep the machines on genuinely different networks (don't put the laptop
on the same Wi-Fi). Mobile 4G/CGNAT on one side is the most valuable
case — it's the hardest NAT class and the one mom will actually use.

## Procedure

1. On each client, copy its `y7:` ID from the header.
2. PC adds the laptop: `+ add contact ^.^` → paste the laptop's Y7 ID.
3. Laptop accepts the request (`requests >.<`).
4. Send 3 messages PC→laptop and 3 laptop→PC. Confirm all arrive.
5. Open the Connectivity pane (`connectivity O.O`) on **both** sides and
   record, per peer: connection **kind** (Relayed / Direct / Internet /
   Lan), **transport** (QUIC / TCP), **via** host if relayed, **RTT**,
   and the **DCUtR** aggregate (`n/m`).
6. Watch for ~60 s: does the badge flip `RELAY` → `DIRECT`? (The upgrade
   loop retries on observed-addr / AutoNAT changes; on full-cone NAT it
   should upgrade within seconds, on symmetric NAT it stays Relayed.)
7. Suspend the laptop ~30 s, resume. Confirm presence recovers and a
   queued message sent while it was asleep is delivered (offline sync).
8. Restart the PC client; confirm chat history is intact and the
   connection re-establishes.

## Pass criteria

- **Must:** messages flow both ways across the open internet with no
  router config; presence shows a non-Offline kind on both sides;
  suspend/resume recovers; restart preserves history; a message sent
  while the peer was offline is delivered on reconnect.
- **Direct-first win:** the connection reaches `Direct` (QUIC preferred).
  If it stays `Relayed`, that's an acceptable fallback — record the NAT
  class (AutoNAT verdict on each side; `Private`+`Private` with one
  symmetric/CGNAT side explains a permanent relay).

## What to capture

Attach both `~/y7ke-live.log` files and the two Connectivity-pane
readings to the "Live manual smoke results" section of
`docs/V2_GLOBAL_NETWORKING_PLAN.md`. Grep the logs for the progression:

```bash
grep -E 'connection established|connection_kind|presence upgraded|dcutr:|autonat: verdict|relay: reservation' ~/y7ke-live.log
```

Expected healthy signatures:
- `relay: reservation accepted` (both sides reserve a circuit)
- `connection established … kind=Relayed` then, on a punchable NAT,
  `presence upgraded via DCUtR` + `kind=Direct`
- after suspend: `presence … Offline` then `connection established …`
  within a tick or two

If the upgrade never happens, the log line
`dcutr: direct upgrade failed (staying on relay)` plus each side's
`autonat: verdict` names why — and the relay keeps the chat working
regardless, which is the whole point of relay-as-fallback.
