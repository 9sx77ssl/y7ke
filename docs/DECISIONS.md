# Architecture Decisions

Append-only ADR log. Newest at the top.

## ADR-007 — UI store modules use `.svelte.ts` extension

**Date:** 2026-05-27 · **Status:** Accepted

**Context.** The frontend brief listed `src/lib/stores/*.ts` as plain TypeScript modules using Svelte 5 rune syntax (`$state`, `$derived`, `$effect`). However, `vite-plugin-svelte` only invokes `svelte.compileModule()` on files whose name contains the infix `.svelte.` (default `DEFAULT_SVELTE_MODULE_INFIX = ['.svelte.']`). A `.ts` file would emit `$state` as a literal identifier and crash at runtime.

**Decision.** Renamed all store modules from `<name>.ts` to `<name>.svelte.ts` (`identity.svelte.ts`, `contacts.svelte.ts`, `requests.svelte.ts`, `chat.svelte.ts`, `presence.svelte.ts`, `events.svelte.ts`, `route.svelte.ts`). Reactive state is exposed via a frozen accessor object (e.g. `export const contacts = { get items() { return state.items; }, ... }`) — the underlying `$state(...)` value is module-private, so callers can't mutate it accidentally.

**Consequences.** TypeScript still resolves the rune globals (they're declared at file scope in `svelte/types/index.d.ts` which is pulled in by `main.ts`'s `mount` import). Stores work in both the Vite dev server and the production build (`vite build`). The accessor-object pattern preserves a `writable()`-like ergonomic shape without the legacy `subscribe()` API.

## ADR-008 — UI events arrive on one Tauri channel, fanned out by `kind`

**Date:** 2026-05-27 · **Status:** Accepted

**Context.** The backend emits eight distinct domain events (identity, contacts, requests, messages, presence, errors). Listening on eight separate Tauri events from the UI is verbose, requires eight handler registrations, and leaks the protocol surface across stores.

**Decision.** A single Tauri event channel `"y7ke://event"` carries an `AppEvent` tagged union (`{ kind, ...payload }` with `serde(tag = "kind", rename_all = "snake_case")`). The UI registers exactly one listener in `events.svelte.ts` (`startEventDispatch`) and switches on `kind` to route into per-store apply functions.

**Consequences.** Adding a new event variant requires exactly two touch points: a new arm in the Rust `AppEvent` enum and a new arm in `dispatch()` in `events.svelte.ts`. No new Tauri event channel names to coordinate. The cost is a slightly less type-safe handler shape (the discriminator union must be narrowed at runtime), but the TypeScript switch on `ev.kind` over a discriminated union recovers exhaustiveness.

## ADR-001 — V1 ships LAN-only, 4-crate workspace

**Date:** 2026-05-27 · **Status:** Accepted

**Context.** The original spec lists libp2p with Kademlia / AutoNAT / DCUtR / relay / QUIC, OS keychain, and a 9-crate decomposition. Shipping that on day one would burn weeks before any end-to-end demo exists.

**Decision.** V1 cuts to 4 crates (`y7ke-core`, `y7ke-storage`, `y7ke-net`, `y7ke-app`) and a libp2p swarm with only TCP + Noise + Yamux + mDNS + ping + identify + three `request_response` codecs. No DHT, no NAT traversal, no QUIC. Master DEK lives in a local file (mode 0600), not in the OS keychain.

**Consequences.** V1 works only on a LAN. Internet messaging arrives in V2 by layering Kademlia + relay + DCUtR on top of the same crate boundaries (the `y7ke-net` crate grows; the swarm gets more behaviours; the API surface to `y7ke-app` stays the same). Storage and identity layers stay V1→V2-compatible: the DEK loader gains a keyring backend without changing what gets encrypted.

## ADR-002 — Identity URI: `y7:<base58(pubkey)>`

**Date:** 2026-05-27 · **Status:** Accepted

**Context.** The spec mandates a `y7:` prefix and key-based identity.

**Decision.** Encode the raw 32-byte Ed25519 public key with the Bitcoin base58 alphabet (`bs58` crate). Resulting strings are ~44 chars with no `0/O/I/l` confusion.

**Consequences.** Trivial to round-trip in tests; pasteable by humans; unambiguous. URI parsing rejects anything not matching `^y7:[1-9A-HJ-NP-Za-km-z]{43,44}$`.

## ADR-003 — Local storage encryption: app-layer columns, not SQLCipher

**Date:** 2026-05-27 · **Status:** Accepted

**Context.** SQLCipher would require a custom `libsqlite3-sys` build with bundled OpenSSL across three platforms and brittle sqlx integration.

**Decision.** Use stock `sqlx-sqlite` with the `bundled` feature. Encrypt sensitive *columns* (`users.ed25519_priv_enc`, `messages.payload_enc`, `sessions.shared_secret_enc`, `keys.material_enc`, `requests.initial_text`) with `ChaCha20-Poly1305(master_dek, random_nonce)`; store nonce alongside each ciphertext. Non-sensitive columns (`y7_id`, timestamps, status) stay in cleartext for indexability.

**Consequences.** Threat model equivalent to SQLCipher: an attacker with both the DB file and the DEK file (or keyring access in V2) decrypts everything. An attacker with only one wins nothing. Indexes still work on the plaintext columns we keep plaintext.

## ADR-004 — Message ID: UUIDv7

**Date:** 2026-05-27 · **Status:** Accepted

**Context.** Need sortable IDs (for paged scrolling and sync deltas) without a central clock authority. Need collision-free across peers.

**Decision.** Use UUIDv7 (`uuid` crate, `v7` feature). 48-bit Unix-ms timestamp + 74 bits randomness. Sorts lexicographically by time; no coordination needed.

**Consequences.** Receiver-side dedup is `INSERT OR IGNORE` on the PK. Per-sender chains are causal by timestamp. Cross-sender order is by *receiver's* insertion time — explicitly not a distributed total order.

## ADR-005 — bincode 2 for wire encoding

**Date:** 2026-05-27 · **Status:** Accepted

**Context.** libp2p's request_response framework offers CBOR and JSON codecs out of the box. Bincode is faster and produces smaller payloads.

**Decision.** Use the built-in `request_response::cbor` codec for V1 (zero glue code, well-tested). Revisit bincode only if profiling shows wire size matters.

**Consequences.** A V2 protocol revision can flip to bincode by versioning the protocol ID (`/y7ke/msg/1.1.0`) and supporting both during the cutover. The bincode crate is still pulled into the workspace for internal serialization (e.g. encrypting structured plaintext before ChaCha20-Poly1305).

## ADR-006 — V1 swarm uses public IPFS bootstrap? **No.**

**Date:** 2026-05-27 · **Status:** Accepted

**Context.** Considered seeding Kad with the public IPFS bootstrap multiaddrs.

**Decision.** V1 has no Kad. Removing the temptation also removes the dependency on third-party infrastructure and prevents Y7KE peers from spamming or appearing in IPFS DHT records.

**Consequences.** V1 is LAN-only. V2 will operate dedicated Y7KE bootstrap relays (or document a self-hosted bootstrap option) rather than reusing IPFS infrastructure.
