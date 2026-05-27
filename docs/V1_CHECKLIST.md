# Y7KE V1 acceptance checklist

Tick once verified, with the commit / log / test name that proves it.

## Functionality (the seven capabilities)

- [x] **C1 — Generate identity.** First-launch flow creates Ed25519
      keypair, encrypts private key with DEK, persists. Subsequent
      launches load the same identity. Verified by
      `identity::tests::generates_then_loads` and `v1_restart_both`.
- [x] **C2 — Add contact by key.** Paste a `y7:` URI → outgoing request
      + handshake. Verified by `v1_e2e`.
- [x] **C3 — Accept / reject / cancel request.** Local state transitions
      + events. Verified by `v1_e2e` (accept), `v1_restart_both`
      (regression for H1 + H3), and `cancel_request` API exposed.
      Accept propagation: `ControlPayload::AcceptedRequest` sent over
      `/y7ke/msg/1.0.0` so the initiator promotes without waiting for
      the first inbound text.
- [x] **C4 — Open chat.** `list_messages(peer, limit)` returns ordered
      history. Verified by `v1_e2e`.
- [x] **C5 — Encrypted messaging.** ChaCha20-Poly1305(session_key) over
      `/y7ke/msg/1.0.0`, Ed25519 signature verified. Verified by
      `messaging::tests` + `v1_e2e`.
- [x] **C6 — SQLite persistence.** Restart preserves identity, sessions,
      messages. Verified by `storage_e2e` + `v1_restart_both`.
- [x] **C7 — Offline sync.** Queue drains on reconnect; no duplicates.
      Verified by `v1_offline_sync`.
- [x] **C8 — Delete propagation & auto-eject** (added post-audit).
      `delete_contact` sends `ControlPayload::ChatDeleted`; the peer
      wipes its conversation and emits `AppEvent::ContactRemoved`; the
      UI exits the chat pane if the deleted peer was open. Verified by
      `v1_delete_propagation`.

## Code quality

- [x] `cargo fmt --all -- --check` clean
- [x] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [x] `pnpm -C ui tsc --noEmit` clean
- [x] `pnpm -C ui build` produces dist/ under 100 KB JS + 60 KB CSS
- [x] All audit findings (C1, H1–H4, M1–M4, L1) closed (see
      `docs/AUDIT.md`)

## Privacy / security

- [x] Private Ed25519 key column-encrypted on disk
      (`storage_e2e::users_round_trip_with_encrypted_private_key`
      verifies ciphertext ≠ plaintext)
- [x] Session shared-secret column-encrypted on disk
      (`storage_e2e::sessions_round_trip_encrypted`)
- [x] Message ciphertext only on disk and on wire (same bytes;
      verified by manual `grep` of plaintext against `y7ke.db`)
- [x] Handshake binds peer's libp2p PeerId to claimed Ed25519 pubkey
      (M4)
- [x] Sync `Pull` scoped to the (self, requester) conversation only (H2)
- [x] Inbound message size capped at `MAX_MESSAGE_BYTES = 64 KiB` (M2)
- [x] Strict CSP in `tauri.conf.json` (no remote scripts, `'self'` only)
- [x] DEK file written with mode `0600` on Unix
      (`dek::tests::generates_on_first_call_and_reloads_on_second`)

## UX / design

- [x] Dark monochrome aesthetic matching reference screenshots
- [x] All buttons use the `<Button>` component (1 format per place)
- [x] All inputs use `<Input>`/`<Textarea>` (no raw `<input>`)
- [x] Custom frameless window — same min/max/close on Linux/macOS/Windows
- [x] JetBrains Mono bundled (3 weights, ~85 KB woff2)
- [x] Click-to-copy with toast on identity display
- [x] Sidebar shows brand only — full `y7:` ID lives at the bottom of
      Add Contact
- [x] Status dots: green (online), gray (offline), yellow (connecting)
- [x] Cancel button on outgoing pending requests

## Build / distribution

- [x] `bundle.active: true` in `tauri.conf.json`
- [x] Real Y7KE icon (`src-tauri/icons/{32x32,128x128,128x128@2x,icon.icns,icon.ico}.png`)
- [x] LICENSE (MIT) committed
- [ ] `cargo tauri build` produces `.deb` + `.AppImage` (Linux)
- [ ] Release workflow successfully builds all three OSes on tag push
- [x] CI workflow passes `fmt + clippy + tests + pnpm tsc + pnpm build`

## Repository hygiene

- [x] Private GitHub repo `9sx77ssl/y7ke` exists and is pushed
- [x] Release workflow at `.github/workflows/release.yml` triggered on
      `v*` tags
- [x] README has install + build instructions
- [x] `docs/AUDIT.md` documents all post-V1 audit findings
- [x] `docs/RELEASE_NOTES_V1.md` lists features + limitations
- [x] `docs/DEMO.md` walks through two-peer scenario
- [x] `docs/TODO.md` lists real V1 polish remaining + V2 backlog

## Stress + privacy verification

- [x] 3-client pairwise stress test (`v1_stress`, ~10 s, --ignored)
      passes — 30 messages, no duplicates, no losses
- [x] Plaintext grep against on-disk SQLite DB returns no matches
      (documented in DEMO.md)

## Manual smoke test (do once per release)

- [ ] `cargo tauri build` on this host succeeds
- [ ] Run the built binary (not dev) — window opens, identity appears,
      Add Contact / Requests / Chat all functional
- [ ] Two built binaries on the same LAN exchange a message

The unchecked `[ ]` boxes are gates for the actual V1 tag. Once `cargo
tauri build` artifacts pass the manual smoke test and the release CI
green-lights all three OS builds, V1 ships.
