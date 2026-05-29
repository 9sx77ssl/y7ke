# Changelog

All notable changes to Y7KE are recorded here. The pre-commit hook bumps
the patch version on every commit and prepends an entry with the commit
subject; release tags pick up the matching section as the release body.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versioning is [SemVer](https://semver.org/).

## [3.0.15] — 2026-05-29

- fix(ui): stop boot $effect from flapping the Tauri listener (dropped live messages)

## [3.0.14] — 2026-05-29

- fix(delete): durable ChatDeleted propagation + square frameless corners

## [3.0.13] — 2026-05-29

- fix(msg): reliable delivery — drain on relay→direct upgrade, periodic retry, lost-ack reconcile

## [3.0.12] — 2026-05-29

- fix(dev): force full page reload on change — stop cargo-tauri-dev HMR from splitting singleton stores

## [3.0.11] — 2026-05-29

- fix(app): automatic reconnect/recovery — boot dial-all-contacts, Kad-escalating ticker, port-stable relay fallback

## [3.0.10] — 2026-05-29

- fix(ui): tear down event listener on HMR + requests/sidebar self-heal poll

## [3.0.9] — 2026-05-29

- fix(app+ui): contact-request flow — surface in requests, no chat until accept, no orphan/not-found

## [3.0.8] — 2026-05-29

- fix(ui): robust window reveal (frontend-driven) + position:fixed root — kill the recurring Linux first-paint bug

## [3.0.7] — 2026-05-29

- docs(claude): standard header + window show-after-ready & release-pipeline pins

## [3.0.6] — 2026-05-29

Post-3.0 polish + repo cleanup (rolls up the 3.0.1–3.0.5 patch line).

### Fixed

- **Frameless window on Linux/webkit2gtk** opened at an inconsistent size
  with a cramped first-paint layout. Root cause: the window was shown before
  GTK had realized its allocation + devicePixelRatio, collapsing the
  `height:100%` root chain. Now the window starts `visible:false` and is
  revealed from Rust only after GTK has realized it; the root height is
  viewport-anchored (`100dvh`). Transparency + rounded corners restored
  (they were never the cause).

### Added

- **donate page** — a static, responsive view (crypto addresses only, no
  backend) reached from the sidebar `donate >//<` footer link: a line-art
  kitty, a slightly larger heading, and BTC / ETH / LTC / SOL rows each with
  the full address + an SVG copy-to-clipboard button.
- The sidebar shows a bare **contact count** next to the "contacts" head
  (hidden at 0).

### Removed

- Repo cleanup: deleted the committed `wallets.txt` (personal address dump,
  now in the donate page) and gitignored it; removed the stale V1-era
  `docs/screenshots/`; removed the now-shipped `docs/V2_GLOBAL_NETWORKING_PLAN.md`
  planning doc (recoverable from git history).

### Docs

- README "still pending in V2-A" → "NAT traversal (shipped in 3.0)" with an
  honest field-status note; dangling references to the removed plan cleaned up.

## [3.0.5] — 2026-05-29

- fix(ui): correct SOL donate address to valid base58

## [3.0.4] — 2026-05-29

- feat(ui): static donate page (crypto wallets) opened from the footer link

## [3.0.3] — 2026-05-29

- feat(ui): sidebar donate link + bare contact count next to "contacts"

## [3.0.2] — 2026-05-29

- fix(ui): show window after GTK realizes it — crisp, consistent frameless layout on Linux

## [3.0.1] — 2026-05-29

- fix(ui): solid (non-transparent) frameless window — kills Linux first-paint layout breakage

## [3.0.0] — 2026-05-29

**Y7KE 3.0 — the "global networking" milestone.** The app graduates from a
LAN/loopback-proven prototype into a direct-first encrypted messenger built
to reach peers across the open internet: QUIC + TCP transports, circuit-relay
fallback, and continuous DCUtR hole-punch upgrades — behind a clean
monochrome UI with full, honest connection visibility. The granular per-commit
history for this release is recorded in the `0.1.x` entries below.

### Added — networking (V2 A1–A6)

- **QUIC + TCP dual transport.** Every peer dials both and races them; QUIC
  wins on UDP-open networks (the path that enables direct hole-punch), TCP is
  the fallback.
- **Circuit Relay v2 fallback.** Peers behind NAT/CGNAT reserve a slot on a
  bootstrap and forward end-to-end-encrypted frames through it; the relay
  never sees plaintext.
- **DCUtR hole-punching** upgrades a Relayed connection to Direct, with a
  continuous upgrade-from-relay loop ("relay is temporary") that keeps
  retrying on observed-address / NAT-verdict changes with bounded backoff.
- **AutoNAT v2** reachability detection (Public / Private / Unknown) drives
  the upgrade loop and the UI verdict.
- **Transport-agnostic bootstrap shorthand** `/dns4/host/PORT/p2p/<id>` (no
  `/tcp` or `/udp`) — the client expands it to both transports. The bootstrap
  daemon (v0.1.6) prints the exact descriptor to paste into a client.
- Reconnect-storm protection: idempotent swarm dials, per-peer reconnect
  backoff + jitter, and bounded concurrent Kad lookups.

### Added — connection visibility & diagnostics

- **Connectivity pane**: per-peer kind + transport + relay path, NAT verdict,
  DCUtR success rate, and per-bootstrap reachability.
- Chat header shows **how you're connected** — e.g. "DIRECT · QUIC" /
  "RELAY · TCP".
- **Copy-diagnostics** export (bug icon beside the logo): version, dial mode,
  NAT detail (tested addr + probe server + consecutive failures), DCUtR
  failure reasons, per-protocol rate-limit drops, active connections,
  bootstrap RTT, and the UI log buffer.

### Changed

- Dial modes consolidated to **two**: "lan only" and "Y7net" (the former
  duplicate `P2p` mode was removed; legacy rows migrate to Y7net).
- Settings **live-apply**: dial-mode / bootstrap changes take effect within
  ~1 s, no restart.

### Fixed — reliability & security

- Sync drains correctly over a relayed circuit; per-peer in-memory growth is
  bounded; `ContactStatus::Blocked` is enforced on both the inbound and sync
  paths (fail-closed); presence recomputation never strands a live peer
  Offline; stale relay/circuit addresses are swept from the address book.

### Fixed — UI/UX

- Layout stability (always-static sidebar, capped modal/toast widths,
  frameless-window resize); reactive correctness (settings re-hydrate,
  no presence desync, no toast floods); accessibility (roving radio pills,
  modal focus/escape, viewport-clamped context menu); monochrome palette
  discipline (tokenised pill borders).

### CI/CD

- Release pipeline gated on `fmt + clippy -D warnings + tests + tsc + build`
  before any bundle is published; Linux AppImage (strict) + Windows NSIS
  (best-effort) bundles; release notes drawn from this changelog.

### Known limitations

- **Live cross-NAT QUIC hole-punch is not yet field-confirmed.** Direct
  upgrade is proven on loopback and in a netns NAT simulation, and QUIC
  reservation against the production bootstrap is verified live — but a
  two-machine, two-ISP smoke test is still pending.
- `ContactStatus::Blocked` is reachable (by rejecting a request) but has no
  management UI yet to view or undo blocks.

## [0.1.103] — 2026-05-29

- fix(ui+app+ci): post-3.0 audit hardening — reactivity, diagnostics, release gate

## [0.1.102] — 2026-05-29

- docs: transport-agnostic bootstrap shorthand + QUIC/A5/A6 shipped state

## [0.1.101] — 2026-05-29

- feat(net+app): transport-agnostic bootstrap + multi-addr QUIC racing + per-peer transport surfacing

## [0.1.100] — 2026-05-29

- fix(ui): show "Y7net" (not "internet") for dial mode in Connectivity pane

## [0.1.99] — 2026-05-29

- chore: regenerate DialMode.ts (drop stale serde-alias doc comment)

## [0.1.98] — 2026-05-29

- refactor: drop the duplicate P2p dial mode → two modes (lan only / Y7net)

## [0.1.97] — 2026-05-29

- fix(ui): clear the two a11y build warnings (clean vite build)

## [0.1.96] — 2026-05-29

- fix(ui): responsive padding, narrow-header, locked-input gutter, listener cleanup

## [0.1.95] — 2026-05-29

- fix(ui): a11y + toast overflow — safe modal keys, roving pills, wrapping

## [0.1.94] — 2026-05-28

- feat(ui): copy-diagnostics bug button + compact Connectivity, verbose → export

## [0.1.93] — 2026-05-28

- fix(ui): self-heal presence so the sidebar dot can't strand stale

## [0.1.92] — 2026-05-28

- fix(ui): frameless window resize was completely dead + reclaim top edge

## [0.1.91] — 2026-05-28

- fix: honest p2p / DCUtR status copy (no over- or under-selling)

## [0.1.90] — 2026-05-28

- fix(ui+app): error/status handling — no toast floods, banners persist

## [0.1.89] — 2026-05-28

- fix(ui): reactive correctness — settings re-hydrate + connectivity refresh race

## [0.1.88] — 2026-05-28

- fix(ui): layout stability — resize-handle z-index, static sidebar, modal/toast caps

## [0.1.87] — 2026-05-28

- fix(net): filter circuit addrs at dial time under LanOnly (B#3 complete)

## [0.1.86] — 2026-05-28

- fix(app): extend block enforcement to the sync path + fail closed

## [0.1.85] — 2026-05-28

- fix(net+app): bound unbounded per-peer growth (audit stale-leak)

## [0.1.84] — 2026-05-28

- fix(app+storage): three sync-reconcile reliability bugs

## [0.1.83] — 2026-05-28

- fix(app): stop two paths from stranding live peers Offline

## [0.1.82] — 2026-05-28

- fix(app): enforce ContactStatus::Blocked on inbound (security)

## [0.1.81] — 2026-05-28

- docs: live cross-network smoke runbook (Phase 3.5)

## [0.1.80] — 2026-05-28

- test(net): QUIC IP-change migration experiment (Phase 3.4)

## [0.1.79] — 2026-05-28

- test(net): symmetric-NAT DCUtR fallback sim + Y7KE_HOLD_SECS (Phase 3.4)

## [0.1.78] — 2026-05-28

- fix(net): prune circuit addrs from the address book on LanOnly (audit B#3)

## [0.1.77] — 2026-05-28

- fix(app): don't resurrect a ghost Offline presence row (audit F1)

## [0.1.76] — 2026-05-28

- test(net): netns NAT-simulation harness for relay fallback (Phase 3.2)

## [0.1.75] — 2026-05-28

- test(net): drive /y7ke/sync/1.0.0 over the relay circuit (Phase 3.1)

## [0.1.74] — 2026-05-28

- fix(app): skip stale (>24h) cached circuit addrs in discovery

## [0.1.73] — 2026-05-28

- fix(app): bound the relay→direct upgrade retry to burst-then-periodic

## [0.1.72] — 2026-05-28

- fix(app): purge in-memory per-peer caches on contact delete

## [0.1.71] — 2026-05-28

- fix(app+net): track connection kinds per ConnectionId, not as a flat set

## [0.1.70] — 2026-05-28

- feat(app): per-peer reconnect backoff + jitter + bounded Kad lookups

## [0.1.69] — 2026-05-28

- feat(net): idempotent swarm dials — collapse reconnect-storm fan-out

## [0.1.68] — 2026-05-28

- feat: Connectivity debug pane — per-peer transport + relay path + NAT verdict

## [0.1.67] — 2026-05-28

- feat(app): instant settings live-apply + stale-relay address-book sweep

## [0.1.66] — 2026-05-28

- feat(app): upgrade-from-relay loop in the presence ticker

## [0.1.65] — 2026-05-28

- feat(net+app): DCUtR failure event + upgrade counters + Tauri queries

## [0.1.64] — 2026-05-28

- feat(net): V2-A3 AutoNAT v2 client + NatReachability verdict

## [0.1.63] — 2026-05-28

- fix(net): identify push listen-addr updates

## [0.1.62] — 2026-05-28

- docs: V2 networking plan — SoT for direct-first hardening phase

## [0.1.61] — 2026-05-28

- fix(hooks): use post-commit + amend so CHANGELOG entry lands in own commit

## [0.1.60] — 2026-05-28

- fix(ui): visible active state on connection-mode pill

## [0.1.58] — 2026-05-28

- chore: cargo fmt + derive_default for DialMode

## [0.1.57] — 2026-05-28

- feat(ui): radio-pill DialMode picker, drop DialModes types

## [0.1.56] — 2026-05-28

- feat(app): live DialMode apply + LanOnly discovery gating

## [0.1.55] — 2026-05-28

- feat(net): ApplyDialMode command + live LAN-only switch in swarm

## [0.1.54] — 2026-05-28

- feat(core+storage): replace DialModes with mutually-exclusive DialMode enum

## [0.1.52] — 2026-05-28

- feat(app): direct-first dial priority

## [0.1.51] — 2026-05-28

- feat(net): DCUtR behaviour (V2-A5)

## [0.1.50] — 2026-05-28

- feat(net): NetEvent::ConnectionUpgraded

## [0.1.49] — 2026-05-28

- feat(net): QUIC transport (V2-A6)

## [0.1.47] — 2026-05-28

- docs: update README/ROADMAP/ARCHITECTURE/CLAUDE for V2-A4 + Settings

## [0.1.46] — 2026-05-28

- fix(ui): move onlyBuiltDependencies to pnpm-workspace.yaml

## [0.1.45] — 2026-05-28

- fix(ui): allow esbuild postinstall to run

## [0.1.44] — 2026-05-28

- fix(ui): calmer bootstrap-row layout

## [0.1.43] — 2026-05-28

- chore: sync Cargo.lock to current workspace versions

## [0.1.40] — 2026-05-28

- feat(tauri): get_settings / update_settings / bootstrap commands

## [0.1.39] — 2026-05-28

- feat(app): runtime settings, dial-mode plumbing, ping cache

## [0.1.38] — 2026-05-28

- feat(net): UpdateBootstraps command and multiaddr_is_lan export

## [0.1.37] — 2026-05-28

- feat(storage): settings table + DAO + Settings wire type

## [0.1.36] — 2026-05-28

- feat(ui): settings view, nav entry, and route

## [0.1.35] — 2026-05-28

- feat(ui): settings bridge wrappers and store

## [0.1.34] — 2026-05-28

- feat(ui): Toggle chip primitive and warn color token

## [0.1.33] — 2026-05-27

- fix(app): dedup outgoing pending requests per peer

## [0.1.32] — 2026-05-27

- fix(app): discovery step 1 now reports whether a dial was actually issued

## [0.1.31] — 2026-05-27

- ci: build only Linux AppImage + Windows .exe

## [0.1.30] — 2026-05-27

- fix(net): auto-redial lost bootstraps + live smoke harness

## [0.1.26] — 2026-05-27

- ci: merge auto-tag into release workflow so v0.1.x actually publishes

## [0.1.25] — 2026-05-27

- ci: auto-tag on version bump + slim CI + quiet production logs

## [0.1.24] — 2026-05-27

- fix(net): demote Kad routine dial-failure spam from WARN to DEBUG

## [0.1.23] — 2026-05-27

- test(three_node_kad): retry find_peer to tolerate slow runners

## [0.1.22] — 2026-05-27

- docs(readme): cross-platform paths for bootstrap.toml

## [0.1.21] — 2026-05-27

- fix(net): clippy type_complexity + collapsible_match; pin bootstrap1.y7v.lol

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
