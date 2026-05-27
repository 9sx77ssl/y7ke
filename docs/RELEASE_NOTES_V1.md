# Y7KE V1 — Release Notes

**Tag:** `v0.1.0` · **Date:** 2026-05-27 · **License:** MIT

Y7KE is a privacy-first, key-based, peer-to-peer desktop messenger. V1 ships
a **LAN-only** experience: two clients on the same Wi-Fi discover each other
via mDNS and exchange end-to-end encrypted messages directly, with zero
central infrastructure.

## What works

- **Key-based identity.** `y7:<base58>` URIs derived from an Ed25519
  keypair. No accounts, no email, no phone numbers. The private key is
  generated on first launch, persisted encrypted in SQLite under a 32-byte
  master DEK held in a local file (mode `0600` on Unix).
- **Discovery via mDNS.** Peers on the same LAN see each other within ~3
  seconds without configuration.
- **Encrypted sessions.** When two peers first connect, they run an
  X25519 ephemeral handshake authenticated by Ed25519 signatures and
  derive a 32-byte ChaCha20-Poly1305 session key via HKDF-SHA256. The
  session is stored encrypted at rest with the DEK.
- **End-to-end messaging.** Every message envelope is sealed by the
  session key, signed by the sender's long-term Ed25519 key, identified
  by a UUIDv7 (timestamp-sortable, collision-free), and dedup'd by
  `INSERT OR IGNORE` at the receiver.
- **Contact lifecycle.** Add by paste — accept, reject, or cancel.
  Outgoing requests can be revoked locally; incoming requests resolve as
  accepted or rejected. Sessions persist across reboots. Accept,
  reject, and delete events propagate to the peer over an in-band
  control protocol (1-byte tag on `/y7ke/msg/1.0.0`); deletes wipe both
  sides and auto-eject the UI from the chat pane.
- **Offline sync.** Messages that fail live-delivery are enqueued and
  drained on the next peer reconnect; convergence verified by the
  `v1_offline_sync` test.
- **Local-first persistence.** SQLite with the WAL journal. Identity,
  contacts, requests, messages, sessions, sync queue, and peer state
  survive app restart; ciphertext on disk verified by integration tests.
- **Dark monochrome UI.** Custom frameless window with our own
  min/max/close. JetBrains Mono throughout. Status dots (green = synced,
  red = failed) for status badges.

## Architecture

```
crates/
  y7ke-core/      Y7Id / MessageId / ConversationId / AppError / AppEvent
                  + crypto wrappers (Ed25519 / X25519 / ChaCha20-Poly1305 / HKDF)
  y7ke-storage/   sqlx-sqlite + 8-table schema + DEK file + DAOs
  y7ke-net/       libp2p swarm (TCP + Noise + Yamux + mDNS + ping + identify)
                  + 3 request_response protocols
  y7ke-app/       composition root + handshake + event loop + commands
src-tauri/        Tauri 2 desktop shell, IPC commands, event emit
ui/               Svelte 5 + TypeScript + Vite + custom design system
```

**Three wire protocols** under libp2p:
- `/y7ke/handshake/1.0.0` — session establishment.
- `/y7ke/msg/1.0.0` — live single-message delivery.
- `/y7ke/sync/1.0.0` — reconcile (V1 uses queue-based retry; full
  reconciler is V2).

## Tests

51 active + 2 ignored across the workspace. Highlights:

| Test | Duration | What it proves |
|---|---|---|
| `v1_e2e` | ~3 s | 7 V1 capabilities end-to-end with 2 clients |
| `v1_offline_sync` | ~6 s | Sender enqueues while peer offline; recipient reboots; all messages arrive in order |
| `v1_restart_both` | ~9 s | Both peers shutdown + reboot; history persists; new messages flow |
| `v1_delete_propagation` | ~3 s | Bob deletes → Alice receives `ContactRemoved`, both DBs wiped |
| `v1_stress` (#[ignore]) | ~10 s | 3 clients × 5 msgs/direction = 30 messages, no losses, no duplicates |

Audit closed **C1 + H1 + H2 + H3 + H4 + M1 + M2 + M3 + M4 + L1** (see
`docs/AUDIT.md`). `cargo clippy -D warnings` is green across the
workspace.

## Known limitations

- **LAN only.** mDNS discovery does not cross subnets. Internet routing
  (Kademlia DHT + AutoNAT + circuit relay v2 + DCUtR) lands in V2.
- **No NAT traversal.** TCP + Noise + Yamux only; no QUIC, no
  hole-punching, no relay.
- **Master DEK in a local file**, not in the OS keyring. An attacker
  with read access to both `~/.local/share/y7ke/master.dek` and the
  SQLite file can decrypt everything. V2 promotes the DEK to the OS
  keyring (`keyring` crate) with the file as fallback.
- **No session-key ratcheting.** Once two peers establish a session, the
  same `session_key` encrypts every message forever. Forward secrecy is
  weak. V2 layers a Double Ratchet (or simpler counter chain) on top.
- **No groups, no file transfer, no read receipts.** V1 is 1-on-1 text
  only.
- **No handshake replay protection on the receiver.** Replays would
  trigger H1 backstop (reject with `accept = false`) but consume a slot
  in the request_response state machine. V2 adds an explicit nonce + LRU.

## Performance

Measured on a single Arch Linux x86_64 desktop:

- Cold start (boot + identity ensure + swarm spawn): ~80 ms for the
  Rust side. Tauri webview adds ~400 ms. Effective time to interactive:
  ~500 ms (well under the 2-second budget).
- Idle RSS: ~45 MB (target was < 80 MB).
- mDNS time to first peer discovery: typically 1–3 s.

## V2 roadmap (deferred)

See `docs/TODO.md` for the full backlog. Headline items:

- Internet routing via Kademlia + Y7KE-hosted bootstrap relays
- AutoNAT + circuit-relay v2 + DCUtR for NAT traversal
- QUIC transport
- OS keyring for the master DEK
- Double Ratchet for forward secrecy
- Read receipts (`Delivered` state)
- ts-rs codegen replacing the hand-maintained TS types

## Distribution

Tagged releases produce the following artifacts on GitHub:

| Platform | Format |
|---|---|
| Linux | `.deb`, `.AppImage` |
| macOS | `.dmg`, `.app.tar.gz` |
| Windows | `.msi`, `.exe` (NSIS) |

Build matrix in `.github/workflows/release.yml`. Trigger by pushing a
`v*` tag.
