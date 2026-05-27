# Changelog

All notable changes to Y7KE are recorded here. The pre-commit hook bumps
the patch version on every commit and prepends an entry with the commit
subject; release tags pick up the matching section as the release body.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versioning is [SemVer](https://semver.org/).

## [0.1.20] — 2026-05-27

- feat(net): V2-A1+A2 internet-mode discovery via Kademlia + bootstrap

## [0.1.19] — 2026-05-27

- docs: V1 done, ROADMAP cleaned, CHANGELOG narrates the class:failed bug

## [0.1.18] — 2026-05-27

### Fixed
- **Root cause for "invisible messages" bug** (versions 0.1.5–0.1.17):
  `MessageBubble.svelte:31` had `class:failed` (Svelte shorthand) without
  an `={expr}` — Svelte interprets this as "apply `failed` class if a
  variable named `failed` is truthy". The variable had been deleted in
  the SVG-icon refactor but the binding survived. Every "mine" message
  render threw `ReferenceError: Can't find variable: failed`, which
  Svelte propagated out of the render effect and tore down the entire
  `{#each chat.messages as msg}` block — including any unrelated
  panels on the same screen. The chat store kept accumulating messages
  correctly (debug logs proved `stateLen=7..10`); the {#each} simply
  couldn't paint a single bubble.

  The bug was invisible because the previous diagnostic effort
  exclusively read **backend** logs (Rust + UI debug routed through
  `log_from_ui`). The actual `ReferenceError` lived in the WebKit
  JavaScript console and never reached those logs. The fix is one line:
  `class:failed={status === 4}`.

  Side effects fixed by the same one-liner: greeting in the Requests
  pane was also blanked out whenever a chat had been opened in the same
  session (any render that touched MessageBubble would die mid-tree).


## [0.1.17] — 2026-05-27

- fix(ui): expose chat store as direct $state — getter pattern lost reactivity

## [0.1.16] — 2026-05-27

- ui: sidebar emoticons, send-button height match, drop dead requests refresh

## [0.1.15] — 2026-05-27

- fix(ui): no-op self-reopen + chat-store diagnostic logging

## [0.1.14] — 2026-05-27

- fix(ci): correct artifact paths; docs: ROADMAP/README crypto-model refresh

## [0.1.13] — 2026-05-27

- test: integration test for send-while-pending-out flow

## [0.1.12] — 2026-05-27

- fix(ui): apply buffered status update when swapping placeholder→realId

## [0.1.11] — 2026-05-27

- fix(ui): cap visible toasts at 2 with FIFO eviction

## [0.1.10] — 2026-05-27

- fix(ui): chat race + y7 ID selectability

## [0.1.9] — 2026-05-27

- ci: build tauri binary with custom-protocol feature enabled

## [0.1.8] — 2026-05-27

- style: rustfmt pass on static-DH and control-key changes

## [0.1.7] — 2026-05-27

- fix(tauri): add custom-protocol feature for standalone binary builds

## [0.1.6] — 2026-05-27

- ci: fix release workflow — tauri-cli semver, artifact paths, publish

## [0.1.5] — 2026-05-27

- security: static DH key derivation replaces stored session keys

## [0.1.4] — 2026-05-27

- fix: Sending stays Sending on retry-queue; no red bubble background

## [0.1.3] — 2026-05-27

- ci: rerun after repo went public

## [0.1.2] — 2026-05-27

- fix(hooks): insert CHANGELOG entry above the first version, not the title

## [0.1.1] — 2026-05-27

- chore: production-grade repo cleanup (rename binary to `y7ke`, enable
  WebKit devtools, faster CI via prebuilt tauri-cli, CHANGELOG +
  pre-commit hooks, drop V1-era docs)

## [0.1.0] — 2026-05-27

Initial pre-release covering the V1 LAN end-to-end product plus a first
slice of V2 hardening:

### Added
- Tauri 2 + Svelte 5 + Rust workspace (4 crates: core / storage / net /
  app + the tauri shell). End-to-end encrypted text messaging over libp2p
  (TCP + Noise + Yamux + mDNS + identify + ping).
- Identity flow (`y7:<base58 ed25519 pub>`), X25519 + HKDF session
  handshake, ChaCha20-Poly1305 message envelopes, UUIDv7 message IDs.
- Contact lifecycle (add by paste / accept / reject / cancel / delete)
  with control-protocol propagation over the message stream.
- Offline sync via `sync_queue` retry + initiator-side 3-round
  `/y7ke/sync/1.0.0` reconcile (Header → Pull → Ack).
- Per-peer leaky-bucket rate limiter on inbound handshake / msg / sync.
- Non-blocking AppHandle::boot — window appears before the swarm is up.
- ts-rs codegen for `AppEvent` + view types into `ui/src/lib/gen/`.
- Delivered status: live delivery flips `Sending → Delivered` on
  `MsgResp.ack`; Synced reserved for explicit `/y7ke/sync/1.0.0` Ack.
- Auto-eject from chat on contact removal (local + remote).
- CI: fmt + clippy + tests on Linux / macOS / Windows; production tauri
  build verified per push; release workflow bundles `.deb / .AppImage /
  .dmg / .msi / .exe` on `v*` tags.
- `Y7KE_DATA_DIR` env override for running multiple local instances.

### Known limitations
- LAN-only discovery (mDNS); internet routing (Kademlia + relay +
  AutoNAT + DCUtR + QUIC) lands in 0.2.x.
- Master DEK in file mode 0600, not OS keyring.
- Static session key — no forward secrecy yet (Double Ratchet scheduled
  for 0.2.x).
