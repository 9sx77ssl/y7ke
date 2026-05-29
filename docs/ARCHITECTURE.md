# Y7KE Architecture

## V1 scope

V1 ships seven user-visible capabilities working end-to-end on a LAN:

1. **Generate identity** — Ed25519 keypair created on first launch, persisted encrypted in SQLite.
2. **Add contact by key** — paste a `y7:` URI, send a contact request.
3. **Accept request** — receiver accepts; both peers persist the contact and a shared X25519 session.
4. **Open chat** — pick a contact, see message history.
5. **Exchange encrypted messages** — live send over `/y7ke/msg/1.0.0`, ChaCha20-Poly1305 with the session key.
6. **SQLite persistence** — survives app restart with ciphertext on disk.
7. **Offline sync after reconnect** — undelivered messages drain through `/y7ke/sync/1.0.0` reconcile protocol when peers meet again.

Discovery is **mDNS-only** — Y7KE V1 is a LAN messenger. NAT
traversal (DHT bootstrap, circuit relay) lands incrementally in V2.
See the "V2 additions" section below for what shipped on top.

## Crate layout

```
crates/
  y7ke-core/      types + errors + IDs + events + crypto primitives (Ed25519, X25519, ChaCha20-Poly1305, HKDF, blake3)
  y7ke-storage/   sqlx-sqlite + migrations + DAOs + master-DEK-file + field encryption
  y7ke-net/       libp2p swarm + 3 request_response codecs + session handshake + sync state machine
  y7ke-app/       composition root + Tauri command surface + headless harness
src-tauri/        Tauri 2 shell, depends on y7ke-app
ui/               Svelte + TypeScript + Vite
```

### Dependency DAG

```
y7ke-core    ─── leaf
y7ke-storage ─── core
y7ke-net     ─── core, storage
y7ke-app     ─── core, storage, net
src-tauri    ─── y7ke-app
ui           ─── @tauri-apps/api only
```

All edges one-way. Adding a crate beyond these four is V2-only.

## Networking

Single `#[derive(NetworkBehaviour)]` (`Y7Behaviour`) aggregating:

- `identify::Behaviour` (`/y7ke/0.1.0`)
- `ping::Behaviour` for liveness / RTT
- `mdns::tokio::Behaviour` — LAN discovery
- `request_response::cbor::Behaviour<HandshakeReq, HandshakeResp>` — `/y7ke/handshake/1.0.0`
- `request_response::cbor::Behaviour<MsgReq, MsgResp>` — `/y7ke/msg/1.0.0`
- `request_response::cbor::Behaviour<SyncReq, SyncResp>` — `/y7ke/sync/1.0.0`
- `kad::Behaviour<MemoryStore>` (V2-A1) — `/y7ke/kad/1.0.0`, server mode, each client `start_providing`s its own key
- `relay::client::Behaviour` (V2-A4) — circuit-relay-v2 client; the bootstrap (separate `y7ke-bootstrap` repo) carries `relay::Behaviour` as server

Transports: TCP + Noise (XX) + Yamux **and** QUIC (`/quic-v1`), with `with_relay_client(...)` adding a separate transport for `/p2p-circuit` dials. Bootstraps are dialed on both TCP and QUIC simultaneously (QUIC preferred — it enables direct hole-punch); the live transport per connection is recorded as `Transport::{Tcp, Quic}`.

### Discovery + dialing

`y7ke_core::settings::DialMode` has exactly two variants:
`LanOnly` (shown in the GUI as "lan only") and `Internet` (shown as
"Y7net", the default). A former third variant `P2p` was removed —
it was a behavioural duplicate of `Internet` (identical dial chain;
the DCUtR hole-punch upgrade runs automatically regardless of mode).
Migration `0006_drop_p2p_dialmode.sql` rewrites legacy `"P2p"`
settings rows to `Internet`.

`crates/y7ke-app/src/app/contacts.rs::dial_with_discovery` runs four
steps in order, gated by the user's current `DialMode`:

1. **Swarm address book** — `net.dial(peer)` looks up the in-memory
   cache populated by mDNS + identify. Returns `Ok(true)` if a dial
   was actually issued, `Ok(false)` if the cache is empty (so the
   chain continues). In `LanOnly` mode, if every known addr is
   non-LAN this step is skipped.
2. **Cached addrs** — `peer_state.last_addrs_json` (filtered by the
   active mode), persisted across restarts.
3. **Kad lookup** — `find_peer(y7_id)` issues `get_providers` with
   a 10-s timeout; in `LanOnly` mode non-LAN results are dropped.
4. **Last-resort re-dial** — by now Kad may have populated the
   routing table.

`connection_kind_for(endpoint)` classifies the new connection using
`ConnectedPoint::is_relayed()` →
`ConnectionKind::{Lan, Internet, Relayed, Direct}`. The UI's
`StatusDot` shows online/offline; a small `RELAY` text label in
muted lilac (or `LAN`/`INTERNET`/`DIRECT` green) renders next to
the peer's nickname.

The underlying transport (`Transport::{Tcp, Quic}`, extracted from
the connection's multiaddr) is tracked per connection alongside the
`ConnectionKind` and surfaced as `transport: Option<Transport>` on
both `AppEvent::PresenceChanged` and `ContactView`. The chat
`ConnectionLabel` renders the pair, e.g. "DIRECT · QUIC".

### Bootstrap reservation + auto-reconnect (V2-A4)

On every `ConnectionEstablished` for a peer that matches an entry
in `TaskState::bootstrap_peers`, the swarm task calls
`swarm.listen_on(<addr>/p2p-circuit)`. libp2p drives the
HOP-RESERVE roundtrip; on accept, `Y7BehaviourEvent::RelayClient`
fires `Event::ReservationReqAccepted` and a new
`SwarmEvent::NewListenAddr` lands carrying
`/<bootstrap>/.../p2p-circuit/p2p/<self>`. That address gets
announced via identify, picked up by Kad as a provider record, and
becomes dialable by other clients.

A `tokio::time::interval(15s)` ticks alongside the swarm select
loop; on each tick it calls `reconnect_lost_bootstraps`, which
iterates `bootstrap_peers` and `swarm.dial(addr)`s any that
`swarm.is_connected()` reports as down. `ConnectionClosed` for a
bootstrap removes the entry from `relay_reserved` so the
reconnect's `listen_on(/p2p-circuit)` re-runs.

## Identity

`y7:<base58(ed25519_pubkey)>` — 32-byte Ed25519 public key encoded with the Bitcoin base58 alphabet, prefixed `y7:`. Roughly 44–46 characters, human-typable, no ambiguous glyphs.

Private key stored encrypted in `users.ed25519_priv_enc` with `ChaCha20-Poly1305(master_dek, random_nonce)` where `master_dek` is a 32-byte file at `<app_data>/y7ke/master.dek` (mode 0600).

## Sessions

When two contacts first connect successfully, they run `/y7ke/handshake/1.0.0`:

1. Initiator generates an X25519 ephemeral keypair, signs the ephemeral public key with its Ed25519 long-term key, sends `(eph_pub_a, sig_a)`.
2. Responder verifies the signature, generates its own ephemeral, sends `(eph_pub_b, sig_b)`.
3. Both compute `shared = X25519(my_eph, their_eph)` and derive `session_key = HKDF-SHA256(salt=blake3(sort(pub_a, pub_b)), ikm=shared, info="y7ke-session-v1", L=32)`.

The 32-byte session key is stored encrypted in `sessions.shared_secret_enc` keyed off the local DEK. Sessions persist across restarts.

## Messages

Each message is a `MessageEnvelope`:

```rust
struct MessageEnvelope {
    message_id:    Uuid,      // UUIDv7, 16 bytes
    sender_pub:    [u8; 32],  // Ed25519 public key of sender
    timestamp_ms:  i64,
    nonce:         [u8; 12],  // ChaCha20-Poly1305 nonce, random per message
    ciphertext:    Vec<u8>,   // session_key encrypts plaintext UTF-8 text
    sig:           [u8; 64],  // Ed25519(sender_priv, message_id || ts || ciphertext)
}
```

Live send pushes the envelope via `/y7ke/msg/1.0.0`. The receiver verifies sig, decrypts, INSERT-OR-IGNOREs into `messages`. Status transitions:

```
Sending → (push acknowledged) → Sent → (sync confirms peer has it) → Synced
                              ↘ (network failure) → Failed (terminal; or re-queued)
```

`Delivered` is a V2 state for read-receipts.

## Offline sync

When the local peer comes online and discovers a known contact via mDNS, it initiates `/y7ke/sync/1.0.0`:

1. **Header** — each side sends per-conversation `(highest_inbound_msg_id, highest_outbound_msg_id)`.
2. **Pull** — side that's behind requests missing messages by `(conversation_id, since_msg_id, limit)`.
3. **Ack** — side that received the pulled messages confirms persisted IDs; sender updates `Sent → Synced` and removes rows from `sync_queue`.

The `sync_queue` table holds pending outbound deliveries with exponential backoff (`next_retry_at = now + min(2^attempts * 30s, 1h)`). A background `RetryDriver` polls every 15s.

## Storage schema

See `crates/y7ke-storage/migrations/` — migrations land in order:

| Migration | What |
|---|---|
| `0001_init.sql` | V1 baseline: `users`, `contacts`, `requests`, `messages`, `sessions`, `keys`, `sync_queue`, `peer_state` + indexes on `messages(conversation_id, timestamp_ms)`, `sync_queue(next_retry_at)`, `contacts(status)`. |
| `0002_strip_session_key.sql` | Drop the legacy `sessions.shared_secret_enc` column once static-DH derivation replaced stored session keys. |
| `0003_dedup_outgoing_requests.sql` | One-shot cleanup for installs that piled up duplicate outgoing requests during the V2-A4 dial-discovery bug. Keeps the earliest row per peer, drops the rest. |
| `0004_settings.sql` | V2-A4 single-row `settings` table seeded with defaults; `(id INTEGER PRIMARY KEY CHECK (id = 1), payload_json TEXT NOT NULL, updated_at INTEGER NOT NULL)`. |
| `0005_dialmode_singular.sql` | Replace the 4-bool `dial_modes` object with a single `dial_mode` enum string; existing rows map to `Internet`. |
| `0006_drop_p2p_dialmode.sql` | Remove the `P2p` dial mode (behavioural duplicate of `Internet`); rewrite any settings row still holding `"P2p"` to `"Internet"`. |

## Settings + dial modes (V2-A4)

`y7ke_core::settings::{Settings, DialMode, BootstrapEntry,
DEFAULT_RELAY_BOOTSTRAP}` define the wire types (re-exported, ts-rs
generates the matching TS in `ui/src/lib/gen/`). Stored as JSON in
the `settings` row. `DialMode` is the two-variant `LanOnly` /
`Internet` enum described under "Discovery + dialing".

`y7ke_storage::dao::SettingsDao::{get, update}` round-trip a
single-row entry. `y7ke_app::AppHandle` exposes
`get_settings`, `update_settings`, `list_bootstraps`,
`ping_all_bootstraps`, `select_best_bootstrap`. `ping_all_bootstraps`
opens raw TCP (5-s budget, `tokio::join_all`) and caches latencies
in `AppInner::bootstrap_pings: RwLock<HashMap<String,
BootstrapPingState>>`. `update_settings` writes the row, calls
`net.update_bootstraps(...)` (the new `NetCommand::UpdateBootstraps`),
and emits `AppEvent::SettingsChanged` so the UI refreshes its
in-memory copy.

`y7ke_app::config::load_bootstraps(&Db)` resolves the effective
list at boot in this order, first non-empty wins: env →
`Settings::extra_bootstraps` from the DB → `bootstrap.toml` file →
compile-time `DEFAULT_BOOTSTRAPS`. Whatever source contributes,
`DEFAULT_RELAY_BOOTSTRAP` is always prepended (deduped) so a typo
in user config can never strand the client.

Bootstrap descriptors use a transport-AGNOSTIC shorthand —
`/dns4/host/<port>/p2p/<id>` (no `/tcp` or `/udp`).
`y7ke_core::expand_bootstrap` expands each shorthand into BOTH a
`/.../tcp/<port>/p2p/<id>` and a `/.../udp/<port>/quic-v1/p2p/<id>`
multiaddr and the swarm races them (QUIC preferred for hole-punch,
TCP fallback). An already-explicit multiaddr (one naming `/tcp` or
`/udp`) passes through unchanged. In the swarm, `bootstrap_peers`
is therefore a `HashMap<PeerId, Vec<Multiaddr>>` (multiple addrs
per bootstrap peer).

## Shipped since V2-A4

- AutoNAT v2 (A3) — public-reachability detection (`NatReachability`)
  + UI pill, driving the upgrade-from-relay loop.
- DCUtR (A5) — upgrades `Relayed` → `Direct` once hole-punched;
  runs automatically (no dedicated dial mode).
- QUIC transport (A6) — `/quic-v1` added; bootstraps raced TCP+QUIC,
  live transport tracked per connection (`Transport::{Tcp, Quic}`).

## What's still deferred

- OS keychain integration (currently DEK is file-only)
- Argon2id passphrase vault for headless / no-libsecret hosts
- Double Ratchet forward secrecy (B2)
- Group conversations, file transfer, onion / anonymous routing
