# Y7KE --- Technical Specification

## Project Goal

Y7KE is a cross-platform desktop application focused on:

-   privacy-first communication
-   peer-to-peer networking
-   local-first architecture
-   end-to-end encryption
-   minimal and fast UX
-   no accounts, emails, passwords, or phone numbers

The application must feel lightweight and highly responsive.

Identity is key-based.

No central message server should exist for message delivery.

------------------------------------------------------------------------

# Core principles

1.  Local-first
2.  Privacy-first
3.  Minimal UX
4.  Fast startup
5.  Cross-platform
6.  Strong typing
7.  Secure by default
8.  Offline capable

------------------------------------------------------------------------

# Stack

Frontend:

-   Svelte
-   TypeScript
-   Vite

Desktop:

-   Tauri v2

Backend:

-   Rust

Networking:

-   libp2p
-   Tokio

Storage:

-   SQLite
-   sqlx

Crypto:

Identity: - Ed25519

Key exchange: - X25519

Encryption: - ChaCha20-Poly1305

Serialization:

-   serde
-   bincode

Logging:

-   tracing

Testing:

-   cargo test
-   integration tests
-   E2E tests

------------------------------------------------------------------------

# Authentication

NO:

-   emails
-   usernames
-   passwords
-   phone numbers

On first launch:

Generate:

-   private key
-   public key
-   local encrypted storage

User ID:

y7:`<public_key>`{=html}

Private key never leaves device.

------------------------------------------------------------------------

# Contacts

Users may:

-   copy ID
-   paste ID
-   send request
-   accept request
-   reject request
-   remove contact

------------------------------------------------------------------------

# Presence / status

Statuses:

-   Online
-   Offline
-   Connecting
-   Relay
-   Direct P2P
-   Last seen

Connection indicator required.

------------------------------------------------------------------------

# Messaging

Version 1:

Only:

-   text messages

No:

-   media
-   stickers
-   reactions
-   avatars

Message states:

-   Sending
-   Sent
-   Delivered
-   Synced
-   Failed

------------------------------------------------------------------------

# Offline synchronization

Messages must never disappear.

Messages stored:

SQLite locally

Flow:

User A offline

User B sends:

message1 message2 message3

Messages stored locally in encrypted queue.

When users reconnect:

-   discover peer
-   compare message IDs
-   exchange missing messages
-   confirm synchronization

Must support:

-   duplicate prevention
-   ordering
-   retries

------------------------------------------------------------------------

# Database requirements

Tables:

users contacts messages requests sessions keys sync_queue peer_state

Messages:

message_id sender receiver timestamp encrypted_payload status

Indexes required.

------------------------------------------------------------------------

# Networking

Use libp2p.

Required:

-   peer discovery
-   DHT
-   relay support
-   NAT traversal
-   encrypted channels

Do NOT implement networking from scratch.

------------------------------------------------------------------------

# Performance requirements

Cold startup:

\<2 sec target

Message send:

near instant

Memory:

minimal

Avoid:

-   unnecessary rerenders
-   polling
-   blocking UI

------------------------------------------------------------------------

# Git workflow

Claude must initialize local git.

Rules:

Commit frequently.

Examples:

feat: add peer discovery

fix: repair sync bug

refactor: split crypto layer

------------------------------------------------------------------------

# Agent system

Use up to 6 agents.

Agent 1:

Core architecture

Agent 2:

Networking

Agent 3:

Crypto

Agent 4:

Database

Agent 5:

Frontend

Agent 6:

Testing and QA

Agents work in parallel.

------------------------------------------------------------------------

# Autonomous workflow

Claude must:

-   create TODO.md
-   create ARCHITECTURE.md
-   create DECISIONS.md
-   create ROADMAP.md

Loop:

plan implement test fix commit

Repeat continuously.

------------------------------------------------------------------------

# Testing rules

Must test locally continuously.

Required:

Unit tests

Integration tests

Network tests

Sync tests

Offline tests

Reconnect tests

Stress tests

Multiple clients simultaneously

------------------------------------------------------------------------

# Initial milestone

V1:

-   identity generation
-   contacts
-   requests
-   P2P chat
-   encrypted storage
-   sync
-   status system

V2:

-   groups
-   relay improvements
-   better synchronization

V3:

-   file transfer
-   optional anonymous routing

------------------------------------------------------------------------

# Important restrictions

Do NOT:

-   create fake implementations
-   hardcode values
-   skip testing
-   write cryptography manually
-   ignore error handling
-   prioritize visuals over architecture

Build a real product.
