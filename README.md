# Y7KE

Privacy-first, key-based, peer-to-peer desktop messenger.

- **No accounts, no email, no phone numbers.** Your identity is a public key.
- **No central server.** Peers discover each other directly and exchange messages over libp2p.
- **End-to-end encrypted.** ChaCha20-Poly1305 per-conversation keys derived on demand from X25519(my_static_identity_scalar, peer_pubkey); Ed25519 signatures over every envelope. **No session key is ever stored** — stealing the SQLite file without the master DEK yields ciphertext only. Traffic stays end-to-end even when forwarded through a bootstrap relay (Circuit Relay v2).
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

## Internet mode (V2-A1 + A2 + A4)

V1 was LAN-only via mDNS. From v0.1.20 the client also speaks
Kademlia DHT for peer lookup; from v0.1.43 it carries a libp2p
**Circuit Relay v2** client so two peers behind NAT/CGNAT can talk
through a public bootstrap. Discovery chain on `dial_with_discovery`,
each step gated by the user's current dial modes:

1. swarm address book (mDNS + identify cache)
2. `peer_state.last_addrs` (persisted across restarts, filtered by
   active modes)
3. `find_peer` via Kad against the configured bootstraps; results
   include relay multiaddrs of the form
   `/dns4/<bootstrap>/.../p2p-circuit/p2p/<peer>` once the peer has
   reserved a slot
4. Direct dial of every returned multiaddr, in order (shorthand
   entries expand to both TCP and QUIC, raced with QUIC preferred)

Each client proactively reserves a `/p2p-circuit` slot at every
configured bootstrap on connect. The bootstrap forwards encrypted
frames only — it never sees plaintext (Noise + ChaCha20-Poly1305
wrap every byte before it leaves the client). A 15-second reconnect
tick redials any bootstrap that drops, so a single VPS restart
recovers in ~10 s instead of waiting for Kad's 5-minute periodic
bootstrap.

Bootstraps are sourced in this order, first non-empty wins:

1. `Y7KE_BOOTSTRAP=…` env var (comma-separated multiaddrs).
2. **User settings** — `Settings::extra_bootstraps` from the
   encrypted SQLite. Set via the in-app **settings :3** page.
3. `bootstrap.toml` in the per-OS config directory:

   | OS | Path |
   |---|---|
   | Linux | `$XDG_CONFIG_HOME/y7ke/bootstrap.toml` (defaults to `~/.config/y7ke/bootstrap.toml`) |
   | macOS | `~/Library/Application Support/com.y7ke.Y7KE/bootstrap.toml` |
   | Windows | `%APPDATA%\y7ke\Y7KE\config\bootstrap.toml` (typically `C:\Users\<you>\AppData\Roaming\y7ke\Y7KE\config\bootstrap.toml`) |

   File format:
   ```toml
   peers = [
     "/dns4/bootstrap1.y7v.lol/4101/p2p/12D3KooW…",
   ]
   ```
   The transport-agnostic shorthand (no `/tcp` or `/udp` segment) is
   auto-expanded by the client into both a TCP and a QUIC multiaddr;
   it dials both and prefers QUIC. Explicit `/tcp` or `/udp`
   multiaddrs pass through unchanged.
4. `y7ke_net::DEFAULT_BOOTSTRAPS` — hardcoded fallback at build
   time, currently `bootstrap1.y7v.lol` (Germany), PeerId
   `12D3KooWEVq9A1w4xk1paGxywwPNy4vz8D92wxE4XKBh8DpA8fSo`. Raw-IP
   fallback `/ip4/89.35.130.67/4101/…` if DNS isn't resolving.

Whatever source contributes them, the hardcoded
`DEFAULT_RELAY_BOOTSTRAP` is **always prepended** (deduped) so a
typo in user config can never strand the client.

### Running your own bootstrap

The standalone daemon lives at <https://github.com/9sx77ssl/y7ke-bootstrap>.
One-line installer:

```bash
bash <(curl -sSL https://github.com/9sx77ssl/y7ke-bootstrap/raw/main/install.sh)
```

The daemon runs Kad + identify + ping + relay-server. v0.1.4+ requires
its public-facing multiaddrs declared via `--external-addr` or the
`Y7KE_BOOTSTRAP_EXTERNAL_ADDR` env (comma-separated). Without it,
libp2p sends reservation acks with an empty address list and clients
reject with `NoAddressesInReservation`.

Example systemd drop-in:
```ini
# /etc/systemd/system/y7ke-bootstrap.service.d/external.conf
[Service]
Environment=Y7KE_BOOTSTRAP_EXTERNAL_ADDR=/dns4/your-host.example/tcp/4101
```

The relay carries ciphertext frames only — operator gets no
metadata beyond `relay: reservation accepted` and `circuit accepted`
log lines.

## Settings — connection modes + bootstrap roster

From v0.1.43, click **settings :3** in the sidebar to open the
settings pane.

- **Connection modes** — two modes: `lan only` and `Y7net`
  (the `Internet` mode). `lan only` uses mDNS for same-WiFi peers;
  `Y7net` enables Kad-resolved direct dial plus forwarding through
  bootstrap relays (covers NAT/CGNAT).
- **Bootstrap nodes** — the locked first row is the hardcoded
  default. Below it, user-added multiaddrs (one per row,
  `+ add bootstrap` to add a blank row). `ping all` opens a TCP
  connection to each (5 s budget) and shows the latency in a
  pill — green ≤150 ms, amber otherwise, red on failure. `save`
  persists to the encrypted SQLite and re-syncs the live swarm
  without a restart.

Changes propagate immediately: `update_settings` fires an
`AppEvent::SettingsChanged` and `NetCommand::UpdateBootstraps`,
the swarm task adds any new entries to its `bootstrap_peers` map
and dials them. Existing connections to removed bootstraps stay
open until they drop naturally.

## Still pending in V2-A

NAT hole-punching (A5 DCUtR — upgrading the relayed connection to a
direct TCP/UDP one once both peers reveal their observed addresses),
AutoNAT v2 (A3 — telling the user "you're publicly reachable" vs
"you need the relay"), and QUIC transport (A6 — UDP-only networks).
With A4 shipped, two peers across arbitrary NATs already talk; A5
makes the connection direct when the NATs allow it.

## Documentation

- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — implementation architecture
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — milestones and what's next
- [`CHANGELOG.md`](CHANGELOG.md) — per-version diff

## License

MIT OR Apache-2.0 at your option.
