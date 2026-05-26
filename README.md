# Y7KE

Privacy-first, key-based, peer-to-peer, local-first desktop messenger.

- No accounts, emails, passwords, or phone numbers — identity is a public key.
- Messages stored locally and end-to-end encrypted.
- No central message server: peers discover each other via libp2p (mDNS, Kademlia DHT, circuit relay with hole-punching) and exchange messages directly.
- Cross-platform (Linux / macOS / Windows) via Tauri 2.

## Status

Pre-V1 active development. See `docs/ROADMAP.md` for milestones and `docs/TODO.md` for the current task list.

## Building

```bash
# Prerequisites: rustc stable, pnpm, and (Linux only) webkit2gtk-4.1 + gtk3 + libsoup3.
cargo check --workspace
pnpm --filter ui install
cd src-tauri && cargo tauri dev
```

## Repository layout

```
crates/        Rust libraries (workspace members)
  y7ke-core/      types + errors + IDs + events + crypto primitives (Ed25519, X25519, ChaCha20-Poly1305, HKDF)
  y7ke-storage/   sqlx + SQLite + master-DEK file + app-layer column encryption
  y7ke-net/       libp2p swarm + session handshake + offline sync state machine
  y7ke-app/       composition root + Tauri command surface + headless test harness
src-tauri/     Tauri 2 desktop shell (depends on y7ke-app)
ui/            Svelte + TypeScript + Vite frontend
docs/          architecture, decisions, roadmap, todo
```

V1 stays LAN-only via mDNS (no DHT bootstrap, no NAT traversal, no QUIC). Internet routing and OS-keychain integration land in V2.

See `technical_task.md` for the original specification and `docs/ARCHITECTURE.md` for the implementation architecture.

## License

MIT OR Apache-2.0 at your option.
