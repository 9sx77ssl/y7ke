# Changelog

All notable changes to Y7KE are recorded here. The pre-commit hook bumps
the patch version on every commit and prepends an entry with the commit
subject; release tags pick up the matching section as the release body.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versioning is [SemVer](https://semver.org/).

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
