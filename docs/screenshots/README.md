# Y7KE V1 screenshots

Captured 2026-05-27 with a single instance of the Tauri shell running
against the Vite dev server (`pnpm --dir ui dev` + `target/debug/y7ke-tauri`)
on a fresh data directory. Window framed at 960×720 (the default size
configured in `src-tauri/tauri.conf.json`).

| File | View | Notes |
|---|---|---|
| `01-main-shell-empty.png` | Main shell, empty contacts | First-launch state after identity was auto-generated. Sidebar shows the truncated Y7 ID; chat area is empty with "Pick a contact, or add one to start" and the full Y7 ID with a Copy button. |
| `03-add-contact.png` | Add contact form | Reached by clicking "+ Add contact". Identity textarea (with `y7:…` placeholder), optional greeting, Send request (disabled while identity is empty), Cancel. |
| `04-requests-empty.png` | Requests view | Reached by clicking "Requests" in the sidebar. Incoming and Outgoing sections each with a 0 counter; Refresh button top right. |

## Reproducing

```bash
# Terminal 1 — dev server
pnpm --dir ui dev

# Terminal 2 — Tauri shell (connects to the running dev server)
cargo run -p y7ke-tauri
```

Two instances on the same LAN will discover each other via mDNS within ~3s and
exchange `/y7ke/handshake/1.0.0` automatically when one user pastes the
other's Y7 ID into "Add contact". The capture above is single-instance only;
a two-window chat capture is left for V2 polish.

## Verified end-to-end

These screenshots prove the Tauri ↔ Rust ↔ Svelte IPC contract is wired
correctly:

- `get_my_id` returns a real `y7:<base58>` value (shown in sidebar and main pane)
- `list_contacts` returns `[]` and the empty state renders
- `list_pending_requests` returns `[]` and the empty state renders
- Routing between Main / AddContact / Requests works via the in-app store
- `Copy` button and form controls render correctly
