# Y7KE Roadmap

## Status snapshot

```
V1 LAN messenger          ✓ shipped (2026-05-27, v0.1.18)
V2 Track A — Internet     ◐ A1+A2+A4 shipped (v0.1.43), A3+A5+A6 pending
V2 Track B — Crypto uplift ◯ not started
V2 Track C — Sync polish   ✓ shipped (C1-C4 all in main)
V2 Track D — Tooling       ◐ D1 done, runtime Settings + dial-mode UI shipped (v0.1.43), D2 pending
V3 Groups / files / Tor    ◯ not started
```

## Lessons learned

When the UI silently drops content, **read the WebKit JavaScript console
first** (Ctrl+Shift+I in any Tauri window). A single `class:failed`
Svelte binding without an `={expr}` threw `ReferenceError` on every
`MessageBubble` render with `is_mine=true`, Svelte killed the entire
`{#each}` tree, and the chat displayed zero bubbles while the backend
logs proudly reported `stateLen=10`. Backend tracing is necessary but
not sufficient — the actual error lived in the browser context. Use
`~/y7ke-dev-two.sh` (outside the repo) to launch two debug-build
instances with source maps so a future stack trace points to
`Component.svelte:line` instead of `index-XXX.js:8061`.

## V1 — LAN end-to-end ✓ (shipped 2026-05-27, v0.1.18)

Two people on the same LAN talk privately with zero infrastructure. All
test groups green. Every UX rough edge from the stabilization pass
landed.

What V1 ships:

- Identity, contacts, requests, encrypted chat, restart-safe history,
  offline `sync_queue` retry, and `/y7ke/sync/1.0.0` 3-round reconcile
  on reconnect.
- mDNS-only discovery, libp2p TCP + Noise + Yamux + identify + ping.
- ChaCha20-Poly1305 / Ed25519 / X25519 / HKDF — every primitive from
  audited Rust crates.
- **Static-DH per-conversation keys.** Session keys are never persisted;
  derived on demand via `HKDF(X25519(my_static, peer_static), conv_id,
  "y7ke-conv-v1")`. The `sessions` table only records handshake
  completion. SQLite + `PRAGMA secure_delete = ON`. Stealing the DB
  without the master DEK file gives ciphertext only.
- In-band control protocol (Accept / Reject / Delete) piggy-backed on
  `/y7ke/msg/1.0.0` with a 1-byte tag. Delete-contact propagates
  bilaterally — both copies vanish when the peer is next online;
  auto-eject from the chat view on local or remote removal.
- Per-peer leaky-bucket rate limiter on inbound handshake / msg / sync.
- Non-blocking `AppHandle::boot` — the window appears before the
  swarm is up; the command surface gates on `app.get().await`.
- Tauri 2 + Svelte 5 + Vite + TypeScript. Custom dark monochrome design
  system, JetBrains Mono throughout. Frameless window with manual
  resize handles and rounded corners. Toast queue capped at 2 with
  FIFO eviction. `+ add contact` → `add contact ^.^`, `requests` →
  `requests >.<`.
- Three release artifacts per `v*` tag: `.deb`, `.AppImage`,
  `y7ke-linux-x86_64.tar.gz` raw binary, plus `.dmg` (macOS) and
  `.msi`/`.exe` (Windows). Built via `cargo build --release -p
  y7ke-tauri --features custom-protocol` so the binary embeds its own
  frontend.

What V1 does **not** ship: internet routing, NAT traversal, forward
secrecy, OS keychain, group chats, file transfer.

## V2 — what's left (Track A + Track B + D2)

### Track A — Internet reachability ◐

> **Goal:** two Y7KE peers on different home networks behind NATs talk
> directly to each other (or via relay when DCUtR fails), without
> the user configuring anything.

1. **A1 — DHT-based peer lookup.** ✓ Shipped (v0.1.20). libp2p Kademlia
   `Behaviour<MemoryStore>` in server mode, protocol `/y7ke/kad/1.0.0`.
   Discovery chain in `crates/y7ke-app/src/app/contacts.rs`:
   `net.dial(peer)` (mDNS cache + identify) → `peer_state.last_addrs`
   → `net.find_peer(y7)` Kad lookup → `dial_address`. `peer_state`
   persists every `addrs` seen so cold-restart skips the Kad round-trip.
2. **A2 — Bootstrap node.** ✓ Shipped (v0.1.20). Standalone crate
   `9sx77ssl/y7ke-bootstrap` (separate repo), zero `y7ke-*` deps —
   pure libp2p Kad + identify + ping. `install.sh` does six minimal
   steps (download, user, systemd, optional firewall, start, print
   PeerId). Client uses `DEFAULT_BOOTSTRAPS` from
   `y7ke-net/src/swarm.rs` with `~/.config/y7ke/bootstrap.toml` and
   `Y7KE_BOOTSTRAP` env-var override.
3. **A3 — AutoNAT v2.** Detect public reachability, cache result,
   surface as a status pill in the UI (`public` / `private` /
   `unknown`).
4. **A4 — Circuit relay v2.** ✓ Shipped (v0.1.43). Client carries
   `libp2p::relay::client::Behaviour`; bootstrap (separate repo,
   `y7ke-bootstrap` v0.1.4) carries `libp2p::relay::Behaviour` as
   server. Each client proactively reserves a `/p2p-circuit` slot at
   every configured bootstrap on `ConnectionEstablished` and a 15-s
   reconnect tick redials any bootstrap that drops, clearing
   `relay_reserved` so the next reconnect re-runs `listen_on`. The
   bootstrap's external addresses are declared via
   `--external-addr` / `Y7KE_BOOTSTRAP_EXTERNAL_ADDR` so the
   reservation acknowledgement carries dialable addrs (without this
   libp2p errors out with `NoAddressesInReservation`). The relay only
   shuffles ciphertext frames — Noise + ChaCha20-Poly1305 wrap every
   byte before it leaves the client. Connection kind reported via
   `ConnectedPoint::is_relayed()` →
   `ConnectionKind::Relayed`; UI shows a `RELAY` label in a muted
   lilac next to the peer nickname.
5. **A5 — DCUtR hole-punching.** Upgrade relay-routed connections to
   direct in `ConnectionEstablished`. Add
   `ConnectionKind::DirectAfterHolepunch` so the UI shows the upgrade.
6. **A6 — QUIC transport.** `libp2p-quic` for UDP-only networks; keep
   TCP+Noise+Yamux as fallback.

#### Runtime Settings UI (shipped v0.1.43, lands alongside A4)

User-controllable connection policy and bootstrap roster. Stored in
the encrypted SQLite (`settings` single-row table, migration
`0004_settings.sql`). Surfaces in the UI via the
`settings :3` Sidebar nav item.

- **Dial modes** (LAN / Internet / Relay / P2P). Each step of
  `dial_with_discovery` reads `Settings::dial_modes` and filters the
  multiaddrs it tries. `P2P` is a UI-visible stub for A5 — toggling
  it has no transport effect yet but the Settings page already
  shows the future "p2p preferred, relay fallback" strategy strings.
- **Bootstrap roster.** The hardcoded `DEFAULT_RELAY_BOOTSTRAP`
  (`bootstrap1.y7v.lol`) is always returned first by
  `list_bootstraps` with `is_default=true` and is rendered locked
  in the UI (input is `readonly`, no delete `×`). Extras live in
  `Settings::extra_bootstraps` and are user-editable.
- **TCP-connect pings.** `ping_all_bootstraps` opens raw TCP with a
  5-s timeout per multiaddr (DNS-resolve `/dns4` → `/ip4`),
  parallel via `tokio::join_all`. Results cached in
  `AppInner::bootstrap_pings`.
- **Live re-sync.** `update_settings` emits
  `AppEvent::SettingsChanged` and dispatches `NetCommand::UpdateBootstraps`
  to the swarm task, which re-syncs its `bootstrap_peers` map and
  dials any new entries without restart.

### Track B — Crypto + secret-storage uplift ◯

1. **B1 — OS keyring for master DEK.** `keyring` crate; `Y7KE_DEK_FILE`
   stays as the headless fallback. Migration: import file → keyring on
   first run.
2. **B2 — Double Ratchet for forward secrecy.** Wraps the existing
   static session key with a per-message DH ratchet + chain key
   advance. Extend `sessions` with a `ratchet_state_enc BLOB` column.
3. **B3 — Handshake replay nonce.** 16-byte random in `HandshakeReq`,
   server-side LRU(1024 entries, 60 s TTL).

### Track D — Tooling

- **D1 — ts-rs codegen.** ✓ Done. `#[derive(TS)]` on `AppEvent`, view
  types, command args. Files generated into `ui/src/lib/gen/` from
  `cargo test`.
- **D2 — Playwright E2E.** ◯ Pending. Replace the LAN-only manual
  smoke test with a scripted scenario covering identity → add contact
  → accept → send → restart → history intact.

### Done in V2 Track C (no further work needed)

- **C1 ✓** Initiator-side `/y7ke/sync/1.0.0` 3-round reconcile (`Header
  → Pull → Ack`).
- **C2 ✓** Read receipts — `Delivered` flips when peer acks the
  `MsgReq`; `Synced` reserved for `/y7ke/sync/1.0.0` Ack.
- **C3 ✓** Per-peer leaky-bucket rate limiter.
- **C4 ✓** Non-blocking `AppHandle::boot`.

### Suggested V2 sequencing (3–5 weeks remaining)

```
✓ done:      A1 + A2 + A4 + Settings UI + runtime dial modes
weeks 1–2:   A3 + A5     AutoNAT + DCUtR (relay → direct upgrade)
weeks 2–3:   A6          QUIC transport (UDP-friendly networks)
weeks 1–2:   B1, B3      OS keyring + handshake replay nonce — isolated
weeks 3–5:   B2          Double Ratchet (biggest single piece)
weeks 4–5:   D2          Playwright E2E after types stabilise
```

V2 ships when:
- Two peers across the open internet (different ISPs, both behind NAT)
  exchange messages with no manual config.
- All sessions advance the ratchet on every message; compromise of the
  current key does not decrypt history.
- The master DEK lives in the OS keyring on macOS, Windows, and any
  Linux with `libsecret` available.
- A Playwright suite covers the full V1 acceptance scenario.

**Known gaps carried into 3.0 (polish, not blockers):**
- Live cross-NAT QUIC hole-punch is proven on loopback + a netns NAT sim and
  the QUIC relay reservation is verified against the production bootstrap, but
  a two-machine / two-ISP field confirmation is still pending.
- A block/unblock management UI: `ContactStatus::Blocked` is reachable (by
  rejecting a request) but cannot yet be viewed or undone from the app.

## V3 — Groups, files, anonymous routing ◯

- Group conversations (pairwise sessions first, then a native group
  ratchet — MLS or Olm-style).
- File transfer (chunked + resumable, Bitswap-inspired).
- Optional onion / anonymous routing (Tor-over-libp2p or in-protocol).
- Mobile (Tauri Mobile or sister app).
