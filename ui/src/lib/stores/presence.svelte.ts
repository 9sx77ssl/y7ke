// Presence store — keeps an in-memory `y7_id -> ConnectionKind` map, fed by
// presence_changed events and the initial `list_contacts` response (each
// contact carries its `presence` snapshot at load time).
//
// Views read presence via `getPresence(y7Id)`; the contacts view derives its
// row indicators from this store rather than the snapshot inside ContactView,
// so a presence_changed event refreshes the indicator without re-fetching all
// contacts.

import type { ConnectionKind } from "../types";

const presence = $state<{ map: Record<string, ConnectionKind> }>({ map: {} });

export function getPresence(y7Id: string): ConnectionKind {
  return presence.map[y7Id] ?? "offline";
}

/** Snapshot from `list_contacts`. Overwrites any cached value. */
export function seedPresence(entries: Array<{ y7_id: string; presence: ConnectionKind }>): void {
  for (const e of entries) {
    presence.map[e.y7_id] = e.presence;
  }
}

/** Called by the events dispatcher when `presence_changed` arrives. */
export function applyPresence(y7Id: string, connection: ConnectionKind): void {
  presence.map[y7Id] = connection;
}

export function presenceLabel(kind: ConnectionKind): string {
  switch (kind) {
    case "offline":
      return "Offline";
    case "connecting":
      return "Connecting";
    case "lan":
      return "LAN";
  }
}
