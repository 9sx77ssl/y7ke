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
crates/y7ke-storage    # sqlx-sqlite + master DEK + DAOs (8 tables)
crates/y7ke-net        # libp2p swarm + 3 request-response protocols
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

## Roadmap pointers

Track ordering (locked by the user): finish **C** (sync correctness +
non-blocking boot polish), then **D** (tooling — ts-rs ✅, CI builds ✅,
UI polish), then **B** (Double Ratchet, OS keyring, handshake replay
nonce), then **A** (Kademlia + bootstrap relays + DCUtR + QUIC).
Don't start internet routing until B is solid.
