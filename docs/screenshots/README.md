# Y7KE V1 screenshots

Captured 2026-05-27 from the **release** build (`cargo tauri build` →
`target/release/y7ke-tauri`) with a fresh data directory. Window
960×720, dark monochrome aesthetic.

| File | View | Notes |
|---|---|---|
| `01-empty-shell.png` | Main shell, empty contacts | First launch after identity auto-generation. Custom titlebar with green online dot + min/max/close. Sidebar shows nav only — the truncated Y7 ID was removed per V1 polish. Empty chat area has the "pick a contact, or add one to start" prompt + a centered CTA. |
| `02-add-contact.png` | Add contact + own identity | Reached via the sidebar "+ add contact" or the empty-state CTA. "new request" card with paste field + greeting + Send/Cancel. **Below the card**, the user's full y7 URI lives in a `KeyDisplay` block with "your identity" label and a help line — single canonical place to copy your own key, click anywhere on the field to copy with a toast confirmation. |
| `03-requests.png` | Requests view | Incoming and Outgoing sections with live counts. Empty states for both. Refresh button top right. Inbound rows get Accept + Reject buttons; outbound rows get Cancel. |

## Reproducing

```bash
# One-time setup
pnpm --dir ui install

# Run the production binary (after `cargo tauri build`):
target/release/y7ke-tauri

# Or run the dev shell + auto Vite via Tauri's beforeDevCommand:
cargo tauri dev
```

Captured via `import -window $(xdotool search --name '^Y7KE$')`.

## Verified end-to-end

These screenshots are from the actual release binary, so they prove:

- The Tauri 2 IPC contract works in `--release` (not just `--debug`).
- The custom frameless window decoration renders identically on a
  real desktop session.
- JetBrains Mono is bundled into the embedded `dist/` and loads from
  the offline AppData asset bundle, no external fetch.
- Strict CSP (`default-src 'self'`) does not block fonts, IPC, or the
  Svelte runtime.
- The redesigned views — Sidebar, AddContact (with bottom KeyDisplay),
  Requests, and EmptyChat — all use the shared design tokens + Button
  / Card / Input / KeyDisplay primitives. No raw `<button>` / `<input>`.
- `get_my_id` returns a real `y7:<base58>` (visible in
  `02-add-contact.png`'s your-identity panel).
