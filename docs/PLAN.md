# Y7KE — Authoritative Project Plan (PLAN.md)

> **Single source of truth.** This document supersedes the previous
> `docs/ARCHITECTURE.md`, `docs/ROADMAP.md`, `docs/LIVE_SMOKE.md`, and the
> deleted `docs/V2_GLOBAL_NETWORKING_PLAN.md`. A future agent updates the
> dated sections below per release; the source code remains the final
> arbiter of behavior.

**One-liner.** Privacy-first peer-to-peer desktop messenger: end-to-end
encrypted text over libp2p, local-first SQLite, no accounts, no servers
that can read your messages.

**Status banner (as of 3.0.25, 2026-05-30).** LAN + offline-sync +
relay-fallback are PROVEN on real sockets (loopback / mDNS on one host).
Cross-NAT **direct QUIC hole-punch is NOT field-confirmed** — proven only
on loopback (and there over a TCP relay) and in a netns sim that by design
falls back to relay (issue #91). Track B crypto uplift (Double Ratchet,
OS keyring, replay nonce) is **not started**. **Connection provenance**
(`origin`/`ip_version`) + structured `cat=` lifecycle logging are PROVEN
(code + ts-rs + tests). The provenance is now surfaced **end-to-end in the
UI**: the chat header renders the full path ("DIRECT · QUIC · IPv6 · via
DCUtR"), `ContactView` + `PresenceChanged` carry `ip_version`/`origin`, and
the diagnostics export carries a bounded **connection-lineage ring**
(None→Relayed→Direct, downgrades, →Offline with timestamps). **IPv6**: the
client now binds `/ip6/::` best-effort (TCP+QUIC) and `::1` direct dial is
PROVEN by test; all dial/discovery/relay paths are IP-family-agnostic; but
real cross-host v6 is UNVERIFIED — gated on the bootstrap publishing an AAAA
+ opening its v6 firewall (ops, not code). Bootstraps are sourced ONLY from
the in-app Settings page + one hardcoded default (env var + config-file
sources removed).

---

## 1. Evidence-tier legend

- **PROVEN** — demonstrated by a passing test on real sockets, OR a
  committed captured real-world log.
- **SIMULATED** — works in loopback / netns NAT-sim only (NOT two real
  machines on two ISPs).
- **UNVERIFIED** — code path exists but never exercised end-to-end.
- **PLANNED** — not implemented; aspirational.

> Do not promote a claim to PROVEN because the code looks right. Cross-NAT
> QUIC hole-punch in particular requires a captured two-machine log.

---

## 2. Architecture overview

Five Rust units in a one-way dependency DAG, plus the shell and UI:

```
y7ke-core    leaf: Y7Id, MessageId (UUIDv7), ConversationId (blake3),
             Ed25519/X25519/ChaCha20-Poly1305/HKDF/blake3, AppEvent,
             status + diagnostics enums
   ↓
y7ke-storage sqlx-sqlite + master DEK file + DAOs + column encryption
   ↓
y7ke-net     libp2p 0.56 swarm + 3 request_response protocols + Kad
             + AutoNAT v2 + relay-client + DCUtR
   ↓
y7ke-app     composition root: owns Db + NetHandle, runs event_loop,
             command surface, rate limiter
   ↓
src-tauri    Tauri 2 shell, command surface, event channel
ui           Svelte 5 + Vite + TypeScript (depends only on @tauri-apps/api)
```

Generated TS types live in `ui/src/lib/gen` (ts-rs; do not hand-edit).
The **bootstrap daemon is a separate repo** (`9sx77ssl/y7ke-bootstrap`),
intentionally with zero `y7ke-*` deps so it can never decrypt traffic.

---

## 3. Chronological milestone history

| Milestone | Version anchor | What landed |
|-----------|----------------|-------------|
| **M0 — shell** | `1197d68`→`34a77a9` (2026-05-27) | Tauri 2 + Svelte 5/Vite + Rust workspace + CI |
| Core/crypto | `f3d91d7` | y7ke-core types, AppEvent, crypto primitives |
| Storage | `28e0719` | master DEK + schema + DAOs |
| Net | `9c3f806` | libp2p swarm + 3 RR protocols + NetHandle |
| App + Tauri | `8184c48`,`9a57005` | composition root, E2E, Tauri wiring |
| **V1 — LAN messenger** | v0.1.18 (`d3ceb6e`) | identity, X25519+HKDF handshake, ChaCha20-Poly1305 envelopes, contact lifecycle, offline sync, rate limiter, ts-rs, mDNS discovery. Security pivot at v0.1.5 (`952322b`): static-DH per-conversation keys, no stored session keys. |
| **V2-A1+A2** | v0.1.20 (`93afeee`) | Kademlia DHT lookup + standalone bootstrap node |
| **V2-A4** | v0.1.43 (`8cc3bae`) | Circuit Relay v2 client + reservation + 15s redial; Settings UI + dial modes (migrations 0004/0005) |
| **V2-A3** | v0.1.64 (`768bade`) | AutoNAT v2 client + NatReachability verdict |
| **V2-A5** | `6656061`,`318c56f`,`01ae81f` | DCUtR behaviour; ConnectionUpgraded event; relay→direct upgrade loop |
| **V2-A6** | `03f6d40` | QUIC `/quic-v1` dual transport |
| V2 hardening | v0.1.65→v0.1.101 | DCUtR counters + Connectivity pane, idempotent dials, fail-closed block enforcement, netns NAT-sim harness, live-smoke runbook, copy-diagnostics export, two-mode dial (migration 0006), transport-agnostic bootstrap shorthand |
| **3.0.0 — global networking** | `4f33a7a` (2026-05-29) | rolls up A1–A6 |
| **3.0.x hardening** | 3.0.1→3.0.16 | frameless first-paint fix (3.0.1/02/08), donate page (3.0.3-05), contact-request fix (3.0.9), HMR teardown (3.0.10), auto-reconnect (3.0.11), dev full-reload (3.0.12), **reliable message delivery (3.0.13)**, **durable ChatDeleted + migration 0007 (3.0.14)**, **boot-$effect listener-flap fix (3.0.15)**, **presence re-establish on delete+re-add + dial-mode hydration (3.0.16)** |
| **3.0.x observability** | 3.0.17→3.0.25 | structured `cat=` lifecycle logging (3.0.20), connection provenance `origin`/`ip_version` end-to-end (3.0.21), best-effort IPv6 listeners + `/dns` shorthand + `::1` proof (3.0.22), bootstraps from Settings + one hardcoded node only (3.0.23), **richer chat-header label (DIRECT·QUIC·IPv6·via DCUtR) + ContactView provenance (Phase 4)**, **connection-lineage ring in diagnostics export (Phase 3)** |

---

## 4. Wire protocols, transport stack, data model

**Wire protocols** (CBOR `request_response`, byte-flat types in
`crates/y7ke-net/src/protocol.rs`):
- `/y7ke/handshake/1.0.0` — HandshakeReq/Resp (no replay nonce yet)
- `/y7ke/msg/1.0.0` — MsgReq/Resp carrying `MessageEnvelope`; control
  payloads ride inside via a 1-byte tag (`0x00` text, `0x01` control:
  RejectedRequest / AcceptedRequest / ChatDeleted)
- `/y7ke/sync/1.0.0` — 3 logical rounds Header→Pull→Ack
- plus `/y7ke/kad/1.0.0` and identify `/y7ke/0.1.0`

**Transport stack:** TCP + Noise(XX) + Yamux **AND** QUIC `/quic-v1`,
plus a `/p2p-circuit` relay-client transport. Bootstraps are dialed on
both TCP+QUIC in one `DialOpts` (QUIC + TCP race; QUIC wins on UDP-open
nets). Bootstrap descriptors are transport-agnostic shorthand
(`/dns4/host/<port>/p2p/<id>`), fanned to both `/tcp` and
`/udp/<port>/quic-v1` by `expand_bootstrap`.

**Connection enums:** `ConnectionKind {Lan, Internet, Relayed, Direct}`
(`Internet` = direct dial outright; `Direct` = relay→DCUtR upgrade; both
non-relay). `Transport {Tcp, Quic}` (no IP-version dimension).
`NatReachability {Public, Private, Unknown}`.

**MessageStatus** serializes as `i64` (serde_repr). Only `Delivered(2)`
and `Synced(3)` are written in production; `Sending(0)` is the in-flight
state; `Sent(1)` and `Failed(4)` are dead. UI renders 2 and 3 identically.

**Data model:** 10 effective tables (`users, contacts, requests, messages,
sessions, keys, sync_queue, peer_state, settings, pending_deletes`),
7 migrations `0001`→`0007`. `messages.payload_enc` is byte-identical to
the `/y7ke/msg` wire ciphertext. `PRAGMA secure_delete = ON`.

---

## 5. Security model

- **Identity:** Ed25519 keypair, encoded `y7:<base58(ed25519_pub)>`.
- **Per-conversation keys (never stored):** derived on demand —
  `HKDF-SHA256(salt = conv_id, ikm = X25519(my_static_scalar,
  peer_x25519_pub), info = "y7ke-conv-v1", L = 32)`. Both X25519 keys come
  from the long-term Ed25519 identity (SHA-512(seed)[..32] + RFC7748 clamp
  for the scalar; Edwards→Montgomery for the pubkey) — the DH is symmetric.
- **Messages:** ChaCha20-Poly1305 AEAD (random 12-byte nonce, sender
  pubkey as AAD) + Ed25519 signature over `message_id || ts_le ||
  ciphertext`.
- **Handshake:** X25519 ephemeral signed by the long-term Ed25519 key.
- **At rest:** Ed25519 private key + DB columns encrypted with a 32-byte
  master DEK at `<app_data>/y7ke/master.dek` (mode `0600` on Unix);
  `secure_delete=ON` zero-fills freed pages.
- **Relay never sees plaintext** — Noise + ChaCha20-Poly1305 wrap every
  byte before it leaves the client (property by construction).

**NOT yet implemented (Track B, aspirational):**
- B1 — OS keyring for the master DEK (currently file-only, no Argon2id
  passphrase fallback).
- B2 — Double Ratchet forward secrecy (current static conv key →
  current-key compromise decrypts all history for that conversation).
- B3 — handshake replay nonce (HandshakeReq has no random nonce; no
  server-side LRU dedup).

---

## 6. Capability matrix

| Capability | Status | Evidence / note |
|------------|--------|-----------------|
| V1 LAN messaging (add→accept→send→restart) | **PROVEN** | `v1_e2e.rs`, `v1_restart_both.rs` (loopback/mDNS) |
| Offline sync (queue drain + 3-round reconcile) | **PROVEN** | `v1_offline_sync.rs`, `v2_sync_reconcile.rs` |
| Chat-delete propagation (online) | **PROVEN** | `v1_delete_propagation.rs` |
| Durable chat-delete stash survives wipe | **PROVEN** | storage `pending_delete_survives_wipe_peer` |
| Delete-while-offline → flush on reconnect | **UNVERIFIED** | parts proven; combined networked path untested |
| Byte-level privacy (disk == wire ciphertext) | **PROVEN** | `v1_privacy.rs` |
| Block enforcement (fail-closed) | **PROVEN** | `v2_block_enforcement.rs` |
| Relay-v2 reservation + relayed round-trip | **SIMULATED** | `four_node_relay.rs` (loopback) |
| QUIC listener bind | **PROVEN** | `quic_listen_smoke.rs` (loopback) |
| QUIC as live peer data transport | **SIMULATED** | only `quic_migrate_node` netns, no committed log |
| Transport preference sort (QUIC>TCP>relay) | **PROVEN** | `dial_priority.rs` unit tests |
| Transport surfacing (label DIRECT·QUIC·IPv6·via DCUtR) | **PROVEN (code)** / **UNVERIFIED (test)** | wired end-to-end (chat header + Connectivity pane); `ContactView`+`PresenceChanged` carry `ip_version`/`origin`; no automated render assertion |
| Connection-lineage ring (diagnostics export) | **PROVEN** | bounded `transition_ring` recorded at the 3 connection arms; surfaced as `DiagnosticsDetail.recent_transitions` + ts-rs; rendered in copy-diagnostics |
| DCUtR relay→direct upgrade *event* | **SIMULATED** | `v2_dcutr_smoke.rs` (loopback, TCP relay) |
| DCUtR upgrade logged (success + failure) | **PROVEN** | `swarm.rs` netlog `cat=DCUTR`, `event_loop.rs` relay→direct `elapsed_ms` |
| Connection provenance (`origin`/`ip_version`) | **PROVEN** | `ConnectionOrigin {DirectDial,DcutrUpgrade,RelayOnly,PublicIpv6,PublicIpv4,Unknown}` + ts-rs; surfaced in logs+diagnostics+pane; unit tests |
| Structured `cat=` lifecycle logging | **PROVEN** | netlog! macro (CONNECTION/DCUTR/RELAY/AUTONAT/…) over existing tracing |
| AutoNAT verdict plumbing | **SIMULATED** | `autonat_smoke.rs` (event only, not content) |
| `ConnectionKind::Internet` (outright direct) | **UNVERIFIED** | classifier+precedence only; never produced |
| QUIC connection migration (RFC 9000) | **UNVERIFIED / non-functional** | drop+re-dial confirmed (commit `e433c92`) |
| **Cross-NAT QUIC hole-punch (two ISPs)** | **SIMULATED → field PLANNED** | no captured log; #91 open |
| IPv6 client listen + `::1` direct dial | **PROVEN (loopback)** | best-effort `/ip6/::` TCP+QUIC (`swarm.rs`), `ipv6_loopback.rs` connects over `::1` |
| IPv6 full-p2p cross-host | **UNVERIFIED (code-complete, ops-gated)** | all paths family-agnostic (verified); needs bootstrap AAAA + v6 firewall + a 2-machine capture |
| Track B (keyring / ratchet / replay nonce) | **PLANNED** | not started |
| D2 Playwright E2E | **PLANNED** | pending type stabilization |

---

## 7. Known limitations

- **No field-confirmed cross-NAT QUIC hole-punch.** Proven only on
  loopback (over a TCP relay) and in a netns sim that by design falls
  back to relay. No two-machine / two-ISP log is committed. (#91)
- **Symmetric NAT cannot be punched** — stock Linux MASQUERADE behaves as
  symmetric NAT; the netns harness asserts clean relay fallback, not a
  punch. Full-cone (the common home-router case) has no automated coverage.
- **QUIC does not migrate in place** — an IP change drops the connection
  and re-dials a fresh ConnectionId; resilience comes from the offline
  queue, not RFC 9000 migration.
- **IPv6 inert end-to-end** — the client binds `/ip6/::` (best-effort) and
  `::1` direct dial is test-proven; all paths are family-agnostic. But no peer
  learns a v6 address until the bootstrap publishes an AAAA + opens its v6
  firewall + advertises a v6 external-addr (ops, not code). Until then v6 is a
  dormant capability; real cross-host v6 P2P is UNVERIFIED.
- **No forward secrecy** — static per-conversation key (Track B pending).
- **Blocked status has no management UI** — reachable by rejecting,
  enforced fail-closed, but no view/undo.

---

## 8. Roadmap

**Done:** V1 (LAN E2E), V2-A A1–A6, V2-C C1–C4 (reconcile, read-receipts,
rate-limit, non-blocking boot), D1 (ts-rs), Settings UI + two-mode dial,
all 3.0.x hardening.

**Done (observability track, 3.0.20→3.0.25):**
1. ~~**Structured lifecycle logging**~~ — `cat=` category field
   (DISCOVERY/TRANSPORT/DCUTR/RELAY/CONNECTION/IPVERSION/AUTONAT) + time-to-
   direct `elapsed_ms` + derived direct→relay downgrade log. *(netlog! over
   existing tracing; no new subscriber)*
2. ~~**Richer connection labels**~~ — `ip_version` + `origin`
   (`ConnectionOrigin`) captured end-to-end; chat header renders
   "DIRECT · QUIC · IPv6 · via DCUtR"; Connectivity pane shows per-conn
   detail; diagnostics export carries the connection-lineage ring.
3. ~~**IPv6 enablement**~~ — client `/ip6/::` best-effort listeners
   (TCP+QUIC); `/dns` default descriptor; loopback `::1` proof test. Bootstrap
   AAAA + v6 firewall remains ops-gated (separate repo + DNS).

**Remaining (proposed order):**
4. **Live cross-NAT smoke (#91)** — run the LIVE_SMOKE procedure on two
   real machines / two ISPs; commit a redacted captured log. This is the
   ONE artifact that flips cross-NAT direct QUIC from SIMULATED to PROVEN.
5. **Track B — crypto uplift:** B1 OS keyring, B2 Double Ratchet, B3
   handshake replay nonce.
6. **Track D2 — Playwright E2E** (once wire types stabilize).
7. **Block/unblock management UI.**
8. **V3 (not started):** groups, file transfer, onion/anonymous routing,
   mobile.

---

## 9. Live cross-NAT smoke procedure (was LIVE_SMOKE.md)

Two real machines on two different ISPs/NATs. On each:

```
RUST_LOG=warn,y7ke=info,y7ke_net=info,libp2p_dcutr=debug \
  Y7KE_DATA_DIR=~/y7ke-live ./y7ke 2>&1 | tee ~/y7ke-live.log
```

Add each other, exchange messages, then grep each log for the healthy
progression:
- `relay: reservation accepted`
- `connection established ... kind=Relayed`
- `dcutr: direct upgrade succeeded`
- `presence upgraded via DCUtR ... kind=Direct` (+ `transport=Quic`)
- each side's `autonat: verdict` (one Public / one Private is the
  interesting cross-NAT case)

**Then commit both redacted logs** under `docs/captures/` and update the
capability matrix row to PROVEN. Until that artifact exists, cross-NAT
direct QUIC stays SIMULATED.

---

## 10. PROVEN vs SIMULATED vs UNVERIFIED vs PLANNED — summary

- **PROVEN (real sockets, one host):** V1 messaging, offline sync,
  online chat-delete + durable-stash invariant, byte-level privacy, block
  enforcement, QUIC bind, transport-preference sort, DCUtR logging,
  connection provenance (`origin`/`ip_version` + ts-rs + unit tests),
  connection-lineage ring in the diagnostics export, structured `cat=`
  lifecycle logging, IPv6 client listen + `::1` direct dial.
- **SIMULATED (loopback / netns only):** relay round-trip, DCUtR
  relay→direct upgrade (loopback, TCP relay), QUIC as data transport,
  AutoNAT plumbing, the full relay→direct chain, cross-NAT QUIC hole-punch.
- **UNVERIFIED:** `Internet` outright-direct kind, `extract_transport`
  end-to-end, QUIC migration, IPv6 full-p2p cross-host (code-complete +
  family-agnostic, but ops-gated on AAAA + v6 firewall), delete-while-offline
  flush, bootstrap-side QUIC acceptance.
- **PLANNED:** Track B (keyring / Double Ratchet / replay nonce), D2
  Playwright, block-management UI, V3.

> **Maintenance note for future agents:** update the status banner (§ top),
> the capability matrix (§6), the milestone table (§3), and the
> PROVEN/SIMULATED summary (§10) on every release. When #91's captured log
> lands, flip the cross-NAT QUIC row and §9's closing sentence.
