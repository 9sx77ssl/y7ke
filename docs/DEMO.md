# Y7KE V1 demo

A 5-minute walkthrough proving the seven V1 capabilities work end-to-end
with two clients on a single host.

## Prerequisites

- Linux (Arch / Ubuntu / Fedora) or macOS or Windows
- Rust stable (≥ 1.80)
- Node 22 + pnpm 10
- Linux only: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`
- `cargo install tauri-cli --version "^2" --locked` (one-time)

## Build + run (dev mode, one command)

```bash
git clone git@github.com:9sx77ssl/y7ke.git
cd y7ke
cargo tauri dev --no-watch
```

The first invocation:

1. Runs `pnpm --dir ui install` (~10 s).
2. Spawns `pnpm --dir ui dev` (Vite at `127.0.0.1:1420`, ~1 s).
3. Compiles the Tauri shell + workspace (~30 s clean, < 5 s incremental).
4. Opens a 960×720 frameless window titled **Y7KE**.

To run a **second client** on the same machine, point it at a different
data directory:

```bash
XDG_DATA_HOME=/tmp/y7ke-bob cargo tauri dev --no-watch
```

(macOS: `HOME=/tmp/y7ke-bob`; Windows: `LOCALAPPDATA=...`.)

## Two-peer scenario

With two windows open side-by-side, walk through the seven capabilities:

### 1. Identity

Each window auto-generates an Ed25519 keypair on first launch. Both show
their **full** `y7:<base58>` URI at the bottom of the **Add contact**
view. Click the key to copy — a "copied" toast appears in the
lower-right.

### 2. Add contact

In window **A**:
1. Click **add contact** in the sidebar.
2. Paste window **B**'s `y7:` URI into the **Identity** field.
3. Optionally type a greeting like `hi from alice`.
4. Click **send request**.

### 3. Accept request

In window **B**:
1. The sidebar **requests** entry shows a count badge.
2. Open **requests**; window **A**'s entry appears under **Incoming**.
3. Click **accept**.
4. Window **A**'s outgoing request resolves automatically as the
   handshake completed; a contact appears in **A**'s sidebar.

### 4. Open chat

In window **A**:
1. Click window **B** in the sidebar **CONTACTS** list (status dot is
   green = online).
2. The chat pane opens with the peer's truncated y7 in the header.

### 5. Exchange encrypted messages

Type a message in either window. The receiver sees the message instantly
(under 100 ms on a local network). Each bubble's footer shows a status
badge:

- `…` sending
- `✓` sent
- `✓✓` synced (peer ack received)
- `!` failed

### 6. Persistence

Close both windows. Reopen them with the same data directories. History
loads from SQLite, the identity is unchanged, contacts and the active
session resume. Send a new message — it flows immediately.

### 7. Offline sync

In **A**, close window **B**. In window **A**, send 3 messages — they
queue with `…` status because the live push fails. Reopen window **B**
with the same data directory. Within a few seconds (mDNS rediscovery +
queue drain), all 3 queued messages arrive at **B**, statuses transition
to `✓✓` in window **A**.

## Cancel an outgoing request

Pre-acceptance, window **A** can revoke the request:

1. Open **requests** in window **A**.
2. Under **Outgoing**, click **cancel** on the pending request.
3. The local state resolves as `cancelled`. Note: window **B** keeps
   showing the pending inbound request until they explicitly reject it.
   (Notify-on-cancel is V2.)

## What to look for

| Capability | Visible signal |
|---|---|
| Identity gen | y7:… URI at bottom of Add Contact |
| Add contact | Outgoing request appears |
| Accept request | Contact moves to CONTACTS list |
| Open chat | Chat header + empty conversation |
| Encrypted msg | Bubble appears in both windows; backend log shows ciphertext |
| Persistence | History after reboot |
| Offline sync | Queued messages arrive on reconnect |

## Building release artifacts locally

```bash
cargo tauri build
# Linux outputs:
ls src-tauri/target/release/bundle/{deb,appimage}/
```

Or push a tag to trigger the GitHub Actions matrix and produce a draft
release with `.deb` + `.AppImage` + `.dmg` + `.msi`:

```bash
git tag v0.1.0
git push origin v0.1.0
```

## Verifying privacy

After running the demo with two clients:

```bash
# Plaintext message text should NOT appear in either DB.
grep --binary-files=text 'hi from alice' \
  ~/.local/share/y7ke/y7ke.db \
  /tmp/y7ke-bob/y7ke/y7ke.db
# Expected: no matches.
```

See `docs/V1_CHECKLIST.md` for the full verification list and
`docs/AUDIT.md` for the security audit findings + fixes.
