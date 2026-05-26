# Y7KE Roadmap

## V1 — LAN end-to-end (target: ~2–3 weeks)

The minimum that lets two people on the same network talk privately and never lose a message.

1. **M0 — Scaffold** ✱
   - Cargo workspace (4 crates) + Tauri 2 shell + Svelte/Vite UI
   - CI green on Linux
   - `cargo tauri dev` opens a blank Y7KE window
2. **M1 — V1 capabilities end-to-end**
   - Generate identity (capability 1)
   - Add contact by key + send request (capability 2)
   - Accept request (capability 3)
   - Open chat (capability 4)
   - Encrypted live messaging (capability 5)
   - SQLite persistence (capability 6)
   - Offline sync after reconnect (capability 7)
3. **M2 — V1 release polish**
   - Multi-client integration tests
   - Stress tests (6 clients, packet loss simulation)
   - Memory + cold-start measurement
   - `.deb` / `.AppImage` packaging via `cargo tauri build`

## V2 — Internet routing & hardening (~3–4 weeks after V1)

- Kademlia DHT with Y7KE-operated bootstrap nodes
- AutoNAT to detect public reachability
- Circuit relay v2 + DCUtR for NAT traversal
- QUIC transport
- OS keychain integration (`keyring` crate) with file fallback
- Argon2id passphrase vault as headless-host alternative
- Read receipts (`Delivered` state)
- TS-rs codegen for command/event types
- Tauri-driver E2E tests

## V3 — Groups, files, anonymous routing

- Group conversations (multi-party messaging on top of pairwise sessions, then native group ratchet)
- File transfer (Bitswap-style chunked + resumable)
- Optional onion / anonymous routing
- Mobile (Tauri Mobile or sister app)
