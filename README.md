# Y7KE

Privacy-first, key-based, peer-to-peer desktop messenger.

- **No accounts, no email, no phone numbers.** Your identity is a public key.
- **No central server.** Peers discover each other directly and exchange messages over libp2p.
- **End-to-end encrypted.** ChaCha20-Poly1305 per-conversation keys derived on demand from X25519(my_static_identity_scalar, peer_pubkey); Ed25519 signatures over every envelope. **No session key is ever stored** — stealing the SQLite file without the master DEK yields ciphertext only.
- **Secure delete.** `PRAGMA secure_delete = ON` zero-fills freed pages so wiped messages and sessions don't linger on disk. Delete-contact propagates bilaterally — both copies vanish when the peer is next online.
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
cargo tauri build                                                   # full bundle (deb / AppImage / dmg / msi)
cargo tauri build --no-bundle                                       # just the binary at target/release/y7ke
cargo build --release -p y7ke-tauri --features custom-protocol      # binary with embedded frontend, no tauri-cli required
```

The `custom-protocol` feature is what makes the binary serve the bundled
frontend over Tauri's asset protocol; without it, release binaries fall
back to the dev server URL (`http://localhost:1420`) and refuse to
connect.

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
- **Per-conversation keys are never stored.** They're derived on every encrypt/decrypt via `HKDF(X25519(my_static_x25519, peer_static_x25519), conv_id, "y7ke-conv-v1")`, where both X25519 keys come from the long-term Ed25519 identity (SHA-512 + clamp on the seed; Edwards-to-Montgomery on the pubkey). The DH is symmetric, so both peers compute the same key.
- The `sessions` table only records "handshake completed" — no key material. Stealing the SQLite file alone gives you 32-byte ciphertexts and nothing to decrypt them with.
- `PRAGMA secure_delete = ON` overwrites freed pages with zeros, so a wiped message or session can't be carved out of the database file.
- Every envelope is signed with the sender's long-term Ed25519 key so receivers detect tampering and impersonation.

## Internet mode (V2-A1+A2)

V1 was LAN-only via mDNS. From v0.1.20, the client also speaks Kademlia
DHT and reaches peers across the open internet through a stable
bootstrap node. Discovery chain on `dial_with_discovery`:

1. swarm address book (mDNS + identify cache)
2. `peer_state.last_addrs` (persisted across restarts)
3. `find_peer` via Kad against the configured bootstraps
4. Direct TCP dial

Bootstraps are sourced in this order, first non-empty wins:

1. `Y7KE_BOOTSTRAP=…` env var (comma-separated multiaddrs)
2. `~/.config/y7ke/bootstrap.toml`:
   ```toml
   peers = [
     "/dns4/bootstrap1.y7v.lol/tcp/4101/p2p/12D3KooW…",
   ]
   ```
3. `y7ke_net::DEFAULT_BOOTSTRAPS` — hardcoded at build time.

Want to run your own bootstrap? The standalone daemon lives in a
separate repo: <https://github.com/9sx77ssl/y7ke-bootstrap>.
One-line installer:

```bash
bash <(curl -sSL https://github.com/9sx77ssl/y7ke-bootstrap/raw/main/install.sh)
```

It downloads the latest release binary, installs a systemd unit at
`/etc/systemd/system/y7ke-bootstrap.service`, opens TCP 4101 if a
firewall is already running (skips if none), and prints the generated
PeerId for you to publish.

What V2-A still doesn't ship: NAT hole-punching (DCUtR), circuit relay,
QUIC. Peers behind symmetric NAT fail their direct dial gracefully and
stay discoverable in the DHT.

## Documentation

- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — implementation architecture
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — milestones and what's next
- [`CHANGELOG.md`](CHANGELOG.md) — per-version diff

## License

MIT OR Apache-2.0 at your option.
