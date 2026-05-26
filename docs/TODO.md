# Y7KE TODO

Live task list. Drop items that are done, add things as they surface.

## M0 — Scaffold (in progress)

- [x] Initialize local git repo inside `/home/rsz/Desktop/Y7KE/` (isolated from parent repo)
- [x] Root `Cargo.toml` workspace with 4 members + shared `workspace.dependencies`
- [x] `rust-toolchain.toml`, `.gitignore`, `README.md`
- [x] Empty `y7ke-core`, `y7ke-storage`, `y7ke-net`, `y7ke-app` crate skeletons (compile-green)
- [x] `docs/ARCHITECTURE.md`, `docs/DECISIONS.md`, `docs/ROADMAP.md`, `docs/TODO.md`
- [ ] Vite + Svelte + TypeScript scaffold under `ui/`
- [ ] `src-tauri/` shell wiring with `frontendDist = ../ui/dist` and Tokio runtime
- [ ] `cargo tauri dev` opens an empty Y7KE window on Linux
- [ ] CI workflow (Linux primary, macOS/Windows continue-on-error)

## M1 — V1 capabilities

- [ ] `y7ke-core::crypto` — Ed25519 / X25519 / ChaCha20-Poly1305 / HKDF wrappers with `zeroize`
- [ ] `y7ke-core::id` — `Y7Id`, `MessageId` (UUIDv7), `ConversationId` (blake3-derived)
- [ ] `y7ke-core::error`, `event`, `status` — `AppError`, `AppEvent`, `MessageStatus`, `ConnectionKind`
- [ ] `y7ke-storage::dek` — DEK file loader + `directories` integration
- [ ] `y7ke-storage::field_crypto` — generic seal/open helpers
- [ ] `y7ke-storage` migrations + DAOs for 8 tables
- [ ] `y7ke-net::behaviour` — single `#[derive(NetworkBehaviour)]` aggregating mDNS + ping + identify + 3 request_response codecs
- [ ] `y7ke-net::swarm` + `handle` — owning async task, `NetCommand`/`NetEvent` mpsc/broadcast facade
- [ ] `y7ke-net::handshake` — X25519 session establishment
- [ ] `y7ke-net::sync` — header/pull/ack state machine + retry driver
- [ ] `y7ke-app::messaging` — send/receive orchestration
- [ ] `y7ke-app::commands` — `get_my_id`, `add_contact_request`, `accept_request`, `reject_request`, `list_contacts`, `list_requests`, `send_message`, `list_messages`
- [ ] Tauri command wrappers + event emit
- [ ] Svelte views: IdentitySetup, Contacts, AddContact, Requests, Chat
- [ ] V1 integration test exercising all 7 capabilities with 2 in-process clients

## M2 — V1 release polish

- [ ] Stress test scaffold (6 clients, simulated network drops via custom Transport wrapper)
- [ ] Cold-start measurement script
- [ ] `cargo tauri build` produces `.deb` + `.AppImage` on Linux
- [ ] README install instructions

## Discovered

(empty — append as found)
