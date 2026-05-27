# Y7KE

Privacy-first, key-based, peer-to-peer desktop messenger.

- **No accounts, no email, no phone numbers.** Your identity is a public key.
- **No central server.** Peers discover each other directly and exchange messages over libp2p.
- **End-to-end encrypted.** ChaCha20-Poly1305 sessions derived from an X25519 handshake; Ed25519 signatures over every envelope.
- **Local-first.** Messages persist in an encrypted SQLite database on disk.
- **Offline-tolerant.** Undelivered messages queue locally and drain on reconnect; an explicit `/y7ke/sync/1.0.0` reconcile catches anything the queue lost.

See [`docs/ROADMAP.md`](docs/ROADMAP.md) for the live plan and
[`CHANGELOG.md`](CHANGELOG.md) for what shipped per version.

## Building from source

### Prerequisites

- Rust stable (≥ 1.80) — `rustup install stable`
- Node.js 22, pnpm ≥ 10 — `npm install -g pnpm`
- Tauri CLI — `cargo install tauri-cli --version "^2" --locked` (only needed if you want `cargo tauri dev`; `cargo build` works without it)
- **Linux:** `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`, `librsvg2-dev`
- macOS / Windows: nothing extra; the system WebView is used

```bash
pnpm --dir ui install
git config core.hooksPath scripts/hooks   # auto-bump version + CHANGELOG on commit
```

### Run in dev

```bash
cargo tauri dev
```

`beforeDevCommand` spawns Vite, the Tauri shell attaches to it, and the
backend runs in debug. Right-click → **Inspect Element** opens WebKit
DevTools (network + console + Svelte state).

### Build a release binary

```bash
cargo tauri build              # full bundle (deb / AppImage / dmg / msi)
cargo tauri build --no-bundle  # just the binary at target/release/y7ke
```

### Two peers on the same box

```bash
Y7KE_DATA_DIR=/tmp/y7ke-alice ./target/release/y7ke &
Y7KE_DATA_DIR=/tmp/y7ke-bob   ./target/release/y7ke &
```

mDNS discovers both within ~3 s. Paste one Y7 URI into the other's
"Add contact", accept on the receiver, exchange messages.

### Tests

```bash
cargo test --workspace                                # unit + integration
cargo test -p y7ke-app --test v1_stress -- --ignored  # 3-client stress
```

mDNS-dependent tests are auto-skipped on macOS / Windows runners
(GitHub Actions mDNS is unreliable); they run unconditionally on Linux
and locally.

## Architecture

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
| Networking | `y7ke-net` | libp2p swarm, three custom request-response protocols: `/y7ke/handshake/1.0.0`, `/y7ke/msg/1.0.0`, `/y7ke/sync/1.0.0` |
| Composition | `y7ke-app` | Wires storage + net, runs the event loop, exposes the command API |
| Desktop shell | `src-tauri` | Tauri 2 shell, command surface, event emission |
| Frontend | `ui/` | Svelte 5 + Vite + TypeScript |

### Privacy model

- The Ed25519 signing key is stored in SQLite encrypted with a 32-byte master DEK at `<app_data>/y7ke/master.dek` (file mode `0600`).
- Disk and wire formats hold the same ciphertext — `messages.payload_enc` is byte-identical to what goes over `/y7ke/msg/1.0.0`.
- Session keys are derived per-conversation via HKDF over the X25519 shared secret; never reused across conversations.
- Every envelope is signed with the sender's long-term Ed25519 key so receivers detect tampering and impersonation.

## Documentation

- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — implementation architecture
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — milestones and what's next
- [`CHANGELOG.md`](CHANGELOG.md) — per-version diff

## License

MIT OR Apache-2.0 at your option.
