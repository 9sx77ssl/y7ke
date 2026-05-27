# Y7KE

A privacy-first, key-based, peer-to-peer desktop messenger.

- **No accounts, no email, no phone numbers.** Your identity is a public key.
- **No central server.** Peers discover each other directly and exchange messages over libp2p.
- **End-to-end encrypted.** ChaCha20-Poly1305 sessions derived from an X25519 handshake.
- **Local-first.** Messages persist in an encrypted SQLite database on disk.
- **Offline-tolerant.** Undelivered messages queue locally and drain on reconnect.

## Status — V1 LAN messenger

V1 ships an end-to-end-tested **LAN messenger**:

1. Generate identity on first launch
2. Add a contact by pasting their `y7:<base58>` URI
3. Accept incoming contact requests
4. Open a chat with any accepted contact
5. Exchange encrypted messages live
6. Survive app restart (encrypted history reloads)
7. Sync queued messages after either side reconnects

Internet routing (NAT traversal, DHT, relay) lands in V2 — see `docs/ROADMAP.md`.

## Building from source

### Prerequisites

- Rust stable (≥ 1.80) — `rustup install stable`
- Node.js 22 and pnpm ≥ 10 — `npm install -g pnpm`
- **Linux only:** `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`, `librsvg2-dev`
- macOS / Windows: nothing extra; system WebView is used

### Run in development

```bash
pnpm --filter ./ui install
cd src-tauri && cargo tauri dev
```

The first build pulls libp2p, sqlx, and Tauri — expect ~3 minutes for a cold compile. Incremental builds are seconds.

### Try two peers on one machine

```bash
# Terminal 1 — copy the y7: ID it prints into the clipboard.
cd src-tauri && cargo tauri dev

# Terminal 2 — uses a separate data dir so it gets a different identity.
XDG_DATA_HOME=/tmp/y7ke-bob cargo tauri dev
```

Both windows discover each other over mDNS. Paste one ID into the other's "Add contact" panel, accept the request on the receiving side, and start chatting.

### Tests

```bash
cargo test --workspace            # ~10s for unit + the E2E tests
cargo test -p y7ke-app --test v1_stress -- --ignored   # 3-client stress, ~10s
```

The two named integration tests (`v1_e2e`, `v1_offline_sync`) spin up multiple in-process AppHandles and exercise the seven V1 capabilities over a real libp2p mDNS swarm.

## How it works

```
   ┌─────────────────────────────────────────────────────────────┐
   │                 src-tauri  ←  ui/  (Svelte 5)               │
   │                       │                                     │
   │                  AppHandle                                  │
   │   ┌──────────────┬────┴────┬──────────────┐                 │
   │   │              │         │              │                 │
   │  y7ke-storage  y7ke-net  identity     event_loop            │
   │   │             │                          │                │
   │  SQLite        libp2p swarm:               │                │
   │  + DEK file    TCP+Noise+Yamux             │                │
   │                + mDNS+ping+identify        │                │
   │                + 3 request_response        │                │
   └─────────────────────────────────────────────────────────────┘
```

| Layer | Crate | Responsibility |
|---|---|---|
| Types + crypto | `y7ke-core` | `Y7Id`, `MessageId` (UUIDv7), `ConversationId` (blake3), Ed25519, X25519, ChaCha20-Poly1305, HKDF |
| Storage | `y7ke-storage` | sqlx-sqlite, 8 tables, app-layer column encryption with master DEK |
| Networking | `y7ke-net` | libp2p swarm, three custom request-response protocols (`/y7ke/handshake/1.0.0`, `/y7ke/msg/1.0.0`, `/y7ke/sync/1.0.0`) |
| Composition | `y7ke-app` | Wires storage + net together, runs the event loop, exposes the command API used by Tauri |
| Desktop shell | `src-tauri` | Tauri 2 shell, command surface, event emission |
| Frontend | `ui/` | Svelte 5 + Vite + TypeScript |

### Privacy model

- The Ed25519 signing key is **stored in SQLite, encrypted with a master DEK** (32 random bytes) that lives at `<app_data>/y7ke/master.dek` (file mode `0600` on Unix). V2 promotes the DEK to the OS keyring with the file as fallback.
- **Message ciphertext only.** Disk and wire formats are identical — the same encrypted bytes go to `messages.payload_enc` and over `/y7ke/msg/1.0.0`.
- Session keys are derived per-conversation via HKDF over the X25519 shared secret, never reused across conversations.
- Each envelope is signed with the sender's long-term Ed25519 key so receivers can detect tampering and impersonation.

## Documentation

- `docs/ARCHITECTURE.md` — current implementation architecture
- `docs/DECISIONS.md` — ADR log of major design choices
- `docs/ROADMAP.md` — V1 / V2 / V3 milestones
- `docs/TODO.md` — live task list
- `technical_task.md` — original product specification

## License

MIT OR Apache-2.0 at your option.
