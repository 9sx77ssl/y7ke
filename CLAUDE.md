# Y7KE — guide for AI assistants

This file primes any AI coding assistant working in this repo. Keep it
short and load-bearing; the source code is the source of truth.

## Product

Privacy-first peer-to-peer desktop messenger. End-to-end encrypted text
messaging over libp2p; local-first SQLite; no accounts. See
[`README.md`](README.md) for the user-facing pitch and
[`docs/ROADMAP.md`](docs/ROADMAP.md) for direction.

## Layout

```
crates/y7ke-core       # types, errors, crypto primitives, AppEvent, status enums
crates/y7ke-storage    # sqlx-sqlite + master DEK + DAOs (9 tables incl. settings)
crates/y7ke-net        # libp2p swarm + 3 request-response protocols + Kad + relay-client
crates/y7ke-app        # composition root — owns Db + NetHandle, runs event_loop
src-tauri              # Tauri 2 shell, command surface, event channel
ui                     # Svelte 5 + Vite + TypeScript
ui/src/lib/gen         # ts-rs-generated types (do not edit by hand)
scripts/hooks          # git hooks (auto-bump version on commit)
docs/                  # ARCHITECTURE, ROADMAP
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

# release binary (no bundle, no tauri-cli needed)
pnpm --dir ui build && cargo build --release -p y7ke-tauri

# run two local peers
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
  string enum.
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
  emit site, thread the transport through — don't drop it.
- **`pnpm-workspace.yaml`** in `ui/` carries `onlyBuiltDependencies:
  - esbuild`. pnpm 10 ignores the equivalent `pnpm` field in
  package.json; the workspace yaml is the only path that lets a
  fresh checkout `pnpm install` without manual `approve-builds`.
- **Bootstrap auto-redial.** A 15-s tick in the swarm task redials
  any configured bootstrap not currently connected; `ConnectionClosed`
  on a bootstrap clears `state.relay_reserved` so the redial re-runs
  `listen_on(/p2p-circuit)`. Don't introduce a faster spin loop —
  it'll hammer the VPS during legitimate outages.

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
+ per-peer transport surfacing shipped. Remaining: live cross-network
manual smoke (needs two real machines on different NATs), then **B**
(Double Ratchet + OS keyring + handshake replay nonce). **D2**
(Playwright E2E) can run in parallel once types stabilise.
