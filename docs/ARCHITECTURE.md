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

Discovery is **mDNS-only** — Y7KE V1 is a LAN messenger. NAT traversal, DHT bootstrap, and internet routing land in V2.

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

## Networking (V1)

Single `#[derive(NetworkBehaviour)]` aggregating:

- `identify::Behaviour` (`/y7ke/0.1.0`)
- `ping::Behaviour` for liveness / RTT
- `mdns::tokio::Behaviour` — sole discovery mechanism in V1
- `request_response::cbor::Behaviour<HandshakeReq, HandshakeResp>` — `/y7ke/handshake/1.0.0`
- `request_response::cbor::Behaviour<MsgReq, MsgResp>` — `/y7ke/msg/1.0.0`
- `request_response::cbor::Behaviour<SyncReq, SyncResp>` — `/y7ke/sync/1.0.0`

Transports: TCP + Noise (XX) + Yamux. No QUIC in V1.

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

See `crates/y7ke-storage/migrations/0001_init.sql` (created in M0 task #4). Tables: `users`, `contacts`, `requests`, `messages`, `sessions`, `keys`, `sync_queue`, `peer_state`. Indexes on `messages(conversation_id, timestamp_ms)`, `sync_queue(next_retry_at)`, `contacts(status)`.

## What's deferred to V2

- QUIC transport
- Kademlia DHT + IPFS/Y7KE bootstrap nodes
- AutoNAT + DCUtR + circuit-relay for NAT traversal
- OS keychain integration (currently DEK is file-only)
- Argon2id passphrase vault for headless / no-libsecret hosts
- Read receipts (`Delivered` status)
- Group conversations
- File transfer
- Onion / anonymous routing
