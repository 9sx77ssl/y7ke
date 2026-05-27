# Y7KE TODO

## V1 — shipping (LAN end-to-end + post-audit fixes applied)

All seven user-visible capabilities pass automated tests and the manual run
against `cargo tauri dev` (see `docs/screenshots/`):

- [x] C1 — Generate identity
- [x] C2 — Add contact by key
- [x] C3 — Accept / reject request
- [x] C4 — Open chat
- [x] C5 — Encrypted live messaging
- [x] C6 — SQLite persistence (restart preserves history — verified by `v1_restart_both.rs`)
- [x] C7 — Offline sync after reconnect (`v1_offline_sync.rs`)

## Audit findings status (see `docs/AUDIT.md`)

**Fixed in commit `fix(audit): close C1+H1+H2+H3+H4+M1+M2+M4 + restart-both test`:**
- [x] C1 — `peer_id_from_y7` no longer panics; `Y7Id::parse_strict` added at IPC boundary
- [x] H1 — `send_contact_request` idempotent; responder rejects re-handshakes (uses `accept = false`)
- [x] H2 — sync responder verifies requester is a conversation participant
- [x] H3 — duplicate pending requests deduplicated
- [x] H4 — silent dial errors now logged
- [x] M1 — `Sent → Synced` status fires on `MsgResp.ack`
- [x] M2 — 64 KiB cap on send + receive; oversized envelopes rejected with `ack=false`
- [x] M3 — `HandshakeResp.accept` is now used (previously dead protocol field)
- [x] M4 — both handlers verify libp2p PeerId matches claimed Ed25519 pubkey
- [x] L1 — peer events without recoverable Y7Id now log

**Still open (V1 polish):**

- [ ] L2 — `MessageId::from_bytes` called twice in `handle_msg` (cosmetic; ~1 LoC fix)
- [ ] A1 — event loop's `Arc<AppInner>` retains net handle on AppHandle drop (clean shutdown requires `app.shutdown().await` before drop)
- [ ] S1 — Tauri CSP set to `null` (no XSS guard); set strict CSP allowing `'self'` only
- [ ] Two-instance live screenshot (handshake + chat with messages visible)
- [ ] Real Y7KE icon (replaces the 1×1 placeholder PNG)
- [ ] `cargo tauri build` — produce `.deb`/`.AppImage` artifacts (enable bundling in tauri.conf.json + add icon set)
- [ ] Cold-start measurement script + memory profile

## V2 — hardening (do not start until V1 polish is complete and reviewed)

Carried from AUDIT.md:

- [ ] **CR1** — session-key ratcheting (Double Ratchet or simpler counter-based chain) for forward secrecy
- [ ] **CR2** — promote master DEK from local file to OS keyring (`keyring` crate) with the file as fallback
- [ ] **CR3** — anti-replay nonce in `HandshakeReq` (16-byte random + LRU server-side)
- [ ] **A2** — implement initiator-side `/y7ke/sync/1.0.0` 3-round reconcile or remove the dead responder code
- [ ] **P1** — in-memory cache: `HashMap<Y7Id, SymmetricKey>` for session keys, LRU for decrypted message text
- [ ] **P2** — non-blocking boot: spawn `AppHandle::boot` in `setup`, register state when ready, show splashscreen
- [ ] **S2** — per-peer leaky-bucket rate limiter on inbound `HandshakeReq`/`MsgReq`/`SyncReq`
- [ ] Kademlia DHT with self-hosted Y7KE bootstrap nodes (replaces mDNS-only)
- [ ] AutoNAT to detect public reachability
- [ ] Circuit relay v2 + DCUtR for NAT traversal
- [ ] QUIC transport (UDP-based, single-RTT handshake)
- [ ] Tauri-driver E2E tests across the full UI flow
- [ ] ts-rs codegen from Rust types into `ui/src/lib/types.ts` (eliminates the hand-maintained mirror)
- [ ] Read receipts (`Delivered` status)

## V3 — groups, files, anonymity

- [ ] Group conversations (multi-party sessions)
- [ ] File transfer (Bitswap-style chunked + resumable)
- [ ] Optional onion / anonymous routing
- [ ] Mobile (Tauri Mobile)
