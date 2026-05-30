# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working
with code in this repository. Keep it short and load-bearing; the source
code is the source of truth.

## Product

Privacy-first peer-to-peer desktop messenger. End-to-end encrypted text
messaging over libp2p; local-first SQLite; no accounts. See
[`README.md`](README.md) for the user-facing pitch and
[`docs/PLAN.md`](docs/PLAN.md) — the single source of truth (architecture,
milestone history, capability matrix with PROVEN/SIMULATED/UNVERIFIED/PLANNED
tags, security model, roadmap).

## Layout

```
crates/y7ke-core       # types, errors, crypto primitives, AppEvent, status enums
crates/y7ke-storage    # sqlx-sqlite + master DEK + DAOs (10 tables; settings + pending_deletes)
crates/y7ke-net        # libp2p swarm + 3 request-response protocols + Kad + relay-client
crates/y7ke-app        # composition root — owns Db + NetHandle, runs event_loop
src-tauri              # Tauri 2 shell, command surface, event channel
ui                     # Svelte 5 + Vite + TypeScript
ui/src/lib/gen         # ts-rs-generated types (do not edit by hand)
scripts/hooks          # git hooks (auto-bump version + CHANGELOG on commit)
docs/PLAN.md           # single source of truth (arch, history, capability matrix, roadmap, live-smoke)
```

## Conventions

- **Short comments.** Single-line, factual. No paragraph blocks
  explaining what the code already says. No "added for X / used by Y"
  references — those rot.
- **No speculative scaffolding.** Don't add features, error handling,
  or abstractions the task doesn't need.
- **Tests stay green.** `cargo test --workspace` and `pnpm tsc --noEmit`
  must pass before commit; the pre-commit hook will bump the patch
  version into the same commit.
- **Wire types are authoritative.** When you change `AppEvent`,
  `ContactView`, `MessageView`, etc., run `cargo test` once so ts-rs
  regenerates `ui/src/lib/gen/`.
- **Bootstrap node lives in a separate repo** — `9sx77ssl/y7ke-bootstrap`.
  Don't add the bootstrap daemon back into this repo. The client side
  (Kad behaviour, `find_peer`, `DEFAULT_BOOTSTRAPS`) is here; the daemon
  is its own crate with zero `y7ke-*` dependencies because it must
  remain stateless (it can't decrypt traffic if it never sees Y7KE wire
  types).
- **mDNS-dependent integration tests** are guarded with
  `#[cfg_attr(any(target_os = "macos", target_os = "windows"), ignore)]`
  because GitHub Actions runners don't surface peers reliably. They run
  on Linux CI + locally on any platform.
- **UI silently broken? Open the WebKit JS console first** (Ctrl+Shift+I
  in any Tauri window). The 0.1.5–0.1.17 "invisible messages" bug was a
  single `class:failed` Svelte binding without `={expr}` — Svelte
  interpreted it as "apply if the variable `failed` is truthy", the
  variable didn't exist, `ReferenceError` killed the `{#each}` tree on
  every render with `is_mine=true`. Backend logs and store tracing all
  showed `stateLen=10`; only the JS console exposed the error. Build
  failures, store-vs-UI desyncs, "reactivity is broken" symptoms — check
  the browser console **before** rewriting Svelte stores or rebuilding
  Vite.
- **Svelte shorthand bindings need `={expr}`.** `class:foo` without `=`
  references a variable named `foo` in scope. If `foo` isn't a top-level
  binding in the component, it throws at render. Always use
  `class:foo={booleanExpr}` unless you've also declared `let foo`.

## Useful commands

```bash
# dev (Tauri + Vite together)
cargo tauri dev

# release binary WITH embedded UI — MUST go through the Tauri CLI.
# Plain `cargo build --release -p y7ke-tauri` does NOT embed the frontend:
# the resulting binary loads the dev URL and dies with "Could not connect to
# localhost". CI uses `cargo tauri build`; do the same locally, or grab the
# CI-built AppImage from the GitHub release.
cargo tauri build --bundles appimage   # → target/release/y7ke (+ AppImage)

# run two local peers (after a `cargo tauri build`)
Y7KE_DATA_DIR=/tmp/y7ke-alice ./target/release/y7ke &
Y7KE_DATA_DIR=/tmp/y7ke-bob   ./target/release/y7ke &

# tests
cargo test --workspace
cargo test -p y7ke-app --test v1_stress -- --ignored

# lint
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
cd ui && pnpm tsc --noEmit
```

## Architectural pins (do not casually break)

- **libp2p protocols.** `/y7ke/handshake/1.0.0`, `/y7ke/msg/1.0.0`,
  `/y7ke/sync/1.0.0`. CBOR-encoded request-response. Wire types live in
  `crates/y7ke-net/src/protocol.rs` and are byte-flat (no
  `y7ke-crypto` types).
- **Control payloads** ride inside `/y7ke/msg/1.0.0` via a tag byte
  (0x00 text, 0x01 control). Adding a new control = add a variant to
  `messaging::ControlPayload` and a branch in `event_loop::handle_control`.
- **MessageStatus** serializes as `i64` (via `serde_repr`) so the UI's
  `MSG_SENDING / MSG_SENT / ...` constants match. Don't switch back to
  string enum. Only two terminal states are written in production:
  `Delivered(2)` (peer's RR handler acked) and `Synced(3)`; `Sent(1)` and
  `Failed(4)` are dead — don't start writing them (the UI renders 2 and 3
  identically, so promoting Delivered→Synced is seamless).
- **Reliable delivery contract.** A send that isn't live-acked is enqueued
  in `sync_queue` (status stays `Sending`) and re-driven by
  `event_loop::drain_queue_for_peer` on every reconnect (`PeerDiscovered` /
  `ConnectionEstablished` / `ConnectionUpgraded`, `respect_schedule=false` =
  flush all) and by the presence ticker (`respect_schedule=true` = honour
  `next_retry_at`). The DCUtR relay→direct upgrade folds the relayed
  connection, killing any RR request pinned to it — so `ConnectionUpgraded`
  MUST drain (it doesn't re-emit otherwise). `SEND_TIMEOUT` (app.rs) MUST
  stay ABOVE the request-response `request_timeout` (15s, behaviour.rs) —
  a shorter cap abandons a still-live request and spuriously re-queues. As a
  durable backstop, `kick_sync_for_peer` promotes our outbound rows the peer
  reports as held (`their.highest_inbound_msg_id`) to `Synced` via
  `messages().promote_outbound_synced`, so a delivered-but-ack-lost message
  can't stay stuck on the sending clock.
- **`AppState`** (Tauri side) wraps an `Option<Arc<AppHandle>>` so
  commands can be invoked before boot completes; `.get().await` blocks
  until the slot is filled.
- **Rate limiter** lives in `crates/y7ke-app/src/rate_limit.rs`. Inbound
  RPCs gate through `inner.rate_limiter.allow_*` before any DB work.
- **`NetCommand::Dial` returns Ok(true)/Ok(false)/Err** via a oneshot;
  `Ok(true)` means a dial was actually issued, `Ok(false)` means the
  swarm has no addresses for the peer (so callers must fall through
  to discovery). The discovery chain in `app/contacts.rs` relies on
  this — don't switch it back to fire-and-forget without rewriting
  `dial_with_discovery`.
- **`DEFAULT_RELAY_BOOTSTRAP`** lives in `y7ke_core::settings`. It's
  always prepended (deduped) to whatever else `load_bootstraps`
  resolves so the UI's Settings page can't strand the client by
  deleting all entries. UI renders it `readonly` with `is_default=true`
  and no delete button.
- **Transport-agnostic bootstrap shorthand.** Bootstrap descriptors are
  written WITHOUT a transport: `/dns4/host/<port>/p2p/<id>` (no `/tcp`,
  no `/udp`). `y7ke_core::expand_bootstrap` fans each shorthand into BOTH
  `/tcp/<port>` and `/udp/<port>/quic-v1` multiaddr strings; an explicit
  multiaddr (already naming `/tcp` or `/udp`) passes through unchanged.
  `app/config.rs::parse_multiaddrs` runs every descriptor through it
  (deduped). In the swarm, `bootstrap_peers` is
  `HashMap<PeerId, Vec<Multiaddr>>` and the build / reconnect /
  `UpdateBootstraps` / `ApplyDialMode` paths dial every addr per peer in
  one `DialOpts` (QUIC + TCP race; QUIC wins on UDP-open nets and enables
  hole-punch, TCP is the fallback). Don't collapse this back to a single
  addr per bootstrap.
- **Connection transport is surfaced.** `AppEvent::PresenceChanged` and
  `ContactView` carry `transport: Option<Transport>`; `refresh_presence`
  returns `(ConnectionKind, Option<Transport>)`; the chat
  `ConnectionLabel` renders e.g. "DIRECT · QUIC". When you add a presence
  emit site, thread the transport through — don't drop it. `Internet` and
  `Direct` are BOTH direct (no relay): `Internet` = direct dial succeeded
  outright; `Direct` = relay→DCUtR hole-punch upgrade. Neither is a relay
  path.
- **Presence is sourced ONLY from `inner.connections`**, written ONLY by the
  `ConnectionEstablished` / `ConnectionUpgraded` handlers; `refresh_presence`
  reads nothing else. libp2p will NOT re-emit `ConnectionEstablished` for an
  already-open connection, so anything that clears `inner.connections` while
  the socket lives (e.g. `wipe_conversation`) would strand presence at
  Offline forever. Two guards keep them in sync: (1) `delete_contact` calls
  `NetHandle::disconnect_peer` (→ `NetCommand::DisconnectPeer` →
  `swarm.disconnect_peer_id`, closes relay+direct) AFTER `flush_pending_delete`
  delivers `ChatDeleted`, so a re-add re-dials fresh; (2) the presence
  ticker's Offline arm drops a stale socket (`check_live==true` but map empty)
  so the next dial re-establishes — never synthesize a fake `ConnEntry` (no
  `ConnectionClosed` could remove it → false-online).
- **Durable chat-delete.** `ChatDeleted` must reach the peer even if they're
  offline. `delete_contact` seals it BEFORE `wipe_conversation` (the wipe
  destroys the session) and stashes the sealed envelope in the
  `pending_deletes` table (migration 0007); `flush_pending_delete` retries it
  on every reconnect until acked. `Db::wipe_peer` MUST NOT clear
  `pending_deletes` — that's what lets the deletion survive the local wipe.
- **`pnpm-workspace.yaml`** in `ui/` carries BOTH `onlyBuiltDependencies:
  - esbuild` (honored by pnpm 10 — what CI pins) AND `allowBuilds:
  esbuild: true` (required by pnpm 11.3+ — what `cargo tauri dev` runs
  locally). pnpm 10 ignores the equivalent `pnpm` field in package.json;
  the workspace yaml is the only path that lets a fresh checkout
  `pnpm install` without manual `approve-builds`. pnpm 11 stopped honoring
  `onlyBuiltDependencies` alone: it ignores esbuild's build, records it in
  `node_modules/.modules.yaml`'s `ignoredBuilds`, and makes `pnpm install`
  exit 1 — which breaks tauri's `beforeDevCommand` deps-status-check. Keep
  both keys; unknown keys are warned-not-errored by either pnpm version, so
  the file is safe on both. (Separately: a pnpm major bump leaves
  `node_modules` linked to the old store — `…/store/vN` — and pnpm wants to
  purge it; in a no-TTY `cargo tauri dev` that aborts with
  `ERR_PNPM_ABORTED_REMOVE_MODULES_DIR_NO_TTY`. One-time `CI=true pnpm
  install` in `ui/` rebuilds against the new store.)
- **Bootstrap auto-redial.** A 15-s tick in the swarm task redials
  any configured bootstrap not currently connected; `ConnectionClosed`
  on a bootstrap clears `state.relay_reserved` so the redial re-runs
  `listen_on(/p2p-circuit)`. Don't introduce a faster spin loop —
  it'll hammer the VPS during legitimate outages.
- **Frameless window is shown after GTK realizes it.** The main window is
  `"visible": false` in `src-tauri/tauri.conf.json`; `main.rs` reveals it
  (`get_webview_window("main").center()/.show()`) on a short post-`setup()`
  tick. Showing immediately on webkit2gtk (Linux) paints the first frame
  against an unsettled allocation → the `100dvh` root collapses (cramped
  layout, a different window size each launch). Don't drop `visible:false`
  or the Rust show. Rust-side `show()` needs no ACL capability; a JS
  `show()` would be rejected (`core:window:default` lacks `allow-show`).
- **App boot runs in `onMount`, never `$effect`.** `App.svelte` starts the
  single Tauri event listener (`startEventDispatch`) and hydrates the
  identity + settings stores once on mount. It MUST be `onMount`: an
  `$effect` that calls a fn which reads-and-writes a tracked `$state` flag
  (e.g. `identity.loading`) across an `await` self-retriggers — the async gap
  dodges Svelte's depth guard — and re-runs its cleanup
  (`stopEventDispatch` → `unlisten`) in a loop, so inbound events emitted
  during a listener-down window are dropped at the Tauri layer (looks like
  "messages only render after re-entering the chat" + a `Couldn't find
  callback id` storm — on RELEASE too, not just dev). Dedup guards for
  boot-time loaders must be a non-reactive module boolean (`inFlight`), NOT a
  tracked `$state` read.
- **Dev full-reload.** `ui/vite.config.ts` forces a full webview reload on
  every dev file save (kills the HMR singleton-store split that otherwise
  splits `$state` across module generations). Consequence: a short burst of
  `Couldn't find callback id` right after a save is EXPECTED and benign in
  dev only (`import.meta.hot` is undefined in the release bundle).
- **Versioning + release are hook- and push-driven.** `scripts/hooks/pre-commit`
  bumps the patch version across `Cargo.toml`, `src-tauri/tauri.conf.json`,
  and `ui/package.json`; `post-commit` prepends a `CHANGELOG.md` entry with
  the commit subject and amends. Pushing `main` triggers
  `.github/workflows/release.yml`, which resolves the version from
  `Cargo.toml`, creates the matching `vX.Y.Z` tag, builds the Linux AppImage
  + Windows NSIS bundles, and publishes a GitHub release whose body is the
  matching `## [X.Y.Z]` CHANGELOG section. The release `build` is gated on
  fmt + clippy + tests + tsc + UI build. To land an exact version (e.g. a
  minor/major) without the patch bump, commit `--no-verify` and write the
  CHANGELOG section by hand.

## V2-A4 notes (circuit relay + Settings)

- **Bootstrap external addresses** must be declared via
  `--external-addr` / `Y7KE_BOOTSTRAP_EXTERNAL_ADDR` on the
  `y7ke-bootstrap` daemon (v0.1.4+). Without them, libp2p's relay
  server sends reservation acks with an empty addrs list and the
  client transport errors with `NoAddressesInReservation`. These are
  the daemon's own *explicit* transport multiaddrs (TCP and QUIC), NOT
  the client shorthand — keep the `/tcp` / `/udp/quic-v1` here. The VPS
  systemd drop-in at
  `/etc/systemd/system/y7ke-bootstrap.service.d/external-addr.conf`
  sets `Y7KE_BOOTSTRAP_EXTERNAL_ADDR=/dns4/bootstrap1.y7v.lol/tcp/4101,/ip4/89.35.130.67/tcp/4101,/dns4/bootstrap1.y7v.lol/udp/4101/quic-v1,/ip4/89.35.130.67/udp/4101/quic-v1`.
  UDP/4101 must be open on the VPS firewall (`ufw allow 4101/udp`) for
  QUIC to reach the bootstrap. The daemon (v0.1.6+) prints the
  transport-agnostic client descriptor on startup
  (`/dns4/bootstrap1.y7v.lol/4101/p2p/<id>`) — that's the line operators
  paste into the client's Settings.
- **`AppEvent::SettingsChanged`** fires when the user saves. The UI's
  events dispatcher subscribes and refreshes its store; the swarm
  task receives a `NetCommand::UpdateBootstraps` to re-sync its
  `bootstrap_peers` map.
- **Idempotent `send_contact_request`** dedups by querying
  `requests().list_pending(Some(Outgoing))` and skipping insert
  when a row for the same peer already exists. Migration
  `0003_dedup_outgoing_requests.sql` collapses any duplicates
  accumulated before the fix.

## Roadmap pointers

A1 + A2 + A3 + A4 + A5 + A6 shipped (AutoNAT v2 reachability, DCUtR
Relayed→Direct upgrade, QUIC dual-transport). Settings UI shipped;
dial modes consolidated to two (LanOnly / Internet="Y7net"). Bootstrap
descriptors are transport-agnostic shorthand. Connectivity debug pane
+ per-peer transport surfacing shipped. 3.x hardening shipped (3.0.13–
3.0.16): reliable message delivery, durable chat-delete propagation,
live-render correctness (boot-effect listener-flap fix), and presence
re-establish on delete+re-add. Remaining: live cross-network manual smoke
(needs two real machines on different NATs), then **B** (Double Ratchet +
OS keyring + handshake replay nonce). **D2** (Playwright E2E) can run in
parallel once types stabilise.
