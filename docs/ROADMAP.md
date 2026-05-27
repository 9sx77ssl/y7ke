# Y7KE Roadmap

## V1 — LAN end-to-end ✓ (shipped 2026-05-27)

Two people on the same LAN talk privately with zero infrastructure. All
test groups green, audit closed, UX stabilization batch landed (`U1`–`U7`
in `docs/TODO.md`).

What's in the V1 binary:

- Identity, contacts, requests, encrypted chat, restart-safe history,
  offline sync queue
- mDNS-only discovery, libp2p TCP + Noise + Yamux
- ChaCha20-Poly1305 / Ed25519 / X25519 / HKDF — every primitive from
  audited Rust crates
- **Static-DH per-conversation key derivation** — session keys never
  hit disk; derived on demand from the long-term identity scalar +
  peer's Ed25519 pubkey (`HKDF(X25519(my_static, peer_static), conv_id)`).
  Stealing the SQLite DB without the master DEK yields ciphertext only.
- SQLite `PRAGMA secure_delete = ON` so wiped messages and sessions are
  zero-filled in freed pages.
- In-band control protocol (Accept / Reject / Delete) over the message
  channel with auto-eject; delete propagates bilaterally — both copies
  vanish when the peer is next online.
- Tauri 2 + Svelte 5 frontend, custom dark monochrome design system,
  JetBrains Mono throughout. Frameless window with manual resize handles
  and rounded corners. Toast queue capped at 2 with FIFO eviction.

What V1 deliberately does **not** ship: internet routing, NAT traversal,
forward secrecy, OS keychain, group chats, file transfer, read receipts.

## V2 — Internet routing + crypto uplift (target: ~4–6 weeks)

V2 transforms Y7KE from a LAN demo into a real product. Two parallel
tracks; A is the bigger lift and unlocks real users, B is a focused
security improvement that runs alongside.

### Track A — Internet reachability (critical path)

> **Goal:** two Y7KE peers on different home networks behind NATs talk
> directly to each other (or via relay when DCUtR fails), without
> the user configuring anything.

1. **A1 — DHT-based peer lookup.** Add libp2p Kademlia with
   `MemoryStore`. Replace the mDNS-only discovery in `crates/y7ke-net`
   with a chain: mDNS cache → `peer_state.last_addrs` → Kad lookup.
2. **A2 — Y7KE bootstrap relays.** A small `crates/y7ke-bootstrap`
   binary running the same swarm minus contacts/storage, deployed
   behind a static IP. The default Y7KE build ships 2–3 hardcoded
   bootstrap multiaddrs; `~/.config/y7ke/bootstrap.toml` overrides
   them.
3. **A3 — AutoNAT v2.** Detect whether we're publicly reachable; cache
   the result and surface it as a status pill in the UI (`public` /
   `private` / `unknown`).
4. **A4 — Circuit relay v2.** Enable both the client and (on bootstrap
   nodes) the server side. When direct dial fails, retry via relay.
5. **A5 — DCUtR (hole-punching).** Upgrade relay-routed connections to
   direct in `ConnectionEstablished` handlers. Add a
   `ConnectionKind::DirectAfterHolepunch` variant so the UI can render
   the upgrade.
6. **A6 — QUIC transport.** Add `libp2p-quic` to the swarm so UDP-only
   networks have a path. Keep TCP+Noise+Yamux as the fallback.

### Track B — Crypto + secret-storage uplift

1. **B1 — OS keyring for master DEK** (`CR2`). `keyring` crate;
   `Y7KE_DEK_FILE` stays as the headless fallback. Migration: if the
   keyring is empty but the file exists, import.
2. **B2 — Double Ratchet for forward secrecy** (`CR1`). Wraps the
   existing static session key; introduces a per-message DH ratchet +
   chain key advance. Persistence: extend the `sessions` table with a
   `ratchet_state_enc BLOB` column.
3. **B3 — Handshake replay nonce** (`CR3`). 16-byte random in
   `HandshakeReq`, server-side LRU(1024 entries, 60 s TTL).

### Track C — Sync correctness & operational polish

1. **C1 — Initiator-side `/y7ke/sync/1.0.0`** (`A2` in TODO). Today the
   responder code exists but no client calls it; either implement the
   3-round reconcile or remove the responder. Pick implement.
2. **C2 — Read receipts** (`Delivered` state). Tag-byte 0x02 control:
   `MessageDelivered { message_id }`. Receiver emits on
   `INSERT OR IGNORE` success; sender flips status `Sent → Delivered`.
3. **C3 — Rate limiter** (`S2`). Per-peer leaky bucket on inbound
   handshake / msg / sync; deny with `accept = false` when exceeded.
4. **C4 — Non-blocking boot** (`P2`). Move `AppHandle::boot` off the
   `setup` hook, register state when ready, show a splashscreen.

### Track D — Tooling

1. **D1 — ts-rs codegen.** `#[derive(TS)]` on `AppEvent`, command args,
   command results. Eliminates the hand-maintained `ui/src/lib/types.ts`.
2. **D2 — Playwright E2E.** Replace the LAN-only manual smoke test
   with a scripted scenario.

### V2 sequencing

```
weeks 1–2:  A1 + A2  (Kad + bootstrap)
weeks 2–3:  A3 + A4  (AutoNAT + relay)
weeks 3–4:  A5 + A6  (DCUtR + QUIC)
weeks 1–3:  B1, B3   (in parallel with track A — small, isolated)
weeks 4–6:  B2       (Double Ratchet — biggest single piece)
weeks 4–6:  C1, C2, C3, C4 (rollover from track A as bandwidth frees up)
weeks 5–6:  D1, D2   (tooling — last so types are stable)
```

V2 ships when:

- Two peers across the open internet (different ISPs, both behind NAT)
  exchange messages with no manual config.
- All sessions advance the ratchet on every message; compromise of a
  current key does not decrypt history.
- The master DEK lives in the OS keyring on macOS, Windows, and any
  Linux with `libsecret` available.
- Read receipts visible in the UI.

## V3 — Groups, files, anonymous routing

- Group conversations (multi-party messaging via pairwise sessions
  first, then a native group ratchet — MLS or Olm-style)
- File transfer (chunked + resumable, Bitswap-inspired)
- Optional onion / anonymous routing (Tor-over-libp2p or in-protocol)
- Mobile (Tauri Mobile or sister app)
