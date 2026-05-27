# Y7KE V1 Audit

**Date:** 2026-05-27 · **Scope:** full workspace after V1 E2E tests passed but before any V2 work begins.

Reviewer's stance: this audit assumes the worst about uninvestigated code. Severity ratings reflect what could happen under adversarial input or in plausible production traffic. Tests passing ≠ correct under attack.

---

## Methodology

1. `grep -rnE '(TODO|FIXME|XXX|HACK|unimplemented!|todo!|panic!)'` across `crates/`, `src-tauri/src/`, `ui/src/`.
2. `grep -rnE '\.(unwrap|expect)\('` filtered to production code (excluding `#[cfg(test)]` modules and `tests/` directories).
3. `grep -rnE '^\s*let _ = '` to find dropped errors.
4. `grep -rniE 'placeholder|stub|mock|fake|dummy'` to surface scaffolding code.
5. Read each handler in `event_loop.rs`, each command in `app.rs`, the handshake/messaging modules, the swarm task, and the DAO layer.

Result: **0** TODO/FIXME comments. **0** `unimplemented!()`/`todo!()`. All `panic!()`/`expect()`/`unwrap()` in production code (≠ tests) audited individually and listed below.

---

## Critical (blocks V1 ship — must fix before any V2 work)

### C1 — `peer_id_from_y7` panics on adversarial Y7Id input

**File:** `crates/y7ke-net/src/swarm.rs:610`

```rust
pub fn peer_id_from_y7(y7_id: &Y7Id) -> PeerId {
    let pubkey = identity::ed25519::PublicKey::try_from_bytes(y7_id.pubkey())
        .expect("Y7Id always carries a valid 32-byte Ed25519 pubkey");
    ...
}
```

`Y7Id::from_pubkey([u8; 32])` accepts arbitrary 32 bytes. `Y7Id::parse(&str)` accepts any 32 base58-decoded bytes. Most 32-byte sequences are NOT valid Ed25519 public keys (must be a point on the curve). `try_from_bytes` returns `Err` for invalid points and we `.expect()` it → **panic in the swarm task → whole app crashes**.

**Attack path:** user pastes a crafted `y7:<base58>` URI into "Add contact". App calls `send_contact_request` → `peer_id_from_y7` → panic. Trivially exploitable from a malicious paste.

**Fix:**
- Validate at `Y7Id::parse` that the bytes form a valid Ed25519 point (try `VerifyingKey::from_bytes`); reject if not.
- Change `peer_id_from_y7` to return `Result<PeerId, AppError>`.
- Update call sites.

---

## High (correctness / privacy — fix before continuing)

### H1 — `send_contact_request` overwrites existing sessions, breaking offline queue

**File:** `crates/y7ke-app/src/app.rs:128–163`

The method always runs the handshake and upserts a new session, regardless of whether one already exists. The new `session_key` is freshly derived from a new X25519 ephemeral, so it does NOT match the key used to encrypt any messages already sitting in `sync_queue` for that peer.

**Failure mode:** Alice adds Bob, sends 3 messages while Bob is offline (queued, encrypted with `session_v1`). Alice clicks "Add Bob" again (or any code path triggers a re-handshake) → session now `session_v2`. mDNS rediscovers Bob → queue drains, but envelopes still hold v1 ciphertext + Alice's local session is v2 → Bob receives garbage → decrypt fails → silent message loss.

**Fix:** make `send_contact_request` idempotent. If a contact + session for `peer` already exists, return without dialing or handshaking.

### H2 — Sync `Pull` responder leaks any conversation it has

**File:** `crates/y7ke-app/src/event_loop.rs:234` (handle_sync, `SyncReq::Pull`)

```rust
let rows = inner.db.messages().pull_after(&conv, since_id, limit as i64).await?;
```

The peer who opened `/y7ke/sync/1.0.0` can request **any** `conversation_id` — including ones they're not a party to. There's no check that the requester (PeerId on the connection) is `sender_pub` or `recipient_pub` for the messages returned.

**Failure mode:** Mallory connects to Alice over mDNS. Mallory enumerates `blake3(sort(alice_pub, X))` for every Y7 ID she's curious about and pulls messages between Alice and X. The 128-bit `ConversationId` is hard to brute-force blind, but if Mallory knows ANY of Alice's contacts (e.g. via mDNS discovery), she pulls their conversations cleanly.

**Fix:** at `SyncReq::Pull`, compare `_peer` (currently underscored — that's the actual evidence!) → derive its `Y7Id` → compute `ConversationId::between(local, requester)` and require it to equal the requested `conversation_id`. Otherwise return `SyncResp::Pull { envelopes: vec![], has_more: false }`.

### H3 — `handle_handshake` inserts a fresh request row on every replay

**File:** `crates/y7ke-app/src/event_loop.rs:107–163`

Every inbound `HandshakeReq` produces:
1. an `upsert` of the session (silently rolls the key — see H1)
2. an unconditional `requests.insert` with `NewRequest::Incoming`

There is no dedup. An attacker who captures or crafts a valid `HandshakeReq` can spam Bob's "Requests" list with thousands of identical entries.

**Fix:**
- Skip the insert if a still-pending incoming request from `initiator_y7` already exists.
- Don't re-derive the session if one is already established with the initiator (or version sessions; see H1).

### H4 — `send_contact_request` silently drops dial errors

**File:** `crates/y7ke-app/src/app.rs:156`

```rust
let _ = self.inner.net.dial(peer).await;
```

If the dial fails (no addresses cached, mDNS hasn't run yet, peer unreachable) we proceed straight to `send_handshake`, which then fails with a less actionable error. The user sees "send_handshake: command channel closed" or similar instead of "no addresses for peer — wait for mDNS".

**Fix:** capture the dial error, log it, AND attempt `send_handshake` anyway (libp2p will buffer-dial if it has any addresses known). The bare `let _ =` is wrong.

---

## Medium (V1 correctness gaps, not exploits)

### M1 — `Sent` → `Synced` transition never fires for queue-drained messages

**File:** `crates/y7ke-app/src/event_loop.rs:269–281` (drain_queue_for_peer)

When a queued message successfully drains, status moves `Sending → Sent`. There's no further transition to `Synced` because the V1 queue path doesn't initiate the 3-round `SyncReq::Ack` flow.

**Fix:** treat `MsgResp.ack=true` from a queue drain (and from initial live send) as confirmation; transition straight to `Synced`. The peer has persisted the row and INSERT-OR-IGNOREs duplicates, so this is safe.

### M2 — No size cap on inbound message ciphertext

**File:** `crates/y7ke-net/src/protocol.rs:99` (MessageEnvelope.ciphertext: Vec<u8>), receiver path in `event_loop::handle_msg`.

A peer can send a 1 GB `MsgReq` and Y7KE will allocate + persist it. mDNS broadcasts on the LAN are unauthenticated — anyone on the network can connect (Noise handshake succeeds for any keypair) and submit a giant envelope.

**Fix:** add a constant `MAX_MESSAGE_BYTES = 64 * 1024` (V1 spec is text only — 64 KB is generous). Reject larger envelopes at `handle_msg` with `MsgResp { ack: false }` and an `AppEvent::BackgroundError`.

### M3 — `HandshakeResp.accept` field is unused

**File:** `crates/y7ke-net/src/protocol.rs:78`, `crates/y7ke-app/src/handshake.rs:122` (responder always sets `accept: true`), `crates/y7ke-app/src/app.rs:158` (initiator never checks).

The protocol field implies meaning ("did the responder accept the session?") but we always set true and never read. Dead protocol surface.

**Fix:** either drop the field (protocol version bump) or use it for a real signal (e.g. "blocked — go away"). For V1 polish, drop it.

### M4 — Inbound handshake doesn't verify peer's libp2p PeerId

**File:** `crates/y7ke-app/src/event_loop.rs:85`

```rust
NetEvent::HandshakeReceived { peer: _, request, channel }
```

We discard `peer`. The connection's libp2p PeerId is derived from the peer's authenticated Ed25519 key (Noise handshake). We then accept `request.initiator_ed25519_pub` purely on the strength of the signature inside the request.

Defense-in-depth: if `PeerId::from(request.initiator_ed25519_pub) != peer`, something's wrong (signature forgery would still pass the explicit verify, but mismatched PeerId is a clear protocol violation).

**Fix:** verify `peer_id_from_y7(&initiator_y7) == peer` before persisting anything. Same check in `handle_msg` and `handle_sync`.

---

## Low (UX / minor cleanups)

### L1 — Presence events suppressed when y7_id_from_peer_id returns None

**File:** `crates/y7ke-app/src/event_loop.rs:67`, `77`

If for any reason we get a `PeerId` without recoverable Ed25519 pubkey (shouldn't happen in V1 — every peer uses an inlined Ed25519 key), the presence event is silently dropped. Should at least log.

### L2 — MessageId::from_bytes called twice in handle_msg

**File:** `crates/y7ke-app/src/event_loop.rs:192` + `213`. Trivial cleanup.

### L3 — `let _ = inner.net.dial(...)` in send_contact_request

See H4. Logged separately for clarity.

---

## Architecture weaknesses

### A1 — Event loop's Arc<AppInner> keeps NetHandle alive after AppHandle drops

**Files:** `crates/y7ke-app/src/app.rs:81`, `crates/y7ke-app/src/event_loop.rs:23`

The background event loop clones `Arc<AppInner>`. When the Tauri shell drops `AppHandle`, only one Arc clone drops; the event loop's Arc keeps the swarm task alive (and the swarm task keeps cmd_rx alive, keeps event_tx alive...). Clean shutdown requires explicit `AppHandle::shutdown` before drop.

**Impact:** Tests that drop AppHandle without calling shutdown leak the runtime task. Not a correctness bug in practice (tokio cancels at process exit) but cosmetically dirty and risks file-locked sqlite handles if cargo test reruns same dir.

**Fix:** make AppHandle's Drop impl spawn a shutdown signal; or document that callers must `shutdown().await` first.

### A2 — Full 3-round sync protocol implemented but never initiated

**Files:** `crates/y7ke-net/src/protocol.rs` (SyncReq::Header/Pull/Ack), `crates/y7ke-app/src/event_loop.rs:222` (handle_sync).

The responder side handles Header / Pull / Ack. The initiator side **never sends them.** V1 uses queue-based retry only. This is dead code that adds API surface area without value.

**Fix (defer to V2):** either remove sync.rs from y7ke-net and the SyncReq/SyncResp wire types from protocol.rs, or implement initiator-side reconcile (the original V1 design). For V1 ship: leave as-is and note as "scaffolded for V2" in DECISIONS.

---

## Crypto concerns

### CR1 — Session keys never rotate

Once derived via HKDF, a `session_key` is reused for every message between two contacts forever. Long-lived keys weaken forward secrecy: if the DEK is compromised tomorrow, every historical message ever exchanged with that contact is decryptable.

**Fix (V2):** ratchet the session key per message (Double Ratchet or simpler counter-based KDF chain).

### CR2 — DEK file has no integrity/authentication

The 32-byte `master.dek` is written plaintext at `<app_data>/y7ke/master.dek` (mode 0600). An attacker with disk access can:
- Read it → decrypt every encrypted column.
- Swap it → next launch creates a "valid" identity in a key controlled by the attacker.

**Fix (V2):** promote to OS keyring (`keyring` crate) with the file as fallback. The keyring is HMAC-authenticated by the OS at the very least.

### CR3 — No anti-replay on `HandshakeReq`

A captured handshake request can be replayed. Each replay creates a new session (H1) and a new request row (H3).

**Fix:** include a 16-byte nonce in HandshakeReq, sign it, and reject duplicates server-side (LRU cache of recent nonces).

---

## Performance bottlenecks

### P1 — `list_messages` decrypts every row synchronously per call

**File:** `crates/y7ke-app/src/app.rs:233–280`

For each row in the conversation: fetch the session (DB hit), build an envelope, verify the Ed25519 sig, decrypt with ChaCha20-Poly1305. At 10,000 messages this is several seconds.

**Fix (V2):** cache `session_key` per peer in memory (`HashMap<Y7Id, SymmetricKey>`); cache decrypted text per `MessageId` in an LRU; pre-decrypt incoming messages so the UI doesn't pay the cost on view.

### P2 — `AppHandle::boot` blocks Tauri startup

**File:** `src-tauri/src/main.rs:36`

```rust
let y7_handle: AppHandle = async_runtime::block_on(AppHandle::boot(config))?;
```

Tauri's window can't open until boot finishes. Boot includes Db::open + migrations + identity ensure + swarm spawn. On a fresh install this is ~500 ms; on a populated DB possibly more. The 2-second cold-start budget is at risk on slow disks.

**Fix:** spawn boot in `setup` (don't block), show a splashscreen / loading state, register the AppHandle once ready. Commands return "not ready" until then.

### P3 — sqlx pool max 8 — fine for single user

Verified: V1 is single-user. No fix needed.

---

## Security issues

### S1 — Tauri CSP set to null

**File:** `src-tauri/tauri.conf.json:21`

```json
"security": { "csp": null }
```

No Content Security Policy. If any UI dependency ever loads remote JS (it doesn't today but could), XSS is unchecked.

**Fix:** set a strict CSP allowing only `'self'` for scripts, styles, and connect. Tauri commands use IPC (`tauri://`), not HTTP, so this won't break invoke.

### S2 — No rate limiting on inbound `HandshakeReq` / `MsgReq` / `SyncReq`

Any peer on the LAN can spam requests as fast as libp2p will deliver them.

**Fix (V2):** per-peer leaky-bucket rate limiter in the swarm task.

### S3 — UI runs without `--frozen-lockfile` enforcement in `beforeBuildCommand`

`pnpm --dir ui build` doesn't enforce the lockfile is unchanged. A poisoned `package.json` could pull a malicious dep. Low risk for V1.

---

## Duplicated code

### D1 — Hex encoding written twice

`crates/y7ke-core/src/id.rs:159–168` (hex_encode in id.rs) duplicates trivial logic. Use the `hex` crate (zero-dep clean) or move to a shared `util` module.

### D2 — sig-bytes construction repeated

`crates/y7ke-app/src/handshake.rs` (open_initiator + respond) reconstructs `[u8; 64]` via index copy in three places. Factor into `sign_eph_for(eph_pub, counterparty_pub)`.

---

## Tech debt

### T1 — `KeysDao` schema exists but no DAO methods

`migrations/0001_init.sql` has the `keys` table; no Rust DAO uses it. Either populate (V2 ratchet keys) or drop the table from the schema.

### T2 — Unused `parse_conversation_id` helper was removed but the hex round-trip test was deleted with it

Lost coverage for hex parsing — acceptable since the function is gone. Note for future: if hex parsing is reintroduced (e.g. for a `list_messages_by_conv` command) re-add the test.

---

## Verified NOT bugs

- **Envelope replay across conversations:** prevented by per-pair session-key uniqueness. Decrypt fails.
- **Handshake replay to a different recipient:** prevented by `responder_pubkey` in the signed bytes.
- **Message dedup:** PK constraint + `INSERT OR IGNORE` on `messages`.
- **ConversationId collisions:** 128-bit truncated blake3, ~2^64 birthday cost.
- **Nonce reuse in ChaCha20-Poly1305:** random 12 bytes per call, 2^96 space.
- **Y7Id ↔ PeerId round-trip:** symmetric, tested in `y7ke-net::swarm::tests`.

---

## Summary

| Severity | Count |
|---|---|
| Critical | 1 |
| High | 4 |
| Medium | 4 |
| Low | 3 |
| Architecture | 2 |
| Crypto | 3 |
| Performance | 3 |
| Security | 3 |
| Duplication | 2 |
| Tech debt | 2 |

**V1 must fix before V2:** C1, H1, H2, H3, H4, M1, M2, M4.
**V1 release polish (defer if time-boxed):** L1, L2, A1, S1.
**V2 hardening backlog:** CR1, CR2, CR3, P1, P2, S2, A2, D1, D2, T1.
