# Y7KE V2 — Global Networking Plan

Source-of-truth document for the V2 networking phase. Synthesises
external research (rust-libp2p source + specs, iroh, Tailscale
DERP / magicsock, Tox + Nebula, RFC 9000, arXiv 2510.27500) against
Y7KE's actual state on `main` (post-`v0.1.61`, commit `b06d3b2`)
and defines a phased implementation roadmap.

**Status of the codebase at the time of writing.** V2-A4 (Circuit
Relay v2 client + server), V2-A5 (DCUtR), V2-A6 (QUIC transport)
and the `DialMode` redesign (`LanOnly` / `Internet` / `P2p`) shipped
in `v0.1.61`. Loopback tests (`four_node_relay`, `v2_dcutr_smoke`,
`quic_listen_smoke`) and the live `live_relay_smoke` against
`bootstrap1.y7v.lol` v0.1.4 prove the wire paths exist. Cross-NAT
real-world success is **untested in CI** and qualitatively weak in
manual smokes.

**North star.** "A modern Tox/Session/Tailscale-inspired
direct-first encrypted messenger built with modern Rust networking
architecture." Two normal humans behind random home NAT or mobile
CGNAT install Y7KE, exchange Y7 IDs, add each other, and chat
**directly over encrypted QUIC** with **zero router configuration**.
Relay is a fallback the system actively retires once a direct path
becomes possible.

---

## 1. Comparative architecture analysis

Five reference systems were studied; below is the distillation
relevant to Y7KE's libp2p-based stack.

### rust-libp2p — strengths, weaknesses, where Y7KE sits

Y7KE is built on rust-libp2p 0.56. The relevant behaviours are
already composed (`crates/y7ke-net/src/behaviour.rs:27`):
`identify`, `ping`, `mdns`, three CBOR request-response codecs,
`kad` (Server mode), `relay::client`, `dcutr`. The transport stack
(`swarm.rs:74-94`) layers TCP + Noise + Yamux **and** QUIC v1, with
DNS resolution and a relay-client transport on top. Both transports
listen at boot (`swarm.rs:129-150`).

Strength: the ecosystem covers all four pillars we need (DHT,
relay, hole punch, dual transport) under one swarm event loop with a
uniform `NetworkBehaviour` API, and recent measurement work
([arXiv 2510.27500](https://arxiv.org/abs/2510.27500)) puts
**DCUtR success at 70 % ± 7.1 %** across 4.4 M attempts in 167
countries — meaning the libp2p hole-punching path is real, not
aspirational.

Weakness: libp2p is "primitives, not opinions". You can wire the
behaviours and still miss critical glue — e.g. `identify` without
`with_push_listen_addr_updates(true)` doesn't proactively notify
peers when a relay reservation lands (see § 8 below), DCUtR fires
once and gives up, idle direct connections default to
[~10 s timeout](https://github.com/libp2p/rust-libp2p/discussions/5741).
The dial graph is the application's job, not the library's. Y7KE's
current `dial_with_discovery` (`crates/y7ke-app/src/app/contacts.rs:114-195`)
already handles steps 1-4 (swarm address book → cached addrs → Kad
lookup → re-check); the gaps are mostly in *triggers* for re-dial,
*upgrades* from relay to direct, and *observability*.

### iroh — direct-first reference architecture

iroh wraps QUIC + relay + discovery behind a single `Endpoint::connect(node_id)`
call where `NodeId` is an Ed25519 pubkey
([iroh.computer](https://www.iroh.computer/)). Their tagline
*"IP addresses break, dial keys instead"* maps 1:1 onto Y7KE's
`Y7Id` = Ed25519 pubkey identity model. The runtime continuously
probes a direct path even after a relay connection establishes,
upgrading transparently — this is the model Y7KE must mirror.

iroh's relay (`iroh-relay`) is intentionally minimal: stateless,
no message storage, encrypted end-to-end. Same invariant as Y7KE's
`y7ke-bootstrap` relay (`/home/rsz/Desktop/y7ke-bootstrap/src/main.rs:81-92`),
which holds 1024 reservation slots with 1 h duration and forwards
ciphertext only.

**Useful idea Y7KE will adopt:** iroh's "relay is a coexisting
path, not a replacing path" — concretely, *every relayed connection
schedules a recurring direct-upgrade attempt* until either it
succeeds or the user-visible mode changes. iroh's `iroh doctor`
diagnostics layout (one screen: observed addrs, AutoNAT verdict,
active reservations, last DCUtR outcome, RTT to relay) is the model
for Y7KE's planned Connectivity pane (§ 10).

**Dangerous idea Y7KE will reject:** wholesale replacement of
libp2p with a from-scratch iroh-style stack. Tempting given the
elegance, but it would discard three working custom request-response
protocols, ts-rs wire types, and the Tauri composition. Borrow the
*UX* (dial by pubkey, doctor pane, continuous probing), keep the
*plumbing* (rust-libp2p 0.56).

### Tailscale magicsock + DERP — production NAT traversal

Tailscale runs over WireGuard, not libp2p, so the wire isn't
copyable, but the **dial strategy and operator philosophy** is the
single most actionable reference:

- ► [Endpoint selection](https://github.com/tailscale/tailscale/tree/main/wgengine/magicsock):
  if a known-good direct address exists and was good recently, use
  only it; otherwise *dual-send* to both DERP and the candidate
  direct addr until one wins. Hysteresis: "don't change anything if
  the latency improvement is less than 1 %". The dual-send window
  is what produces *no-perceived-latency upgrades* — the connection
  feels live the moment the relay path forms, then transparently
  swaps to direct seconds later.
- ► DERP "is both our fallback of last resort to get connectivity,
  and our helper to upgrade to a peer-to-peer connection"
  ([tailscale.com/kb/1232](https://tailscale.com/kb/1232/derp-servers)).
  All packets through DERP remain WireGuard-encrypted; DERP sees
  ciphertext only — the exact invariant Y7KE enforces at the
  bootstrap relay (Noise wraps every Y7KE byte before the
  `/p2p-circuit` frame is built).
- ► The
  [birthday-attack symmetric-NAT defeat](https://tailscale.com/blog/how-nat-traversal-works)
  (174 probes → 50 %, 1024 → 98 %, 2048 → 99.9 %) closes the long
  tail that DCUtR alone leaves on the table. **We will NOT chase
  this in Phase 2** — it's expensive (170 k packets per side for
  the symmetric-NAT-on-both-sides worst case) and the ~30 %
  failure tail it could close is dominated by users who still have
  the relay as a backup.

### Tox c-toxcore — battle-tested patterns, anti-patterns

Tox's DHT is a Kademlia variant routing by raw Ed25519 pubkey
XOR distance; bootstrap nodes are just long-lived DHT participants
with hard-coded (pubkey, IP) tuples — the **exact pattern** Y7KE
uses (`crates/y7ke-net/src/swarm.rs::DEFAULT_BOOTSTRAPS`,
`/dns4/bootstrap1.y7v.lol/tcp/4101/p2p/12D3KooW…`). Tox's TCP relay
fallback is stateless and ciphertext-only — same invariant Y7KE
preserves.

**Anti-pattern Y7KE rejects:** Tox has no DCUtR-equivalent. Once
on TCP relay, the connection stays on TCP relay for its lifetime,
with no upgrade attempt loop. CGNAT users in the Tox literature
become permanent relay clients, eating community-operated relay
budget indefinitely. **Y7KE's "relay is temporary" stance is
explicitly a correction of this Tox failure mode.**

### Nebula — discovery vs relay separation

Slack's [Nebula](https://github.com/slackhq/nebula) lighthouses
"allow individual peers to find each other and optionally use UDP
hole punching" — they are **pure discovery** services, not relays.
Lighthouses need a routable IP and reachable port 4242 UDP. Relay is
a separate opt-in role configured per-host.

Y7KE today *conflates* discovery and relay in `y7ke-bootstrap`
(it's both a Kad DHT root and a Circuit Relay v2 server). This is
operationally convenient but couples two resource profiles —
discovery is cheap, relay is bandwidth-heavy. **Adopt later
(out of Phase 2 scope):** in a V3 refactor, split into a
`y7ke-lighthouse` (Kad + identify only) and `y7ke-relay`
(`relay::Behaviour` only). Lets the community run cheap
discovery-only nodes; relay becomes an explicit, separately funded
role. For Phase 2 we accept the current conflated bootstrap.

### WebRTC ICE/STUN/TURN — concepts only

WebRTC's
[ICE candidate gathering + connectivity checks](https://webrtc.org/getting-started/turn-server)
is the conceptual ancestor of DCUtR. We will not implement
WebRTC; we extract the principle that **candidate enumeration must
be exhaustive and ranked** before the synchronised dial fires.
Y7KE's `sort_addrs_for_dial`
(`crates/y7ke-net/src/dial_priority.rs`) already implements ranking
(direct QUIC > direct TCP > circuit). The gap is candidate
**freshness** — without `identify` push, stale candidate sets get
dialed, and DCUtR's CONNECT message arrives with outdated
`ObsAddrs` (see § 8).

---

## 2. NAT traversal deep-dive

### Direct dialing

The base case: peer A knows peer B's `Multiaddr` (e.g.
`/ip4/A.B.C.D/udp/PORT/quic-v1/p2p/<B>`) and opens a connection.
For Y7KE this comes from one of four sources in `dial_with_discovery`
(`crates/y7ke-app/src/app/contacts.rs:114-195`): mDNS-populated
swarm address book (LAN only), `peer_state.last_addrs_json` (cached
across restarts), Kademlia `get_providers` query, and a re-check of
the address book after Kad has populated routing. Direct dial works
on full-cone NAT, on either side if behind a router with port
forwarding, and on IPv6 networks where no NAT64 is in front.

### Relay fallback

When direct dial fails, both peers reserve a slot on a public
relay (Y7KE's `bootstrap1.y7v.lol`) and exchange traffic through
it. The reservation lifecycle, per
[the Circuit Relay v2 spec](https://github.com/libp2p/specs/blob/master/relay/circuit-v2.md):

- Client opens `/libp2p/circuit/relay/0.2.0/hop`, sends
  `HopMessage{type=RESERVE}`.
- Relay responds with `HopMessage{type=STATUS, status=OK, reservation={expire, addrs, voucher}, limit={duration, data}}`.
- ► The spec explicitly states "**it's the responsibility of the
  client to refresh**" — no server-driven renewal. Y7KE's
  `relay::client::Behaviour` handles this transparently.
- ► The reservation "**remains valid until its expiration, as long
  as there is an active connection from the peer to the relay**" —
  a dropped TCP/QUIC connection invalidates the reservation
  immediately, not at TTL. This is why Y7KE's 15 s bootstrap
  reconnect tick (`swarm.rs:224, 256-276`) clears
  `state.relay_reserved` on `ConnectionClosed` and re-issues
  `listen_on(.../p2p-circuit)` on reconnect.

Y7KE's bootstrap server uses `relay::Config { max_reservations: 1024, max_circuits_per_peer: 16, reservation_duration: 3600s, max_circuit_duration: 3600s, max_circuit_bytes: 0 }`
(`/home/rsz/Desktop/y7ke-bootstrap/src/main.rs:81-92`). The
`add_external_address` call at `main.rs:153` is what fixed the
historical `NoAddressesInReservation` bug — without explicitly
declared external addrs, libp2p's relay server returns an empty
addrs list and clients reject.

### Hole punching mechanics (DCUtR)

The
[DCUtR spec](https://github.com/libp2p/specs/blob/master/relay/DCUtR.md):

1. Both peers are connected via the relay. Inbound peer B opens
   `/libp2p/dcutr` stream **over the existing relayed connection**.
2. B sends `HolePunch{Type=CONNECT=100, ObsAddrs=[…]}` carrying its
   identify-observed addresses.
3. A replies with its own `Connect` containing A's observed
   addresses.
4. ► "B starts a timer for **half the RTT** measured from the time
   between sending the initial Connect and receiving the response".
5. B sends `HolePunch{Type=SYNC=300}`.
6. After the half-RTT timer, both sides simultaneously dial each
   other's `ObsAddrs`. The simultaneous open punches the NAT.
7. ► On failure: "Inbound peers (here B) **SHOULD retry twice
   (thus a total of 3 attempts)** before considering the upgrade
   as failed."

Real-world measurement
([arXiv 2510.27500](https://arxiv.org/abs/2510.27500), 4.4 M
attempts, 167 countries) shows:

- ► **DCUtR success: 70 % ± 7.1 %** aggregate.
- ► **97.6 % of successful connections established on the first
  attempt** — the spec's "retry up to 3" mostly catches the long
  tail.
- ► **TCP vs QUIC statistically indistinguishable, both ~70 %** —
  refuting the folklore that QUIC hole-punches dramatically better.

### NAT failure cases

- **Full-cone NAT.** Direct dial works in both directions; relay
  only used for first introduction. ~50 % of home routers.
- **Restricted-cone / port-restricted-cone NAT.** Outbound creates
  a pinhole only for the specific destination tuple; DCUtR's
  simultaneous open succeeds because both sides dial the predicted
  port at the same instant.
- **Symmetric NAT.** Each outbound to a different destination gets
  a different external port. DCUtR's address prediction fails —
  the port the peer dials is not the port the NAT will assign for
  the new flow. This is the residual ~30 % the field papers
  measure.
- **Symmetric ↔ symmetric NAT.** Worst case. Only Tailscale's
  birthday-attack approach (probe 1000+ ports in parallel) gets
  past this; DCUtR cannot. Y7KE accepts these users as
  permanent-relay clients in Phase 2.
- **CGNAT (Carrier-Grade NAT, mobile networks).** Doubly NAT'd
  (handset + carrier). Often symmetric. The papers show CGNAT is
  the dominant source of the 30 % failure rate.
- **IPv6-only with NAT64.** Different failure mode: the address
  candidates returned in `ObsAddrs` are IPv6, the dialing peer is
  IPv4. libp2p does fall back through the relay; Y7KE inherits
  this correctness.

### QUIC migration

[RFC 9000 §9](https://www.rfc-editor.org/rfc/rfc9000#name-connection-migration)
specifies path validation:

- ► §9.1: an endpoint probing a new path sends **PATH_CHALLENGE**
  frames; the peer replies with **PATH_RESPONSE** from the new
  address echoing the 8-byte challenge data. Migration is
  confirmed once a matching PATH_RESPONSE arrives.
- ► §9.3 anti-spoof: an endpoint MUST NOT shift its send buffers
  to the new path until validation completes, but MAY
  anti-amplification-limit traffic during probing.
- ► §5.1.1: the `active_connection_id_limit` transport parameter
  caps how many CIDs the endpoint tracks. `NEW_CONNECTION_ID`
  frames supply fresh CIDs so the peer can move to a new path
  without revealing connection linkage.

Practical Y7KE consequence: when a phone's IP shifts from Wi-Fi
(10.0.0.5) to CGNAT (100.64.x.x), an existing QUIC connection does
**not need to redial, re-handshake or re-authenticate** — it
migrates transparently. libp2p-quic in rust-libp2p 0.56 pins the
local UDP socket and doesn't expose CID rotation as a tunable; this
is a tracked audit item in Phase 1 for the V2-A6 finishing work.

### Reconnect storms

After a multi-minute network blip (laptop suspend, WiFi swap), 50+
contacts may come back at once. Each one trips
`dial_with_discovery` → Kad `find_peer` → multi-addr fanout. Without
backoff this hammers the bootstrap and Kad routing for tens of
seconds. Phase 3 step 11 adds per-peer jitter (0–500 ms) and a
bounded `tokio::sync::Semaphore` (N=4) on in-flight Kad lookups.

---

## 3. Y7KE adaptation matrix

| Concept (source) | Decision | Rationale |
|---|---|---|
| identify push of listen-addr updates ([rust-libp2p #4007](https://github.com/libp2p/rust-libp2p/issues/4007)) | **Adopt** | One-line fix at `behaviour.rs:67`. Unlocks fresh `ObsAddrs` for DCUtR; almost certainly the single biggest win. |
| AutoNAT v2 client + server | **Adopt** | Without it the client always reserves relay slots, even on public IPs. With it, reserve only when verdict is `Private`. |
| DCUtR retry loop (libp2p does 3 attempts internally) | **Adopt as baseline + extend** | Spec retry is for the synchronised-dial moment. We add an *outer* loop that re-tries DCUtR on `ObsAddr` change / AutoNAT flip / 60 s timer with exp backoff. iroh and Tailscale both do this. |
| Multi-relay reservations ([rust-libp2p #3210](https://github.com/libp2p/rust-libp2p/discussions/3210)) | **Adapt later** | mxinden suggests 20 closest from Kad; we currently have one VPS relay. Multi-relay → after we have ≥2 production relays. |
| iroh `Endpoint::connect(node_id)` UX | **Adapt** | Y7KE already dials by `Y7Id` (Ed25519 pubkey) — same shape. Borrow the "doctor" pane idea for Connectivity surface. |
| Tailscale dual-send (relay + direct simultaneously until one wins) | **Adapt — partial** | libp2p does this naturally via `dial_address` on all returned addrs concurrently. We don't need to add anything; we need to *make sure the relay path doesn't preempt direct candidates being tried first* — already handled by `sort_addrs_for_dial`. |
| Tailscale birthday-attack symmetric NAT defeat | **Reject (this phase)** | Closes the symmetric-NAT-on-both-sides tail. Cost: 170 k probe packets per side worst case. Not worth Phase 2 capacity; relay still gets them connected. |
| iroh's PCP/NAT-PMP/UPnP port mapping integration | **Postpone** | Tailscale: "makes one NAT vanish". Real wins. Behind AutoNAT+DCUtR push in priority. Revisit after measuring our 70 % baseline. |
| Nebula split-discovery-from-relay roles | **Adopt later** | V3 refactor. For Phase 2 we keep the conflated bootstrap. |
| Tox permanent-relay model (no upgrade loop) | **Reject** | The `relay is temporary` invariant exists precisely to avoid this. |
| `Config::with_idle_connection_timeout` ≥ 60 s ([discussion #5741](https://github.com/libp2p/rust-libp2p/discussions/5741)) | **Adopt** | Y7KE currently uses `IDLE_CONNECTION_TIMEOUT = 300s` (`swarm.rs:53`). Above the libp2p default 10 s; verified safe. Document in SoT as conscious choice. |
| WebRTC TURN | **Reject** | Implementation reuse is impossible; conceptual fallback role already covered by Circuit Relay v2. |
| Wholesale switch to iroh stack | **Reject** | Erases the three custom Y7KE protocols, ts-rs wire types, Tauri composition. Borrow UX, keep plumbing. |

---

## 4. Y7KE networking lifecycle

The intended lifecycle for a fresh client coming online:

```
Boot
 ├─► load Settings.dial_mode + Settings.extra_bootstraps from SQLite
 ├─► build_swarm: TCP + QUIC + DNS + relay-client transports
 ├─► listen on /ip4/0.0.0.0/tcp/0 + /ip4/0.0.0.0/udp/0/quic-v1
 │
 ├─► If dial_mode != LanOnly:
 │     ├─► dial each bootstrap (TCP + QUIC racing — direct QUIC wins
 │     │   on most paths)
 │     ├─► identify push lands on both sides; AutoNAT v2 probe runs
 │     │   ──► verdict: Public | Private | Unknown
 │     ├─► If Private: swarm.listen_on(<bootstrap>/p2p-circuit) —
 │     │   reservation
 │     │   ──► /p2p-circuit/p2p/<self> listen address appears in our
 │     │        identify push, propagates to peers via Kad provider
 │     │        records
 │     └─► Kad: start_providing(self) — we're discoverable
 │
 ├─► Add-contact dial (dial_with_discovery, contacts.rs:114-195):
 │     1. swarm address book (mDNS / identify cache)
 │     2. peer_state.last_addrs_json (persisted across restarts)
 │     3. Kad find_peer → multi-addr fanout, ranked by
 │        sort_addrs_for_dial (QUIC direct > TCP direct > circuit)
 │     4. re-check swarm address book
 │
 ├─► On Relayed connection landing:
 │     ├─► AppEvent::PresenceChanged { kind: Relayed, via: <bootstrap-y7id>, transport: Tcp/Quic }
 │     ├─► UI shows RELAY badge with bootstrap name
 │     └─► upgrade_loop schedules first DCUtR attempt
 │
 ├─► Continuous relay → direct upgrade attempt loop:
 │     - On observed-addr change (from identify push)        ──► retry now
 │     - On AutoNAT verdict flip Private → Public            ──► retry now
 │     - On 60s timer while still Relayed, backoff 60 → 300 → 600s
 │     - Reset backoff on any observed-addr change
 │
 ├─► DCUtR succeeds:
 │     ├─► NetEvent::ConnectionUpgraded { peer, kind: Direct }
 │     ├─► event_loop adds Direct to connection_kinds set;
 │     │   best_kind precedence (Direct=5 > Lan=4 > Internet=3 > Relayed=2)
 │     │   reports Direct
 │     ├─► AppEvent::PresenceChanged { kind: Direct, transport: Quic|Tcp }
 │     ├─► UI badge flips RELAY → DIRECT, lilac → green
 │     └─► /sync/1.0.0 + /msg/1.0.0 traffic migrates to direct path
 │
 ├─► Network change (Wi-Fi swap, suspend/resume):
 │     ├─► For QUIC: PATH_CHALLENGE/RESPONSE migration (RFC 9000 §9)
 │     │   ──► chat continues with no handshake
 │     ├─► For TCP: connection drops; ConnectionClosed event;
 │     │   discovery chain re-runs after ~1s on presence ticker
 │     │   wake-up (kicked by NetCommand::PresenceProbeNow on any
 │     │   observed-addr change)
 │     └─► Bootstrap reconnect tick (15s) repairs lost relay
 │         reservation if applicable
 │
 └─► Relay fallback (DCUtR exhausted, both peers behind symmetric NAT):
       ├─► Stay on relay indefinitely; messages flow ciphertext via
       │   /p2p-circuit
       ├─► Sync/reconcile (/y7ke/sync/1.0.0) drains over the relay
       │   stream (verified by v2_sync_over_relay test, Phase 3)
       └─► AppEvent::PresenceChanged keeps reporting Relayed
```

---

## 5. Connection state machine

States, with where each lives in current code:

| State | Where it lives | Transition trigger |
|---|---|---|
| `Connecting` | `event_loop.rs:80` (after `PresenceChanged` to non-Offline, before kind known) | `dial_with_discovery` started for this peer |
| `Discovering` | implicit — `pending_find_peer` map in `swarm.rs::TaskState` | Kad `find_peer` in flight |
| `ReservingRelay` | implicit in `relay::client::Behaviour` state machine | `listen_on(<bootstrap>/p2p-circuit)` called, awaiting `ReservationReqAccepted` |
| `Relayed` | `event_loop.rs:84` (kind=Relayed inserted into `connection_kinds`) | `ConnectionEstablished` with `is_relayed()` true |
| `UpgradingToDirect` | new — Phase 2 step 4 | `upgrade_loop` triggers DCUtR attempt while Relayed |
| `Direct` | `event_loop.rs:102-115` (kind=Direct inserted on `ConnectionUpgraded`) | `dcutr::Event::DirectConnectionUpgradeSucceeded` |
| `Reconnecting` | implicit — between `ConnectionClosed` and next `ConnectionEstablished` | TCP closed; QUIC migrated via PATH_CHALLENGE doesn't enter this state |
| `Failed` | `event_loop.rs:91-105` (presence → Offline, `connection_kinds` set cleared) | All transports exhausted in `dial_with_discovery`; or `ConnectionClosed` with no reconnect within ~30 s |

Transition events emitted to the UI (all `AppEvent::PresenceChanged`,
with new fields per Phase 2 step 7):

```
Connecting     → Relayed:    PresenceChanged { kind: Relayed, via, transport, last_rtt_ms }
Relayed        → Direct:     PresenceChanged { kind: Direct,  via=None, transport, last_rtt_ms }
Direct/Relayed → Offline:    PresenceChanged { kind: Offline, via=None, transport=None }
Offline        → Connecting: PresenceChanged { kind: Connecting }
```

Best-kind precedence (`app.rs:163`, already implemented):
`Direct=5 > Lan=4 > Internet=3 > Relayed=2 > Connecting=1 > Offline=0`.
A peer with both a Relayed and a Direct connection reports Direct.

---

## 6. Relay strategy

Y7KE inherits libp2p Circuit Relay v2 wholesale. The strategy is
how we *use* it, not what we re-implement.

### Reservation lifecycle

- **Acquisition.** On every `ConnectionEstablished` to a known
  bootstrap (`swarm.rs::TaskState::bootstrap_peers`), call
  `swarm.listen_on(<addr>/p2p-circuit)`. `relay::client::Behaviour`
  performs the HOP handshake. Idempotent — libp2p dedupes a
  duplicate listen.
- **Renewal.** Client-driven (per spec, server does not push
  renew). `relay::client::Behaviour` handles automatically; we
  observe `Event::ReservationReqAccepted { renewal: bool, .. }`.
- **Expiration.** On `ConnectionClosed` to the relay,
  `state.relay_reserved` is cleared (`swarm.rs::handle_swarm_event`
  ~line 470); the 15 s reconnect tick reopens the connection and
  re-issues the listen. Recovery window: ~10–15 s (verified by
  V2-A4 acceptance test against the live VPS).

### Stateless invariant

Bootstrap relay sees only ciphertext frames. The Noise + ChaCha20-Poly1305
encryption wraps the Y7KE wire payload before the libp2p relay layer
ever sees a byte. The bootstrap repo (`/home/rsz/Desktop/y7ke-bootstrap`)
deliberately has **zero `y7ke-*` dependencies** so it can't even
decode wire types. This is structurally enforced.

### Abuse protection

`relay::Config` knobs (set in `/home/rsz/Desktop/y7ke-bootstrap/src/main.rs:81-92`):

- `max_reservations: 1024` — total slots
- `max_circuits: 1024` — total in-flight circuits
- `max_circuits_per_peer: 16` — bounds a single peer's relay use
- `reservation_duration: 3600 s`
- `max_circuit_duration: 3600 s`
- `max_circuit_bytes: 0` (unlimited bytes per circuit)

Future tightening (out of Phase 2): per-peer rate-limit by
`identify` agent-string match `^y7ke-net/` — gates abuse from
non-Y7KE libp2p clients that find the bootstrap via DHT crawl.

### "Relay is temporary"

The key behavioural invariant for Phase 2. Every Relayed connection
schedules a recurring upgrade attempt. See § 8.

### Idle cleanup

Connections idle for `IDLE_CONNECTION_TIMEOUT = 300 s`
(`swarm.rs:53`) drop. This is above the libp2p default 10 s
([discussion #5741](https://github.com/libp2p/rust-libp2p/discussions/5741))
deliberately so a successful DCUtR upgrade has time to attract real
traffic before the direct connection times out. **Document this as a
conscious choice in the live deployment guide.**

Stale relay multiaddrs in `peer_state.last_addrs_json` are evicted
in Phase 2 step 8: on `apply_dial_mode(LanOnly)`, clear circuit
multiaddrs from the address book; evict cached `peer_state.last_addrs`
rows older than 24 h for any peer whose only known addrs are
circuits. Prevents dead-relay multiaddrs from haunting the swarm.

---

## 7. QUIC strategy

QUIC is the preferred transport. TCP is the fallback for
QUIC-blocked networks (rare but real — some corporate firewalls
block UDP 443 / UDP 4101).

### Dual transport

Currently:

- Client listens on both `/ip4/0.0.0.0/tcp/0` and
  `/ip4/0.0.0.0/udp/0/quic-v1` (`swarm.rs:129-150`).
- `sort_addrs_for_dial` (`dial_priority.rs`) ranks QUIC > TCP >
  circuit, so when Kad returns a mixed set we dial QUIC candidates
  first.
- **Gap:** the bootstrap is TCP-only. Clients can never reach the
  bootstrap over QUIC, defeating the "QUIC-first" story for the
  most important connection. Phase 2 step 5 closes this by adding
  `.with_quic()` to the bootstrap build chain and updating
  `DEFAULT_BOOTSTRAPS` with the matching `udp/.../quic-v1`
  multiaddr.

### Connection migration

[RFC 9000 §9](https://www.rfc-editor.org/rfc/rfc9000#name-connection-migration) — the laptop sleep/resume + Wi-Fi swap killer
feature. libp2p-quic in 0.56 sits on top of `quinn` which supports
the wire-level path validation but does not expose CID rotation as
a configurable. **In-scope research for the Phase 3 step 12 test:**
verify by experiment whether a libp2p-quic Y7KE connection actually
migrates when a netns source IP changes, or whether it drops + the
discovery chain re-runs. If migration works: great, document in
this doc. If it doesn't: file a tracking issue, accept the drop +
discovery-rerun behaviour for now.

### Address validation

RFC 9000 §8 Retry tokens are a server-side amortisation tool. Not
in Phase 2 — the bootstrap is the only QUIC server we operate, and
its bandwidth budget is bounded by `max_reservations`. Revisit if
production logs show UDP source-spoof attempts.

### Transport failover

When a direct path fails mid-session (NAT rebind kicks the
mapping), libp2p drops the connection; `ConnectionClosed` fires;
`dial_with_discovery` re-runs and the relay path takes over. The
upgrade loop re-tries direct on the new mapping. Acceptable. No
explicit failover state machine needed.

---

## 8. DCUtR / hole-punching strategy

The single highest-leverage change in this entire plan.

### Why our current DCUtR is degraded

`crates/y7ke-net/src/behaviour.rs:67` constructs `identify::Config::new(IDENTIFY_PROTOCOL_VERSION.to_string(), local_keypair.public()).with_agent_version(IDENTIFY_AGENT_VERSION.to_string()).with_interval(60s)` — no
`.with_push_listen_addr_updates(true)`. Consequence:

1. Client A boots, dials bootstrap, reserves a relay slot at
   T=200 ms. A's `/p2p-circuit` listen address appears in the
   swarm.
2. Identify pushes once per 60-s tick. A's existing peer B (added
   earlier, in the same boot cycle) does **not** learn about the
   new `/p2p-circuit` listen address until the next periodic
   identify push.
3. When DCUtR fires on B's behalf, B's `ObsAddrs` for A is stale —
   does not include the relay-circuit address. The CONNECT message
   carries the wrong candidate set. Hole punch fails because B
   isn't dialing the right A endpoint.
4. ► [Issue #4007](https://github.com/libp2p/rust-libp2p/issues/4007)
   surfaces exactly this failure mode in the rust-libp2p tracker.

Fix in Phase 2 step 1: add `.with_push_listen_addr_updates(true)`.
With it, every `NewListenAddr` event (including the
reservation-acquired `/p2p-circuit` address) immediately triggers
an identify push to all active peers. DCUtR's `ObsAddrs` becomes
fresh; the hole-punch success window aligns.

### Synchronised dial timing

Per spec: inbound peer B times its SYNC for **half the RTT** of the
preceding CONNECT round-trip. This is the only timing constant. The
spec recommends 3 attempts total (1 + 2 retries). 97.6 % of
successful upgrades happen on attempt 1
([arXiv 2510.27500](https://arxiv.org/abs/2510.27500)) — the
retries catch the long tail.

### When NOT to retry

The libp2p `dcutr::Behaviour` retries up to 3 within a single
upgrade attempt. The Phase 2 step 4 `upgrade_loop` adds an *outer*
attempt cycle with explicit non-retry conditions:

- AutoNAT verdict is `Private` AND no `ObsAddr` change since last
  attempt — wait, don't waste packets.
- The peer is on a symmetric-NAT-on-both-sides path and we've
  failed 3+ outer attempts. Mark as `RelayPermanent` for this
  session, retry only on next observed-addr change.
- The peer is on `LanOnly` mode (no Kad, no relay) — discovery
  chain doesn't go through Kad, DCUtR never fires.

### Relay-assisted upgrades

Y7KE's bootstrap already runs `relay::Behaviour` (the server side).
Server-side: no Y7KE-side code lives in DCUtR coordination — the
relay just forwards the `/libp2p/dcutr` stream like any other
stream. No special wiring needed at the bootstrap.

### Direct path promotion

On `NetEvent::ConnectionUpgraded { kind: Direct }`, the
`event_loop` inserts `Direct` into the peer's `connection_kinds`
set. `best_kind()` precedence promotes `Direct` over `Relayed`
automatically. Existing libp2p streams routed through the relay
**do not** automatically migrate to the new direct connection — the
next `send_request` will pick the direct connection because libp2p
prefers shorter paths. In-flight responses on the relay path
complete normally. No special migration handling needed.

---

## 9. Security analysis

### No plaintext at relay

The Noise XX handshake completes between Alice and Bob before any
Y7KE protocol traffic flows. All subsequent bytes through the relay
are ChaCha20-Poly1305 ciphertext from libp2p's secure channel.
On top of that, Y7KE wraps every `/y7ke/msg/1.0.0` payload in
`MessageEnvelope { sender_pub, ciphertext: ChaCha20-Poly1305(static_dh_conv_key, plaintext), sig: Ed25519 }`. Even if the libp2p
secure channel layer were broken, the inner Y7KE encryption
remains.

### Downgrade resistance

A `Direct` connection is strictly preferred over `Relayed` via
`best_kind()` precedence. A malicious relay cannot force a peer
back onto the relayed path — once a direct connection exists,
libp2p picks it for new streams. The relay can drop the relayed
stream, but that triggers the existing `ConnectionClosed` →
re-dial flow and the direct path remains.

### Identity validation

Every libp2p connection ends Noise XX with a verified Ed25519
public key. Y7KE additionally derives `Y7Id` from that pubkey
(`y7_id_from_peer_id` in `crates/y7ke-net/src/swarm.rs`) and the
inbound `handle_handshake` arm at `crates/y7ke-app/src/event_loop.rs:149-165`
verifies the application-layer Ed25519 signature in
`HandshakeReq.initiator_ed25519_pub` matches the libp2p PeerId.
Mismatch → refuse without touching storage. This is the dual-binding
that prevents a malicious libp2p peer from claiming a different Y7
identity than the one their TLS key corresponds to.

### Replay resistance

Each `MessageEnvelope` carries a UUIDv7 `message_id`. The receiver
`INSERT OR IGNORE`s into `messages`. A replayed envelope is a no-op
at storage and emits no UI event. Sync envelopes are filtered in
`event_loop.rs::SyncReq::Pull` to only those signed by us (the
recent fix that closed the "synced envelope signed by wrong key"
WARN spam).

### Transport integrity

Noise XX guarantees end-to-end channel integrity below libp2p; the
Y7KE Ed25519 signature on every envelope provides app-layer
integrity above. Two independent layers.

### Relay trust minimisation

The bootstrap repo has zero `y7ke-*` dependencies (CLAUDE.md pins
this). It physically cannot decode `MsgReq`, `HandshakeReq`,
`SyncReq` — those types don't exist in its binary. Operator
compromise of the VPS gives only `libp2p_relay::Event::CircuitReqAccepted`
metadata: which `PeerId` opened a circuit to which other `PeerId`
at which time. No conversation content, no Y7 IDs (since `Y7Id`
isn't a libp2p concept), no message rates above the per-peer
circuit limit.

---

## 10. Diagnostics strategy

Borrowed shape from iroh's `iroh doctor` UX: one screen, structured
fields, copy-paste-able.

### Tracing targets

Every dial, every connection-kind transition, every DCUtR attempt
emits a structured `tracing` event:

```
RUST_LOG=y7ke_net=debug,libp2p_dcutr=info,libp2p_relay=info,libp2p_autonat=info
```

Existing fields in current logs (verified via `crates/y7ke-net/src/swarm.rs::addr_class`):
`peer`, `addr`, `class=lan|loopback|internet|relay|quic|other`,
`kind=Lan|Internet|Relayed|Direct`. Phase 2 step 4 adds:
`upgrade_attempt={n}`, `upgrade_outcome=Pending|Succeeded|Failed`,
`backoff_remaining_s={n}`.

### Per-peer connection meta

New `ConnectionMeta { via: Option<String>, transport: Transport, last_rtt_ms: Option<u32>, last_reconnect_at: Option<i64>, dcutr_last: Option<DcutrOutcome> }`
tracked in `event_loop.rs` (Phase 2 step 7), exposed via a new
`list_active_connections() -> Vec<ConnectionView>` Tauri command.
Indexed by `Y7Id`.

### NAT status

`AutoNATv2` verdict (`Public | Private | Unknown`) persisted in
`AppInner::nat_status` (Phase 2 step 2). Surfaces as a pill in the
Connectivity pane. Drives the upgrade loop's "should we even try
direct" decision.

### Aggregate counters

`AppInner::dcutr_attempts: AtomicU64`,
`AppInner::dcutr_successes: AtomicU64`,
`AppInner::dcutr_failures: AtomicU64`. Read via
`get_dcutr_stats() -> { attempts, successes, failures, success_rate }`
Tauri command. Displayed as a single line in the Connectivity pane:
"DCUtR: 5 / 7 (71 %)".

### UI surface

New view `ui/src/views/Connectivity.svelte` reachable from the
Sidebar (`connectivity O.O` NavItem). Sections:

- **System**: NAT status pill, dial mode, active bootstraps with
  per-bootstrap reservation state + last RTT, DCUtR aggregate
  success rate.
- **Active connections**: for each Accepted contact with a
  non-Offline presence, one row with `y7_id (nickname)`, `kind`
  badge (Direct/Lan/Internet/Relayed), `transport` (QUIC/TCP), `via`
  if Relayed, last RTT, DCUtR last-attempt status (succ/fail/never).

Minimal monochrome — existing tokens, no charts, no animations
beyond existing transitions.

---

## 11. Real-world testing matrix

| Scenario | Expected behaviour | Log signature |
|---|---|---|
| **home ↔ home NAT (full-cone)** | DCUtR upgrades within 3 s of relay reservation | `dcutr: direct upgrade succeeded peer=… in 1.8s` then `connection_kind=Direct` |
| **home ↔ mobile CGNAT** | Relay first, DCUtR attempt fails, upgrade loop re-tries on AutoNAT flip / ObsAddr churn, ~50 % succeed on retry | `dcutr: direct upgrade failed (staying on relay) peer=… error=…` then later `upgrade_attempt=2 outcome=Succeeded` OR permanent `RelayPermanent` |
| **mobile ↔ mobile (both CGNAT)** | Permanent relay; sync/msg flow over `/p2p-circuit` indefinitely | `upgrade_attempt=3 outcome=Failed`, no further attempts |
| **VPN ↔ NAT** | Direct preferred (VPN often has public IP), relay fallback if VPN blocks UDP | `connection_kind=Direct transport=Quic` |
| **IPv6 ↔ IPv4** | Direct fails (no shared family), relay path established | `dcutr: direct upgrade failed`, stays Relayed |
| **relay restart** | All clients ConnectionClosed → 15 s reconnect → reservation re-acquired → DCUtR re-fires for active peers | `connection closed peer=<bootstrap>` then `redialing lost bootstrap` then `relay: reservation accepted` |
| **bootstrap restart** | Same as relay restart (single node in production) | identical signatures |
| **WiFi switching** | If QUIC: connection migrates (PATH_CHALLENGE/RESPONSE), chat continues. If TCP: drop + redial + DCUtR re-fires | `quinn: path validated new_addr=…` (libp2p-quic permitting) OR `connection closed cause=Closed` + redial |
| **suspend / resume** | Same as WiFi switching; presence ticker (30 s) + observed-addr push kick re-resolution within ~1 s of wake | `check_live=false → presence=Offline` then `connection established kind=Internet` |
| **mid-chat upgrade** | Messages route through best path; switch is silent — no UI flicker beyond the badge changing RELAY → DIRECT | `AppEvent::PresenceChanged { kind: Direct }` lands while `/y7ke/msg/1.0.0` continues |

### Test coverage today vs target

| Test | Today | Phase 3 target |
|---|---|---|
| `two_node.rs` (handshake over loopback) | ✓ | unchanged |
| `four_node_relay.rs` (Kad + relay loopback) | ✓ | unchanged |
| `v2_dcutr_smoke.rs` (DCUtR loopback) | ✓ | extend to assert ≤3 s after push fix |
| `quic_listen_smoke.rs` (QUIC listens at boot) | ✓ | unchanged |
| `v2_sync_reconcile.rs` (sync via mDNS) | ✓ | unchanged |
| sync via /p2p-circuit | ✓ folded into `four_node_relay.rs` (Phase 3.1) | extend with offline-queue drain |
| `autonat_smoke.rs` (AutoNAT verdict) | — | **new — Phase 2 step 2** |
| netns NAT sim — blocked path (relay required) | ✓ `scripts/nat-sim/run.sh` + `nat_sim_node` | — |
| netns NAT sim — symmetric NAT (DCUtR fallback) | ✓ `scripts/nat-sim/run-symmetric.sh` | — |
| `v2_transport_migration.rs` (IP change → QUIC migration) | — | **new — Phase 3 step 12** |
| `live_relay_smoke` (live VPS) | ✓ TCP | extend with QUIC variant — Phase 2 step 5 |
| **manual cross-network smoke** | — | **new — Phase 3 step 13, captured in this doc as ground truth** |

The manual cross-network smoke (home WiFi PC ↔ mom's mobile 4G
laptop) is the canonical acceptance gate. Logs from both machines
get attached to this doc post-execution as the artefact that says
"yes, real-world direct-first works", or, failing that, names the
specific NAT class that forced relay and the upgrade-loop attempt
history.

**netns NAT-sim finding (2026-05-28).** The local netns harness proved
the relay-fallback path under both *blocked-path* (`run.sh`) and
*symmetric-NAT* (`run-symmetric.sh`) conditions: DCUtR fires with the
correct identify-observed mapped addresses on both sides, fails to
punch, and the relay connection survives intact — the "stable relay
beats reconnect chaos" invariant holds. Notably **stock Linux
iptables/nftables `MASQUERADE` cannot simulate a full-cone (endpoint-
independent) NAT**: conntrack shows the same client UDP port mapping to
*different* external ports per destination (one toward the relay, another
toward the peer) — i.e. it behaves as a symmetric NAT, which DCUtR is
documented to be unable to punch. A true full-cone sim would require the
out-of-tree `FULLCONENAT` kernel module. The DCUtR *success* path is
therefore validated by the loopback `v2_dcutr_smoke.rs`; an end-to-end
NAT hole-punch must be confirmed by the real-world cross-network smoke.

---

## 12. Implementation roadmap

Maps to the 14 tasks in the approved plan
(`~/.claude/plans/check-technical-task-md-and-treat-glittery-moore.md`).
Each commit is independently `git revert`-able; each phase has a
clean test boundary.

### Phase A — relay stabilisation (already shipped in v0.1.61)

- ✓ Circuit Relay v2 client + server
- ✓ Auto-reconnect on bootstrap drop (15 s)
- ✓ Reservation refresh on reconnect
- ✓ DialMode mutual-exclusivity (LanOnly / Internet / P2p) + live
  apply
- ✓ External-addr advertisement on bootstrap

Risk closed. Rollback: not needed (shipped).

### Phase B — QUIC transport (already shipped in v0.1.61, bootstrap pending)

- ✓ Client: dual TCP + QUIC listeners
- ✓ `sort_addrs_for_dial` prefers QUIC > TCP > circuit
- ☐ Bootstrap: add QUIC (Phase 2 step 5 — **this plan**)
- ☐ Verify connection migration end-to-end (Phase 3 step 12)

Risk: bootstrap QUIC deploy is the only risky bit. Rollback:
`systemctl restart y7ke-bootstrap` rolls back via the GitHub
release auto-update pinning to the previous tag.

### Phase C — DCUtR + hole punching (basics shipped; upgrade loop new)

- ✓ DCUtR client behaviour wired
- ✓ `ConnectionKind::Direct` emitted on success
- ☐ identify push of listen-addr updates (Phase 2 step 1)
- ☐ DCUtR failure event + counters (Phase 2 step 3)
- ☐ Aggressive upgrade-from-relay loop (Phase 2 step 4)
- ☐ AutoNAT v2 client + server (Phase 2 step 2)

Risk: aggressive upgrade loop adds DCUtR signalling traffic.
Bounded by exp backoff. Rollback per commit.

### Phase D — diagnostics + optimisation

- ☐ Connectivity debug pane (Phase 2 step 6)
- ☐ Accurate RELAY tooltip with `via` + `transport` (Phase 2 step 7)
- ☐ Instant settings live-apply + stale-relay sweep (Phase 2 step 8)
- ☐ Reconnect-storm backoff (Phase 3 step 11)
- ☐ Sync-over-relay test (Phase 3 step 9)
- ☐ NAT-sim test harness (Phase 3 step 10)
- ☐ QUIC migration test (Phase 3 step 12)
- ☐ Live cross-network manual log capture (Phase 3 step 13)

Risk: UI changes touch existing components; CI catches regressions.
Test additions only add coverage. Rollback per commit.

### Dependencies between phases

- Phase 2 step 4 (upgrade loop) **depends on** step 1 (identify
  push) and step 2 (AutoNAT) — without them the loop has no
  signal to act on.
- Phase 2 step 5 (bootstrap QUIC) **depends on** Phase 1 doc
  research confirming libp2p-quic can talk to the bootstrap over
  QUIC without library-level surprises. Done in the doc above.
- Phase 3 step 10 (NAT-sim) **depends on** Phase 2 steps 1-4
  being merged so the upgrade loop can be exercised.
- Phase 3 step 13 (manual smoke) is the gate — only run after
  Phase 2 + Phase 3 implementation commits land.

### Rollback considerations

Each numbered commit ships alone. `git revert` is the rollback
primitive. The doc itself (this file) is reverted by reverting its
single commit; subsequent implementation references the *current*
revision of the doc, not a frozen copy, so an in-progress doc
revision doesn't strand pending commits.

### Out of scope (V3 or later)

- Multi-relay reservations (need ≥2 production relays first)
- Tailscale-style birthday-attack symmetric NAT defeat
- PCP/NAT-PMP/UPnP port mapping
- Bootstrap-cluster failover orchestration
- Discovery / relay role separation (Nebula-style)
- Mobile (Tauri Mobile) port
- Double Ratchet forward secrecy (B-track)
- Onion routing, relay meshes, distributed consensus — explicitly
  rejected by user

---

## Live manual smoke results

*(To be filled in after Phase 3 step 13 execution. Will contain:
home WiFi machine y7 ID, mobile 4G machine y7 ID, log captures
from both, observed connection kind progression, DCUtR attempt
counts, any rejected scenarios with the NAT-class diagnosis.)*

---

## References

External:

- [Circuit Relay v2 spec](https://github.com/libp2p/specs/blob/master/relay/circuit-v2.md)
- [DCUtR spec](https://github.com/libp2p/specs/blob/master/relay/DCUtR.md)
- [libp2p hole punching concepts](https://libp2p.io/docs/hole-punching/)
- [rust-libp2p discussion #5741](https://github.com/libp2p/rust-libp2p/discussions/5741) (idle connection timeout, transport observations)
- [rust-libp2p discussion #3210](https://github.com/libp2p/rust-libp2p/discussions/3210) (multi-relay reservation strategy)
- [rust-libp2p issue #4007](https://github.com/libp2p/rust-libp2p/issues/4007) (`NoAddresses` during DCUtR upgrade)
- [iroh repository](https://github.com/n0-computer/iroh)
- [iroh.computer](https://www.iroh.computer/)
- [Tailscale: how NAT traversal works](https://tailscale.com/blog/how-nat-traversal-works)
- [Tailscale DERP](https://tailscale.com/kb/1232/derp-servers)
- [tailscale/magicsock source](https://github.com/tailscale/tailscale/tree/main/wgengine/magicsock)
- [TokTok c-toxcore](https://github.com/TokTok/c-toxcore/tree/master/toxcore)
- [slackhq/nebula](https://github.com/slackhq/nebula)
- [RFC 9000 — QUIC](https://www.rfc-editor.org/rfc/rfc9000)
- [arXiv 2510.27500 — Challenging Tribal Knowledge: Large Scale Measurement Campaign on Decentralized NAT Traversal](https://arxiv.org/abs/2510.27500)

Y7KE internal:

- `CLAUDE.md` — repository conventions
- `docs/ROADMAP.md` — phase tracking
- `docs/ARCHITECTURE.md` — V1+V2 architecture
- `~/.claude/plans/check-technical-task-md-and-treat-glittery-moore.md` — approved V2 hardening plan
