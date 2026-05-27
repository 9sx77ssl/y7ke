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

## V1 stabilization (added after the initial audit)

Two-client UX bugs surfaced during real-device dogfooding; all fixed and
test-covered:

- [x] **U1** — non-blocking `send_message`: returns immediately with the
      persisted MessageId, `push_one` runs in the background with a 5 s
      timeout, fallback to `sync_queue` on libp2p hang. No more
      "sending…" freezes.
- [x] **U2** — presence cache (`AppInner.presence: RwLock<HashMap>`)
      updated by `ConnectionEstablished/Closed` and read by
      `list_contacts`, so the sidebar dot matches reality even when the
      event fires before the contact row is inserted.
- [x] **U3** — accept propagation: `ControlPayload::AcceptedRequest`
      sent over `/y7ke/msg/1.0.0` so the initiator's row promotes from
      `pending_out → accepted` without waiting for the first inbound
      text. Auto-promote is restricted to `pending_out` only (preserves
      the accept-gate on `pending_in`).
- [x] **U4** — delete propagation + auto-eject: `delete_contact` sends
      `ControlPayload::ChatDeleted`, the peer's event loop wipes its own
      conversation and emits `AppEvent::ContactRemoved`. The UI
      dispatcher reroutes to `openEmpty()` if the deleted peer was the
      open chat. Verified by `v1_delete_propagation`.
- [x] **U5** — fresh-state navigation: every router transition
      (`openChat/Empty/AddContact/Requests`) resets the chat store so
      re-entering a previously deleted chat works.
- [x] **U6** — toaster bottom-left, sidebar-width, lifted 40 px above
      the contact-count footer.
- [x] **U7** — native context menu suppressed; right-click on a
      contact opens our `ContextMenu` → `Modal` confirm flow.

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

See `docs/ROADMAP.md` for the sequenced V2 plan with milestones. Backlog
items (track-ordered):

**Track A — Internet reachability**

- [ ] Kademlia DHT with self-hosted Y7KE bootstrap nodes (replaces mDNS-only)
- [ ] AutoNAT v2 to detect public reachability
- [ ] Circuit relay v2 + DCUtR for NAT traversal
- [ ] QUIC transport (UDP-based, single-RTT handshake)
- [ ] Bootstrap-node binary + deployment manifest (systemd / Dockerfile)

**Track B — Cryptographic uplift**

- [ ] **CR1** — session-key ratcheting (Double Ratchet) for forward secrecy
- [ ] **CR2** — promote master DEK from local file to OS keyring (`keyring` crate) with the file as fallback
- [ ] **CR3** — anti-replay nonce in `HandshakeReq` (16-byte random + LRU server-side)

**Track C — Sync correctness & observability**

- [ ] **A2** — implement initiator-side `/y7ke/sync/1.0.0` 3-round reconcile or remove the dead responder code
- [ ] **P1** — in-memory cache: `HashMap<Y7Id, SymmetricKey>` for session keys, LRU for decrypted message text
- [ ] **P2** — non-blocking boot: spawn `AppHandle::boot` in `setup`, register state when ready, show splashscreen
- [ ] **S2** — per-peer leaky-bucket rate limiter on inbound `HandshakeReq`/`MsgReq`/`SyncReq`

**Track D — UX & tooling**

- [ ] Read receipts (`Delivered` status — bidirectional ack)
- [ ] ts-rs codegen from Rust types into `ui/src/lib/types.ts` (eliminates the hand-maintained mirror)
- [ ] Tauri-driver / Playwright E2E tests across the full UI flow
- [ ] Notification toast on new message when chat is not focused

## V3 — groups, files, anonymity

- [ ] Group conversations (multi-party sessions)
- [ ] File transfer (Bitswap-style chunked + resumable)
- [ ] Optional onion / anonymous routing
- [ ] Mobile (Tauri Mobile)
