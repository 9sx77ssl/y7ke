# Y7KE TODO

## V1 — LAN end-to-end ✅ shipping

All seven user-visible capabilities pass automated tests:

- [x] **C1 — Generate identity.** Ed25519 keypair, persisted encrypted under master DEK; `Y7Id` URI `y7:<base58>`.
- [x] **C2 — Add contact by key.** `send_contact_request` parses URI, dials peer over mDNS, runs `/y7ke/handshake/1.0.0`.
- [x] **C3 — Accept / reject request.** Local state transitions + `RequestResolved` / `ContactAdded` events.
- [x] **C4 — Open chat.** `list_messages(peer)` derives `ConversationId` and lists ordered messages.
- [x] **C5 — Encrypted live messaging.** ChaCha20-Poly1305(session_key) over `/y7ke/msg/1.0.0`; Ed25519 sig verified.
- [x] **C6 — SQLite persistence.** Per-message ciphertext on disk; sessions encrypted with DEK; restart preserves history.
- [x] **C7 — Offline sync.** Failed live-sends enqueue in `sync_queue`; mDNS rediscovery drains it.

Verified by `tests/v1_e2e.rs` (~3s) and `tests/v1_offline_sync.rs` (~6s) — 2 in-process clients on the same host.

## V1 release polish (next)

- [ ] Multi-client stress test (4–6 clients sustained message exchange)
- [ ] Cold-start measurement script + tuning
- [ ] Memory profiling (target: < 80MB RSS idle)
- [ ] `cargo tauri build` → `.deb` / `.AppImage` artifacts
- [ ] Real Y7KE icon (replaces 1×1 placeholder)
- [ ] README install + usage instructions
- [ ] Confirm `cargo tauri dev` opens window on Linux + driver-based UI smoke test

## V2 — Internet + hardening

- [ ] Kademlia DHT with self-hosted bootstrap relays
- [ ] AutoNAT (detect public reachability)
- [ ] Circuit relay v2 + DCUtR (NAT traversal)
- [ ] QUIC transport
- [ ] OS keychain integration (`keyring` crate, DEK promotion)
- [ ] Argon2id passphrase vault (headless-host fallback)
- [ ] Tauri-driver E2E tests across the UI flow
- [ ] ts-rs codegen for command + event types (replace hand-written `types.ts`)
- [ ] Read receipts (`Delivered` status)

## V3 — Groups, files, anonymous routing

- [ ] Group conversations (multi-party sessions)
- [ ] File transfer (Bitswap-style chunked + resumable)
- [ ] Optional onion / anonymous routing
- [ ] Mobile (Tauri Mobile)
